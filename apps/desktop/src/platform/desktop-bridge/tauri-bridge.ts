import { invoke } from "@tauri-apps/api/core";

import { requireTauriRuntime } from "../tauri-runtime";
import { plainTextDocument, projectDocumentText } from "./document";
import { BridgeError, normalizeBridgeError } from "./error";
import type {
  AppSettingsV1,
  CommitReceipt,
  DesktopBridge,
  ExportRecord,
  ExportRequest,
  ProjectEntry,
  ProjectSnapshot,
  RedoRequest,
  SaveDraftRequest,
  SaveNavigationRequest,
  SaveNavigationResult,
  SaveTranslationDocumentRequest,
  SettingsPatch,
  TranslationWorkItem,
  UndoRequest,
  ValidationReport,
  TermRecord,
  AnnotationRecord,
  WorkbenchPageRequest,
  WorkspaceMutation,
  ResourceQueuePage,
  ImagePreview,
  OcrDocument,
} from "./types";

const DEFAULT_SETTINGS: AppSettingsV1 = {
  schemaVersion: 1,
  language: "zh-CN",
  theme: "system",
  density: "compact",
  editorFontFamily: '"Noto Serif SC", "Source Han Serif SC", serif',
  readingFontSize: 18,
  lineHeight: 1.8,
  wordWrap: true,
  shortcutOverrides: {},
  panelWidths: { explorer: 260, inspector: 320 },
};

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  requireTauriRuntime(command);
  try {
    return await invoke<T>(command, args);
  } catch (reason) {
    const message = reason instanceof Error ? reason.message : String(reason);
    if (/__TAURI__|invoke|not found|not a function/i.test(message)) {
      throw new BridgeError("ipc_unavailable", message, { command });
    }
    throw normalizeBridgeError(reason);
  }
}

export class TauriDesktopBridge implements DesktopBridge {
  createProject(request: import("./types").CreateProjectRequest) {
    return call<import("./types").ProjectSummary>("create_project", { request });
  }

  openProject(root: string) {
    return call<{ projectId: string; root: string; commitSequence: number }>("open_project", {
      request: { root },
    });
  }

  importFile(sourcePath: string, projectRoot: string) {
    return call<{
      project: { projectId: string; root: string; commitSequence: number };
      format: string;
      units: number;
      activated: boolean;
      reviewRequired: number;
    }>("import_file", { request: { sourcePath, projectRoot } });
  }

  importWorkspaceFiles(request: { projectId: string; parentId: string; sourcePaths: string[] }) {
    return call<import("./types").WorkspaceMutationReceipt>("import_workspace_files", { request });
  }

  createWorkspaceFile(request: { projectId: string; parentId: string; name: string }) {
    return call<import("./types").WorkspaceMutationReceipt>("create_workspace_file", { request });
  }

  readWorkspaceFile(request: { projectId: string; nodeId: string }) {
    return call<import("./types").WorkspaceFile>("read_workspace_file", { request });
  }

  writeWorkspaceFile(request: {
    projectId: string;
    nodeId: string;
    content: string;
    expectedModifiedAtMs?: number;
  }) {
    return call<import("./types").WorkspaceFile>("write_workspace_file", { request });
  }

  readWorkspaceState(projectId: string) {
    return call<import("./types").WorkspaceStateV1 | null>("read_workspace_state", {
      request: { projectId },
    });
  }

  writeWorkspaceState(projectId: string, state: import("./types").WorkspaceStateV1) {
    return call<void>("write_workspace_state", { request: { projectId, state } });
  }
  async bootstrap() {
    const projects = await call<ProjectEntry[]>("list_projects");
    let settings = DEFAULT_SETTINGS;
    try {
      settings = await this.getSettings();
    } catch (reason) {
      if (!(reason instanceof BridgeError) || reason.code !== "not_implemented") throw reason;
    }
    return { projects, settings };
  }

  projectSnapshot() {
    return call<ProjectSnapshot>("workbench_snapshot", {
      request: { view: "LongForm", limit: 100 },
    });
  }

  async projectTree(request: import("./types").ProjectTreeRequest) {
    return call<import("./types").ProjectTreeSnapshot>("project_tree", { request });
  }

  searchProject(request: { projectId: string; query: string; limit?: number }) {
    return call<import("./types").ProjectSearchResult[]>("search_project", { request });
  }

  async workbenchPage(request: WorkbenchPageRequest) {
    const snapshot = await call<ProjectSnapshot>("workbench_snapshot", {
      request: { view: request.view, limit: request.limit ?? 100 },
    });
    return {
      items: snapshot.units,
      nextCursor: null,
      projectCommitSequence: snapshot.project.commitSequence,
    };
  }

