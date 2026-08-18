import { createFileRoute, Link, useNavigate } from "@tanstack/react-router";
import { FileUp } from "lucide-react";
import { useState } from "react";

import { buttonVariants } from "../components/ui/button";
import { importFile, openProject } from "../lib/ipc";

export const Route = createFileRoute("/import")({ component: ImportPage });

function ImportPage() {
  const navigate = useNavigate();
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
      const project = await openProject(root.trim());
      await navigate({ to: "/projects/$projectId/content", params: { projectId: project.projectId } });
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setOpening(false);
    }
  };

  const handleImport = async () => {
    if (!sourcePath.trim() || !root.trim()) return;
    setImporting(true);
    setError(null);
    try {
      const result = await importFile(sourcePath.trim(), root.trim());
      await navigate({ to: "/projects/$projectId/content", params: { projectId: result.project.projectId } });
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
          返回项目库
        </Link>
        <h1 className="mt-8 text-xl font-semibold">导入作品</h1>
        <div className="mt-6 flex min-h-[260px] flex-col items-center justify-center border border-dashed border-[var(--border-strong)] bg-[var(--surface-raised)] px-8 text-center">
          <FileUp size={28} className="text-[var(--accent)]" />
          <p className="mb-1 mt-4 text-sm font-medium">选择 TXT、Markdown 或 EPUB 文件</p>
          <p className="m-0 text-xs text-[var(--text-muted)]">原文件会被保留，项目使用本地副本工作。</p>
          <label className="mt-5 flex w-full max-w-[520px] items-center gap-2">
            <span className="sr-only">项目目录</span>
            <input
              value={root}
              onChange={(event) => setRoot(event.target.value)}
              placeholder="输入已创建的项目目录"
              className="h-9 min-w-0 flex-1 border border-[var(--border)] bg-[var(--surface)] px-3 text-sm outline-none focus:border-[var(--accent)]"
            />
            <button
              type="button"
              onClick={handleOpen}
              disabled={opening}
              className={buttonVariants({ variant: "primary" })}
            >
              {opening ? "打开中" : "打开项目"}
            </button>
          </label>
          <label className="mt-3 flex w-full max-w-[520px] items-center gap-2">
            <span className="sr-only">原始文件路径</span>
            <input
              value={sourcePath}
              onChange={(event) => setSourcePath(event.target.value)}
              placeholder="输入 TXT、Markdown 或 EPUB 文件路径"
              className="h-9 min-w-0 flex-1 border border-[var(--border)] bg-[var(--surface)] px-3 text-sm outline-none focus:border-[var(--accent)]"
            />
            <button
              type="button"
              onClick={handleImport}
              disabled={importing}
              className={buttonVariants({ variant: "secondary" })}
            >
              {importing ? "导入中" : "导入文件"}
            </button>
          </label>
          {error && <p className="mb-0 mt-3 text-xs text-[var(--danger)]">{error}</p>}
        </div>
      </div>
    </div>
  );
}
