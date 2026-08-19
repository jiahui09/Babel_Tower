import type { DesktopBridge } from "../platform/desktop-bridge";

export type CommandIconName =
  | "file-import"
  | "file-output"
  | "undo"
  | "redo"
  | "panel-left"
  | "panel-right"
  | "focus"
  | "settings"
  | "search"
  | "shield";

export type CommandCategory = "file" | "edit" | "view" | "help";
export type CommandLabelKey =
  | "menu:import"
  | "menu:export"
  | "menu:undo"
  | "menu:redo"
  | "menu:commandPalette"
  | "menu:toggleExplorer"
  | "menu:toggleInspector"
  | "menu:focusMode"
  | "workbench:issues"
  | "common:settings";

export interface CommandContext {
  projectId: string | null;
  activeUnitId: string | null;
  bridge: DesktopBridge;
  actions: {
    importWork(): Promise<void>;
    openExports(): Promise<void>;
    openValidation(): Promise<void>;
    openSettings(): void;
    openCommandPalette(): void;
    toggleExplorer(): void;
    toggleInspector(): void;
    toggleFocusMode(): void;
  };
}

export interface CommandAvailability {
  enabled: boolean;
  reasonKey?: string;
}

export interface CommandDescriptor {
  id: string;
  labelKey: CommandLabelKey;
  categoryKey: `menu:${CommandCategory}`;
  shortcut?: string[];
  icon: CommandIconName;
  isVisible(context: CommandContext): boolean;
  getAvailability(context: CommandContext): CommandAvailability;
  run(context: CommandContext): Promise<void>;
}

const available = () => ({ enabled: true });
const projectRequired = (context: CommandContext): CommandAvailability => ({
  enabled: context.projectId !== null,
  reasonKey: context.projectId === null ? "errors:notFound" : undefined,
});
const editRequired = (context: CommandContext): CommandAvailability => ({
  enabled: context.projectId !== null && context.activeUnitId !== null,
  reasonKey: context.activeUnitId === null ? "errors:notFound" : undefined,
});

function command(
  descriptor: Omit<CommandDescriptor, "isVisible" | "getAvailability"> & {
    isVisible?: CommandDescriptor["isVisible"];
    getAvailability?: CommandDescriptor["getAvailability"];
  },
): CommandDescriptor {
  return {
    isVisible: () => true,
    getAvailability: available,
    ...descriptor,
  };
}

export const commandRegistry = [
  command({
    id: "file.import",
    labelKey: "menu:import",
    categoryKey: "menu:file",
    shortcut: ["Mod", "O"],
    icon: "file-import",
    run: (context) => context.actions.importWork(),
  }),
  command({
    id: "file.export",
    labelKey: "menu:export",
    categoryKey: "menu:file",
    shortcut: ["Mod", "Shift", "E"],
    icon: "file-output",
    getAvailability: projectRequired,
    run: (context) => context.actions.openExports(),
  }),
  command({
    id: "edit.undo",
    labelKey: "menu:undo",
    categoryKey: "menu:edit",
    shortcut: ["Mod", "Z"],
    icon: "undo",
    getAvailability: editRequired,
    run: async (context) => {
      if (!context.projectId) return;
      await context.bridge.undo({
        projectId: context.projectId,
        unitId: context.activeUnitId as string,
        commandId: crypto.randomUUID().replace(/-/g, ""),
        createdAtMs: Date.now(),
      });
    },
  }),
  command({
    id: "edit.redo",
    labelKey: "menu:redo",
    categoryKey: "menu:edit",
    shortcut: ["Mod", "Shift", "Z"],
    icon: "redo",
    getAvailability: editRequired,
    run: async (context) => {
      if (!context.projectId) return;
      await context.bridge.redo({
        projectId: context.projectId,
        unitId: context.activeUnitId as string,
        commandId: crypto.randomUUID().replace(/-/g, ""),
        createdAtMs: Date.now(),
      });
    },
  }),
  command({
    id: "view.commandPalette",
    labelKey: "menu:commandPalette",
    categoryKey: "menu:view",
    shortcut: ["Mod", "K"],
    icon: "search",
    run: async (context) => context.actions.openCommandPalette(),
  }),
  command({
    id: "view.explorer",
    labelKey: "menu:toggleExplorer",
    categoryKey: "menu:view",
    shortcut: ["Mod", "Shift", "B"],
    icon: "panel-left",
    run: async (context) => context.actions.toggleExplorer(),
  }),
  command({
    id: "view.inspector",
    labelKey: "menu:toggleInspector",
    categoryKey: "menu:view",
    shortcut: ["Mod", "Shift", "I"],
    icon: "panel-right",
    run: async (context) => context.actions.toggleInspector(),
  }),
  command({
    id: "view.focusMode",
    labelKey: "menu:focusMode",
    categoryKey: "menu:view",
    shortcut: ["Mod", "Shift", "F"],
    icon: "focus",
    run: async (context) => context.actions.toggleFocusMode(),
  }),
  command({
    id: "view.validation",
    labelKey: "workbench:issues",
    categoryKey: "menu:view",
    icon: "shield",
    getAvailability: projectRequired,
    run: (context) => context.actions.openValidation(),
  }),
  command({
    id: "view.settings",
    labelKey: "common:settings",
    categoryKey: "menu:view",
    shortcut: ["Mod", ","],
    icon: "settings",
    run: async (context) => context.actions.openSettings(),
  }),
] satisfies CommandDescriptor[];

export function commandsForCategory(category: CommandCategory, context: CommandContext) {
  return commandRegistry.filter(
    (descriptor) => descriptor.categoryKey === `menu:${category}` && descriptor.isVisible(context),
  );
}

export function shortcutLabel(shortcut?: string[]) {
  if (!shortcut) return "";
  const modifier = navigator.platform.toLowerCase().includes("mac") ? "⌘" : "Ctrl";
  return shortcut.map((part) => (part === "Mod" ? modifier : part)).join("+");
}

export function findShortcutConflicts(commands: CommandDescriptor[]) {
  const seen = new Map<string, string>();
  const conflicts: Array<{ shortcut: string; commandIds: [string, string] }> = [];
  for (const descriptor of commands) {
    if (!descriptor.shortcut) continue;
    const shortcut = descriptor.shortcut.map((part) => part.toLowerCase()).join("+");
    const existing = seen.get(shortcut);
    if (existing) conflicts.push({ shortcut, commandIds: [existing, descriptor.id] });
    else seen.set(shortcut, descriptor.id);
  }
  return conflicts;
}
