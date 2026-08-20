export type WorkspaceView = "LongForm" | "Units" | "Resources";

export type TranslationStatus = "untranslated" | "draft" | "translated" | "reviewed" | "blocked";

export type TextMark =
  | { kind: "bold" }
  | { kind: "italic" }
  | { kind: "strike" }
  | { kind: "code" }
  | { kind: "link"; href: string };

export type TranslationInline =
  | { kind: "text"; text: string; marks: TextMark[] }
  | { kind: "protected"; tokenId: string; label: string; signature: string }
  | { kind: "placeholder"; name: string; rule: string };

export interface TranslationBlock {
  kind: "paragraph" | "heading" | "quote" | "listItem" | "codeBlock";
  inlines: TranslationInline[];
}

export interface TranslationDocumentV1 {
  schemaVersion: 1;
  blocks: TranslationBlock[];
}

export interface ProjectEntry {
  projectId: string;
  name: string;
  root: string;
  lastOpenedAtMs: number;
}

export interface AppBootstrap {
  projects: ProjectEntry[];
  settings: AppSettingsV1;
}

export interface OpenProjectResult {
  project: ProjectSummary;
}
export interface ImportResult {
  project: ProjectSummary;
  format: string;
  units: number;
  activated: boolean;
  reviewRequired: number;
}

export interface ProjectSummary {
  projectId: string;
  root: string;
  commitSequence: number;
}

export interface CreateProjectRequest {
  name: string;
  parentDirectory: string;
}

export interface WorkspaceFile {
  nodeId: string;
  uri: string;
  name: string;
  content: string;
  readonly: boolean;
  modifiedAtMs: number;
}

export interface WorkspaceStateV1 {
  schemaVersion: 1;
  tabs: Array<{
    id: string;
    uri?: string;
    title: string;
    kind: string;
    readonly: boolean;
    pinned: boolean;
  }>;
  groups: Array<{ id: "primary" | "secondary"; tabIds: string[]; activeTabId: string | null }>;
  expandedNodeIds: string[];
  selectedNodeId: string | null;
}

export interface UnitSummary {
  unitId: string;
  sourceUnitKey: string;
  sourceText: string;
  translation: string | null;
  localIndex: number;
  status?: TranslationStatus;
}

export interface NavigationSnapshot {
  position: {
    view: WorkspaceView;
    unitId: string | null;
    resourceId: string | null;
    regionId: string | null;
    scrollAnchorUnitId: string | null;
    scrollOffsetPx: number;
    zoomMillionths: number;
    filters: Record<string, unknown>;
  };
  clientSessionId: string;
  positionSequence: number;
  updatedAtMs: number;
}

export interface SaveNavigationRequest {
  projectId: string;
  view: WorkspaceView;
  unitId?: string | null;
  resourceId?: string | null;
  regionId?: string | null;
  scrollAnchorUnitId?: string | null;
  scrollOffsetPx?: number;
  zoomMillionths?: number;
  filters?: Record<string, unknown>;
  clientSessionId: string;
  positionSequence: number;
  updatedAtMs: number;
}

export interface SaveNavigationResult {
  accepted: boolean;
  sequence: number;
}

export interface ProjectSnapshot {
  schemaVersion: number;
  project: ProjectSummary;
  navigation: NavigationSnapshot | null;
  units: UnitSummary[];
  currentUnit: {
    unitId: string;
    sourceUnitKey: string;
    sourceText: string;
    translation: string | null;
    projectCommitSequence: number;
  } | null;
}

export interface WorkbenchPageRequest {
  projectId: string;
  view: WorkspaceView;
  after?: { localIndex: number; unitId: string };
  limit?: number;
  filters?: Record<string, unknown>;
}

export interface WorkbenchPage {
  items: UnitSummary[];
  nextCursor: { localIndex: number; unitId: string } | null;
  projectCommitSequence: number;
}

export interface TranslationWorkItem {
  unitId: string;
  sourceUnitKey: string;
  sourceText: string;
  translation: TranslationDocumentV1;
  translationText: string;
  status: TranslationStatus;
  revisionId: number | null;
  projectCommitSequence: number;
}

