import { create } from "zustand";
import { persist } from "zustand/middleware";

export type SaveState = "editing" | "saving" | "saved" | "error" | "conflict";
export type ExplorerPanel = "resources" | "outline" | "search";
export type InspectorPanel = "terms" | "annotations" | "properties" | "issues";
export type WorkbenchTabKind = "chapter" | "unitCollection" | "image" | "source" | "diff" | "workspaceFile";

export interface WorkbenchTab {
  id: string;
  projectId: string;
  kind: WorkbenchTabKind;
  title: string;
  unitId?: string;
  resourceId?: string;
  uri?: string;
  nodeId?: string;
  isReadonly?: boolean;
  pinned?: boolean;
  dirty: boolean;
}

export interface EditorGroup {
  id: "primary" | "secondary";
  tabIds: string[];
  activeTabId: string | null;
}

interface WorkbenchState {
  saveState: SaveState;
  explorerOpen: boolean;
  inspectorOpen: boolean;
  problemsOpen: boolean;
  focusMode: boolean;
  focusModeRestore: { explorerOpen: boolean; inspectorOpen: boolean } | null;
  commandPaletteOpen: boolean;
  settingsOpen: boolean;
  commandError: string | null;
  explorerPanel: ExplorerPanel;
  inspectorPanel: InspectorPanel;
  selectedExplorerNodeId: string | null;
  explorerWidth: number;
  inspectorWidth: number;
  splitRatio: number;
  focusedGroupId: EditorGroup["id"];
  groups: EditorGroup[];
  tabs: WorkbenchTab[];
  flushers: Record<string, () => Promise<boolean>>;
  setSaveState: (state: SaveState) => void;
  setExplorerOpen: (open: boolean) => void;
  setInspectorOpen: (open: boolean) => void;
  setProblemsOpen: (open: boolean) => void;
  toggleProblems: () => void;
  toggleExplorer: () => void;
  toggleInspector: () => void;
  toggleFocusMode: () => void;
  setCommandPaletteOpen: (open: boolean) => void;
  setSettingsOpen: (open: boolean) => void;
  setCommandError: (message: string | null) => void;
  setExplorerPanel: (panel: ExplorerPanel) => void;
  setInspectorPanel: (panel: InspectorPanel) => void;
  setSelectedExplorerNodeId: (nodeId: string | null) => void;
  setPanelWidths: (explorer: number, inspector: number) => void;
  setSplitRatio: (ratio: number) => void;
  setFocusedGroup: (groupId: EditorGroup["id"]) => void;
  openTab: (tab: WorkbenchTab, groupId?: EditorGroup["id"]) => void;
  activateTab: (tabId: string, groupId?: EditorGroup["id"]) => void;
  markTabDirty: (tabId: string, dirty: boolean) => void;
  closeTab: (tabId: string, groupId?: EditorGroup["id"]) => void;
  closeOtherTabs: (tabId: string, groupId?: EditorGroup["id"]) => void;
  closeTabsToRight: (tabId: string, groupId?: EditorGroup["id"]) => void;
  moveTab: (tabId: string, targetIndex: number, groupId?: EditorGroup["id"]) => void;
  replaceProjectSession: (projectId: string, tabs: WorkbenchTab[], groups: EditorGroup[]) => void;
  reconcileWorkspaceFiles: (
    projectId: string,
    resources: Array<{ nodeId: string; uri?: string; title: string }>,
  ) => void;
  splitTab: (tabId: string) => void;
  registerTabFlusher: (tabId: string, flusher: () => Promise<boolean>) => () => void;
}

const initialGroups: EditorGroup[] = [
  { id: "primary", tabIds: [], activeTabId: null },
  { id: "secondary", tabIds: [], activeTabId: null },
];

