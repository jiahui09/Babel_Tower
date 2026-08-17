# Design

## Source of truth

- Status: Draft — complete first-version design, pending product review
- Last refreshed: 2026-08-17
- Primary product surfaces: project library, import, project overview, long-form workspace, structured-unit workspace, resource workspace, validation, export history, recovery and diagnostics.
- Evidence reviewed: `.omx/plans/prd-offline-translation-workbench.md`, `.omx/plans/architecture-offline-translation-workbench.md`, `.omx/plans/test-spec-offline-translation-workbench.md`, `.omx/specs/deep-interview-offline-translation-workbench.md`.
- Observed facts: this is a greenfield repository without UI source, existing components, visual assets, screenshots, routes, or brand kit.
- Design inference: the initial design must optimize sustained, focused translation work rather than acquisition, dashboards, or generic document management.

## Brand

- Personality: calm, exacting, literary, dependable, and quietly capable.
- Trust signals: persistent save state, visible project name and source format, reversible commands, precise diagnostics, restrained motion, and predictable location in the project.
- Avoid: IDE/terminal metaphors, AI-assistant visual language, marketing-style hero composition, decorative gradients, oversized empty cards, excessive rounded containers, noisy status badges, and exposed parser/worker terminology.
- Product name treatment: `Babel Tower` appears in the application title bar and project library header; it is not a decorative splash screen.

## Product goals

- Goals:
  - Keep translators in their current text, unit, or image region with as little interface management as possible.
  - Make the three workspaces feel like different lenses over one project, not three separate tools.
  - Make save, recovery, validation, and export understandable without exposing implementation details.
  - Keep the offline, human-authored nature of every translation visible in system behavior, not promotional copy.
- Non-goals:
  - Do not optimize for AI generation, chat, social collaboration, admin dashboards, or developer extensibility in version one.
  - Do not use navigation depth, cards, or settings pages to reveal internal architecture.
  - Do not make a touch-first mobile experience; the application is a desktop writing environment.
- Success signals:
  - A translator can reopen a project and resume the exact chapter/unit without searching for it.
  - Switching workspace preserves selection/context where a compatible target exists.
  - An export block explains the affected content and the next user action in plain language.
  - Core editing commands remain discoverable by keyboard and do not shift layout while saving.

## Personas and jobs

- Primary personas:
  - Individual literary translator: spends hours reading and drafting chapter-length translations.
  - Hobby localization translator: needs strict source/target correspondence and format-safe delivery.
  - Small independent localization worker: handles text plus image lettering, then validates a final artifact.
- User jobs:
  - Import an original work safely and understand what is ready to translate.
  - Read context, write a human translation, mark progress, and continue later.
  - Find unfinished, blocked, or inconsistent content quickly.
  - Review image text, edit its transcription/translation, and preview embedded text.
  - Deliver a new file while preserving the original and understanding any limitations.
- Key contexts of use: offline, long uninterrupted writing sessions, laptop or desktop monitors, mixed CJK/Latin text, keyboard-heavy operation, and projects containing thousands of units.

## Information architecture

- Primary navigation:
  - Project library is the application root.
  - Import is a focused flow, not a permanent sidebar destination.
  - Inside a project, a persistent project rail provides `内容`, `单元`, `资源`, `校验`, and `导出`.
  - The active content workspace changes the central canvas; project identity, save state, tasks, and export remain globally available.
- Core routes/screens:

  | Route | Purpose | Entry and exit rules |
  | --- | --- | --- |
  | `/` | Project library and resume entry | Opens the last project only on an explicit Resume action; never auto-redirects while a recovery decision is pending. |
  | `/import` | Import source, name project, review extracted result | Cancel returns to `/`; success goes once to `/projects/:projectId/overview`. |
  | `/projects/:projectId/overview` | Project status, recent location, format summary, blockers | Resume enters the last valid workspace/location; no automatic redirect if there is no content. |
  | `/projects/:projectId/content/:chapterId?unit=:unitId` | Long-form workspace | A chapter/unit selection is optional and normalized only once after project data loads. |
  | `/projects/:projectId/units?filter=&unit=` | Structured-unit workspace | Search/filter are query parameters; selection changes URL without leaving the workspace. |
  | `/projects/:projectId/resources?resource=&region=` | Resource workspace | A selected region is optional; missing/deleted targets show an in-place empty state rather than redirecting. |
  | `/projects/:projectId/validate` | Validation results and resolution navigation | Resolving an item opens its workspace target in a new route, then Back returns here. |
  | `/projects/:projectId/exports/:runId?` | Export setup, progress, results, and history | Export is a dialog/flow over this route; completion remains in export history. |
  | `/recovery/:projectId` | Explicit crash/recovery decision | Complete recovery returns to the project overview; no route silently discards pending local UI edits. |
