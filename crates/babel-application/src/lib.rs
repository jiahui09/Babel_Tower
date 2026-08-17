//! Application boundary for the authoritative project writer.

use std::{
    collections::{HashMap, HashSet},
    fs::{self, File, OpenOptions},
    io::{Cursor, Read},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, RecvTimeoutError, SyncSender, TryRecvError, TrySendError},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant, SystemTime},
};

use babel_adapter_host::CapabilityRegistry;
use babel_adapter_protocol::{
    Adapter, AdapterError, CancellationToken, Cursor as AdapterCursor, ExecutionContext,
    ExtractedUnit, InventoryItem, OverlayUnit, Page, TaskBudget,
};
use babel_domain::core::{ProjectId, RevisionKind, TaskId, TaskState, WorkPriority};
use babel_markdown_adapter::MarkdownAdapter;
use babel_resource_graph::{RESOURCE_GRAPH_SCHEMA_VERSION, ResourceGraph, ResourceKind};
use babel_runtime::{
    ipc::MAX_FRAME_BYTES,
    process_worker::{ProcessWorker, WorkerCancelToken, WorkerError, WorkerLaunch},
};
use babel_storage::{
    backup::{BackupError, BackupSnapshot},
    cas,
    gc::{self, GcReport},
    project::{
        AnnotationRecord, BatchReplaceReceipt, DuplicateSourceGroup, GenerationBatch,
        GenerationBindingRecord, GenerationBindingView, GenerationDescriptor, GenerationEdgeRecord,
        GenerationResourceRecord, GenerationUnitRecord, MarkerRecord, ObjectRecord, ProjectStore,
        ReplacePreviewItem, SaveReceipt, TaskRecord, TermRecord, TranslationHistoryItem,
        UpsertTermRequest, candidate_set_hash,
    },
    query::ProjectQuery,
};
use babel_tir::{Token, UnitContent};
use babel_txt_adapter::TxtAdapter;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

const WRITER_QUEUE_CAPACITY: usize = 1_024;
const GC_GRACE_PERIOD: Duration = Duration::from_secs(24 * 60 * 60);
const GC_BATCH_ITEMS: usize = 2_000;
const GC_BATCH_WALL_TIME: Duration = Duration::from_millis(50);
const MAX_INTERACTIVE_BURST: usize = 32;
const FORMAT_PIPELINE_PAGE_BYTES: u64 = 64 * 1024 * 1024;
const FORMAT_PIPELINE_PAGE_NODES: u32 = 100_000;
const FORMAT_WORKER_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const FORMAT_WORKER_CHUNK_BYTES: usize = 1024 * 1024;
const FORMAT_WRITER_BATCH_ITEMS: usize = 5_000;

const TXT_ADAPTER_ID: &str = "org.babel-tower.txt";
const MARKDOWN_ADAPTER_ID: &str = "org.babel-tower.markdown";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FormatKind {
    Txt,
    Markdown,
}

