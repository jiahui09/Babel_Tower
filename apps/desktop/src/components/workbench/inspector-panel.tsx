import { useQuery } from "@tanstack/react-query";
import { useNavigate } from "@tanstack/react-router";
import { MessageSquareText, Tags, TriangleAlert, Wrench } from "lucide-react";
import { useTranslation } from "react-i18next";

import { useDesktopBridge, type ProjectSnapshot } from "../../platform/desktop-bridge";
import { validationQuery } from "../../queries/project";
import { useWorkbenchStore, type InspectorPanel } from "../../stores/workbench";
import { cn } from "../../lib/utils";
import { Button } from "../ui/button";
import { ScrollArea } from "../ui/scroll-area";
import { Tooltip } from "../ui/tooltip";

const panels: Array<{
  id: InspectorPanel;
  icon: typeof Tags;
  label: "terms" | "annotations" | "properties" | "issues";
}> = [
  { id: "terms", icon: Tags, label: "terms" },
  { id: "annotations", icon: MessageSquareText, label: "annotations" },
  { id: "properties", icon: Wrench, label: "properties" },
  { id: "issues", icon: TriangleAlert, label: "issues" },
];

export function InspectorPanelView({
  snapshot,
  currentUnitId,
}: {
  snapshot: ProjectSnapshot;
  currentUnitId: string | null;
}) {
  const { t } = useTranslation(["workbench", "common"]);
  const panel = useWorkbenchStore((state) => state.inspectorPanel);
  const setPanel = useWorkbenchStore((state) => state.setInspectorPanel);
  const current = snapshot.units.find((unit) => unit.unitId === currentUnitId) ?? null;
  const bridge = useDesktopBridge();
  const navigate = useNavigate();
  const terms = useQuery({
    queryKey: ["project", snapshot.project.projectId, "terms", current?.sourceText],
    queryFn: () =>
      bridge.termsForUnit({ projectId: snapshot.project.projectId, sourceText: current!.sourceText }),
    enabled: Boolean(current),
  });
  const annotations = useQuery({
    queryKey: ["project", snapshot.project.projectId, "annotations", currentUnitId],
    queryFn: () =>
      bridge.annotationsForUnit({ projectId: snapshot.project.projectId, unitId: currentUnitId! }),
    enabled: Boolean(currentUnitId),
  });
  const validation = useQuery(validationQuery(bridge, snapshot.project.projectId));
  return (
    <aside className="grid h-full min-h-0 grid-rows-[32px_1fr] bg-[var(--surface-raised)]">
      <div className="flex items-center border-b border-[var(--border)] px-1">
        {panels.map(({ id, icon: Icon, label }) => (
          <Tooltip key={id} label={t(label)}>
            <Button
              variant="icon"
              className={cn("size-7", panel === id && "bg-[var(--surface-inset)] text-[var(--text)]")}
              onClick={() => setPanel(id)}
              aria-label={t(label)}
            >
              <Icon size={15} />
            </Button>
          </Tooltip>
        ))}
      </div>
      <ScrollArea className="min-h-0 p-3">
        {panel === "properties" ? (
          <dl className="m-0 grid grid-cols-[96px_1fr] gap-x-3 gap-y-2 text-xs">
            <dt className="text-[var(--text-muted)]">{t("project")}</dt>
            <dd className="m-0 break-all text-[var(--text-secondary)]">{snapshot.project.projectId}</dd>
            <dt className="text-[var(--text-muted)]">{t("unitCount")}</dt>
            <dd className="m-0">{snapshot.units.length}</dd>
            <dt className="text-[var(--text-muted)]">{t("commitSequence")}</dt>
            <dd className="m-0">{snapshot.project.commitSequence}</dd>
            {current && (
              <>
                <dt className="text-[var(--text-muted)]">{t("currentUnit")}</dt>
                <dd className="m-0 break-all">{current.localIndex + 1}</dd>
              </>
            )}
          </dl>
        ) : panel === "terms" ? (
          <div className="space-y-2 text-xs">
            {terms.isPending && <p className="m-0 text-[var(--text-muted)]">{t("common:loading")}</p>}
            {terms.isError && <p className="m-0 text-[var(--danger)]">{terms.error.message}</p>}
            {!terms.isPending && !terms.isError && terms.data?.length === 0 && (
              <p className="m-0 text-[var(--text-muted)]">{t("emptyTerms")}</p>
            )}
            {terms.data?.map((term) => (
              <div key={term.termId} className="border-b border-[var(--border)] pb-2">
                <div className="font-medium">{term.sourceText}</div>
                <div className="text-[var(--accent)]">{term.preferredTranslation}</div>
                {term.notes && <div className="mt-1 text-[var(--text-muted)]">{term.notes}</div>}
              </div>
            ))}
          </div>
        ) : panel === "annotations" ? (
          <div className="space-y-2 text-xs">
            {annotations.isPending && <p className="m-0 text-[var(--text-muted)]">{t("common:loading")}</p>}
            {annotations.isError && <p className="m-0 text-[var(--danger)]">{annotations.error.message}</p>}
            {!annotations.isPending && !annotations.isError && annotations.data?.length === 0 && (
              <p className="m-0 text-[var(--text-muted)]">{t("emptyAnnotations")}</p>
            )}
            {annotations.data?.map((annotation) => (
              <div key={annotation.annotationId} className="border-b border-[var(--border)] pb-2">
                <div>{annotation.body}</div>
                <div className="mt-1 text-[var(--text-muted)]">
                  {annotation.stale ? t("staleAnnotation") : t("activeAnnotation")}
                </div>
              </div>
            ))}
          </div>
        ) : panel === "issues" ? (
          <div className="space-y-2 text-xs">
            {validation.isPending && <p className="m-0 text-[var(--text-muted)]">{t("common:loading")}</p>}
            {validation.isError && <p className="m-0 text-[var(--danger)]">{validation.error.message}</p>}
            {validation.data?.issues.length === 0 && (
              <p className="m-0 text-[var(--success)]">{t("noValidationIssues")}</p>
            )}
            {validation.data?.issues.map((issue) => (
              <button
                key={issue.id}
                type="button"
                className="block w-full border-b border-[var(--border)] pb-2 text-left hover:bg-[var(--surface-inset)]"
                onClick={() =>
                  issue.unitId &&
                  void navigate({
                    to: "/projects/$projectId/content",
                    params: { projectId: snapshot.project.projectId },
                    search: { unitId: issue.unitId },
                  })
                }
                disabled={!issue.unitId}
              >
                <span
                  className={issue.severity === "blocking" ? "text-[var(--danger)]" : "text-[var(--warning)]"}
                >
                  {t(issue.messageKey, { ns: "errors", defaultValue: issue.messageKey })}
                </span>
                {issue.detail && <span className="mt-1 block text-[var(--text-muted)]">{issue.detail}</span>}
              </button>
            ))}
          </div>
        ) : (
          <p className="m-0 text-xs leading-5 text-[var(--text-muted)]">{t(panel)}</p>
        )}
      </ScrollArea>
    </aside>
  );
}