- Route guards:
  - Unknown project: show a recoverable "项目不可用" screen with Library and Locate Project actions, never bounce between routes.
  - Missing optional chapter/unit/resource query target: retain the current workspace and show a non-blocking "该内容已不可用" notice.
  - Unconfirmed import, export destination conflict, and recovery choice use a modal decision; browser-like Back must return to the prior stable route, not restart a flow.
  - Navigation never depends on save acknowledgement for route completion; pending edits stay represented in the shared save state.
- Content hierarchy:
  - Level 1: application / project identity.
  - Level 2: persistent workspace destination.
  - Level 3: chapter, collection, filter, or resource.
  - Level 4: selected unit/region and inspector detail.

## Design principles

1. Translation comes first. The editable target text is the visual primary action; interface chrome remains compact and stable.
2. Location is never implicit. The project, workspace, chapter/filter, and selected content must be readable at a glance.
3. State must be calm but unambiguous. Saving, blocked export, and task failures are specific, persistent when needed, and never represented only by color.
4. One object, many lenses. A unit selected in long-form or structured mode is the same unit; selection/context follows it when the user deliberately switches mode.
5. Complexity lives behind progressive disclosure. The default surface uses human language; raw diagnostics, source anchors, and technical detail appear only on request.
6. Long sessions deserve visual restraint. Stable columns, no layout jumps, moderate density, and strong typography matter more than decoration.
- Tradeoffs: use compact desktop density over touch-target maximalism; use a disciplined component system over page-specific novelty; use native dialog patterns for irreversible export/recovery decisions over custom animated flows.

## Visual language

- Color:
  - Canvas: `#F7F8F7`; raised surface: `#FFFFFF`; quiet inset: `#EEF1F0`; border: `#D7DDDA`.
  - Ink: `#1C2421`; secondary text: `#5C6762`; muted text: `#7B8580`.
  - Primary action/focus: `#166B5B`; hover: `#0F5548`; focus ring: `#7AC6B6`.
  - Informational selection: `#DDECF5` with ink `#1C4E67`; draft: `#DCECE6`; reviewed: `#D9E9D5`; warning: `#FFF0C7`; destructive: `#B83B3B`.
  - Use semantic CSS variables (`--surface`, `--text`, `--accent`, `--warning`, etc.), never raw colors in feature components. Status always combines color, icon, and text.
- Typography:
  - UI: `Inter, Noto Sans SC, system-ui, sans-serif`.
  - Reading/editor content: `Noto Serif SC, Source Han Serif SC, serif`; user may choose an installed local reading font.
  - UI body 14px / 20px, compact metadata 12px / 16px, section title 16px / 24px, page title 20px / 28px. Editor defaults to 18px / 1.8 with a user-controlled reading-size range.
  - No viewport-based font scaling and no negative letter spacing.
- Spacing/layout rhythm:
  - Base unit: 4px. Primary gaps: 8, 12, 16, 24, 32px.
  - Desktop shell: top bar 48px; left project rail 232px; contextual list pane 280px; inspector 320px; center canvas flexes with a 680px minimum writing width where space allows.
  - Resizable panes have persistent per-project widths. At narrow desktop widths, the contextual list and inspector become toggled side sheets; the editor never drops below a usable line length.
- Shape/radius/elevation:
  - Inputs, menus, popovers, dialogs, and repeated item cards use 6px radius.
  - Toolbar/icon buttons are 32px square; compact 28px only inside dense tables.
  - Sections are layout bands, not floating cards. Borders and subtle surface contrast define hierarchy; shadows are reserved for menus, dialogs, and drag previews.