export interface SaveTranslationDocumentRequest {
  projectId: string;
  unitId: string;
  sourceUnitKey: string;
  commandId: string;
  expectedRevisionId: number | null;
  document: TranslationDocumentV1;
  createdAtMs: number;
}

export interface SaveDraftRequest {
  projectId: string;
  unitId: string;
  document: TranslationDocumentV1;
  updatedAtMs: number;
}

export interface UndoRequest {
  projectId: string;
  unitId: string;
  commandId: string;
  createdAtMs: number;
}

export type RedoRequest = UndoRequest;

export interface CommitReceipt {
  accepted: boolean;
  sequence: number;
  commitSequence: number | null;
  revisionId?: string | null;
}

export interface ProjectTreeRequest {
  projectId: string;
}

export interface ProjectTreeNode {
  id: string;
  parentId: string | null;
  section: "source" | "workspace" | "derived";
  kind: "root" | "folder" | "text" | "chapter" | "image" | "resource";
  name: string;
  semanticPath: string;
  mappedPath?: string;
  capabilities: {
    open: boolean;
    createChild: boolean;
    rename: boolean;
    move: boolean;
    delete: boolean;
    reveal: boolean;
    drop: boolean;
  };
}

export interface ProjectTreeSnapshot {
  nodes: ProjectTreeNode[];
  commitSequence: number;
}

export interface ProjectSearchResult {
  unitId: string;
  sourceUnitKey: string;
  sourceText: string;
  translation: string | null;
  localIndex: number;
}

export type WorkspaceMutation =
  | { kind: "createFolder"; projectId: string; parentId: string; name: string }
  | { kind: "rename"; projectId: string; nodeId: string; name: string }
  | { kind: "move"; projectId: string; nodeId: string; parentId: string }
  | { kind: "trash"; projectId: string; nodeId: string }
  | { kind: "restore"; projectId: string; nodeId: string }
  | { kind: "reveal"; projectId: string; nodeId: string };

export interface WorkspaceMutationReceipt {
  operationId: string;
  commitSequence: number;
  affectedNodeIds: string[];
}

export interface ResourceQueueItem {
  generationId: string;
  unitId: string;
  sourceUnitKey: string;
  sourceText: string;
  translation: string | null;
  readingOrder: number;
  regionId: string;
  regionSemanticPath: string;
  imageResourceId: string | null;
  imageSemanticPath: string | null;
  polygon: [number, number][];
  coordinateSpace: string;
  correctedSourceText: string | null;
}
export interface ResourceQueuePage {
  items: ResourceQueueItem[];
  nextCursor: { readingOrder: number; unitId: string } | null;
  projectCommitSequence: number;
}
export interface ImagePreview {
  mediaType: string;
  byteLength: number;
  sourceHash: string;
  dataUrl: string;
}
export interface OcrRegion {
  reading_order: number;
  polygon: { x: number; y: number }[];
  block_type: string;
  language: string | null;
  text: string;
  normalized_text: string;
  confidence_millionths: number;
}
export interface OcrDocument {
  schema_version: number;
  source_hash_hex: string;
  input_kind: string;
  profile: Record<string, unknown>;
  engine: {
    engine_id: string;
    engine_version: string;
    runtime: string;
    runtime_version: string;
    model_ids: string[];
  };
  pages: Array<{ page_index: number; width: number; height: number; regions: OcrRegion[] }>;
}

export interface ValidationIssue {
  id: string;
  severity: "blocking" | "warning" | "info";
  messageKey: string;
  unitId?: string;
  resourceId?: string;
  detail?: string;
}

export interface ValidationReport {
  issues: ValidationIssue[];
  checkedAtMs: number;
  projectCommitSequence: number;
}

export interface TermRecord {
  termId: string;
  sourceText: string;
  preferredTranslation: string;
  notes: string;
  state: string;
  variants: string[];
}

export interface AnnotationRecord {
  annotationId: string;
  unitId: string;
  baseRevisionId: number | null;
  currentRevisionId: number | null;
  graphemeStart: number;
  graphemeEnd: number;
  body: string;
  state: string;
  stale: boolean;
}

export interface ExportRecord {
  id: string;
  createdAtMs: number;
  path: string;
  format: string;
  outputHash: string | null;
  status: "running" | "succeeded" | "failed";
  error?: string;
}

