# Domain Model

## Relationship Map

```text
Project (project_id, root)
  -> active ImportGeneration
     -> ResourceGraph -> Resource / ImageRegion
     -> TranslationUnit (stable unit_id, source_unit_key)
        -> TranslationRevision chain -> UnitHead
        -> DraftSession(base_revision_id)
        -> Annotation / Marker / TranslationStatus
  -> ProjectNavigation
  -> Workspace files + WorkspaceOperationLog
  -> ExportRecord -> frozen snapshot / published output
  -> TaskRecord / DiagnosticEvent / Recovery state
```

## Entity Facts

| Entity            | Definition and identity                                                                            | Persistence / relationships                                                                     | Mutation path and consumers                                                        |
| ----------------- | -------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------- |
| Project           | Local root plus stable project ID; only one authoritative writer/session is opened                 | Registry/settings at desktop boundary; SQLite project store below root                          | create/open commands -> active `Kernel`; library and all project routes consume it |
| Workspace         | User-managed files plus UI session (tabs/groups/tree selection)                                    | Files under controlled workspace root; operation log in SQLite; UI state JSON and browser store | Explorer commands and `AppShell` hydration/persistence                             |
| Document          | Two meanings: imported source artifact and structured `TranslationDocumentV1`                      | Source/CAS/generation artifacts; translation document serialized in revision/draft              | Adapters create units; Tiptap edits translation documents                          |
| Translation Unit  | Stable work item identified by `unit_id`; source identity uses `source_unit_key`                   | `unit`, generation binding and revision head tables                                             | Import/binding creates; editor, search, validation and export consume              |
| Resource          | Graph node with locator/semantic path; images may own spatial regions                              | generation resource/edge tables and CAS object references                                       | Adapters/resource graph create; resource UI/OCR/export consume                     |
| Translation       | Structured TIR plus plain-text projection and status                                               | `translation_revision`, `unit_head`, search projection                                          | save with command ID and expected revision; all three workspaces consume           |
| TranslationStatus | Rust enum: Untranslated, Draft, Translated, Reviewed, Blocked (`babel-domain/src/workbench.rs:37`) | Projected with work items; not consistently authoritative in frontend list                      | Core should own; some UI currently derives from text presence                      |
| Revision          | Immutable translation revision with parent/restored revision identity                              | SQLite revision chain and unit head                                                             | save/undo/redo append new revisions; editor sends expected head                    |
| Draft             | Recoverable structured document based on a durable revision                                        | `draft_session`; does not advance durable commit sequence                                       | autosave/explicit draft IPC; recovery must reject changed durable base             |
| Export            | Recorded frozen-snapshot publication to a destination                                              | `export_record`, staging/publish intent and output hash                                         | validate -> create export -> safe publish -> list records                          |
| Recovery          | Core reconciliation of interrupted exports/tasks/workspace operations and drafts                   | SQLite state plus filesystem staging/CAS                                                        | project open/startup paths; dedicated UI does not expose decisions yet             |

## Lifecycle Notes

1. Import creates a generation and resources/units; activation occurs only after binding/review constraints are satisfied.
2. Translation save is idempotent by command ID and appends a revision; it never mutates a source object.
3. Draft is intentionally non-authoritative and tied to a base revision.
4. Undo/redo restore content by creating new revisions rather than rewriting history.
5. Export reads a frozen generation/revision projection and publishes with no-clobber semantics.

## Ambiguities to Preserve

- Frontend `Document` also means a workspace text file; it is not the same authority as `TranslationDocumentV1`.
- `WorkspaceStateV1` is UI session state, while Rust workspace operations represent filesystem mutation state.
- Recovery core has multiple automatic recovery mechanisms; the user-facing recovery route does not yet model them.