- Motion:
  - 120-180ms opacity/position transitions for sheets, menus, and save-state changes; no decorative animation.
  - Respect `prefers-reduced-motion`; operations must remain legible without motion.
- Imagery/iconography:
  - Use Lucide icons at 16px or 18px, paired with text for non-obvious commands.
  - Resource mode uses actual project images only. Do not use stock or illustrative artwork as filler.

## Components

- Component foundation:
  - `shadcn/ui` component source + Radix UI primitives for accessible dialogs, menus, popovers, tabs, tooltips, scroll areas, switches, separators, and form controls.
  - Tailwind CSS consumes semantic CSS variable tokens; `class-variance-authority` defines supported variants.
  - Lucide supplies all application icons. Tiptap, TanStack Virtual, and Canvas are specialized internals, not visual design systems.
- New/changed domain components:
  - `AppShell`: title bar, project rail, central canvas, optional context pane/inspector, task and save affordances.
  - `ProjectLibrary`: project rows, last location, source format, progress, actions, empty state.
  - `WorkspaceSwitcher`: compact segmented control for 内容 / 单元 / 资源; routes are distinct but control state follows the current route.
  - `SaveIndicator`: `正在编辑` / `正在保存` / `已保存` / `保存失败`; fixed-width region so text changes never move surrounding controls.
  - `ChapterNavigator`, `UnitFilterBar`, `ResourceNavigator`: workspace-specific contextual panes.
  - `TranslationEditor`: source context, protected inline tokens, target editor, unit state control, previous/next navigation.
  - `UnitTable`: virtualized two-column source/target rows, status filter, bulk state tools, keyboard selection.
  - `ResourceCanvas`: image viewport, selectable region overlays, zoom, reading order, transcription/translation inspector.
  - `InspectorPanel`: metadata, source context, notes, diagnostics, and only task-relevant controls.
  - `ValidationList`, `TaskCenter`, `ExportDialog`, `RecoveryDialog`, `DiagnosticDrawer`.
- Variants and states:
  - Buttons: `primary`, `secondary`, `ghost`, `destructive`, `icon`; no arbitrary custom variants in feature code.
  - Status: `untranslated`, `draft`, `translated`, `reviewed`, `blocked` has fixed icon/text/color mapping.
  - Inputs/editor: default, focused, saving, error, read-only, disabled; error includes an actionable text explanation.
  - Dialogs: confirmation for destructive/replacement choices; progress dialogs cannot be dismissed when it would leave an operation ambiguous, but support backgrounding when safe.
- Token/component ownership:
  - `apps/desktop/src/design/tokens.css`: CSS variables and semantic token documentation.
  - `apps/desktop/src/components/ui/`: vendored/maintained shadcn primitives; no product rules.
  - `apps/desktop/src/components/workbench/`: reusable domain components and layout contracts.
  - Feature routes compose domain components; they do not create ad hoc buttons, colors, dialogs, or shadows.

## Accessibility

- Target standard: WCAG 2.2 AA for application controls and reading surfaces.
- Keyboard/focus behavior:
  - Every command has a visible focus state. Dialog focus is trapped and restored to its invoker.
  - `Tab` traverses controls predictably; arrow keys navigate lists/tables only when those widgets own focus.
  - `Ctrl/Cmd+S`, undo/redo, global search, workspace switching, next/previous unit, and inspector toggle have documented shortcut support with menu discoverability.
  - Editor shortcuts must not be intercepted while composing CJK input; IME composition is tested explicitly.
- Contrast/readability: normal text and controls meet AA contrast; editor uses no low-contrast placeholder-as-content; line length is constrained and reading size can be increased independently of UI density.
- Screen-reader semantics: landmarks for app navigation/main/inspector; table/list semantics match interaction model; icon-only controls require names/tooltips; live regions announce save errors and task completion without narrating every keystroke.
- Reduced motion and sensory considerations: no auto-playing media; reduced-motion setting removes pane slide animations; warnings do not flash.

## Responsive behavior

