# Architecture Findings

## Actual Layering

| Layer                 | Owns                                                                                | Does not own                             | Entry points                                 | Calls                                  |
| --------------------- | ----------------------------------------------------------------------------------- | ---------------------------------------- | -------------------------------------------- | -------------------------------------- |
| React UI              | Rendering, user events, local input state                                           | Durable project truth                    | `main.tsx`, route modules, `AppShell`        | Query, Zustand, DesktopBridge          |
| Query / Stores        | Server-cache projection and UI session state                                        | Translation/revision authority           | `queries/project.ts`, `stores/*.ts`          | DesktopBridge or local reducers        |
| DesktopBridge         | Typed frontend API and DTO conversion                                               | Persistence decisions                    | `types.ts`, `tauri-bridge.ts`                | Tauri `invoke`                         |
| Tauri boundary        | Command registration, active session, dialogs/filesystem boundary, worker discovery | Format algorithms and revision semantics | `src-tauri/src/lib.rs:1778-1820`             | `Kernel`, filesystem, OCR process      |
| Application core      | Use-case orchestration and invariants                                               | UI layout                                | `babel-application::Kernel`                  | storage, adapters, graph, TIR, runtime |
| Persistence / workers | SQLite/CAS/recovery and isolated format/OCR execution                               | UI state                                 | `babel-storage`, `babel-adapter-host`, tools | filesystem/process IPC                 |

## Confirmed Strengths

1. **Production/fixture separation is explicit.** Production IPC failure is not replaced with demo data (`apps/desktop/src/main.tsx:24-31`).
2. **Translation authority remains below React.** Revisions, drafts, undo/redo and conflict checks live in Rust/SQLite; React sends expected revision IDs (`platform/desktop-bridge/types.ts:158-200`).
3. **Formats converge on one application core.** TXT, Markdown and EPUB imports/exports share `Kernel` paths and stable work items (`crates/babel-application/src/lib.rs:2018-2142`, `2485-2550`).
4. **Source safety has executable coverage.** CAS immutability, frozen export snapshots and no-clobber publication are covered by Rust tests.
5. **Worker boundaries are bounded.** Protocol frame limits, cancellation, deadlines and process termination have tests in adapter/runtime crates.

## Boundary Violations and Duplication

1. **Tauri and application both implement workspace path rules.** Tauri has `workspace_root_for_kernel`, `workspace_path`, direct read/write helpers (`src-tauri/lib.rs:776-826`); application also has workspace root/path validation and recovery (`babel-application/src/lib.rs:4047-4210`). This is a security-sensitive duplicated boundary.
2. **Frontend derives some business status.** The units UI infers draft/untranslated from local text rather than always consuming authoritative `TranslationStatus` (`routes/projects.$projectId.units.tsx:116-146`).
3. **Settings and workbench layout overlap.** `AppSettingsV1.panelWidths` exists, while live widths are independently persisted by WorkbenchStore (`stores/settings.ts:17-28`, `stores/workbench.ts:291-310`).
4. **Two persistence channels store workspace session.** WorkbenchStore uses browser persistence and `AppShell` also reads/writes project workspace state through IPC (`app-shell.tsx:108-170`). Their hydration order and conflict rule are implicit.
5. **Bridge method arguments are sometimes discarded.** `projectSnapshot(projectId)` ignores `projectId`, and `workItem`/term/annotation methods rely on the single active kernel session (`tauri-bridge.ts:120-166`, `208-215`). This is valid only under a strict one-active-project invariant and makes contract misuse harder to detect.

## Workflow Status Summary

| Workflow                         | Status                                  | Evidence boundary                                                                            |
| -------------------------------- | --------------------------------------- | -------------------------------------------------------------------------------------------- |
| Create/open/import project       | Implemented                             | UI -> Bridge -> registered Tauri commands -> Kernel                                          |
| Explorer CRUD/search/file IO     | Implemented                             | Real command paths; desktop E2E unverified                                                   |
| Open/edit/save/revision/conflict | Implemented                             | Structured documents and expected revision IDs; Rust tests pass                              |
| Tabs/split/session restore       | Partial                                 | Substantial implementation; thin regression coverage and dual persistence                    |
| Validation/export                | Implemented core, partially verified UI | Real IPC; no real desktop E2E                                                                |
| Recovery                         | Partial/Fake UI                         | Core crash/export/workspace recovery exists; `/recovery` page only navigates                 |
| OCR/derived resource             | Implemented development path            | Linux asset/runtime present; restart/E2E/release closure incomplete                          |
| Settings                         | Partial                                 | Language/theme/typography persist; word wrap, shortcuts and panel width ownership incomplete |
