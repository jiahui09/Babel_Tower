//! Capability-scoped adapter host and deterministic mock adapter contract harness.

use std::{
    collections::HashMap,
    fs::{self, File, OpenOptions},
    io::{BufRead, BufReader, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::Mutex,
};

use babel_adapter_protocol::{
    ADAPTER_PROTOCOL_MAJOR, ADAPTER_PROTOCOL_MINOR, Adapter, AdapterError, AdapterManifest,
    CapabilityAccess, CapabilityIo, Cursor, ExecutionContext, ExportPlan, ExtractedUnit,
    InventoryItem, MaterializeProgress, ObjectHandle, Operation, OverlayUnit, Page, ProbeResult,
    ProtocolRange, ReadSeek, SafetyLimits, StagingHandle, VerificationReport,
};
use babel_domain::core::{GenerationId, ResourceId};
use babel_resource_graph::{
    EdgeKind, Locator, ResourceEdge, ResourceKind, ResourceNode, resource_key,
};
use babel_runtime::dag::{self, Claim};
use babel_tir::{TIR_SCHEMA_VERSION, Token, UnitContent};
use rusqlite::Connection;
use sha2::{Digest, Sha256};
use thiserror::Error;
use uuid::Uuid;

#[derive(Clone)]
enum Grant {
    Object {
        path: PathBuf,
        hash: [u8; 32],
        byte_length: u64,
    },
    Staging {
        path: PathBuf,
    },
}

pub struct CapabilityRegistry {
    object_root: PathBuf,
    staging_root: PathBuf,
    grants: Mutex<HashMap<[u8; 16], Grant>>,
}

impl CapabilityRegistry {
    pub fn new(
        object_root: impl AsRef<Path>,
        staging_root: impl AsRef<Path>,
    ) -> std::io::Result<Self> {
        let object_root = object_root.as_ref().to_owned();
        let staging_root = staging_root.as_ref().to_owned();
        fs::create_dir_all(&object_root)?;
        fs::create_dir_all(&staging_root)?;
        Ok(Self {
            object_root,
            staging_root,
            grants: Mutex::new(HashMap::new()),
        })
    }

    pub fn grant_object(
        &self,
        object_hash: [u8; 32],
        byte_length: u64,
    ) -> Result<ObjectHandle, AdapterError> {
        let hex = hex::encode(object_hash);
        let path = self
            .object_root
            .join("sha256")
            .join(&hex[..2])
            .join(&hex[2..]);
        let metadata = fs::metadata(&path).map_err(|_| AdapterError::CapabilityDenied)?;
        if metadata.len() != byte_length || sha256_file(&path)? != object_hash {
            return Err(AdapterError::CapabilityDenied);
        }
        let capability_id = *Uuid::new_v4().as_bytes();
        self.grants.lock().unwrap().insert(
            capability_id,
            Grant::Object {
                path,
                hash: object_hash,
                byte_length,
            },
        );
        Ok(ObjectHandle {
            capability_id,
            object_hash,
            access: CapabilityAccess::ReadObject,
            byte_length,
        })
    }

    pub fn create_staging(&self) -> Result<StagingHandle, AdapterError> {
        let capability_id = *Uuid::new_v4().as_bytes();
        let path = self
            .staging_root
            .join(format!("{}.candidate", hex::encode(capability_id)));
        File::create(&path)?.sync_all()?;
        self.grants
            .lock()
            .unwrap()
            .insert(capability_id, Grant::Staging { path });
        Ok(StagingHandle {
            capability_id,
            access: CapabilityAccess::ReadWriteStaging,
        })
    }

    pub fn staging_bytes(&self, handle: &StagingHandle) -> Result<Vec<u8>, AdapterError> {
        let mut reader = self.open_staging(handle)?;
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes)?;
        Ok(bytes)
    }

    fn grant(&self, capability_id: &[u8; 16]) -> Result<Grant, AdapterError> {
        self.grants
            .lock()
            .unwrap()
            .get(capability_id)
            .cloned()
            .ok_or(AdapterError::CapabilityDenied)
    }
}

