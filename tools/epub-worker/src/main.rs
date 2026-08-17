use std::{collections::HashMap, fs, io, process};

use anyhow::{Context, Result, bail};
use babel_adapter_host::CapabilityRegistry;
use babel_adapter_protocol::{
    Adapter, AdapterError, CancellationToken, Cursor, ExecutionContext, InventoryItem, Page,
    TaskBudget,
};
use babel_domain::core::{GenerationId, ResourceId};
use babel_epub_adapter::{EpubAdapter, PreparedEpub};
use babel_resource_graph::ResourceKind;
use babel_runtime::ipc::{
    Handshake, PROTOCOL_MAJOR, PROTOCOL_MINOR, WorkerRequest, WorkerResponse, read_frame,
    validate_handshake, write_frame,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tempfile::TempDir;

const MAX_EXTRACT_PAGE_BYTES: usize = 3 * 1024 * 1024;
const MAX_EXTRACT_PAGE_UNITS: usize = 2_000;

#[derive(Debug, Deserialize)]
#[serde(tag = "operation", rename_all = "kebab-case")]
enum EpubWorkerRequest {
    Probe {
        bytes: Vec<u8>,
    },
    ExtractPreview {
        bytes: Vec<u8>,
        limit: usize,
    },
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
        cursor: Option<Cursor>,
    },
    ExtractPage {
        session_id: u64,
        generation_id: [u8; 16],
        resource_id: [u8; 16],
        cursor: Option<Cursor>,
    },
}

#[derive(Debug, Serialize)]
struct ProbeReply {
    detected_media_type: Option<String>,
    reason_code: String,
    confidence_millionths: u32,
    adapter_id: String,
    adapter_build: String,
    identity_version: u32,
}

#[derive(Debug, Serialize)]
struct ExtractPreviewReply {
    lines: Vec<String>,
}

#[derive(Debug, Serialize)]
struct LoadBeginReply {
    session_id: u64,
    max_chunk_bytes: usize,
}

#[derive(Debug, Serialize)]
struct LoadChunkReply {
    received_bytes: u64,
}

#[derive(Debug, Serialize)]
struct LoadFinishReply {
    byte_length: u64,
}

#[derive(Debug, Serialize)]
struct InventoryPageReply {
    page: Page<InventoryItem>,
}

#[derive(Debug, Serialize)]
struct ExtractPageReply {
    page: Page<babel_adapter_protocol::ExtractedUnit>,
    worker_peak_rss_kib: Option<u64>,
}

struct LoadSession {
    temp: TempDir,
    path: std::path::PathBuf,
    source_hash: [u8; 32],
    byte_length: u64,
    received_bytes: u64,
}

struct ReadySession {
    temp: TempDir,
    registry: CapabilityRegistry,
    handle: babel_adapter_protocol::ObjectHandle,
    prepared: Option<PreparedEpub>,
    cached_extract: Option<CachedExtract>,
}

struct CachedExtract {
    generation_id: [u8; 16],
    resource_id: [u8; 16],
    units: Vec<babel_adapter_protocol::ExtractedUnit>,
}

enum Session {
    Loading(LoadSession),
    Ready(Box<ReadySession>),
}

struct WorkerState {
    next_session_id: u64,
    sessions: HashMap<u64, Session>,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error:#}");
        process::exit(1);
    }
}

fn run() -> Result<()> {
    let mut stdin = io::stdin().lock();
    let mut stdout = io::stdout().lock();
    let handshake: Handshake = read_frame(&mut stdin).context("read worker handshake")?;
    validate_handshake(
        &handshake,
        &handshake.session_nonce,
        &handshake.capability_token,
    )
    .context("validate worker handshake")?;
    write_frame(
        &mut stdout,
        &Handshake {
            protocol_major: PROTOCOL_MAJOR,
            protocol_minor: PROTOCOL_MINOR,
            session_nonce: handshake.session_nonce,
            capability_token: handshake.capability_token,
        },
    )
    .context("write worker handshake ack")?;

    let mut state = WorkerState {
        next_session_id: 1,
        sessions: HashMap::new(),
    };
    loop {
        let request: WorkerRequest = match read_frame(&mut stdin) {
            Ok(request) => request,
            Err(_) => return Ok(()),
        };
        let response = handle_request(&mut state, request.request_id, &request.payload);
        write_frame(&mut stdout, &response).context("write EPUB worker response")?;
    }
}

