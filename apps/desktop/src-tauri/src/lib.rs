use std::{
    collections::HashMap,
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
    sync::Mutex,
    time::Duration,
};

use babel_application::{
    Kernel, ResourceQueueCursor, WorkspaceMutationReceipt, WorkspaceMutationRequest,
};
use babel_domain::core::{ProjectId, ResourceId, UnitId};
use babel_domain::workbench::{NavigationFilters, NavigationPosition, TranslationStatus, WorkspaceView};
use babel_image::{RegionRenderParameters, RenderStyle, SpatialPolygon};
use babel_ocr::{OcrDocument, OcrInputKind, OcrProfile};
use babel_resource_graph::Locator;
use babel_runtime::{
    ipc::MAX_FRAME_BYTES,
    process_worker::{ProcessWorker, WorkerCancelToken, WorkerLaunch},
};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Manager, State};

pub const IPC_SCHEMA_VERSION: u32 = 1;

struct DesktopState {
    session: Mutex<Option<Kernel>>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProjectSummary {
    project_id: String,
    root: String,
    commit_sequence: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProjectEntry {
    project_id: String,
    name: String,
    root: String,
    last_opened_at_ms: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ImportFileRequest {
    source_path: String,
    project_root: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateProjectRequest { name: String, parent_directory: String }

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WorkspaceFilesRequest { project_id: String, parent_id: String, source_paths: Vec<String> }

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateWorkspaceFileRequest { project_id: String, parent_id: String, name: String }

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WorkspaceFileRequest { project_id: String, node_id: String }

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WriteWorkspaceFileRequest { project_id: String, node_id: String, content: String, expected_modified_at_ms: Option<i64> }

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WorkspaceStateRequest { project_id: String, state: serde_json::Value }

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct WorkspaceFile { node_id: String, uri: String, name: String, content: String, readonly: bool, modified_at_ms: i64 }

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ImportResult {
    project: ProjectSummary,
    format: String,
    units: usize,
    activated: bool,
    review_required: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct UnitSummary {
    unit_id: String,
    source_unit_key: String,
    source_text: String,
    translation: Option<String>,
    local_index: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProjectTreeRequest { project_id: String }

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProjectTreeCapabilities { open: bool, create_child: bool, rename: bool, r#move: bool, delete: bool, reveal: bool, drop: bool }

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProjectTreeNode { id: String, parent_id: Option<String>, section: String, kind: String, name: String, semantic_path: String, mapped_path: Option<String>, capabilities: ProjectTreeCapabilities }

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProjectTreeSnapshot { nodes: Vec<ProjectTreeNode>, commit_sequence: i64 }

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct NavigationSummary {
    position: NavigationPositionSummary,
    client_session_id: String,
    position_sequence: u64,
    updated_at_ms: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct NavigationPositionSummary {
    schema_version: u32,
    project_id: String,
    view: WorkspaceView,
    unit_id: Option<String>,
    resource_id: Option<String>,
    region_id: Option<String>,
    scroll_anchor_unit_id: Option<String>,
    scroll_offset_px: i32,
    zoom_millionths: u32,
    filters: NavigationFiltersSummary,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct NavigationFiltersSummary {
    query: Option<String>,
    statuses: Vec<TranslationStatus>,
    only_incomplete: bool,
    only_with_issues: bool,
}

fn navigation_summary(position: NavigationPosition) -> NavigationPositionSummary {
    NavigationPositionSummary {
        schema_version: position.schema_version,
        project_id: hex::encode(position.project_id.as_bytes()),
        view: position.view,
        unit_id: position.unit_id.map(|id| hex::encode(id.as_bytes())),
        resource_id: position.resource_id.map(|id| hex::encode(id.as_bytes())),
        region_id: position.region_id.map(|id| hex::encode(id.as_bytes())),
        scroll_anchor_unit_id: position
            .scroll_anchor_unit_id
            .map(|id| hex::encode(id.as_bytes())),
        scroll_offset_px: position.scroll_offset_px,
        zoom_millionths: position.zoom_millionths,
        filters: NavigationFiltersSummary {
            query: position.filters.query,
            statuses: position.filters.statuses,
            only_incomplete: position.filters.only_incomplete,
            only_with_issues: position.filters.only_with_issues,
        },
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct WorkbenchSnapshot {
    schema_version: u32,
    project: ProjectSummary,
    navigation: Option<NavigationSummary>,
    units: Vec<UnitSummary>,
    current_unit: Option<babel_application::TranslationWorkItem>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ResourceQueueRequest {
    after_reading_order: Option<u64>,
    after_unit_id: Option<String>,
    limit: Option<usize>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ResourceQueueItem {
    generation_id: String,
    unit_id: String,
    source_unit_key: String,
    source_text: String,
    translation: Option<String>,
    reading_order: u64,
    region_id: String,
    region_semantic_path: String,
    image_resource_id: Option<String>,
    image_semantic_path: Option<String>,
    polygon: Vec<[f32; 2]>,
    coordinate_space: String,
    corrected_source_text: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ImagePreviewRequest {
    generation_id: String,
    resource_id: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ImagePreviewReply {
    media_type: String,
    byte_length: usize,
    source_hash: String,
    data_url: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ResourceQueueSummary {
    items: Vec<ResourceQueueItem>,
    next_cursor: Option<ResourceQueueCursorSummary>,
    project_commit_sequence: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ResourceQueueCursorSummary {
    reading_order: u64,
    unit_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OpenProjectRequest {
    root: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WorkbenchRequest {
    view: Option<WorkspaceView>,
    after_local_index: Option<i64>,
    limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SaveTranslationRequest {
    source_unit_key: String,
    command_id: String,
    text: String,
    document: Option<babel_tir::TranslationDocumentV1>,
    expected_revision_id: Option<i64>,
    created_at_ms: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WorkItemRequest {
    unit_id: String,
    view: Option<WorkspaceView>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HistoryCommandRequest {
    unit_id: String,
    command_id: String,
    created_at_ms: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExportRequest {
    project_id: String,
    destination_path: String,
    command_id: String,
    created_at_ms: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListExportsRequest { project_id: String }

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ValidationIssue {
    id: String,
    severity: String,
    message_key: String,
    unit_id: Option<String>,
    detail: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ValidationReport {
    issues: Vec<ValidationIssue>,
    checked_at_ms: i64,
    project_commit_sequence: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FindTermsRequest {
    text: String,
    limit: usize,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AnnotationsForUnitRequest {
    unit_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SearchProjectRequest {
    project_id: String,
    query: String,
    limit: Option<usize>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProjectSearchResult {
    unit_id: String,
    source_unit_key: String,
    source_text: String,
    translation: Option<String>,
    local_index: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct TermRecordDto {
    term_id: String,
    source_text: String,
    preferred_translation: String,
    notes: String,
    state: String,
    variants: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AnnotationRecordDto {
    annotation_id: String,
    unit_id: String,
    base_revision_id: Option<i64>,
    current_revision_id: Option<i64>,
    grapheme_start: u64,
    grapheme_end: u64,
    body: String,
    state: String,
    stale: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ExportRecord {
    id: String,
    created_at_ms: i64,
    path: String,
    format: String,
    output_hash: String,
    status: String,
    error: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SaveImageRegionEditRequest {
    generation_id: String,
    unit_id: String,
    region_id: String,
    command_id: String,
    corrected_source_text: Option<String>,
    created_at_ms: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OcrImageRegionRequest {
    generation_id: String,
    region_id: String,
    image_resource_id: String,
    profile: Option<OcrProfile>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RenderImageRegionRequest {
    generation_id: String,
    unit_id: String,
    region_id: String,
    image_resource_id: String,
    polygon: Vec<[f32; 2]>,
    translation: String,
    font_size_px: Option<f32>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RenderImageRegionReply {
    data_url: String,
    output_hash: String,
    commit_sequence: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct OcrImageRegionReply {
    document: OcrDocument,
    replayed: bool,
}

#[derive(Debug, Serialize)]
#[serde(tag = "operation", rename_all = "kebab-case")]
enum OcrWorkerRequest {
    Recognize {
        image_bytes: Vec<u8>,
        input_kind: OcrInputKind,
        media_type: String,
        source_hash_hex: String,
        profile: OcrProfile,
    },
}

#[derive(Debug, Deserialize)]
struct OcrWorkerReply {
    document: OcrDocument,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SaveNavigationRequest {
    project_id: String,
    view: WorkspaceView,
    unit_id: Option<String>,
    resource_id: Option<String>,
    region_id: Option<String>,
    scroll_anchor_unit_id: Option<String>,
    scroll_offset_px: Option<i32>,
    zoom_millionths: Option<u32>,
    client_session_id: String,
    position_sequence: u64,
    updated_at_ms: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SaveDraftRequest {
    unit_id: String,
    document: serde_json::Value,
    updated_at_ms: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SaveResult {
    accepted: bool,
    sequence: u64,
    commit_sequence: Option<i64>,
    revision_id: Option<i64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AppSettingsV1 {
    schema_version: u32,
    language: String,
    theme: String,
    density: String,
    editor_font_family: String,
    reading_font_size: f32,
    line_height: f32,
    word_wrap: bool,
    shortcut_overrides: HashMap<String, Vec<String>>,
    panel_widths: PanelWidths,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PanelWidths {
    explorer: u32,
    inspector: u32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SettingsPatch {
    language: Option<String>,
    theme: Option<String>,
    density: Option<String>,
    editor_font_family: Option<String>,
    reading_font_size: Option<f32>,
    line_height: Option<f32>,
    word_wrap: Option<bool>,
    shortcut_overrides: Option<HashMap<String, Vec<String>>>,
    panel_widths: Option<PanelWidths>,
}

impl Default for AppSettingsV1 {
    fn default() -> Self {
        Self {
            schema_version: 1,
            language: "zh-CN".to_owned(),
            theme: "system".to_owned(),
            density: "compact".to_owned(),
            editor_font_family: "\"Noto Serif SC\", \"Source Han Serif SC\", serif".to_owned(),
            reading_font_size: 18.0,
            line_height: 1.8,
            word_wrap: true,
            shortcut_overrides: HashMap::new(),
            panel_widths: PanelWidths {
                explorer: 260,
                inspector: 320,
            },
        }
    }
}

fn project_registry_path(app: &AppHandle) -> Result<PathBuf, String> {
    let directory = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("cannot resolve application data directory: {error}"))?;
    fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    Ok(directory.join("projects.json"))
}

fn settings_path(app: &AppHandle) -> Result<PathBuf, String> {
    let directory = app
        .path()
        .app_config_dir()
        .map_err(|error| format!("cannot resolve application config directory: {error}"))?;
    fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    Ok(directory.join("settings-v1.json"))
}

fn read_settings(app: &AppHandle) -> Result<AppSettingsV1, String> {
    let path = settings_path(app)?;
    let backup = path.with_extension("json.bak");
    if !path.exists() && backup.exists() {
        fs::rename(&backup, &path).map_err(|error| error.to_string())?;
    }
    if !path.exists() {
        return Ok(AppSettingsV1::default());
    }
    let bytes = fs::read(path).map_err(|error| error.to_string())?;
    let settings: AppSettingsV1 = serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
    validate_settings(&settings)?;
    Ok(settings)
}

fn write_settings(app: &AppHandle, settings: &AppSettingsV1) -> Result<(), String> {
    validate_settings(settings)?;
    let path = settings_path(app)?;
    let temporary = path.with_extension("json.tmp");
    let backup = path.with_extension("json.bak");
    let bytes = serde_json::to_vec_pretty(settings).map_err(|error| error.to_string())?;
    let mut file = File::create(&temporary).map_err(|error| error.to_string())?;
    file.write_all(&bytes).map_err(|error| error.to_string())?;
    file.sync_all().map_err(|error| error.to_string())?;
    drop(file);
    match fs::rename(&temporary, &path) {
        Ok(()) => Ok(()),
        Err(_error) if path.exists() => {
            if backup.exists() {
                fs::remove_file(&backup).map_err(|remove_error| remove_error.to_string())?;
            }
            fs::rename(&path, &backup).map_err(|rename_error| rename_error.to_string())?;
            if let Err(rename_error) = fs::rename(&temporary, &path) {
                let _ = fs::rename(&backup, &path);
                return Err(rename_error.to_string());
            }
            fs::remove_file(&backup).map_err(|remove_error| remove_error.to_string())?;
            Ok(())
        }
        Err(error) => Err(error.to_string()),
    }
}

fn validate_settings(settings: &AppSettingsV1) -> Result<(), String> {
    if settings.schema_version != 1 {
        return Err("unsupported settings schema version".to_owned());
    }
    if !matches!(settings.language.as_str(), "zh-CN" | "en-US") {
        return Err("unsupported interface language".to_owned());
    }
    if !matches!(settings.theme.as_str(), "light" | "dark" | "system") {
        return Err("unsupported theme".to_owned());
    }
    if !matches!(settings.density.as_str(), "compact" | "comfortable") {
        return Err("unsupported interface density".to_owned());
    }
    if !(12.0..=32.0).contains(&settings.reading_font_size)
        || !(1.2..=2.4).contains(&settings.line_height)
        || !(200..=480).contains(&settings.panel_widths.explorer)
        || !(240..=520).contains(&settings.panel_widths.inspector)
    {
        return Err("settings value is outside the supported range".to_owned());
    }
    Ok(())
}

fn read_project_registry(app: &AppHandle) -> Result<Vec<ProjectEntry>, String> {
    let path = project_registry_path(app)?;
    match fs::read(&path) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(|error| error.to_string()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(error) => Err(error.to_string()),
    }
}

fn register_project(app: &AppHandle, summary: &ProjectSummary) -> Result<(), String> {
    let path = project_registry_path(app)?;
    let mut entries = read_project_registry(app)?;
    let name = Path::new(&summary.root)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("未命名项目")
        .to_owned();
    let entry = ProjectEntry {
        project_id: summary.project_id.clone(),
        name,
        root: summary.root.clone(),
        last_opened_at_ms: now_millis(),
    };
    entries.retain(|item| item.project_id != entry.project_id && item.root != entry.root);
    entries.insert(0, entry);
    entries.truncate(50);
    let bytes = serde_json::to_vec_pretty(&entries).map_err(|error| error.to_string())?;
    fs::write(path, bytes).map_err(|error| error.to_string())
}

fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or_default()
}

fn decode<const N: usize>(value: &str, label: &str) -> Result<[u8; N], String> {
    let bytes =
        hex::decode(value).map_err(|error| format!("{label} must be hexadecimal: {error}"))?;
    bytes
        .try_into()
        .map_err(|_| format!("{label} must contain exactly {} bytes", N))
}

fn with_kernel<T>(
    state: &State<'_, DesktopState>,
    action: impl FnOnce(&Kernel) -> Result<T, String>,
) -> Result<T, String> {
    let guard = state
        .session
        .lock()
        .map_err(|_| "desktop session lock is poisoned".to_owned())?;
    let kernel = guard
        .as_ref()
        .ok_or_else(|| "no project is open".to_owned())?;
    action(kernel)
}

#[tauri::command]
fn open_project(
    request: OpenProjectRequest,
    app: AppHandle,
    state: State<'_, DesktopState>,
) -> Result<ProjectSummary, String> {
    let requested_root = PathBuf::from(&request.root);
    let requested_root = requested_root
        .canonicalize()
        .unwrap_or(requested_root);
    let active_project = {
        let guard = state
            .session
            .lock()
            .map_err(|_| "desktop session lock is poisoned".to_owned())?;
        guard.as_ref().and_then(|kernel| {
            let active_root = kernel.database_path().parent()?.to_path_buf();
            let active_root = active_root.canonicalize().unwrap_or(active_root);
            if active_root != requested_root {
                return None;
            }
            let commit_sequence = kernel.query().ok()?.commit_sequence().ok()?;
            Some(ProjectSummary {
                project_id: hex::encode(kernel.project_id().as_bytes()),
                root: request.root.clone(),
                commit_sequence,
            })
        })
    };
    if let Some(summary) = active_project {
        register_project(&app, &summary)?;
        return Ok(summary);
    }
    ensure_project_config(Path::new(&request.root))?;
    let kernel = Kernel::open(&request.root).map_err(|error| error.to_string())?;
    let commit_sequence = kernel
        .query()
        .map_err(|error| error.to_string())?
        .commit_sequence()
        .map_err(|error| error.to_string())?;
    let summary = ProjectSummary {
        project_id: hex::encode(kernel.project_id().as_bytes()),
        root: request.root,
        commit_sequence,
    };
    let mut guard = state
        .session
        .lock()
        .map_err(|_| "desktop session lock is poisoned".to_owned())?;
    *guard = Some(kernel);
    register_project(&app, &summary)?;
    Ok(summary)
}

#[tauri::command]
fn create_project(
    request: CreateProjectRequest,
    app: AppHandle,
    state: State<'_, DesktopState>,
) -> Result<ProjectSummary, String> {
    let name = request.name.trim();
    if name.is_empty() || Path::new(name).components().count() != 1 {
        return Err("项目名称无效".to_owned());
    }
    let root = PathBuf::from(&request.parent_directory).join(name);
    if root.exists() && fs::read_dir(&root).map_err(|e| e.to_string())?.next().is_some() {
        return Err("项目目录已存在且不为空".to_owned());
    }
    fs::create_dir_all(root.join("workspace")).map_err(|e| e.to_string())?;
    ensure_project_config(&root)?;
    fs::write(root.join("workspace/README.md"), b"# New Project\n\n").map_err(|e| e.to_string())?;
    let kernel = Kernel::open(&root).map_err(|error| error.to_string())?;
    let commit_sequence = kernel.query().map_err(|e| e.to_string())?.commit_sequence().map_err(|e| e.to_string())?;
    let summary = ProjectSummary { project_id: hex::encode(kernel.project_id().as_bytes()), root: root.to_string_lossy().into_owned(), commit_sequence };
    let mut guard = state.session.lock().map_err(|_| "desktop session lock is poisoned".to_owned())?;
    *guard = Some(kernel);
    register_project(&app, &summary)?;
    let initial_state = serde_json::json!({ "schemaVersion": 1, "tabs": [{ "id": "file:workspace/README.md", "uri": root.join("workspace/README.md").to_string_lossy(), "title": "README.md", "kind": "workspaceFile", "readonly": false, "pinned": true }], "groups": [{ "id": "primary", "tabIds": ["file:workspace/README.md"], "activeTabId": "file:workspace/README.md" }, { "id": "secondary", "tabIds": [], "activeTabId": null }], "expandedNodeIds": ["workspace-root"], "selectedNodeId": "workspace/README.md" });
    fs::write(root.join(".config/workspace-state.json"), serde_json::to_vec_pretty(&initial_state).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    Ok(summary)
}

fn ensure_project_config(root: &Path) -> Result<(), String> {
    let config = root.join(".config");
    fs::create_dir_all(&config).map_err(|e| e.to_string())?;
    let settings = config.join("settings.json");
    if !settings.exists() { fs::write(settings, b"{\n  \"schemaVersion\": 1\n}\n").map_err(|e| e.to_string())?; }
    Ok(())
}

fn workspace_root_for_kernel(kernel: &Kernel) -> Result<PathBuf, String> {
    Ok(kernel.database_path().parent().ok_or_else(|| "项目目录不可用".to_owned())?.join("workspace"))
}

fn ensure_kernel_project(kernel: &Kernel, project_id: &str) -> Result<(), String> {
    if hex::encode(kernel.project_id().as_bytes()) != project_id { return Err("项目不匹配".to_owned()); }
    Ok(())
}

fn workspace_path(root: &Path, node_id: &str) -> Result<PathBuf, String> {
    let relative = node_id.strip_prefix("workspace/").ok_or_else(|| "仅允许访问工作区文件".to_owned())?;
    if relative.is_empty() || Path::new(relative).components().any(|c| matches!(c, std::path::Component::ParentDir | std::path::Component::RootDir | std::path::Component::Prefix(_))) {
        return Err("工作区路径无效".to_owned());
    }
    Ok(root.join(relative))
}

fn parent_workspace_path(root: &Path, parent_id: &str) -> Result<PathBuf, String> {
    if parent_id == "workspace-root" { return Ok(root.to_path_buf()); }
    workspace_path(root, parent_id)
}

fn modified_at_ms(path: &Path) -> Result<i64, String> {
    let modified = fs::metadata(path).and_then(|m| m.modified()).map_err(|e| e.to_string())?;
    Ok(modified.duration_since(std::time::UNIX_EPOCH).map_err(|e| e.to_string())?.as_millis() as i64)
}

fn workspace_file(path: &Path, root: &Path, readonly: bool) -> Result<WorkspaceFile, String> {
    let relative = path.strip_prefix(root).map_err(|_| "工作区路径越界".to_owned())?.to_string_lossy().replace('\\', "/");
    let name = path.file_name().and_then(|v| v.to_str()).unwrap_or_default().to_owned();
    Ok(WorkspaceFile { node_id: format!("workspace/{relative}"), uri: path.to_string_lossy().into_owned(), name, content: fs::read_to_string(path).map_err(|e| e.to_string())?, readonly, modified_at_ms: modified_at_ms(path)? })
}

fn crypto_operation_id() -> String { hex::encode(ProjectId::new().as_bytes()) }

#[tauri::command]
fn create_workspace_file(request: CreateWorkspaceFileRequest, state: State<'_, DesktopState>) -> Result<WorkspaceMutationReceipt, String> {
    with_kernel(&state, |kernel| {
        ensure_kernel_project(kernel, &request.project_id)?;
        let root = workspace_root_for_kernel(kernel)?;
        if request.name.trim().is_empty() || Path::new(&request.name).components().count() != 1 { return Err("文件名称无效".to_owned()); }
        let parent = parent_workspace_path(&root, &request.parent_id)?;
        if !parent.is_dir() { return Err("目标文件夹不存在".to_owned()); }
        let path = parent.join(request.name.trim());
        if path.exists() { return Err("文件已存在".to_owned()); }
        fs::write(&path, b"").map_err(|e| e.to_string())?;
        Ok(WorkspaceMutationReceipt { operation_id: crypto_operation_id(), commit_sequence: kernel.query().map_err(|e| e.to_string())?.commit_sequence().map_err(|e| e.to_string())?, affected_node_ids: vec![format!("workspace/{}", path.strip_prefix(&root).unwrap().to_string_lossy().replace('\\', "/"))] })
    })
}

#[tauri::command]
fn import_workspace_files(request: WorkspaceFilesRequest, state: State<'_, DesktopState>) -> Result<WorkspaceMutationReceipt, String> {
    with_kernel(&state, |kernel| {
        ensure_kernel_project(kernel, &request.project_id)?;
        let root = workspace_root_for_kernel(kernel)?;
        let parent = parent_workspace_path(&root, &request.parent_id)?;
        if !parent.is_dir() { return Err("目标文件夹不存在".to_owned()); }
        let mut affected = Vec::new();
        for source in request.source_paths {
            let source = PathBuf::from(source);
            let name = source.file_name().and_then(|v| v.to_str()).ok_or_else(|| "导入文件名无效".to_owned())?;
            let destination = parent.join(name);
            fs::copy(&source, &destination).map_err(|e| e.to_string())?;
            affected.push(format!("workspace/{}", destination.strip_prefix(&root).unwrap().to_string_lossy().replace('\\', "/")));
        }
        Ok(WorkspaceMutationReceipt { operation_id: crypto_operation_id(), commit_sequence: kernel.query().map_err(|e| e.to_string())?.commit_sequence().map_err(|e| e.to_string())?, affected_node_ids: affected })
    })
}

#[tauri::command]
fn read_workspace_file(request: WorkspaceFileRequest, state: State<'_, DesktopState>) -> Result<WorkspaceFile, String> {
    with_kernel(&state, |kernel| { ensure_kernel_project(kernel, &request.project_id)?; let root = workspace_root_for_kernel(kernel)?; let path = workspace_path(&root, &request.node_id)?; if !path.is_file() { return Err("文件不存在".to_owned()); } workspace_file(&path, &root, false) })
}

#[tauri::command]
fn write_workspace_file(request: WriteWorkspaceFileRequest, state: State<'_, DesktopState>) -> Result<WorkspaceFile, String> {
    with_kernel(&state, |kernel| {
        ensure_kernel_project(kernel, &request.project_id)?;
        let root = workspace_root_for_kernel(kernel)?;
        let path = workspace_path(&root, &request.node_id)?;
        if !path.is_file() { return Err("文件不存在".to_owned()); }
        if let Some(expected) = request.expected_modified_at_ms { if modified_at_ms(&path)? != expected { return Err("文件已在外部发生变化".to_owned()); } }
        fs::write(&path, request.content).map_err(|e| e.to_string())?;
        workspace_file(&path, &root, false)
    })
}

#[tauri::command]
fn read_workspace_state(request: WorkspaceFileRequest, state: State<'_, DesktopState>) -> Result<Option<serde_json::Value>, String> {
    with_kernel(&state, |kernel| { let project_id = hex::encode(kernel.project_id().as_bytes()); if request.project_id != project_id { return Err("项目不匹配".to_owned()); } let path = kernel.database_path().parent().ok_or_else(|| "项目目录不可用".to_owned())?.join(".config/workspace-state.json"); match fs::read_to_string(path) { Ok(text) => serde_json::from_str(&text).map(Some).map_err(|e| e.to_string()), Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None), Err(e) => Err(e.to_string()) } })
}

#[tauri::command]
fn write_workspace_state(request: WorkspaceStateRequest, state: State<'_, DesktopState>) -> Result<(), String> {
    with_kernel(&state, |kernel| { let project_id = hex::encode(kernel.project_id().as_bytes()); if request.project_id != project_id { return Err("项目不匹配".to_owned()); } let config = kernel.database_path().parent().ok_or_else(|| "项目目录不可用".to_owned())?.join(".config"); fs::create_dir_all(&config).map_err(|e| e.to_string())?; fs::write(config.join("workspace-state.json"), serde_json::to_vec_pretty(&request.state).map_err(|e| e.to_string())?).map_err(|e| e.to_string()) })
}

#[tauri::command]
fn list_projects(app: AppHandle) -> Result<Vec<ProjectEntry>, String> {
    read_project_registry(&app)
}

#[tauri::command]
fn import_file(
    request: ImportFileRequest,
    app: AppHandle,
    state: State<'_, DesktopState>,
) -> Result<ImportResult, String> {
    let source_path = PathBuf::from(&request.source_path);
    let format = source_path
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase)
        .ok_or_else(|| "源文件缺少扩展名".to_owned())?;
    ensure_project_config(Path::new(&request.project_root))?;
    let kernel = Kernel::open(&request.project_root).map_err(|error| error.to_string())?;
    let source_id = *ProjectId::new().as_bytes();
    let report = match format.as_str() {
        "txt" => {
            let mut source = File::open(&source_path).map_err(|error| error.to_string())?;
            kernel
                .import_txt_reader(source_id, &mut source, now_millis())
                .map_err(|error| error.to_string())?
        }
        "md" | "markdown" => kernel
            .import_markdown_path(source_id, &source_path, now_millis())
            .map_err(|error| error.to_string())?,
        "epub" => {
            let mut source = File::open(&source_path).map_err(|error| error.to_string())?;
            kernel
                .import_epub_reader(source_id, &mut source, now_millis())
                .map_err(|error| error.to_string())?
        }
        _ => return Err("仅支持 TXT、Markdown 和 EPUB 文件".to_owned()),
    };
    let summary = ProjectSummary {
        project_id: hex::encode(kernel.project_id().as_bytes()),
        root: request.project_root,
        commit_sequence: kernel
            .query()
            .map_err(|error| error.to_string())?
            .commit_sequence()
            .map_err(|error| error.to_string())?,
    };
    let mut guard = state
        .session
        .lock()
        .map_err(|_| "desktop session lock is poisoned".to_owned())?;
    *guard = Some(kernel);
    register_project(&app, &summary)?;
    Ok(ImportResult {
        project: summary,
        format,
        units: report.units,
        activated: report.activated,
        review_required: report.review_required,
    })
}

#[tauri::command]
fn workbench_snapshot(
    request: WorkbenchRequest,
    state: State<'_, DesktopState>,
) -> Result<WorkbenchSnapshot, String> {
    with_kernel(&state, |kernel| {
        let query = kernel.query().map_err(|error| error.to_string())?;
        let commit_sequence = query.commit_sequence().map_err(|error| error.to_string())?;
        let limit = request.limit.unwrap_or(50).clamp(1, 256);
        let units = query
            .page_after(request.after_local_index.unwrap_or(-1), limit)
            .map_err(|error| error.to_string())?
            .into_iter()
            .map(|unit| UnitSummary {
                unit_id: hex::encode(unit.unit_id),
                source_unit_key: hex::encode(unit.source_unit_key),
                source_text: unit.source_text,
                translation: unit.translation,
                local_index: unit.local_index,
            })
            .collect();
        let saved_navigation = query
            .navigation_position()
            .map_err(|error| error.to_string())?;
        let navigation = saved_navigation.as_ref().map(|saved| NavigationSummary {
                position: navigation_summary(saved.position.clone()),
                client_session_id: saved.client_session_id.clone(),
                position_sequence: saved.position_sequence,
                updated_at_ms: saved.updated_at_ms,
            });
        let current_unit = saved_navigation
            .as_ref()
            .and_then(|saved| saved.position.unit_id)
            .map(|unit_id| {
                kernel
                    .translation_work_item(unit_id, request.view.unwrap_or(WorkspaceView::LongForm))
            })
            .transpose()
            .map_err(|error| error.to_string())?;
        Ok(WorkbenchSnapshot {
            schema_version: IPC_SCHEMA_VERSION,
            project: ProjectSummary {
                project_id: hex::encode(kernel.project_id().as_bytes()),
                root: kernel
                    .database_path()
                    .parent()
                    .unwrap_or_else(|| std::path::Path::new("."))
                    .to_string_lossy()
                    .into_owned(),
                commit_sequence,
            },
            navigation,
            units,
            current_unit,
        })
    })
}

#[tauri::command]
fn project_tree(request: ProjectTreeRequest, state: State<'_, DesktopState>) -> Result<ProjectTreeSnapshot, String> {
    with_kernel(&state, |kernel| {
        let project_id = hex::encode(kernel.project_id().as_bytes());
        if request.project_id != project_id { return Err("资源树项目与当前打开项目不一致".to_owned()); }
        let query = kernel.query().map_err(|error| error.to_string())?;
        let commit_sequence = query.commit_sequence().map_err(|error| error.to_string())?;
        let database_path = kernel.database_path();
        let root = database_path.parent().ok_or_else(|| "项目目录不可用".to_owned())?;
        let mut nodes = Vec::new();
        nodes.push(ProjectTreeNode { id: "source-root".to_owned(), parent_id: None, section: "source".to_owned(), kind: "root".to_owned(), name: "source".to_owned(), semantic_path: ".".to_owned(), mapped_path: None, capabilities: ProjectTreeCapabilities { open: false, create_child: false, rename: false, r#move: false, delete: false, reveal: false, drop: false } });
        let mut after_local_index = -1;
        loop {
            let page = query.page_after(after_local_index, 256).map_err(|error| error.to_string())?;
            if page.is_empty() { break; }
            for unit in &page {
                nodes.push(ProjectTreeNode { id: hex::encode(&unit.unit_id), parent_id: Some("source-root".to_owned()), section: "source".to_owned(), kind: "text".to_owned(), name: if unit.source_text.is_empty() { format!("#{}", unit.local_index + 1) } else { unit.source_text.chars().take(48).collect() }, semantic_path: hex::encode(&unit.source_unit_key), mapped_path: None, capabilities: ProjectTreeCapabilities { open: true, create_child: false, rename: false, r#move: false, delete: false, reveal: false, drop: false } });
            }
            after_local_index = page.last().map(|unit| unit.local_index).unwrap_or(after_local_index);
            if page.len() < 256 { break; }
        }
        for (id, section, name, directory) in [
            ("workspace-root", "workspace", "workspace", root.join("workspace")),
            ("recycle-root", "workspace", "recycle", root.join("recycle")),
            ("derived-root", "derived", "derived", root.join("derived")),
        ] {
            nodes.push(ProjectTreeNode { id: id.to_owned(), parent_id: None, section: section.to_owned(), kind: "root".to_owned(), name: name.to_owned(), semantic_path: ".".to_owned(), mapped_path: Some(directory.to_string_lossy().into_owned()), capabilities: ProjectTreeCapabilities { open: false, create_child: id == "workspace-root", rename: false, r#move: false, delete: false, reveal: true, drop: id == "workspace-root" } });
            collect_project_tree_nodes(&directory, id, section, if id == "recycle-root" { "recycle" } else if id == "derived-root" { "derived" } else { "workspace" }, &directory, &mut nodes)?;
        }
        Ok(ProjectTreeSnapshot { nodes, commit_sequence })
    })
}

fn collect_project_tree_nodes(directory: &Path, parent_id: &str, section: &str, prefix: &str, base: &Path, nodes: &mut Vec<ProjectTreeNode>) -> Result<(), String> {
    if !directory.exists() { return Ok(()); }
    let mut entries = fs::read_dir(directory).map_err(|error| error.to_string())?.collect::<Result<Vec<_>, _>>().map_err(|error| error.to_string())?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let metadata = fs::symlink_metadata(entry.path()).map_err(|error| error.to_string())?;
        if metadata.file_type().is_symlink() { continue; }
        let relative = entry.path().strip_prefix(base).map_err(|error| error.to_string())?.to_string_lossy().replace('\\', "/");
        let node_id = format!("{prefix}/{relative}");
        let is_directory = metadata.is_dir();
        let recycle = prefix == "recycle";
        let open = !is_directory && !recycle && is_workspace_text_file(&entry.path());
        nodes.push(ProjectTreeNode { id: node_id.clone(), parent_id: Some(parent_id.to_owned()), section: section.to_owned(), kind: if is_directory { "folder" } else { "resource" }.to_owned(), name: entry.file_name().to_string_lossy().into_owned(), semantic_path: relative, mapped_path: Some(entry.path().to_string_lossy().into_owned()), capabilities: ProjectTreeCapabilities { open, create_child: is_directory && !recycle, rename: !recycle, r#move: !recycle, delete: !recycle, reveal: true, drop: is_directory && !recycle } });
        if is_directory { collect_project_tree_nodes(&entry.path(), &node_id, section, prefix, base, nodes)?; }
    }
    Ok(())
}

fn is_workspace_text_file(path: &Path) -> bool {
    matches!(path.extension().and_then(|value| value.to_str()).map(str::to_ascii_lowercase).as_deref(), Some("txt" | "md" | "markdown" | "json" | "yaml" | "yml" | "toml" | "csv"))
}

#[tauri::command]
fn resource_queue(
    request: ResourceQueueRequest,
    state: State<'_, DesktopState>,
) -> Result<ResourceQueueSummary, String> {
    with_kernel(&state, |kernel| {
        let after = match (request.after_reading_order, request.after_unit_id) {
            (None, None) => None,
            (Some(reading_order), Some(unit_id)) => Some(ResourceQueueCursor {
                reading_order,
                unit_id: UnitId::from_bytes(decode::<16>(&unit_id, "afterUnitId")?),
            }),
            _ => return Err("资源队列游标必须同时包含 readingOrder 和 unitId".to_owned()),
        };
        let page = kernel
            .resource_queue(after, request.limit.unwrap_or(100))
            .map_err(|error| error.to_string())?;
        let items = page
            .items
            .into_iter()
            .map(|item| {
                let edit = kernel
                    .image_region_edit(item.unit_id)
                    .map_err(|error| error.to_string())?;
                let (region_id, region_semantic_path) = item
                    .resources
                    .iter()
                    .find(|resource| resource.kind == "ImageRegion")
                    .map(|resource| {
                        (
                            hex::encode(resource.resource_id.as_bytes()),
                            resource.semantic_path.clone(),
                        )
                    })
                    .ok_or_else(|| "资源队列工作项缺少 ImageRegion".to_owned())?;
                let image = item.resources.iter().find(|resource| {
                    resource.kind == "Image" && resource.relation == "RegionOf"
                });
                let (polygon, coordinate_space) = match item.locator {
                    Locator::SpatialRegion {
                        polygon,
                        coordinate_space,
                        ..
                    } => (polygon, coordinate_space),
                    _ => return Err("图片区域工作项缺少空间定位".to_owned()),
                };
                Ok(ResourceQueueItem {
                    generation_id: hex::encode(item.generation_id),
                    unit_id: hex::encode(item.unit_id.as_bytes()),
                    source_unit_key: hex::encode(item.source_unit_key),
                    source_text: item.source_text,
                    translation: item.translation,
                    reading_order: item.reading_order,
                    region_id,
                    region_semantic_path,
                    image_resource_id: image
                        .map(|resource| hex::encode(resource.resource_id.as_bytes())),
                    image_semantic_path: image.map(|resource| resource.semantic_path.clone()),
                    polygon,
                    coordinate_space,
                    corrected_source_text: edit.and_then(|record| record.corrected_source_text),
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        Ok(ResourceQueueSummary {
            items,
            next_cursor: page.next_cursor.map(|cursor| ResourceQueueCursorSummary {
                reading_order: cursor.reading_order,
                unit_id: hex::encode(cursor.unit_id.as_bytes()),
            }),
            project_commit_sequence: page.project_commit_sequence,
        })
    })
}

#[tauri::command]
fn save_image_region_edit(
    request: SaveImageRegionEditRequest,
    state: State<'_, DesktopState>,
) -> Result<SaveResult, String> {
    with_kernel(&state, |kernel| {
        let receipt = kernel
            .save_image_region_edit(babel_application::SaveImageRegionEditRequest {
                unit_id: decode::<16>(&request.unit_id, "unitId")?,
                generation_id: decode::<16>(&request.generation_id, "generationId")?,
                region_resource_id: decode::<16>(&request.region_id, "regionId")?,
                command_id: decode::<32>(&request.command_id, "commandId")?,
                corrected_source_text: request.corrected_source_text,
                render_parameters_json: br#"{"schema_version":1,"font_state":"unconfigured"}"#
                    .to_vec(),
                derived_object_hash: None,
                created_at_ms: request.created_at_ms,
            })
            .map_err(|error| error.to_string())?;
        Ok(SaveResult {
            accepted: true,
            sequence: receipt.revision_id as u64,
            commit_sequence: Some(receipt.commit_sequence),
            revision_id: Some(receipt.revision_id),
        })
    })
}

#[tauri::command]
fn image_preview(
    request: ImagePreviewRequest,
    state: State<'_, DesktopState>,
) -> Result<ImagePreviewReply, String> {
    with_kernel(&state, |kernel| {
        let preview = kernel
            .read_image_preview(
                decode::<16>(&request.generation_id, "generationId")?,
                decode::<16>(&request.resource_id, "resourceId")?,
            )
            .map_err(|error| error.to_string())?;
        Ok(ImagePreviewReply {
            data_url: format!("data:{};base64,{}", preview.media_type, preview.data_base64),
            media_type: preview.media_type,
            byte_length: preview.byte_length,
            source_hash: hex::encode(preview.source_hash),
        })
    })
}

fn first_existing_path(candidates: impl IntoIterator<Item = PathBuf>) -> Result<PathBuf, String> {
    candidates
        .into_iter()
        .find(|path| path.is_file())
        .ok_or_else(|| "OCR worker 不存在，请先构建或安装 OCR 运行组件".to_owned())
}

fn ocr_worker_path(app: &AppHandle) -> Result<PathBuf, String> {
    if let Some(path) = std::env::var_os("BABEL_OCR_WORKER") {
        return first_existing_path([PathBuf::from(path)]);
    }
    let name = if cfg!(windows) {
        "babel-ocr-worker.exe"
    } else {
        "babel-ocr-worker"
    };
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let resource_dir = app
        .path()
        .resource_dir()
        .map_err(|error| format!("无法解析 OCR 资源目录: {error}"))?;
    first_existing_path([
        resource_dir.join(name),
        manifest_dir.join("../../../target/debug").join(name),
        manifest_dir.join("../../../target/release").join(name),
    ])
}

fn ocr_asset_root(app: &AppHandle) -> Result<PathBuf, String> {
    if let Some(path) = std::env::var_os("BABEL_OCR_ASSET_ROOT") {
        let root = PathBuf::from(path);
        if root.join("manifest.json").is_file() {
            return Ok(root);
        }
    }
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let resource_dir = app
        .path()
        .resource_dir()
        .map_err(|error| format!("无法解析 OCR 资源目录: {error}"))?;
    [
        resource_dir.join("ocr/ppocrv6-tiny"),
        manifest_dir.join("../../../resources/ocr/ppocrv6-tiny"),
    ]
    .into_iter()
    .find(|root| root.join("manifest.json").is_file())
        .ok_or_else(|| "OCR 模型资产清单不存在".to_owned())
}

fn image_font_path(app: &AppHandle) -> Result<PathBuf, String> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut candidates = Vec::new();
    if let Some(path) = std::env::var_os("BABEL_IMAGE_FONT") {
        candidates.push(PathBuf::from(path));
    }
    if let Ok(resource_dir) = app.path().resource_dir() {
        candidates.push(resource_dir.join("fonts/BabelSans.ttf"));
    }
    if cfg!(windows) {
        candidates.extend([
            PathBuf::from(r"C:\Windows\Fonts\arial.ttf"),
            PathBuf::from(r"C:\Windows\Fonts\segoeui.ttf"),
        ]);
    } else {
        candidates.extend([
            PathBuf::from("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"),
            PathBuf::from("/usr/share/fonts/truetype/liberation2/LiberationSans-Regular.ttf"),
        ]);
    }
    candidates.push(manifest_dir.join("../../../resources/fonts/BabelSans.ttf"));
    candidates
        .into_iter()
        .find(|path| path.is_file())
        .ok_or_else(|| "未找到嵌字字体，请配置 BABEL_IMAGE_FONT 或安装应用字体资产".to_owned())
}

fn render_image_region_request(
    app: &AppHandle,
    kernel: &Kernel,
    request: RenderImageRegionRequest,
) -> Result<RenderImageRegionReply, String> {
    if request.translation.trim().is_empty() {
        return Err("请先填写人工译文".to_owned());
    }
    let generation_id = decode::<16>(&request.generation_id, "generationId")?;
    let unit_id = decode::<16>(&request.unit_id, "unitId")?;
    let region_id = decode::<16>(&request.region_id, "regionId")?;
    let image_resource_id = decode::<16>(&request.image_resource_id, "imageResourceId")?;
    let preview = kernel
        .read_image_preview(generation_id, image_resource_id)
        .map_err(|error| error.to_string())?;
    let source_bytes = BASE64
        .decode(preview.data_base64.as_bytes())
        .map_err(|error| format!("图片预览解码失败: {error}"))?;
    let font_bytes = fs::read(image_font_path(app)?).map_err(|error| format!("读取嵌字字体失败: {error}"))?;
    let font_hash: [u8; 32] = Sha256::digest(&font_bytes).into();
    let font_size_px = request.font_size_px.unwrap_or(22.0).clamp(8.0, 96.0);
    let parameters = RegionRenderParameters {
        schema_version: 1,
        font_object_hash: font_hash,
        font_size_millipx: (font_size_px * 1000.0) as u32,
        text_color: [255, 255, 255, 255],
        padding_px: 6,
        background_color: None,
    };
    let rendered = babel_image::render_png(
        &source_bytes,
        preview.source_hash,
        &SpatialPolygon { points: request.polygon },
        &request.translation,
        &RenderStyle {
            font_bytes,
            font_size_px,
            text_color: parameters.text_color,
            padding_px: parameters.padding_px,
            background_color: parameters.background_color,
        },
    )
    .map_err(|error| error.to_string())?;
    let now = now_millis();
    let published = kernel
        .publish_source(region_id, "image/png".to_owned(), rendered.bytes.clone(), now)
        .map_err(|error| error.to_string())?;
    let command_id: [u8; 32] = Sha256::digest(
        [b"babel-image-render-v1".as_slice(), &published.hash, &unit_id].concat(),
    )
    .into();
    let receipt = kernel
        .save_image_region_edit(babel_application::SaveImageRegionEditRequest {
            unit_id,
            generation_id,
            region_resource_id: region_id,
            command_id,
            corrected_source_text: None,
            render_parameters_json: serde_json::to_vec(&parameters)
                .map_err(|error| error.to_string())?,
            derived_object_hash: Some(published.hash),
            created_at_ms: now,
        })
        .map_err(|error| error.to_string())?;
    Ok(RenderImageRegionReply {
        data_url: format!("data:image/png;base64,{}", BASE64.encode(rendered.bytes)),
        output_hash: hex::encode(published.hash),
        commit_sequence: receipt.commit_sequence,
    })
}

#[tauri::command]
fn render_image_region(
    app: AppHandle,
    request: RenderImageRegionRequest,
    state: State<'_, DesktopState>,
) -> Result<RenderImageRegionReply, String> {
    with_kernel(&state, |kernel| render_image_region_request(&app, kernel, request))
}

fn recognize_image_region(
    app: &AppHandle,
    kernel: &Kernel,
    request: OcrImageRegionRequest,
) -> Result<OcrImageRegionReply, String> {
    let generation_id = decode::<16>(&request.generation_id, "generationId")?;
    let region_id = decode::<16>(&request.region_id, "regionId")?;
    let image_resource_id = decode::<16>(&request.image_resource_id, "imageResourceId")?;
    let preview = kernel
        .read_image_preview(generation_id, image_resource_id)
        .map_err(|error| error.to_string())?;
    let image_bytes = BASE64
        .decode(preview.data_base64.as_bytes())
        .map_err(|error| format!("图片预览解码失败: {error}"))?;
    let profile = request.profile.unwrap_or_default();
    profile.validate().map_err(|error| error.to_string())?;
    let asset_root = ocr_asset_root(app)?;
    let manifest_path = asset_root.join("manifest.json");
    let manifest_bytes = fs::read(&manifest_path).map_err(|error| error.to_string())?;
    let model_hash: [u8; 32] = Sha256::digest(&manifest_bytes).into();
    let worker_path = ocr_worker_path(app)?;
    let cancel = WorkerCancelToken::new();
    let mut launch = WorkerLaunch::new(worker_path, b"babel-ocr-tauri-v1".to_vec());
    launch.handshake_timeout = Duration::from_secs(15);
    launch.request_timeout = Duration::from_secs(90);
    launch.max_response_bytes = MAX_FRAME_BYTES;
    launch = launch
        .env("BABEL_OCR_MANIFEST", &manifest_path)
        .env("BABEL_OCR_ASSET_ROOT", &asset_root);
    let runtime_dir = asset_root.join("runtime");
    if cfg!(windows) {
        let current_path = std::env::var_os("PATH").unwrap_or_default();
        let mut path = runtime_dir.clone().into_os_string();
        path.push(if cfg!(windows) { ";" } else { ":" });
        path.push(current_path);
        launch = launch.env("PATH", path);
    } else {
        launch = launch.env("LD_LIBRARY_PATH", runtime_dir);
    }
    let mut worker = ProcessWorker::spawn(launch, &cancel).map_err(|error| error.to_string())?;
    let payload = serde_json::to_vec(&OcrWorkerRequest::Recognize {
        image_bytes,
        input_kind: OcrInputKind::Image,
        media_type: preview.media_type,
        source_hash_hex: hex::encode(preview.source_hash),
        profile,
    })
    .map_err(|error| error.to_string())?;
    let response = worker
        .request(1, payload, &cancel)
        .map_err(|error| error.to_string())?;
    if response.status != 0 {
        return Err(if response.diagnostic.is_empty() {
            "OCR worker 识别失败".to_owned()
        } else {
            response.diagnostic
        });
    }
    let reply: OcrWorkerReply = serde_json::from_slice(&response.payload)
        .map_err(|error| format!("OCR worker 返回无效结果: {error}"))?;
    let candidate_json = serde_json::to_vec(&reply.document).map_err(|error| error.to_string())?;
    let replayed = kernel
        .save_ocr_candidate(babel_application::SaveOcrCandidateRequest {
            generation_id,
            region_resource_id: region_id,
            model_hash,
            candidate_json,
            created_at_ms: now_millis(),
        })
        .map_err(|error| error.to_string())?;
    Ok(OcrImageRegionReply {
        document: reply.document,
        replayed,
    })
}

#[tauri::command]
fn ocr_image_region(
    app: AppHandle,
    request: OcrImageRegionRequest,
    state: State<'_, DesktopState>,
) -> Result<OcrImageRegionReply, String> {
    with_kernel(&state, |kernel| recognize_image_region(&app, kernel, request))
}

#[tauri::command]
fn save_translation(
    request: SaveTranslationRequest,
    state: State<'_, DesktopState>,
) -> Result<SaveResult, String> {
    with_kernel(&state, |kernel| {
        let source_unit_key = decode::<32>(&request.source_unit_key, "sourceUnitKey")?;
        let command_id = decode::<32>(&request.command_id, "commandId")?;
        let receipt = if let Some(document) = request.document {
            kernel.save_translation_document(
                source_unit_key,
                command_id,
                request.expected_revision_id,
                document,
                request.created_at_ms,
            )
        } else {
            kernel.save_translation(source_unit_key, command_id, request.text, request.created_at_ms)
        }
        .map_err(|error| error.to_string())?;
        Ok(SaveResult {
            accepted: true,
            sequence: receipt.revision_id as u64,
            commit_sequence: Some(receipt.commit_sequence),
            revision_id: Some(receipt.revision_id),
        })
    })
}

#[tauri::command]
fn save_draft(
    request: SaveDraftRequest,
    state: State<'_, DesktopState>,
) -> Result<(), String> {
    with_kernel(&state, |kernel| {
        let patch = serde_json::to_vec(&request.document).map_err(|error| error.to_string())?;
        kernel
            .save_draft(
                decode::<16>(&request.unit_id, "unitId")?.to_vec(),
                None,
                "desktop-session".to_owned(),
                patch,
                request.updated_at_ms,
            )
            .map_err(|error| error.to_string())
    })
}

#[tauri::command]
fn work_item(
    request: WorkItemRequest,
    state: State<'_, DesktopState>,
) -> Result<babel_application::TranslationWorkItem, String> {
    with_kernel(&state, |kernel| {
        kernel
            .translation_work_item(
                UnitId::from_bytes(decode::<16>(&request.unit_id, "unitId")?),
                request.view.unwrap_or(WorkspaceView::LongForm),
            )
            .map_err(|error| error.to_string())
    })
}

#[tauri::command]
fn undo_translation(
    request: HistoryCommandRequest,
    state: State<'_, DesktopState>,
) -> Result<SaveResult, String> {
    with_kernel(&state, |kernel| {
        let receipt = kernel
            .undo_translation(
                UnitId::from_bytes(decode::<16>(&request.unit_id, "unitId")?),
                decode::<32>(&request.command_id, "commandId")?,
                request.created_at_ms,
            )
            .map_err(|error| error.to_string())?;
        Ok(SaveResult {
            accepted: true,
            sequence: receipt.revision_id as u64,
            commit_sequence: Some(receipt.commit_sequence),
            revision_id: Some(receipt.revision_id),
        })
    })
}

#[tauri::command]
fn redo_translation(
    request: HistoryCommandRequest,
    state: State<'_, DesktopState>,
) -> Result<SaveResult, String> {
    with_kernel(&state, |kernel| {
        let receipt = kernel
            .redo_translation(
                UnitId::from_bytes(decode::<16>(&request.unit_id, "unitId")?),
                decode::<32>(&request.command_id, "commandId")?,
                request.created_at_ms,
            )
            .map_err(|error| error.to_string())?;
        Ok(SaveResult {
            accepted: true,
            sequence: receipt.revision_id as u64,
            commit_sequence: Some(receipt.commit_sequence),
            revision_id: Some(receipt.revision_id),
        })
    })
}

fn active_format(kernel: &Kernel) -> Result<&'static str, String> {
    for format in ["txt", "markdown", "epub"] {
        if kernel.validate_active_format_id(format).is_ok() {
            return Ok(format);
        }
    }
    Err("无法识别当前项目格式".to_owned())
}

#[tauri::command]
fn find_terms(request: FindTermsRequest, state: State<'_, DesktopState>) -> Result<Vec<TermRecordDto>, String> {
    with_kernel(&state, |kernel| {
        kernel
            .find_terms(request.text, request.limit.clamp(1, 100))
            .map_err(|error| error.to_string())
            .map(|records| {
                records
                    .into_iter()
                    .map(|record| TermRecordDto {
                        term_id: hex::encode(record.term_id),
                        source_text: record.source_text,
                        preferred_translation: record.preferred_translation,
                        notes: record.notes,
                        state: record.state,
                        variants: record.variants,
                    })
                    .collect()
            })
    })
}

#[tauri::command]
fn search_project(
    request: SearchProjectRequest,
    state: State<'_, DesktopState>,
) -> Result<Vec<ProjectSearchResult>, String> {
    with_kernel(&state, |kernel| {
        let project_id = ProjectId::from_bytes(decode::<16>(&request.project_id, "projectId")?);
        if project_id != kernel.project_id() {
            return Err("搜索项目与当前打开项目不一致".to_owned());
        }
        let query = kernel.search(request.query, request.limit.unwrap_or(50).clamp(1, 100))
            .map_err(|error| error.to_string())?;
        let project_query = kernel.query().map_err(|error| error.to_string())?;
        let results = query.into_iter().filter_map(|unit_id| {
            let bytes = decode::<16>(&unit_id, "unitId").ok()?;
            let record = project_query.workbench_unit(&bytes).ok()??;
            Some(ProjectSearchResult {
                unit_id: hex::encode(record.unit_id),
                source_unit_key: hex::encode(record.source_unit_key),
                source_text: record.source_text,
                translation: record.translation,
                local_index: record.reading_order as i64,
            })
        }).collect::<Vec<_>>();
        Ok(results)
    })
}

#[tauri::command]
fn annotations_for_unit(
    request: AnnotationsForUnitRequest,
    state: State<'_, DesktopState>,
) -> Result<Vec<AnnotationRecordDto>, String> {
    with_kernel(&state, |kernel| {
        let unit_id = decode::<16>(&request.unit_id, "unitId")?;
        kernel
            .annotations_for_unit(unit_id.to_vec())
            .map_err(|error| error.to_string())
            .map(|records| {
                records
                    .into_iter()
                    .map(|record| AnnotationRecordDto {
                        annotation_id: hex::encode(record.annotation_id),
                        unit_id: hex::encode(record.unit_id),
                        base_revision_id: record.base_revision_id,
                        current_revision_id: record.current_revision_id,
                        grapheme_start: record.grapheme_start,
                        grapheme_end: record.grapheme_end,
                        body: record.body,
                        state: record.state,
                        stale: record.stale,
                    })
                    .collect()
            })
    })
}

#[tauri::command]
fn validate_project(state: State<'_, DesktopState>) -> Result<ValidationReport, String> {
    with_kernel(&state, |kernel| {
        let format = active_format(kernel)?;
        let query = kernel.query().map_err(|error| error.to_string())?;
        let issues = kernel
            .validate_active_format_id(format)
            .map_err(|error| error.to_string())?
            .into_iter()
            .map(|issue| -> Result<ValidationIssue, String> { Ok(ValidationIssue {
                id: format!("{}:{}", hex::encode(issue.source_unit_key), issue.code),
                severity: if issue.code == "missing-translation" { "blocking" } else { "warning" }.to_owned(),
                message_key: if issue.code == "missing-translation" { "missingTranslation" } else { "formatIssue" }.to_owned(),
                unit_id: query
                    .unit_id_for_source_key(&issue.source_unit_key)
                    .map_err(|error| error.to_string())?
                    .map(hex::encode),
                detail: Some(issue.code),
            }) })
            .collect::<Result<Vec<_>, _>>()?;
        let commit_sequence = kernel.query().map_err(|error| error.to_string())?.commit_sequence().map_err(|error| error.to_string())?;
        Ok(ValidationReport { issues, checked_at_ms: now_millis(), project_commit_sequence: commit_sequence })
    })
}

#[tauri::command]
fn create_export(request: ExportRequest, state: State<'_, DesktopState>) -> Result<ExportRecord, String> {
    with_kernel(&state, |kernel| {
        if request.project_id != hex::encode(kernel.project_id().as_bytes()) { return Err("导出项目与当前打开项目不一致".to_owned()); }
        let format = active_format(kernel)?;
        let root = kernel.database_path().parent().ok_or_else(|| "项目目录不可用".to_owned())?.to_owned();
        let staging = root.join("staging").join(format!("render-{}", request.command_id));
        if let Some(parent) = staging.parent() { fs::create_dir_all(parent).map_err(|error| error.to_string())?; }
        let report = kernel.export_active_format_id_to_path(format, &staging).map_err(|error| error.to_string())?;
        let bytes = fs::read(&report.path).map_err(|error| error.to_string())?;
        let digest = Sha256::digest(request.command_id.as_bytes());
        let export_id = i64::from_be_bytes(digest[..8].try_into().expect("hash slice length")).unsigned_abs() as i64;
        kernel.publish_export_bytes(export_id, &bytes, Path::new(&request.destination_path), format, request.created_at_ms).map_err(|error| error.to_string())?;
        let _ = fs::remove_file(&staging);
        Ok(ExportRecord {
            id: export_id.to_string(),
            created_at_ms: request.created_at_ms,
            path: request.destination_path,
            format: format.to_owned(),
            output_hash: hex::encode(report.output_hash),
            status: "succeeded".to_owned(),
            error: None,
        })
    })
}

#[tauri::command]
fn list_exports(request: ListExportsRequest, state: State<'_, DesktopState>) -> Result<Vec<ExportRecord>, String> {
    with_kernel(&state, |kernel| {
        if request.project_id != hex::encode(kernel.project_id().as_bytes()) { return Err("导出项目与当前打开项目不一致".to_owned()); }
        kernel.query().map_err(|error| error.to_string())?.export_records().map_err(|error| error.to_string()).map(|records| records.into_iter().map(|record| ExportRecord { id: record.export_id.to_string(), created_at_ms: record.created_at_ms.unwrap_or_default(), path: record.destination_path.unwrap_or_default(), format: record.format.unwrap_or_else(|| "unknown".to_owned()), output_hash: record.expected_hash.map(hex::encode).unwrap_or_default(), status: match record.state.as_str() { "Published" => "succeeded", "Failed" | "CancelledAfterCrash" => "failed", _ => "running" }.to_owned(), error: record.error }).collect())
    })
}

#[tauri::command]
fn get_settings(app: AppHandle) -> Result<AppSettingsV1, String> {
    read_settings(&app)
}

#[tauri::command]
fn patch_settings(app: AppHandle, request: SettingsPatch) -> Result<AppSettingsV1, String> {
    let mut settings = read_settings(&app)?;
    if let Some(value) = request.language {
        settings.language = value;
    }
    if let Some(value) = request.theme {
        settings.theme = value;
    }
    if let Some(value) = request.density {
        settings.density = value;
    }
    if let Some(value) = request.editor_font_family {
        settings.editor_font_family = value;
    }
    if let Some(value) = request.reading_font_size {
        settings.reading_font_size = value;
    }
    if let Some(value) = request.line_height {
        settings.line_height = value;
    }
    if let Some(value) = request.word_wrap {
        settings.word_wrap = value;
    }
    if let Some(value) = request.shortcut_overrides {
        settings.shortcut_overrides = value;
    }
    if let Some(value) = request.panel_widths {
        settings.panel_widths = value;
    }
    write_settings(&app, &settings)?;
    Ok(settings)
}

#[tauri::command]
fn save_navigation(
    request: SaveNavigationRequest,
    state: State<'_, DesktopState>,
) -> Result<SaveResult, String> {
    with_kernel(&state, |kernel| {
        let project_id = ProjectId::from_bytes(decode::<16>(&request.project_id, "projectId")?);
        if project_id != kernel.project_id() {
            return Err("导航项目与当前打开项目不一致".to_owned());
        }
        let position = NavigationPosition {
            schema_version: 1,
            project_id,
            view: request.view,
            unit_id: request
                .unit_id
                .as_deref()
                .map(|value| decode::<16>(value, "unitId").map(UnitId::from_bytes))
                .transpose()?,
            resource_id: request
                .resource_id
                .as_deref()
                .map(|value| decode::<16>(value, "resourceId").map(ResourceId::from_bytes))
                .transpose()?,
            region_id: request
                .region_id
                .as_deref()
                .map(|value| decode::<16>(value, "regionId").map(ResourceId::from_bytes))
                .transpose()?,
            scroll_anchor_unit_id: request
                .scroll_anchor_unit_id
                .as_deref()
                .map(|value| decode::<16>(value, "scrollAnchorUnitId").map(UnitId::from_bytes))
                .transpose()?,
            scroll_offset_px: request.scroll_offset_px.unwrap_or(0),
            zoom_millionths: request.zoom_millionths.unwrap_or(1_000_000),
            filters: NavigationFilters::default(),
        };
        let receipt = kernel
            .save_navigation_position(
                position,
                request.client_session_id,
                request.position_sequence,
                request.updated_at_ms,
            )
            .map_err(|error| error.to_string())?;
        Ok(SaveResult {
            accepted: receipt.accepted,
            sequence: receipt.position_sequence,
            commit_sequence: None,
            revision_id: None,
        })
    })
}

#[tauri::command]
fn mutate_workspace(
    request: WorkspaceMutationRequest,
    state: State<'_, DesktopState>,
) -> Result<WorkspaceMutationReceipt, String> {
    with_kernel(&state, |kernel| {
        kernel
            .mutate_workspace(request)
            .map_err(|error| error.to_string())
    })
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(DesktopState {
            session: Mutex::new(None),
        })
        .invoke_handler(tauri::generate_handler![
            create_project,
            open_project,
            list_projects,
            import_file,
            import_workspace_files,
            create_workspace_file,
            read_workspace_file,
            write_workspace_file,
            read_workspace_state,
            write_workspace_state,
            workbench_snapshot,
            project_tree,
            resource_queue,
            image_preview,
            ocr_image_region,
            render_image_region,
            save_translation,
            save_draft,
            work_item,
            undo_translation,
            redo_translation,
            find_terms,
            search_project,
            annotations_for_unit,
            validate_project,
            list_exports,
            create_export,
            get_settings,
            patch_settings,
            mutate_workspace,
            save_image_region_edit,
            save_navigation
        ])
        .run(tauri::generate_context!())
        .expect("error while running Babel Tower");
}
