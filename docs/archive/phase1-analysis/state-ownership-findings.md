# State Ownership Findings

## Ownership Matrix

| State                        | Authoritative owner                              | Persistent?                          | Read / write path                       | Derived / consumers                | Duplicate source and risk                                                                  |
| ---------------------------- | ------------------------------------------------ | ------------------------------------ | --------------------------------------- | ---------------------------------- | ------------------------------------------------------------------------------------------ |
| Project identity/root        | Desktop registry + active `Kernel`               | Yes                                  | list/open/create IPC                    | Library, all project commands      | Bridge accepts project IDs that active-session commands may ignore; medium                 |
| Workspace files/path         | Controlled filesystem, mediated by Tauri/Kernel  | Yes                                  | project tree/read/write/mutate IPC      | Explorer/file editor               | Path validation duplicated in Tauri and application; high                                  |
| Translation unit identity    | Rust generation/binding store                    | Yes                                  | snapshot/work item queries              | All workspaces/export              | Frontend reconstructs some IDs from tab string prefixes; medium                            |
| Translation document         | Revision head in SQLite                          | Yes                                  | `work_item` / `save_translation`        | Tiptap, search, validation/export  | Editor-local JSON is a view; contract is sound                                             |
| TranslationStatus            | Rust domain/work-item projection                 | Yes                                  | work item/page                          | Units/status UI                    | Units UI sometimes derives status from text presence; high                                 |
| Revision                     | SQLite unit head/revision chain                  | Yes                                  | work item -> expected revision -> save  | Editor/conflict/undo               | Query cache may be stale until invalidation; normal but requires tests                     |
| Draft                        | SQLite `draft_session`                           | Yes                                  | save draft/recovery core                | Editor/recovery                    | Dedicated recovery UI does not query it; high                                              |
| Dirty                        | WorkbenchStore tab + editor-local change version | Browser/session                      | editor callbacks and flusher registry   | Tabs, close dialog, save indicator | Multiple dirty representations are intentionally coupled but weakly tested; high           |
| Open/active tabs             | WorkbenchStore                                   | Browser + desktop workspace JSON     | reducer; `AppShell` IPC hydration/write | Tabs/routes/editor groups          | Dual persistence has no explicit conflict/version rule; high                               |
| Editor state                 | Tiptap/CodeMirror component                      | No, except draft/revision projection | component update -> save callbacks      | Current editor                     | Correctly non-authoritative                                                                |
| Split groups/layout          | WorkbenchStore                                   | Browser + workspace JSON             | tab reducers and resizable panels       | AppShell                           | Same dual persistence issue; split ratio only browser-persisted                            |
| Panel widths                 | WorkbenchStore live value                        | Browser                              | resizable panel -> `setPanelWidths`     | AppShell                           | Also exists in `AppSettingsV1` but is never synchronized; high                             |
| Settings                     | Desktop settings JSON; Zustand mirror            | Yes, also browser mirror             | hydrator/get; dialog/patch/rollback     | i18n/theme/CSS                     | Two caches; desktop response wins after mount. Browser fallback semantics implicit; medium |
| shortcutOverrides            | Settings schema only                             | Yes as inert data                    | get/patch possible                      | No command consumer                | Static registry ignores it; incomplete/high                                                |
| wordWrap                     | Settings schema/store                            | Yes                                  | dialog patch                            | No editor consumer found           | Visible control without behavior; fake/incomplete/high                                     |
| Explorer expansion/selection | WorkspaceStore                                   | Workspace JSON                       | project tree + AppShell state write     | Explorer                           | WorkbenchStore also contains unused `selectedExplorerNodeId`; duplicate/dead state; medium |
| Recovery state               | Rust task/export/draft/workspace records         | Yes                                  | automatic core recovery                 | Core only                          | `/recovery` route has no read/write path; critical product gap                             |
| Export state                 | SQLite export record                             | Yes                                  | list/create export IPC                  | Export page                        | Page-local `creating/error` is appropriate transient state                                 |
| Query cache                  | TanStack QueryClient                             | No                                   | query functions / invalidations         | UI projections                     | Key design is mostly consistent; no persistence                                            |

## Highest-Risk Duplicate Sources

1. **Workspace session:** browser-persisted WorkbenchStore versus `WorkspaceStateV1` stored through IPC (`app-shell.tsx:108-170`). No timestamp/schema conflict resolution beyond schema version 1.
2. **Panel width:** settings DTO (`types.ts:359-370`) versus live WorkbenchStore (`workbench.ts:102-104`, `294-309`).
3. **Translation status:** authoritative enum (`babel-domain/src/workbench.rs:37`) versus UI inference in units list.
4. **Explorer selection:** WorkspaceStore is consumed; WorkbenchStore's `selectedExplorerNodeId` appears unused by Explorer.
5. **Settings cache:** persisted Zustand is replaced asynchronously by desktop settings; temporary startup values can differ.

## Required Ownership Decisions (Phase 2 input, not implementation)

- Desktop project workspace JSON should be the project-scoped tab/group/tree authority; browser storage should be either a cache with versioning or removed for those fields.
- `panelWidths` needs one owner and one project/global scope definition.
- Core `TranslationStatus` must be the only status source.
- Inert settings must either gain consumers or stop being exposed as functional controls.