fn handle_request(state: &mut WorkerState, request_id: u64, payload: &[u8]) -> WorkerResponse {
    match handle_payload(state, payload) {
        Ok(payload) => WorkerResponse {
            request_id,
            status: 0,
            payload,
            diagnostic: String::new(),
        },
        Err(error) => WorkerResponse {
            request_id,
            status: 1,
            payload: Vec::new(),
            diagnostic: format!("{error:#}"),
        },
    }
}

fn handle_payload(state: &mut WorkerState, payload: &[u8]) -> Result<Vec<u8>> {
    let request: EpubWorkerRequest =
        serde_json::from_slice(payload).context("decode EPUB worker request")?;
    match request {
        EpubWorkerRequest::Probe { bytes } => {
            let (temp, registry, handle) = capability_fixture(&bytes)?;
            let _temp = temp;
            let adapter = EpubAdapter::new();
            let token = CancellationToken::default();
            let budget = budget(bytes.len() as u64);
            let context = ExecutionContext::new(&budget, &token);
            let probe = adapter.probe(&handle, &registry, &context)?;
            Ok(serde_json::to_vec(&ProbeReply {
                detected_media_type: probe.detected_media_type,
                reason_code: probe.reason_code,
                confidence_millionths: probe.confidence_millionths,
                adapter_id: adapter.manifest().adapter_id.clone(),
                adapter_build: adapter.manifest().adapter_build.clone(),
                identity_version: adapter.manifest().identity_version,
            })?)
        }
        EpubWorkerRequest::ExtractPreview { bytes, limit } => {
            let (temp, registry, handle) = capability_fixture(&bytes)?;
            let _temp = temp;
            let adapter = EpubAdapter::new();
            let token = CancellationToken::default();
            let budget = budget(bytes.len() as u64);
            let context = ExecutionContext::new(&budget, &token);
            let generation_id = GenerationId::new();
            let inventory = adapter.inventory(&handle, generation_id, None, &registry, &context)?;
            let resource_id = inventory
                .items
                .into_iter()
                .find_map(|item| match item {
                    InventoryItem::Node(node) if node.kind == ResourceKind::TextStream => {
                        Some(node.resource_id)
                    }
                    _ => None,
                })
                .context("EPUB inventory has no text stream")?;
            let page = adapter.extract(
                &handle,
                generation_id,
                resource_id,
                None,
                &registry,
                &context,
            )?;
            let lines = page
                .items
                .into_iter()
                .take(limit)
                .map(|unit| {
                    unit.content
                        .tokens
                        .into_iter()
                        .find_map(|token| match token {
                            babel_tir::Token::Text { text, .. } => Some(text),
                            _ => None,
                        })
                        .ok_or_else(|| anyhow::anyhow!("extracted EPUB unit has no text token"))
                })
                .collect::<Result<Vec<_>>>()?;
            Ok(serde_json::to_vec(&ExtractPreviewReply { lines })?)
        }
        EpubWorkerRequest::LoadBegin {
            source_hash_hex,
            byte_length,
        } => {
            let source_hash = decode_hash(&source_hash_hex)?;
            let temp = TempDir::new().context("create EPUB worker temp dir")?;
            let objects = temp.path().join("objects");
            let encoded = hex::encode(source_hash);
            let path = objects
                .join("sha256")
                .join(&encoded[..2])
                .join(&encoded[2..]);
            fs::create_dir_all(path.parent().context("CAS path has parent")?)?;
            fs::write(&path, []).context("initialize EPUB worker CAS object")?;
            let session_id = state.next_session_id;
            state.next_session_id = state
                .next_session_id
                .checked_add(1)
                .context("EPUB worker session id overflow")?;
            state.sessions.insert(
                session_id,
                Session::Loading(LoadSession {
                    temp,
                    path,
                    source_hash,
                    byte_length,
                    received_bytes: 0,
                }),
            );
            Ok(serde_json::to_vec(&LoadBeginReply {
                session_id,
                max_chunk_bytes: max_load_chunk_bytes(),
            })?)
        }
        EpubWorkerRequest::LoadChunk {
            session_id,
            offset,
            data_hex,
        } => {
            let session = loading_session_mut(state, session_id)?;
            if offset != session.received_bytes {
                bail!(
                    "EPUB worker load offset mismatch: expected {}, got {}",
                    session.received_bytes,
                    offset
                );
            }
            let bytes = hex::decode(data_hex).context("decode EPUB worker load chunk")?;
            if bytes.len() > max_load_chunk_bytes() {
                bail!("EPUB worker load chunk exceeds per-frame limit");
            }
            let next = session
                .received_bytes
                .checked_add(bytes.len() as u64)
                .context("EPUB worker load byte count overflow")?;
            if next > session.byte_length {
                bail!("EPUB worker load exceeds declared byte length");
            }
            use std::io::Write;
            let mut file = fs::OpenOptions::new()
                .append(true)
                .open(&session.path)
                .context("open EPUB worker CAS object for append")?;
            file.write_all(&bytes)
                .context("append EPUB worker load chunk")?;
            session.received_bytes = next;
            Ok(serde_json::to_vec(&LoadChunkReply {
                received_bytes: session.received_bytes,
            })?)
        }
        EpubWorkerRequest::LoadFinish { session_id } => {
            let loading = match state.sessions.remove(&session_id) {
                Some(Session::Loading(session)) => session,
                Some(Session::Ready(session)) => {
                    state.sessions.insert(session_id, Session::Ready(session));
                    bail!("EPUB worker session is already loaded");
                }
                None => bail!("unknown EPUB worker session {session_id}"),
            };
            if loading.received_bytes != loading.byte_length {
                bail!(
                    "EPUB worker load incomplete: expected {}, got {}",
                    loading.byte_length,
                    loading.received_bytes
                );
            }
            let actual = hash_file(&loading.path)?;
            if actual != loading.source_hash {
                bail!("EPUB worker load hash mismatch");
            }
            let registry = CapabilityRegistry::new(
                loading.temp.path().join("objects"),
                loading.temp.path().join("staging"),
            )?;
            let handle = registry.grant_object(loading.source_hash, loading.byte_length)?;
            let byte_length = loading.byte_length;
            state.sessions.insert(
                session_id,
                Session::Ready(Box::new(ReadySession {
                    temp: loading.temp,
                    registry,
                    handle,
                    prepared: None,
                    cached_extract: None,
                })),
            );
            Ok(serde_json::to_vec(&LoadFinishReply { byte_length })?)
        }
        EpubWorkerRequest::ProbeLoaded { session_id } => {
            let session = ready_session_mut(state, session_id)?;
            let adapter = EpubAdapter::new();
            let token = CancellationToken::default();
            let budget = budget(session.handle.byte_length);
            let context = ExecutionContext::new(&budget, &token);
            let (detected_media_type, reason_code, confidence_millionths) =
                match adapter.prepare(&session.handle, &session.registry, &context) {
                    Ok(prepared) => {
                        session.prepared = Some(prepared);
                        (
                            Some("application/epub+zip".to_owned()),
                            "epub-ocf-container".to_owned(),
                            1_000_000,
                        )
                    }
                    Err(AdapterError::InvalidInput(reason)) => (None, reason, 0),
                    Err(error) => return Err(error.into()),
                };
            Ok(serde_json::to_vec(&ProbeReply {
                detected_media_type,
                reason_code,
                confidence_millionths,
                adapter_id: adapter.manifest().adapter_id.clone(),
                adapter_build: adapter.manifest().adapter_build.clone(),
                identity_version: adapter.manifest().identity_version,
            })?)
        }
        EpubWorkerRequest::InventoryPage {
            session_id,
            generation_id,
            cursor,
        } => {
            let _generation = GenerationId::from_bytes(generation_id);
            let session = ready_session_mut(state, session_id)?;
            let adapter = EpubAdapter::new();
            let token = CancellationToken::default();
            let budget = budget(session.handle.byte_length);
            let context = ExecutionContext::new(&budget, &token);
            ensure_prepared(session, &adapter, &context)?;
            let page = adapter.inventory_prepared(
                session.prepared.as_ref().context("prepared EPUB missing")?,
                &session.handle,
                cursor.as_ref(),
                &context,
            )?;
            Ok(serde_json::to_vec(&InventoryPageReply { page })?)
        }
        EpubWorkerRequest::ExtractPage {
            session_id,
            generation_id,
            resource_id,
            cursor,
        } => {
            let session = ready_session_mut(state, session_id)?;
            ensure_extract_cache(session, generation_id, resource_id)?;
            let cache = session
                .cached_extract
                .as_ref()
                .context("EPUB worker extract cache missing")?;
            let start = decode_page_cursor(cursor.as_ref())?;
            if start > cache.units.len() {
                bail!("invalid EPUB worker extract cursor");
            }
            let mut end = start;
            let mut emitted_bytes = 0_usize;
            while end < cache.units.len() && end - start < MAX_EXTRACT_PAGE_UNITS {
                let unit_bytes = estimated_unit_json_bytes(&cache.units[end]);
                if unit_bytes > MAX_EXTRACT_PAGE_BYTES {
                    bail!("one EPUB unit exceeds the IPC page budget");
                }
                if end > start && emitted_bytes + unit_bytes > MAX_EXTRACT_PAGE_BYTES {
                    break;
                }
                emitted_bytes += unit_bytes;
                end += 1;
            }
            let page = Page {
                items: cache.units[start..end].to_vec(),
                next_cursor: (end < cache.units.len()).then(|| encode_page_cursor(end)),
                emitted_bytes: emitted_bytes as u64,
            };
            Ok(serde_json::to_vec(&ExtractPageReply {
                page,
                worker_peak_rss_kib: linux_peak_rss_kib(),
            })?)
        }
    }
}

