import { useQuery } from "@tanstack/react-query";
import { ChevronRight, FileText, Folder, Image, Search, TreePine } from "lucide-react";
import { Button as AriaButton, Tree, TreeItem, TreeItemContent } from "react-aria-components";
import { useTranslation } from "react-i18next";
import { useState } from "react";

import { cn } from "../../lib/utils";
import { useDesktopBridge, type ProjectTreeNode } from "../../platform/desktop-bridge";
import { projectSearchQuery, projectTreeQuery } from "../../queries/project";
import { useWorkbenchStore, type ExplorerPanel } from "../../stores/workbench";
import { Button } from "../ui/button";
import { Input } from "../ui/input";
import { ScrollArea } from "../ui/scroll-area";
import { Tooltip } from "../ui/tooltip";

const panels: Array<{ id: ExplorerPanel; icon: typeof Folder; key: "resources" | "outline" | "search" }> = [
  { id: "resources", icon: Folder, key: "resources" },
  { id: "outline", icon: TreePine, key: "outline" },
  { id: "search", icon: Search, key: "search" },
];

export function ProjectExplorer({
  projectId,
  onOpenNode,
}: {
  projectId: string;
  onOpenNode: (node: ProjectTreeNode) => void;
}) {
  const { t } = useTranslation("explorer");
  const bridge = useDesktopBridge();
  const panel = useWorkbenchStore((state) => state.explorerPanel);
  const setPanel = useWorkbenchStore((state) => state.setExplorerPanel);
  const query = useQuery(projectTreeQuery(bridge, projectId));
  const [searchText, setSearchText] = useState("");
  const searchQuery = useQuery(projectSearchQuery(bridge, projectId, searchText));

  return (
    <aside
      className="grid h-full min-h-0 grid-rows-[32px_1fr] bg-[var(--surface-raised)]"
      aria-label={t("title")}
    >
      <div className="flex items-center border-b border-[var(--border)] px-1">
        {panels.map(({ id, icon: Icon, key }) => (
          <Tooltip key={id} label={t(key)}>
            <Button
              variant="icon"
              className={cn("size-7", panel === id && "bg-[var(--surface-inset)] text-[var(--text)]")}
              aria-label={t(key)}
              onClick={() => setPanel(id)}
            >
              <Icon size={15} />
            </Button>
          </Tooltip>
        ))}
      </div>
      {panel === "search" ? (
        <div className="p-2">
          <Input
            autoFocus
            type="search"
            value={searchText}
            onChange={(event) => setSearchText(event.target.value)}
            placeholder={t("search")}
            aria-label={t("search")}
          />
          <div className="mt-2 space-y-1">
            {searchText.trim().length < 2 ? (
              <p className="m-0 p-2 text-xs text-[var(--text-muted)]">{t("searchHint")}</p>
            ) : searchQuery.isPending ? (
              <p className="m-0 p-2 text-xs text-[var(--text-muted)]">{t("searching")}</p>
            ) : searchQuery.isError ? (
              <p className="m-0 p-2 text-xs text-[var(--danger)]">{searchQuery.error.message}</p>
            ) : searchQuery.data.length === 0 ? (
              <p className="m-0 p-2 text-xs text-[var(--text-muted)]">{t("noSearchResults")}</p>
            ) : (
              searchQuery.data.map((result) => (
                <button
                  key={result.unitId}
                  type="button"
                  className="block w-full border-b border-[var(--border)] p-2 text-left text-xs hover:bg-[var(--surface-inset)]"
                  onClick={() =>
                    onOpenNode({
                      id: result.unitId,
                      parentId: "source-root",
                      section: "source",
                      kind: "text",
                      name: result.sourceText,
                      semanticPath: result.sourceUnitKey,
                      capabilities: {
                        open: true,
                        createChild: false,
                        rename: false,
                        move: false,
                        delete: false,
                        reveal: false,
                        drop: false,
                      },
                    })
                  }
                >
                  <span className="block truncate text-[var(--text-secondary)]">{result.sourceText}</span>
                  {result.translation && (
                    <span className="mt-1 block truncate text-[var(--text-muted)]">{result.translation}</span>
                  )}
                </button>
              ))
            )}
          </div>
        </div>
      ) : query.isPending ? (
        <div className="p-3 text-xs text-[var(--text-muted)]">{t("loading", { ns: "common" })}</div>
      ) : query.isError ? (
        <div className="p-3 text-xs leading-5 text-[var(--danger)]">{query.error.message}</div>
      ) : (
        <ScrollArea className="min-h-0">
          <ProjectTree nodes={query.data.nodes} onOpenNode={onOpenNode} />
        </ScrollArea>
      )}
    </aside>
  );
}