  async workItem(projectId: string, unitId: string): Promise<TranslationWorkItem> {
    void projectId;
    const item = await call<{
      sourceUnitKey: number[];
      sourceText: string;
      translation: string | null;
      translationDocument: TranslationWorkItem["translation"];
      status: "Untranslated" | "Draft" | "Translated" | "Reviewed" | "Blocked";
      revisionId: number | null;
      projectCommitSequence: number;
    }>("work_item", { request: { unitId, view: "LongForm" } });
    const text = item.translation ?? "";
    return {
      unitId,
      sourceUnitKey: item.sourceUnitKey.map((value) => value.toString(16).padStart(2, "0")).join(""),
      sourceText: item.sourceText,
      translation: item.translationDocument ?? plainTextDocument(text),
      translationText: text,
      status: item.status.toLowerCase() as TranslationWorkItem["status"],
      revisionId: item.revisionId,
      projectCommitSequence: item.projectCommitSequence,
    };
  }

  saveTranslationDocument(request: SaveTranslationDocumentRequest) {
    return call<CommitReceipt>("save_translation", {
      request: {
        sourceUnitKey: request.sourceUnitKey,
        commandId: request.commandId,
        text: projectDocumentText(request.document),
        document: request.document,
        expectedRevisionId: request.expectedRevisionId,
        createdAtMs: request.createdAtMs,
      },
    });
  }

  async saveDraft(request: SaveDraftRequest) {
    await call<void>("save_draft", {
      request: {
        unitId: request.unitId,
        document: request.document,
        updatedAtMs: request.updatedAtMs,
      },
    });
  }

  saveNavigation(request: SaveNavigationRequest) {
    return call<SaveNavigationResult>("save_navigation", { request });
  }

  undo(request: UndoRequest) {
    return call<CommitReceipt>("undo_translation", { request });
  }

  redo(request: RedoRequest) {
    return call<CommitReceipt>("redo_translation", { request });
  }

  validate(projectId: string) {
    return call<ValidationReport>("validate_project", { request: { projectId } });
  }

  termsForUnit(request: { projectId: string; sourceText: string }) {
    void request.projectId;
    return call<TermRecord[]>("find_terms", { request: { text: request.sourceText, limit: 20 } });
  }

  annotationsForUnit(request: { projectId: string; unitId: string }) {
    void request.projectId;
    return call<AnnotationRecord[]>("annotations_for_unit", { request: { unitId: request.unitId } });
  }

  listExports(projectId: string) {
    return call<ExportRecord[]>("list_exports", { request: { projectId } });
  }

  createExport(request: ExportRequest) {
    return call<ExportRecord>("create_export", { request });
  }

  getSettings() {
    return call<AppSettingsV1>("get_settings").catch((reason) => {
      if (reason instanceof BridgeError && reason.code === "ipc_unavailable") throw reason;
      throw new BridgeError("not_implemented", "Settings persistence is not available");
    });
  }

  patchSettings(request: SettingsPatch) {
    return call<AppSettingsV1>("patch_settings", { request });
  }

  mutateWorkspace(request: WorkspaceMutation) {
    return call<never>("mutate_workspace", { request });
  }

  resourceQueue(request: { after?: { readingOrder: number; unitId: string }; limit?: number } = {}) {
    return call<ResourceQueuePage>("resource_queue", {
      request: {
        afterReadingOrder: request.after?.readingOrder ?? null,
        afterUnitId: request.after?.unitId ?? null,
        limit: request.limit ?? 100,
      },
    });
  }

  imagePreview(request: { generationId: string; resourceId: string }) {
    return call<ImagePreview>("image_preview", { request });
  }

  recognizeImageRegion(request: { generationId: string; regionId: string; imageResourceId: string }) {
    return call<{ document: OcrDocument; replayed: boolean }>("ocr_image_region", {
      request: { ...request, profile: null },
    });
  }

  renderImageRegion(request: {
    generationId: string;
    unitId: string;
    regionId: string;
    imageResourceId: string;
    polygon: [number, number][];
    translation: string;
  }) {
    return call<{ dataUrl: string; outputHash: string; commitSequence: number }>("render_image_region", {
      request,
    });
  }

  saveImageRegionEdit(request: {
    generationId: string;
    unitId: string;
    regionId: string;
    correctedSourceText: string;
  }) {
    return call<CommitReceipt>("save_image_region_edit", {
      request: { ...request, commandId: crypto.randomUUID().replace(/-/g, ""), createdAtMs: Date.now() },
    });
  }
}