fn linux_peak_rss_kib() -> Option<u64> {
    let status = fs::read_to_string("/proc/self/status").ok()?;
    status
        .lines()
        .find_map(|line| line.strip_prefix("VmHWM:"))?
        .split_ascii_whitespace()
        .next()?
        .parse()
        .ok()
}

fn loading_session_mut(state: &mut WorkerState, session_id: u64) -> Result<&mut LoadSession> {
    match state.sessions.get_mut(&session_id) {
        Some(Session::Loading(session)) => Ok(session),
        Some(Session::Ready(_)) => bail!("EPUB worker session is already loaded"),
        None => bail!("unknown EPUB worker session {session_id}"),
    }
}

fn ready_session_mut(state: &mut WorkerState, session_id: u64) -> Result<&mut ReadySession> {
    match state.sessions.get_mut(&session_id) {
        Some(Session::Ready(session)) => {
            let _keep_temp_alive = session.temp.path();
            Ok(session)
        }
        Some(Session::Loading(_)) => bail!("EPUB worker session is not loaded"),
        None => bail!("unknown EPUB worker session {session_id}"),
    }
}

fn estimated_unit_json_bytes(unit: &babel_adapter_protocol::ExtractedUnit) -> usize {
    let text_bytes = unit
        .content
        .tokens
        .iter()
        .map(|token| match token {
            babel_tir::Token::Text { text, .. } => escaped_json_len(text),
            _ => 256,
        })
        .sum::<usize>();
    let locator_bytes = match &unit.locator {
        babel_resource_graph::Locator::ArchiveMemberByteSpan { member_path, .. } => {
            escaped_json_len(member_path)
        }
        _ => 512,
    };
    1_024 + text_bytes + locator_bytes
}