export interface ExportRequest {
  projectId: string;
  destinationPath: string;
  commandId: string;
  createdAtMs: number;
}

export type AppLanguage = "zh-CN" | "en-US";
export type AppTheme = "light" | "dark" | "system";
export type InterfaceDensity = "compact" | "comfortable";

export interface AppSettingsV1 {
  schemaVersion: 1;
  language: AppLanguage;
  theme: AppTheme;
  density: InterfaceDensity;
  editorFontFamily: string;
  readingFontSize: number;
  lineHeight: number;
  wordWrap: boolean;
  shortcutOverrides: Record<string, string[]>;
  panelWidths: { explorer: number; inspector: number };
}

export type SettingsPatch = Partial<Omit<AppSettingsV1, "schemaVersion">>;

export interface DesktopBridge {
  bootstrap(): Promise<AppBootstrap>;
  createProject(request: CreateProjectRequest): Promise<ProjectSummary>;
  openProject(root: string): Promise<ProjectSummary>;
  importFile(sourcePath: string, projectRoot: string): Promise<ImportResult>;
  importWorkspaceFiles(request: {
    projectId: string;
    parentId: string;
    sourcePaths: string[];
  }): Promise<WorkspaceMutationReceipt>;
  createWorkspaceFile(request: {
    projectId: string;
    parentId: string;
    name: string;
  }): Promise<WorkspaceMutationReceipt>;
  readWorkspaceFile(request: { projectId: string; nodeId: string }): Promise<WorkspaceFile>;
  writeWorkspaceFile(request: {
    projectId: string;
    nodeId: string;
    content: string;
    expectedModifiedAtMs?: number;
  }): Promise<WorkspaceFile>;
  readWorkspaceState(projectId: string): Promise<WorkspaceStateV1 | null>;
  writeWorkspaceState(projectId: string, state: WorkspaceStateV1): Promise<void>;
  projectSnapshot(projectId: string): Promise<ProjectSnapshot>;
  projectTree(request: ProjectTreeRequest): Promise<ProjectTreeSnapshot>;
  searchProject(request: {
    projectId: string;
    query: string;
    limit?: number;
  }): Promise<ProjectSearchResult[]>;
  workbenchPage(request: WorkbenchPageRequest): Promise<WorkbenchPage>;
  workItem(projectId: string, unitId: string): Promise<TranslationWorkItem>;
  saveTranslationDocument(request: SaveTranslationDocumentRequest): Promise<CommitReceipt>;
  saveDraft(request: SaveDraftRequest): Promise<void>;
  saveNavigation(request: SaveNavigationRequest): Promise<SaveNavigationResult>;
  undo(request: UndoRequest): Promise<CommitReceipt>;
  redo(request: RedoRequest): Promise<CommitReceipt>;
  validate(projectId: string): Promise<ValidationReport>;
  termsForUnit(request: { projectId: string; sourceText: string }): Promise<TermRecord[]>;
  annotationsForUnit(request: { projectId: string; unitId: string }): Promise<AnnotationRecord[]>;
  listExports(projectId: string): Promise<ExportRecord[]>;
  createExport(request: ExportRequest): Promise<ExportRecord>;
  getSettings(): Promise<AppSettingsV1>;
  patchSettings(request: SettingsPatch): Promise<AppSettingsV1>;
  mutateWorkspace(request: WorkspaceMutation): Promise<WorkspaceMutationReceipt>;
  resourceQueue(request?: {
    after?: { readingOrder: number; unitId: string };
    limit?: number;
  }): Promise<ResourceQueuePage>;
  imagePreview(request: { generationId: string; resourceId: string }): Promise<ImagePreview>;
  recognizeImageRegion(request: {
    generationId: string;
    regionId: string;
    imageResourceId: string;
  }): Promise<{ document: OcrDocument; replayed: boolean }>;
  renderImageRegion(request: {
    generationId: string;
    unitId: string;
    regionId: string;
    imageResourceId: string;
    polygon: [number, number][];
    translation: string;
  }): Promise<{ dataUrl: string; outputHash: string; commitSequence: number }>;
  saveImageRegionEdit(request: {
    generationId: string;
    unitId: string;
    regionId: string;
    correctedSourceText: string;
  }): Promise<CommitReceipt>;
}
