import { Command as CommandPrimitive } from "cmdk";
import { Search } from "lucide-react";
import * as React from "react";

import { cn } from "../../lib/utils";

export const Command = React.forwardRef<
  React.ElementRef<typeof CommandPrimitive>,
  React.ComponentPropsWithoutRef<typeof CommandPrimitive>
>(({ className, ...props }, ref) => (
  <CommandPrimitive ref={ref} className={cn("flex h-full flex-col", className)} {...props} />
));
Command.displayName = "Command";

export function CommandInput(props: React.ComponentPropsWithoutRef<typeof CommandPrimitive.Input>) {
  return (
    <div className="flex h-11 items-center gap-2 border-b border-[var(--border)] px-3">
      <Search size={16} className="text-[var(--text-muted)]" />
      <CommandPrimitive.Input
        className="h-full min-w-0 flex-1 bg-transparent text-sm text-[var(--text)] placeholder:text-[var(--text-muted)]"
        {...props}
      />
    </div>
  );
}

export const CommandList = React.forwardRef<
  React.ElementRef<typeof CommandPrimitive.List>,
  React.ComponentPropsWithoutRef<typeof CommandPrimitive.List>
>(({ className, ...props }, ref) => (
  <CommandPrimitive.List ref={ref} className={cn("max-h-[360px] overflow-y-auto p-1", className)} {...props} />
));
CommandList.displayName = "CommandList";

export const CommandEmpty = CommandPrimitive.Empty;
export const CommandGroup = CommandPrimitive.Group;

export const CommandItem = React.forwardRef<
  React.ElementRef<typeof CommandPrimitive.Item>,
  React.ComponentPropsWithoutRef<typeof CommandPrimitive.Item>
>(({ className, ...props }, ref) => (
  <CommandPrimitive.Item
    ref={ref}
    className={cn(
      "flex h-9 cursor-default select-none items-center gap-2 rounded-[4px] px-2 text-sm data-[disabled=true]:opacity-45 data-[selected=true]:bg-[var(--selection)] data-[selected=true]:text-[var(--selection-text)]",
      className,
    )}
    {...props}
  />
));
CommandItem.displayName = "CommandItem";

