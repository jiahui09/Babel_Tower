import { Link, Outlet, useRouterState } from "@tanstack/react-router";
import { ChevronLeft, Download, PanelLeftClose, PanelLeftOpen, Search, ShieldCheck } from "lucide-react";

import { Button, buttonVariants } from "../ui/button";
import { Tooltip } from "../ui/tooltip";
import { cn } from "../../lib/utils";
import { useWorkbenchStore } from "../../stores/workbench";
import { SaveIndicator } from "./save-indicator";
import { WorkspaceSwitcher } from "./workspace-switcher";

const chapters = ["序章", "第一章　港口", "第二章　雨夜", "第三章　远航"];

export function AppShell({ projectId }: { projectId: string }) {
  const pathname = useRouterState({ select: (state) => state.location.pathname });
  const collapsed = useWorkbenchStore((state) => state.contextCollapsed);
  const toggleContext = useWorkbenchStore((state) => state.toggleContext);
  const resourceMode = pathname.endsWith("/resources");
  const unitMode = pathname.endsWith("/units");

  return (
    <div className="grid h-full grid-rows-[var(--topbar-height)_1fr] bg-[var(--surface)]">
      <header className="flex items-center gap-3 border-b border-[var(--border)] bg-[var(--surface-raised)] px-3">
        <Link
          to="/"
          className="flex min-w-[170px] items-center gap-2 text-sm font-semibold text-[var(--text)]"
        >
          <ChevronLeft size={16} aria-hidden="true" />
          暮色航线
        </Link>
        <WorkspaceSwitcher projectId={projectId} />
        <div className="ml-auto flex items-center gap-1">
          <SaveIndicator />
          <Tooltip label="搜索项目">
            <Button variant="icon" aria-label="搜索项目">
              <Search size={16} />
            </Button>
          </Tooltip>
          <Link
            to="/projects/$projectId/validate"
            params={{ projectId }}
            className={cn(buttonVariants({ variant: "ghost" }), "text-xs")}
          >
            <ShieldCheck size={16} />
            问题 2
          </Link>
          <Link
            to="/projects/$projectId/exports"
            params={{ projectId }}
            className={cn(buttonVariants({ variant: "primary" }), "text-xs")}
          >
            <Download size={16} />
            导出
          </Link>
        </div>
      </header>

      <div
        className={cn(
          "grid min-h-0",
          collapsed ? "grid-cols-[44px_1fr]" : "grid-cols-[var(--context-width)_1fr]",
        )}
      >
        <aside className="flex min-h-0 flex-col border-r border-[var(--border)] bg-[var(--surface-raised)]">
          <div className="flex h-11 items-center justify-between border-b border-[var(--border)] px-2">
            {!collapsed && (
              <span className="px-1 text-xs font-semibold text-[var(--text-secondary)]">
                {resourceMode ? "资源" : unitMode ? "筛选" : "章节"}
              </span>
            )}
            <Tooltip label={collapsed ? "展开上下文" : "收起上下文"}>
              <Button
                variant="icon"
                onClick={toggleContext}
                aria-label={collapsed ? "展开上下文" : "收起上下文"}
              >
                {collapsed ? <PanelLeftOpen size={16} /> : <PanelLeftClose size={16} />}
              </Button>
            </Tooltip>
          </div>
          {!collapsed && (
            <div className="min-h-0 overflow-auto p-2">
              {resourceMode ? <ResourceContext /> : unitMode ? <UnitContext /> : <ChapterContext />}
            </div>
          )}
        </aside>
        <main className="min-w-0 overflow-hidden">
          <Outlet />
        </main>
      </div>
    </div>
  );
}

function ChapterContext() {
  return (
    <ol className="m-0 list-none p-0">
      {chapters.map((chapter, index) => (
        <li
          key={chapter}
          className={cn(
            "flex h-9 items-center gap-2 rounded-[4px] px-2 text-sm",
            index === 1
              ? "bg-[var(--selection)] text-[var(--selection-text)]"
              : "text-[var(--text-secondary)]",
          )}
        >
          <span
            className={cn(
              "size-1.5 rounded-full",
              index === 0 ? "bg-[var(--accent)]" : "border border-[var(--border-strong)]",
            )}
          />
          <span className="truncate">{chapter}</span>
        </li>
      ))}
    </ol>
  );
}

function UnitContext() {
  return (
    <div className="space-y-3 text-sm">
      <label className="flex items-center gap-2 text-[var(--text-secondary)]">
        <input type="checkbox" defaultChecked />
        仅显示未完成
      </label>
      <div className="border-t border-[var(--border)] pt-3 text-xs text-[var(--text-muted)]">
        当前章节 · 18 / 42
      </div>
    </div>
  );
}

function ResourceContext() {
  return (
    <div className="px-2 py-3 text-sm leading-6 text-[var(--text-muted)]">当前项目尚未生成图片文字区域。</div>
  );
}