- Supported breakpoints/devices: Windows and Arch desktop windows from 1024px wide upward; primary acceptance widths are 1280x800, 1440x900, and 1920x1080. Below 1024px is supported as a constrained desktop window, not as a mobile layout.
- Layout adaptations:
  - >= 1440px: project rail, context pane, central canvas, and inspector may be visible together.
  - 1024-1439px: project rail remains; only one secondary pane is visible at a time, controlled by icon buttons with tooltips.
  - < 1024px: rails collapse to sheets; long-form editor remains primary; export/validation use full-height dialogs or dedicated routes.
  - Tables retain stable row heights and horizontal scrolling where needed; columns do not squeeze text into unreadable fragments.
- Touch/hover differences: mouse/keyboard is the primary interaction. Hover reveals secondary row actions only when the same actions are keyboard-accessible and present in an overflow menu.

## Interaction states

- Loading: render the shell immediately with skeleton rows/paragraphs that preserve final layout. Never show an empty editor while a chapter is loading without a clear loading state.
- Empty:
  - Library: a single unframed import affordance with a concise project-focused description.
  - Project with no extracted content: show extraction result and diagnostic entry, not a generic blank page.
  - Filter has no results: retain filters and offer one clear reset action.
- Error: inline errors remain adjacent to the blocked control; operation failures also enter Task Center/Diagnostic Drawer with retry or reveal-in-context actions. No raw stack traces in the normal surface.
- Success: use stable status text and optional low-priority toast. Do not use celebratory animation for routine saves or exports.
- Disabled: explain why in supporting text or tooltip, especially for blocked export, unavailable undo, and image operations requiring a selected region.
- Offline/slow network: the product assumes offline. No spinner or warning for absent network; only local file, OCR, indexing, import, export, and disk-space states are shown.
- Save state: editing is local UI state; `saved` appears only after authoritative acknowledgement. Failure preserves typed content and exposes retry without making the editor look complete.

## Content voice

- Tone: concise, composed, practical, and non-technical by default.
- Terminology:
  - Use `项目`, `原件`, `译文`, `单元`, `资源`, `校验`, `导出`, `任务`, and `问题`.
  - Do not expose `sidecar`, `worker`, `parser`, `IPC`, `runtime`, or `WAL` in routine UI. Use `文字识别`, `内容处理`, `检查`, or `详细信息` where appropriate.
- Microcopy rules:
  - Name the user-visible result first: `无法导出此章节` rather than `导出任务失败`.
  - Tell the user what remains safe: `原件和已保存译文未改变`.
  - Give one next action: `查看问题`, `重试`, `选择其他位置`, or `返回项目`.
  - Avoid AI-like invitations, exclamation marks, vague praise, and jargon-heavy warnings.

## Implementation constraints

- Framework/styling system: React 19 + TypeScript + Vite inside Tauri 2. Use shadcn/ui + Radix UI + Tailwind CSS + Lucide. Use CSS variables for tokens and `class-variance-authority` for component variants.
- Design-token constraints: all colors, spacing, radii, typography, z-index, durations, and pane dimensions come from design tokens. Feature code may not introduce raw hex colors, arbitrary shadows, or one-off component variants without updating this document and the token layer.
- Performance constraints:
  - Do not render all units or a whole book chapter set at once; virtualize structured lists and window long-form chapters.
  - Saving must not cause editor remount, scroll jump, selection loss, or layout shifts.
  - Canvas redraw is scoped to visible image regions; thumbnails load lazily.
- Compatibility constraints: offline-first; CJK IME support; Windows and Arch desktop; no remote font, icon, asset, analytics, or AI dependency. Respect the architecture plan's Tauri platform gates.
- Test/screenshot expectations:
  - Storybook or an equivalent isolated component harness covers every domain component state.
  - Playwright desktop smoke tests cover all routes, all route guards, CJK IME composition, keyboard navigation, save/recovery states, and export/validation handoffs.
  - Capture visual baselines at 1280x800 and 1440x900 for the library, all three workspaces, validation, export, empty, loading, and error states.
  - Review screenshots for overlap, truncation, focus visibility, unstable toolbar widths, and accidental generic-dashboard aesthetics before merge.

## Open questions

