import { createFileRoute, Link } from "@tanstack/react-router";
import { ArrowRight, BookOpen, FilePlus2 } from "lucide-react";
import { useEffect, useState } from "react";

import { buttonVariants } from "../components/ui/button";
import { listProjects, type ProjectEntry } from "../lib/ipc";
import { cn } from "../lib/utils";

export const Route = createFileRoute("/")({ component: ProjectLibrary });

function ProjectLibrary() {
  const [projects, setProjects] = useState<ProjectEntry[]>([]);
  useEffect(() => {
    void listProjects()
      .then(setProjects)
      .catch(() => undefined);
  }, []);
  const project = projects[0];

  return (
    <div className="h-full overflow-auto bg-[var(--surface)]">
      <header className="flex h-14 items-center border-b border-[var(--border)] bg-[var(--surface-raised)] px-6">
        <h1 className="text-lg font-semibold">Babel Tower</h1>
        <Link to="/import" className={cn(buttonVariants({ variant: "primary" }), "ml-auto")}>
          <FilePlus2 size={16} />
          导入作品
        </Link>
      </header>
      <div className="mx-auto max-w-[960px] px-6 py-10">
        <h2 className="mb-3 text-base font-semibold">继续工作</h2>
        <article className="grid grid-cols-[48px_1fr_auto] items-center gap-4 border-y border-[var(--border)] bg-[var(--surface-raised)] px-4 py-4">
          <div className="flex size-10 items-center justify-center rounded-[6px] bg-[var(--surface-inset)] text-[var(--accent)]">
            <BookOpen size={20} />
          </div>
          <div className="min-w-0">
            <h3 className="m-0 text-sm font-semibold">{project?.name ?? "暮色航线"}</h3>
            <p className="m-0 mt-1 text-xs text-[var(--text-secondary)]">
              {project ? "已登记项目 · 可继续翻译" : "预览项目 · 第一章「港口」· 18 / 42"}
            </p>
          </div>
          <Link
            to="/projects/$projectId/content"
            params={{ projectId: project?.projectId ?? "preview" }}
            className={buttonVariants({ variant: "secondary" })}
          >
            继续翻译
            <ArrowRight size={15} />
          </Link>
        </article>
      </div>
    </div>
  );
}
