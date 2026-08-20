import { createFileRoute, Link, useNavigate } from "@tanstack/react-router";
import { FolderOpen, FileUp, FolderPlus } from "lucide-react";
import { useRef, useState } from "react";
import { useTranslation } from "react-i18next";

import { Button, buttonVariants } from "../components/ui/button";
import { Input } from "../components/ui/input";
import { chooseProjectDirectory, chooseSourceFile } from "../lib/dialog";
import { useDesktopBridge } from "../platform/desktop-bridge";

export const Route = createFileRoute("/import")({ component: ImportPage });

export type ImportValidationError = "missingProjectDirectory" | "missingSourceFile";

export function validateImportRequest(root: string, sourcePath: string): ImportValidationError | null {
  if (!root.trim()) return "missingProjectDirectory";
  if (!sourcePath.trim()) return "missingSourceFile";
  return null;
}

export function ImportPage() {
  const navigate = useNavigate();
  const bridge = useDesktopBridge();
  const { t } = useTranslation(["common", "workbench", "explorer"]);
  const [root, setRoot] = useState("");
  const [sourcePath, setSourcePath] = useState("");
  const [projectName, setProjectName] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [opening, setOpening] = useState(false);
  const [importing, setImporting] = useState(false);
  const projectDirectoryInput = useRef<HTMLInputElement>(null);

  const handleOpen = async () => {
    if (!root.trim()) return;
    setOpening(true);
    setError(null);
    try {
      const project = await bridge.openProject(root.trim());
      await navigate({
        to: "/projects/$projectId/content",
        params: { projectId: project.projectId },
        search: { unitId: undefined },
      });
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setOpening(false);
    }
  };

  const handleChooseRoot = async () => {
    try {
      const selected = await chooseProjectDirectory();
      if (selected) {
        setRoot(selected);
        setError(null);
      }
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    }
  };

  const handleChooseSource = async () => {
    try {
      const selected = await chooseSourceFile();
      if (selected) {
        setSourcePath(selected);
        setError(null);
      }
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    }
  };

  const handleImport = async () => {
    const validationError = validateImportRequest(root, sourcePath);
    if (validationError) {
      setError(t(`workbench:${validationError}`));
      if (validationError === "missingProjectDirectory") projectDirectoryInput.current?.focus();
      return;
    }
    setImporting(true);
    setError(null);
    try {
      const result = await bridge.importFile(sourcePath.trim(), root.trim());
      await navigate({
        to: "/projects/$projectId/content",
        params: { projectId: result.project.projectId },
        search: { unitId: undefined },
      });
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setImporting(false);
    }
  };

  const handleCreate = async () => {
    if (!root.trim() || !projectName.trim()) return;
    setOpening(true);
    setError(null);
    try {
      const project = await bridge.createProject({ name: projectName.trim(), parentDirectory: root.trim() });
      await navigate({ to: "/projects/$projectId/content", params: { projectId: project.projectId }, search: { unitId: undefined } });
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setOpening(false);
    }
  };

  return (
    <div className="h-full overflow-auto bg-[var(--surface)] p-8">
      <div className="mx-auto max-w-[760px]">
        <Link to="/" className={buttonVariants({ variant: "ghost" })}>
          {t("common:back")}
        </Link>
        <h1 className="mt-8 text-xl font-semibold">{t("workbench:importWork")}</h1>
        <div className="mt-6 flex min-h-[260px] flex-col items-center justify-center border border-dashed border-[var(--border-strong)] bg-[var(--surface-raised)] px-8 text-center">
          <FileUp size={28} className="text-[var(--accent)]" />
          <p className="mb-1 mt-4 text-sm font-medium">{t("workbench:chooseSupportedFile")}</p>
          <p className="m-0 text-xs text-[var(--text-muted)]">{t("workbench:originalPreserved")}</p>
          <p className="mb-0 mt-2 text-xs text-[var(--text-secondary)]">
            {t("workbench:importRequirements")}
          </p>
          <label className="mt-5 flex w-full max-w-[520px] items-center gap-2">
            <span className="sr-only">{t("workbench:projectName")}</span>
            <Input value={projectName} onChange={(event) => setProjectName(event.target.value)} placeholder={t("workbench:projectNamePlaceholder")} className="h-9 min-w-0 flex-1" />
            <Button type="button" onClick={handleCreate} disabled={opening || !root.trim() || !projectName.trim()} variant="primary">
              <FolderPlus size={16} />
              {t("workbench:createProject")}
            </Button>
          </label>
          <label className="mt-3 flex w-full max-w-[520px] items-center gap-2">
            <span className="sr-only">{t("workbench:projectDirectory")}</span>
            <Input
              ref={projectDirectoryInput}
              value={root}
              onChange={(event) => {
                setRoot(event.target.value);
                setError(null);
              }}
              placeholder={t("workbench:projectDirectoryPlaceholder")}
              className="h-9 min-w-0 flex-1"
            />
            <Button
              type="button"
              onClick={handleChooseRoot}
              className={buttonVariants({ variant: "icon" })}
              aria-label={t("workbench:chooseProjectDirectory")}
              title={t("workbench:chooseProjectDirectory")}
            >
              <FolderOpen size={16} />
            </Button>
            <Button
              type="button"
              onClick={handleOpen}
              disabled={opening}
              className={buttonVariants({ variant: "primary" })}
            >
              {opening ? t("workbench:opening") : t("workbench:openProject")}
            </Button>
          </label>
          <label className="mt-3 flex w-full max-w-[520px] items-center gap-2">
            <span className="sr-only">{t("workbench:sourceFilePath")}</span>
            <Input
              value={sourcePath}
              onChange={(event) => {
                setSourcePath(event.target.value);
                setError(null);
              }}
              placeholder={t("workbench:sourceFilePlaceholder")}
              className="h-9 min-w-0 flex-1"
            />
            <Button
              type="button"
              onClick={handleChooseSource}
              className={buttonVariants({ variant: "icon" })}
              aria-label={t("workbench:chooseSourceFile")}
              title={t("workbench:chooseSourceFile")}
            >
              <FolderOpen size={16} />
            </Button>
            <Button
              type="button"
              onClick={handleImport}
              disabled={importing}
              className={buttonVariants({ variant: "secondary" })}
            >
              {importing ? t("workbench:importing") : t("workbench:importFile")}
            </Button>
          </label>
          {error && (
            <p className="mb-0 mt-3 text-xs text-[var(--danger)]" role="alert">
              {error}
            </p>
          )}
        </div>
      </div>
    </div>
  );
}