- [ ] Logo, application icon, and final wordmark treatment / product owner / affects installer, title bar, and project library identity.
- [ ] Default CJK serif font licensing and packaged size / engineering + product owner / affects offline package size and long-form reading quality.
- [ ] Whether translators need personal annotations separate from source/translation content / product owner / affects inspector and data model.
- [ ] Whether a compact dark theme belongs in version one / product owner / affects token completeness and visual QA scope.
- [ ] Exact keyboard shortcut map and localization language set / product owner + UX / affects command menu and accessibility validation.
- [ ] Which EPUB structural diagnostics should be elevated from detail view to blocking validation / format engineering / affects validation information hierarchy.

## Application shell and layout

The standard project window is a persistent workbench, not a collection of full-page cards. It has one stable frame and replaces only the workspace canvas and its contextual pane.

```text
┌──────────────────────────────────────────────────────────────────────────────────────────────┐
│ Babel Tower / Project title        Editing · Saved   Undo Redo   Search   Tasks   Export      │
├──────────────┬──────────────────────┬──────────────────────────────────────┬──────────────────┤
│ Project rail │ Context pane         │ Primary workspace                    │ Inspector        │
│              │                      │                                      │                  │
│ Overview     │ chapters / filters / │ long-form editor OR                  │ current unit,    │
│ ──────────── │ resources, depending │ structured unit list OR              │ source context,  │
│ Content      │ on current workspace │ resource canvas                      │ diagnostics      │
│ Units        │                      │                                      │                  │
│ Resources    │                      │                                      │                  │
│ Validate     │                      │                                      │                  │
│ Exports      │                      │                                      │                  │
├──────────────┴──────────────────────┴──────────────────────────────────────┴──────────────────┤
│ Current location · Unit state · Character count · Local task activity                         │
└──────────────────────────────────────────────────────────────────────────────────────────────┘
```

- The title bar is a 48px command band. It never becomes a second navigation rail.
- The project rail is navigation only. It does not repeat progress dashboards or settings.
- The context pane belongs to the active workspace and can be collapsed without changing route or selection.
- The inspector is closed by default below 1440px. It is an inspection surface, not a required second editor.
- The bottom status strip is 24px and contains non-actionable context; actionable failures open Task Center or Diagnostics.
- Global commands are ordered by frequency: save state, undo/redo, search, tasks, export. Application settings live in a native menu or compact project/library menu, not in the workbench rail.

## Screen specifications

### Project library

Purpose: begin, resume, locate, and organize local projects without making project administration the product's main experience.

- Header: `Babel Tower`, compact `导入作品` primary button, project search field, overflow menu for settings/about.
- Main content: full-width project list grouped by `继续工作` and `其他项目`; each row contains project name, source format, last location, modified time, translation progress, and a subtle blocked indicator when applicable.
- Row primary action: open/resume. Secondary actions live in a `More` menu: reveal folder, duplicate portable backup, rename, remove library reference. Removing a library reference never deletes source/project files in the same action.
- Empty state: centered but unframed title `导入第一部作品`, one sentence about local projects, one `选择文件` primary command, and supported-format text. No illustration is required before a brand asset exists.
- Error/absence: a missing project is still listed with `需要定位` and a folder action. It is never silently removed.

### Import flow

Purpose: turn a source file into a local project while making format limitations clear before committing work.

The route uses a three-step stepper within one centered dialog or dedicated narrow page. Back/Cancel remain visible; only an active import task asks for confirmation before leaving.

1. `选择作品`: file picker plus drop zone; accepted formats visible; duplicate-source detection presents `打开已有项目` and `仍要创建副本`.
2. `确认项目`: editable project name, source format/encoding summary, source location disclosure, and a concise original-file guarantee.
3. `检查结果`: extracted chapter/unit/resource counts, supported image count, warnings, and `进入项目` primary action. A blocking safety issue disables entry and exposes `查看问题`.

Do not expose archive entries, parsing internals, or worker logs in this flow. Technical information is available only through `详细信息` in the result step.

### Project overview

Purpose: orient a returning user before they resume work, without becoming a dashboard.

