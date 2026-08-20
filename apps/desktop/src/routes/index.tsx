import { useQuery } from "@tanstack/react-query";
import { createFileRoute, Link } from "@tanstack/react-router";
import { ArrowRight, BookOpen, FilePlus2, Settings } from "lucide-react";
import { useTranslation } from "react-i18next";

import { Button, buttonVariants } from "../components/ui/button";
import { useDesktopBridge } from "../platform/desktop-bridge";
import { bootstrapQuery } from "../queries/project";
import { useWorkbenchStore } from "../stores/workbench";
import { cn } from "../lib/utils";

export const Route = createFileRoute("/")({ component: ProjectLibrary });

function ProjectLibrary() {
  const { t, i18n } = useTranslation(["workbench", "common"]);
  const bridge = useDesktopBridge();
  const projects = useQuery(bootstrapQuery(bridge));
  const setSettingsOpen = useWorkbenchStore((state) => state.setSettingsOpen);

  return (
    <div className="h-full overflow-auto bg-[var(--surface)]">
      <header className="flex h-12 items-center border-b border-[var(--border)] bg-[var(--surface-raised)] px-5">
        <h1 className="m-0 text-base font-semibold">{t("appName", { ns: "common" })}</h1>
        <div className="ml-auto flex items-center gap-1">
          <Button
            variant="icon"
            onClick={() => setSettingsOpen(true)}
            aria-label={t("openSettings", { ns: "workbench" })}
          >
            <Settings size={16} />
          </Button>
          <Link to="/import" className={cn(buttonVariants({ variant: "primary" }), "ml-1")}>
            <FilePlus2 size={16} />
            {t("importWork", { ns: "workbench" })}
          </Link>
        </div>
      </header>
      <main className="mx-auto max-w-[1040px] px-6 py-8">
        <h2 className="mb-3 text-sm font-semibold">{t("continueWorking", { ns: "workbench" })}</h2>
        {projects.isPending ? (
          <p className="text-sm text-[var(--text-muted)]">{t("loading", { ns: "common" })}</p>
        ) : projects.isError ? (
          <section className="border-y border-[var(--border)] bg-[var(--surface-raised)] px-4 py-6">
            <h3 className="m-0 text-sm font-semibold">{t("loadProjectsFailed", { ns: "workbench" })}</h3>
            <p className="mt-2 text-xs leading-5 text-[var(--danger)]">{projects.error.message}</p>
            <Button className="mt-3" onClick={() => void projects.refetch()}>
              {t("retry", { ns: "common" })}
            </Button>
          </section>
        ) : projects.data.projects.length === 0 ? (
          <section className="border-y border-[var(--border)] py-10 text-center">
            <BookOpen size={24} className="mx-auto text-[var(--text-muted)]" />
            <h3 className="mb-1 mt-3 text-sm font-semibold">{t("noProjects", { ns: "workbench" })}</h3>
            <p className="m-0 text-xs text-[var(--text-secondary)]">
              {t("noProjectsDetail", { ns: "workbench" })}
            </p>
            <Link to="/import" className={cn(buttonVariants({ variant: "primary" }), "mt-4")}>
              <FilePlus2 size={16} />
              {t("importWork", { ns: "workbench" })}
            </Link>
          </section>
        ) : (
          <div className="border-y border-[var(--border)] bg-[var(--surface-raised)]">
            {projects.data.projects.map((project) => (
              <article
                key={project.projectId}
                className="grid grid-cols-[40px_1fr_auto] items-center gap-3 border-b border-[var(--border)] px-3 py-3 last:border-b-0"
              >
                <div className="grid size-9 place-items-center rounded-[var(--radius)] bg-[var(--surface-inset)] text-[var(--accent)]">
                  <BookOpen size={18} />
                </div>
                <div className="min-w-0">
                  <h3 className="m-0 truncate text-sm font-semibold">{project.name}</h3>
                  <p className="m-0 mt-1 truncate text-xs text-[var(--text-muted)]">
                    {t("lastOpened", {
                      ns: "workbench",
                      time: new Intl.DateTimeFormat(i18n.language, {
                        dateStyle: "medium",
                        timeStyle: "short",
                      }).format(project.lastOpenedAtMs),
                    })}
                  </p>
                </div>
                <Link
                  to="/projects/$projectId/content"
                  params={{ projectId: project.projectId }}
                  search={{ unitId: undefined }}
                  className={buttonVariants({ variant: "secondary" })}
                >
                  {t("continueTranslation", { ns: "workbench" })}
                  <ArrowRight size={15} />
                </Link>
              </article>
            ))}
          </div>
        )}
      </main>
    </div>
  );
}
