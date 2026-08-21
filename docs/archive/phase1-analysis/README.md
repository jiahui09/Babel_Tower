# Babel Tower Phase 1: Repository Archaeology Summary

> Status: complete as a Phase 1 fact investigation on 2026-08-20. This directory is analysis evidence, not the Phase 2 formal documentation system.

## 1. Current Architecture

The application is a React/TanStack desktop UI over a typed DesktopBridge and Tauri command boundary. Tauri owns the active project session and some desktop filesystem/config work; `babel-application::Kernel` orchestrates domain operations; SQLite/CAS and isolated format/OCR workers provide authority and durable execution. See [repository-map.md](repository-map.md) and [architecture-findings.md](architecture-findings.md).

## 2. Core Business Model

One project activates imported generations containing stable translation units and resources. Structured translations form immutable revision chains with separate drafts; validation/export use authoritative frozen projections. See [product-model.md](product-model.md) and [domain-model.md](domain-model.md).

## 3. State Ownership

Core translation, revision, draft, export and navigation truth belongs to Rust/SQLite. Query is a cache; Zustand should own UI session state. Current violations include dual workspace-session persistence, duplicate panel widths, derived frontend status and inert settings. See [state-ownership-findings.md](state-ownership-findings.md).

## 4. IPC Architecture

The command surface is broad and mostly closed end to end. A confirmed request mismatch breaks `read_workspace_state`, and several APIs silently rely on the active project rather than their declared project ID. See [ipc-findings.md](ipc-findings.md).

## 5. Current Workflows

Create/open/import, Explorer file operations, structured edit/save/revision, validation/export and OCR-derived resources have real implementation paths. Recovery UI, shortcut overrides and full workbench/restart behavior remain partial. See [workbench-findings.md](workbench-findings.md).

## 6. Testing

Fresh frontend unit tests (7 files/14 tests) and the full Rust workspace passed. Typecheck and ESLint passed during the frontend check, but Prettier failed on 21 files. Browser E2E is fixture-only; real Tauri E2E is missing. See [testing-findings.md](testing-findings.md).

## 7. Release State

Historical Phase 3 packages prove limited packaging feasibility, not the current desktop product. Windows native validation, final installer/runtime/font/license/SBOM closure and real desktop acceptance are missing. See [release-findings.md](release-findings.md).

## 8. Current Priorities

- P0: dirty bulk-tab close can discard unsaved state; real desktop E2E absent; recovery decision UI absent.
- P1: workspace/session and settings double ownership; status divergence; IPC request mismatch; red frontend/release gates.
- P2: inconsistent success feedback and visible disabled About entry.

Full records: [risk-register.md](risk-register.md).

## 9. Documentation Conflicts

`DESIGN.md` and `PROJECT_STATUS.md` contain outdated and internally contradictory completion statements, especially for file writes, export and test counts. See [documentation-conflicts.md](documentation-conflicts.md).

## 10. Most Important Architectural Warning

The Rust core has stronger invariants and coverage than the desktop shell. The largest risk concentration is at state reconciliation and IPC DTO boundaries: UI session state has competing persistence sources, while message contracts lack generated or tested schema alignment.

## 11. Recommended Phase 2 Documentation System

After stakeholder review, create formal, versioned documents for product capabilities, runtime boundaries, domain/schema, state ownership/hydration, IPC contracts, save/close/recovery invariants, testing strategy and release acceptance. Link each normative claim to an executable gate. **Phase 2 has not been started.**

## Artifact Index

1. [repository-map.md](repository-map.md)
2. [architecture-findings.md](architecture-findings.md)
3. [product-model.md](product-model.md)
4. [domain-model.md](domain-model.md)
5. [state-ownership-findings.md](state-ownership-findings.md)
6. [ipc-findings.md](ipc-findings.md)
7. [workbench-findings.md](workbench-findings.md)
8. [testing-findings.md](testing-findings.md)
9. [release-findings.md](release-findings.md)
10. [documentation-conflicts.md](documentation-conflicts.md)
11. [acceptance-report-verification.md](acceptance-report-verification.md)
12. [risk-register.md](risk-register.md)
