import { open, save } from "@tauri-apps/plugin-dialog";

import { requireTauriRuntime } from "../platform/tauri-runtime";

export async function chooseSourceFile() {
  requireTauriRuntime("chooseSourceFile");
  const selected = await open({
    multiple: false,
    directory: false,
    filters: [{ name: "支持的作品", extensions: ["txt", "md", "markdown", "epub"] }],
  });
  return typeof selected === "string" ? selected : null;
}

export async function chooseWorkspaceFiles() {
  requireTauriRuntime("chooseWorkspaceFiles");
  const selected = await open({ multiple: true, directory: false });
  if (!selected) return [];
  return Array.isArray(selected) ? selected : [selected];
}

export async function chooseProjectDirectory() {
  requireTauriRuntime("chooseProjectDirectory");
  const selected = await open({ multiple: false, directory: true, recursive: false });
  return typeof selected === "string" ? selected : null;
}

export async function chooseExportPath() {
  requireTauriRuntime("chooseExportPath");
  const selected = await save({
    filters: [{ name: "翻译作品", extensions: ["txt", "md", "epub"] }],
  });
  return selected ?? null;
}
