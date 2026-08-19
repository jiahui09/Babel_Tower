import { useQuery } from "@tanstack/react-query";
import { createFileRoute, Link, useNavigate } from "@tanstack/react-router";
import { AlertTriangle, ArrowLeft, CircleCheck, Info } from "lucide-react";
import { useTranslation } from "react-i18next";

import { Button } from "../components/ui/button";
import { useDesktopBridge } from "../platform/desktop-bridge";
import { projectSnapshotQuery } from "../queries/project";

export const Route = createFileRoute("/projects/$projectId/validate")({
  validateSearch: (search: Record<string, unknown>) => ({
    unitId: typeof search.unitId === "string" ? search.unitId : undefined,
  }),
  component: ValidationPage,
});

function ValidationPage() {
  const { projectId } = Route.useParams();
  const { t } = useTranslation(["workbench", "common", "errors"]);
  const bridge = useDesktopBridge();
  const navigate = useNavigate();
  const snapshot = useQuery(projectSnapshotQuery(bridge, projectId));
  const report = useQuery({
    queryKey: ["validation", projectId],
    queryFn: () => bridge.validate(projectId),
    enabled: snapshot.isSuccess,
  });
  if (snapshot.isPending || report.isPending) return <State text={t("common:loading")} />;
  if (snapshot.isError || report.isError)
    return (
      <State
        text={(snapshot.error ?? report.error)?.message ?? t("errors:bridgeUnavailable")}
        error
        onRetry={() => void report.refetch()}
        retryLabel={t("common:retry")}
      />
    );
  const issues = report.data.issues;
  return (
    <div className="h-full overflow-auto p-8">
      <div className="mx-auto max-w-[860px]">
        <Link
          to="/projects/$projectId/content"
          params={{ projectId }}
          search={{ unitId: undefined }}
          className="flex items-center gap-2 text-sm text-[var(--text-secondary)]"
        >
          <ArrowLeft size={15} /> {t("common:back")}
        </Link>
        <div className="mt-8 flex items-start justify-between gap-4">
          <div>
            <h1 className="m-0 text-xl font-semibold">{t("workbench:issues")}</h1>
            <p className="mb-0 mt-2 text-sm text-[var(--text-secondary)]">
              {t("workbench:validationChecked", { time: new Date(report.data.checkedAtMs).toLocaleString() })}
            </p>
          </div>
          <Button variant="secondary" onClick={() => void report.refetch()}>
            {t("common:retry")}
          </Button>
        </div>
        {issues.length === 0 ? (
          <div className="mt-8 flex items-center gap-3 border-y border-[var(--border)] bg-[var(--surface-raised)] p-6 text-sm text-[var(--success)]">
            <CircleCheck size={18} /> {t("workbench:noValidationIssues")}
          </div>
        ) : (
          <div className="mt-8 border-y border-[var(--border)] bg-[var(--surface-raised)]">
            {issues.map((issue) => (
              <div key={issue.id} className="flex gap-3 border-b border-[var(--border)] p-4 last:border-b-0">
                {issue.severity === "blocking" ? (
                  <AlertTriangle size={18} className="text-[var(--danger)]" />
                ) : (
                  <Info size={18} className="text-[var(--warning)]" />
                )}
                <div className="min-w-0">
                  <strong className="text-sm">
                    {t(issue.messageKey, { ns: "errors", defaultValue: issue.messageKey })}
                  </strong>
                  {issue.detail && (
                    <p className="mb-0 mt-1 text-xs text-[var(--text-secondary)]">{issue.detail}</p>
                  )}
                  {issue.unitId && (
                    <Button
                      variant="ghost"
                      className="mt-2 px-0"
                      onClick={() =>
                        void navigate({
                          to: "/projects/$projectId/content",
                          params: { projectId },
                          search: { unitId: issue.unitId },
                        })
                      }
                    >
                      {t("workbench:openUnit")}
                    </Button>
                  )}
                </div>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

function State({
  text,
  error = false,
  onRetry,
  retryLabel = "",
}: {
  text: string;
  error?: boolean;
  onRetry?: () => void;
  retryLabel?: string;
}) {
  return (
    <div
      className={`grid h-full place-items-center p-8 text-sm ${error ? "text-[var(--danger)]" : "text-[var(--text-muted)]"}`}
    >
      <div className="text-center">
        <p>{text}</p>
        {onRetry && (
          <Button variant="secondary" onClick={onRetry}>
            {retryLabel}
          </Button>
        )}
      </div>
    </div>
  );
}
