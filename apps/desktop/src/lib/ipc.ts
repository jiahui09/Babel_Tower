import { invoke } from "@tauri-apps/api/core";

export type WorkspaceView = "LongForm" | "Units" | "Resources";

export interface UnitSummary {
  unitId: string;
  sourceUnitKey: string;
  sourceText: string;
  translation: string | null;
  localIndex: number;
}

export interface ProjectSummary {
  projectId: string;
  root: string;
  commitSequence: number;
}

export interface ProjectEntry {
  projectId: string;
  name: string;
  root: string;
  lastOpenedAtMs: number;
}

export interface WorkbenchSnapshot {
  schemaVersion: number;
  project: ProjectSummary;
  navigation: {
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
  } | null;
  units: UnitSummary[];
  currentUnit: {
    unitId: number[];
    sourceUnitKey: number[];
    sourceText: string;
    translation: string | null;
    projectCommitSequence: number;
  } | null;
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
  pages: Array<{
    page_index: number;
    width: number;
    height: number;
    regions: OcrRegion[];
  }>;
}

function commandId() {
  return Array.from(crypto.getRandomValues(new Uint8Array(32)))
    .map((value) => value.toString(16).padStart(2, "0"))
    .join("");
}

export async function saveImageRegionEdit(item: ResourceQueueItem, correctedSourceText: string) {
  return invoke<{ accepted: boolean; sequence: number; commitSequence: number | null }>(
    "save_image_region_edit",
    {
      request: {
        generationId: item.generationId,
        unitId: item.unitId,
        regionId: item.regionId,
        commandId: commandId(),
        correctedSourceText,
        createdAtMs: Date.now(),
      },
    },
  );
}

export async function recognizeImageRegion(item: ResourceQueueItem) {
  if (!item.imageResourceId) throw new Error("当前图片资源没有可读取的对象");
  return invoke<{ document: OcrDocument; replayed: boolean }>("ocr_image_region", {
    request: {
      generationId: item.generationId,
      regionId: item.regionId,
      imageResourceId: item.imageResourceId,
      profile: null,
    },
  });
}

export async function renderImageRegion(item: ResourceQueueItem, translation: string) {
  if (!item.imageResourceId) throw new Error("当前图片资源没有可读取的对象");
  return invoke<{ dataUrl: string; outputHash: string; commitSequence: number }>("render_image_region", {
    request: {
      generationId: item.generationId,
      unitId: item.unitId,
      regionId: item.regionId,
      imageResourceId: item.imageResourceId,
      polygon: item.polygon,
      translation,
    },
  });
}

export interface ResourceQueuePage {
  items: ResourceQueueItem[];
  nextCursor: { readingOrder: number; unitId: string } | null;
  projectCommitSequence: number;
}

export async function openProject(root: string) {
  return invoke<ProjectSummary>("open_project", { request: { root } });
}

export async function listProjects() {
  return invoke<ProjectEntry[]>("list_projects");
}

export async function importFile(sourcePath: string, projectRoot: string) {
  return invoke<{
    project: ProjectSummary;
    format: string;
    units: number;
    activated: boolean;
    reviewRequired: number;
  }>("import_file", { request: { sourcePath, projectRoot } });
}

export async function getWorkbenchSnapshot(view: WorkspaceView = "LongForm") {
  return invoke<WorkbenchSnapshot>("workbench_snapshot", { request: { view, limit: 100 } });
}

export async function getResourceQueue(after?: { readingOrder: number; unitId: string }, limit = 100) {
  return invoke<ResourceQueuePage>("resource_queue", {
    request: {
      afterReadingOrder: after?.readingOrder ?? null,
      afterUnitId: after?.unitId ?? null,
      limit,
    },
  });
}

export async function getImagePreview(item: ResourceQueueItem) {
  if (!item.imageResourceId) throw new Error("当前图片资源没有可读取的对象");
  return invoke<{
    mediaType: string;
    byteLength: number;
    sourceHash: string;
    dataUrl: string;
  }>("image_preview", {
    request: { generationId: item.generationId, resourceId: item.imageResourceId },
  });
}

export async function saveTranslation(sourceUnitKey: string, text: string) {
  return invoke<{ accepted: boolean; sequence: number; commitSequence: number | null }>("save_translation", {
    request: {
      sourceUnitKey,
      commandId: commandId(),
      text,
      createdAtMs: Date.now(),
    },
  });
}
