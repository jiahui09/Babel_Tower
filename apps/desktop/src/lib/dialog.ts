import { open, save } from "@tauri-apps/plugin-dialog";

export async function chooseSourceFile() {
  const selected = await open({
    multiple: false,
    directory: false,
    filters: [{ name: "支持的作品", extensions: ["txt", "md", "markdown", "epub"] }],
  });
  return typeof selected === "string" ? selected : null;
}

export async function chooseProjectDirectory() {
  const selected = await open({ multiple: false, directory: true, recursive: false });
  return typeof selected === "string" ? selected : null;
}

export async function chooseExportPath() {
  const selected = await save({
    filters: [{ name: "翻译作品", extensions: ["txt", "md", "epub"] }],
  });
  return selected ?? null;
}
