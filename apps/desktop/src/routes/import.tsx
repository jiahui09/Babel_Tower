import { createFileRoute, Link, useNavigate } from "@tanstack/react-router";
import { FolderOpen, FileUp } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";

import { Button, buttonVariants } from "../components/ui/button";
import { Input } from "../components/ui/input";
import { chooseProjectDirectory, chooseSourceFile } from "../lib/dialog";
import { useDesktopBridge } from "../platform/desktop-bridge";

export const Route = createFileRoute("/import")({ component: ImportPage });

function ImportPage() {
  const navigate = useNavigate();
  const bridge = useDesktopBridge();
  const { t } = useTranslation(["common", "workbench", "explorer"]);
  const [root, setRoot] = useState("");
  const [sourcePath, setSourcePath] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [opening, setOpening] = useState(false);
  const [importing, setImporting] = useState(false);

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
      if (selected) setRoot(selected);
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    }
  };

  const handleChooseSource = async () => {
    try {
      const selected = await chooseSourceFile();
      if (selected) setSourcePath(selected);
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    }
  };

  const handleImport = async () => {
    if (!sourcePath.trim() || !root.trim()) return;
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
          <label className="mt-5 flex w-full max-w-[520px] items-center gap-2">
            <span className="sr-only">{t("workbench:projectDirectory")}</span>
            <Input
              value={root}
              onChange={(event) => setRoot(event.target.value)}
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
              onChange={(event) => setSourcePath(event.target.value)}
              placeholder={t("workbench:sourceFilePlaceholder")}
              className="h-9 min-w-0 flex-1"
            />
            <Button
              onClick={handleChooseSource}
              className={buttonVariants({ variant: "icon" })}
              aria-label={t("workbench:chooseSourceFile")}
              title={t("workbench:chooseSourceFile")}
            >
              <FolderOpen size={16} />
            </Button>
            <Button
              onClick={handleImport}
              disabled={importing}
              className={buttonVariants({ variant: "secondary" })}
            >
              {importing ? t("workbench:importing") : t("workbench:importFile")}
            </Button>
          </label>
          {error && <p className="mb-0 mt-3 text-xs text-[var(--danger)]">{error}</p>}
        </div>
      </div>
    </div>
  );
}
