# Workbench Findings

## Capability Audit

| Operation          | Status      | Actual chain                                                      | Gap                                                                                |
| ------------------ | ----------- | ----------------------------------------------------------------- | ---------------------------------------------------------------------------------- |
| Open tab           | Implemented | route/Explorer -> `openTab` -> group active tab -> route/editor   | Route-generated and restored identities use conventions                            |
| Activate tab       | Implemented | tab click -> store -> optional route navigation                   | Secondary activation has no route callback                                         |
| Close clean tab    | Implemented | close button -> `closeTab`                                        | No focused-group regression test                                                   |
| Close dirty tab    | Implemented | dialog -> registered flusher -> close or error                    | UI coverage absent; label uses danger variant for save-and-close                   |
| Close others/right | Implemented | Radix context menu -> store reducers                              | Dirty tabs are removed without flushing/confirmation; **data-loss risk**           |
| Drag reorder       | Implemented | HTML drag payload -> `moveTab`                                    | No cross-group drag; no tests                                                      |
| Split              | Implemented | split icon -> add same tab to secondary group                     | Mirrored workspace file becomes read-only; translation mirror uses same Query data |
| Merge              | Partial     | No explicit merge; close all secondary tabs collapses split       | Required operation not modeled                                                     |
| Project switch     | Partial     | project route hydrates workspace once and filters tabs by project | `workspaceHydrated` and browser-persisted state need multi-project tests           |
| Session restart    | Partial     | read/write `WorkspaceStateV1` after project opens                 | No dirty/draft restoration proof; dual persistence                                 |

## Important Findings

1. `closeOtherTabs` and `closeTabsToRight` directly remove tab IDs (`stores/workbench.ts:179-213`) and do not consult `dirty` or registered flushers. This bypasses the protection used by the individual close button (`document-tabs.tsx:76-151`). **P0 candidate because unsaved UI state can be discarded.**
2. `splitTab` mirrors the same tab in two groups rather than creating a second document identity (`workbench.ts:265-277`). This is correct for views, but two editable translation editors can share a revision unless the primary/secondary mirror rule consistently makes one read-only; that behavior needs real tests.
3. Workspace files mirrored in both groups are explicitly read-only in secondary (`secondary-editor-group.tsx:21-32`, `61-63`). Source/diff tabs are projections.
4. Workspace state writes are debounced 300 ms (`app-shell.tsx:137-170`), with errors surfaced globally. There is no flush-on-window-close evidence.
5. Split ratio, tabs and groups persist in browser; tabs/groups also persist per project through IPC. The hydration winner is timing-dependent and undocumented.

## Coverage

`stores/workspace.test.ts` exercises Explorer state, but no focused test file covers WorkbenchStore tab reducers, dirty close flows, split behavior or project switching. The single Playwright fixture spec only navigates long-form/units/resources (`e2e/workbench.spec.ts:3-19`).
