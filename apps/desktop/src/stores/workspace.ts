import { create } from "zustand";

import type { ProjectTreeNode, WorkspaceStateV1 } from "../platform/desktop-bridge";

interface WorkspaceStore {
  projectId: string | null;
  nodes: ProjectTreeNode[];
  expandedNodeIds: string[];
  selectedNodeId: string | null;
  loading: boolean;
  error: string | null;
  beginLoad: (projectId: string) => void;
  loadTree: (projectId: string, nodes: ProjectTreeNode[]) => void;
  failLoad: (message: string) => void;
  toggleExpanded: (nodeId: string) => void;
  setSelected: (nodeId: string | null) => void;
  reveal: (nodeId: string) => void;
  restoreState: (projectId: string, state: WorkspaceStateV1 | null) => void;
  reset: (projectId?: string) => void;
}

const ancestorsOf = (nodes: ProjectTreeNode[], nodeId: string) => {
  const byId = new Map(nodes.map((node) => [node.id, node]));
  const ancestors: string[] = [];
  let current = byId.get(nodeId);
  while (current?.parentId) {
    ancestors.unshift(current.parentId);
    current = byId.get(current.parentId);
  }
  return ancestors;
};

export const useWorkspaceStore = create<WorkspaceStore>((set) => ({
  projectId: null,
  nodes: [],
  expandedNodeIds: [],
  selectedNodeId: null,
  loading: false,
  error: null,
  beginLoad: (projectId) => set({ projectId, loading: true, error: null }),
  loadTree: (projectId, nodes) =>
    set((state) => {
      const ids = new Set(nodes.map((node) => node.id));
      return {
        projectId,
        nodes,
        loading: false,
        error: null,
        expandedNodeIds: state.expandedNodeIds.filter((id) => ids.has(id)),
        selectedNodeId: state.selectedNodeId && ids.has(state.selectedNodeId) ? state.selectedNodeId : null,
      };
    }),
  failLoad: (error) => set({ loading: false, error }),
  toggleExpanded: (nodeId) =>
    set((state) => ({
      expandedNodeIds: state.expandedNodeIds.includes(nodeId)
        ? state.expandedNodeIds.filter((id) => id !== nodeId)
        : [...state.expandedNodeIds, nodeId],
    })),
  setSelected: (selectedNodeId) => set({ selectedNodeId }),
  reveal: (selectedNodeId) =>
    set((state) => ({
      selectedNodeId,
      expandedNodeIds: Array.from(new Set([...state.expandedNodeIds, ...ancestorsOf(state.nodes, selectedNodeId)])),
    })),
  restoreState: (projectId, workspaceState) =>
    set({
      projectId,
      expandedNodeIds: workspaceState?.expandedNodeIds ?? [],
      selectedNodeId: workspaceState?.selectedNodeId ?? null,
    }),
  reset: (projectId = undefined) =>
    set({ projectId: projectId ?? null, nodes: [], expandedNodeIds: [], selectedNodeId: null, loading: false, error: null }),
}));

