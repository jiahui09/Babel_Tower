import { useQuery } from "@tanstack/react-query";
import { Link, Outlet, useNavigate, useRouterState } from "@tanstack/react-router";
import {
  ChevronLeft,
  Download,
  PanelLeftClose,
  PanelLeftOpen,
  PanelRightClose,
  PanelRightOpen,
  Search,
  ShieldCheck,
} from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";

import type { CommandContext } from "../../commands/registry";
import { useDesktopBridge, type ProjectTreeNode } from "../../platform/desktop-bridge";
import { bootstrapQuery, openProjectQuery, projectSnapshotQuery } from "../../queries/project";
import { useWorkbenchStore, type WorkbenchTab } from "../../stores/workbench";
import { Button, buttonVariants } from "../ui/button";
import { ResizableHandle, ResizablePanel, ResizablePanelGroup } from "../ui/resizable";
import { Tooltip } from "../ui/tooltip";
import { ApplicationMenubar, CommandPalette, useCommandShortcuts } from "./command-surfaces";
import { DocumentTabs } from "./document-tabs";
import { InspectorPanelView } from "./inspector-panel";
import { ProblemsPanel } from "./problems-panel";
import { ProjectExplorer } from "./project-explorer";
import { SaveIndicator } from "./save-indicator";
import { SecondaryEditorGroup } from "./secondary-editor-group";
import { WorkspaceSwitcher } from "./workspace-switcher";

