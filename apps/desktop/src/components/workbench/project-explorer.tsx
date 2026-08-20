import { useQuery, useQueryClient } from "@tanstack/react-query";
import {
  ChevronRight,
  FilePlus,
  FolderInput,
  FileText,
  Folder,
  Image,
  Pencil,
  RotateCcw,
  Search,
  Trash2,
  TreePine,
} from "lucide-react";
import { Button as AriaButton, Tree, TreeItem, TreeItemContent } from "react-aria-components";
import { useTranslation } from "react-i18next";
import { useEffect, useState } from "react";

import { cn } from "../../lib/utils";
import { useDesktopBridge, type ProjectTreeNode } from "../../platform/desktop-bridge";
import { projectSearchQuery, projectTreeQuery } from "../../queries/project";
import { useWorkbenchStore, type ExplorerPanel } from "../../stores/workbench";
import { useWorkspaceStore } from "../../stores/workspace";
import { chooseWorkspaceFiles } from "../../lib/dialog";
import { Button } from "../ui/button";
import { Dialog, DialogContent, DialogDescription, DialogTitle } from "../ui/dialog";
import { Input } from "../ui/input";
import { ScrollArea } from "../ui/scroll-area";
import { Tooltip } from "../ui/tooltip";

const panels: Array<{ id: ExplorerPanel; icon: typeof Folder; key: "resources" | "outline" | "search" }> = [
  { id: "resources", icon: Folder, key: "resources" },
  { id: "outline", icon: TreePine, key: "outline" },
  { id: "search", icon: Search, key: "search" },
];