impl CapabilityIo for CapabilityRegistry {
    fn open_object(&self, handle: &ObjectHandle) -> Result<Box<dyn ReadSeek>, AdapterError> {
        if handle.access != CapabilityAccess::ReadObject {
            return Err(AdapterError::CapabilityDenied);
        }
        match self.grant(&handle.capability_id)? {
            Grant::Object {
                path,
                hash,
                byte_length,
            } if hash == handle.object_hash && byte_length == handle.byte_length => {
                Ok(Box::new(File::open(path)?))
            }
            _ => Err(AdapterError::CapabilityDenied),
        }
    }

    fn write_staging_at(
        &self,
        handle: &StagingHandle,
        offset: u64,
        bytes: &[u8],
    ) -> Result<(), AdapterError> {
        if handle.access != CapabilityAccess::ReadWriteStaging {
            return Err(AdapterError::CapabilityDenied);
        }
        match self.grant(&handle.capability_id)? {
            Grant::Staging { path } => {
                let mut file = OpenOptions::new().write(true).open(path)?;
                file.seek(SeekFrom::Start(offset))?;
                file.write_all(bytes)?;
                file.sync_data()?;
                Ok(())
            }
            _ => Err(AdapterError::CapabilityDenied),
        }
    }

    fn open_staging(&self, handle: &StagingHandle) -> Result<Box<dyn ReadSeek>, AdapterError> {
        if handle.access != CapabilityAccess::ReadWriteStaging {
            return Err(AdapterError::CapabilityDenied);
        }
        match self.grant(&handle.capability_id)? {
            Grant::Staging { path } => Ok(Box::new(File::open(path)?)),
            _ => Err(AdapterError::CapabilityDenied),
        }
    }
}

pub struct MockTextAdapter {
    manifest: AdapterManifest,
}

impl Default for MockTextAdapter {
    fn default() -> Self {
        Self {
            manifest: AdapterManifest {
                adapter_id: "org.babel-tower.mock-text".to_owned(),
                adapter_build: "1".to_owned(),
                protocol_range: ProtocolRange {
                    major: ADAPTER_PROTOCOL_MAJOR,
                    minimum_minor: ADAPTER_PROTOCOL_MINOR,
                    maximum_minor: ADAPTER_PROTOCOL_MINOR,
                },
                identity_version: 1,
                mime_types: vec!["text/plain".to_owned()],
                extensions: vec!["txt".to_owned()],
                resource_kinds: vec![ResourceKind::Document, ResourceKind::TextStream],
                export_fidelity_tier: "contract-only".to_owned(),
                deterministic_stages: vec![
                    Operation::Probe,
                    Operation::Inventory,
                    Operation::Extract,
                    Operation::PlanExport,
                ],
                safety_limits: SafetyLimits {
                    maximum_input_bytes: 1024 * 1024 * 1024,
                    maximum_output_bytes: 1024 * 1024 * 1024,
                    maximum_nodes_per_page: 10_000,
                    maximum_page_bytes: 4 * 1024 * 1024,
                },
            },
        }
    }
}

impl Adapter for MockTextAdapter {
    fn manifest(&self) -> &AdapterManifest {
        &self.manifest
    }

    fn probe(
        &self,
        input: &ObjectHandle,
        io: &dyn CapabilityIo,
        context: &ExecutionContext<'_>,
    ) -> Result<ProbeResult, AdapterError> {
        context.checkpoint()?;
        if input.byte_length > self.manifest.safety_limits.maximum_input_bytes {
            return Err(AdapterError::BudgetExceeded);
        }
        let mut reader = io.open_object(input)?;
        let mut prefix = vec![0; input.byte_length.min(4096) as usize];
        let read = reader.read(&mut prefix)?;
        prefix.truncate(read);
        let valid = std::str::from_utf8(&prefix).is_ok();
        Ok(ProbeResult {
            confidence_millionths: if valid { 900_000 } else { 0 },
            detected_media_type: valid.then(|| "text/plain".to_owned()),
            reason_code: if valid { "utf8" } else { "invalid-utf8" }.to_owned(),
        })
    }

