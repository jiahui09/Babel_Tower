# IPC Findings

## Contract Topology

`DesktopBridge` declares the frontend surface (`types.ts:374-439`); `TauriDesktopBridge` maps it to string commands through one guarded `call()` helper (`tauri-bridge.ts:45-284`); Rust implements `#[tauri::command]` functions and registers them in one `generate_handler!` list (`src-tauri/lib.rs:1785-1818`). Production callers use `useDesktopBridge`; direct frontend filesystem access was not found. File dialogs use the official Tauri dialog plugin and return paths to Bridge calls.

## Command Catalog

| Command group / commands                          | Purpose and request/response                                | Rust/persistence                                      | Main callers/tests              | Status / gap                                                                          |
| ------------------------------------------------- | ----------------------------------------------------------- | ----------------------------------------------------- | ------------------------------- | ------------------------------------------------------------------------------------- |
| `list_projects`, `create_project`, `open_project` | Bootstrap/create/open project summary                       | Tauri registry JSON + active Kernel                   | library/import/AppShell         | Implemented; one-active-project invariant                                             |
| `import_file`                                     | Source path + project root -> format/unit activation result | Kernel -> adapter worker -> generation/SQLite/CAS     | import route; Rust format tests | Implemented                                                                           |
| `project_tree`, `search_project`                  | Tree/search projections                                     | Kernel query + controlled FS tree                     | Explorer                        | Implemented; tree polls every 2s                                                      |
| `create_workspace_file`, `import_workspace_files` | Create/copy controlled workspace entries                    | Tauri FS plus Kernel mutation receipt/log             | Explorer                        | Implemented; path policy duplicated                                                   |
| `read_workspace_file`, `write_workspace_file`     | Text file DTO; optional mtime conflict                      | Tauri controlled FS                                   | WorkspaceFileEditor             | Implemented; desktop E2E missing                                                      |
| `mutate_workspace`                                | folder create, rename, move, trash, restore, reveal         | Kernel workspace operation log/recovery               | Explorer                        | Implemented; frontend return typed as `never` incorrectly (`tauri-bridge.ts:237-239`) |
| `read_workspace_state`, `write_workspace_state`   | Per-project tabs/groups/tree JSON                           | `.config/workspace-state.json`                        | AppShell                        | **Broken read request contract**; see below                                           |
| `workbench_snapshot`, `work_item`                 | Project/navigation/unit/work-item projection                | Active Kernel query                                   | Query layer/routes              | Implemented; project ID arguments partly ignored                                      |
| `save_translation`, `save_draft`                  | Durable revision or non-durable draft                       | SQLite revision/draft tables                          | Translation editors             | Core implemented; ordinary editor path does not meaningfully use `saveDraft`          |
| `undo_translation`, `redo_translation`            | Append restoring revision                                   | SQLite revision/undo groups                           | CommandRegistry                 | Implemented/tested                                                                    |
| `save_navigation`                                 | Monotonic client navigation position                        | `project_navigation`                                  | Long-form route                 | Implemented/tested                                                                    |
| `resource_queue`, `image_preview`                 | Stable image-region page and controlled CAS preview         | Kernel/resource graph/CAS                             | Resource route                  | Implemented                                                                           |
| `ocr_image_region`                                | OCR request -> document/replayed flag                       | OCR worker + cache                                    | Resource route                  | Implemented development path; release/E2E unverified                                  |
| `save_image_region_edit`                          | Correct source-region revision                              | SQLite image-region revision/head                     | Resource editor                 | Implemented                                                                           |
| `render_image_region`                             | Translation/polygon -> derived PNG/hash/commit              | image renderer + CAS + region revision                | Resource route                  | Implemented; font/runtime closure incomplete                                          |
| `find_terms`, `annotations_for_unit`              | Translation aid projections                                 | Active Kernel/SQLite                                  | Inspector                       | Read paths implemented; mutation UI not in this surface                               |
| `validate_project`                                | Current format validation report                            | Kernel format validator                               | Problems/validate routes        | Implemented                                                                           |
| `create_export`, `list_exports`                   | Safe export and authoritative records                       | Kernel frozen export + SQLite record + destination FS | Export page                     | Implemented; desktop/native release proof absent                                      |
| `get_settings`, `patch_settings`                  | Global settings read/validated patch                        | app-data `settings-v1.json`                           | Providers/settings dialog       | Implemented; some fields inert                                                        |

## Confirmed Contract Defects

### P0/P1: `read_workspace_state` cannot deserialize its normal frontend request

- Frontend sends `{ request: { projectId } }` (`tauri-bridge.ts:100-103`).
- Rust receives `WorkspaceFileRequest`, whose `node_id` field is mandatory (`src-tauri/lib.rs:69-72`, `863-865`).
- Tauri deserialization therefore requires a field the frontend never sends. The subsequent Rust body does not use `node_id`, showing the wrong DTO was reused.
- Consumer impact: `AppShell` catches the failure and reports a command error, but per-project tabs/groups/Explorer state will not restore (`app-shell.tsx:108-135`).

### P1: Interface/context drift around the active project

- `DesktopBridge.projectSnapshot(projectId)` is implemented as a no-argument method and relies on the active Kernel (`tauri-bridge.ts:120-123`).
- `workItem`, `termsForUnit` and `annotationsForUnit` discard `projectId` (`tauri-bridge.ts:145-147`, `208-215`).
- This works only while exactly one matching project is active. Some file/tree commands explicitly call `ensure_kernel_project`; these projection commands do not consistently enforce the same boundary.

### P1: Incorrect mutation return type

`mutateWorkspace` promises `WorkspaceMutationReceipt` in the interface, but the Tauri implementation calls `call<never>` (`types.ts:419`, `tauri-bridge.ts:237-239`). Runtime data still resolves, but TypeScript callers cannot safely consume the receipt through the concrete implementation.

## Error Model

The Bridge normalizes Tauri failures and distinguishes unavailable IPC. Most Rust commands currently return `Result<T, String>`, so structured error codes are lost at the IPC boundary; UI conflict detection sometimes inspects message text. This is functional but brittle across localization/refactoring.

## Test Gaps

- No automatic inventory test proves Bridge command strings equal registered Rust handlers.
- No serialization contract test would catch the `read_workspace_state` missing `nodeId` defect.
- No real Tauri E2E proves filesystem, settings, OCR or export commands.