- Top: project name, source format, last saved time, and one primary `继续翻译` action that targets the last valid location.
- Main left: `下一步` list, limited to at most three high-value actions: resume, resolve highest-priority validation issue, or continue a local task.
- Main right: compact project facts: translated/reviewed counts, source chapters/resources, recent exports. These are text-led summaries, not charts.
- Bottom: recent locations as a simple list. The user can enter Content, Units, or Resources directly from the rail.
- Empty project: explains that extraction found no editable content and offers diagnostics; it does not simulate progress.

### Long-form workspace (`Content`)

Purpose: support chapter-length reading and writing with an uninterrupted target-text focus.

- Context pane: chapter tree with translated-state dots, chapter title, optional search-within-project result list, and a compact progress summary. It does not display every unit as a second giant list.
- Canvas header: breadcrumb `Project / Content / Chapter`, chapter title, workspace switcher, reading/editor controls, and inspector toggle.
- Canvas body: source context appears in a quiet, collapsible column or preceding block; target translation remains the editing surface. The default comparison style is stacked on narrower windows and two-column above the writing-width threshold.
- Protected inline structure appears as low-emphasis inline chips with a stable label, never as an editable raw markup string. Selecting a chip opens a small explanation/inspector entry.
- Footer navigation: previous/next chapter, current chapter translation progress, and unit location when the cursor is inside an addressable unit.
- Text selection/cursor is preserved through save acknowledgement, inspector toggles, and context-pane collapse. A user-initiated workspace switch carries the selected `unit_id` when possible.

### Structured-unit workspace (`Units`)

Purpose: provide fast, strict correspondence work for translators who need to process units systematically.

- Header: workspace switcher, search, status filter, chapter/collection filter, `仅显示未完成` toggle, and a compact bulk-action menu.
- Main: virtualized rows with fixed structure: index/status, source text, target editor/summary, and context marker. Source and target columns are resizable but retain a readable minimum.
- Selected row: uses an information-tinted background, a focus outline, and opens contextual information in the inspector. It is not signaled only by color.
- Inline edit: starts via Enter/click, commits through the shared save protocol, and keeps the row height stable. Multi-line translation opens a row expansion rather than a free-floating modal.
- Bulk actions are limited to state changes and must show the number of affected units before confirmation. Text content is never bulk-overwritten.
- Keyboard: Up/Down changes active row; Enter edits target; Escape exits edit without changing selection; `Ctrl/Cmd+Enter` changes the state according to the chosen explicit command.

### Resource workspace (`Resources`)

Purpose: let a translator inspect image text and create a controlled translated derivative without altering the original.

- Context pane: resource thumbnail list, grouped by source location. Each entry shows region count and unresolved/blocked marker.
- Canvas toolbar: fit, 100%, zoom out/in, region visibility, reading order, and inspector toggle. Tool buttons are icon buttons with tooltips.
- Canvas: actual source image on a neutral checker-free canvas; regions use a thin high-contrast outline, numbered reading-order label, and selected-region fill. Handles appear only on the selected region.
- Inspector: source recognition candidate, editable source transcription, manual translation field, font/size/direction/alignment controls, and derivative preview state. OCR is described as `识别结果`, never `译文`.
- Primary actions: `保存区域` and `预览嵌字`; creating a derivative is explicit and reports that the original image remains unchanged.
- No selected region: inspector explains how to choose or add a region; controls requiring selection are disabled with reason text.

### Validation (`Validate`)

Purpose: give translators a clear pre-delivery list of issues, not a technical log viewer.

- Header: summary sentence such as `3 个问题需要处理，2 项提示可稍后查看`, with `重新检查` and a severity filter.
- List: each item has severity icon/text, human explanation, affected chapter/unit/resource, and one `前往处理` command. Technical details are collapsible.
- Severity definition: `阻止导出` is reserved for unsafe mapping, encoding, missing required translation, or unrecoverable output conflict; `需要注意` does not block export; `信息` is hidden by default after first review.
- Resolving an item opens its exact workspace target. Validation route remains in history so Back returns to the filtered issue list.
- No problems: use concise confirmation and show the last validation timestamp; do not replace the page with celebration art.

### Export and export history (`Exports`)

Purpose: make a new delivery artifact predictable while protecting the original work.