    fn inventory(
        &self,
        input: &ObjectHandle,
        _generation_id: GenerationId,
        cursor: Option<&Cursor>,
        _io: &dyn CapabilityIo,
        context: &ExecutionContext<'_>,
    ) -> Result<Page<InventoryItem>, AdapterError> {
        context.checkpoint()?;
        let document_key = resource_key(
            &input.object_hash,
            &self.manifest.adapter_id,
            self.manifest.identity_version,
            "document",
        );
        let stream_key = resource_key(
            &input.object_hash,
            &self.manifest.adapter_id,
            self.manifest.identity_version,
            "document/text",
        );
        let document_id = id_from_hash(document_key);
        let stream_id = id_from_hash(stream_key);
        let items = vec![
            InventoryItem::Node(ResourceNode {
                resource_id: document_id,
                resource_key: document_key,
                kind: ResourceKind::Document,
                semantic_path: "document".to_owned(),
                locator: Locator::ByteSpan {
                    object_hash: input.object_hash,
                    start: 0,
                    end: input.byte_length,
                },
            }),
            InventoryItem::Node(ResourceNode {
                resource_id: stream_id,
                resource_key: stream_key,
                kind: ResourceKind::TextStream,
                semantic_path: "document/text".to_owned(),
                locator: Locator::ByteSpan {
                    object_hash: input.object_hash,
                    start: 0,
                    end: input.byte_length,
                },
            }),
            InventoryItem::Edge(ResourceEdge {
                from: document_id,
                to: stream_id,
                kind: EdgeKind::Contains,
                ordinal: 0,
            }),
        ];
        paginate_items(items, cursor, context, &self.manifest)
    }

    fn extract(
        &self,
        input: &ObjectHandle,
        generation_id: GenerationId,
        resource_id: ResourceId,
        cursor: Option<&Cursor>,
        io: &dyn CapabilityIo,
        context: &ExecutionContext<'_>,
    ) -> Result<Page<ExtractedUnit>, AdapterError> {
        context.checkpoint()?;
        let start = decode_u64_cursor(cursor)?;
        let mut reader = BufReader::new(io.open_object(input)?);
        reader.seek(SeekFrom::Start(start))?;
        let byte_limit = context.budget.bounded_page_bytes(&self.manifest);
        let node_limit = context.budget.bounded_page_nodes(&self.manifest) as usize;
        let mut items = Vec::new();
        let mut emitted_bytes = 0_u64;
        let mut offset = start;
        loop {
            context.checkpoint()?;
            let line_start = offset;
            let mut line = String::new();
            let read = reader.read_line(&mut line)?;
            if read == 0 {
                break;
            }
            if !line.is_char_boundary(line.len()) {
                return Err(AdapterError::InvalidInput("invalid UTF-8".to_owned()));
            }
            offset += read as u64;
            let text = line.trim_end_matches(['\r', '\n']).to_owned();
            let mut identity = Sha256::new();
            identity.update(self.manifest.identity_version.to_be_bytes());
            identity.update((text.len() as u64).to_be_bytes());
            identity.update(text.as_bytes());
            let source_unit_key = identity.finalize().into();
            items.push(ExtractedUnit {
                generation_id,
                resource_id,
                source_unit_key,
                locator: Locator::ByteSpan {
                    object_hash: input.object_hash,
                    start: line_start,
                    end: offset,
                },
                content: UnitContent {
                    schema_version: TIR_SCHEMA_VERSION,
                    tokens: vec![Token::Text {
                        text,
                        style_hint: None,
                    }],
                },
            });
            emitted_bytes += read as u64;
            if items.len() >= node_limit || emitted_bytes >= byte_limit {
                break;
            }
        }
        Ok(Page {
            items,
            next_cursor: (offset < input.byte_length).then(|| encode_u64_cursor(offset)),
            emitted_bytes,
        })
    }

    fn plan_export(
        &self,
        input: &ObjectHandle,
        generation_id: GenerationId,
        frozen_commit_sequence: i64,
        overlays: &[OverlayUnit],
        context: &ExecutionContext<'_>,
    ) -> Result<ExportPlan, AdapterError> {
        context.checkpoint()?;
        let overlay_hash = hash_overlays(overlays);
        let mut plan = Sha256::new();
        plan.update(generation_id.as_bytes());
        plan.update(input.object_hash);
        plan.update(frozen_commit_sequence.to_be_bytes());
        plan.update(overlay_hash);
        Ok(ExportPlan {
            plan_id: plan.finalize().into(),
            generation_id,
            source_object_hash: input.object_hash,
            frozen_commit_sequence,
            overlay_hash,
            ordered_source_unit_keys: overlays
                .iter()
                .map(|overlay| overlay.source_unit_key)
                .collect(),
            ordered_overlay_hashes: overlays.iter().map(hash_overlay).collect(),
        })
    }