export function AppShell({ projectId }: { projectId: string }) {
  const { t } = useTranslation(["workbench", "common", "menu", "errors"]);
  const bridge = useDesktopBridge();
  const navigate = useNavigate();
  const pathname = useRouterState({ select: (state) => state.location.pathname });
  const bootstrap = useQuery(bootstrapQuery(bridge));
  const projectEntry = bootstrap.data?.projects.find((project) => project.projectId === projectId);
  const openQuery = useQuery(openProjectQuery(bridge, projectEntry));
  const snapshotQuery = useQuery({
    ...projectSnapshotQuery(bridge, projectId),
    enabled: openQuery.isSuccess,
  });
  const explorerOpen = useWorkbenchStore((state) => state.explorerOpen);
  const inspectorOpen = useWorkbenchStore((state) => state.inspectorOpen);
  const problemsOpen = useWorkbenchStore((state) => state.problemsOpen);
  const focusMode = useWorkbenchStore((state) => state.focusMode);
  const explorerWidth = useWorkbenchStore((state) => state.explorerWidth);
  const inspectorWidth = useWorkbenchStore((state) => state.inspectorWidth);
  const toggleExplorer = useWorkbenchStore((state) => state.toggleExplorer);
  const toggleInspector = useWorkbenchStore((state) => state.toggleInspector);
  const setProblemsOpen = useWorkbenchStore((state) => state.setProblemsOpen);
  const toggleFocusMode = useWorkbenchStore((state) => state.toggleFocusMode);
  const setCommandPaletteOpen = useWorkbenchStore((state) => state.setCommandPaletteOpen);
  const setSettingsOpen = useWorkbenchStore((state) => state.setSettingsOpen);
  const setPanelWidths = useWorkbenchStore((state) => state.setPanelWidths);
  const openTab = useWorkbenchStore((state) => state.openTab);
  const activeTabId = useWorkbenchStore((state) => state.groups[0]?.activeTabId);
  const activeTab = useWorkbenchStore((state) => state.tabs.find((tab) => tab.id === activeTabId));
  const secondaryHasTabs = useWorkbenchStore((state) => Boolean(state.groups.find((group) => group.id === "secondary")?.tabIds.length));
  const splitRatio = useWorkbenchStore((state) => state.splitRatio);
  const setSplitRatio = useWorkbenchStore((state) => state.setSplitRatio);
  const wideWindow = useMediaQuery("(min-width: 1440px)");
  const inspectorVisible = inspectorOpen && wideWindow && !focusMode;
  const explorerVisible = explorerOpen && !focusMode;
  const projectName = bootstrap.data?.projects.find((project) => project.projectId === projectId)?.name;
  const navigationUnitId = snapshotQuery.data?.navigation?.position.unitId ?? null;
  const currentUnit =
    snapshotQuery.data?.units.find((unit) => unit.unitId === navigationUnitId) ??
    (snapshotQuery.data?.currentUnit
      ? (snapshotQuery.data.units.find((unit) => unit.unitId === snapshotQuery.data?.currentUnit?.unitId) ??
        null)
      : null);

  useEffect(() => {
    if (!snapshotQuery.data) return;
    if (pathname.endsWith("/source") && activeTab?.kind === "source") return;
    if (pathname.endsWith("/diff") && activeTab?.kind === "diff") return;
    openTab(
      tabForPath(pathname, projectId, {
        longForm: t("longForm", { ns: "workbench" }),
        units: t("units", { ns: "workbench" }),
        resources: t("resources", { ns: "workbench" }),
        issues: t("issues", { ns: "workbench" }),
        export: t("export", { ns: "menu" }),
        source: t("sourceView", { ns: "editor" }),
        diff: t("diffView", { ns: "editor" }),
      }),
      "primary",
    );
  }, [activeTab?.kind, openTab, pathname, projectId, snapshotQuery.data, t]);

  const context = useMemo<CommandContext>(
    () => ({
      projectId,
      activeUnitId: currentUnit?.unitId ?? null,
      bridge,
      actions: {
        importWork: async () => navigate({ to: "/import" }),
        openExports: async () => navigate({ to: "/projects/$projectId/exports", params: { projectId } }),
        openValidation: async () => {
          setProblemsOpen(true);
        },
        openSettings: () => setSettingsOpen(true),
        openCommandPalette: () => setCommandPaletteOpen(true),
        toggleExplorer,
        toggleInspector,
        toggleFocusMode,
      },
    }),
    [
      bridge,
      currentUnit?.unitId,
      navigate,
      projectId,
      setCommandPaletteOpen,
      setSettingsOpen,
      setProblemsOpen,
      toggleExplorer,
      toggleFocusMode,
      toggleInspector,
    ],
  );
  useCommandShortcuts(context);

  if (bootstrap.isSuccess && !projectEntry) {
    return (
      <div className="grid h-full place-items-center bg-[var(--surface)] p-8">
        <section className="max-w-[560px] border border-[var(--border)] bg-[var(--surface-raised)] p-6">
          <h1 className="m-0 text-base font-semibold">{t("workbench:projectUnavailable")}</h1>
          <p className="mt-3 text-sm leading-6 text-[var(--text-secondary)]">
            {t("workbench:projectUnavailableDetail")}
          </p>
          <Link to="/" className={buttonVariants({ variant: "secondary" })}>
            {t("common:back")}
          </Link>
        </section>
      </div>
    );
  }

  if (bootstrap.isPending || openQuery.isPending || snapshotQuery.isPending) {
    return (
      <div className="grid h-full place-items-center bg-[var(--surface)] text-sm text-[var(--text-muted)]">
        {t("common:loading")}
      </div>
    );
  }
  if (bootstrap.isError || openQuery.isError || snapshotQuery.isError) {
    const loadError = bootstrap.error ?? openQuery.error ?? snapshotQuery.error;
    return (
      <div className="grid h-full place-items-center bg-[var(--surface)] p-8">
        <section className="max-w-[560px] border border-[var(--border)] bg-[var(--surface-raised)] p-6">
          <h1 className="m-0 text-base font-semibold">{t("workbench:projectUnavailable")}</h1>
          <p className="mt-3 text-sm leading-6 text-[var(--text-secondary)]">
            {t("workbench:projectUnavailableDetail")}
          </p>
          <pre className="max-h-32 overflow-auto bg-[var(--surface-inset)] p-3 text-xs text-[var(--danger)]">
            {loadError instanceof Error ? loadError.message : String(loadError)}
          </pre>
          <div className="mt-4 flex gap-2">
            <Link to="/" className={buttonVariants({ variant: "secondary" })}>
              {t("common:back")}
            </Link>
            <Button variant="primary" onClick={() => void snapshotQuery.refetch()}>
              {t("common:retry")}
            </Button>
          </div>
        </section>
      </div>
    );
  }

  const snapshot = snapshotQuery.data;
  const onActivateTab = (tab: WorkbenchTab) => navigateToTab(tab, projectId, navigate);
  const openNode = (node: ProjectTreeNode) => {
    openTab({
      id: `source:${node.id}`,
      projectId,
      kind: "source",
      title: node.name,
      unitId: node.id,
      dirty: false,
    });
    void navigate({ to: "/projects/$projectId/source", params: { projectId } });
  };

  return (
    <div
      className={`grid h-full bg-[var(--surface)] ${problemsOpen ? "grid-rows-[28px_40px_minmax(0,1fr)_220px_24px]" : "grid-rows-[28px_40px_minmax(0,1fr)_24px]"}`}
    >
      <header className="flex items-center border-b border-[var(--border)] bg-[var(--surface-raised)] px-1">
        <ApplicationMenubar context={context} />
        <span className="ml-auto pr-2 text-[11px] text-[var(--text-muted)]">Babel Tower</span>
      </header>
      <div className="flex min-w-0 items-center gap-2 border-b border-[var(--border)] bg-[var(--surface-raised)] px-2">
        <Link
          to="/"
          className={buttonVariants({ variant: "icon" })}
          aria-label={t("workbench:projectLibrary")}
        >
          <ChevronLeft size={16} />
        </Link>
        <span className="max-w-[220px] truncate text-sm font-semibold">
          {projectName ?? snapshot.project.projectId}
        </span>
        <WorkspaceSwitcher projectId={projectId} />
        <div className="ml-auto flex items-center gap-1">
          <SaveIndicator />
          <Tooltip label={t("menu:toggleExplorer")}>
            <Button variant="icon" onClick={toggleExplorer} aria-label={t("menu:toggleExplorer")}>
              {explorerVisible ? <PanelLeftClose size={16} /> : <PanelLeftOpen size={16} />}
            </Button>
          </Tooltip>
          <Tooltip label={t("menu:commandPalette")}>
            <Button
              variant="icon"
              onClick={() => setCommandPaletteOpen(true)}
              aria-label={t("menu:commandPalette")}
            >
              <Search size={16} />
            </Button>
          </Tooltip>
          <Tooltip label={t("menu:toggleInspector")}>
            <Button variant="icon" onClick={toggleInspector} aria-label={t("menu:toggleInspector")}>
              {inspectorVisible ? <PanelRightClose size={16} /> : <PanelRightOpen size={16} />}
            </Button>
          </Tooltip>
          <Button variant="ghost" onClick={() => void context.actions.openValidation()}>
            <ShieldCheck size={16} />
            {t("workbench:issues")}
          </Button>
          <Button variant="primary" onClick={() => void context.actions.openExports()}>
            <Download size={16} />
            {t("menu:export")}
          </Button>
        </div>
      </div>
      <ResizablePanelGroup orientation="horizontal" className="min-h-0">
        {explorerVisible && (
          <>
            <ResizablePanel
              id="explorer"
              defaultSize={explorerWidth}
              minSize={200}
              maxSize={420}
              onResize={(size) => setPanelWidths(Math.round(size.inPixels), inspectorWidth)}
            >
              <ProjectExplorer projectId={projectId} onOpenNode={openNode} />
            </ResizablePanel>
            <ResizableHandle />
          </>
        )}
        <ResizablePanel id="editor" minSize={560}>
          {secondaryHasTabs ? (
            <ResizablePanelGroup orientation="horizontal" className="min-h-0">
              <ResizablePanel id="primary-editor-group" defaultSize={splitRatio * 100} minSize={320} onResize={(size) => setSplitRatio(Math.max(0.25, Math.min(0.75, size.asPercentage / 100)))}>
                <main className="grid h-full min-h-0 grid-rows-[32px_1fr]">
                  <DocumentTabs groupId="primary" onActivate={onActivateTab} />
                  <div className="min-h-0 overflow-hidden"><Outlet /></div>
                </main>
              </ResizablePanel>
              <ResizableHandle />
              <ResizablePanel id="secondary-editor-group" minSize={320}>
                <SecondaryEditorGroup projectId={projectId} fallbackUnitId={currentUnit?.unitId ?? null} />
              </ResizablePanel>
            </ResizablePanelGroup>
          ) : (
            <main className="grid h-full min-h-0 grid-rows-[32px_1fr]">
              <DocumentTabs groupId="primary" onActivate={onActivateTab} />
              <div className="min-h-0 overflow-hidden"><Outlet /></div>
            </main>
          )}
        </ResizablePanel>
        {inspectorVisible && (
          <>
            <ResizableHandle />
            <ResizablePanel
              id="inspector"
              defaultSize={inspectorWidth}
              minSize={260}
              maxSize={440}
              onResize={(size) => setPanelWidths(explorerWidth, Math.round(size.inPixels))}
            >
              <InspectorPanelView snapshot={snapshot} currentUnitId={currentUnit?.unitId ?? null} />
            </ResizablePanel>
          </>
        )}
      </ResizablePanelGroup>
      {problemsOpen && <ProblemsPanel projectId={projectId} onClose={() => setProblemsOpen(false)} />}
      <footer className="flex items-center gap-3 border-t border-[var(--border)] bg-[var(--surface-raised)] px-2 text-[11px] text-[var(--text-muted)]">
        <span>{projectName ?? snapshot.project.projectId}</span>
        <span>
          {t("workbench:unitPosition", {
            current: currentUnit ? currentUnit.localIndex + 1 : 0,
            total: snapshot.units.length,
          })}
        </span>
        <span className="ml-auto">#{snapshot.project.commitSequence}</span>
      </footer>
      <CommandPalette context={context} />
    </div>
  );
}

