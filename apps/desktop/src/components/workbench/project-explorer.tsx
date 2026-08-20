import { useQuery, useQueryClient } from "@tanstack/react-query";
import { ChevronRight, FilePlus, FileText, Folder, Image, Pencil, RotateCcw, Search, Trash2, TreePine } from "lucide-react";
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
  const queryClient = useQueryClient();
  const panel = useWorkbenchStore((state) => state.explorerPanel);
  const setPanel = useWorkbenchStore((state) => state.setExplorerPanel);
  const query = useQuery(projectTreeQuery(bridge, projectId));
  const [searchText, setSearchText] = useState("");
  const searchQuery = useQuery(projectSearchQuery(bridge, projectId, searchText));
  const [mutationError, setMutationError] = useState<string | null>(null);
  const [mutating, setMutating] = useState(false);

  const mutate = async (request: Parameters<typeof bridge.mutateWorkspace>[0]) => {
    setMutating(true);
    setMutationError(null);
    try {
      await bridge.mutateWorkspace(request);
      await queryClient.invalidateQueries({ queryKey: ["project-tree", projectId] });
    } catch (error) {
      setMutationError(error instanceof Error ? error.message : String(error));
    } finally {
      setMutating(false);
    }
  };

  const createFolder = () => {
    const name = globalThis.prompt(t("newFolder"));
    if (name?.trim()) void mutate({ kind: "createFolder", projectId, parentId: "workspace-root", name: name.trim() });
  };

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
          {mutationError && <p className="mt-2 p-2 text-xs text-[var(--danger)]">{mutationError}</p>}
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
          <div className="flex items-center justify-end border-b border-[var(--border)] px-2 py-1">
            <Button variant="icon" disabled={mutating} onClick={createFolder} aria-label={t("newFolder")}>
              <FilePlus size={14} />
            </Button>
          </div>
          {mutationError && <p className="m-0 p-2 text-xs text-[var(--danger)]">{mutationError}</p>}
          <ProjectTree projectId={projectId} nodes={query.data.nodes} onOpenNode={onOpenNode} onMutate={mutate} />
        </ScrollArea>
      )}
    </aside>
  );
}

function ProjectTree({
  projectId,
  nodes,
  onOpenNode,
  onMutate,
}: {
  projectId: string;
  nodes: ProjectTreeNode[];
  onOpenNode: (node: ProjectTreeNode) => void;
  onMutate: (request: Parameters<ReturnType<typeof useDesktopBridge>["mutateWorkspace"]>[0]) => Promise<void>;
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
                {sectionNodes.filter((node) => node.parentId === root.id).map((node) => (
                  <ProjectNode key={node.id} node={node} nodes={sectionNodes} projectId={projectId} onOpenNode={onOpenNode} onMutate={onMutate} />
                ))}
              </TreeItem>
            ))}
        </TreeItem>
      ))}
    </Tree>
  );
}

function ProjectNode({ node, nodes, projectId, onOpenNode, onMutate }: { node: ProjectTreeNode; nodes: ProjectTreeNode[]; projectId: string; onOpenNode: (node: ProjectTreeNode) => void; onMutate: (request: Parameters<ReturnType<typeof useDesktopBridge>["mutateWorkspace"]>[0]) => Promise<void> }) {
  const { t } = useTranslation("explorer");
  const children = nodes.filter((candidate) => candidate.parentId === node.id);
  const recycled = node.id.startsWith("recycle/");
  return (
    <TreeItem id={node.id} textValue={node.name} onAction={() => node.capabilities.open && onOpenNode(node)}>
      <TreeItemContent>{({ level, isSelected }) => (
        <div className={cn("group flex h-7 items-center gap-1.5 rounded-[4px] pr-2 text-[var(--text-secondary)]", isSelected && "bg-[var(--selection)] text-[var(--selection-text)]")} style={{ paddingLeft: `${Math.max(level - 1, 0) * 14 + 8}px` }}>
          {node.kind === "folder" ? <Folder size={14} /> : node.kind === "image" ? <Image size={14} /> : <FileText size={14} />}
          <span className="min-w-0 flex-1 truncate">{node.name}</span>
          {recycled && <button type="button" className="grid size-5 place-items-center opacity-0 group-hover:opacity-100" aria-label={t("restore")} onClick={(event) => { event.stopPropagation(); void onMutate({ kind: "restore", projectId, nodeId: node.id }); }}><RotateCcw size={12} /></button>}
          {node.capabilities.rename && <button type="button" className="grid size-5 place-items-center opacity-0 group-hover:opacity-100" aria-label={t("rename")} onClick={(event) => { event.stopPropagation(); const name = globalThis.prompt(t("rename"), node.name); if (name?.trim() && name.trim() !== node.name) void onMutate({ kind: "rename", projectId, nodeId: node.id, name: name.trim() }); }}><Pencil size={12} /></button>}
          {node.capabilities.delete && <button type="button" className="grid size-5 place-items-center opacity-0 group-hover:opacity-100" aria-label={t("moveToTrash")} onClick={(event) => { event.stopPropagation(); if (globalThis.confirm(t("confirmMoveToTrash", { name: node.name }))) void onMutate({ kind: "trash", projectId, nodeId: node.id }); }}><Trash2 size={12} /></button>}
        </div>
      )}</TreeItemContent>
      {children.map((child) => <ProjectNode key={child.id} node={child} nodes={nodes} projectId={projectId} onOpenNode={onOpenNode} onMutate={onMutate} />)}
    </TreeItem>
  );
}