type PendingWorkspaceMutation =
  | { kind: "createFolder"; parentId: string; name: string }
  | { kind: "createFile"; parentId: string; name: string }
  | { kind: "rename"; node: ProjectTreeNode; name: string }
  | { kind: "trash"; node: ProjectTreeNode }
  | null;

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
  const selectedNodeId = useWorkspaceStore((state) => state.selectedNodeId);
  const setSelectedNodeId = useWorkspaceStore((state) => state.setSelected);
  const loadTree = useWorkspaceStore((state) => state.loadTree);
  const reconcileWorkspaceFiles = useWorkbenchStore((state) => state.reconcileWorkspaceFiles);
  const query = useQuery(projectTreeQuery(bridge, projectId));
  const [searchText, setSearchText] = useState("");
  const searchQuery = useQuery(projectSearchQuery(bridge, projectId, searchText));
  const [mutationError, setMutationError] = useState<string | null>(null);
  const [mutating, setMutating] = useState(false);
  const [pendingMutation, setPendingMutation] = useState<PendingWorkspaceMutation>(null);

  const mutate = async (request: Parameters<typeof bridge.mutateWorkspace>[0]): Promise<boolean> => {
    setMutating(true);
    setMutationError(null);
    try {
      await bridge.mutateWorkspace(request);
      await queryClient.invalidateQueries({ queryKey: ["project", projectId, "tree"] });
      return true;
    } catch (error) {
      setMutationError(error instanceof Error ? error.message : String(error));
      return false;
    } finally {
      setMutating(false);
    }
  };

  const workspaceRoot = query.data?.nodes.find((node) => node.id === "workspace-root");

  useEffect(() => {
    if (!query.data) return;
    loadTree(projectId, query.data.nodes);
    reconcileWorkspaceFiles(
      projectId,
      query.data.nodes
        .filter((node) => node.section === "workspace" && node.kind === "resource")
        .map((node) => ({ nodeId: node.id, uri: node.mappedPath, title: node.name })),
    );
  }, [loadTree, projectId, query.data, reconcileWorkspaceFiles]);

  const importFiles = async () => {
    const sourcePaths = await chooseWorkspaceFiles();
    if (!sourcePaths.length || !workspaceRoot) return;
    setMutating(true);
    setMutationError(null);
    try {
      const receipt = await bridge.importWorkspaceFiles({
        projectId,
        parentId: workspaceRoot.id,
        sourcePaths,
      });
      await queryClient.invalidateQueries({ queryKey: ["project", projectId, "tree"] });
      const last = receipt.affectedNodeIds[receipt.affectedNodeIds.length - 1];
      if (last) setSelectedNodeId(last);
    } catch (error) {
      setMutationError(error instanceof Error ? error.message : String(error));
    } finally {
      setMutating(false);
    }
  };

  const handleDrop = async (event: React.DragEvent<HTMLElement>) => {
    event.preventDefault();
    if (!workspaceRoot) return;
    const sourcePaths = Array.from(event.dataTransfer.files)
      .map((file) => (file as File & { path?: string }).path)
      .filter((path): path is string => Boolean(path));
    if (!sourcePaths.length) return;
    setMutating(true);
    setMutationError(null);
    try {
      const receipt = await bridge.importWorkspaceFiles({
        projectId,
        parentId: workspaceRoot.id,
        sourcePaths,
      });
      await queryClient.invalidateQueries({ queryKey: ["project", projectId, "tree"] });
      const last = receipt.affectedNodeIds[receipt.affectedNodeIds.length - 1];
      if (last) setSelectedNodeId(last);
    } catch (error) {
      setMutationError(error instanceof Error ? error.message : String(error));
    } finally {
      setMutating(false);
    }
  };

  return (
    <aside
      className="grid h-full min-h-0 grid-rows-[32px_1fr] bg-[var(--surface-raised)]"
      aria-label={t("title")}
      onDragOver={(event) => event.preventDefault()}
      onDrop={(event) => void handleDrop(event)}
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
            <Button
              variant="icon"
              disabled={mutating || panel !== "resources" || !workspaceRoot?.capabilities.createChild}
              onClick={() =>
                workspaceRoot &&
                setPendingMutation({ kind: "createFile", parentId: workspaceRoot.id, name: "" })
              }
              aria-label={t("newFile")}
              title={t("newFile")}
            >
              <FileText size={14} />
            </Button>
            <Button
              variant="icon"
              disabled={mutating || panel !== "resources" || !workspaceRoot?.capabilities.createChild}
              onClick={() =>
                workspaceRoot &&
                setPendingMutation({ kind: "createFolder", parentId: workspaceRoot.id, name: "" })
              }
              aria-label={t("newFolder")}
              title={panel === "resources" ? t("newFolder") : t("resources")}
            >
              <FilePlus size={14} />
            </Button>
            <Button
              variant="icon"
              disabled={mutating || panel !== "resources"}
              onClick={() => void importFiles()}
              aria-label={t("importFiles")}
              title={t("importFiles")}
            >
              <FolderInput size={14} />
            </Button>
            <Button
              variant="icon"
              disabled={query.isFetching}
              onClick={() => void query.refetch()}
              aria-label={t("refresh")}
              title={t("refresh")}
            >
              <RotateCcw size={14} />
            </Button>
          </div>
          {mutationError && <p className="m-0 p-2 text-xs text-[var(--danger)]">{mutationError}</p>}
          <ProjectTree
            projectId={projectId}
            nodes={query.data.nodes}
            mode={panel}
            selectedNodeId={selectedNodeId}
            onSelectionChange={setSelectedNodeId}
            onOpenNode={onOpenNode}
            onMutate={mutate}
            onRequestMutation={setPendingMutation}
          />
        </ScrollArea>
      )}
      <WorkspaceMutationDialog
        pending={pendingMutation}
        busy={mutating}
        onOpenChange={(open) => !open && setPendingMutation(null)}
        onNameChange={(name) =>
          setPendingMutation((current) => (current && "name" in current ? { ...current, name } : current))
        }
        onConfirm={async () => {
          if (!pendingMutation) return;
          if (pendingMutation.kind === "createFile") {
            if (!pendingMutation.name.trim()) return;
            setMutating(true);
            try {
              const receipt = await bridge.createWorkspaceFile({
                projectId,
                parentId: pendingMutation.parentId,
                name: pendingMutation.name.trim(),
              });
              await queryClient.invalidateQueries({ queryKey: ["project", projectId, "tree"] });
              const nodeId = receipt.affectedNodeIds[0];
              if (nodeId) setSelectedNodeId(nodeId);
              setPendingMutation(null);
            } catch (error) {
              setMutationError(error instanceof Error ? error.message : String(error));
            } finally {
              setMutating(false);
            }
            return;
          }
          const request =
            pendingMutation.kind === "createFolder"
              ? pendingMutation.name.trim()
                ? {
                    kind: "createFolder" as const,
                    projectId,
                    parentId: pendingMutation.parentId,
                    name: pendingMutation.name.trim(),
                  }
                : null
              : pendingMutation.kind === "rename"
                ? pendingMutation.name.trim() && pendingMutation.name.trim() !== pendingMutation.node.name
                  ? {
                      kind: "rename" as const,
                      projectId,
                      nodeId: pendingMutation.node.id,
                      name: pendingMutation.name.trim(),
                    }
                  : null
                : { kind: "trash" as const, projectId, nodeId: pendingMutation.node.id };
          if (request && (await mutate(request))) setPendingMutation(null);
        }}
      />
    </aside>
  );
}

