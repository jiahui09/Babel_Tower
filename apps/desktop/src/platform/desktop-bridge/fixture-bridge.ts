import { BridgeError } from "./error";
import type { DesktopBridge } from "./types";

export function createFixtureBridge(overrides: Partial<DesktopBridge>): DesktopBridge {
  const missing = async (method: string): Promise<never> => {
    throw new BridgeError("not_implemented", `Fixture does not implement ${method}`);
  };
  return {
    bootstrap: () => missing("bootstrap"),
    openProject: () => missing("openProject"),
    importFile: () => missing("importFile"),
    projectSnapshot: () => missing("projectSnapshot"),
    projectTree: () => missing("projectTree"),
    searchProject: () => missing("searchProject"),
    workbenchPage: () => missing("workbenchPage"),
    workItem: () => missing("workItem"),
    saveTranslationDocument: () => missing("saveTranslationDocument"),
    saveDraft: () => missing("saveDraft"),
    saveNavigation: () => missing("saveNavigation"),
    undo: () => missing("undo"),
    redo: () => missing("redo"),
    validate: () => missing("validate"),
    termsForUnit: () => missing("termsForUnit"),
    annotationsForUnit: () => missing("annotationsForUnit"),
    listExports: () => missing("listExports"),
    createExport: () => missing("createExport"),
    getSettings: () => missing("getSettings"),
    patchSettings: () => missing("patchSettings"),
    mutateWorkspace: () => missing("mutateWorkspace"),
    resourceQueue: () => missing("resourceQueue"),
    imagePreview: () => missing("imagePreview"),
    recognizeImageRegion: () => missing("recognizeImageRegion"),
    renderImageRegion: () => missing("renderImageRegion"),
    saveImageRegionEdit: () => missing("saveImageRegionEdit"),
    ...overrides,
  };
}
