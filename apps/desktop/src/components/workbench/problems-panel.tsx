import { useQuery } from "@tanstack/react-query";
import { AlertTriangle, Info, X } from "lucide-react";
import { useNavigate } from "@tanstack/react-router";
import { useTranslation } from "react-i18next";

import { useDesktopBridge } from "../../platform/desktop-bridge";
import { validationQuery } from "../../queries/project";
import { Button } from "../ui/button";

export function ProblemsPanel({ projectId, onClose }: { projectId: string; onClose: () => void }) {
  const { t } = useTranslation(["workbench", "common", "errors"]);
  const bridge = useDesktopBridge();
  const navigate = useNavigate();
  const validation = useQuery(validationQuery(bridge, projectId));

  return (
    <section className="grid min-h-0 grid-rows-[32px_1fr] border-t border-[var(--border)] bg-[var(--surface-raised)]">
      <header className="flex items-center gap-2 border-b border-[var(--border)] px-2">
        <span className="text-xs font-semibold">{t("issues")}</span>
        {validation.data && (
          <span className="text-[11px] text-[var(--text-muted)]">{validation.data.issues.length}</span>
        )}
        <div className="ml-auto flex items-center gap-1">
          <Button variant="icon" onClick={() => void validation.refetch()} aria-label={t("common:retry")}>
            <Info size={14} />
          </Button>
          <Button variant="icon" onClick={onClose} aria-label={t("common:close")}>
            <X size={14} />
          </Button>
        </div>
      </header>
      <div className="min-h-0 overflow-auto">
        {validation.isPending && (
          <p className="p-3 text-xs text-[var(--text-muted)]">{t("common:loading")}</p>
        )}
        {validation.isError && <p className="p-3 text-xs text-[var(--danger)]">{validation.error.message}</p>}
        {validation.data?.issues.length === 0 && (
          <p className="p-3 text-xs text-[var(--success)]">{t("noValidationIssues")}</p>
        )}
        {validation.data?.issues.map((issue) => (
          <button
            key={issue.id}
            type="button"
            disabled={!issue.unitId}
            className="flex w-full items-start gap-2 border-b border-[var(--border)] px-3 py-2 text-left text-xs hover:bg-[var(--surface-inset)] disabled:cursor-default disabled:hover:bg-transparent"
            onClick={() =>
              issue.unitId &&
              void navigate({
                to: "/projects/$projectId/content",
                params: { projectId },
                search: { unitId: issue.unitId },
              })
            }
          >
            {issue.severity === "blocking" ? (
              <AlertTriangle size={14} className="mt-0.5 shrink-0 text-[var(--danger)]" />
            ) : (
              <Info size={14} className="mt-0.5 shrink-0 text-[var(--warning)]" />
            )}
            <span className="min-w-0">
              <span className="block font-medium">
                {t(issue.messageKey, { ns: "errors", defaultValue: issue.messageKey })}
              </span>
              {issue.detail && <span className="mt-1 block text-[var(--text-muted)]">{issue.detail}</span>}
            </span>
          </button>
        ))}
      </div>
    </section>
  );
}
