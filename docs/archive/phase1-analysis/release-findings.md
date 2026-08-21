# Release Findings

## Fact Table

| Release concern     | Status                                                | Evidence / limit                                                                                        |
| ------------------- | ----------------------------------------------------- | ------------------------------------------------------------------------------------------------------- |
| Tauri bundle config | Existing, unverified                                  | `apps/desktop/src-tauri/tauri.conf.json` enables bundle targets; no fresh full Tauri bundle acceptance  |
| Arch phase package  | Artifact exists, validation evidence stale/incomplete | `release/arch` contains package/manifest; manifest does not prove current desktop product               |
| Windows PE          | Cross-built artifact exists                           | `release/windows/pe-build-manifest.json`; Linux cross-build is not native acceptance                    |
| Windows installer   | Missing from release closure                          | No current `release/windows/release-manifest.json`; packaging docs label Windows built-unverified       |
| Real Windows run    | Missing                                               | No install/start/worker/OCR/export native evidence                                                      |
| OCR assets/runtime  | Linux bundle exists, hashes/licenses present          | `resources/ocr/ppocrv6-tiny`; Windows runtime closure unverified                                        |
| Fonts               | Missing final closure                                 | No bundled licensed production font set found                                                           |
| Licenses            | Partial                                               | Workspace AGPL declaration and OCR `LICENSES.md`; complete dependency/product license bundle unverified |
| SBOM                | Missing/unverified as final artifact                  | No verified top-level current desktop SBOM found                                                        |
| CI                  | Partial                                               | Rust core and phase package workflows; desktop frontend/release matrix absent                           |

## Interpretation

The packaging material proves historical Phase 3 TXT feasibility, not the current React/Tauri multi-format desktop release. `packaging/README.md:1-33` explicitly limits its claim and marks Windows as `BUILT_UNVERIFIED`. Generated executables and packages must not be treated as release acceptance without manifests and native probes tied to the same source revision.

## Current Gate

**Not releasable.** Core tests pass, but frontend formatting fails, real desktop E2E is missing, Windows native closure is absent, and font/license/SBOM evidence is incomplete.
