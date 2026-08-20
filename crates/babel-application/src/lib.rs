//! Application boundary for the authoritative project writer.

use std::{
    collections::{HashMap, HashSet},
    fs::{self, File, OpenOptions},
    io::{Cursor, Read, Write},
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
    Adapter, AdapterError, CancellationToken, CapabilityIo, Cursor as AdapterCursor,
    ExecutionContext, ExtractedUnit, ImageOverlay, InventoryItem, OverlayUnit, Page, TaskBudget,
};
use babel_domain::{
    core::{ProjectId, ResourceId, RevisionKind, TaskId, TaskState, UnitId, WorkPriority},
    workbench::{NavigationPosition, TranslationStatus, WorkspaceView},
};
use babel_epub_adapter::EpubAdapter;
use babel_markdown_adapter::MarkdownAdapter;
use babel_resource_graph::{Locator, RESOURCE_GRAPH_SCHEMA_VERSION, ResourceGraph, ResourceKind};
use babel_runtime::{
    ipc::MAX_FRAME_BYTES,
    process_worker::{ProcessWorker, WorkerCancelToken, WorkerError, WorkerLaunch},
};
pub use babel_storage::project::{
    ImageRegionEditRecord, OcrCandidateCacheRecord, SaveImageRegionEditRequest,
    SaveOcrCandidateRequest,
};
use babel_storage::{
    backup::{BackupError, BackupSnapshot},
    cas,
    gc::{self, GcReport},
    project::{
        AnnotationRecord, BatchReplaceReceipt, DuplicateSourceGroup, GenerationBatch,
        GenerationBindingRecord, GenerationBindingView, GenerationDescriptor, GenerationEdgeRecord,
        GenerationResourceRecord, GenerationUnitRecord, MarkerRecord, NavigationSaveReceipt,
        ObjectRecord, ProjectStore, RecordWorkspaceOperationRequest, ReplacePreviewItem,
        SaveReceipt, TaskRecord, TermRecord, TranslationHistoryItem, UpsertTermRequest,
        WorkspaceOperationState, FinishWorkspaceOperationRequest, candidate_set_hash,
    },
    query::ProjectQuery,
};
use babel_tir::{Token, TranslationDocumentV1, UnitContent};
use babel_txt_adapter::TxtAdapter;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
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
const FORMAT_WORKER_CHUNK_BYTES: usize = 1024 * 1024;
const FORMAT_WRITER_BATCH_ITEMS: usize = 5_000;
const MAX_IMAGE_PREVIEW_BYTES: usize = 20 * 1024 * 1024;

const TXT_ADAPTER_ID: &str = "org.babel-tower.txt";
const MARKDOWN_ADAPTER_ID: &str = "org.babel-tower.markdown";
const EPUB_ADAPTER_ID: &str = "org.babel-tower.epub";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FormatKind {
    Txt,
    Markdown,
    Epub,
}

impl FormatKind {
    const fn request_timeout(self) -> Duration {
        match self {
            Self::Epub => Duration::from_secs(120),
            Self::Txt | Self::Markdown => Duration::from_secs(30),
        }
    }