fn escaped_json_len(value: &str) -> usize {
    value
        .chars()
        .map(|character| match character {
            '"' | '\\' | '\u{0008}' | '\u{000c}' | '\n' | '\r' | '\t' => 2,
            value if value <= '\u{001f}' => 6,
            value => value.len_utf8(),
        })
        .sum()
}

fn ensure_extract_cache(
    session: &mut ReadySession,
    generation_id: [u8; 16],
    resource_id: [u8; 16],
) -> Result<()> {
    if session.cached_extract.as_ref().is_some_and(|cache| {
        cache.generation_id == generation_id && cache.resource_id == resource_id
    }) {
        return Ok(());
    }
    let adapter = EpubAdapter::new();
    let token = CancellationToken::default();
    let budget = full_extract_budget(session.handle.byte_length);
    let context = ExecutionContext::new(&budget, &token);
    let generation = GenerationId::from_bytes(generation_id);
    ensure_prepared(session, &adapter, &context)?;
    let resource = ResourceId::from_bytes(resource_id);
    let units = adapter.extract_resource_prepared(
        session.prepared.as_ref().context("prepared EPUB missing")?,
        &session.handle,
        generation,
        resource,
        &session.registry,
        &context,
    )?;
    session.cached_extract = Some(CachedExtract {
        generation_id,
        resource_id,
        units,
    });
    Ok(())
}