function useMediaQuery(query: string) {
  const [matches, setMatches] = useState(() => window.matchMedia(query).matches);
  useEffect(() => {
    const media = window.matchMedia(query);
    const update = () => setMatches(media.matches);
    update();
    media.addEventListener("change", update);
    return () => media.removeEventListener("change", update);
  }, [query]);
  return matches;
}

function tabForPath(
  pathname: string,
  projectId: string,
  labels: {
    longForm: string;
    units: string;
    resources: string;
    issues: string;
    export: string;
    source: string;
    diff: string;
  },
): WorkbenchTab {
  if (pathname.endsWith("/units"))
    return { id: "units", projectId, kind: "unitCollection", title: labels.units, dirty: false };
  if (pathname.endsWith("/resources"))
    return { id: "resources", projectId, kind: "image", title: labels.resources, dirty: false };
  if (pathname.endsWith("/validate"))
    return { id: "validate", projectId, kind: "unitCollection", title: labels.issues, dirty: false };
  if (pathname.endsWith("/exports"))
    return { id: "exports", projectId, kind: "source", title: labels.export, dirty: false };
  if (pathname.endsWith("/source"))
    return { id: "source", projectId, kind: "source", title: labels.source, dirty: false };
  if (pathname.endsWith("/diff"))
    return { id: "diff", projectId, kind: "diff", title: labels.diff, dirty: false };
  return { id: "content", projectId, kind: "chapter", title: labels.longForm, dirty: false };
}

function navigateToTab(tab: WorkbenchTab, projectId: string, navigate: ReturnType<typeof useNavigate>) {
  if (tab.id === "units") return void navigate({ to: "/projects/$projectId/units", params: { projectId } });
  if (tab.id === "resources")
    return void navigate({ to: "/projects/$projectId/resources", params: { projectId } });
  if (tab.id === "validate")
    return void navigate({
      to: "/projects/$projectId/validate",
      params: { projectId },
      search: { unitId: undefined },
    });
  if (tab.id === "exports")
    return void navigate({ to: "/projects/$projectId/exports", params: { projectId } });
  if (tab.kind === "source")
    return void navigate({ to: "/projects/$projectId/source", params: { projectId } });
  if (tab.kind === "diff") return void navigate({ to: "/projects/$projectId/diff", params: { projectId } });
  return void navigate({
    to: "/projects/$projectId/content",
    params: { projectId },
    search: { unitId: undefined },
  });
}