- Default route shows export history: output name/location, format, time, snapshot state, result, and `在文件夹中显示` action.
- `新建导出` opens a three-part dialog: destination/name, validation summary, and final confirmation. The dialog states `将创建新文件，原件不会被修改`.
- When validation blocks export, the primary action is `查看问题`; `仍然导出` is never offered for integrity blockers.
- Export progress can be backgrounded to Task Center. It presents stages in user language: prepare, write translation, check result, finish.
- Completion shows output path, validation result, and open/reveal actions. Failure retains the last successful export and says whether a partial staging file was cleaned.

### Tasks, diagnostics, recovery, and global search

- Task Center is a right-side sheet opened from the title bar. It lists active/recent local tasks with stage, progress, retry/cancel when safe, and a link to affected content.
- Diagnostic Drawer is reached from a task/error or project menu. Its default view is a human-readable summary; a copy/export detail action is explicit and warns that paths may be included.
- Recovery opens before a project workspace only when a decision is required. It presents the last confirmed save, any recoverable UI draft, and actions `恢复并继续`, `仅打开已保存内容`, and `返回项目库`. None discard content invisibly.
- Global search opens as a command dialog. Results group by source text, translation, chapter, and resource; selecting one navigates once to the relevant workspace target and closes the dialog.

## Core user flows

### First import to first translation

```text
Project library
  -> Import / choose source
  -> Confirm project
  -> Review extraction result
  -> Enter project overview
  -> Continue translation
  -> Content workspace at first untranslated unit
  -> Type translation
  -> Editing -> Saving -> Saved
```

Acceptance: user sees the original-file guarantee before project creation; a successful save does not navigate, scroll, or replace editor state.

### Cross-workspace context switch

```text
Selected unit in Content
  -> choose Units in workspace switcher
  -> Units route opens with same unit query
  -> selected row is scrolled into view and focused only after user input
  -> choose Resources only when selected unit has a linked region
  -> Resource workspace selects that region; otherwise explains no linked resource in place
```

Acceptance: no route loop, no synthetic duplicate unit selection, and no loss of unsaved local text. Switching to an unavailable representation is an in-place explanation, not a redirect.

### Image text workflow

```text
Resources
  -> choose image
  -> choose/add region
  -> review recognition result
  -> correct source text if needed
  -> type human translation
  -> choose typography/layout
  -> preview embedded text
  -> save region / create derivative
```

Acceptance: OCR never fills the target translation; the original image remains inspectable and unchanged after every action.

### Validate to export

```text
Validate
  -> choose blocking issue
  -> go to exact content location
  -> resolve and save
  -> Back to prior validation filter
  -> recheck
  -> New export
  -> destination + confirmation
  -> backgroundable task
  -> Export history
```

Acceptance: failed validation cannot be bypassed for integrity errors; export completion never sends the user to import or project library.

### Failure and recovery

```text
Write/edit or task interruption
  -> persistent error/task state
  -> user keeps visible typed text where possible
  -> retry / open details / return to work
  -> on restart, recovery route only if a decision is necessary
  -> explicit resume choice
```

Acceptance: all normal UI language explains the user-visible situation and safe data boundary; raw technical detail is opt-in.

## Command, shortcut, and state policy

| Command | Default shortcut | Availability | User-visible result |
| --- | --- | --- | --- |
| Global search | `Ctrl/Cmd+K` | all project routes | Opens search dialog; does not steal text input while IME is composing. |
| Undo / redo | `Ctrl/Cmd+Z` / `Ctrl/Cmd+Shift+Z` | when shared history permits | Updates the same project state in every workspace. |
| Save | `Ctrl/Cmd+S` | editor focus or project route | Requests save; indicator changes only after authoritative response. |
| Next / previous unit | `Alt+Down` / `Alt+Up` | Content and Units | Moves selection without changing filters. |
| Switch workspace | `Alt+1` / `Alt+2` / `Alt+3` | project routes | Content / Units / Resources; preserves compatible selected context. |
| Toggle inspector | `Alt+I` | workspaces | Opens/closes secondary pane without changing route. |
| Export | `Ctrl/Cmd+Shift+E` | project routes | Opens export flow; blockers are shown before choosing destination. |

