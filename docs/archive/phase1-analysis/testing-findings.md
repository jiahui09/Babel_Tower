# Testing Findings

| Area                            | Status on 2026-08-20                      | Evidence                                                                                             |
| ------------------------------- | ----------------------------------------- | ---------------------------------------------------------------------------------------------------- |
| Frontend unit/component         | Existing, passing                         | Fresh `pnpm --dir apps/desktop test`: 7 files, 14 tests passed                                       |
| Rust unit/integration/doc tests | Existing, passing                         | Fresh `cargo test --workspace`: all workspace targets passed                                         |
| Worker IPC integration          | Existing, passing                         | TXT/Markdown/EPUB worker test suites passed                                                          |
| Fixture browser E2E             | Existing, not rerun                       | Playwright starts Vite with `VITE_DESKTOP_BRIDGE=fixture` (`playwright.config.ts:13-18`)             |
| Real desktop E2E                | Missing                                   | No tauri-driver/WebDriver/installed-app harness; fixture config cannot prove IPC/filesystem behavior |
| Typecheck                       | Existing, passing in fresh `check` prefix | Vite typecheck build and `tsc --noEmit` completed                                                    |
| ESLint                          | Existing, passing in fresh `check` prefix | `eslint . --max-warnings 0` completed                                                                |
| Vite production build           | Existing, passing                         | Fresh typecheck build succeeded; prior explicit build stage also has generated `dist`                |
| Prettier                        | Existing, failing                         | Fresh `pnpm --dir apps/desktop check` stopped with 21 files reported unformatted                     |
| Architecture script             | Existing, passing                         | Fresh `./tools/check-architecture.sh`: `architecture dependency direction: ok`                       |

## Coverage Shape

Strongest coverage is below the UI: SQLite atomicity/recovery, stable identity, format round trips, worker IPC, cancellation, CAS and no-clobber export. Frontend tests cover selected stores/bridge conversion/import validation/save indicator/command registry. They do not proportionally cover the blast radius of tabs, split groups, settings hydration, recovery UI, Explorer mutations, OCR and export.

## Critical Missing Scenarios

1. Real desktop create -> import -> edit -> save -> close/reopen -> exact navigation restoration.
2. Dirty close, close-other/right, split, project switch and session persistence.
3. Workspace file CRUD and filesystem conflict through Tauri.
4. OCR candidate cache across restart, corrected source, translation and derived resource export.
5. Validation-blocked export, destination conflict, retry and output verification.
6. Settings restart behavior and effective shortcut/word-wrap behavior.

## CI Gap

`.github/workflows/core-quality.yml` runs Rust formatting, clippy, tests and architecture checks, but its path filter and job do not validate the desktop TypeScript application. No workflow currently proves frontend `check` or fixture/desktop E2E.