    fn materialize(
        &self,
        plan: &ExportPlan,
        input: &ObjectHandle,
        overlays: &[OverlayUnit],
        staging: &StagingHandle,
        cursor: Option<&Cursor>,
        io: &dyn CapabilityIo,
        context: &ExecutionContext<'_>,
    ) -> Result<MaterializeProgress, AdapterError> {
        context.checkpoint()?;
        if input.object_hash != plan.source_object_hash
            || overlays.len() != plan.ordered_source_unit_keys.len()
            || plan.ordered_overlay_hashes.len() != plan.ordered_source_unit_keys.len()
        {
            return Err(AdapterError::InvalidInput(
                "overlay changed after export snapshot".to_owned(),
            ));
        }
        let (mut index, offset) = decode_materialize_cursor(cursor)?;
        let node_limit = context.budget.bounded_page_nodes(&self.manifest) as usize;
        let byte_limit = context.budget.bounded_page_bytes(&self.manifest) as usize;
        let mut chunk = Vec::new();
        let start_index = index;
        while index < plan.ordered_source_unit_keys.len() && index - start_index < node_limit {
            let overlay = overlays
                .get(index)
                .ok_or_else(|| AdapterError::InvalidInput("overlay is missing".to_owned()))?;
            if overlay.source_unit_key != plan.ordered_source_unit_keys[index]
                || hash_overlay(overlay) != plan.ordered_overlay_hashes[index]
            {
                return Err(AdapterError::InvalidInput(
                    "overlay changed after export snapshot".to_owned(),
                ));
            }
            let required = overlay.translated_text.len() + 1;
            if !chunk.is_empty() && chunk.len() + required > byte_limit {
                break;
            }
            if required > byte_limit {
                return Err(AdapterError::BudgetExceeded);
            }
            chunk.extend_from_slice(overlay.translated_text.as_bytes());
            chunk.push(b'\n');
            index += 1;
        }
        let next_offset = offset + chunk.len() as u64;
        let output_limit = context
            .budget
            .maximum_bytes
            .min(self.manifest.safety_limits.maximum_output_bytes);
        if next_offset > output_limit {
            return Err(AdapterError::BudgetExceeded);
        }
        let mut existing = io.open_staging(staging)?;
        let existing_length = existing.seek(SeekFrom::End(0))?;
        match existing_length {
            length if length == offset => io.write_staging_at(staging, offset, &chunk)?,
            length if length == next_offset => {
                existing.seek(SeekFrom::Start(offset))?;
                let mut written = vec![0; chunk.len()];
                existing.read_exact(&mut written)?;
                if written != chunk {
                    return Err(AdapterError::InvalidCursor);
                }
            }
            _ => return Err(AdapterError::InvalidCursor),
        }
        let complete = index == plan.ordered_source_unit_keys.len();
        Ok(MaterializeProgress {
            next_cursor: (!complete).then(|| encode_materialize_cursor(index, next_offset)),
            bytes_written: chunk.len() as u64,
            complete,
        })
    }