- Keyboard shortcuts are proposed defaults pending product review and must be remappable or documented in a command menu before release.
- Commands with a browser/system collision may use native menu equivalents on Windows and Linux; no command is hidden only behind a shortcut.
- `Ctrl/Cmd+S` during IME composition waits for composition completion and never commits partial composition text.

## Design decisions requiring review

| Decision | Proposed choice | Why it matters |
| --- | --- | --- |
| Shell density | 48px top bar + rail/pane workbench | Keeps editing primary but commits to desktop-first layout. |
| Default editor comparison | Source context plus target-primary stacked/two-column adaptive view | Supports reading while reserving width for CJK writing. |
| Default theme | Light only in version one | Reduces visual QA surface; dark theme remains an open question. |
| Navigation | Project rail with five destinations; Import outside project | Avoids route recursion and reduces product-level navigation noise. |
| Component base | shadcn/ui + Radix + Tailwind + Lucide | Establishes consistent accessibility and interaction vocabulary. |
| Validation language | Human issue list, technical details collapsed | Keeps translators focused without hiding diagnosability. |
| OCR interaction | Candidate source only; target always human-entered | Enforces the product's non-AI translation boundary. |
| Export | Dedicated history route plus modal creation flow | Makes delivery auditable without turning workspaces into wizards. |

## Product review checklist

The product owner should review this checklist before UI implementation. Mark each item approved, revise the linked design section when needed, and leave unresolved items in `Open questions`.

### Product fit

- [ ] The first screen, project library, and import flow match the intended audience of individual translators rather than a generic SaaS audience.
- [ ] The product visibly prioritizes human translation and does not imply machine translation, chat assistance, or cloud dependency.
- [ ] TXT, Markdown, EPUB, and image text workflows have the right prominence for the first release.
- [ ] The design does not accidentally promise game-resource, audio, video, collaboration, or arbitrary-format support.

### Navigation and routing

- [ ] The five in-project destinations (`内容`, `单元`, `资源`, `校验`, `导出`) are the correct top-level mental model.
- [ ] Import belongs outside the project rail and project overview is sufficient as the return/orientation point.
- [ ] Cross-workspace switching should preserve selected context exactly as specified, including the unavailable-target behavior.
- [ ] Recovery, missing project, import cancel, validation resolution, and export completion have acceptable non-cyclic routes.
- [ ] No additional primary page is needed before implementation.

### Workspace usability

- [ ] Long-form mode gives sufficient space and reading comfort for extended CJK writing.
- [ ] Structured-unit mode has the right source/target density, bulk-state scope, and keyboard model.
- [ ] Resource mode gives enough control over OCR review, typography, layout, and derivative preview without exposing technical internals.
- [ ] Validation severity and export blocking rules match the desired delivery standard.
- [ ] The proposed save, task, failure, and recovery language feels trustworthy and understandable.

### Visual system

- [ ] The calm editorial/workbench tone is appropriate for Babel Tower.
- [ ] The proposed light palette, serif reading face, compact UI typography, spacing, and 6px radius are acceptable.
- [ ] The product should use this unified component foundation: shadcn/ui, Radix UI, Tailwind CSS, and Lucide.
- [ ] The stated anti-patterns are correct: no oversized cards, decorative gradients, AI aesthetics, terminal styling, or generic dashboard visuals.
- [ ] A version-one light-only theme is acceptable, or dark theme scope should be decided now.

### Delivery quality

- [ ] WCAG 2.2 AA, keyboard navigation, focus behavior, CJK IME, and reduced-motion behavior are release requirements.
- [ ] The target desktop widths and pane-collapse behavior are acceptable.
- [ ] Visual baseline screenshots and route/IME/interaction smoke tests are required before each significant UI merge.
- [ ] The listed open questions have owners before the affected implementation phase begins.

## Review outcome

- Decision status: pending product-owner review.
- Approval condition: all non-open checklist items are accepted or revised; any rejected decision is changed here before implementation.
- Next implementation artifact after approval: route map, token stylesheet, component inventory, and Storybook/Playwright baseline plan derived directly from this document.
