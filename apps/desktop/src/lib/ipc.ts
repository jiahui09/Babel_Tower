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

export async function saveTranslation(sourceUnitKey: string, text: string) {
  const commandId = Array.from(crypto.getRandomValues(new Uint8Array(32)))
    .map((value) => value.toString(16).padStart(2, "0"))
    .join("");
  return invoke<{ accepted: boolean; sequence: number; commitSequence: number | null }>("save_translation", {
    request: {
      sourceUnitKey,
      commandId,
      text,
      createdAtMs: Date.now(),
    },
  });
}