function ProjectTree({
  nodes,
  onOpenNode,
}: {
  nodes: ProjectTreeNode[];
  onOpenNode: (node: ProjectTreeNode) => void;
}) {
  const { t } = useTranslation("explorer");
  const sections = (["source", "workspace", "derived"] as const).map((section) => ({
    section,
    nodes: nodes.filter((node) => node.section === section),
  }));
  return (
    <Tree aria-label={t("title")} selectionMode="single" className="p-1 text-sm">
      {sections.map(({ section, nodes: sectionNodes }) => (
        <TreeItem key={section} id={`section:${section}`} textValue={t(section)}>
          <TreeItemContent>
            {({ isExpanded, hasChildItems }) => (
              <div className="flex h-7 items-center gap-1 px-1 font-medium text-[var(--text-secondary)]">
                {hasChildItems ? (
                  <AriaButton slot="chevron" className="grid size-5 place-items-center">
                    <ChevronRight
                      size={13}
                      className={cn("transition-transform", isExpanded && "rotate-90")}
                    />
                  </AriaButton>
                ) : (
                  <span className="size-5" />
                )}
                <Folder size={14} />
                <span>{t(section)}</span>
                {section === "source" && (
                  <span className="ml-auto text-[10px] font-normal">{t("readonly")}</span>
                )}
              </div>
            )}
          </TreeItemContent>
          {sectionNodes
            .filter((node) => node.parentId === null)
            .map((root) => (
              <TreeItem key={root.id} id={root.id} textValue={t(section)}>
                <TreeItemContent>
                  {({ isExpanded, hasChildItems }) => (
                    <div className="flex h-7 items-center gap-1 px-1 font-medium text-[var(--text-secondary)]">
                      {hasChildItems ? (
                        <AriaButton slot="chevron" className="grid size-5 place-items-center">
                          <ChevronRight
                            size={13}
                            className={cn("transition-transform", isExpanded && "rotate-90")}
                          />
                        </AriaButton>
                      ) : (
                        <span className="size-5" />
                      )}
                      <Folder size={14} />
                      <span>{root.id === `${section}-root` ? t(section) : root.name}</span>
                    </div>
                  )}
                </TreeItemContent>
                {sectionNodes
                  .filter((node) => node.parentId === root.id)
                  .map((node) => (
                    <TreeItem
                      key={node.id}
                      id={node.id}
                      textValue={node.name}
                      onAction={() => node.capabilities.open && onOpenNode(node)}
                    >
                      <TreeItemContent>
                        {({ level, isSelected }) => (
                          <div
                            className={cn(
                              "flex h-7 items-center gap-1.5 rounded-[4px] pr-2 text-[var(--text-secondary)]",
                              isSelected && "bg-[var(--selection)] text-[var(--selection-text)]",
                            )}
                            style={{ paddingLeft: `${Math.max(level - 1, 0) * 14 + 8}px` }}
                          >
                            {node.kind === "image" ? <Image size={14} /> : <FileText size={14} />}
                            <span className="truncate">{node.name}</span>
                          </div>
                        )}
                      </TreeItemContent>
                    </TreeItem>
                  ))}
              </TreeItem>
            ))}
        </TreeItem>
      ))}
    </Tree>
  );
}
