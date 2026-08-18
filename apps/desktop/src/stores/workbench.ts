import { create } from "zustand";

export type SaveState = "editing" | "saving" | "saved" | "error";

interface WorkbenchState {
  saveState: SaveState;
  contextCollapsed: boolean;
  setSaveState: (state: SaveState) => void;
  toggleContext: () => void;
}

export const useWorkbenchStore = create<WorkbenchState>((set) => ({
  saveState: "saved",
  contextCollapsed: false,
  setSaveState: (saveState) => set({ saveState }),
  toggleContext: () => set((state) => ({ contextCollapsed: !state.contextCollapsed })),
}));
