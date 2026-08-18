import { Link } from "@tanstack/react-router";
import { FileText, Image, Rows3 } from "lucide-react";

import { cn } from "../../lib/utils";

const items = [
  { to: "/projects/$projectId/content", label: "长文", icon: FileText },
  { to: "/projects/$projectId/units", label: "单元", icon: Rows3 },
  { to: "/projects/$projectId/resources", label: "资源", icon: Image },
] as const;

export function WorkspaceSwitcher({ projectId }: { projectId: string }) {
  return (
    <nav
      className="flex h-8 items-center rounded-[6px] border border-[var(--border)] bg-[var(--surface-inset)] p-0.5"
      aria-label="观察方式"
    >
      {items.map(({ to, label, icon: Icon }) => (
        <Link
          key={to}
          to={to}
          params={{ projectId }}
          className={cn(
            "flex h-[26px] items-center gap-1.5 rounded-[4px] px-2.5 text-xs text-[var(--text-secondary)]",
          )}
          activeProps={{
            className:
              "flex h-[26px] items-center gap-1.5 rounded-[4px] bg-[var(--surface-raised)] px-2.5 text-xs font-medium text-[var(--text)] shadow-sm",
          }}
        >
          <Icon size={14} aria-hidden="true" />
          {label}
        </Link>
      ))}
    </nav>
  );
}
