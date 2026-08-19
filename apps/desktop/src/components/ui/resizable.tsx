import { Group, Panel, Separator } from "react-resizable-panels";

export const ResizablePanelGroup = Group;
export const ResizablePanel = Panel;
export function ResizableHandle() {
  return <Separator className="w-1 bg-[var(--border)] transition-colors hover:bg-[var(--accent)]" />;
}