fn ensure_prepared(
    session: &mut ReadySession,
    adapter: &EpubAdapter,
    context: &ExecutionContext<'_>,
) -> Result<()> {
    if session.prepared.is_none() {
        session.prepared = Some(adapter.prepare(&session.handle, &session.registry, context)?);
    }
    Ok(())
}

fn decode_page_cursor(cursor: Option<&Cursor>) -> Result<usize> {
    match cursor {
        None => Ok(0),
        Some(Cursor(bytes)) if bytes.len() == 8 => {
            Ok(u64::from_be_bytes(bytes.as_slice().try_into().unwrap()) as usize)
        }
        Some(_) => bail!("invalid EPUB worker page cursor"),
    }
}

fn encode_page_cursor(value: usize) -> Cursor {
    Cursor((value as u64).to_be_bytes().to_vec())
}

fn decode_hash(hex_value: &str) -> Result<[u8; 32]> {
    let bytes = hex::decode(hex_value).context("decode source hash")?;
    bytes
        .try_into()
        .map_err(|_| anyhow::anyhow!("source hash must be 32 bytes"))
}

fn hash_file(path: &std::path::Path) -> Result<[u8; 32]> {
    let mut file = fs::File::open(path).context("open loaded EPUB worker object")?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; max_load_chunk_bytes()];
    loop {
        let read = io::Read::read(&mut file, &mut buffer).context("hash EPUB worker object")?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hasher.finalize().into())
}

fn max_load_chunk_bytes() -> usize {
    1024 * 1024
}

fn capability_fixture(
    bytes: &[u8],
) -> Result<(
    TempDir,
    CapabilityRegistry,
    babel_adapter_protocol::ObjectHandle,
)> {
    if bytes.len() > babel_runtime::ipc::MAX_FRAME_BYTES {
        bail!("EPUB worker request exceeds frame-sized preview path");
    }
    let temp = TempDir::new().context("create EPUB worker temp dir")?;
    let objects = temp.path().join("objects");
    let hash: [u8; 32] = Sha256::digest(bytes).into();
    let encoded = hex::encode(hash);
    let path = objects
        .join("sha256")
        .join(&encoded[..2])
        .join(&encoded[2..]);
    fs::create_dir_all(path.parent().context("CAS path has parent")?)?;
    fs::write(&path, bytes).context("write EPUB worker CAS object")?;
    let registry = CapabilityRegistry::new(&objects, temp.path().join("staging"))?;
    let handle = registry.grant_object(hash, bytes.len() as u64)?;
    Ok((temp, registry, handle))
}

fn budget(bytes: u64) -> TaskBudget {
    TaskBudget {
        timeout_ms: 120_000,
        maximum_bytes: bytes
            .saturating_mul(8)
            .clamp(64 * 1024 * 1024, 4 * 1024 * 1024 * 1024),
        maximum_nodes: 2_000_000,
        page_bytes: 1024 * 1024,
        page_nodes: 2_000,
    }
}

fn full_extract_budget(bytes: u64) -> TaskBudget {
    TaskBudget {
        timeout_ms: 120_000,
        maximum_bytes: bytes
            .saturating_mul(8)
            .clamp(64 * 1024 * 1024, 4 * 1024 * 1024 * 1024),
        maximum_nodes: 1_000_000,
        page_bytes: 64 * 1024 * 1024,
        page_nodes: 100_000,
    }
}