    const fn label(self) -> &'static str {
        match self {
            Self::Txt => "TXT",
            Self::Markdown => "Markdown",
            Self::Epub => "EPUB",
        }
    }

    const fn format_id(self) -> &'static str {
        match self {
            Self::Txt => "txt",
            Self::Markdown => "markdown",
            Self::Epub => "epub",
        }
    }

    const fn media_type(self) -> &'static str {
        match self {
            Self::Txt => "text/plain",
            Self::Markdown => "text/markdown",
            Self::Epub => "application/epub+zip",
        }
    }

    const fn worker_env(self) -> &'static str {
        match self {
            Self::Txt => "BABEL_TXT_WORKER",
            Self::Markdown => "BABEL_MARKDOWN_WORKER",
            Self::Epub => "BABEL_EPUB_WORKER",
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
            Self::Epub => {
                if cfg!(windows) {
                    "babel-epub-worker.exe"
                } else {
                    "babel-epub-worker"
                }
            }
        }
    }

    const fn worker_capability(self) -> &'static [u8] {
        match self {
            Self::Txt => b"babel-txt-worker-v1",
            Self::Markdown => b"babel-markdown-worker-v1",
            Self::Epub => b"babel-epub-worker-v1",
        }
    }

    fn from_adapter_id(adapter_id: &str) -> Result<Self, KernelError> {
        match adapter_id {
            TXT_ADAPTER_ID => Ok(Self::Txt),
            MARKDOWN_ADAPTER_ID => Ok(Self::Markdown),
            EPUB_ADAPTER_ID => Ok(Self::Epub),
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
    ImageRegionCommitted {
        revision_id: i64,
        commit_sequence: i64,
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
    pub worker_peak_rss_kib: Option<u64>,
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
pub struct FormatFileExportReport {
    pub generation_id: [u8; 16],
    pub frozen_commit_sequence: i64,
    pub output_hash: [u8; 32],
    pub byte_length: u64,
    pub path: PathBuf,
}

pub const TRANSLATION_WORK_ITEM_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceAssociation {
    pub resource_id: ResourceId,
    pub kind: String,
    pub semantic_path: String,
    pub relation: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslationWorkItem {
    pub schema_version: u32,
    pub project_id: ProjectId,
    pub view: WorkspaceView,
    pub generation_id: [u8; 16],
    pub unit_id: UnitId,
    pub source_unit_key: [u8; 32],
    pub source: UnitContent,
    pub source_text: String,
    pub translation: Option<String>,
    pub translation_document: TranslationDocumentV1,
    pub status: TranslationStatus,
    pub locator: Locator,
    pub reading_order: u64,
    pub revision_id: Option<i64>,
    pub revision_commit_sequence: Option<i64>,
    pub project_commit_sequence: i64,
    pub resources: Vec<ResourceAssociation>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImagePreview {
    pub media_type: String,
    pub byte_length: usize,
    pub source_hash: [u8; 32],
    pub data_base64: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ResourceQueuePage {
    pub items: Vec<TranslationWorkItem>,
    pub next_cursor: Option<ResourceQueueCursor>,
    pub project_commit_sequence: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceQueueCursor {
    pub reading_order: u64,
    pub unit_id: UnitId,
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum WorkspaceMutationRequest {
    CreateFolder {
        project_id: String,
        parent_id: String,
        name: String,
    },
    Rename {
        project_id: String,
        node_id: String,
        name: String,
    },
    Move {
        project_id: String,
        node_id: String,
        parent_id: String,
    },
    Trash {
        project_id: String,
        node_id: String,
    },
    Restore {
        project_id: String,
        node_id: String,
    },
    Reveal {
        project_id: String,
        node_id: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceMutationReceipt {
    pub operation_id: String,
    pub commit_sequence: i64,
    pub affected_node_ids: Vec<String>,
}

pub type TxtImportReport = FormatImportReport;
pub type TxtBindingReview = FormatBindingReview;
pub type TxtValidationIssue = FormatValidationIssue;
pub type TxtExportReport = FormatExportReport;
pub type MarkdownImportReport = FormatImportReport;
pub type MarkdownBindingReview = FormatBindingReview;
pub type MarkdownValidationIssue = FormatValidationIssue;
pub type MarkdownExportReport = FormatExportReport;
pub type EpubImportReport = FormatImportReport;
pub type EpubBindingReview = FormatBindingReview;
pub type EpubValidationIssue = FormatValidationIssue;
pub type EpubExportReport = FormatExportReport;
pub type EpubFileExportReport = FormatFileExportReport;

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
    worker_peak_rss_kib: Option<u64>,
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
    external_objects: Vec<ObjectRecord>,
    worker_peak_rss_kib: Option<u64>,
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
        worker_peak_rss_kib: None,
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
    let text_resources = nodes
        .iter()
        .filter(|node| node.kind == ResourceKind::TextStream)
        .map(|node| node.resource_id)
        .collect::<Vec<_>>();
    if text_resources.is_empty() {
        return Err(AdapterError::InvalidInput(format!(
            "{} inventory has no text stream",
            format.label()
        ))
        .into());
    }
    let mut units = Vec::new();
    let mut worker_peak_rss_kib = None;
    for text_resource in text_resources {
        let (resource_units, resource_peak_rss_kib) = collect_format_units_from_worker(
            &mut client,
            session_id,
            generation_id,
            text_resource,
        )?;
        units.extend(resource_units);
        worker_peak_rss_kib = worker_peak_rss_kib.max(resource_peak_rss_kib);
    }

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
        external_objects: Vec::new(),
        worker_peak_rss_kib,
    })
}

fn attach_markdown_assets(
    prepared: &mut PreparedFormatImport,
    source_path: &Path,
    object_root: impl AsRef<Path>,
    created_at_ms: i64,
) -> Result<(), KernelError> {
    let parent = source_path.parent().unwrap_or_else(|| Path::new("."));
    let object_root = object_root.as_ref();
    for node in &mut prepared.nodes {
        if node.kind != ResourceKind::Image {
            continue;
        }
        let Locator::StructuralPath { path_segments, .. } = &node.locator else {
            continue;
        };
        let Some(url) = path_segments.last() else {
            continue;
        };
        let relative = Path::new(url);
        if relative.is_absolute()
            || relative.components().any(|component| {
                matches!(
                    component,
                    std::path::Component::ParentDir | std::path::Component::RootDir
                )
            })
            || url.contains("://")
            || url.starts_with('#')
        {
            continue;
        }
        let path = parent.join(relative);
        if !path.is_file() {
            continue;
        }
        let bytes = fs::read(&path)?;
        let (hash, _, byte_length) = cas::publish_reader(object_root, bytes.as_slice())?;
        let media_type = image::guess_format(bytes.as_slice())
            .ok()
            .map(|format| match format {
                image::ImageFormat::Png => "image/png",
                image::ImageFormat::Jpeg => "image/jpeg",
                image::ImageFormat::WebP => "image/webp",
                image::ImageFormat::Gif => "image/gif",
                _ => "application/octet-stream",
            })
            .unwrap_or("application/octet-stream")
            .to_owned();
        prepared.external_objects.push(ObjectRecord {
            hash,
            byte_length,
            media_type,
        });
        node.locator = Locator::ByteSpan {
            object_hash: hash,
            start: 0,
            end: byte_length,
        };
    }
    let _ = created_at_ms;
    Ok(())
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
        external_objects,
        worker_peak_rss_kib,
    } = prepared;
    for object in &external_objects {
        store.register_object_reference(
            "generation-resource",
            generation_id.as_bytes(),
            object,
            created_at_ms,
        )?;
    }
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
        worker_peak_rss_kib,
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
        FormatKind::Epub => Ok(Box::new(EpubAdapter::new())),
    }
}

struct MaterializedFormatExport {
    registry: CapabilityRegistry,
    staging: babel_adapter_protocol::StagingHandle,
    format: FormatKind,
    image_overlays: Vec<ImageOverlay>,
    generation_id: [u8; 16],
    frozen_commit_sequence: i64,
    output_hash: [u8; 32],
    byte_length: u64,
}

fn materialize_format_export(
    store: &ProjectStore,
    object_root: impl AsRef<Path>,
    staging_root: impl AsRef<Path>,
    generation_id: &[u8; 16],
    frozen_commit_sequence: i64,
) -> Result<MaterializedFormatExport, KernelError> {
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
    let budget = format_budget_for(format, byte_length);
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
    let image_overlays = store
        .image_export_overlays(generation_id, frozen_commit_sequence)?
        .into_iter()
        .map(|record| {
            let object = store
                .object_record(&record.derived_object_hash)
                .map_err(|error| AdapterError::InvalidInput(error.to_string()))?;
            let source_locator =
                serde_json::from_slice(&record.image_locator_json).map_err(|error| {
                    AdapterError::InvalidInput(format!("invalid image locator JSON: {error}"))
                })?;
            let region_locator =
                serde_json::from_slice(&record.region_locator_json).map_err(|error| {
                    AdapterError::InvalidInput(format!(
                        "invalid image region locator JSON: {error}"
                    ))
                })?;
            let derived_object = registry
                .grant_object(object.hash, object.byte_length)
                .map_err(|error| AdapterError::InvalidInput(error.to_string()))?;
            Ok(ImageOverlay {
                image_resource_id: babel_domain::core::ResourceId::from_bytes(
                    record.image_resource_id,
                ),
                source_locator,
                region_locator,
                derived_object,
                media_type: record.media_type,
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
    if !image_overlays.is_empty() {
        adapter.apply_image_overlays(
            &plan,
            &source,
            &image_overlays,
            &staging,
            &registry,
            &execution,
        )?;
    }
    let verification = adapter.verify_output(&staging, &registry, &execution)?;
    if !verification.valid {
        return Err(KernelError::Adapter(AdapterError::InvalidInput(
            verification.issue_codes.join(","),
        )));
    }
    Ok(MaterializedFormatExport {
        registry,
        staging,
        format,
        image_overlays,
        generation_id: *generation_id,
        frozen_commit_sequence,
        output_hash: verification.output_hash,
        byte_length: verification.byte_length,
    })
}

fn export_format_bytes(
    store: &ProjectStore,
    object_root: impl AsRef<Path>,
    staging_root: impl AsRef<Path>,
    generation_id: &[u8; 16],
    frozen_commit_sequence: i64,
) -> Result<FormatExportReport, KernelError> {
    let descriptor = store.source_snapshot_descriptor(generation_id)?;
    if FormatKind::from_adapter_id(&descriptor.adapter_id)? == FormatKind::Markdown
        && !store.markdown_image_resources(generation_id)?.is_empty()
    {
        return Err(AdapterError::InvalidInput(
            "Markdown project contains image resources; use file export to preserve the resource closure"
                .to_owned(),
        )
        .into());
    }
    let materialized = materialize_format_export(
        store,
        object_root,
        staging_root,
        generation_id,
        frozen_commit_sequence,
    )?;
    Ok(FormatExportReport {
        generation_id: materialized.generation_id,
        frozen_commit_sequence: materialized.frozen_commit_sequence,
        output_hash: materialized.output_hash,
        byte_length: materialized.byte_length,
        bytes: materialized.registry.staging_bytes(&materialized.staging)?,
    })
}

fn export_format_to_path(
    store: &ProjectStore,
    object_root: impl AsRef<Path>,
    staging_root: impl AsRef<Path>,
    generation_id: &[u8; 16],
    frozen_commit_sequence: i64,
    destination: PathBuf,
) -> Result<FormatFileExportReport, KernelError> {
    let materialized = materialize_format_export(
        store,
        object_root,
        staging_root,
        generation_id,
        frozen_commit_sequence,
    )?;
    if materialized.format == FormatKind::Markdown {
        publish_markdown_image_closure(
            store,
            generation_id,
            frozen_commit_sequence,
            &materialized,
            &destination,
        )?;
    }
    materialized
        .registry
        .publish_staging_no_clobber(&materialized.staging, &destination)?;
    Ok(FormatFileExportReport {
        generation_id: materialized.generation_id,
        frozen_commit_sequence: materialized.frozen_commit_sequence,
        output_hash: materialized.output_hash,
        byte_length: materialized.byte_length,
        path: destination,
    })
}

fn publish_markdown_image_closure(
    store: &ProjectStore,
    generation_id: &[u8; 16],
    _frozen_commit_sequence: i64,
    materialized: &MaterializedFormatExport,
    destination: &Path,
) -> Result<(), KernelError> {
    let mut overlays = HashMap::<ResourceId, Vec<&ImageOverlay>>::new();
    for overlay in &materialized.image_overlays {
        overlays
            .entry(overlay.image_resource_id)
            .or_default()
            .push(overlay);
    }
    let parent = destination.parent().unwrap_or_else(|| Path::new("."));
    let resources = store.markdown_image_resources(generation_id)?;
    let mut pending = Vec::with_capacity(resources.len());
    for resource in resources {
        let relative = resource
            .semantic_path
            .strip_prefix("image/")
            .ok_or_else(|| AdapterError::InvalidInput("invalid Markdown image path".to_owned()))?;
        let relative = Path::new(relative);
        if relative.as_os_str().is_empty()
            || relative.is_absolute()
            || relative.components().any(|component| {
                matches!(
                    component,
                    std::path::Component::ParentDir | std::path::Component::RootDir
                )
            })
        {
            return Err(AdapterError::InvalidInput(
                "Markdown image path is outside the export directory".to_owned(),
            )
            .into());
        }
        let target = parent.join(relative);
        if target.exists() {
            return Err(KernelError::Io(std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                format!(
                    "Markdown image export target already exists: {}",
                    target.display()
                ),
            )));
        }
        let locator: Locator = serde_json::from_slice(&resource.locator_json).map_err(|error| {
            AdapterError::InvalidInput(format!("invalid Markdown image locator: {error}"))
        })?;
        let source = match locator {
            Locator::ByteSpan {
                object_hash,
                start: 0,
                end,
            } => {
                let record = store.object_record(&object_hash)?;
                if end != record.byte_length {
                    return Err(AdapterError::InvalidInput(
                        "Markdown image locator does not cover its authoritative object".to_owned(),
                    )
                    .into());
                }
                materialized
                    .registry
                    .grant_object(object_hash, record.byte_length)?
            }
            _ => {
                return Err(AdapterError::InvalidInput(
                    "Markdown image is not available in the project resource closure".to_owned(),
                )
                .into());
            }
        };
        let mut source_bytes = Vec::new();
        materialized
            .registry
            .open_object(&source)?
            .read_to_end(&mut source_bytes)?;
        let bytes = match overlays.get(&ResourceId::from_bytes(resource.resource_id)) {
            Some(layers) => {
                compose_markdown_image_overlays(&source_bytes, layers, &materialized.registry)?
            }
            None => source_bytes,
        };
        pending.push((target, bytes));
    }
    for (target, bytes) in pending {
        let target_parent = target.parent().ok_or_else(|| {
            AdapterError::InvalidInput("Markdown image export path has no parent".to_owned())
        })?;
        fs::create_dir_all(target_parent)?;
        let mut writer = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&target)?;
        writer.write_all(&bytes)?;
        writer.flush()?;
    }
    Ok(())
}

fn compose_markdown_image_overlays(
    source_bytes: &[u8],
    overlays: &[&ImageOverlay],
    registry: &CapabilityRegistry,
) -> Result<Vec<u8>, KernelError> {
    let format = image::guess_format(source_bytes).map_err(|error| {
        AdapterError::InvalidInput(format!("unsupported Markdown image: {error}"))
    })?;
    let mut composed = image::load_from_memory(source_bytes)
        .map_err(|error| AdapterError::InvalidInput(format!("invalid Markdown image: {error}")))?
        .to_rgba8();
    for overlay in overlays {
        let Locator::SpatialRegion { polygon, .. } = &overlay.region_locator else {
            return Err(AdapterError::InvalidInput(
                "Markdown image overlay requires a spatial region locator".to_owned(),
            )
            .into());
        };
        let mut derived_bytes = Vec::new();
        registry
            .open_object(&overlay.derived_object)?
            .read_to_end(&mut derived_bytes)?;
        let derived = image::load_from_memory(&derived_bytes)
            .map_err(|error| AdapterError::InvalidInput(format!("invalid derived image: {error}")))?
            .to_rgba8();
        if derived.dimensions() != composed.dimensions() {
            return Err(AdapterError::InvalidInput(
                "derived image dimensions do not match the Markdown source image".to_owned(),
            )
            .into());
        }
        for y in 0..composed.height() {
            for x in 0..composed.width() {
                if image_point_in_polygon(x as f32 + 0.5, y as f32 + 0.5, polygon) {
                    *composed.get_pixel_mut(x, y) = *derived.get_pixel(x, y);
                }
            }
        }
    }
    let mut output = Vec::new();
    image::DynamicImage::ImageRgba8(composed)
        .write_to(&mut Cursor::new(&mut output), format)
        .map_err(|error| {
            AdapterError::InvalidInput(format!("cannot encode Markdown image: {error}"))
        })?;
    Ok(output)
}

fn image_point_in_polygon(x: f32, y: f32, polygon: &[[f32; 2]]) -> bool {
    if polygon.len() < 3 {
        return false;
    }
    let mut inside = false;
    let mut previous = polygon.len() - 1;
    for current in 0..polygon.len() {
        let [current_x, current_y] = polygon[current];
        let [previous_x, previous_y] = polygon[previous];
        if (current_y > y) != (previous_y > y) {
            let intersection =
                (previous_x - current_x) * (y - current_y) / (previous_y - current_y) + current_x;
            if x < intersection {
                inside = !inside;
            }
        }
        previous = current;
    }
    inside
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
    #[error("export recovery failed: {0}")]
    Recovery(#[from] babel_storage::recovery::RecoveryError),
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
    #[error("workbench projection is invalid: {0}")]
    InvalidWorkbenchProjection(String),
    #[error("workspace mutation is invalid: {0}")]
    InvalidWorkspaceMutation(String),
    #[error("translation work item was not found")]
    WorkItemNotFound,
    #[error("translation revision conflict")]
    RevisionConflict,
    #[error("translation history has no valid target for this operation")]
    HistoryUnavailable,
    #[error("translation document is invalid: {0}")]
    InvalidTranslationDocument(String),
    #[error("image rendering failed: {0}")]
    Image(#[from] babel_image::RenderError),
    #[error("image preview resource is invalid: {0}")]
    InvalidImagePreview(String),
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
        fs::create_dir_all(root.join("workspace"))?;
        fs::create_dir_all(root.join("recycle"))?;
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

        let mut store = ProjectStore::open(root.join("project.sqlite3"))?;
        store.recover_interrupted_tasks(now_millis()?)?;
        recover_workspace_operations(&mut store, &root, now_millis()?)?;
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

    /// Read and re-verify an immutable CAS object before handing it to a
    /// renderer or exporter. Callers never receive an unverified path.
    pub fn read_object(&self, hash: [u8; 32]) -> Result<Vec<u8>, KernelError> {
        let path = cas_path(&self.root.join("objects"), &hash);
        cas::verify_object(&path, &hash)?;
        Ok(fs::read(path)?)
    }

    /// Read an image through its generation locator and return bounded,
    /// verified preview bytes. Filesystem and archive paths stay inside core.
    pub fn read_image_preview(
        &self,
        generation_id: [u8; 16],
        resource_id: [u8; 16],
    ) -> Result<ImagePreview, KernelError> {
        let query = self.query()?;
        let resource = query
            .generation_resource(&generation_id, &resource_id)?
            .ok_or_else(|| {
                KernelError::InvalidImagePreview("image resource not found".to_owned())
            })?;
        if resource.kind != "Image" {
            return Err(KernelError::InvalidImagePreview(
                "resource is not an image".to_owned(),
            ));
        }
        let locator: Locator = serde_json::from_slice(&resource.locator_json)
            .map_err(|error| KernelError::InvalidImagePreview(error.to_string()))?;
        let (source_hash, bytes) = match locator {
            Locator::ByteSpan {
                object_hash,
                start,
                end,
            } => {
                let object = self.read_object(object_hash)?;
                let start = usize::try_from(start).map_err(|_| {
                    KernelError::InvalidImagePreview("image range overflows usize".to_owned())
                })?;
                let end = usize::try_from(end).map_err(|_| {
                    KernelError::InvalidImagePreview("image range overflows usize".to_owned())
                })?;
                if start > end || end > object.len() {
                    return Err(KernelError::InvalidImagePreview(
                        "image byte range is outside the verified object".to_owned(),
                    ));
                }
                (object_hash, object[start..end].to_vec())
            }
            Locator::ArchiveMemberByteSpan {
                object_hash,
                member_path,
                start,
                end,
            } => {
                let object = self.read_object(object_hash)?;
                let mut archive = zip::ZipArchive::new(Cursor::new(object)).map_err(|error| {
                    KernelError::InvalidImagePreview(format!("invalid archive: {error}"))
                })?;
                let mut member = archive.by_name(&member_path).map_err(|error| {
                    KernelError::InvalidImagePreview(format!("image member not found: {error}"))
                })?;
                let member_size = usize::try_from(member.size()).map_err(|_| {
                    KernelError::InvalidImagePreview("image member is too large".to_owned())
                })?;
                if member_size > MAX_IMAGE_PREVIEW_BYTES {
                    return Err(KernelError::InvalidImagePreview(
                        "image member exceeds preview limit".to_owned(),
                    ));
                }
                let mut member_bytes = Vec::with_capacity(member_size);
                member.read_to_end(&mut member_bytes)?;
                let start = usize::try_from(start).map_err(|_| {
                    KernelError::InvalidImagePreview("image range overflows usize".to_owned())
                })?;
                let end = usize::try_from(end).map_err(|_| {
                    KernelError::InvalidImagePreview("image range overflows usize".to_owned())
                })?;
                if start > end || end > member_bytes.len() {
                    return Err(KernelError::InvalidImagePreview(
                        "image member range is invalid".to_owned(),
                    ));
                }
                (object_hash, member_bytes[start..end].to_vec())
            }
            _ => {
                return Err(KernelError::InvalidImagePreview(
                    "image locator is not a byte or archive-member locator".to_owned(),
                ));
            }
        };
        if bytes.is_empty() || bytes.len() > MAX_IMAGE_PREVIEW_BYTES {
            return Err(KernelError::InvalidImagePreview(
                "image preview size is outside the allowed range".to_owned(),
            ));
        }
        let format = image::guess_format(&bytes).map_err(|error| {
            KernelError::InvalidImagePreview(format!("unsupported image: {error}"))
        })?;
        let media_type = match format {
            image::ImageFormat::Png => "image/png",
            image::ImageFormat::Jpeg => "image/jpeg",
            image::ImageFormat::WebP => "image/webp",
            _ => {
                return Err(KernelError::InvalidImagePreview(
                    "only PNG, JPEG and WebP previews are supported".to_owned(),
                ));
            }
        };
        Ok(ImagePreview {
            media_type: media_type.to_owned(),
            byte_length: bytes.len(),
            source_hash,
            data_base64: BASE64.encode(bytes),
        })
    }

    pub fn render_image_region(
        &self,
        source_hash: [u8; 32],
        polygon: &babel_image::SpatialPolygon,
        translation: &str,
        style: &babel_image::RenderStyle,
    ) -> Result<babel_image::RenderedImage, KernelError> {
        let bytes = self.read_object(source_hash)?;
        Ok(babel_image::render_png(
            &bytes,
            source_hash,
            polygon,
            translation,
            style,
        )?)
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

    pub fn save_translation_document(
        &self,
        source_unit_key: [u8; 32],
        command_id: [u8; 32],
        expected_revision_id: Option<i64>,
        document: TranslationDocumentV1,
        created_at_ms: i64,
    ) -> Result<SaveReceipt, KernelError> {
        document
            .validate()
            .map_err(|error| KernelError::InvalidTranslationDocument(error.to_string()))?;
        let response = self.request(
            &self.interactive,
            Command::SaveTranslationDocument {
                source_unit_key,
                command_id,
                expected_revision_id,
                document,
                created_at_ms,
            },
        )?;
        match response {
            Response::Saved(receipt) => Ok(receipt),
            _ => Err(KernelError::UnexpectedResponse),
        }
    }

    pub fn save_image_region_edit(
        &self,
        request: SaveImageRegionEditRequest,
    ) -> Result<SaveReceipt, KernelError> {
        let response = self.request(&self.interactive, Command::SaveImageRegionEdit { request })?;
        match response {
            Response::Saved(receipt) => Ok(receipt),
            _ => Err(KernelError::UnexpectedResponse),
        }
    }

    pub fn save_ocr_candidate(
        &self,
        request: SaveOcrCandidateRequest,
    ) -> Result<bool, KernelError> {
        let response = self.request(&self.interactive, Command::SaveOcrCandidate { request })?;
        match response {
            Response::OcrCandidateSaved { replayed } => Ok(replayed),
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

    pub fn undo_translation(
        &self,
        unit_id: UnitId,
        command_id: [u8; 32],
        created_at_ms: i64,
    ) -> Result<SaveReceipt, KernelError> {
        match self.request(
            &self.interactive,
            Command::UndoTranslation {
                unit_id: *unit_id.as_bytes(),
                command_id,
                created_at_ms,
            },
        )? {
            Response::Saved(receipt) => Ok(receipt),
            _ => Err(KernelError::UnexpectedResponse),
        }
    }

    pub fn redo_translation(
        &self,
        unit_id: UnitId,
        command_id: [u8; 32],
        created_at_ms: i64,
    ) -> Result<SaveReceipt, KernelError> {
        match self.request(
            &self.interactive,
            Command::RedoTranslation {
                unit_id: *unit_id.as_bytes(),
                command_id,
                created_at_ms,
            },
        )? {
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

    pub fn mutate_workspace(
        &self,
        request: WorkspaceMutationRequest,
    ) -> Result<WorkspaceMutationReceipt, KernelError> {
        let response = self.request(
            &self.background,
            Command::MutateWorkspace { request },
        )?;
        match response {
            Response::WorkspaceMutated(receipt) => Ok(receipt),
            _ => Err(KernelError::UnexpectedResponse),
        }
    }

    pub fn save_navigation_position(
        &self,
        position: NavigationPosition,
        client_session_id: String,
        position_sequence: u64,
        updated_at_ms: i64,
    ) -> Result<NavigationSaveReceipt, KernelError> {
        let response = self.request(
            &self.interactive,
            Command::SaveNavigationPosition {
                position,
                client_session_id,
                position_sequence,
                updated_at_ms,
            },
        )?;
        match response {
            Response::NavigationSaved(receipt) => Ok(receipt),
            _ => Err(KernelError::UnexpectedResponse),
        }
    }

    pub fn navigation_position(
        &self,
    ) -> Result<Option<babel_storage::query::SavedNavigationPosition>, KernelError> {
        Ok(self.query()?.navigation_position()?)
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

    pub fn import_markdown_path(
        &self,
        source_id: [u8; 16],
        source_path: impl AsRef<Path>,
        created_at_ms: i64,
    ) -> Result<MarkdownImportReport, KernelError> {
        let source_path = source_path.as_ref();
        let import_started = Instant::now();
        let mut source = File::open(source_path)?;
        let published = self.publish_source_reader(
            source_id,
            FormatKind::Markdown.media_type().to_owned(),
            &mut source,
            created_at_ms,
        )?;
        let mut prepared = prepare_format_source_with_worker(
            FormatKind::Markdown,
            self.root.join("objects"),
            published.hash,
        )?;
        attach_markdown_assets(
            &mut prepared,
            source_path,
            self.root.join("objects"),
            created_at_ms,
        )?;
        let response = self.request(
            &self.background,
            Command::CommitFormatImport {
                prepared,
                created_at_ms,
            },
        )?;
        profile_import("import.markdown_path.total", import_started.elapsed());
        match response {
            Response::FormatImported(report) => Ok(report),
            _ => Err(KernelError::UnexpectedResponse),
        }
    }

    pub fn import_epub_reader(
        &self,
        source_id: [u8; 16],
        reader: impl Read,
        created_at_ms: i64,
    ) -> Result<EpubImportReport, KernelError> {
        self.import_format_reader(FormatKind::Epub, source_id, reader, created_at_ms)
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

    pub fn validate_active_epub(&self) -> Result<Vec<EpubValidationIssue>, KernelError> {
        self.validate_active_format(FormatKind::Epub)
    }

    pub fn validate_active_format_id(&self, format_id: &str) -> Result<Vec<FormatValidationIssue>, KernelError> {
        let format = match format_id {
            "txt" => FormatKind::Txt,
            "markdown" | "md" => FormatKind::Markdown,
            "epub" => FormatKind::Epub,
            other => return Err(KernelError::WorkerDiagnostic(format!("unsupported format: {other}"))),
        };
        self.validate_active_format(format)
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

    pub fn pending_epub_bindings(
        &self,
        generation_id: [u8; 16],
    ) -> Result<Vec<EpubBindingReview>, KernelError> {
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

    pub fn activate_epub_import(
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

    pub fn export_active_epub(&self) -> Result<EpubExportReport, KernelError> {
        self.export_active_format(FormatKind::Epub)
    }

    pub fn export_active_epub_to_path(
        &self,
        destination: impl AsRef<Path>,
    ) -> Result<EpubFileExportReport, KernelError> {
        self.export_active_format_to_path(FormatKind::Epub, destination.as_ref().to_owned())
    }

    pub fn export_active_markdown_to_path(
        &self,
        destination: impl AsRef<Path>,
    ) -> Result<FormatFileExportReport, KernelError> {
        self.export_active_format_to_path(FormatKind::Markdown, destination.as_ref().to_owned())
    }

    pub fn export_active_format_id_to_path(
        &self,
        format_id: &str,
        destination: impl AsRef<Path>,
    ) -> Result<FormatFileExportReport, KernelError> {
        let format = match format_id {
            "txt" => FormatKind::Txt,
            "markdown" | "md" => FormatKind::Markdown,
            "epub" => FormatKind::Epub,
            other => return Err(KernelError::WorkerDiagnostic(format!("unsupported format: {other}"))),
        };
        self.export_active_format_to_path(format, destination.as_ref().to_owned())
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

    fn export_active_format_to_path(
        &self,
        expected_format: FormatKind,
        destination: PathBuf,
    ) -> Result<FormatFileExportReport, KernelError> {
        let response = self.request(
            &self.background,
            Command::ExportActiveFormatToPath {
                expected_format,
                destination,
            },
        )?;
        match response {
            Response::FormatFileExported(report) => Ok(report),
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

    pub fn publish_export_bytes(
        &self,
        export_id: i64,
        bytes: &[u8],
        destination: &Path,
        format: &str,
        created_at_ms: i64,
    ) -> Result<(), KernelError> {
        babel_storage::recovery::run_export_to_path_with_hook(
            &self.root,
            export_id,
            bytes,
            destination,
            format,
            created_at_ms,
            |_| {},
        )
        ?;
        Ok(())
    }

    pub fn query(&self) -> Result<ProjectQuery, KernelError> {
        Ok(ProjectQuery::open(self.database_path())?)
    }

    pub fn translation_work_item(
        &self,
        unit_id: UnitId,
        view: WorkspaceView,
    ) -> Result<TranslationWorkItem, KernelError> {
        let query = self.query()?;
        let commit_sequence = query.commit_sequence()?;
        let record = query
            .workbench_unit(unit_id.as_bytes())?
            .ok_or(KernelError::WorkItemNotFound)?;
        assemble_work_item(&query, self.project_id, view, commit_sequence, record)
    }

    pub fn resource_queue(
        &self,
        after: Option<ResourceQueueCursor>,
        limit: usize,
    ) -> Result<ResourceQueuePage, KernelError> {
        let query = self.query()?;
        let project_commit_sequence = query.commit_sequence()?;
        let page_size = limit.clamp(1, 256);
        let mut records = query.resource_queue_after(
            after.map(|cursor| (cursor.reading_order, *cursor.unit_id.as_bytes())),
            page_size + 1,
        )?;
        let has_more = records.len() > page_size;
        if has_more {
            records.truncate(page_size);
        }
        let items = records
            .into_iter()
            .map(|record| {
                assemble_work_item(
                    &query,
                    self.project_id,
                    WorkspaceView::Resources,
                    project_commit_sequence,
                    record,
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
        let next_cursor = has_more
            .then(|| {
                items.last().map(|item| ResourceQueueCursor {
                    reading_order: item.reading_order,
                    unit_id: item.unit_id,
                })
            })
            .flatten();
        Ok(ResourceQueuePage {
            items,
            next_cursor,
            project_commit_sequence,
        })
    }

    pub fn image_region_edit(
        &self,
        unit_id: UnitId,
    ) -> Result<Option<ImageRegionEditRecord>, KernelError> {
        Ok(self.query()?.image_region_edit(unit_id.as_bytes())?)
    }

    pub fn ocr_candidate(
        &self,
        generation_id: [u8; 16],
        region_resource_id: [u8; 16],
        model_hash: [u8; 32],
    ) -> Result<Option<OcrCandidateCacheRecord>, KernelError> {
        Ok(self
            .query()?
            .ocr_candidate(&generation_id, &region_resource_id, &model_hash)?)
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

fn assemble_work_item(
    query: &ProjectQuery,
    project_id: ProjectId,
    view: WorkspaceView,
    project_commit_sequence: i64,
    record: babel_storage::query::WorkbenchUnitRecord,
) -> Result<TranslationWorkItem, KernelError> {
    let locator: Locator = serde_json::from_slice(&record.locator_json)
        .map_err(|error| KernelError::InvalidWorkbenchProjection(error.to_string()))?;
    let source: UnitContent = serde_json::from_slice(&record.tir_json)
        .map_err(|error| KernelError::InvalidWorkbenchProjection(error.to_string()))?;
    source
        .validate()
        .map_err(|error| KernelError::InvalidWorkbenchProjection(error.to_string()))?;

    let mut resources = Vec::new();
    let primary_resource_id = ResourceId::from_bytes(record.resource_id);
    resources.push(ResourceAssociation {
        resource_id: primary_resource_id,
        kind: record.resource_kind,
        semantic_path: record.semantic_path,
        relation: "source".to_owned(),
    });
    let mut seen = HashSet::from([record.resource_id]);
    for resource in query.related_resources(&record.generation_id, &record.resource_id)? {
        if seen.insert(resource.resource_id) {
            resources.push(ResourceAssociation {
                resource_id: ResourceId::from_bytes(resource.resource_id),
                kind: resource.kind,
                semantic_path: resource.semantic_path,
                relation: resource.edge_kind,
            });
        }
    }
    for token in &source.tokens {
        let Token::Reference {
            resource_id,
            relation,
        } = token
        else {
            continue;
        };
        if !seen.insert(*resource_id.as_bytes()) {
            continue;
        }
        if let Some(resource) =
            query.generation_resource(&record.generation_id, resource_id.as_bytes())?
        {
            resources.push(ResourceAssociation {
                resource_id: *resource_id,
                kind: resource.kind,
                semantic_path: resource.semantic_path,
                relation: relation.clone(),
            });
        }
    }

    let translation_document = match record.translation_document_json.as_deref() {
        Some(json) => {
            let document: TranslationDocumentV1 = serde_json::from_slice(json)
                .map_err(|error| KernelError::InvalidWorkbenchProjection(error.to_string()))?;
            document
                .validate()
                .map_err(|error| KernelError::InvalidWorkbenchProjection(error.to_string()))?;
            document
        }
        None => TranslationDocumentV1::from_plain_text(record.translation.as_deref().unwrap_or("")),
    };
    let status = match record.translation.as_deref() {
        None | Some("") => TranslationStatus::Untranslated,
        Some(_) => TranslationStatus::Draft,
    };
    Ok(TranslationWorkItem {
        schema_version: TRANSLATION_WORK_ITEM_SCHEMA_VERSION,
        project_id,
        view,
        generation_id: record.generation_id,
        unit_id: UnitId::from_bytes(record.unit_id),
        source_unit_key: record.source_unit_key,
        source,
        source_text: record.source_text,
        translation: record.translation,
        translation_document,
        status,
        locator,
        reading_order: record.reading_order,
        revision_id: record.revision_id,
        revision_commit_sequence: record.revision_commit_sequence,
        project_commit_sequence,
        resources,
    })
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
    SaveTranslationDocument {
        source_unit_key: [u8; 32],
        command_id: [u8; 32],
        expected_revision_id: Option<i64>,
        document: TranslationDocumentV1,
        created_at_ms: i64,
    },
    SaveImageRegionEdit {
        request: SaveImageRegionEditRequest,
    },
    SaveOcrCandidate {
        request: SaveOcrCandidateRequest,
    },
    RestoreTranslation {
        source_unit_key: [u8; 32],
        command_id: [u8; 32],
        expected_head_revision_id: i64,
        restores_revision_id: i64,
        kind: RevisionKind,
        created_at_ms: i64,
    },
    UndoTranslation {
        unit_id: [u8; 16],
        command_id: [u8; 32],
        created_at_ms: i64,
    },
    RedoTranslation {
        unit_id: [u8; 16],
        command_id: [u8; 32],
        created_at_ms: i64,
    },
    SaveDraft {
        unit_id: Vec<u8>,
        base_revision_id: Option<i64>,
        client_session_id: String,
        patch: Vec<u8>,
        updated_at_ms: i64,
    },
    SaveNavigationPosition {
        position: NavigationPosition,
        client_session_id: String,
        position_sequence: u64,
        updated_at_ms: i64,
    },
    MutateWorkspace {
        request: WorkspaceMutationRequest,
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
    ExportActiveFormatToPath {
        expected_format: FormatKind,
        destination: PathBuf,
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
    OcrCandidateSaved { replayed: bool },
    NavigationSaved(NavigationSaveReceipt),
    WorkspaceMutated(WorkspaceMutationReceipt),
    Task(TaskRecord),
    Diagnostic(i64),
    GarbageCollected(GcReport),
    BackupStarted(BackupSnapshot),
    FormatImported(FormatImportReport),
    FormatBindings(Vec<FormatBindingReview>),
    FormatValidation(Vec<FormatValidationIssue>),
    FormatExported(FormatExportReport),
    FormatFileExported(FormatFileExportReport),
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
        Command::SaveTranslationDocument {
            source_unit_key,
            command_id,
            expected_revision_id,
            document,
            created_at_ms,
        } => {
            let text = document.plain_text();
            let document_json = serde_json::to_vec(&document)
                .map_err(|error| KernelError::InvalidTranslationDocument(error.to_string()))?;
            let receipt = store
                .save_translation_document(
                    &source_unit_key,
                    &command_id,
                    &text,
                    i64::from(document.schema_version),
                    &document_json,
                    expected_revision_id,
                    created_at_ms,
                )
                .map_err(|error| match error {
                    rusqlite::Error::InvalidQuery => KernelError::RevisionConflict,
                    other => KernelError::Storage(other),
                })?;
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
        Command::SaveImageRegionEdit { request } => {
            let receipt = store.save_image_region_edit(&request)?;
            if !receipt.replayed {
                publish_event(
                    subscribers,
                    CommitEvent::ImageRegionCommitted {
                        revision_id: receipt.revision_id,
                        commit_sequence: receipt.commit_sequence,
                    },
                );
            }
            Ok(Response::Saved(receipt))
        }
        Command::SaveOcrCandidate { request } => {
            let replayed = store.save_ocr_candidate(&request)?;
            Ok(Response::OcrCandidateSaved { replayed })
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
        Command::UndoTranslation {
            unit_id,
            command_id,
            created_at_ms,
        } => {
            let receipt = store
                .undo_translation(&unit_id, &command_id, created_at_ms)
                .map_err(|error| match error {
                    rusqlite::Error::InvalidQuery | rusqlite::Error::QueryReturnedNoRows => {
                        KernelError::HistoryUnavailable
                    }
                    other => KernelError::Storage(other),
                })?;
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
        Command::RedoTranslation {
            unit_id,
            command_id,
            created_at_ms,
        } => {
            let receipt = store
                .redo_translation(&unit_id, &command_id, created_at_ms)
                .map_err(|error| match error {
                    rusqlite::Error::InvalidQuery | rusqlite::Error::QueryReturnedNoRows => {
                        KernelError::HistoryUnavailable
                    }
                    other => KernelError::Storage(other),
                })?;
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
        Command::SaveNavigationPosition {
            position,
            client_session_id,
            position_sequence,
            updated_at_ms,
        } => Ok(Response::NavigationSaved(store.save_navigation_position(
            &position,
            &client_session_id,
            position_sequence,
            updated_at_ms,
        )?)),
        Command::MutateWorkspace { request } => Ok(Response::WorkspaceMutated(mutate_workspace(
            root,
            store,
            request,
            now_millis().map_err(KernelError::Clock)?,
        )?)),
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
        Command::ExportActiveFormatToPath {
            expected_format,
            destination,
        } => {
            let generation_id = store
                .active_generation()?
                .ok_or(rusqlite::Error::QueryReturnedNoRows)?;
            ensure_generation_format(store, &generation_id, expected_format)?;
            Ok(Response::FormatFileExported(export_format_to_path(
                store,
                root.join("objects"),
                root.join("staging"),
                &generation_id,
                store.commit_sequence()?,
                destination,
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
) -> Result<(Vec<ExtractedUnit>, Option<u64>), KernelError> {
    let mut cursor = None;
    let mut units = Vec::new();
    let mut worker_peak_rss_kib = None;
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
        worker_peak_rss_kib = worker_peak_rss_kib.max(reply.worker_peak_rss_kib);
        cursor = reply.page.next_cursor;
        if cursor.is_none() {
            break;
        }
    }
    Ok((units, worker_peak_rss_kib))
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
        launch.request_timeout = format.request_timeout();
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

fn format_budget_for(format: FormatKind, source_bytes: u64) -> TaskBudget {
    let mut budget = format_budget(source_bytes);
    if format == FormatKind::Epub {
        budget.timeout_ms = 120_000;
        budget.maximum_bytes = source_bytes
            .saturating_mul(8)
            .clamp(FORMAT_PIPELINE_PAGE_BYTES, 4 * 1024 * 1024 * 1024);
        budget.maximum_nodes = 2_000_000;
    }
    budget
}

fn cas_path(object_root: &Path, hash: &[u8; 32]) -> PathBuf {
    let encoded = hex::encode(hash);
    object_root
        .join("sha256")
        .join(&encoded[..2])
        .join(&encoded[2..])
}

const WORKSPACE_ROOT_NODE_ID: &str = "workspace-root";
const RECYCLE_ROOT_NODE_ID: &str = "recycle-root";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WorkspaceNodeKind {
    Source,
    Workspace,
    Recycle,
    Derived,
}

struct ResolvedWorkspaceNode {
    kind: WorkspaceNodeKind,
    path: PathBuf,
}

fn workspace_root_path(root: &Path) -> PathBuf {
    root.join("workspace")
}

fn recycle_root_path(root: &Path) -> PathBuf {
    root.join("recycle")
}

fn resolve_workspace_node(root: &Path, node_id: &str) -> Result<ResolvedWorkspaceNode, KernelError> {
    if node_id == WORKSPACE_ROOT_NODE_ID {
        return Ok(ResolvedWorkspaceNode {
            kind: WorkspaceNodeKind::Workspace,
            path: workspace_root_path(root),
        });
    }
    if node_id == RECYCLE_ROOT_NODE_ID {
        return Ok(ResolvedWorkspaceNode {
            kind: WorkspaceNodeKind::Recycle,
            path: recycle_root_path(root),
        });
    }
    if let Some(relative) = node_id.strip_prefix("workspace/").or_else(|| node_id.strip_prefix("workspace:")) {
        return resolve_scoped_node(root, WorkspaceNodeKind::Workspace, relative);
    }
    if let Some(relative) = node_id.strip_prefix("recycle/").or_else(|| node_id.strip_prefix("recycle:")) {
        return resolve_scoped_node(root, WorkspaceNodeKind::Recycle, relative);
    }
    if node_id.len() == 32 && node_id.chars().all(|value| value.is_ascii_hexdigit()) {
        return Ok(ResolvedWorkspaceNode {
            kind: WorkspaceNodeKind::Source,
            path: root.join("source").join(node_id),
        });
    }
    if node_id == "source-root" {
        return Ok(ResolvedWorkspaceNode {
            kind: WorkspaceNodeKind::Source,
            path: root.join("source"),
        });
    }
    if node_id == "derived-root" {
        return Ok(ResolvedWorkspaceNode {
            kind: WorkspaceNodeKind::Derived,
            path: root.join("derived"),
        });
    }
    if let Some(relative) = node_id.strip_prefix("derived/").or_else(|| node_id.strip_prefix("derived:")) {
        return resolve_scoped_node(root, WorkspaceNodeKind::Derived, relative);
    }
    Err(KernelError::InvalidWorkspaceMutation(format!(
        "unrecognized node id: {node_id}"
    )))
}

fn resolve_scoped_node(
    root: &Path,
    kind: WorkspaceNodeKind,
    relative: &str,
) -> Result<ResolvedWorkspaceNode, KernelError> {
    let relative_path = validate_relative_workspace_path(relative)?;
    let base = match kind {
        WorkspaceNodeKind::Workspace => workspace_root_path(root),
        WorkspaceNodeKind::Recycle => recycle_root_path(root),
        WorkspaceNodeKind::Source => root.join("source"),
        WorkspaceNodeKind::Derived => root.join("derived"),
    };
    Ok(ResolvedWorkspaceNode {
        kind,
        path: base.join(&relative_path),
    })
}

fn validate_relative_workspace_path(relative: &str) -> Result<PathBuf, KernelError> {
    let path = Path::new(relative);
    if relative.is_empty()
        || path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                std::path::Component::ParentDir
                    | std::path::Component::RootDir
                    | std::path::Component::Prefix(_)
            )
        })
    {
        return Err(KernelError::InvalidWorkspaceMutation(format!(
            "invalid relative path: {relative}"
        )));
    }
    Ok(path.to_owned())
}

fn normalize_relative_path(path: &Path) -> String {
    path.iter()
        .map(|component| component.to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

fn reject_symlink_ancestors(path: &Path) -> Result<(), KernelError> {
    let mut current = PathBuf::new();
    for component in path.components() {
        current.push(component.as_os_str());
        if let Ok(metadata) = fs::symlink_metadata(&current) {
            if metadata.file_type().is_symlink() {
                return Err(KernelError::InvalidWorkspaceMutation(format!(
                    "path escapes through symlink: {}",
                    current.display()
                )));
            }
        }
    }
    Ok(())
}

fn relative_to_workspace_root(root: &Path, path: &Path) -> Result<PathBuf, KernelError> {
    let workspace_root = workspace_root_path(root);
    let recycle_root = recycle_root_path(root);
    if let Ok(relative) = path.strip_prefix(&workspace_root) {
        Ok(PathBuf::from("workspace").join(relative))
    } else if let Ok(relative) = path.strip_prefix(&recycle_root) {
        Ok(PathBuf::from("recycle").join(relative))
    } else {
        Err(KernelError::InvalidWorkspaceMutation(format!(
            "path is outside the project workspace: {}",
            path.display()
        )))
    }
}

fn workspace_node_id_from_path(root: &Path, path: &Path) -> Result<String, KernelError> {
    Ok(normalize_relative_path(&relative_to_workspace_root(root, path)?))
}

fn record_workspace_operation(
    store: &ProjectStore,
    request: &RecordWorkspaceOperationRequest,
) -> rusqlite::Result<()> {
    store.record_workspace_operation(request)
}

fn finish_workspace_operation(
    store: &ProjectStore,
    request: &FinishWorkspaceOperationRequest,
) -> rusqlite::Result<()> {
    store.finish_workspace_operation(request)
}

fn recover_workspace_operations(
    store: &mut ProjectStore,
    root: &Path,
    recovered_at_ms: i64,
) -> Result<(), KernelError> {
    let pending = store
        .workspace_operations_in_state(WorkspaceOperationState::Preparing)
        .map_err(KernelError::Storage)?;
    for record in pending {
        let mut finish = FinishWorkspaceOperationRequest {
            operation_id: record.operation_id.clone(),
            state: WorkspaceOperationState::CancelledAfterCrash,
            commit_sequence: None,
            error: record.error.clone(),
            completed_at_ms: recovered_at_ms,
        };
        match record.kind.as_str() {
            "create-folder" => {
                if let Some(target_path) = &record.target_path {
                    let path = root.join(target_path);
                    if path.is_dir() {
                        finish.state = WorkspaceOperationState::Completed;
                    }
                }
            }
            "rename" | "move" => {
                if let (Some(source_path), Some(target_path)) =
                    (&record.source_path, &record.target_path)
                {
                    let source = root.join(source_path);
                    let target = root.join(target_path);
                    if target.exists() && !source.exists() {
                        finish.state = WorkspaceOperationState::Completed;
                    }
                }
            }
            "trash" => {
                if let Some(recycle_path) = &record.recycle_path {
                    let recycle = root.join(recycle_path);
                    let source_exists = record
                        .source_path
                        .as_deref()
                        .map(|path| root.join(path).exists())
                        .unwrap_or(false);
                    if recycle.exists() && !source_exists {
                        finish.state = WorkspaceOperationState::Completed;
                    }
                }
            }
            "restore" => {
                if let (Some(source_path), Some(target_path)) =
                    (&record.source_path, &record.target_path)
                {
                    let source = root.join(source_path);
                    let target = root.join(target_path);
                    if target.exists() && !source.exists() {
                        finish.state = WorkspaceOperationState::Completed;
                    }
                }
            }
            "reveal" => {
                finish.state = WorkspaceOperationState::Completed;
            }
            _ => {
                finish.state = WorkspaceOperationState::Failed;
                finish.error = Some("unknown workspace operation".to_owned());
            }
        }
        if finish.state == WorkspaceOperationState::Completed {
            if finish.commit_sequence.is_none() {
                let commit_sequence = store
                    .complete_workspace_operation(&finish.operation_id, recovered_at_ms)
                    .map_err(KernelError::Storage)?;
                finish.commit_sequence = Some(commit_sequence);
            }
        } else {
            finish_workspace_operation(store, &finish).map_err(KernelError::Storage)?;
        }
    }
    Ok(())
}

fn mutate_workspace(
    root: &Path,
    store: &mut ProjectStore,
    request: WorkspaceMutationRequest,
    created_at_ms: i64,
) -> Result<WorkspaceMutationReceipt, KernelError> {
    let operation_id = hex::encode(TaskId::new().as_bytes());
    let mut affected_node_ids = Vec::new();
    let mut record = RecordWorkspaceOperationRequest {
        operation_id: operation_id.clone(),
        kind: String::new(),
        source_node_id: None,
        target_node_id: None,
        source_path: None,
        target_path: None,
        recycle_path: None,
        created_at_ms,
    };

    let result: Result<(), KernelError> = match request {
        WorkspaceMutationRequest::CreateFolder {
            project_id,
            parent_id,
            name,
        } => {
            ensure_project_matches(store, &project_id)?;
            let parent = resolve_workspace_node(root, &parent_id)?;
            if parent.kind != WorkspaceNodeKind::Workspace {
                return Err(KernelError::InvalidWorkspaceMutation(
                    "folders can only be created under the workspace root".to_owned(),
                ));
            }
            validate_workspace_entry_name(&name)?;
            let target_path = parent.path.join(&name);
            reject_symlink_ancestors(&target_path.parent().unwrap_or(&target_path).to_path_buf())?;
            record.kind = "create-folder".to_owned();
            record.target_node_id = Some(format!(
                "workspace/{}",
                normalize_relative_path(
                    &target_path
                        .strip_prefix(workspace_root_path(root))
                        .map_err(|_| KernelError::InvalidWorkspaceMutation(
                            "target escaped workspace root".to_owned()
                        ))?
                        .to_owned()
                )
            ));
            record.target_path = Some(
                relative_to_workspace_root(root, &target_path)?
                    .to_string_lossy()
                    .into_owned(),
            );
            record_workspace_operation(store, &record).map_err(KernelError::Storage)?;
            fs::create_dir_all(&target_path)?;
            affected_node_ids.push(record.target_node_id.clone().unwrap());
            Ok(())
        }
        WorkspaceMutationRequest::Rename {
            project_id,
            node_id,
            name,
        } => {
            ensure_project_matches(store, &project_id)?;
            let source = resolve_workspace_node(root, &node_id)?;
            if source.kind != WorkspaceNodeKind::Workspace {
                return Err(KernelError::InvalidWorkspaceMutation(
                    "only workspace items can be renamed".to_owned(),
                ));
            }
            validate_workspace_entry_name(&name)?;
            let parent = source.path.parent().ok_or_else(|| {
                KernelError::InvalidWorkspaceMutation("source path has no parent".to_owned())
            })?;
            reject_symlink_ancestors(parent)?;
            let target_path = parent.join(&name);
            record.kind = "rename".to_owned();
            record.source_node_id = Some(node_id.clone());
            record.target_node_id = Some(format!(
                "workspace/{}",
                normalize_relative_path(
                    &target_path
                        .strip_prefix(workspace_root_path(root))
                        .map_err(|_| KernelError::InvalidWorkspaceMutation(
                            "target escaped workspace root".to_owned()
                        ))?
                        .to_owned()
                )
            ));
            record.source_path = Some(
                relative_to_workspace_root(root, &source.path)?
                    .to_string_lossy()
                    .into_owned(),
            );
            record.target_path = Some(
                relative_to_workspace_root(root, &target_path)?
                    .to_string_lossy()
                    .into_owned(),
            );
            record_workspace_operation(store, &record).map_err(KernelError::Storage)?;
            fs::rename(&source.path, &target_path)?;
            affected_node_ids.push(node_id);
            affected_node_ids.push(record.target_node_id.clone().unwrap());
            Ok(())
        }
        WorkspaceMutationRequest::Move {
            project_id,
            node_id,
            parent_id,
        } => {
            ensure_project_matches(store, &project_id)?;
            let source = resolve_workspace_node(root, &node_id)?;
            if source.kind != WorkspaceNodeKind::Workspace {
                return Err(KernelError::InvalidWorkspaceMutation(
                    "only workspace items can be moved".to_owned(),
                ));
            }
            let parent = resolve_workspace_node(root, &parent_id)?;
            if parent.kind != WorkspaceNodeKind::Workspace {
                return Err(KernelError::InvalidWorkspaceMutation(
                    "workspace items can only be moved inside the workspace".to_owned(),
                ));
            }
            reject_symlink_ancestors(&parent.path)?;
            let file_name = source.path.file_name().ok_or_else(|| {
                KernelError::InvalidWorkspaceMutation("source path has no file name".to_owned())
            })?;
            let target_path = parent.path.join(file_name);
            record.kind = "move".to_owned();
            record.source_node_id = Some(node_id.clone());
            record.target_node_id = Some(format!(
                "workspace/{}",
                normalize_relative_path(
                    &target_path
                        .strip_prefix(workspace_root_path(root))
                        .map_err(|_| KernelError::InvalidWorkspaceMutation(
                            "target escaped workspace root".to_owned()
                        ))?
                        .to_owned()
                )
            ));
            record.source_path = Some(
                relative_to_workspace_root(root, &source.path)?
                    .to_string_lossy()
                    .into_owned(),
            );
            record.target_path = Some(
                relative_to_workspace_root(root, &target_path)?
                    .to_string_lossy()
                    .into_owned(),
            );
            record_workspace_operation(store, &record).map_err(KernelError::Storage)?;
            fs::rename(&source.path, &target_path)?;
            affected_node_ids.push(node_id);
            affected_node_ids.push(record.target_node_id.clone().unwrap());
            Ok(())
        }
        WorkspaceMutationRequest::Trash { project_id, node_id } => {
            ensure_project_matches(store, &project_id)?;
            let source = resolve_workspace_node(root, &node_id)?;
            if source.kind != WorkspaceNodeKind::Workspace {
                return Err(KernelError::InvalidWorkspaceMutation(
                    "only workspace items can be moved to the recycle bin".to_owned(),
                ));
            }
            let relative = source
                .path
                .strip_prefix(workspace_root_path(root))
                .map_err(|_| KernelError::InvalidWorkspaceMutation(
                    "source escaped workspace root".to_owned(),
                ))?
                .to_owned();
            let recycle_path = recycle_root_path(root)
                .join(&operation_id)
                .join(&relative);
            if let Some(parent) = recycle_path.parent() {
                reject_symlink_ancestors(parent)?;
                fs::create_dir_all(parent)?;
            }
            record.kind = "trash".to_owned();
            record.source_node_id = Some(node_id.clone());
            record.source_path = Some(
                relative_to_workspace_root(root, &source.path)?
                    .to_string_lossy()
                    .into_owned(),
            );
            record.recycle_path = Some(
                relative_to_workspace_root(root, &recycle_path)?
                    .to_string_lossy()
                    .into_owned(),
            );
            record_workspace_operation(store, &record).map_err(KernelError::Storage)?;
            fs::rename(&source.path, &recycle_path)?;
            affected_node_ids.push(node_id);
            affected_node_ids.push(format!(
                "recycle/{}/{}",
                operation_id,
                normalize_relative_path(&relative)
            ));
            Ok(())
        }
        WorkspaceMutationRequest::Restore { project_id, node_id } => {
            ensure_project_matches(store, &project_id)?;
            let source = resolve_workspace_node(root, &node_id)?;
            if source.kind != WorkspaceNodeKind::Recycle {
                return Err(KernelError::InvalidWorkspaceMutation(
                    "only recycle items can be restored".to_owned(),
                ));
            }
            let trash = store
                .workspace_operations_in_state(WorkspaceOperationState::Completed)
                .map_err(KernelError::Storage)?
                .into_iter()
                .rev()
                .find(|record| {
                    record.kind == "trash"
                        && record
                            .recycle_path
                            .as_deref()
                            .map(|path| root.join(path) == source.path)
                            .unwrap_or(false)
                })
                .ok_or_else(|| {
                    KernelError::InvalidWorkspaceMutation(
                        "restore target has no matching trash log entry".to_owned(),
                    )
                })?;
            let target_path = trash.source_path.as_ref().ok_or_else(|| {
                KernelError::InvalidWorkspaceMutation("trash log is missing original path".to_owned())
            })?;
            let target_path = root.join(target_path);
            if let Some(parent) = target_path.parent() {
                reject_symlink_ancestors(parent)?;
                fs::create_dir_all(parent)?;
            }
            record.kind = "restore".to_owned();
            record.source_node_id = Some(node_id.clone());
            record.target_node_id = Some(workspace_node_id_from_path(root, &target_path)?);
            record.source_path = Some(
                relative_to_workspace_root(root, &source.path)?
                    .to_string_lossy()
                    .into_owned(),
            );
            record.target_path = Some(
                relative_to_workspace_root(root, &target_path)?
                    .to_string_lossy()
                    .into_owned(),
            );
            record_workspace_operation(store, &record).map_err(KernelError::Storage)?;
            fs::rename(&source.path, &target_path)?;
            affected_node_ids.push(node_id);
            affected_node_ids.push(record.target_node_id.clone().unwrap());
            Ok(())
        }
        WorkspaceMutationRequest::Reveal { project_id, node_id } => {
            ensure_project_matches(store, &project_id)?;
            let node = resolve_workspace_node(root, &node_id)?;
            match node.kind {
                WorkspaceNodeKind::Source | WorkspaceNodeKind::Workspace | WorkspaceNodeKind::Recycle | WorkspaceNodeKind::Derived => {}
            }
            if !node.path.exists() {
                return Err(KernelError::InvalidWorkspaceMutation(format!(
                    "path does not exist: {}",
                    node.path.display()
                )));
            }
            record.kind = "reveal".to_owned();
            record.source_node_id = Some(node_id.clone());
            record.source_path = Some(
                relative_to_workspace_root(root, &node.path)?
                    .to_string_lossy()
                    .into_owned(),
            );
            record_workspace_operation(store, &record).map_err(KernelError::Storage)?;
            affected_node_ids.push(node_id);
            Ok(())
        }
    };

    match result {
        Ok(()) => {
            let commit_sequence = store
                .complete_workspace_operation(&operation_id, created_at_ms)
                .map_err(KernelError::Storage)?;
            Ok(WorkspaceMutationReceipt {
                operation_id,
                commit_sequence,
                affected_node_ids,
            })
        }
        Err(error) => {
            let _ = finish_workspace_operation(
                store,
                &FinishWorkspaceOperationRequest {
                    operation_id,
                    state: WorkspaceOperationState::Failed,
                    commit_sequence: None,
                    error: Some(error.to_string()),
                    completed_at_ms: created_at_ms,
                },
            );
            Err(error)
        }
    }
}

fn validate_workspace_entry_name(name: &str) -> Result<(), KernelError> {
    let path = Path::new(name);
    if name.is_empty()
        || path.is_absolute()
        || path.components().count() != 1
        || path.components().any(|component| {
            matches!(
                component,
                std::path::Component::ParentDir
                    | std::path::Component::RootDir
                    | std::path::Component::Prefix(_)
            )
        })
    {
        return Err(KernelError::InvalidWorkspaceMutation(format!(
            "invalid workspace entry name: {name}"
        )));
    }
    Ok(())
}

fn ensure_project_matches(store: &ProjectStore, project_id: &str) -> Result<(), KernelError> {
    let expected = hex::encode(store.project_id()?.as_bytes());
    if expected != project_id {
        return Err(KernelError::InvalidWorkspaceMutation(
            "workspace mutation project id does not match the open project".to_owned(),
        ));
    }
    Ok(())
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
    use std::{io::Write, process::Command as ProcessCommand, sync::Arc, sync::Once};

    use rusqlite::Connection;
    use sha2::{Digest, Sha256};
    use tempfile::TempDir;
    use zip::{CompressionMethod, ZipArchive, ZipWriter, write::SimpleFileOptions};

    use super::*;

    static TXT_WORKER_BUILD: Once = Once::new();
    static MARKDOWN_WORKER_BUILD: Once = Once::new();
    static EPUB_WORKER_BUILD: Once = Once::new();

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

    fn ensure_epub_worker_binary() {
        EPUB_WORKER_BUILD.call_once(|| {
            let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_owned());
            let status = ProcessCommand::new(cargo)
                .args(["build", "--quiet", "-p", "babel-epub-worker"])
                .status()
                .expect("build babel-epub-worker");
            assert!(status.success(), "babel-epub-worker build failed: {status}");
        });
    }

    fn epub_fixture(spine: &[&str]) -> Vec<u8> {
        let cursor = Cursor::new(Vec::new());
        let mut writer = ZipWriter::new(cursor);
        let spine_xml = spine
            .iter()
            .map(|id| format!(r#"<itemref idref="{id}"/>"#))
            .collect::<String>();
        let package = format!(
            r#"<package version="3.0"><manifest><item id="c1" href="chapter1.xhtml" media-type="application/xhtml+xml"/><item id="c2" href="chapter2.xhtml" media-type="application/xhtml+xml"/><item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/><item id="css" href="style.css" media-type="text/css"/></manifest><spine>{spine_xml}</spine></package>"#
        );
        let entries = [
            (
                "mimetype",
                b"application/epub+zip".as_slice(),
                CompressionMethod::Stored,
            ),
            (
                "META-INF/container.xml",
                br#"<container><rootfiles><rootfile full-path="EPUB/package.opf"/></rootfiles></container>"#.as_slice(),
                CompressionMethod::Deflated,
            ),
            (
                "EPUB/package.opf",
                package.as_bytes(),
                CompressionMethod::Deflated,
            ),
            (
                "EPUB/chapter1.xhtml",
                br#"<html xmlns="http://www.w3.org/1999/xhtml"><body><h1>Title</h1><p>Alpha</p></body></html>"#.as_slice(),
                CompressionMethod::Deflated,
            ),
            (
                "EPUB/chapter2.xhtml",
                br#"<html xmlns="http://www.w3.org/1999/xhtml"><body><p>Beta</p></body></html>"#.as_slice(),
                CompressionMethod::Deflated,
            ),
            (
                "EPUB/nav.xhtml",
                br#"<html xmlns="http://www.w3.org/1999/xhtml"><body><nav><a href="chapter1.xhtml">One</a><a href="chapter2.xhtml">Two</a></nav></body></html>"#.as_slice(),
                CompressionMethod::Deflated,
            ),
            (
                "EPUB/style.css",
                b"body { color: black; }".as_slice(),
                CompressionMethod::Deflated,
            ),
        ];
        for (name, bytes, method) in entries {
            writer
                .start_file(
                    name,
                    SimpleFileOptions::default().compression_method(method),
                )
                .unwrap();
            writer.write_all(bytes).unwrap();
        }
        writer.finish().unwrap().into_inner()
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

    fn project_id_hex(root: &Path) -> String {
        let store = ProjectStore::open(root.join("project.sqlite3")).unwrap();
        hex::encode(store.project_id().unwrap().as_bytes())
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
    fn markdown_path_import_and_file_export_keep_relative_image_closure() {
        ensure_markdown_worker_binary();
        let temp = TempDir::new().unwrap();
        let source_dir = temp.path().join("source");
        fs::create_dir_all(source_dir.join("assets")).unwrap();
        let image_path = source_dir.join("assets/cover.png");
        let mut image_bytes = Vec::new();
        image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
            2,
            2,
            image::Rgba([20, 40, 60, 255]),
        ))
        .write_to(&mut Cursor::new(&mut image_bytes), image::ImageFormat::Png)
        .unwrap();
        fs::write(&image_path, &image_bytes).unwrap();
        let source_path = source_dir.join("book.md");
        fs::write(&source_path, b"# Title\n\n![cover](assets/cover.png)\n").unwrap();

        let kernel = Kernel::open(temp.path().join("book.babel")).unwrap();
        let report = kernel
            .import_markdown_path([10; 16], &source_path, 1_000)
            .unwrap();
        let units = kernel.query().unwrap().page_after(-1, 100).unwrap();
        for (index, unit) in units.iter().enumerate() {
            kernel
                .save_translation(
                    unit.source_unit_key.clone().try_into().unwrap(),
                    hash_parts(&[b"markdown-image-closure", &(index as u64).to_be_bytes()]),
                    unit.source_text.clone(),
                    2_000 + index as i64,
                )
                .unwrap();
        }
        let destination = temp.path().join("export/book.md");
        fs::create_dir_all(destination.parent().unwrap()).unwrap();
        assert!(kernel.export_active_markdown().is_err());
        kernel.export_active_markdown_to_path(&destination).unwrap();
        assert_eq!(
            fs::read(&destination).unwrap(),
            fs::read(&source_path).unwrap()
        );
        assert_eq!(
            fs::read(destination.parent().unwrap().join("assets/cover.png")).unwrap(),
            image_bytes
        );
        assert_eq!(report.units, units.len());
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

    #[test]
    fn epub_import_save_reopen_export_and_spine_reorder_share_the_generic_core() {
        ensure_epub_worker_binary();
        let temp = TempDir::new().unwrap();
        let project = temp.path().join("book.babel");
        let source = epub_fixture(&["c1", "c2"]);
        {
            let kernel = Kernel::open(&project).unwrap();
            let report = kernel
                .import_epub_reader([12; 16], Cursor::new(source.clone()), 1_000)
                .unwrap();
            assert!(report.activated);
            assert_eq!(report.units, 3);
            assert_eq!(kernel.validate_active_epub().unwrap().len(), 3);
            let units = kernel.query().unwrap().page_after(-1, 10).unwrap();
            assert_eq!(
                units
                    .iter()
                    .map(|unit| unit.source_text.as_str())
                    .collect::<Vec<_>>(),
                ["Title", "Alpha", "Beta"]
            );
            for unit in units {
                let translation = match unit.source_text.as_str() {
                    "Title" => "标题",
                    "Alpha" => "阿尔法",
                    "Beta" => "贝塔",
                    other => panic!("unexpected EPUB source unit: {other}"),
                };
                kernel
                    .save_translation(
                        unit.source_unit_key.try_into().unwrap(),
                        hash_parts(&[b"kernel-epub", unit.source_text.as_bytes()]),
                        translation.to_owned(),
                        2_000,
                    )
                    .unwrap();
            }
            assert!(kernel.validate_active_epub().unwrap().is_empty());
        }

        let kernel = Kernel::open(&project).unwrap();
        assert!(kernel.validate_active_epub().unwrap().is_empty());
        let destination = temp.path().join("translated.epub");
        let export = kernel.export_active_epub_to_path(&destination).unwrap();
        assert_eq!(export.path, destination);
        assert_eq!(
            export.byte_length,
            fs::metadata(&destination).unwrap().len()
        );
        let mut archive = ZipArchive::new(File::open(&destination).unwrap()).unwrap();
        let mut chapter = String::new();
        archive
            .by_name("EPUB/chapter1.xhtml")
            .unwrap()
            .read_to_string(&mut chapter)
            .unwrap();
        assert!(chapter.contains("标题"));
        assert!(chapter.contains("阿尔法"));
        let mut stylesheet = String::new();
        archive
            .by_name("EPUB/style.css")
            .unwrap()
            .read_to_string(&mut stylesheet)
            .unwrap();
        assert_eq!(stylesheet, "body { color: black; }");
        drop(archive);
        let original_export = fs::read(&destination).unwrap();
        assert!(matches!(
            kernel.export_active_epub_to_path(&destination),
            Err(KernelError::Adapter(AdapterError::Io(_)))
        ));
        assert_eq!(fs::read(&destination).unwrap(), original_export);

        let reordered = kernel
            .import_epub_reader([13; 16], Cursor::new(epub_fixture(&["c2", "c1"])), 3_000)
            .unwrap();
        assert!(reordered.activated);
        assert_eq!(reordered.review_required, 0);
        let units = kernel.query().unwrap().page_after(-1, 10).unwrap();
        assert_eq!(
            units
                .iter()
                .map(|unit| (unit.source_text.as_str(), unit.translation.as_deref()))
                .collect::<Vec<_>>(),
            [
                ("Beta", Some("贝塔")),
                ("Title", Some("标题")),
                ("Alpha", Some("阿尔法")),
            ]
        );
    }

    #[test]
    fn one_translation_work_item_is_shared_by_every_workspace_view() {
        ensure_txt_worker_binary();
        let temp = TempDir::new().unwrap();
        let project = temp.path().join("work-item.babel");
        let kernel = Kernel::open(&project).unwrap();
        kernel
            .import_txt_reader([31; 16], Cursor::new(b"Alpha\nBeta\n".to_vec()), 1_000)
            .unwrap();
        let row = kernel.query().unwrap().page_after(-1, 1).unwrap().remove(0);
        let unit_id = UnitId::from_bytes(row.unit_id.clone().try_into().unwrap());

        let long_form = kernel
            .translation_work_item(unit_id, WorkspaceView::LongForm)
            .unwrap();
        let units = kernel
            .translation_work_item(unit_id, WorkspaceView::Units)
            .unwrap();
        let resources = kernel
            .translation_work_item(unit_id, WorkspaceView::Resources)
            .unwrap();

        assert_eq!(long_form.unit_id, units.unit_id);
        assert_eq!(units.unit_id, resources.unit_id);
        assert_eq!(long_form.source, units.source);
        assert_eq!(units.source, resources.source);
        assert_eq!(long_form.status, TranslationStatus::Untranslated);
        assert_eq!(long_form.view, WorkspaceView::LongForm);
        assert_eq!(units.view, WorkspaceView::Units);
        assert_eq!(resources.view, WorkspaceView::Resources);

        kernel
            .save_translation(
                row.source_unit_key.try_into().unwrap(),
                [32; 32],
                "阿尔法".to_owned(),
                2_000,
            )
            .unwrap();
        let translated = kernel
            .translation_work_item(unit_id, WorkspaceView::LongForm)
            .unwrap();
        assert_eq!(translated.translation.as_deref(), Some("阿尔法"));
        assert_eq!(translated.status, TranslationStatus::Draft);
        assert_eq!(translated.revision_id, Some(1));
        assert_eq!(translated.revision_commit_sequence, Some(1));
        assert_eq!(translated.project_commit_sequence, 1);
    }

    #[test]
    fn navigation_rejects_stale_updates_and_recovers_after_reopen() {
        let temp = TempDir::new().unwrap();
        let project = temp.path().join("navigation.babel");
        fs::create_dir_all(&project).unwrap();
        let mut store = ProjectStore::open(project.join("project.sqlite3")).unwrap();
        store.seed_units(1).unwrap();
        drop(store);

        let expected;
        {
            let kernel = Kernel::open(&project).unwrap();
            let unit_id = UnitId::from_bytes(
                kernel.query().unwrap().page_after(-1, 1).unwrap()[0]
                    .unit_id
                    .clone()
                    .try_into()
                    .unwrap(),
            );
            let mut first = NavigationPosition::new(kernel.project_id(), WorkspaceView::LongForm);
            first.unit_id = Some(unit_id);
            first.scroll_anchor_unit_id = Some(unit_id);
            first.scroll_offset_px = 240;
            first.filters.query = Some("未完成".to_owned());
            let saved = kernel
                .save_navigation_position(first.clone(), "session-a".to_owned(), 2, 100)
                .unwrap();
            assert!(saved.accepted);

            let mut stale = first.clone();
            stale.view = WorkspaceView::Units;
            stale.scroll_offset_px = 480;
            let stale_receipt = kernel
                .save_navigation_position(stale, "session-a".to_owned(), 1, 101)
                .unwrap();
            assert!(!stale_receipt.accepted);
            assert_eq!(
                kernel.navigation_position().unwrap().unwrap().position,
                first
            );

            let mut resumed = first;
            resumed.view = WorkspaceView::Units;
            resumed.scroll_offset_px = 64;
            resumed.filters.only_incomplete = true;
            let new_session = kernel
                .save_navigation_position(resumed.clone(), "session-b".to_owned(), 0, 102)
                .unwrap();
            assert!(new_session.accepted);
            assert_eq!(kernel.query().unwrap().commit_sequence().unwrap(), 0);
            expected = resumed;
        }

        let reopened = Kernel::open(&project).unwrap();
        let recovered = reopened.navigation_position().unwrap().unwrap();
        assert_eq!(recovered.position, expected);
        assert_eq!(recovered.client_session_id, "session-b");
        assert_eq!(recovered.position_sequence, 0);
        assert_eq!(recovered.updated_at_ms, 102);
    }

    #[test]
    fn resource_queue_pages_image_regions_in_stable_reading_order() {
        let temp = TempDir::new().unwrap();
        let project = temp.path().join("resource-queue.babel");
        fs::create_dir_all(&project).unwrap();
        let generation_id = babel_domain::core::GenerationId::new();
        let image_id = ResourceId::new();
        let first_region_id = ResourceId::new();
        let second_region_id = ResourceId::new();
        let mut store = ProjectStore::open(project.join("project.sqlite3")).unwrap();
        store
            .begin_generation(&GenerationDescriptor {
                generation_id: *generation_id.as_bytes(),
                source_snapshot_hash: [41; 32],
                adapter_id: "org.babel-tower.image-test".to_owned(),
                adapter_build: "test".to_owned(),
                identity_version: 1,
                created_at_ms: 1,
            })
            .unwrap();

        let polygon = vec![[0.0, 0.0], [100.0, 0.0], [100.0, 40.0], [0.0, 40.0]];
        let region_locator = |region_id: ResourceId| Locator::SpatialRegion {
            resource_id: image_id,
            polygon: polygon.clone(),
            coordinate_space: format!("pixel:{:?}", region_id.as_bytes()),
        };
        let empty_candidates = serde_json::to_vec(&Vec::<[u8; 16]>::new()).unwrap();
        let units = [
            (first_region_id, [51; 16], [61; 32], 2_u64, "first"),
            (second_region_id, [52; 16], [62; 32], 5_u64, "second"),
        ];
        let mut batch = GenerationBatch {
            resources: vec![GenerationResourceRecord {
                resource_id: *image_id.as_bytes(),
                resource_key: [40; 32],
                kind: "Image".to_owned(),
                semantic_path: "images/page-1.png".to_owned(),
                locator_json: serde_json::to_vec(&Locator::OpaqueAdapter {
                    adapter_id: "image-test".to_owned(),
                    schema_version: 1,
                    bytes_hash: [41; 32],
                })
                .unwrap(),
            }],
            ..GenerationBatch::default()
        };
        for (index, (region_id, extracted_id, source_key, reading_order, text)) in
            units.into_iter().enumerate()
        {
            let locator = region_locator(region_id);
            batch.resources.push(GenerationResourceRecord {
                resource_id: *region_id.as_bytes(),
                resource_key: [70 + index as u8; 32],
                kind: "ImageRegion".to_owned(),
                semantic_path: format!("images/page-1.png#region-{}", index + 1),
                locator_json: serde_json::to_vec(&locator).unwrap(),
            });
            batch.edges.push(GenerationEdgeRecord {
                from_resource_id: *region_id.as_bytes(),
                to_resource_id: *image_id.as_bytes(),
                edge_kind: "RegionOf".to_owned(),
                ordinal: index as u32,
            });
            batch.units.push(GenerationUnitRecord {
                extracted_unit_id: extracted_id,
                source_unit_key: source_key,
                resource_id: *region_id.as_bytes(),
                locator_json: serde_json::to_vec(&locator).unwrap(),
                tir_json: serde_json::to_vec(&UnitContent {
                    schema_version: babel_tir::TIR_SCHEMA_VERSION,
                    tokens: vec![Token::Text {
                        text: text.to_owned(),
                        style_hint: None,
                    }],
                })
                .unwrap(),
                reading_order,
            });
            batch.bindings.push(GenerationBindingRecord {
                binding_id: [80 + index as u8; 16],
                extracted_unit_id: extracted_id,
                disposition: "Orphaned".to_owned(),
                selected_unit_id: None,
                policy_version: 1,
                candidates_hash: candidate_set_hash(&empty_candidates),
                candidates_json: empty_candidates.clone(),
            });
        }
        store
            .append_generation_batch(generation_id.as_bytes(), &[91; 32], &[92; 32], &batch)
            .unwrap();
        store.seal_generation(generation_id.as_bytes()).unwrap();
        store
            .activate_generation(generation_id.as_bytes(), 2)
            .unwrap();
        drop(store);

        let kernel = Kernel::open(&project).unwrap();
        let first_page = kernel.resource_queue(None, 1).unwrap();
        assert_eq!(first_page.items.len(), 1);
        assert_eq!(first_page.items[0].reading_order, 2);
        assert_eq!(first_page.items[0].view, WorkspaceView::Resources);
        assert!(matches!(
            first_page.items[0].locator,
            Locator::SpatialRegion { .. }
        ));
        assert_eq!(first_page.items[0].resources[0].kind, "ImageRegion");
        assert_eq!(first_page.items[0].resources[1].kind, "Image");
        assert_eq!(first_page.items[0].resources[1].relation, "RegionOf");
        assert_eq!(first_page.next_cursor.unwrap().reading_order, 2);

        let second_page = kernel.resource_queue(first_page.next_cursor, 1).unwrap();
        assert_eq!(second_page.items.len(), 1);
        assert_eq!(second_page.items[0].reading_order, 5);
        assert_ne!(second_page.items[0].unit_id, first_page.items[0].unit_id);
        assert_eq!(second_page.next_cursor, None);
    }

    #[test]
    fn workspace_mutations_are_logged_and_recover_trash_operations() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("book.babel");
        fs::create_dir_all(root.join("workspace")).unwrap();
        fs::create_dir_all(root.join("recycle")).unwrap();
        let mut store = ProjectStore::open(root.join("project.sqlite3")).unwrap();
        store.seed_units(1).unwrap();
        let project_id = project_id_hex(&root);

        let create = Kernel::open(&root)
            .unwrap()
            .mutate_workspace(WorkspaceMutationRequest::CreateFolder {
                project_id: project_id.clone(),
                parent_id: "workspace-root".to_owned(),
                name: "drafts".to_owned(),
            })
            .unwrap();
        assert_eq!(create.commit_sequence, 1);
        assert!(root.join("workspace/drafts").is_dir());

        let chapter = root.join("workspace/drafts/chapter.txt");
        fs::write(&chapter, b"chapter").unwrap();
        let kernel = Kernel::open(&root).unwrap();
        let trash = kernel
            .mutate_workspace(WorkspaceMutationRequest::Trash {
                project_id: project_id.clone(),
                node_id: "workspace/drafts/chapter.txt".to_owned(),
            })
            .unwrap();
        let recycle_node_id = trash
            .affected_node_ids
            .iter()
            .find(|value| value.starts_with("recycle/"))
            .cloned()
            .unwrap();
        let recycle_path = root
            .join("recycle")
            .join(&trash.operation_id)
            .join("drafts")
            .join("chapter.txt");
        assert!(recycle_path.exists());
        assert!(!chapter.exists());

        let restore = kernel
            .mutate_workspace(WorkspaceMutationRequest::Restore {
                project_id,
                node_id: recycle_node_id,
            })
            .unwrap();
        assert!(chapter.exists());
        assert!(!recycle_path.exists());
        assert!(restore.commit_sequence > trash.commit_sequence);

        let reopened = ProjectStore::open(root.join("project.sqlite3")).unwrap();
        let records = reopened
            .workspace_operations_in_state(WorkspaceOperationState::Completed)
            .unwrap();
        assert!(records.iter().any(|record| record.kind == "create-folder"));
        assert!(records.iter().any(|record| record.kind == "trash"));
        assert!(records.iter().any(|record| record.kind == "restore"));
    }

    #[test]
    fn unfinished_workspace_trash_is_completed_during_recovery() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("book.babel");
        fs::create_dir_all(root.join("workspace")).unwrap();
        fs::create_dir_all(root.join("recycle")).unwrap();
        let mut store = ProjectStore::open(root.join("project.sqlite3")).unwrap();
        store.seed_units(1).unwrap();

        let source = root.join("workspace").join("chapter.txt");
        let recycle = root
            .join("recycle")
            .join("crash-recovery")
            .join("chapter.txt");
        fs::write(&source, b"chapter").unwrap();
        store
            .record_workspace_operation(&RecordWorkspaceOperationRequest {
                operation_id: "crash-recovery".to_owned(),
                kind: "trash".to_owned(),
                source_node_id: Some("workspace/chapter.txt".to_owned()),
                target_node_id: None,
                source_path: Some("workspace/chapter.txt".to_owned()),
                target_path: None,
                recycle_path: Some("recycle/crash-recovery/chapter.txt".to_owned()),
                created_at_ms: 1,
            })
            .unwrap();
        fs::create_dir_all(recycle.parent().unwrap()).unwrap();
        fs::rename(&source, &recycle).unwrap();
        drop(store);

        let kernel = Kernel::open(&root).unwrap();
        assert!(recycle.exists());
        assert!(!source.exists());

        let reopened = ProjectStore::open(kernel.database_path()).unwrap();
        let records = reopened
            .workspace_operations_in_state(WorkspaceOperationState::Completed)
            .unwrap();
        let record = records
            .iter()
            .find(|record| record.operation_id == "crash-recovery")
            .unwrap();
        assert_eq!(record.kind, "trash");
        assert_eq!(record.commit_sequence, Some(1));
        assert_eq!(record.state, WorkspaceOperationState::Completed);
    }
}
