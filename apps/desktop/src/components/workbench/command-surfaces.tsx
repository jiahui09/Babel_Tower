import {
  FileInput,
  FileOutput,
  Focus,
  PanelLeft,
  PanelRight,
  Redo2,
  Search,
  Settings,
  ShieldCheck,
  Undo2,
  type LucideIcon,
} from "lucide-react";
import { useEffect } from "react";
import { useTranslation } from "react-i18next";

import {
  commandRegistry,
  commandsForCategory,
  shortcutLabel,
  type CommandContext,
  type CommandDescriptor,
  type CommandIconName,
} from "../../commands/registry";
import { useWorkbenchStore } from "../../stores/workbench";
import { Command, CommandEmpty, CommandGroup, CommandInput, CommandItem, CommandList } from "../ui/command";
import { Dialog, DialogContent, DialogTitle } from "../ui/dialog";
import {
  Menubar,
  MenubarContent,
  MenubarItem,
  MenubarMenu,
  MenubarShortcut,
  MenubarTrigger,
} from "../ui/menubar";

const icons: Record<CommandIconName, LucideIcon> = {
  "file-import": FileInput,
  "file-output": FileOutput,
  undo: Undo2,
  redo: Redo2,
  "panel-left": PanelLeft,
  "panel-right": PanelRight,
  focus: Focus,
  settings: Settings,
  search: Search,
  shield: ShieldCheck,
};

export function ApplicationMenubar({ context }: { context: CommandContext }) {
  const { t } = useTranslation(["menu", "workbench", "common", "errors"]);
  return (
    <Menubar>
      {(["file", "edit", "view"] as const).map((category) => (
        <MenubarMenu key={category}>
          <MenubarTrigger>{t(category, { ns: "menu" })}</MenubarTrigger>
          <MenubarContent>
            {commandsForCategory(category, context).map((descriptor) => {
              const availability = descriptor.getAvailability(context);
              const Icon = icons[descriptor.icon];
              return (
                <MenubarItem
                  key={descriptor.id}
                  disabled={!availability.enabled}
                  title={availability.reasonKey ? t(availability.reasonKey as never) : undefined}
                  onSelect={() => void runCommand(descriptor, context)}
                >
                  <Icon size={15} />
                  {t(descriptor.labelKey as never)}
                  {descriptor.shortcut && (
                    <MenubarShortcut>{shortcutLabel(descriptor.shortcut)}</MenubarShortcut>
                  )}
                </MenubarItem>
              );
            })}
          </MenubarContent>
        </MenubarMenu>
      ))}
      <MenubarMenu>
        <MenubarTrigger>{t("help", { ns: "menu" })}</MenubarTrigger>
        <MenubarContent>
          <MenubarItem disabled title={t("notAvailable", { ns: "common" })}>
            {t("about", { ns: "menu" })}
            <span className="ml-auto text-xs text-[var(--text-muted)]">
              {t("notAvailable", { ns: "common" })}
            </span>
          </MenubarItem>
        </MenubarContent>
      </MenubarMenu>
    </Menubar>
  );
}

export function CommandPalette({ context }: { context: CommandContext }) {
  const { t } = useTranslation(["menu", "workbench", "common", "errors"]);
  const open = useWorkbenchStore((state) => state.commandPaletteOpen);
  const setOpen = useWorkbenchStore((state) => state.setCommandPaletteOpen);
  const visible = commandRegistry.filter((descriptor) => descriptor.isVisible(context));
  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogContent className="w-[min(620px,calc(100vw-32px))] p-0" aria-describedby={undefined}>
        <DialogTitle className="sr-only">{t("commandPalette", { ns: "menu" })}</DialogTitle>
        <Command>
          <CommandInput autoFocus placeholder={t("commandPalette", { ns: "menu" })} />
          <CommandList>
            <CommandEmpty>{t("notAvailable", { ns: "common" })}</CommandEmpty>
            <CommandGroup>
              {visible.map((descriptor) => (
                <PaletteItem key={descriptor.id} descriptor={descriptor} context={context} />
              ))}
            </CommandGroup>
          </CommandList>
        </Command>
      </DialogContent>
    </Dialog>
  );
}

function PaletteItem({ descriptor, context }: { descriptor: CommandDescriptor; context: CommandContext }) {
  const { t } = useTranslation(["menu", "workbench", "common", "errors"]);
  const setOpen = useWorkbenchStore((state) => state.setCommandPaletteOpen);
  const availability = descriptor.getAvailability(context);
  const Icon = icons[descriptor.icon];
  return (
    <CommandItem
      value={`${descriptor.id} ${t(descriptor.labelKey as never)}`}
      disabled={!availability.enabled}
      onSelect={() => {
        setOpen(false);
        void runCommand(descriptor, context);
      }}
    >
      <Icon size={15} />
      <span>{t(descriptor.labelKey as never)}</span>
      {descriptor.shortcut && (
        <span className="ml-auto text-xs text-[var(--text-muted)]">{shortcutLabel(descriptor.shortcut)}</span>
      )}
    </CommandItem>
  );
}

async function runCommand(descriptor: CommandDescriptor, context: CommandContext) {
  const setSaveState = useWorkbenchStore.getState().setSaveState;
  try {
    await descriptor.run(context);
  } catch {
    setSaveState("error");
  }
}

export function useCommandShortcuts(context: CommandContext) {
  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      if (isEditableTarget(event.target)) return;
      for (const descriptor of commandRegistry) {
        if (!descriptor.shortcut || !descriptor.isVisible(context)) continue;
        if (!matchesShortcut(event, descriptor.shortcut)) continue;
        const availability = descriptor.getAvailability(context);
        if (!availability.enabled) return;
        event.preventDefault();
        void runCommand(descriptor, context);
        return;
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [context]);
}

function isEditableTarget(target: EventTarget | null) {
  if (!(target instanceof HTMLElement)) return false;
  if (target.isContentEditable) return true;
  return target.closest("input, textarea, select, [contenteditable='true'], .tiptap, .cm-editor") !== null;
}

function matchesShortcut(event: KeyboardEvent, shortcut: string[]) {
  const parts = new Set(shortcut.map((part) => part.toLowerCase()));
  const key = event.key.toLowerCase();
  const expectedKey = shortcut[shortcut.length - 1]?.toLowerCase();
  return (
    key === expectedKey &&
    event.shiftKey === parts.has("shift") &&
    event.altKey === parts.has("alt") &&
    (event.metaKey || event.ctrlKey) === parts.has("mod")
  );
}