function ProjectTree({
  projectId,
  nodes,
  mode,
  selectedNodeId,
  onSelectionChange,
  onOpenNode,
  onMutate,
  onRequestMutation,
}: {
  projectId: string;
  nodes: ProjectTreeNode[];
  mode: Exclude<ExplorerPanel, "search">;
  selectedNodeId: string | null;
  onSelectionChange: (nodeId: string | null) => void;
  onOpenNode: (node: ProjectTreeNode) => void;
  onMutate: (
    request: Parameters<ReturnType<typeof useDesktopBridge>["mutateWorkspace"]>[0],
  ) => Promise<boolean>;
  onRequestMutation: (pending: PendingWorkspaceMutation) => void;
}) {
  const { t } = useTranslation("explorer");
  const visibleNodes = mode === "outline" ? nodes.filter((node) => node.section === "source") : nodes;
  const sections = (["source", "workspace", "derived"] as const)
    .filter((section) => mode !== "outline" || section === "source")
    .map((section) => ({
      section,
      nodes: visibleNodes.filter((node) => node.section === section),
    }));
  return (
    <Tree
      aria-label={t(mode)}
      selectionMode="single"
      selectedKeys={selectedNodeId ? [selectedNodeId] : []}
      onSelectionChange={(keys) => {
        if (keys === "all") return onSelectionChange(null);
        const [nodeId] = Array.from(keys);
        onSelectionChange(typeof nodeId === "string" ? nodeId : null);
      }}
      className="p-1 text-sm"
    >
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
                    <ProjectNode
                      key={node.id}
                      node={node}
                      nodes={sectionNodes}
                      projectId={projectId}
                      onOpenNode={onOpenNode}
                      onMutate={onMutate}
                      onRequestMutation={onRequestMutation}
                    />
                  ))}
              </TreeItem>
            ))}
        </TreeItem>
      ))}
    </Tree>
  );
}

