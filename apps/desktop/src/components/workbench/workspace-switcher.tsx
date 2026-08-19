import { Link } from "@tanstack/react-router";
import { FileText, Image, Rows3 } from "lucide-react";
import { useTranslation } from "react-i18next";

import { cn } from "../../lib/utils";

const items = [
  { to: "/projects/$projectId/content", search: { unitId: undefined }, labelKey: "longForm", icon: FileText },
  { to: "/projects/$projectId/units", labelKey: "units", icon: Rows3 },
  { to: "/projects/$projectId/resources", labelKey: "resources", icon: Image },
] as const;

export function WorkspaceSwitcher({ projectId }: { projectId: string }) {
  const { t } = useTranslation("workbench");
  return (
    <nav
      className="flex h-8 items-center rounded-[6px] border border-[var(--border)] bg-[var(--surface-inset)] p-0.5"
      aria-label={t("translation")}
    >
      {items.map(({ to, labelKey, icon: Icon }) => (
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
          {t(labelKey)}
        </Link>
      ))}
    </nav>
  );
}