    fn verify_output(
        &self,
        candidate: &StagingHandle,
        io: &dyn CapabilityIo,
        context: &ExecutionContext<'_>,
    ) -> Result<VerificationReport, AdapterError> {
        context.checkpoint()?;
        let mut reader = io.open_staging(candidate)?;
        let mut hasher = Sha256::new();
        let mut bytes = 0_u64;
        let mut buffer = [0_u8; 64 * 1024];
        loop {
            let read = reader.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            hasher.update(&buffer[..read]);
            bytes += read as u64;
            if bytes > context.budget.maximum_bytes {
                return Err(AdapterError::BudgetExceeded);
            }
        }
        Ok(VerificationReport {
            valid: true,
            output_hash: hasher.finalize().into(),
            byte_length: bytes,
            issue_codes: Vec::new(),
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArtifactOutcome {
    Computed([u8; 32]),
    Reused([u8; 32]),
}

#[derive(Debug, Error)]
pub enum HostError {
    #[error("persistent DAG failed: {0}")]
    Dag(#[from] dag::DagError),
    #[error("artifact is owned by another live worker")]
    Busy,
    #[error("artifact dependencies are not ready")]
    Blocked,
    #[error("DAG returned an invalid output hash")]
    InvalidOutputHash,
}

pub fn run_artifact(
    connection: &mut Connection,
    artifact_key: [u8; 32],
    owner: &str,
    now_ms: i64,
    lease_ms: i64,
    compute: impl FnOnce() -> [u8; 32],
) -> Result<ArtifactOutcome, HostError> {
    dag::initialize(connection)?;
    dag::register(connection, &artifact_key, now_ms)?;
    match dag::claim(connection, &artifact_key, owner, now_ms, lease_ms)? {
        Claim::Acquired { fencing_token } => {
            let output = compute();
            dag::publish(
                connection,
                &artifact_key,
                owner,
                fencing_token,
                &output,
                now_ms,
            )?;
            Ok(ArtifactOutcome::Computed(output))
        }
        Claim::Ready { output_hash } => Ok(ArtifactOutcome::Reused(
            output_hash
                .try_into()
                .map_err(|_| HostError::InvalidOutputHash)?,
        )),
        Claim::Busy { .. } => Err(HostError::Busy),
        Claim::Blocked { .. } => Err(HostError::Blocked),
    }
}

fn paginate_items<T: Clone + serde::Serialize>(
    items: Vec<T>,
    cursor: Option<&Cursor>,
    context: &ExecutionContext<'_>,
    manifest: &AdapterManifest,
) -> Result<Page<T>, AdapterError> {
    let start = decode_u64_cursor(cursor)? as usize;
    if start > items.len() {
        return Err(AdapterError::InvalidCursor);
    }
    let node_limit = context.budget.bounded_page_nodes(manifest) as usize;
    let byte_limit = context.budget.bounded_page_bytes(manifest);
    let mut page = Vec::new();
    let mut emitted = 0;
    for item in items.iter().skip(start) {
        let size = serde_json::to_vec(item)
            .map_err(|error| AdapterError::InvalidInput(error.to_string()))?
            .len() as u64;
        if !page.is_empty() && (page.len() >= node_limit || emitted + size > byte_limit) {
            break;
        }
        if size > byte_limit {
            return Err(AdapterError::BudgetExceeded);
        }
        page.push(item.clone());
        emitted += size;
    }
    let next = start + page.len();
    Ok(Page {
        items: page,
        next_cursor: (next < items.len()).then(|| encode_u64_cursor(next as u64)),
        emitted_bytes: emitted,
    })
}

fn id_from_hash(hash: [u8; 32]) -> ResourceId {
    ResourceId::from_bytes(hash[..16].try_into().expect("hash prefix is 16 bytes"))
}

fn decode_u64_cursor(cursor: Option<&Cursor>) -> Result<u64, AdapterError> {
    match cursor {
        None => Ok(0),
        Some(Cursor(bytes)) if bytes.len() == 8 => Ok(u64::from_be_bytes(
            bytes.as_slice().try_into().expect("checked cursor length"),
        )),
        Some(_) => Err(AdapterError::InvalidCursor),
    }
}

fn encode_u64_cursor(value: u64) -> Cursor {
    Cursor(value.to_be_bytes().to_vec())
}

fn decode_materialize_cursor(cursor: Option<&Cursor>) -> Result<(usize, u64), AdapterError> {
    match cursor {
        None => Ok((0, 0)),
        Some(Cursor(bytes)) if bytes.len() == 16 => {
            let index = u64::from_be_bytes(bytes[..8].try_into().expect("checked cursor length"));
            let offset = u64::from_be_bytes(bytes[8..].try_into().expect("checked cursor length"));
            Ok((index as usize, offset))
        }
        Some(_) => Err(AdapterError::InvalidCursor),
    }
}

fn encode_materialize_cursor(index: usize, offset: u64) -> Cursor {
    let mut bytes = Vec::with_capacity(16);
    bytes.extend_from_slice(&(index as u64).to_be_bytes());
    bytes.extend_from_slice(&offset.to_be_bytes());
    Cursor(bytes)
}

fn hash_overlays(overlays: &[OverlayUnit]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    for overlay in overlays {
        hasher.update(overlay.source_unit_key);
        let locator = serde_json::to_vec(&overlay.source_locator).expect("locator serializes");
        hasher.update((locator.len() as u64).to_be_bytes());
        hasher.update(locator);
        hasher.update((overlay.translated_text.len() as u64).to_be_bytes());
        hasher.update(overlay.translated_text.as_bytes());
    }
    hasher.finalize().into()
}

fn hash_overlay(overlay: &OverlayUnit) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(overlay.source_unit_key);
    let locator = serde_json::to_vec(&overlay.source_locator).expect("locator serializes");
    hasher.update((locator.len() as u64).to_be_bytes());
    hasher.update(locator);
    hasher.update((overlay.translated_text.len() as u64).to_be_bytes());
    hasher.update(overlay.translated_text.as_bytes());
    hasher.finalize().into()
}

fn sha256_file(path: &Path) -> Result<[u8; 32], AdapterError> {
    let mut reader = BufReader::new(File::open(path)?);
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hasher.finalize().into())
}

#[cfg(test)]
mod tests {
    use std::{
        io::Cursor as IoCursor,
        sync::atomic::{AtomicUsize, Ordering},
    };

    use babel_adapter_protocol::{AdapterEnvelope, CancellationToken, TaskBudget, WireOperation};
    use babel_runtime::ipc::{read_frame, write_frame};
    use prost::Message;
    use tempfile::TempDir;

    use super::*;

    fn fixture() -> (TempDir, CapabilityRegistry, ObjectHandle) {
        let temp = TempDir::new().unwrap();
        let objects = temp.path().join("objects");
        let bytes = b"one\ntwo\nthree\n";
        let hash: [u8; 32] = Sha256::digest(bytes).into();
        let hex = hex::encode(hash);
        let path = objects.join("sha256").join(&hex[..2]).join(&hex[2..]);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
        let registry = CapabilityRegistry::new(&objects, temp.path().join("staging")).unwrap();
        let handle = registry.grant_object(hash, bytes.len() as u64).unwrap();
        (temp, registry, handle)
    }

    fn context<'a>(budget: &'a TaskBudget, token: &'a CancellationToken) -> ExecutionContext<'a> {
        ExecutionContext::new(budget, token)
    }

    fn budget(page_nodes: u32) -> TaskBudget {
        TaskBudget {
            timeout_ms: 10_000,
            maximum_bytes: 1024 * 1024,
            maximum_nodes: 100,
            page_bytes: 1024,
            page_nodes,
        }
    }

    #[test]
    fn extraction_is_paged_and_capabilities_cannot_be_forged() {
        let (_temp, registry, handle) = fixture();
        let adapter = MockTextAdapter::default();
        let token = CancellationToken::default();
        let budget = budget(1);
        let generation = GenerationId::new();
        let resource = ResourceId::new();
        let first = adapter
            .extract(
                &handle,
                generation,
                resource,
                None,
                &registry,
                &context(&budget, &token),
            )
            .unwrap();
        assert_eq!(first.items.len(), 1);
        assert!(first.next_cursor.is_some());

        let mut forged = handle.clone();
        forged.capability_id = [0; 16];
        assert!(matches!(
            registry.open_object(&forged),
            Err(AdapterError::CapabilityDenied)
        ));
    }

    #[test]
    fn export_snapshot_resumes_idempotently_after_interruption() {
        let (_temp, registry, handle) = fixture();
        let adapter = MockTextAdapter::default();
        let token = CancellationToken::default();
        let budget = budget(1);
        let generation = GenerationId::new();
        let overlays = vec![
            OverlayUnit {
                source_unit_key: [1; 32],
                source_locator: Locator::ByteSpan {
                    object_hash: handle.object_hash,
                    start: 0,
                    end: 4,
                },
                translated_text: "一".to_owned(),
            },
            OverlayUnit {
                source_unit_key: [2; 32],
                source_locator: Locator::ByteSpan {
                    object_hash: handle.object_hash,
                    start: 4,
                    end: 8,
                },
                translated_text: "二".to_owned(),
            },
        ];
        let plan = adapter
            .plan_export(&handle, generation, 7, &overlays, &context(&budget, &token))
            .unwrap();
        let staging = registry.create_staging().unwrap();
        let first = adapter
            .materialize(
                &plan,
                &handle,
                &overlays,
                &staging,
                None,
                &registry,
                &context(&budget, &token),
            )
            .unwrap();
        assert!(!first.complete);
        let replayed_first = adapter
            .materialize(
                &plan,
                &handle,
                &overlays,
                &staging,
                None,
                &registry,
                &context(&budget, &token),
            )
            .unwrap();
        assert_eq!(replayed_first, first);

        let forged_cursor = encode_materialize_cursor(1, 1_000);
        assert!(matches!(
            adapter.materialize(
                &plan,
                &handle,
                &overlays,
                &staging,
                Some(&forged_cursor),
                &registry,
                &context(&budget, &token),
            ),
            Err(AdapterError::InvalidCursor)
        ));

        let mut changed_future_page = overlays.clone();
        changed_future_page[1].translated_text = "changed".to_owned();
        assert!(matches!(
            adapter.materialize(
                &plan,
                &handle,
                &changed_future_page,
                &staging,
                first.next_cursor.as_ref(),
                &registry,
                &context(&budget, &token),
            ),
            Err(AdapterError::InvalidInput(_))
        ));

        let restarted_adapter = MockTextAdapter::default();
        let second = restarted_adapter
            .materialize(
                &plan,
                &handle,
                &overlays,
                &staging,
                first.next_cursor.as_ref(),
                &registry,
                &context(&budget, &token),
            )
            .unwrap();
        assert!(second.complete);
        assert_eq!(
            registry.staging_bytes(&staging).unwrap(),
            "一\n二\n".as_bytes()
        );

        let mut changed = overlays.clone();
        changed[0].translated_text = "changed".to_owned();
        assert!(matches!(
            restarted_adapter.materialize(
                &plan,
                &handle,
                &changed,
                &staging,
                None,
                &registry,
                &context(&budget, &token),
            ),
            Err(AdapterError::InvalidInput(_))
        ));

        let mut output_limited = budget.clone();
        output_limited.maximum_bytes = 1;
        let limited_staging = registry.create_staging().unwrap();
        assert!(matches!(
            adapter.materialize(
                &plan,
                &handle,
                &overlays,
                &limited_staging,
                None,
                &registry,
                &context(&output_limited, &token),
            ),
            Err(AdapterError::BudgetExceeded)
        ));
    }

    #[test]
    fn deterministic_stage_is_reused_through_the_persistent_dag() {
        let mut connection = Connection::open_in_memory().unwrap();
        let calls = AtomicUsize::new(0);
        let key = [4; 32];
        let first = run_artifact(&mut connection, key, "worker-a", 1, 100, || {
            calls.fetch_add(1, Ordering::Relaxed);
            [5; 32]
        })
        .unwrap();
        let second = run_artifact(&mut connection, key, "worker-b", 2, 100, || {
            calls.fetch_add(1, Ordering::Relaxed);
            [6; 32]
        })
        .unwrap();
        assert_eq!(first, ArtifactOutcome::Computed([5; 32]));
        assert_eq!(second, ArtifactOutcome::Reused([5; 32]));
        assert_eq!(calls.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn adapter_envelope_uses_the_bounded_runtime_frame() {
        let envelope = AdapterEnvelope {
            request_id: 9,
            protocol_major: ADAPTER_PROTOCOL_MAJOR,
            protocol_minor: ADAPTER_PROTOCOL_MINOR,
            operation: WireOperation::Extract as i32,
            cursor: vec![1, 2],
            payload: vec![3; 128],
            maximum_response_bytes: 1024,
        };
        let mut wire = Vec::new();
        write_frame(&mut wire, &envelope).unwrap();
        let decoded: AdapterEnvelope = read_frame(&mut IoCursor::new(wire)).unwrap();
        assert_eq!(decoded.encode_to_vec(), envelope.encode_to_vec());
    }
}
