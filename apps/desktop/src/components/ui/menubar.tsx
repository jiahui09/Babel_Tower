import * as MenubarPrimitive from "@radix-ui/react-menubar";
import * as React from "react";

import { cn } from "../../lib/utils";

export const Menubar = React.forwardRef<
  React.ElementRef<typeof MenubarPrimitive.Root>,
  React.ComponentPropsWithoutRef<typeof MenubarPrimitive.Root>
>(({ className, ...props }, ref) => (
  <MenubarPrimitive.Root ref={ref} className={cn("flex h-7 items-center", className)} {...props} />
));
Menubar.displayName = "Menubar";
export const MenubarMenu = MenubarPrimitive.Menu;
export const MenubarGroup = MenubarPrimitive.Group;
export const MenubarSeparator = () => <MenubarPrimitive.Separator className="my-1 h-px bg-[var(--border)]" />;

export const MenubarTrigger = React.forwardRef<
  React.ElementRef<typeof MenubarPrimitive.Trigger>,
  React.ComponentPropsWithoutRef<typeof MenubarPrimitive.Trigger>
>(({ className, ...props }, ref) => (
  <MenubarPrimitive.Trigger
    ref={ref}
    className={cn(
      "flex h-6 select-none items-center rounded-[4px] px-2 text-xs text-[var(--text-secondary)] hover:bg-[var(--surface-inset)] data-[state=open]:bg-[var(--surface-inset)] data-[state=open]:text-[var(--text)]",
      className,
    )}
    {...props}
  />
));
MenubarTrigger.displayName = "MenubarTrigger";

export const MenubarContent = React.forwardRef<
  React.ElementRef<typeof MenubarPrimitive.Content>,
  React.ComponentPropsWithoutRef<typeof MenubarPrimitive.Content>
>(({ className, ...props }, ref) => (
  <MenubarPrimitive.Portal>
    <MenubarPrimitive.Content
      ref={ref}
      align="start"
      sideOffset={2}
      className={cn(
        "z-50 min-w-[220px] rounded-[var(--radius)] border border-[var(--border)] bg-[var(--surface-raised)] p-1 shadow-lg",
        className,
      )}
      {...props}
    />
  </MenubarPrimitive.Portal>
));
MenubarContent.displayName = "MenubarContent";

export const MenubarItem = React.forwardRef<
  React.ElementRef<typeof MenubarPrimitive.Item>,
  React.ComponentPropsWithoutRef<typeof MenubarPrimitive.Item>
>(({ className, children, ...props }, ref) => (
  <MenubarPrimitive.Item
    ref={ref}
    className={cn(
      "flex h-8 select-none items-center gap-2 rounded-[4px] px-2 text-sm text-[var(--text)] outline-none data-[disabled]:opacity-45 data-[highlighted]:bg-[var(--selection)] data-[highlighted]:text-[var(--selection-text)]",
      className,
    )}
    {...props}
  >
    {children}
  </MenubarPrimitive.Item>
));
MenubarItem.displayName = "MenubarItem";

export function MenubarShortcut({ children }: { children: React.ReactNode }) {
  return <span className="ml-auto pl-5 text-xs text-[var(--text-muted)]">{children}</span>;
}