impl FormatKind {
    const fn label(self) -> &'static str {
        match self {
            Self::Txt => "TXT",
            Self::Markdown => "Markdown",
        }
    }

    const fn format_id(self) -> &'static str {
        match self {
            Self::Txt => "txt",
            Self::Markdown => "markdown",
        }
    }

    const fn media_type(self) -> &'static str {
        match self {
            Self::Txt => "text/plain",
            Self::Markdown => "text/markdown",
        }
    }

    const fn worker_env(self) -> &'static str {
        match self {
            Self::Txt => "BABEL_TXT_WORKER",
            Self::Markdown => "BABEL_MARKDOWN_WORKER",
        }
    }

    const fn worker_binary(self) -> &'static str {
        match self {
            Self::Txt => {
                if cfg!(windows) {
                    "babel-txt-worker.exe"
                } else {
                    "babel-txt-worker"
                }
            }
            Self::Markdown => {
                if cfg!(windows) {
                    "babel-markdown-worker.exe"
                } else {
                    "babel-markdown-worker"
                }
            }
        }
    }

    const fn worker_capability(self) -> &'static [u8] {
        match self {
            Self::Txt => b"babel-txt-worker-v1",
            Self::Markdown => b"babel-markdown-worker-v1",
        }
    }

    fn from_adapter_id(adapter_id: &str) -> Result<Self, KernelError> {
        match adapter_id {
            TXT_ADAPTER_ID => Ok(Self::Txt),
            MARKDOWN_ADAPTER_ID => Ok(Self::Markdown),
            other => Err(KernelError::WorkerDiagnostic(format!(
                "unsupported generation adapter id: {other}"
            ))),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CommitEvent {
    TranslationCommitted {
        revision_id: i64,
        commit_sequence: i64,
    },
    TranslationBatchCommitted {
        affected_units: usize,
        commit_sequence_end: i64,
    },
    ObjectReferenced {
        object_hash: [u8; 32],
    },
    TaskChanged {
        task_id: [u8; 16],
        state: TaskState,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PublishedObject {
    pub hash: [u8; 32],
    pub byte_length: u64,
    pub path: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FormatImportReport {
    pub generation_id: [u8; 16],
    pub source_hash: [u8; 32],
    pub byte_length: u64,
    pub units: usize,
    pub activated: bool,
    pub review_required: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FormatBindingReview {
    pub binding_id: [u8; 16],
    pub extracted_unit_id: [u8; 16],
    pub disposition: String,
    pub candidates: Vec<[u8; 16]>,
    pub candidates_hash: [u8; 32],
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FormatValidationIssue {
    pub source_unit_key: [u8; 32],
    pub code: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FormatExportReport {
    pub generation_id: [u8; 16],
    pub frozen_commit_sequence: i64,
    pub output_hash: [u8; 32],
    pub byte_length: u64,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddAnnotationRequest {
    pub annotation_id: [u8; 16],
    pub unit_id: Vec<u8>,
    pub base_revision_id: Option<i64>,
    pub grapheme_start: u64,
    pub grapheme_end: u64,
    pub body: String,
    pub created_at_ms: i64,
}

pub type TxtImportReport = FormatImportReport;
pub type TxtBindingReview = FormatBindingReview;
pub type TxtValidationIssue = FormatValidationIssue;
pub type TxtExportReport = FormatExportReport;
pub type MarkdownImportReport = FormatImportReport;
pub type MarkdownBindingReview = FormatBindingReview;
pub type MarkdownValidationIssue = FormatValidationIssue;
pub type MarkdownExportReport = FormatExportReport;

#[derive(Debug, Serialize)]
#[serde(tag = "operation", rename_all = "kebab-case")]
enum FormatWorkerRequest {
    LoadBegin {
        source_hash_hex: String,
        byte_length: u64,
    },
    LoadChunk {
        session_id: u64,
        offset: u64,
        data_hex: String,
    },
    LoadFinish {
        session_id: u64,
    },
    ProbeLoaded {
        session_id: u64,
    },
    InventoryPage {
        session_id: u64,
        generation_id: [u8; 16],
        cursor: Option<AdapterCursor>,
    },
    ExtractPage {
        session_id: u64,
        generation_id: [u8; 16],
        resource_id: [u8; 16],
        cursor: Option<AdapterCursor>,
    },
}

#[derive(Debug, Deserialize)]
struct LoadBeginReply {
    session_id: u64,
    max_chunk_bytes: usize,
}

#[derive(Debug, Deserialize)]
struct LoadChunkReply {
    received_bytes: u64,
}

#[derive(Debug, Deserialize)]
struct LoadFinishReply {
    byte_length: u64,
}

#[derive(Debug, Deserialize)]
struct WorkerProbeReply {
    detected_media_type: Option<String>,
    reason_code: String,
    adapter_id: String,
    adapter_build: String,
    identity_version: u32,
}

#[derive(Debug, Deserialize)]
struct InventoryPageReply {
    page: Page<InventoryItem>,
}

#[derive(Debug, Deserialize)]
struct ExtractPageReply {
    page: Page<ExtractedUnit>,
}

struct PreparedFormatImport {
    format: FormatKind,
    source_hash: [u8; 32],
    byte_length: u64,
    generation_id: babel_domain::core::GenerationId,
    adapter_id: String,
    adapter_build: String,
    identity_version: u32,
    nodes: Vec<babel_resource_graph::ResourceNode>,
    edges: Vec<babel_resource_graph::ResourceEdge>,
    units: Vec<ExtractedUnit>,
}

pub struct CommitSubscription {
    receiver: Receiver<CommitEvent>,
    lagged: Arc<AtomicBool>,
}

impl CommitSubscription {
    pub fn recv_timeout(&self, timeout: Duration) -> Result<CommitEvent, RecvTimeoutError> {
        self.receiver.recv_timeout(timeout)
    }

    pub fn try_recv(&self) -> Result<CommitEvent, TryRecvError> {
        self.receiver.try_recv()
    }

    pub fn take_lagged(&self) -> bool {
        self.lagged.swap(false, Ordering::AcqRel)
    }
}

#[cfg(test)]
fn import_txt_bytes(
    store: &mut ProjectStore,
    object_root: impl AsRef<Path>,
    staging_root: impl AsRef<Path>,
    bytes: &[u8],
    created_at_ms: i64,
) -> Result<TxtImportReport, KernelError> {
    let object_root = object_root.as_ref();
    let staging_root = staging_root.as_ref();
    let (source_hash, _, byte_length) = cas::publish_reader(object_root, bytes)?;
    let registry = CapabilityRegistry::new(object_root, staging_root)?;
    let source = registry.grant_object(source_hash, byte_length)?;
    let adapter = TxtAdapter::new();
    adapter.manifest().validate()?;
    let token = CancellationToken::default();
    let budget = format_budget(byte_length.max(bytes.len() as u64));
    let execution = ExecutionContext::new(&budget, &token);
    let probe = adapter.probe(&source, &registry, &execution)?;
    if probe.detected_media_type.as_deref() != Some("text/plain") {
        return Err(KernelError::Adapter(AdapterError::InvalidInput(
            probe.reason_code,
        )));
    }

    let generation_id = babel_domain::core::GenerationId::new();
    let (nodes, edges) = collect_txt_inventory(&adapter, &source, &registry, generation_id)?;
    ResourceGraph {
        schema_version: RESOURCE_GRAPH_SCHEMA_VERSION,
        generation_id,
        nodes: nodes.clone(),
        edges: edges.clone(),
    }
    .validate()
    .map_err(|error| AdapterError::InvalidInput(error.to_string()))?;
    store.begin_generation(&GenerationDescriptor {
        generation_id: *generation_id.as_bytes(),
        source_snapshot_hash: source_hash,
        adapter_id: adapter.manifest().adapter_id.clone(),
        adapter_build: adapter.manifest().adapter_build.clone(),
        identity_version: adapter.manifest().identity_version,
        created_at_ms,
    })?;

    let resource_batch = GenerationBatch {
        resources: nodes
            .iter()
            .map(|node| GenerationResourceRecord {
                resource_id: *node.resource_id.as_bytes(),
                resource_key: node.resource_key,
                kind: format!("{:?}", node.kind),
                semantic_path: node.semantic_path.clone(),
                locator_json: serde_json::to_vec(&node.locator).expect("locator serializes"),
            })
            .collect(),
        edges: edges
            .iter()
            .map(|edge| GenerationEdgeRecord {
                from_resource_id: *edge.from.as_bytes(),
                to_resource_id: *edge.to.as_bytes(),
                edge_kind: format!("{:?}", edge.kind),
                ordinal: edge.ordinal,
            })
            .collect(),
        ..GenerationBatch::default()
    };
    store.append_generation_batch(
        generation_id.as_bytes(),
        &hash_parts(&[b"txt-resources", generation_id.as_bytes()]),
        &hash_parts(&[b"txt-resources-payload", &source_hash]),
        &resource_batch,
    )?;

    let text_resource = nodes
        .iter()
        .find(|node| node.kind == ResourceKind::TextStream)
        .ok_or_else(|| AdapterError::InvalidInput("TXT inventory has no text stream".to_owned()))?
        .resource_id;
    let mut cursor = None;
    let mut units = Vec::new();
    loop {
        let page = adapter.extract(
            &source,
            generation_id,
            text_resource,
            cursor.as_ref(),
            &registry,
            &execution,
        )?;
        for unit in &page.items {
            unit.content
                .validate()
                .map_err(|error| AdapterError::InvalidInput(error.to_string()))?;
        }
        units.extend(page.items);
        cursor = page.next_cursor;
        if cursor.is_none() {
            break;
        }
    }

    let previous_units = if let Some(active_generation) = store.active_generation()? {
        store
            .generation_units(&active_generation)?
            .into_iter()
            .map(|unit| (unit.source_unit_key, unit.unit_id))
            .collect::<HashMap<_, _>>()
    } else {
        HashMap::new()
    };

    let mut unit_batch = GenerationBatch::default();
    for (index, unit) in units.iter().enumerate() {
        let extracted_hash = hash_parts(&[
            b"txt-extracted-unit-v1",
            generation_id.as_bytes(),
            &(index as u64).to_be_bytes(),
            &unit.source_unit_key,
        ]);
        let extracted_unit_id: [u8; 16] = extracted_hash[..16].try_into().expect("hash prefix");
        unit_batch.units.push(GenerationUnitRecord {
            extracted_unit_id,
            source_unit_key: unit.source_unit_key,
            resource_id: *unit.resource_id.as_bytes(),
            locator_json: serde_json::to_vec(&unit.locator).expect("locator serializes"),
            tir_json: serde_json::to_vec(&unit.content).expect("TIR serializes"),
            reading_order: index as u64,
        });

        let binding_hash = hash_parts(&[
            b"txt-binding-v1",
            generation_id.as_bytes(),
            &extracted_unit_id,
        ]);
        let binding_id: [u8; 16] = binding_hash[..16].try_into().expect("hash prefix");
        if let Some(existing_unit_id) = previous_units.get(&unit.source_unit_key).copied() {
            let candidates_json = serde_json::to_vec(&vec![existing_unit_id])
                .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
            unit_batch.bindings.push(GenerationBindingRecord {
                binding_id,
                extracted_unit_id,
                disposition: "Exact".to_owned(),
                selected_unit_id: Some(existing_unit_id),
                policy_version: 1,
                candidates_hash: candidate_set_hash(&candidates_json),
                candidates_json,
            });
        } else {
            let candidates_json = serde_json::to_vec(&Vec::<[u8; 16]>::new())
                .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
            unit_batch.bindings.push(GenerationBindingRecord {
                binding_id,
                extracted_unit_id,
                disposition: "Orphaned".to_owned(),
                selected_unit_id: None,
                policy_version: 1,
                candidates_hash: candidate_set_hash(&candidates_json),
                candidates_json,
            });
        }
    }
    store.append_generation_batch(
        generation_id.as_bytes(),
        &hash_parts(&[b"txt-units", generation_id.as_bytes()]),
        &hash_parts(&[b"txt-units-payload", &source_hash]),
        &unit_batch,
    )?;
    store.seal_generation(generation_id.as_bytes())?;
    store.activate_generation(generation_id.as_bytes(), created_at_ms)?;
    Ok(TxtImportReport {
        generation_id: *generation_id.as_bytes(),
        source_hash,
        byte_length,
        units: units.len(),
        activated: true,
        review_required: 0,
    })
}

struct FormatBindingPlan {
    disposition: &'static str,
    selected_unit_id: Option<[u8; 16]>,
    candidates: Vec<[u8; 16]>,
}

fn plan_format_bindings(
    store: &ProjectStore,
    units: &[ExtractedUnit],
) -> Result<Vec<FormatBindingPlan>, KernelError> {
    let Some(active_generation) = store.active_generation()? else {
        return Ok(units
            .iter()
            .map(|_| FormatBindingPlan {
                disposition: "Orphaned",
                selected_unit_id: None,
                candidates: Vec::new(),
            })
            .collect());
    };
    let previous = store.generation_units(&active_generation)?;
    let exact = previous
        .iter()
        .map(|unit| (unit.source_unit_key, unit.unit_id))
        .collect::<HashMap<_, _>>();
    let exact_claims = units
        .iter()
        .filter_map(|unit| exact.get(&unit.source_unit_key).copied())
        .collect::<HashSet<_>>();

    let mut old_by_text: HashMap<String, Vec<[u8; 16]>> = HashMap::new();
    for unit in &previous {
        if !exact_claims.contains(&unit.unit_id) {
            old_by_text
                .entry(tir_text(&unit.tir_json)?)
                .or_default()
                .push(unit.unit_id);
        }
    }
    for candidates in old_by_text.values_mut() {
        candidates.sort_unstable();
        candidates.dedup();
    }

    let mut new_text = Vec::with_capacity(units.len());
    for unit in units {
        let text = unit_text(&unit.content);
        new_text.push(text);
    }

    Ok(units
        .iter()
        .zip(new_text)
        .map(|(unit, text)| {
            if let Some(selected) = exact.get(&unit.source_unit_key).copied() {
                return FormatBindingPlan {
                    disposition: "Exact",
                    selected_unit_id: Some(selected),
                    candidates: vec![selected],
                };
            }
            let candidates = old_by_text.get(&text).cloned().unwrap_or_default();
            if candidates.len() == 1 {
                FormatBindingPlan {
                    disposition: "Shifted",
                    selected_unit_id: None,
                    candidates,
                }
            } else if candidates.len() >= 2 {
                FormatBindingPlan {
                    disposition: "Ambiguous",
                    selected_unit_id: None,
                    candidates,
                }
            } else {
                FormatBindingPlan {
                    disposition: "Orphaned",
                    selected_unit_id: None,
                    candidates: Vec::new(),
                }
            }
        })
        .collect())
}

fn tir_text(tir_json: &[u8]) -> Result<String, KernelError> {
    let content: UnitContent = serde_json::from_slice(tir_json)
        .map_err(|error| AdapterError::InvalidInput(format!("invalid stored TIR: {error}")))?;
    Ok(unit_text(&content))
}

fn unit_text(content: &UnitContent) -> String {
    content
        .tokens
        .iter()
        .filter_map(|token| match token {
            Token::Text { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect()
}

fn format_binding_review(binding: GenerationBindingView) -> FormatBindingReview {
    FormatBindingReview {
        binding_id: binding.binding_id,
        extracted_unit_id: binding.extracted_unit_id,
        disposition: binding.disposition,
        candidates: binding.candidates,
        candidates_hash: binding.candidates_hash,
    }
}

fn adapter_metadata(format: FormatKind) -> Result<(String, String, u32), KernelError> {
    let adapter = adapter_for_format(format)?;
    adapter.manifest().validate()?;
    Ok((
        adapter.manifest().adapter_id.clone(),
        adapter.manifest().adapter_build.clone(),
        adapter.manifest().identity_version,
    ))
}

fn prepare_format_source_with_worker(
    format: FormatKind,
    object_root: impl AsRef<Path>,
    source_hash: [u8; 32],
) -> Result<PreparedFormatImport, KernelError> {
    let object_root = object_root.as_ref();
    let source_path = cas_path(object_root, &source_hash);
    let byte_length = fs::metadata(&source_path)?.len();
    let mut client = FormatWorkerClient::spawn(format)?;
    let session_id = client.load_object(&source_path, source_hash, byte_length)?;
    let (adapter_id, adapter_build, identity_version) = adapter_metadata(format)?;
    let probe: WorkerProbeReply =
        client.request(&FormatWorkerRequest::ProbeLoaded { session_id })?;
    if probe.detected_media_type.as_deref() != Some(format.media_type()) {
        return Err(KernelError::Adapter(AdapterError::InvalidInput(
            probe.reason_code,
        )));
    }
    if probe.adapter_id != adapter_id
        || probe.adapter_build != adapter_build
        || probe.identity_version != identity_version
    {
        return Err(KernelError::WorkerDiagnostic(format!(
            "{} worker adapter metadata does not match the application adapter",
            format.label()
        )));
    }

    let generation_id = babel_domain::core::GenerationId::new();
    let (nodes, edges) =
        collect_format_inventory_from_worker(&mut client, session_id, generation_id)?;
    ResourceGraph {
        schema_version: RESOURCE_GRAPH_SCHEMA_VERSION,
        generation_id,
        nodes: nodes.clone(),
        edges: edges.clone(),
    }
    .validate()
    .map_err(|error| AdapterError::InvalidInput(error.to_string()))?;
    let text_resource = nodes
        .iter()
        .find(|node| node.kind == ResourceKind::TextStream)
        .ok_or_else(|| {
            AdapterError::InvalidInput(format!("{} inventory has no text stream", format.label()))
        })?
        .resource_id;
    let units =
        collect_format_units_from_worker(&mut client, session_id, generation_id, text_resource)?;

    Ok(PreparedFormatImport {
        format,
        source_hash,
        byte_length,
        generation_id,
        adapter_id,
        adapter_build,
        identity_version,
        nodes,
        edges,
        units,
    })
}

fn commit_prepared_format(
    store: &mut ProjectStore,
    prepared: PreparedFormatImport,
    created_at_ms: i64,
    mut yield_at_boundary: impl FnMut(&mut ProjectStore) -> Result<(), KernelError>,
) -> Result<FormatImportReport, KernelError> {
    let commit_started = Instant::now();
    let PreparedFormatImport {
        format,
        source_hash,
        byte_length,
        generation_id,
        adapter_id,
        adapter_build,
        identity_version,
        nodes,
        edges,
        units,
    } = prepared;
    store.begin_generation(&GenerationDescriptor {
        generation_id: *generation_id.as_bytes(),
        source_snapshot_hash: source_hash,
        adapter_id,
        adapter_build,
        identity_version,
        created_at_ms,
    })?;

    let resource_batch = GenerationBatch {
        resources: nodes
            .iter()
            .map(|node| GenerationResourceRecord {
                resource_id: *node.resource_id.as_bytes(),
                resource_key: node.resource_key,
                kind: format!("{:?}", node.kind),
                semantic_path: node.semantic_path.clone(),
                locator_json: serde_json::to_vec(&node.locator).expect("locator serializes"),
            })
            .collect(),
        edges: edges
            .iter()
            .map(|edge| GenerationEdgeRecord {
                from_resource_id: *edge.from.as_bytes(),
                to_resource_id: *edge.to.as_bytes(),
                edge_kind: format!("{:?}", edge.kind),
                ordinal: edge.ordinal,
            })
            .collect(),
        ..GenerationBatch::default()
    };
    store.append_generation_batch(
        generation_id.as_bytes(),
        &hash_parts(&[
            format.format_id().as_bytes(),
            b"resources",
            generation_id.as_bytes(),
        ]),
        &hash_parts(&[
            format.format_id().as_bytes(),
            b"resources-payload",
            &source_hash,
        ]),
        &resource_batch,
    )?;
    profile_import("commit.resources", commit_started.elapsed());

    let bindings_started = Instant::now();
    let binding_plans = plan_format_bindings(store, &units)?;
    profile_import("commit.binding_plans", bindings_started.elapsed());
    let empty_candidates_json = b"[]".to_vec();
    let empty_candidates_hash = candidate_set_hash(&empty_candidates_json);

    let batches_started = Instant::now();
    let mut batch_build = Duration::ZERO;
    let mut batch_write = Duration::ZERO;
    for (batch_index, chunk) in units.chunks(FORMAT_WRITER_BATCH_ITEMS).enumerate() {
        let build_started = Instant::now();
        let mut unit_batch = GenerationBatch::default();
        for (offset, unit) in chunk.iter().enumerate() {
            let index = batch_index * FORMAT_WRITER_BATCH_ITEMS + offset;
            let binding_plan = &binding_plans[index];
            let extracted_hash = hash_parts(&[
                format.format_id().as_bytes(),
                b"extracted-unit-v1",
                generation_id.as_bytes(),
                &(index as u64).to_be_bytes(),
                &unit.source_unit_key,
            ]);
            let extracted_unit_id: [u8; 16] = extracted_hash[..16].try_into().expect("hash prefix");
            unit_batch.units.push(GenerationUnitRecord {
                extracted_unit_id,
                source_unit_key: unit.source_unit_key,
                resource_id: *unit.resource_id.as_bytes(),
                locator_json: serde_json::to_vec(&unit.locator).expect("locator serializes"),
                tir_json: serde_json::to_vec(&unit.content).expect("TIR serializes"),
                reading_order: index as u64,
            });

            let binding_hash = hash_parts(&[
                format.format_id().as_bytes(),
                b"binding-v1",
                generation_id.as_bytes(),
                &extracted_unit_id,
            ]);
            let binding_id: [u8; 16] = binding_hash[..16].try_into().expect("hash prefix");
            let (candidates_json, candidates_hash) = if binding_plan.disposition == "Orphaned"
                && binding_plan.candidates.is_empty()
            {
                (empty_candidates_json.clone(), empty_candidates_hash)
            } else {
                let candidates_json = serde_json::to_vec(&binding_plan.candidates)
                    .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
                let candidates_hash = candidate_set_hash(&candidates_json);
                (candidates_json, candidates_hash)
            };
            unit_batch.bindings.push(GenerationBindingRecord {
                binding_id,
                extracted_unit_id,
                disposition: binding_plan.disposition.to_owned(),
                selected_unit_id: binding_plan.selected_unit_id,
                policy_version: 1,
                candidates_hash,
                candidates_json,
            });
        }
        batch_build += build_started.elapsed();
        let batch_ordinal = (batch_index as u64).to_be_bytes();
        let write_started = Instant::now();
        store.append_generation_batch(
            generation_id.as_bytes(),
            &hash_parts(&[
                format.format_id().as_bytes(),
                b"units",
                generation_id.as_bytes(),
                &batch_ordinal,
            ]),
            &hash_parts(&[
                format.format_id().as_bytes(),
                b"units-payload",
                &source_hash,
                &batch_ordinal,
            ]),
            &unit_batch,
        )?;
        batch_write += write_started.elapsed();
        yield_at_boundary(store)?;
    }
    profile_import("commit.batch_build", batch_build);
    profile_import("commit.batch_write", batch_write);
    profile_import("commit.batches_total", batches_started.elapsed());
    let review_required = store.unresolved_binding_count(generation_id.as_bytes())?;
    let activated = review_required == 0;
    if activated {
        let seal_started = Instant::now();
        store.seal_generation(generation_id.as_bytes())?;
        profile_import("commit.seal", seal_started.elapsed());
        let activate_started = Instant::now();
        store.activate_generation(generation_id.as_bytes(), created_at_ms)?;
        profile_import("commit.activate", activate_started.elapsed());
    }
    profile_import("commit.total", commit_started.elapsed());
    Ok(FormatImportReport {
        generation_id: *generation_id.as_bytes(),
        source_hash,
        byte_length,
        units: units.len(),
        activated,
        review_required,
    })
}

fn validate_format_export(
    store: &ProjectStore,
    generation_id: &[u8; 16],
    frozen_commit_sequence: i64,
) -> Result<Vec<FormatValidationIssue>, KernelError> {
    Ok(store
        .frozen_unit_snapshot(generation_id, frozen_commit_sequence)?
        .into_iter()
        .filter(|unit| unit.translation.is_none())
        .map(|unit| FormatValidationIssue {
            source_unit_key: unit.source_unit_key,
            code: "missing-translation".to_owned(),
        })
        .collect())
}

fn adapter_for_format(format: FormatKind) -> Result<Box<dyn Adapter>, KernelError> {
    match format {
        FormatKind::Txt => Ok(Box::new(TxtAdapter::new())),
        FormatKind::Markdown => Ok(Box::new(MarkdownAdapter::new())),
    }
}

fn export_format_bytes(
    store: &ProjectStore,
    object_root: impl AsRef<Path>,
    staging_root: impl AsRef<Path>,
    generation_id: &[u8; 16],
    frozen_commit_sequence: i64,
) -> Result<FormatExportReport, KernelError> {
    let issues = validate_format_export(store, generation_id, frozen_commit_sequence)?;
    if !issues.is_empty() {
        let descriptor = store.source_snapshot_descriptor(generation_id)?;
        return Err(KernelError::FormatValidationFailed {
            adapter_id: descriptor.adapter_id,
            issues: issues.len(),
        });
    }
    let descriptor = store.source_snapshot_descriptor(generation_id)?;
    let format = FormatKind::from_adapter_id(&descriptor.adapter_id)?;
    let object_root = object_root.as_ref();
    let staging_root = staging_root.as_ref();
    let source_path = cas_path(object_root, &descriptor.source_snapshot_hash);
    let byte_length = fs::metadata(&source_path)?.len();
    let registry = CapabilityRegistry::new(object_root, staging_root)?;
    let source = registry.grant_object(descriptor.source_snapshot_hash, byte_length)?;
    let adapter = adapter_for_format(format)?;
    let token = CancellationToken::default();
    let budget = format_budget(byte_length);
    let execution = ExecutionContext::new(&budget, &token);
    let snapshots = store.frozen_unit_snapshot(generation_id, frozen_commit_sequence)?;
    let overlays = snapshots
        .iter()
        .map(|unit| {
            Ok(OverlayUnit {
                source_unit_key: unit.source_unit_key,
                source_locator: serde_json::from_slice(&unit.locator_json).map_err(|error| {
                    AdapterError::InvalidInput(format!("invalid locator JSON: {error}"))
                })?,
                translated_text: unit
                    .translation
                    .clone()
                    .ok_or_else(|| AdapterError::InvalidInput("missing translation".to_owned()))?,
            })
        })
        .collect::<Result<Vec<_>, AdapterError>>()?;
    let plan = adapter.plan_export(
        &source,
        babel_domain::core::GenerationId::from_bytes(*generation_id),
        frozen_commit_sequence,
        &overlays,
        &execution,
    )?;
    let staging = registry.create_staging()?;
    let mut cursor: Option<AdapterCursor> = None;
    loop {
        let page = adapter.materialize(
            &plan,
            &source,
            &overlays,
            &staging,
            cursor.as_ref(),
            &registry,
            &execution,
        )?;
        cursor = page.next_cursor;
        if cursor.is_none() {
            break;
        }
    }
    let verification = adapter.verify_output(&staging, &registry, &execution)?;
    if !verification.valid {
        return Err(KernelError::Adapter(AdapterError::InvalidInput(
            verification.issue_codes.join(","),
        )));
    }
    Ok(FormatExportReport {
        generation_id: *generation_id,
        frozen_commit_sequence,
        output_hash: verification.output_hash,
        byte_length: verification.byte_length,
        bytes: registry.staging_bytes(&staging)?,
    })
}

#[cfg(test)]
fn validate_txt_export(
    store: &ProjectStore,
    generation_id: &[u8; 16],
    frozen_commit_sequence: i64,
) -> Result<Vec<TxtValidationIssue>, KernelError> {
    validate_format_export(store, generation_id, frozen_commit_sequence)
}

#[cfg(test)]
fn export_txt_bytes(
    store: &ProjectStore,
    object_root: impl AsRef<Path>,
    staging_root: impl AsRef<Path>,
    generation_id: &[u8; 16],
    frozen_commit_sequence: i64,
) -> Result<TxtExportReport, KernelError> {
    export_format_bytes(
        store,
        object_root,
        staging_root,
        generation_id,
        frozen_commit_sequence,
    )
}

struct Subscriber {
    sender: SyncSender<CommitEvent>,
    lagged: Arc<AtomicBool>,
}

#[derive(Debug, Error)]
pub enum KernelError {
    #[error("project I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("authoritative database failed: {0}")]
    Storage(#[from] rusqlite::Error),
    #[error("project backup failed: {0}")]
    Backup(#[from] BackupError),
    #[error("system clock is before the Unix epoch: {0}")]
    Clock(#[from] std::time::SystemTimeError),
    #[error("the project writer is no longer available")]
    WriterUnavailable,
    #[error("this project is already open by another writer")]
    ProjectAlreadyOpen,
    #[error("the project writer returned an unexpected response")]
    UnexpectedResponse,
    #[error("format adapter failed: {0}")]
    Adapter(#[from] AdapterError),
    #[error("format worker failed: {0}")]
    Worker(#[from] WorkerError),
    #[error("format worker returned an error: {0}")]
    WorkerDiagnostic(String),
    #[error("format export validation failed for {adapter_id} with {issues} blocking issue(s)")]
    FormatValidationFailed { adapter_id: String, issues: usize },
    #[error("TXT export validation failed with {0} blocking issue(s)")]
    TxtValidationFailed(usize),
}

pub struct Kernel {
    root: PathBuf,
    _project_lock: File,
    project_id: ProjectId,
    interactive: SyncSender<WriterMessage>,
    background: SyncSender<WriterMessage>,
    subscribers: Arc<Mutex<Vec<Subscriber>>>,
    writer: Option<JoinHandle<()>>,
}

impl Kernel {
    pub fn open(root: impl AsRef<Path>) -> Result<Self, KernelError> {
        let root = root.as_ref().to_owned();
        fs::create_dir_all(root.join("objects"))?;
        fs::create_dir_all(root.join("staging"))?;
        fs::create_dir_all(root.join("runtime"))?;
        fs::create_dir_all(root.join("diagnostics"))?;
        let project_lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(root.join("runtime/project.lock"))?;
        if let Err(error) = project_lock.try_lock() {
            let error: std::io::Error = error.into();
            return Err(if error.kind() == std::io::ErrorKind::WouldBlock {
                KernelError::ProjectAlreadyOpen
            } else {
                KernelError::Io(error)
            });
        }
        cas::cleanup_temporary(&root.join("objects"))?;

        let store = ProjectStore::open(root.join("project.sqlite3"))?;
        store.recover_interrupted_tasks(now_millis()?)?;
        let project_id = store.project_id()?;
        let (interactive_tx, interactive_rx) = mpsc::sync_channel(WRITER_QUEUE_CAPACITY);
        let (background_tx, background_rx) = mpsc::sync_channel(WRITER_QUEUE_CAPACITY);
        let subscribers = Arc::new(Mutex::new(Vec::new()));
        let writer_subscribers = Arc::clone(&subscribers);
        let writer_root = root.clone();
        let writer = thread::Builder::new()
            .name("babel-project-writer".to_owned())
            .spawn(move || {
                writer_loop(
                    writer_root,
                    store,
                    interactive_rx,
                    background_rx,
                    writer_subscribers,
                );
            })?;

        Ok(Self {
            root,
            _project_lock: project_lock,
            project_id,
            interactive: interactive_tx,
            background: background_tx,
            subscribers,
            writer: Some(writer),
        })
    }

    pub const fn project_id(&self) -> ProjectId {
        self.project_id
    }

    pub fn subscribe(&self, capacity: usize) -> CommitSubscription {
        let (sender, receiver) = mpsc::sync_channel(capacity.max(1));
        let lagged = Arc::new(AtomicBool::new(false));
        self.subscribers.lock().unwrap().push(Subscriber {
            sender,
            lagged: Arc::clone(&lagged),
        });
        CommitSubscription { receiver, lagged }
    }

    pub fn save_translation(
        &self,
        source_unit_key: [u8; 32],
        command_id: [u8; 32],
        text: String,
        created_at_ms: i64,
    ) -> Result<SaveReceipt, KernelError> {
        let response = self.request(
            &self.interactive,
            Command::SaveTranslation {
                source_unit_key,
                command_id,
                text,
                created_at_ms,
            },
        )?;
        match response {
            Response::Saved(receipt) => Ok(receipt),
            _ => Err(KernelError::UnexpectedResponse),
        }
    }

    pub fn restore_translation(
        &self,
        source_unit_key: [u8; 32],
        command_id: [u8; 32],
        expected_head_revision_id: i64,
        restores_revision_id: i64,
        kind: RevisionKind,
        created_at_ms: i64,
    ) -> Result<SaveReceipt, KernelError> {
        let response = self.request(
            &self.interactive,
            Command::RestoreTranslation {
                source_unit_key,
                command_id,
                expected_head_revision_id,
                restores_revision_id,
                kind,
                created_at_ms,
            },
        )?;
        match response {
            Response::Saved(receipt) => Ok(receipt),
            _ => Err(KernelError::UnexpectedResponse),
        }
    }

    pub fn save_draft(
        &self,
        unit_id: Vec<u8>,
        base_revision_id: Option<i64>,
        client_session_id: String,
        patch: Vec<u8>,
        updated_at_ms: i64,
    ) -> Result<(), KernelError> {
        let response = self.request(
            &self.interactive,
            Command::SaveDraft {
                unit_id,
                base_revision_id,
                client_session_id,
                patch,
                updated_at_ms,
            },
        )?;
        match response {
            Response::Done => Ok(()),
            _ => Err(KernelError::UnexpectedResponse),
        }
    }

    pub fn publish_source(
        &self,
        source_id: [u8; 16],
        media_type: String,
        bytes: Vec<u8>,
        created_at_ms: i64,
    ) -> Result<PublishedObject, KernelError> {
        self.publish_source_reader(source_id, media_type, Cursor::new(bytes), created_at_ms)
    }

    pub fn publish_source_reader(
        &self,
        source_id: [u8; 16],
        media_type: String,
        reader: impl Read,
        created_at_ms: i64,
    ) -> Result<PublishedObject, KernelError> {
        let (hash, path, byte_length) = cas::publish_reader(&self.root.join("objects"), reader)?;
        let response = self.request(
            &self.background,
            Command::RegisterSourceObject {
                source_id,
                media_type,
                hash,
                byte_length,
                created_at_ms,
            },
        )?;
        match response {
            Response::Done => Ok(PublishedObject {
                hash,
                byte_length,
                path,
            }),
            _ => Err(KernelError::UnexpectedResponse),
        }
    }

    pub fn import_txt_reader(
        &self,
        source_id: [u8; 16],
        reader: impl Read,
        created_at_ms: i64,
    ) -> Result<TxtImportReport, KernelError> {
        self.import_format_reader(FormatKind::Txt, source_id, reader, created_at_ms)
    }

    pub fn import_markdown_reader(
        &self,
        source_id: [u8; 16],
        reader: impl Read,
        created_at_ms: i64,
    ) -> Result<MarkdownImportReport, KernelError> {
        self.import_format_reader(FormatKind::Markdown, source_id, reader, created_at_ms)
    }

    fn import_format_reader(
        &self,
        format: FormatKind,
        source_id: [u8; 16],
        reader: impl Read,
        created_at_ms: i64,
    ) -> Result<FormatImportReport, KernelError> {
        let import_started = Instant::now();
        let published = self.publish_source_reader(
            source_id,
            format.media_type().to_owned(),
            reader,
            created_at_ms,
        )?;
        profile_import("import.publish", import_started.elapsed());
        let prepare_started = Instant::now();
        let prepared =
            prepare_format_source_with_worker(format, self.root.join("objects"), published.hash)?;
        profile_import("import.prepare", prepare_started.elapsed());
        let commit_started = Instant::now();
        let response = self.request(
            &self.background,
            Command::CommitFormatImport {
                prepared,
                created_at_ms,
            },
        )?;
        profile_import("import.commit_request", commit_started.elapsed());
        profile_import("import.total", import_started.elapsed());
        match response {
            Response::FormatImported(report) => Ok(report),
            _ => Err(KernelError::UnexpectedResponse),
        }
    }

    pub fn validate_active_txt(&self) -> Result<Vec<TxtValidationIssue>, KernelError> {
        self.validate_active_format(FormatKind::Txt)
    }

    pub fn validate_active_markdown(&self) -> Result<Vec<MarkdownValidationIssue>, KernelError> {
        self.validate_active_format(FormatKind::Markdown)
    }

    fn validate_active_format(
        &self,
        expected_format: FormatKind,
    ) -> Result<Vec<FormatValidationIssue>, KernelError> {
        let response = self.request(
            &self.background,
            Command::ValidateActiveFormat { expected_format },
        )?;
        match response {
            Response::FormatValidation(issues) => Ok(issues),
            _ => Err(KernelError::UnexpectedResponse),
        }
    }

    pub fn pending_txt_bindings(
        &self,
        generation_id: [u8; 16],
    ) -> Result<Vec<TxtBindingReview>, KernelError> {
        self.pending_bindings(generation_id)
    }

    pub fn pending_markdown_bindings(
        &self,
        generation_id: [u8; 16],
    ) -> Result<Vec<MarkdownBindingReview>, KernelError> {
        self.pending_bindings(generation_id)
    }

    pub fn pending_bindings(
        &self,
        generation_id: [u8; 16],
    ) -> Result<Vec<FormatBindingReview>, KernelError> {
        let response =
            self.request(&self.background, Command::PendingBindings { generation_id })?;
        match response {
            Response::FormatBindings(bindings) => Ok(bindings),
            _ => Err(KernelError::UnexpectedResponse),
        }
    }

    pub fn decide_txt_binding(
        &self,
        binding_id: [u8; 16],
        selected_unit_id: [u8; 16],
        expected_candidate_hash: [u8; 32],
        command_id: [u8; 32],
        reason_code: String,
        created_at_ms: i64,
    ) -> Result<(), KernelError> {
        self.decide_binding(
            binding_id,
            selected_unit_id,
            expected_candidate_hash,
            command_id,
            reason_code,
            created_at_ms,
        )
    }

    pub fn decide_binding(
        &self,
        binding_id: [u8; 16],
        selected_unit_id: [u8; 16],
        expected_candidate_hash: [u8; 32],
        command_id: [u8; 32],
        reason_code: String,
        created_at_ms: i64,
    ) -> Result<(), KernelError> {
        let response = self.request(
            &self.interactive,
            Command::DecideBinding {
                binding_id,
                selected_unit_id,
                expected_candidate_hash,
                command_id,
                reason_code,
                created_at_ms,
            },
        )?;
        match response {
            Response::Done => Ok(()),
            _ => Err(KernelError::UnexpectedResponse),
        }
    }

    pub fn reject_txt_binding_as_new(
        &self,
        binding_id: [u8; 16],
        expected_candidate_hash: [u8; 32],
        command_id: [u8; 32],
        reason_code: String,
        created_at_ms: i64,
    ) -> Result<(), KernelError> {
        self.reject_binding_as_new(
            binding_id,
            expected_candidate_hash,
            command_id,
            reason_code,
            created_at_ms,
        )
    }

    pub fn reject_binding_as_new(
        &self,
        binding_id: [u8; 16],
        expected_candidate_hash: [u8; 32],
        command_id: [u8; 32],
        reason_code: String,
        created_at_ms: i64,
    ) -> Result<(), KernelError> {
        let response = self.request(
            &self.interactive,
            Command::RejectBindingAsNew {
                binding_id,
                expected_candidate_hash,
                command_id,
                reason_code,
                created_at_ms,
            },
        )?;
        match response {
            Response::Done => Ok(()),
            _ => Err(KernelError::UnexpectedResponse),
        }
    }

    pub fn activate_txt_import(
        &self,
        generation_id: [u8; 16],
        activated_at_ms: i64,
    ) -> Result<(), KernelError> {
        self.activate_import(generation_id, activated_at_ms)
    }

    pub fn activate_markdown_import(
        &self,
        generation_id: [u8; 16],
        activated_at_ms: i64,
    ) -> Result<(), KernelError> {
        self.activate_import(generation_id, activated_at_ms)
    }

    pub fn activate_import(
        &self,
        generation_id: [u8; 16],
        activated_at_ms: i64,
    ) -> Result<(), KernelError> {
        let response = self.request(
            &self.background,
            Command::ActivateImport {
                generation_id,
                activated_at_ms,
            },
        )?;
        match response {
            Response::Done => Ok(()),
            _ => Err(KernelError::UnexpectedResponse),
        }
    }

    pub fn search(&self, query: String, limit: usize) -> Result<Vec<String>, KernelError> {
        let response = self.request(&self.interactive, Command::Search { query, limit })?;
        match response {
            Response::Search(results) => Ok(results),
            _ => Err(KernelError::UnexpectedResponse),
        }
    }

    pub fn upsert_term(&self, request: UpsertTermRequest) -> Result<(), KernelError> {
        match self.request(&self.interactive, Command::UpsertTerm { request })? {
            Response::Done => Ok(()),
            _ => Err(KernelError::UnexpectedResponse),
        }
    }

    pub fn terms(&self, include_deprecated: bool) -> Result<Vec<TermRecord>, KernelError> {
        match self.request(&self.interactive, Command::Terms { include_deprecated })? {
            Response::Terms(records) => Ok(records),
            _ => Err(KernelError::UnexpectedResponse),
        }
    }

    pub fn find_terms(&self, text: String, limit: usize) -> Result<Vec<TermRecord>, KernelError> {
        match self.request(&self.interactive, Command::FindTerms { text, limit })? {
            Response::Terms(records) => Ok(records),
            _ => Err(KernelError::UnexpectedResponse),
        }
    }

    pub fn add_annotation(&self, request: AddAnnotationRequest) -> Result<(), KernelError> {
        match self.request(&self.interactive, Command::AddAnnotation { request })? {
            Response::Done => Ok(()),
            _ => Err(KernelError::UnexpectedResponse),
        }
    }

    pub fn annotations_for_unit(
        &self,
        unit_id: Vec<u8>,
    ) -> Result<Vec<AnnotationRecord>, KernelError> {
        match self.request(&self.interactive, Command::AnnotationsForUnit { unit_id })? {
            Response::Annotations(records) => Ok(records),
            _ => Err(KernelError::UnexpectedResponse),
        }
    }

    pub fn set_marker(
        &self,
        marker_id: [u8; 16],
        unit_id: Vec<u8>,
        kind: String,
        label: String,
        created_at_ms: i64,
    ) -> Result<(), KernelError> {
        match self.request(
            &self.interactive,
            Command::SetMarker {
                marker_id,
                unit_id,
                kind,
                label,
                created_at_ms,
            },
        )? {
            Response::Done => Ok(()),
            _ => Err(KernelError::UnexpectedResponse),
        }
    }

    pub fn delete_marker(
        &self,
        unit_id: Vec<u8>,
        kind: String,
        label: String,
    ) -> Result<usize, KernelError> {
        match self.request(
            &self.interactive,
            Command::DeleteMarker {
                unit_id,
                kind,
                label,
            },
        )? {
            Response::MarkerDeleted(count) => Ok(count),
            _ => Err(KernelError::UnexpectedResponse),
        }
    }

    pub fn markers_for_unit(&self, unit_id: Vec<u8>) -> Result<Vec<MarkerRecord>, KernelError> {
        match self.request(&self.interactive, Command::MarkersForUnit { unit_id })? {
            Response::Markers(records) => Ok(records),
            _ => Err(KernelError::UnexpectedResponse),
        }
    }

    pub fn translation_history(
        &self,
        source_text: String,
        limit: usize,
    ) -> Result<Vec<TranslationHistoryItem>, KernelError> {
        match self.request(
            &self.interactive,
            Command::TranslationHistory { source_text, limit },
        )? {
            Response::TranslationHistory(records) => Ok(records),
            _ => Err(KernelError::UnexpectedResponse),
        }
    }

    pub fn duplicate_source_groups(
        &self,
        minimum_count: usize,
        limit: usize,
    ) -> Result<Vec<DuplicateSourceGroup>, KernelError> {
        match self.request(
            &self.interactive,
            Command::DuplicateSourceGroups {
                minimum_count,
                limit,
            },
        )? {
            Response::DuplicateSourceGroups(records) => Ok(records),
            _ => Err(KernelError::UnexpectedResponse),
        }
    }

    pub fn preview_replace_translations(
        &self,
        find_text: String,
        replacement_text: String,
        limit: usize,
    ) -> Result<Vec<ReplacePreviewItem>, KernelError> {
        match self.request(
            &self.interactive,
            Command::PreviewReplaceTranslations {
                find_text,
                replacement_text,
                limit,
            },
        )? {
            Response::ReplacePreview(records) => Ok(records),
            _ => Err(KernelError::UnexpectedResponse),
        }
    }

    pub fn apply_replace_translations(
        &self,
        batch_id: [u8; 32],
        find_text: String,
        replacement_text: String,
        expected: Vec<ReplacePreviewItem>,
        created_at_ms: i64,
    ) -> Result<BatchReplaceReceipt, KernelError> {
        match self.request(
            &self.interactive,
            Command::ApplyReplaceTranslations {
                batch_id,
                find_text,
                replacement_text,
                expected,
                created_at_ms,
            },
        )? {
            Response::BatchReplaced(receipt) => Ok(receipt),
            _ => Err(KernelError::UnexpectedResponse),
        }
    }

    pub fn export_active_txt(&self) -> Result<TxtExportReport, KernelError> {
        self.export_active_format(FormatKind::Txt)
    }

    pub fn export_active_markdown(&self) -> Result<MarkdownExportReport, KernelError> {
        self.export_active_format(FormatKind::Markdown)
    }

    fn export_active_format(
        &self,
        expected_format: FormatKind,
    ) -> Result<FormatExportReport, KernelError> {
        let response = self.request(
            &self.background,
            Command::ExportActiveFormat { expected_format },
        )?;
        match response {
            Response::FormatExported(report) => Ok(report),
            _ => Err(KernelError::UnexpectedResponse),
        }
    }

    pub fn create_task(
        &self,
        task_id: TaskId,
        task_kind: String,
        priority: WorkPriority,
        created_at_ms: i64,
    ) -> Result<(), KernelError> {
        let response = self.request(
            &self.background,
            Command::CreateTask {
                task_id,
                task_kind,
                priority,
                created_at_ms,
            },
        )?;
        match response {
            Response::Done => Ok(()),
            _ => Err(KernelError::UnexpectedResponse),
        }
    }

    pub fn transition_task(
        &self,
        task_id: TaskId,
        state: TaskState,
        failure_code: Option<String>,
        updated_at_ms: i64,
    ) -> Result<TaskRecord, KernelError> {
        let response = self.request(
            &self.background,
            Command::TransitionTask {
                task_id,
                state,
                failure_code,
                updated_at_ms,
            },
        )?;
        match response {
            Response::Task(task) => Ok(task),
            _ => Err(KernelError::UnexpectedResponse),
        }
    }

    pub fn record_diagnostic(
        &self,
        severity: String,
        code: String,
        user_message: String,
        technical_detail: Option<String>,
        created_at_ms: i64,
    ) -> Result<i64, KernelError> {
        let response = self.request(
            &self.background,
            Command::RecordDiagnostic {
                severity,
                code,
                user_message,
                technical_detail,
                created_at_ms,
            },
        )?;
        match response {
            Response::Diagnostic(event_id) => Ok(event_id),
            _ => Err(KernelError::UnexpectedResponse),
        }
    }

    pub fn backup_to(
        &self,
        target_root: impl AsRef<Path>,
        created_at_ms: i64,
    ) -> Result<i64, KernelError> {
        let lease_id = TaskId::new();
        let response = self.request(
            &self.background,
            Command::BeginBackup {
                lease_id: *lease_id.as_bytes(),
                created_at_ms,
            },
        )?;
        let snapshot = match response {
            Response::BackupStarted(snapshot) => snapshot,
            _ => return Err(KernelError::UnexpectedResponse),
        };

        let materialized = snapshot.materialize(&self.root.join("objects"), target_root.as_ref());
        let completed = materialized.is_ok();
        let finish = self.request(
            &self.background,
            Command::FinishBackupPin {
                lease_id: *lease_id.as_bytes(),
                completed,
            },
        );
        materialized?;
        match finish? {
            Response::Done => Ok(snapshot.commit_sequence()),
            _ => Err(KernelError::UnexpectedResponse),
        }
    }

    pub fn gc_dry_run(&self) -> Result<GcReport, KernelError> {
        let older_than = SystemTime::now() - GC_GRACE_PERIOD;
        Ok(gc::dry_run_database(
            &self.database_path(),
            &self.root.join("objects"),
            older_than,
        )?)
    }

    pub fn garbage_collect(&self) -> Result<GcReport, KernelError> {
        let plan = self.gc_dry_run()?;
        let response = self.request(
            &self.background,
            Command::GarbageCollectCandidates {
                candidates: plan.candidates,
            },
        )?;
        match response {
            Response::GarbageCollected(report) => Ok(report),
            _ => Err(KernelError::UnexpectedResponse),
        }
    }

    pub fn database_path(&self) -> PathBuf {
        self.root.join("project.sqlite3")
    }

    pub fn query(&self) -> Result<ProjectQuery, KernelError> {
        Ok(ProjectQuery::open(self.database_path())?)
    }

    fn request(
        &self,
        queue: &SyncSender<WriterMessage>,
        command: Command,
    ) -> Result<Response, KernelError> {
        let (reply, response) = mpsc::sync_channel(1);
        queue
            .send(WriterMessage { command, reply })
            .map_err(|_| KernelError::WriterUnavailable)?;
        response
            .recv()
            .map_err(|_| KernelError::WriterUnavailable)?
    }
}

fn profile_import(stage: &str, elapsed: Duration) {
    if std::env::var_os("BABEL_PROFILE_IMPORT").is_some() {
        eprintln!("babel-import-profile {stage}={}ms", elapsed.as_millis());
    }
}

impl Drop for Kernel {
    fn drop(&mut self) {
        let (reply, _response) = mpsc::sync_channel(1);
        let _ = self.background.send(WriterMessage {
            command: Command::Shutdown,
            reply,
        });
        if let Some(writer) = self.writer.take() {
            let _ = writer.join();
        }
    }
}

struct WriterMessage {
    command: Command,
    reply: SyncSender<Result<Response, KernelError>>,
}

enum Command {
    SaveTranslation {
        source_unit_key: [u8; 32],
        command_id: [u8; 32],
        text: String,
        created_at_ms: i64,
    },
    RestoreTranslation {
        source_unit_key: [u8; 32],
        command_id: [u8; 32],
        expected_head_revision_id: i64,
        restores_revision_id: i64,
        kind: RevisionKind,
        created_at_ms: i64,
    },
    SaveDraft {
        unit_id: Vec<u8>,
        base_revision_id: Option<i64>,
        client_session_id: String,
        patch: Vec<u8>,
        updated_at_ms: i64,
    },
    RegisterSourceObject {
        source_id: [u8; 16],
        media_type: String,
        hash: [u8; 32],
        byte_length: u64,
        created_at_ms: i64,
    },
    CreateTask {
        task_id: TaskId,
        task_kind: String,
        priority: WorkPriority,
        created_at_ms: i64,
    },
    TransitionTask {
        task_id: TaskId,
        state: TaskState,
        failure_code: Option<String>,
        updated_at_ms: i64,
    },
    RecordDiagnostic {
        severity: String,
        code: String,
        user_message: String,
        technical_detail: Option<String>,
        created_at_ms: i64,
    },
    BeginBackup {
        lease_id: [u8; 16],
        created_at_ms: i64,
    },
    FinishBackupPin {
        lease_id: [u8; 16],
        completed: bool,
    },
    GarbageCollectCandidates {
        candidates: Vec<babel_storage::gc::GcCandidate>,
    },
    CommitFormatImport {
        prepared: PreparedFormatImport,
        created_at_ms: i64,
    },
    PendingBindings {
        generation_id: [u8; 16],
    },
    DecideBinding {
        binding_id: [u8; 16],
        selected_unit_id: [u8; 16],
        expected_candidate_hash: [u8; 32],
        command_id: [u8; 32],
        reason_code: String,
        created_at_ms: i64,
    },
    RejectBindingAsNew {
        binding_id: [u8; 16],
        expected_candidate_hash: [u8; 32],
        command_id: [u8; 32],
        reason_code: String,
        created_at_ms: i64,
    },
    ActivateImport {
        generation_id: [u8; 16],
        activated_at_ms: i64,
    },
    ValidateActiveFormat {
        expected_format: FormatKind,
    },
    ExportActiveFormat {
        expected_format: FormatKind,
    },
    UpsertTerm {
        request: UpsertTermRequest,
    },
    Terms {
        include_deprecated: bool,
    },
    FindTerms {
        text: String,
        limit: usize,
    },
    AddAnnotation {
        request: AddAnnotationRequest,
    },
    AnnotationsForUnit {
        unit_id: Vec<u8>,
    },
    SetMarker {
        marker_id: [u8; 16],
        unit_id: Vec<u8>,
        kind: String,
        label: String,
        created_at_ms: i64,
    },
    DeleteMarker {
        unit_id: Vec<u8>,
        kind: String,
        label: String,
    },
    MarkersForUnit {
        unit_id: Vec<u8>,
    },
    TranslationHistory {
        source_text: String,
        limit: usize,
    },
    DuplicateSourceGroups {
        minimum_count: usize,
        limit: usize,
    },
    PreviewReplaceTranslations {
        find_text: String,
        replacement_text: String,
        limit: usize,
    },
    ApplyReplaceTranslations {
        batch_id: [u8; 32],
        find_text: String,
        replacement_text: String,
        expected: Vec<ReplacePreviewItem>,
        created_at_ms: i64,
    },
    Search {
        query: String,
        limit: usize,
    },
    Shutdown,
}

enum Response {
    Saved(SaveReceipt),
    Task(TaskRecord),
    Diagnostic(i64),
    GarbageCollected(GcReport),
    BackupStarted(BackupSnapshot),
    FormatImported(FormatImportReport),
    FormatBindings(Vec<FormatBindingReview>),
    FormatValidation(Vec<FormatValidationIssue>),
    FormatExported(FormatExportReport),
    Terms(Vec<TermRecord>),
    Annotations(Vec<AnnotationRecord>),
    Markers(Vec<MarkerRecord>),
    MarkerDeleted(usize),
    TranslationHistory(Vec<TranslationHistoryItem>),
    DuplicateSourceGroups(Vec<DuplicateSourceGroup>),
    ReplacePreview(Vec<ReplacePreviewItem>),
    BatchReplaced(BatchReplaceReceipt),
    Search(Vec<String>),
    Done,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ReadyQueue {
    Interactive,
    Background,
    None,
}

const fn choose_ready_queue(
    interactive_ready: bool,
    background_ready: bool,
    interactive_burst: usize,
) -> ReadyQueue {
    if background_ready && (!interactive_ready || interactive_burst >= MAX_INTERACTIVE_BURST) {
        ReadyQueue::Background
    } else if interactive_ready {
        ReadyQueue::Interactive
    } else if background_ready {
        ReadyQueue::Background
    } else {
        ReadyQueue::None
    }
}

fn writer_loop(
    root: PathBuf,
    mut store: ProjectStore,
    interactive: Receiver<WriterMessage>,
    background: Receiver<WriterMessage>,
    subscribers: Arc<Mutex<Vec<Subscriber>>>,
) {
    let mut interactive_burst = 0_usize;
    loop {
        let forced_background =
            choose_ready_queue(true, true, interactive_burst) == ReadyQueue::Background;
        let message = match forced_background.then(|| background.try_recv()).transpose() {
            Ok(Some(message)) => {
                interactive_burst = 0;
                message
            }
            Err(TryRecvError::Disconnected) | Err(TryRecvError::Empty) | Ok(None) => {
                match interactive.try_recv() {
                    Ok(message) => {
                        interactive_burst += 1;
                        message
                    }
                    Err(TryRecvError::Disconnected) | Err(TryRecvError::Empty) => {
                        match background.recv_timeout(Duration::from_millis(2)) {
                            Ok(message) => {
                                interactive_burst = 0;
                                message
                            }
                            Err(RecvTimeoutError::Timeout) => match interactive.try_recv() {
                                Ok(message) => {
                                    interactive_burst += 1;
                                    message
                                }
                                Err(TryRecvError::Empty) => continue,
                                Err(TryRecvError::Disconnected) => break,
                            },
                            Err(RecvTimeoutError::Disconnected) => match interactive.recv() {
                                Ok(message) => message,
                                Err(_) => break,
                            },
                        }
                    }
                }
            }
        };

        if matches!(message.command, Command::Shutdown) {
            let _ = message.reply.send(Ok(Response::Done));
            break;
        }
        let result = execute(
            &root,
            &mut store,
            message.command,
            &subscribers,
            Some(&interactive),
        );
        let _ = message.reply.send(result);
    }
}

fn execute(
    root: &Path,
    store: &mut ProjectStore,
    command: Command,
    subscribers: &Arc<Mutex<Vec<Subscriber>>>,
    interactive: Option<&Receiver<WriterMessage>>,
) -> Result<Response, KernelError> {
    match command {
        Command::SaveTranslation {
            source_unit_key,
            command_id,
            text,
            created_at_ms,
        } => {
            let receipt =
                store.save_translation(&source_unit_key, &command_id, &text, created_at_ms)?;
            if !receipt.replayed {
                publish_event(
                    subscribers,
                    CommitEvent::TranslationCommitted {
                        revision_id: receipt.revision_id,
                        commit_sequence: receipt.commit_sequence,
                    },
                );
            }
            Ok(Response::Saved(receipt))
        }
        Command::RestoreTranslation {
            source_unit_key,
            command_id,
            expected_head_revision_id,
            restores_revision_id,
            kind,
            created_at_ms,
        } => {
            let receipt = store.restore_translation(
                &source_unit_key,
                &command_id,
                expected_head_revision_id,
                restores_revision_id,
                kind,
                created_at_ms,
            )?;
            if !receipt.replayed {
                publish_event(
                    subscribers,
                    CommitEvent::TranslationCommitted {
                        revision_id: receipt.revision_id,
                        commit_sequence: receipt.commit_sequence,
                    },
                );
            }
            Ok(Response::Saved(receipt))
        }
        Command::SaveDraft {
            unit_id,
            base_revision_id,
            client_session_id,
            patch,
            updated_at_ms,
        } => {
            store.save_draft(
                &unit_id,
                base_revision_id,
                &client_session_id,
                &patch,
                updated_at_ms,
            )?;
            Ok(Response::Done)
        }
        Command::RegisterSourceObject {
            source_id,
            media_type,
            hash,
            byte_length,
            created_at_ms,
        } => {
            let object = ObjectRecord {
                hash,
                byte_length,
                media_type,
            };
            store.register_object_reference("source", &source_id, &object, created_at_ms)?;
            publish_event(
                subscribers,
                CommitEvent::ObjectReferenced { object_hash: hash },
            );
            Ok(Response::Done)
        }
        Command::CreateTask {
            task_id,
            task_kind,
            priority,
            created_at_ms,
        } => {
            store.create_task(task_id, &task_kind, priority, created_at_ms)?;
            publish_event(
                subscribers,
                CommitEvent::TaskChanged {
                    task_id: *task_id.as_bytes(),
                    state: TaskState::Pending,
                },
            );
            Ok(Response::Done)
        }
        Command::TransitionTask {
            task_id,
            state,
            failure_code,
            updated_at_ms,
        } => {
            let task =
                store.transition_task(task_id, state, failure_code.as_deref(), updated_at_ms)?;
            publish_event(
                subscribers,
                CommitEvent::TaskChanged {
                    task_id: *task_id.as_bytes(),
                    state,
                },
            );
            Ok(Response::Task(task))
        }
        Command::RecordDiagnostic {
            severity,
            code,
            user_message,
            technical_detail,
            created_at_ms,
        } => Ok(Response::Diagnostic(store.record_diagnostic(
            &severity,
            &code,
            &user_message,
            technical_detail.as_deref(),
            created_at_ms,
        )?)),
        Command::BeginBackup {
            lease_id,
            created_at_ms,
        } => {
            let pin = store.begin_backup_pin(&lease_id, created_at_ms)?;
            match BackupSnapshot::capture_pinned(
                &root.join("project.sqlite3"),
                pin.commit_sequence,
                pin.object_hashes,
            ) {
                Ok(snapshot) => Ok(Response::BackupStarted(snapshot)),
                Err(error) => {
                    store.finish_backup_pin(&lease_id, false)?;
                    Err(KernelError::Backup(error))
                }
            }
        }
        Command::FinishBackupPin {
            lease_id,
            completed,
        } => {
            store.finish_backup_pin(&lease_id, completed)?;
            Ok(Response::Done)
        }
        Command::GarbageCollectCandidates { candidates } => {
            let objects = root.join("objects");
            let report = gc::sweep_candidates(
                store,
                &objects,
                candidates,
                GC_BATCH_ITEMS,
                GC_BATCH_WALL_TIME,
            )?;
            Ok(Response::GarbageCollected(report))
        }
        Command::CommitFormatImport {
            prepared,
            created_at_ms,
        } => {
            let mut at_boundary = |store: &mut ProjectStore| {
                if let Some(interactive) = interactive {
                    service_interactive(root, store, interactive, subscribers)?;
                }
                Ok(())
            };
            Ok(Response::FormatImported(commit_prepared_format(
                store,
                prepared,
                created_at_ms,
                &mut at_boundary,
            )?))
        }
        Command::PendingBindings { generation_id } => Ok(Response::FormatBindings(
            store
                .unresolved_bindings(&generation_id)?
                .into_iter()
                .map(format_binding_review)
                .collect(),
        )),
        Command::DecideBinding {
            binding_id,
            selected_unit_id,
            expected_candidate_hash,
            command_id,
            reason_code,
            created_at_ms,
        } => {
            store.decide_binding(
                &command_id,
                &binding_id,
                &selected_unit_id,
                &expected_candidate_hash,
                &reason_code,
                created_at_ms,
            )?;
            Ok(Response::Done)
        }
        Command::RejectBindingAsNew {
            binding_id,
            expected_candidate_hash,
            command_id,
            reason_code,
            created_at_ms,
        } => {
            store.reject_binding_as_new(
                &command_id,
                &binding_id,
                &expected_candidate_hash,
                &reason_code,
                created_at_ms,
            )?;
            Ok(Response::Done)
        }
        Command::ActivateImport {
            generation_id,
            activated_at_ms,
        } => {
            store.seal_generation(&generation_id)?;
            store.activate_generation(&generation_id, activated_at_ms)?;
            Ok(Response::Done)
        }
        Command::ValidateActiveFormat { expected_format } => {
            let generation_id = store
                .active_generation()?
                .ok_or(rusqlite::Error::QueryReturnedNoRows)?;
            ensure_generation_format(store, &generation_id, expected_format)?;
            Ok(Response::FormatValidation(validate_format_export(
                store,
                &generation_id,
                store.commit_sequence()?,
            )?))
        }
        Command::ExportActiveFormat { expected_format } => {
            let generation_id = store
                .active_generation()?
                .ok_or(rusqlite::Error::QueryReturnedNoRows)?;
            ensure_generation_format(store, &generation_id, expected_format)?;
            Ok(Response::FormatExported(export_format_bytes(
                store,
                root.join("objects"),
                root.join("staging"),
                &generation_id,
                store.commit_sequence()?,
            )?))
        }
        Command::UpsertTerm { request } => {
            store.upsert_term(&request)?;
            Ok(Response::Done)
        }
        Command::Terms { include_deprecated } => {
            Ok(Response::Terms(store.terms(include_deprecated)?))
        }
        Command::FindTerms { text, limit } => Ok(Response::Terms(store.find_terms(&text, limit)?)),
        Command::AddAnnotation { request } => {
            store.add_annotation(
                &request.annotation_id,
                &request.unit_id,
                request.base_revision_id,
                request.grapheme_start..request.grapheme_end,
                &request.body,
                request.created_at_ms,
            )?;
            Ok(Response::Done)
        }
        Command::AnnotationsForUnit { unit_id } => {
            Ok(Response::Annotations(store.annotations_for_unit(&unit_id)?))
        }
        Command::SetMarker {
            marker_id,
            unit_id,
            kind,
            label,
            created_at_ms,
        } => {
            store.set_marker(&marker_id, &unit_id, &kind, &label, created_at_ms)?;
            Ok(Response::Done)
        }
        Command::DeleteMarker {
            unit_id,
            kind,
            label,
        } => Ok(Response::MarkerDeleted(
            store.delete_marker(&unit_id, &kind, &label)?,
        )),
        Command::MarkersForUnit { unit_id } => {
            Ok(Response::Markers(store.markers_for_unit(&unit_id)?))
        }
        Command::TranslationHistory { source_text, limit } => Ok(Response::TranslationHistory(
            store.translation_history(&source_text, limit)?,
        )),
        Command::DuplicateSourceGroups {
            minimum_count,
            limit,
        } => Ok(Response::DuplicateSourceGroups(
            store.duplicate_source_groups(minimum_count, limit)?,
        )),
        Command::PreviewReplaceTranslations {
            find_text,
            replacement_text,
            limit,
        } => Ok(Response::ReplacePreview(
            store.preview_replace_translations(&find_text, &replacement_text, limit)?,
        )),
        Command::ApplyReplaceTranslations {
            batch_id,
            find_text,
            replacement_text,
            expected,
            created_at_ms,
        } => {
            let receipt = store.apply_replace_translations(
                &batch_id,
                &find_text,
                &replacement_text,
                &expected,
                created_at_ms,
            )?;
            if !receipt.replayed {
                publish_event(
                    subscribers,
                    CommitEvent::TranslationBatchCommitted {
                        affected_units: receipt.affected_units,
                        commit_sequence_end: receipt.commit_sequence_end,
                    },
                );
            }
            Ok(Response::BatchReplaced(receipt))
        }
        Command::Search { query, limit } => {
            while store.flush_search_dirty(2_000)? != 0 {}
            Ok(Response::Search(store.search(&query, limit)?))
        }
        Command::Shutdown => Ok(Response::Done),
    }
}

fn ensure_generation_format(
    store: &ProjectStore,
    generation_id: &[u8; 16],
    expected: FormatKind,
) -> Result<(), KernelError> {
    let descriptor = store.source_snapshot_descriptor(generation_id)?;
    let actual = FormatKind::from_adapter_id(&descriptor.adapter_id)?;
    if actual == expected {
        Ok(())
    } else {
        Err(KernelError::WorkerDiagnostic(format!(
            "active generation uses {}, not {}",
            actual.label(),
            expected.label()
        )))
    }
}

fn service_interactive(
    root: &Path,
    store: &mut ProjectStore,
    interactive: &Receiver<WriterMessage>,
    subscribers: &Arc<Mutex<Vec<Subscriber>>>,
) -> Result<(), KernelError> {
    for _ in 0..MAX_INTERACTIVE_BURST {
        let message = match interactive.try_recv() {
            Ok(message) => message,
            Err(TryRecvError::Empty) => break,
            Err(TryRecvError::Disconnected) => return Err(KernelError::WriterUnavailable),
        };
        let result = execute(root, store, message.command, subscribers, None);
        let _ = message.reply.send(result);
    }
    Ok(())
}

fn publish_event(subscribers: &Arc<Mutex<Vec<Subscriber>>>, event: CommitEvent) {
    subscribers.lock().unwrap().retain(|subscriber| {
        match subscriber.sender.try_send(event.clone()) {
            Ok(()) => true,
            Err(TrySendError::Full(_)) => {
                subscriber.lagged.store(true, Ordering::Release);
                true
            }
            Err(TrySendError::Disconnected(_)) => false,
        }
    });
}

#[cfg(test)]
fn collect_txt_inventory(
    adapter: &TxtAdapter,
    source: &babel_adapter_protocol::ObjectHandle,
    registry: &CapabilityRegistry,
    generation_id: babel_domain::core::GenerationId,
) -> Result<
    (
        Vec<babel_resource_graph::ResourceNode>,
        Vec<babel_resource_graph::ResourceEdge>,
    ),
    KernelError,
> {
    let token = CancellationToken::default();
    let budget = format_budget(source.byte_length);
    let execution = ExecutionContext::new(&budget, &token);
    let mut cursor = None;
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    loop {
        let page =
            adapter.inventory(source, generation_id, cursor.as_ref(), registry, &execution)?;
        for item in page.items {
            match item {
                InventoryItem::Node(node) => nodes.push(node),
                InventoryItem::Edge(edge) => edges.push(edge),
            }
        }
        cursor = page.next_cursor;
        if cursor.is_none() {
            break;
        }
    }
    Ok((nodes, edges))
}

fn collect_format_inventory_from_worker(
    client: &mut FormatWorkerClient,
    session_id: u64,
    generation_id: babel_domain::core::GenerationId,
) -> Result<
    (
        Vec<babel_resource_graph::ResourceNode>,
        Vec<babel_resource_graph::ResourceEdge>,
    ),
    KernelError,
> {
    let mut cursor = None;
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    loop {
        let reply: InventoryPageReply = client.request(&FormatWorkerRequest::InventoryPage {
            session_id,
            generation_id: *generation_id.as_bytes(),
            cursor,
        })?;
        for item in reply.page.items {
            match item {
                InventoryItem::Node(node) => nodes.push(node),
                InventoryItem::Edge(edge) => edges.push(edge),
            }
        }
        cursor = reply.page.next_cursor;
        if cursor.is_none() {
            break;
        }
    }
    Ok((nodes, edges))
}

fn collect_format_units_from_worker(
    client: &mut FormatWorkerClient,
    session_id: u64,
    generation_id: babel_domain::core::GenerationId,
    resource_id: babel_domain::core::ResourceId,
) -> Result<Vec<ExtractedUnit>, KernelError> {
    let mut cursor = None;
    let mut units = Vec::new();
    loop {
        let reply: ExtractPageReply = client.request(&FormatWorkerRequest::ExtractPage {
            session_id,
            generation_id: *generation_id.as_bytes(),
            resource_id: *resource_id.as_bytes(),
            cursor,
        })?;
        for unit in &reply.page.items {
            unit.content
                .validate()
                .map_err(|error| AdapterError::InvalidInput(error.to_string()))?;
        }
        units.extend(reply.page.items);
        cursor = reply.page.next_cursor;
        if cursor.is_none() {
            break;
        }
    }
    Ok(units)
}

struct FormatWorkerClient {
    format: FormatKind,
    worker: ProcessWorker,
    cancel: WorkerCancelToken,
    next_request_id: u64,
}

impl FormatWorkerClient {
    fn spawn(format: FormatKind) -> Result<Self, KernelError> {
        let cancel = WorkerCancelToken::new();
        let mut launch = WorkerLaunch::new(
            format_worker_binary(format),
            format.worker_capability().to_vec(),
        );
        launch.handshake_timeout = Duration::from_secs(5);
        launch.request_timeout = FORMAT_WORKER_REQUEST_TIMEOUT;
        launch.max_response_bytes = MAX_FRAME_BYTES;
        Ok(Self {
            format,
            worker: ProcessWorker::spawn(launch, &cancel)?,
            cancel,
            next_request_id: 1,
        })
    }

    fn load_object(
        &mut self,
        path: &Path,
        source_hash: [u8; 32],
        byte_length: u64,
    ) -> Result<u64, KernelError> {
        let begin: LoadBeginReply = self.request(&FormatWorkerRequest::LoadBegin {
            source_hash_hex: hex::encode(source_hash),
            byte_length,
        })?;
        let chunk_limit = begin.max_chunk_bytes.min(FORMAT_WORKER_CHUNK_BYTES);
        if chunk_limit == 0 {
            return Err(KernelError::WorkerDiagnostic(format!(
                "{} worker returned zero chunk size",
                self.format.label()
            )));
        }
        let mut file = File::open(path)?;
        let mut offset = 0_u64;
        let mut buffer = vec![0_u8; chunk_limit];
        loop {
            let read = file.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            let reply: LoadChunkReply = self.request(&FormatWorkerRequest::LoadChunk {
                session_id: begin.session_id,
                offset,
                data_hex: hex::encode(&buffer[..read]),
            })?;
            offset = offset.checked_add(read as u64).ok_or_else(|| {
                KernelError::WorkerDiagnostic(format!(
                    "{} load offset overflow",
                    self.format.label()
                ))
            })?;
            if reply.received_bytes != offset {
                return Err(KernelError::WorkerDiagnostic(format!(
                    "{} worker acknowledged {} bytes, expected {}",
                    self.format.label(),
                    reply.received_bytes,
                    offset
                )));
            }
        }
        let finish: LoadFinishReply = self.request(&FormatWorkerRequest::LoadFinish {
            session_id: begin.session_id,
        })?;
        if finish.byte_length != byte_length {
            return Err(KernelError::WorkerDiagnostic(format!(
                "{} worker loaded {} bytes, expected {}",
                self.format.label(),
                finish.byte_length,
                byte_length
            )));
        }
        Ok(begin.session_id)
    }

    fn request<T: serde::de::DeserializeOwned>(
        &mut self,
        request: &FormatWorkerRequest,
    ) -> Result<T, KernelError> {
        let request_id = self.next_request_id;
        self.next_request_id = self.next_request_id.checked_add(1).ok_or_else(|| {
            KernelError::WorkerDiagnostic(format!(
                "{} worker request id overflow",
                self.format.label()
            ))
        })?;
        let payload = serde_json::to_vec(request).map_err(|error| {
            KernelError::WorkerDiagnostic(format!(
                "encode {} worker request failed: {error}",
                self.format.label()
            ))
        })?;
        if payload.len() > MAX_FRAME_BYTES {
            return Err(KernelError::WorkerDiagnostic(format!(
                "{} worker request payload {} exceeds frame limit {}",
                self.format.label(),
                payload.len(),
                MAX_FRAME_BYTES
            )));
        }
        let response = self.worker.request(request_id, payload, &self.cancel)?;
        if response.status != 0 {
            return Err(KernelError::WorkerDiagnostic(response.diagnostic));
        }
        serde_json::from_slice(&response.payload).map_err(|error| {
            KernelError::WorkerDiagnostic(format!(
                "decode {} worker response failed: {error}",
                self.format.label()
            ))
        })
    }
}

fn format_worker_binary(format: FormatKind) -> PathBuf {
    if let Some(path) = std::env::var_os(format.worker_env()) {
        return PathBuf::from(path);
    }
    let binary_name = format.worker_binary();
    let Ok(current) = std::env::current_exe() else {
        return PathBuf::from(binary_name);
    };
    let Some(parent) = current.parent() else {
        return PathBuf::from(binary_name);
    };
    let direct = parent.join(binary_name);
    if direct.exists() {
        return direct;
    }
    parent
        .parent()
        .map(|debug_dir| debug_dir.join(binary_name))
        .unwrap_or(direct)
}

fn format_budget(source_bytes: u64) -> TaskBudget {
    TaskBudget {
        timeout_ms: 30_000,
        maximum_bytes: (source_bytes.saturating_mul(4)).max(FORMAT_PIPELINE_PAGE_BYTES),
        maximum_nodes: 1_000_000,
        page_bytes: FORMAT_PIPELINE_PAGE_BYTES,
        page_nodes: FORMAT_PIPELINE_PAGE_NODES,
    }
}

fn cas_path(object_root: &Path, hash: &[u8; 32]) -> PathBuf {
    let encoded = hex::encode(hash);
    object_root
        .join("sha256")
        .join(&encoded[..2])
        .join(&encoded[2..])
}

fn now_millis() -> Result<i64, std::time::SystemTimeError> {
    Ok(SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)?
        .as_millis() as i64)
}

fn hash_parts(parts: &[&[u8]]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update((part.len() as u64).to_be_bytes());
        hasher.update(part);
    }
    hasher.finalize().into()
}

#[cfg(test)]
mod tests {
    use std::{process::Command as ProcessCommand, sync::Arc, sync::Once};

    use rusqlite::Connection;
    use sha2::{Digest, Sha256};
    use tempfile::TempDir;

    use super::*;

    static TXT_WORKER_BUILD: Once = Once::new();
    static MARKDOWN_WORKER_BUILD: Once = Once::new();

    fn ensure_txt_worker_binary() {
        TXT_WORKER_BUILD.call_once(|| {
            let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_owned());
            let status = ProcessCommand::new(cargo)
                .args(["build", "--quiet", "-p", "babel-txt-worker"])
                .status()
                .expect("build babel-txt-worker");
            assert!(status.success(), "babel-txt-worker build failed: {status}");
        });
    }

    fn ensure_markdown_worker_binary() {
        MARKDOWN_WORKER_BUILD.call_once(|| {
            let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_owned());
            let status = ProcessCommand::new(cargo)
                .args(["build", "--quiet", "-p", "babel-markdown-worker"])
                .status()
                .expect("build babel-markdown-worker");
            assert!(
                status.success(),
                "babel-markdown-worker build failed: {status}"
            );
        });
    }

    #[test]
    fn background_work_cannot_be_starved_by_interactive_writes() {
        assert_eq!(
            choose_ready_queue(true, true, MAX_INTERACTIVE_BURST - 1),
            ReadyQueue::Interactive
        );
        assert_eq!(
            choose_ready_queue(true, true, MAX_INTERACTIVE_BURST),
            ReadyQueue::Background
        );
        assert_eq!(choose_ready_queue(false, true, 0), ReadyQueue::Background);
    }

    fn project_with_unit() -> (TempDir, [u8; 32]) {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("book.babel");
        fs::create_dir_all(&root).unwrap();
        let mut store = ProjectStore::open(root.join("project.sqlite3")).unwrap();
        store.seed_units(1).unwrap();
        let source_key = store.page_after(-1, 1).unwrap()[0]
            .source_unit_key
            .clone()
            .try_into()
            .unwrap();
        (temp, source_key)
    }

    #[test]
    fn concurrent_command_retries_create_one_revision_and_one_event() {
        let (temp, source_key) = project_with_unit();
        let kernel = Arc::new(Kernel::open(temp.path().join("book.babel")).unwrap());
        let events = kernel.subscribe(8);
        let mut callers = Vec::new();
        for _ in 0..16 {
            let kernel = Arc::clone(&kernel);
            callers.push(thread::spawn(move || {
                kernel
                    .save_translation(source_key, [9; 32], "译文".to_owned(), 1_000)
                    .unwrap()
            }));
        }
        let receipts: Vec<_> = callers
            .into_iter()
            .map(|call| call.join().unwrap())
            .collect();
        assert!(receipts.iter().all(|receipt| revision_id(receipt) == 1));
        assert!(matches!(
            events.recv_timeout(Duration::from_secs(1)).unwrap(),
            CommitEvent::TranslationCommitted {
                commit_sequence: 1,
                ..
            }
        ));
        assert!(events.try_recv().is_err());
    }

    #[test]
    fn source_is_published_before_the_database_reference() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("book.babel");
        let kernel = Kernel::open(&root).unwrap();
        let bytes = b"original source".to_vec();
        let published = kernel
            .publish_source([4; 16], "text/plain".to_owned(), bytes.clone(), 1_000)
            .unwrap();
        assert_eq!(fs::read(&published.path).unwrap(), bytes);
        let expected: [u8; 32] = Sha256::digest(b"original source").into();
        assert_eq!(published.hash, expected);

        let connection = Connection::open(kernel.database_path()).unwrap();
        let references: i64 = connection
            .query_row(
                "SELECT count(*) FROM object_reference WHERE object_hash = ?1",
                [published.hash.as_slice()],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(references, 1);
    }

    #[test]
    fn publishing_a_source_never_modifies_the_original_path() {
        let temp = TempDir::new().unwrap();
        let original = temp.path().join("original.txt");
        fs::write(&original, b"author copy").unwrap();
        let before = fs::metadata(&original).unwrap();
        let bytes = fs::read(&original).unwrap();
        let kernel = Kernel::open(temp.path().join("book.babel")).unwrap();
        kernel
            .publish_source([5; 16], "text/plain".to_owned(), bytes, 1)
            .unwrap();
        let after = fs::metadata(&original).unwrap();
        assert_eq!(fs::read(&original).unwrap(), b"author copy");
        assert_eq!(before.len(), after.len());
        assert_eq!(
            before.permissions().readonly(),
            after.permissions().readonly()
        );
        assert_eq!(before.modified().unwrap(), after.modified().unwrap());
    }

    #[test]
    fn task_transitions_are_explicit_and_terminal_states_stay_terminal() {
        let temp = TempDir::new().unwrap();
        let kernel = Kernel::open(temp.path().join("book.babel")).unwrap();
        let task_id = TaskId::new();
        kernel
            .create_task(task_id, "import".to_owned(), WorkPriority::P3Background, 1)
            .unwrap();
        kernel
            .transition_task(task_id, TaskState::Running, None, 2)
            .unwrap();
        kernel
            .transition_task(task_id, TaskState::Completed, None, 3)
            .unwrap();
        assert!(
            kernel
                .transition_task(task_id, TaskState::Running, None, 4)
                .is_err()
        );
    }

    #[test]
    fn undo_is_a_new_revision_and_rejects_a_stale_head() {
        let (temp, source_key) = project_with_unit();
        let kernel = Kernel::open(temp.path().join("book.babel")).unwrap();
        let first = kernel
            .save_translation(source_key, [1; 32], "第一版".to_owned(), 1)
            .unwrap();
        let second = kernel
            .save_translation(source_key, [2; 32], "第二版".to_owned(), 2)
            .unwrap();
        let undo = kernel
            .restore_translation(
                source_key,
                [3; 32],
                second.revision_id,
                first.revision_id,
                RevisionKind::Undo,
                3,
            )
            .unwrap();
        assert_eq!(undo.commit_sequence, 3);
        assert!(
            kernel
                .restore_translation(
                    source_key,
                    [4; 32],
                    second.revision_id,
                    first.revision_id,
                    RevisionKind::Undo,
                    4,
                )
                .is_err()
        );
        assert_eq!(
            kernel.query().unwrap().page_after(-1, 1).unwrap()[0]
                .translation
                .as_deref(),
            Some("第一版")
        );
    }

    #[test]
    fn draft_survives_but_never_overwrites_a_changed_durable_base() {
        let (temp, source_key) = project_with_unit();
        let kernel = Kernel::open(temp.path().join("book.babel")).unwrap();
        let first = kernel
            .save_translation(source_key, [1; 32], "已确认一".to_owned(), 1)
            .unwrap();
        let unit_id = kernel.query().unwrap().page_after(-1, 1).unwrap()[0]
            .unit_id
            .clone();
        kernel
            .save_draft(
                unit_id.clone(),
                Some(first.revision_id),
                "window-1".to_owned(),
                b"local patch".to_vec(),
                2,
            )
            .unwrap();
        kernel
            .save_translation(source_key, [2; 32], "已确认二".to_owned(), 3)
            .unwrap();
        let draft = kernel
            .query()
            .unwrap()
            .draft_for(&unit_id, "window-1")
            .unwrap()
            .unwrap();
        assert_eq!(
            draft.disposition,
            babel_storage::project::DraftDisposition::BaseChanged
        );
        assert_eq!(draft.patch, b"local patch");
        assert_eq!(kernel.query().unwrap().commit_sequence().unwrap(), 2);
    }

    #[test]
    fn running_task_is_paused_when_the_kernel_recovers() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("book.babel");
        let task_id = TaskId::new();
        {
            let kernel = Kernel::open(&root).unwrap();
            kernel
                .create_task(task_id, "index".to_owned(), WorkPriority::P3Background, 1)
                .unwrap();
            kernel
                .transition_task(task_id, TaskState::Running, None, 2)
                .unwrap();
        }
        let recovered = Kernel::open(&root).unwrap();
        drop(recovered);
        let store = ProjectStore::open(root.join("project.sqlite3")).unwrap();
        let task = store.task(task_id).unwrap();
        assert_eq!(task.state, TaskState::Paused);
        assert_eq!(task.failure_code.as_deref(), Some("interrupted"));
    }

    #[test]
    fn backup_is_a_verified_snapshot_and_does_not_replace_an_existing_target() {
        let (temp, source_key) = project_with_unit();
        let root = temp.path().join("book.babel");
        let kernel = Kernel::open(&root).unwrap();
        kernel
            .save_translation(source_key, [7; 32], "已确认".to_owned(), 1)
            .unwrap();
        kernel
            .publish_source([3; 16], "text/plain".to_owned(), b"source".to_vec(), 2)
            .unwrap();
        let target = temp.path().join("backup.babel");
        let sequence = kernel.backup_to(&target, 3).unwrap();
        assert_eq!(sequence, 1);

        let backup = ProjectStore::open(target.join("project.sqlite3")).unwrap();
        assert_eq!(backup.commit_sequence().unwrap(), 1);
        assert_eq!(
            backup.page_after(-1, 1).unwrap()[0].translation.as_deref(),
            Some("已确认")
        );
        assert!(kernel.backup_to(&target, 4).is_err());
    }

    #[test]
    fn only_one_authoritative_writer_can_open_a_project() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("book.babel");
        let first = Kernel::open(&root).unwrap();
        assert!(matches!(
            Kernel::open(&root),
            Err(KernelError::ProjectAlreadyOpen)
        ));
        drop(first);
        assert!(Kernel::open(&root).is_ok());
    }

    #[test]
    fn diagnostics_are_local_records_exposed_through_read_only_projection() {
        let temp = TempDir::new().unwrap();
        let kernel = Kernel::open(temp.path().join("book.babel")).unwrap();
        let event_id = kernel
            .record_diagnostic(
                "Warning".to_owned(),
                "SOURCE_ENCODING_UNCERTAIN".to_owned(),
                "无法可靠判断文本编码".to_owned(),
                Some("confidence=0.42".to_owned()),
                1,
            )
            .unwrap();
        assert_eq!(event_id, 1);
        assert_eq!(kernel.query().unwrap().diagnostic_count().unwrap(), 1);
    }

    #[test]
    fn bounded_subscription_reports_lag_instead_of_growing_without_limit() {
        let temp = TempDir::new().unwrap();
        let kernel = Kernel::open(temp.path().join("book.babel")).unwrap();
        let events = kernel.subscribe(1);
        kernel
            .create_task(
                TaskId::new(),
                "first".to_owned(),
                WorkPriority::P3Background,
                1,
            )
            .unwrap();
        kernel
            .create_task(
                TaskId::new(),
                "second".to_owned(),
                WorkPriority::P3Background,
                2,
            )
            .unwrap();
        assert!(events.take_lagged());
        assert!(matches!(
            events.try_recv(),
            Ok(CommitEvent::TaskChanged { .. })
        ));
        assert!(!events.take_lagged());
    }

    fn revision_id(receipt: &SaveReceipt) -> i64 {
        receipt.revision_id
    }

    #[test]
    fn txt_pipeline_imports_saves_validates_and_exports_frozen_snapshot() {
        let temp = TempDir::new().unwrap();
        let objects = temp.path().join("objects");
        let staging = temp.path().join("staging");
        let mut store = ProjectStore::open(temp.path().join("project.sqlite3")).unwrap();
        let report = import_txt_bytes(
            &mut store,
            &objects,
            &staging,
            b"\xef\xbb\xbfone\r\ntwo\nthree",
            1_000,
        )
        .unwrap();
        assert_eq!(report.units, 3);
        let units = store.page_after(-1, 10).unwrap();
        assert_eq!(units.len(), 3);
        assert_eq!(
            validate_txt_export(
                &store,
                &report.generation_id,
                store.commit_sequence().unwrap()
            )
            .unwrap()
            .len(),
            3
        );
        for (index, unit) in units.iter().enumerate() {
            let source_key: [u8; 32] = unit.source_unit_key.clone().try_into().unwrap();
            store
                .save_translation(
                    &source_key,
                    &hash_parts(&[b"test-command", &(index as u64).to_be_bytes()]),
                    &format!("T{index}"),
                    2_000 + index as i64,
                )
                .unwrap();
        }
        let frozen = store.commit_sequence().unwrap();
        let export =
            export_txt_bytes(&store, &objects, &staging, &report.generation_id, frozen).unwrap();
        assert_eq!(export.bytes, b"\xef\xbb\xbfT0\r\nT1\nT2");

        let first_key: [u8; 32] = units[0].source_unit_key.clone().try_into().unwrap();
        store
            .save_translation(&first_key, &[9; 32], "later edit", 3_000)
            .unwrap();
        let export =
            export_txt_bytes(&store, &objects, &staging, &report.generation_id, frozen).unwrap();
        assert_eq!(export.bytes, b"\xef\xbb\xbfT0\r\nT1\nT2");
    }

    #[test]
    fn kernel_txt_api_imports_validates_and_exports() {
        ensure_txt_worker_binary();
        let temp = TempDir::new().unwrap();
        let kernel = Kernel::open(temp.path().join("book.babel")).unwrap();
        let report = kernel
            .import_txt_reader([8; 16], Cursor::new(b"one\ntwo\n".to_vec()), 1_000)
            .unwrap();
        assert_eq!(report.units, 2);
        assert!(report.activated);
        assert_eq!(report.review_required, 0);
        assert_eq!(kernel.validate_active_txt().unwrap().len(), 2);
        let query = kernel.query().unwrap();
        let units = query.page_after(-1, 10).unwrap();
        drop(query);
        for (index, unit) in units.iter().enumerate() {
            let source_key: [u8; 32] = unit.source_unit_key.clone().try_into().unwrap();
            kernel
                .save_translation(
                    source_key,
                    hash_parts(&[b"kernel-txt", &(index as u64).to_be_bytes()]),
                    format!("T{index}"),
                    2_000 + index as i64,
                )
                .unwrap();
        }
        assert!(kernel.validate_active_txt().unwrap().is_empty());
        let export = kernel.export_active_txt().unwrap();
        assert_eq!(export.bytes, b"T0\nT1\n");
    }

    #[test]
    fn kernel_markdown_api_preserves_structure_through_isolated_worker() {
        ensure_markdown_worker_binary();
        let temp = TempDir::new().unwrap();
        let kernel = Kernel::open(temp.path().join("book.babel")).unwrap();
        let source = b"# Title\n\nHello **world** and [site](https://example.test).\n";
        let report = kernel
            .import_markdown_reader([9; 16], Cursor::new(source.to_vec()), 1_000)
            .unwrap();
        assert!(report.activated);
        assert!(report.units >= 4);

        let units = kernel.query().unwrap().page_after(-1, 100).unwrap();
        for (index, unit) in units.iter().enumerate() {
            let translation = match unit.source_text.as_str() {
                "Title" => "标题".to_owned(),
                "Hello " => "你好 ".to_owned(),
                "world" => "世界".to_owned(),
                " and " => " 和 ".to_owned(),
                "site" => "站点".to_owned(),
                other => other.to_owned(),
            };
            kernel
                .save_translation(
                    unit.source_unit_key.clone().try_into().unwrap(),
                    hash_parts(&[b"kernel-markdown", &(index as u64).to_be_bytes()]),
                    translation,
                    2_000 + index as i64,
                )
                .unwrap();
        }

        assert!(kernel.validate_active_markdown().unwrap().is_empty());
        let export = kernel.export_active_markdown().unwrap();
        assert_eq!(
            export.bytes,
            "# 标题\n\n你好 **世界** 和 [站点](https://example.test).\n".as_bytes()
        );
    }

    #[test]
    fn translation_aids_share_the_authoritative_writer_and_batch_preconditions() {
        ensure_txt_worker_binary();
        let temp = TempDir::new().unwrap();
        let kernel = Kernel::open(temp.path().join("book.babel")).unwrap();
        kernel
            .import_txt_reader([6; 16], Cursor::new(b"repeat\nrepeat\n".to_vec()), 1)
            .unwrap();
        let units = kernel.query().unwrap().page_after(-1, 10).unwrap();
        let mut receipts = Vec::new();
        for (index, unit) in units.iter().enumerate() {
            receipts.push(
                kernel
                    .save_translation(
                        unit.source_unit_key.clone().try_into().unwrap(),
                        hash_parts(&[b"aid-save", &(index as u64).to_be_bytes()]),
                        "hello old".to_owned(),
                        10 + index as i64,
                    )
                    .unwrap(),
            );
        }

        let term = UpsertTermRequest {
            term_id: [1; 16],
            source_text: "repeat".to_owned(),
            preferred_translation: "重复".to_owned(),
            variants: vec!["Repeat".to_owned()],
            notes: "fixture".to_owned(),
            state: "Active".to_owned(),
            timestamp_ms: 20,
        };
        kernel.upsert_term(term.clone()).unwrap();
        kernel.upsert_term(term).unwrap();
        assert_eq!(kernel.find_terms("Repeat".to_owned(), 10).unwrap().len(), 1);

        kernel
            .add_annotation(AddAnnotationRequest {
                annotation_id: [2; 16],
                unit_id: units[0].unit_id.clone(),
                base_revision_id: Some(receipts[0].revision_id),
                grapheme_start: 0,
                grapheme_end: 5,
                body: "check wording".to_owned(),
                created_at_ms: 21,
            })
            .unwrap();
        kernel
            .set_marker(
                [3; 16],
                units[0].unit_id.clone(),
                "review".to_owned(),
                "needs-check".to_owned(),
                22,
            )
            .unwrap();
        kernel
            .set_marker(
                [4; 16],
                units[0].unit_id.clone(),
                "review".to_owned(),
                "needs-check".to_owned(),
                23,
            )
            .unwrap();
        assert_eq!(
            kernel
                .markers_for_unit(units[0].unit_id.clone())
                .unwrap()
                .len(),
            1
        );
        assert_eq!(kernel.duplicate_source_groups(2, 10).unwrap().len(), 1);
        assert_eq!(
            kernel
                .translation_history("repeat".to_owned(), 10)
                .unwrap()
                .len(),
            2
        );

        let preview = kernel
            .preview_replace_translations("hello".to_owned(), "你好".to_owned(), 10)
            .unwrap();
        assert_eq!(preview.len(), 2);
        let receipt = kernel
            .apply_replace_translations(
                [5; 32],
                "hello".to_owned(),
                "你好".to_owned(),
                preview.clone(),
                30,
            )
            .unwrap();
        assert_eq!(receipt.affected_units, 2);
        assert!(!receipt.replayed);
        assert!(
            kernel
                .apply_replace_translations(
                    [5; 32],
                    "hello".to_owned(),
                    "你好".to_owned(),
                    preview,
                    30,
                )
                .unwrap()
                .replayed
        );
        let translated = kernel.query().unwrap().page_after(-1, 10).unwrap();
        assert!(
            translated
                .iter()
                .all(|unit| unit.translation.as_deref() == Some("你好 old"))
        );

        kernel
            .save_translation(
                units[0].source_unit_key.clone().try_into().unwrap(),
                [6; 32],
                "new head".to_owned(),
                40,
            )
            .unwrap();
        assert!(
            kernel
                .annotations_for_unit(units[0].unit_id.clone())
                .unwrap()[0]
                .stale
        );
    }

    #[test]
    fn kernel_txt_import_uses_bounded_generation_transactions() {
        ensure_txt_worker_binary();
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("book.babel");
        let mut source = String::new();
        for index in 0..2_501 {
            source.push_str(&format!("line {index}\n"));
        }
        let kernel = Kernel::open(&root).unwrap();
        let report = kernel
            .import_txt_reader([7; 16], Cursor::new(source.into_bytes()), 1_000)
            .unwrap();
        assert_eq!(report.units, 2_501);
        drop(kernel);

        let connection = Connection::open(root.join("project.sqlite3")).unwrap();
        let generation_batches: i64 = connection
            .query_row(
                "SELECT count(*) FROM generation_batch_receipt WHERE generation_id = ?1",
                [report.generation_id.as_slice()],
                |row| row.get(0),
            )
            .unwrap();
        let largest_unit_batch: i64 = connection
            .query_row(
                "SELECT max(item_count) FROM generation_batch_receipt
                 WHERE generation_id = ?1 AND item_count > 3",
                [report.generation_id.as_slice()],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(
            generation_batches,
            1 + i64::try_from(report.units.div_ceil(FORMAT_WRITER_BATCH_ITEMS)).unwrap()
        );
        assert!(largest_unit_batch <= 2 * FORMAT_WRITER_BATCH_ITEMS as i64);
        assert!(largest_unit_batch >= 2);
    }

    #[test]
    fn txt_reimport_keeps_old_generation_active_until_shifted_units_are_reviewed() {
        ensure_txt_worker_binary();
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("book.babel");
        let kernel = Kernel::open(&root).unwrap();
        kernel
            .import_txt_reader([1; 16], Cursor::new(b"one\ntwo\n".to_vec()), 1)
            .unwrap();
        let original = kernel.query().unwrap().page_after(-1, 10).unwrap();
        let second_key: [u8; 32] = original[1].source_unit_key.clone().try_into().unwrap();
        kernel
            .save_translation(second_key, [2; 32], "TWO".to_owned(), 2)
            .unwrap();

        let reimport = kernel
            .import_txt_reader([3; 16], Cursor::new(b"zero\none\ntwo\n".to_vec()), 3)
            .unwrap();
        assert!(!reimport.activated);
        assert_eq!(reimport.review_required, 2);
        assert_eq!(kernel.query().unwrap().page_after(-1, 10).unwrap().len(), 2);

        let reviews = kernel.pending_txt_bindings(reimport.generation_id).unwrap();
        assert_eq!(reviews.len(), 2);
        assert!(
            reviews
                .iter()
                .all(|review| { review.disposition == "Shifted" && review.candidates.len() == 1 })
        );
        for (index, review) in reviews.into_iter().enumerate() {
            kernel
                .decide_txt_binding(
                    review.binding_id,
                    review.candidates[0],
                    review.candidates_hash,
                    hash_parts(&[b"review", &(index as u64).to_be_bytes()]),
                    "confirmed-move".to_owned(),
                    4 + index as i64,
                )
                .unwrap();
        }
        assert!(
            kernel
                .pending_txt_bindings(reimport.generation_id)
                .unwrap()
                .is_empty()
        );
        kernel
            .activate_txt_import(reimport.generation_id, 10)
            .unwrap();

        let imported = kernel.query().unwrap().page_after(-1, 10).unwrap();
        assert_eq!(imported.len(), 3);
        assert_eq!(imported[2].translation.as_deref(), Some("TWO"));
    }

    #[test]
    fn duplicate_new_units_compete_for_old_translation_until_one_is_rejected_as_new() {
        ensure_txt_worker_binary();
        let temp = TempDir::new().unwrap();
        let kernel = Kernel::open(temp.path().join("book.babel")).unwrap();
        kernel
            .import_txt_reader([1; 16], Cursor::new(b"A\n".to_vec()), 1)
            .unwrap();
        let original = kernel.query().unwrap().page_after(-1, 1).unwrap();
        let source_key: [u8; 32] = original[0].source_unit_key.clone().try_into().unwrap();
        kernel
            .save_translation(source_key, [2; 32], "OLD-A".to_owned(), 2)
            .unwrap();

        let reimport = kernel
            .import_txt_reader([3; 16], Cursor::new(b"X\nA\nA\n".to_vec()), 3)
            .unwrap();
        assert_eq!(reimport.review_required, 2);
        let reviews = kernel.pending_txt_bindings(reimport.generation_id).unwrap();
        assert_eq!(reviews.len(), 2);
        assert!(reviews.iter().all(|review| {
            review.disposition == "Shifted"
                && review.candidates.len() == 1
                && review.candidates[0] == reviews[0].candidates[0]
        }));

        kernel
            .decide_txt_binding(
                reviews[0].binding_id,
                reviews[0].candidates[0],
                reviews[0].candidates_hash,
                [4; 32],
                "inherit-old-translation".to_owned(),
                4,
            )
            .unwrap();
        kernel
            .reject_txt_binding_as_new(
                reviews[1].binding_id,
                reviews[1].candidates_hash,
                [5; 32],
                "duplicate-is-new-unit".to_owned(),
                5,
            )
            .unwrap();
        kernel
            .reject_txt_binding_as_new(
                reviews[1].binding_id,
                reviews[1].candidates_hash,
                [5; 32],
                "duplicate-is-new-unit".to_owned(),
                5,
            )
            .unwrap();
        kernel
            .activate_txt_import(reimport.generation_id, 6)
            .unwrap();

        let imported = kernel.query().unwrap().page_after(-1, 10).unwrap();
        assert_eq!(imported.len(), 3);
        assert_eq!(
            imported
                .iter()
                .filter(|unit| unit.translation.as_deref() == Some("OLD-A"))
                .count(),
            1
        );
    }

    #[test]
    fn kernel_markdown_api_imports_saves_validates_and_exports_frozen_snapshot() {
        ensure_markdown_worker_binary();
        let temp = TempDir::new().unwrap();
        let kernel = Kernel::open(temp.path().join("book.babel")).unwrap();
        let report = kernel
            .import_markdown_reader(
                [8; 16],
                Cursor::new(b"# Title\n\nHello **world**.".to_vec()),
                1_000,
            )
            .unwrap();
        assert!(report.activated);
        assert_eq!(
            kernel.validate_active_markdown().unwrap().len(),
            report.units
        );

        let query = kernel.query().unwrap();
        let units = query.page_after(-1, 10).unwrap();
        drop(query);
        assert_eq!(units.len(), report.units);
        for unit in &units {
            let translation = match unit.source_text.as_str() {
                "Title" => "标题",
                "Hello " => "你好 ",
                "world" => "世界",
                "." => "。",
                other => panic!("unexpected Markdown source unit: {other:?}"),
            };
            let source_key: [u8; 32] = unit.source_unit_key.clone().try_into().unwrap();
            kernel
                .save_translation(
                    source_key,
                    hash_parts(&[b"kernel-markdown", unit.source_text.as_bytes()]),
                    translation.to_owned(),
                    2_000,
                )
                .unwrap();
        }
        let frozen = kernel.query().unwrap().commit_sequence().unwrap();
        assert!(kernel.validate_active_markdown().unwrap().is_empty());
        let export = kernel.export_active_markdown().unwrap();
        assert_eq!(export.frozen_commit_sequence, frozen);
        assert_eq!(export.bytes, "# 标题\n\n你好 **世界**。".as_bytes());

        let title_key: [u8; 32] = units
            .iter()
            .find(|unit| unit.source_text == "Title")
            .unwrap()
            .source_unit_key
            .clone()
            .try_into()
            .unwrap();
        kernel
            .save_translation(title_key, [9; 32], "later".to_owned(), 3_000)
            .unwrap();
        assert_eq!(
            kernel.export_active_markdown().unwrap().bytes,
            "# later\n\n你好 **世界**。".as_bytes()
        );
    }

    #[test]
    fn markdown_reimport_uses_generic_binding_review_before_activation() {
        ensure_markdown_worker_binary();
        let temp = TempDir::new().unwrap();
        let kernel = Kernel::open(temp.path().join("book.babel")).unwrap();
        kernel
            .import_markdown_reader([1; 16], Cursor::new(b"# Title\n\nAlpha".to_vec()), 1)
            .unwrap();
        let original = kernel.query().unwrap().page_after(-1, 10).unwrap();
        let alpha_key: [u8; 32] = original
            .iter()
            .find(|unit| unit.source_text == "Alpha")
            .unwrap()
            .source_unit_key
            .clone()
            .try_into()
            .unwrap();
        kernel
            .save_translation(alpha_key, [2; 32], "阿尔法".to_owned(), 2)
            .unwrap();

        let reimport = kernel
            .import_markdown_reader(
                [3; 16],
                Cursor::new(b"Intro\n\n# Title\n\nAlpha".to_vec()),
                3,
            )
            .unwrap();
        assert!(!reimport.activated);
        assert!(reimport.review_required >= 1);
        assert_eq!(kernel.query().unwrap().page_after(-1, 10).unwrap().len(), 2);

        let reviews = kernel
            .pending_markdown_bindings(reimport.generation_id)
            .unwrap();
        assert_eq!(reviews.len(), reimport.review_required);
        assert!(
            reviews
                .iter()
                .all(|review| review.disposition == "Shifted" && review.candidates.len() == 1)
        );
        for (index, review) in reviews.into_iter().enumerate() {
            kernel
                .decide_binding(
                    review.binding_id,
                    review.candidates[0],
                    review.candidates_hash,
                    hash_parts(&[b"markdown-review", &(index as u64).to_be_bytes()]),
                    "confirmed-markdown-shift".to_owned(),
                    4 + index as i64,
                )
                .unwrap();
        }
        kernel
            .activate_markdown_import(reimport.generation_id, 10)
            .unwrap();

        let imported = kernel.query().unwrap().page_after(-1, 10).unwrap();
        assert_eq!(imported.len(), 3);
        assert_eq!(
            imported
                .iter()
                .find(|unit| unit.source_text == "Alpha")
                .unwrap()
                .translation
                .as_deref(),
            Some("阿尔法")
        );
    }
}
