# Product Fact Model

## What Babel Tower Is

Babel Tower is a local-first, single-user desktop translation workbench for manually translating TXT, Markdown and EPUB content, including image-region OCR and derived image rendering. The authoritative core is Rust/SQLite/CAS; the React application is a desktop projection over Tauri IPC. It is not currently a production-signed general localization platform.

## Supported in Current Code

- Create and register a local project; open a known project directory.
- Import TXT, Markdown and EPUB through isolated format paths.
- Browse source/workspace/derived trees; search source and translation projections.
- Create/import/read/write/rename/trash/restore workspace files.
- Edit structured translations, save drafts and durable revisions, detect stale revisions, undo and redo.
- Navigate long-form, units and image resources over shared work-item identity.
- Validate active content and export frozen TXT/Markdown/EPUB snapshots without overwriting originals.
- Preview source images, run bundled Linux OCR, correct recognized source text, translate and render derived PNG resources.
- Persist language, theme, density and typography settings through a desktop settings file.

## Not Supported or Not Proven

- Machine translation, generative rewriting, accounts or collaboration.
- PDF/audio/video/game resource import.
- Complete user-facing recovery decisions (restore/discard/retry) despite substantial core recovery machinery.
- User-configurable shortcut overrides.
- Proven real-Tauri end-to-end workflows; current Playwright runs a browser fixture.
- Windows native installation/start/OCR/worker/export acceptance.
- Final font, license and SBOM closure for the complete desktop application.
- Photoshop-grade background reconstruction or automatic font/style matching.

## Primary User Workflow

```text
Project library
  -> create/open project or import supported source
  -> project Explorer and restored workspace session
  -> open source/workspace item in a tab
  -> edit structured translation
  -> draft autosave / durable save with expected revision
  -> validate authoritative snapshot
  -> choose non-clobber destination and export
  -> inspect export record
```

| Step              | Status      | Main code path                                       | Gap                                                |
| ----------------- | ----------- | ---------------------------------------------------- | -------------------------------------------------- |
| Project creation  | Implemented | `routes/import.tsx` -> `create_project`              | Real desktop E2E absent                            |
| Import            | Implemented | `importFile` -> `import_file` -> Kernel adapters     | Unsupported encoding remains explicit              |
| Explorer          | Implemented | `ProjectExplorer` -> project tree/workspace commands | Path rules duplicated across layers                |
| Open document     | Implemented | Explorer -> WorkbenchStore tab -> route/editor       | Some tab identity restored from string conventions |
| Edit/save         | Implemented | Tiptap -> save draft/revision -> Query invalidation  | UI tests are narrow                                |
| Revision/conflict | Implemented | expected revision -> SQLite head check               | Conflict UX lacks full E2E                         |
| Validation        | Implemented | validation query -> `validate_project`               | UI only reports current core result                |
| Export            | Implemented | path dialog -> `create_export` -> Kernel             | Real desktop and Windows validation absent         |

## Secondary Workflows

- **Workbench/tabs/split:** implemented reducers, drag/drop and second editor group; merge-back is only achieved by closing secondary tabs, not a distinct merge command.
- **Recovery:** core startup/export/workspace recovery is implemented; dedicated route is a static explanation plus links and therefore incomplete.
- **OCR:** OCR candidate and corrected source remain separate from translation; derived images are immutable CAS outputs.
- **Settings:** load/optimistic patch/rollback exist. Shortcut override and word-wrap consumption do not.
