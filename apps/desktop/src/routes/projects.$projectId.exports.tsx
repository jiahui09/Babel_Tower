import { useQuery } from "@tanstack/react-query";
import { createFileRoute, Link } from "@tanstack/react-router";
import { ArrowLeft, FileOutput } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";

import { Button } from "../components/ui/button";
import { chooseExportPath } from "../lib/dialog";
import { useDesktopBridge } from "../platform/desktop-bridge";

export const Route = createFileRoute("/projects/$projectId/exports")({ component: ExportsPage });

function ExportsPage() {
  const { projectId } = Route.useParams();
  const { t } = useTranslation(["workbench", "common", "errors"]);
  const bridge = useDesktopBridge();
  const [creating, setCreating] = useState(false);
  const [actionError, setActionError] = useState<string | null>(null);
  const exportsQuery = useQuery({
    queryKey: ["exports", projectId],
    queryFn: () => bridge.listExports(projectId),
  });
  if (exportsQuery.isPending) return <State text={t("common:loading")} />;
  if (exportsQuery.isError)
    return (
      <State
        text={exportsQuery.error.message}
        error
        retryLabel={t("common:retry")}
        onRetry={() => void exportsQuery.refetch()}
      />
    );
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
        <div className="mt-8 flex items-center">
          <div>
            <h1 className="m-0 text-xl font-semibold">{t("workbench:exportRecords")}</h1>
            <p className="mb-0 mt-2 text-sm text-[var(--text-secondary)]">
              {t("workbench:exportOriginalSafe")}
            </p>
          </div>
          <Button
            variant="primary"
            className="ml-auto"
            disabled={creating}
            onClick={async () => {
              setCreating(true);
              setActionError(null);
              try {
                const destinationPath = await chooseExportPath();
                if (!destinationPath) return;
                await bridge.createExport({
                  projectId,
                  destinationPath,
                  commandId: crypto.randomUUID().replace(/-/g, ""),
                  createdAtMs: Date.now(),
                });
                await exportsQuery.refetch();
              } catch (reason) {
                setActionError(reason instanceof Error ? reason.message : String(reason));
              } finally {
                setCreating(false);
              }
            }}
          >
            <FileOutput size={16} /> {t("workbench:newExport")}
          </Button>
        </div>
        {actionError && <p className="mt-4 text-xs text-[var(--danger)]">{actionError}</p>}
        {exportsQuery.data.length === 0 ? (
          <div className="mt-8 border-y border-[var(--border)] py-8 text-center text-sm text-[var(--text-muted)]">
            {t("workbench:noExports")}
          </div>
        ) : (
          <div className="mt-8 border-y border-[var(--border)]">
            {exportsQuery.data.map((record) => (
              <div
                key={record.id}
                className="flex items-center justify-between border-b border-[var(--border)] p-4 last:border-b-0"
              >
                <div className="min-w-0">
                  <span className="block truncate text-sm">{record.path}</span>
                  <span className="mt-1 block text-xs text-[var(--text-muted)]">
                    {record.format} · {record.outputHash || t("workbench:hashUnavailable")}
                  </span>
                  {record.error && (
                    <span className="mt-1 block text-xs text-[var(--danger)]">{record.error}</span>
                  )}
                </div>
                <span
                  className={
                    record.status === "succeeded"
                      ? "text-xs text-[var(--success)]"
                      : "text-xs text-[var(--danger)]"
                  }
                >
                  {t(`workbench:exportStatus.${record.status}`)}
                </span>
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
  retryLabel,
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