function ProjectNode({
  node,
  nodes,
  projectId,
  onOpenNode,
  onMutate,
  onRequestMutation,
}: {
  node: ProjectTreeNode;
  nodes: ProjectTreeNode[];
  projectId: string;
  onOpenNode: (node: ProjectTreeNode) => void;
  onMutate: (
    request: Parameters<ReturnType<typeof useDesktopBridge>["mutateWorkspace"]>[0],
  ) => Promise<boolean>;
  onRequestMutation: (pending: PendingWorkspaceMutation) => void;
}) {
  const { t } = useTranslation("explorer");
  const children = nodes.filter((candidate) => candidate.parentId === node.id);
  const recycled = node.id.startsWith("recycle/");
  return (
    <TreeItem id={node.id} textValue={node.name} onAction={() => node.capabilities.open && onOpenNode(node)}>
      <TreeItemContent>
        {({ level, isSelected }) => (
          <div
            className={cn(
              "group flex h-7 items-center gap-1.5 rounded-[4px] pr-2 text-[var(--text-secondary)]",
              isSelected && "bg-[var(--selection)] text-[var(--selection-text)]",
            )}
            style={{ paddingLeft: `${Math.max(level - 1, 0) * 14 + 8}px` }}
          >
            {node.kind === "folder" ? (
              <Folder size={14} />
            ) : node.kind === "image" ? (
              <Image size={14} />
            ) : (
              <FileText size={14} />
            )}
            <span className="min-w-0 flex-1 truncate">{node.name}</span>
            {recycled && (
              <button
                type="button"
                className="grid size-5 place-items-center opacity-0 group-hover:opacity-100"
                aria-label={t("restore")}
                onClick={(event) => {
                  event.stopPropagation();
                  void onMutate({ kind: "restore", projectId, nodeId: node.id });
                }}
              >
                <RotateCcw size={12} />
              </button>
            )}
            {node.capabilities.rename && (
              <button
                type="button"
                className="grid size-5 place-items-center opacity-0 group-hover:opacity-100"
                aria-label={t("rename")}
                onClick={(event) => {
                  event.stopPropagation();
                  onRequestMutation({ kind: "rename", node, name: node.name });
                }}
              >
                <Pencil size={12} />
              </button>
            )}
            {node.capabilities.delete && (
              <button
                type="button"
                className="grid size-5 place-items-center opacity-0 group-hover:opacity-100"
                aria-label={t("moveToTrash")}
                onClick={(event) => {
                  event.stopPropagation();
                  onRequestMutation({ kind: "trash", node });
                }}
              >
                <Trash2 size={12} />
              </button>
            )}
          </div>
        )}
      </TreeItemContent>
      {children.map((child) => (
        <ProjectNode
          key={child.id}
          node={child}
          nodes={nodes}
          projectId={projectId}
          onOpenNode={onOpenNode}
          onMutate={onMutate}
          onRequestMutation={onRequestMutation}
        />
      ))}
    </TreeItem>
  );
}

function WorkspaceMutationDialog({
  pending,
  busy,
  onOpenChange,
  onNameChange,
  onConfirm,
}: {
  pending: PendingWorkspaceMutation;
  busy: boolean;
  onOpenChange: (open: boolean) => void;
  onNameChange: (name: string) => void;
  onConfirm: () => Promise<void>;
}) {
  const { t } = useTranslation(["explorer", "common"]);
  const isTextInput =
    pending?.kind === "createFolder" || pending?.kind === "createFile" || pending?.kind === "rename";
  const title =
    pending?.kind === "createFolder"
      ? t("newFolder")
      : pending?.kind === "createFile"
        ? t("newFile")
        : pending?.kind === "rename"
          ? t("rename")
          : t("moveToTrash");
  return (
    <Dialog open={pending !== null} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-[420px]">
        <DialogTitle>{title}</DialogTitle>
        <DialogDescription>
          {pending?.kind === "trash" ? t("confirmMoveToTrash", { name: pending.node.name }) : title}
        </DialogDescription>
        {isTextInput && pending && "name" in pending && (
          <Input
            autoFocus
            value={pending.name}
            disabled={busy}
            aria-label={title}
            onChange={(event) => onNameChange(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter") void onConfirm();
            }}
          />
        )}
        <div className="mt-5 flex justify-end gap-2">
          <Button disabled={busy} onClick={() => onOpenChange(false)}>
            {t("cancel", { ns: "common" })}
          </Button>
          <Button
            variant={pending?.kind === "trash" ? "danger" : "primary"}
            disabled={busy}
            onClick={() => void onConfirm()}
          >
            {t("confirm", { ns: "common" })}
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}