export const useWorkbenchStore = create<WorkbenchState>()(
  persist(
    (set) => ({
      saveState: "saved",
      explorerOpen: true,
      inspectorOpen: true,
      problemsOpen: false,
      focusMode: false,
      focusModeRestore: null,
      commandPaletteOpen: false,
      settingsOpen: false,
      commandError: null,
      explorerPanel: "resources",
      inspectorPanel: "properties",
      selectedExplorerNodeId: null,
      explorerWidth: 260,
      inspectorWidth: 320,
      splitRatio: 0.5,
      focusedGroupId: "primary",
      groups: initialGroups,
      tabs: [],
      flushers: {},
      setSaveState: (saveState) => set({ saveState }),
      setExplorerOpen: (explorerOpen) => set({ explorerOpen }),
      setInspectorOpen: (inspectorOpen) => set({ inspectorOpen }),
      setProblemsOpen: (problemsOpen) => set({ problemsOpen }),
      toggleProblems: () => set((state) => ({ problemsOpen: !state.problemsOpen })),
      toggleExplorer: () => set((state) => ({ explorerOpen: !state.explorerOpen })),
      toggleInspector: () => set((state) => ({ inspectorOpen: !state.inspectorOpen })),
      toggleFocusMode: () =>
        set((state) => ({
          focusMode: !state.focusMode,
          focusModeRestore: state.focusMode
            ? null
            : { explorerOpen: state.explorerOpen, inspectorOpen: state.inspectorOpen },
          explorerOpen: state.focusMode ? (state.focusModeRestore?.explorerOpen ?? true) : false,
          inspectorOpen: state.focusMode ? (state.focusModeRestore?.inspectorOpen ?? true) : false,
        })),
      setCommandPaletteOpen: (commandPaletteOpen) => set({ commandPaletteOpen }),
      setSettingsOpen: (settingsOpen) => set({ settingsOpen }),
      setCommandError: (commandError) => set({ commandError }),
      setExplorerPanel: (explorerPanel) => set({ explorerPanel }),
      setInspectorPanel: (inspectorPanel) => set({ inspectorPanel, inspectorOpen: true }),
      setSelectedExplorerNodeId: (selectedExplorerNodeId) => set({ selectedExplorerNodeId }),
      setPanelWidths: (explorerWidth, inspectorWidth) => set({ explorerWidth, inspectorWidth }),
      setSplitRatio: (splitRatio) => set({ splitRatio }),
      setFocusedGroup: (focusedGroupId) => set({ focusedGroupId }),
      openTab: (tab, groupId) =>
        set((state) => {
          const targetGroup = groupId ?? state.focusedGroupId;
          const tabs = state.tabs.some((item) => item.id === tab.id) ? state.tabs : [...state.tabs, tab];
          const groups = state.groups.map((group) =>
            group.id === targetGroup
              ? {
                  ...group,
                  tabIds: group.tabIds.includes(tab.id) ? group.tabIds : [...group.tabIds, tab.id],
                  activeTabId: tab.id,
                }
              : group,
          );
          return { tabs, groups, focusedGroupId: targetGroup };
        }),
      activateTab: (tabId, groupId) =>
        set((state) => {
          const targetGroup = groupId ?? state.focusedGroupId;
          return {
            focusedGroupId: targetGroup,
            groups: state.groups.map((group) =>
              group.id === targetGroup && group.tabIds.includes(tabId)
                ? { ...group, activeTabId: tabId }
                : group,
            ),
          };
        }),
      markTabDirty: (tabId, dirty) =>
        set((state) => ({ tabs: state.tabs.map((tab) => (tab.id === tabId ? { ...tab, dirty } : tab)) })),
      closeTab: (tabId, groupId) =>
        set((state) => {
          const targetGroup = groupId ?? state.focusedGroupId;
          const groups = state.groups.map((group) => {
            if (group.id !== targetGroup) return group;
            const tabIds = group.tabIds.filter((id) => id !== tabId);
            return {
              ...group,
              tabIds,
              activeTabId:
                group.activeTabId === tabId ? (tabIds[tabIds.length - 1] ?? null) : group.activeTabId,
            };
          });
          const stillOpen = groups.some((group) => group.tabIds.includes(tabId));
          return { groups, tabs: stillOpen ? state.tabs : state.tabs.filter((tab) => tab.id !== tabId) };
        }),
      closeOtherTabs: (tabId, groupId) =>
        set((state) => {
          const targetGroup = groupId ?? state.focusedGroupId;
          const groups = state.groups.map((group) =>
            group.id === targetGroup
              ? {
                  ...group,
                  tabIds: group.tabIds.filter(
                    (id) => id === tabId || state.tabs.find((tab) => tab.id === id)?.pinned,
                  ),
                  activeTabId: tabId,
                }
              : group,
          );
          const openIds = new Set(groups.flatMap((group) => group.tabIds));
          return { groups, tabs: state.tabs.filter((tab) => openIds.has(tab.id)) };
        }),
      closeTabsToRight: (tabId, groupId) =>
        set((state) => {
          const targetGroup = groupId ?? state.focusedGroupId;
          const groups = state.groups.map((group) => {
            if (group.id !== targetGroup) return group;
            const index = group.tabIds.indexOf(tabId);
            return index < 0
              ? group
              : {
                  ...group,
                  tabIds: group.tabIds.filter(
                    (id, itemIndex) => itemIndex <= index || state.tabs.find((tab) => tab.id === id)?.pinned,
                  ),
                };
          });
          const openIds = new Set(groups.flatMap((group) => group.tabIds));
          return { groups, tabs: state.tabs.filter((tab) => openIds.has(tab.id)) };
        }),
      moveTab: (tabId, targetIndex, groupId) =>
        set((state) => {
          const targetGroup = groupId ?? state.focusedGroupId;
          return {
            groups: state.groups.map((group) => {
              if (group.id !== targetGroup) return group;
              const tabIds = group.tabIds.filter((id) => id !== tabId);
              tabIds.splice(Math.max(0, Math.min(targetIndex, tabIds.length)), 0, tabId);
              return { ...group, tabIds };
            }),
          };
        }),
      replaceProjectSession: (projectId, tabs, groups) =>
        set({
          tabs: tabs.filter((tab) => tab.projectId === projectId),
          groups,
          focusedGroupId: "primary",
          flushers: {},
        }),
      reconcileWorkspaceFiles: (projectId, resources) =>
        set((state) => {
          const byNode = new Map(resources.map((resource) => [resource.nodeId, resource]));
          const removed = new Set(
            state.tabs
              .filter(
                (tab) =>
                  tab.projectId === projectId &&
                  tab.kind === "workspaceFile" &&
                  (!tab.nodeId || !byNode.has(tab.nodeId)),
              )
              .map((tab) => tab.id),
          );
          const tabs = state.tabs
            .filter((tab) => !removed.has(tab.id))
            .map((tab) => {
              const resource = tab.nodeId ? byNode.get(tab.nodeId) : undefined;
              return resource ? { ...tab, title: resource.title, uri: resource.uri } : tab;
            });
          const groups = state.groups.map((group) => {
            const tabIds = group.tabIds.filter((id) => !removed.has(id));
            return {
              ...group,
              tabIds,
              activeTabId:
                group.activeTabId && tabIds.includes(group.activeTabId)
                  ? group.activeTabId
                  : (tabIds[tabIds.length - 1] ?? null),
            };
          });
          return { tabs, groups };
        }),
      splitTab: (tabId) =>
        set((state) => ({
          groups: state.groups.map((group) =>
            group.id === "secondary"
              ? {
                  ...group,
                  tabIds: group.tabIds.includes(tabId) ? group.tabIds : [...group.tabIds, tabId],
                  activeTabId: tabId,
                }
              : group,
          ),
          focusedGroupId: "secondary",
        })),
      registerTabFlusher: (tabId, flusher) => {
        const previous = useWorkbenchStore.getState().flushers[tabId];
        const composed = previous ? async () => (await previous()) && (await flusher()) : flusher;
        set((state) => ({ flushers: { ...state.flushers, [tabId]: composed } }));
        return () =>
          set((state) => {
            if (state.flushers[tabId] !== composed) return state;
            const next = { ...state.flushers };
            if (previous) next[tabId] = previous;
            else delete next[tabId];
            return { flushers: next };
          });
      },
    }),
    {
      name: "babel-tower-workbench-v2",
      partialize: (state) => ({
        explorerOpen: state.explorerOpen,
        inspectorOpen: state.inspectorOpen,
        problemsOpen: state.problemsOpen,
        focusMode: state.focusMode,
        focusModeRestore: state.focusModeRestore,
        explorerPanel: state.explorerPanel,
        inspectorPanel: state.inspectorPanel,
        selectedExplorerNodeId: state.selectedExplorerNodeId,
        explorerWidth: state.explorerWidth,
        inspectorWidth: state.inspectorWidth,
        splitRatio: state.splitRatio,
        focusedGroupId: state.focusedGroupId,
        groups: state.groups,
        tabs: state.tabs,
      }),
    },
  ),
);
