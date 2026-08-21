# Acceptance Report Verification

This verifies the acceptance report produced immediately before Phase 1.

| Prior finding                              | Verdict                 | Evidence and correction                                                                                                                                                                                                                   |
| ------------------------------------------ | ----------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Recovery is a fake page                    | **PARTIALLY CONFIRMED** | The route only links away (`recovery.$projectId.tsx:8-31`), so user recovery decisions are fake/incomplete. Core automatic draft/export/task/workspace recovery is real and heavily tested; calling all recovery fake would be incorrect. |
| Shortcut configuration incomplete          | **CONFIRMED**           | `shortcutOverrides` persists in schema but registry/listener uses static descriptor shortcuts (`registry.ts:84-186`; `command-surfaces.tsx:150-184`).                                                                                     |
| Real Tauri E2E missing                     | **CONFIRMED**           | Playwright starts fixture Vite (`playwright.config.ts:13-18`); only one navigation smoke exists.                                                                                                                                          |
| Settings have double source                | **PARTIALLY CONFIRMED** | Desktop settings are intended authority and Zustand is a mirror, but Zustand also persists locally. The sharper confirmed double-source defect is `panelWidths`, split between settings and WorkbenchStore.                               |
| TranslationStatus double source            | **CONFIRMED**           | Core enum exists, while units UI derives display state from translation text in places.                                                                                                                                                   |
| Workbench regression coverage insufficient | **CONFIRMED**           | No dedicated WorkbenchStore/tabs/split test suite; one fixture smoke does not cover mutations.                                                                                                                                            |
| Documentation conflicts                    | **CONFIRMED**           | Export/file-write and current test status claims conflict; see `documentation-conflicts.md`.                                                                                                                                              |
| Prettier failures                          | **CONFIRMED CURRENTLY** | Fresh full frontend check failed on 21 files after typecheck/lint succeeded.                                                                                                                                                              |
| Release closure incomplete                 | **CONFIRMED**           | Windows native, real desktop E2E, fonts and final license/SBOM closure are absent.                                                                                                                                                        |

## Additional Finding Missed by Prior Report

`closeOtherTabs` and `closeTabsToRight` bypass dirty-tab flush/confirmation, unlike individual close. This is a more immediate data-loss risk than formatting or the disabled About item (`stores/workbench.ts:179-213`).
