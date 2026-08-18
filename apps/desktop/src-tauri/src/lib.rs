use std::{
    fs::{self, File},
    path::{Path, PathBuf},
    sync::Mutex,
};

use babel_application::Kernel;
use babel_domain::core::ProjectId;
use babel_domain::workbench::{NavigationPosition, WorkspaceView};
use serde::{Deserialize, Serialize};
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

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct NavigationSummary {
    position: NavigationPosition,
    client_session_id: String,
    position_sequence: u64,
    updated_at_ms: i64,
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
    created_at_ms: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SaveNavigationRequest {
    position: NavigationPosition,
    client_session_id: String,
    position_sequence: u64,
    updated_at_ms: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SaveResult {
    accepted: bool,
    sequence: u64,
    commit_sequence: Option<i64>,
}

fn project_registry_path(app: &AppHandle) -> Result<PathBuf, String> {
    let directory = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("cannot resolve application data directory: {error}"))?;
    fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    Ok(directory.join("projects.json"))
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
    let mut source = File::open(&source_path).map_err(|error| error.to_string())?;
    let kernel = Kernel::open(&request.project_root).map_err(|error| error.to_string())?;
    let source_id = *ProjectId::new().as_bytes();
    let report = match format.as_str() {
        "txt" => kernel
            .import_txt_reader(source_id, &mut source, now_millis())
            .map_err(|error| error.to_string())?,
        "md" | "markdown" => kernel
            .import_markdown_reader(source_id, &mut source, now_millis())
            .map_err(|error| error.to_string())?,
        "epub" => kernel
            .import_epub_reader(source_id, &mut source, now_millis())
            .map_err(|error| error.to_string())?,
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
        let navigation = query
            .navigation_position()
            .map_err(|error| error.to_string())?
            .map(|saved| NavigationSummary {
                position: saved.position,
                client_session_id: saved.client_session_id,
                position_sequence: saved.position_sequence,
                updated_at_ms: saved.updated_at_ms,
            });
        let current_unit = navigation
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
fn save_translation(
    request: SaveTranslationRequest,
    state: State<'_, DesktopState>,
) -> Result<SaveResult, String> {
    with_kernel(&state, |kernel| {
        let receipt = kernel
            .save_translation(
                decode::<32>(&request.source_unit_key, "sourceUnitKey")?,
                decode::<32>(&request.command_id, "commandId")?,
                request.text,
                request.created_at_ms,
            )
            .map_err(|error| error.to_string())?;
        Ok(SaveResult {
            accepted: true,
            sequence: receipt.revision_id as u64,
            commit_sequence: Some(receipt.commit_sequence),
        })
    })
}

#[tauri::command]
fn save_navigation(
    request: SaveNavigationRequest,
    state: State<'_, DesktopState>,
) -> Result<SaveResult, String> {
    with_kernel(&state, |kernel| {
        let receipt = kernel
            .save_navigation_position(
                request.position,
                request.client_session_id,
                request.position_sequence,
                request.updated_at_ms,
            )
            .map_err(|error| error.to_string())?;
        Ok(SaveResult {
            accepted: receipt.accepted,
            sequence: receipt.position_sequence,
            commit_sequence: None,
        })
    })
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(DesktopState {
            session: Mutex::new(None),
        })
        .invoke_handler(tauri::generate_handler![
            open_project,
            list_projects,
            import_file,
            workbench_snapshot,
            save_translation,
            save_navigation
        ])
        .run(tauri::generate_context!())
        .expect("error while running Babel Tower");
}
