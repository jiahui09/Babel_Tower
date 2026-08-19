import { useQuery, useQueryClient } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import { ChevronDown, ChevronUp } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";

import { TranslationEditor } from "../components/workbench/translation-editor";
import { Button } from "../components/ui/button";
import { plainTextDocument, useDesktopBridge, type UnitSummary } from "../platform/desktop-bridge";
import { projectSnapshotQuery, workItemQuery } from "../queries/project";
import { useWorkbenchStore } from "../stores/workbench";

export const Route = createFileRoute("/projects/$projectId/content")({
  validateSearch: (search: Record<string, unknown>) => ({
    unitId: typeof search.unitId === "string" ? search.unitId : undefined,
  }),
  component: LongFormPage,
});

function LongFormPage() {
  const { projectId } = Route.useParams();
  const { unitId: requestedUnitId } = Route.useSearch();
  const { t } = useTranslation(["workbench", "editor", "common"]);
  const bridge = useDesktopBridge();
  const queryClient = useQueryClient();
  const snapshot = useQuery(projectSnapshotQuery(bridge, projectId));
  const [activeIndexOverride, setActiveIndexOverride] = useState<number | null>(null);
  const markTabDirty = useWorkbenchStore((state) => state.markTabDirty);
  const registerTabFlusher = useWorkbenchStore((state) => state.registerTabFlusher);

  const units = useMemo(() => snapshot.data?.units ?? [], [snapshot.data?.units]);
  const restoredIndex =
    (requestedUnitId ?? snapshot.data?.navigation?.position.unitId)
      ? units.findIndex(
          (unit) => unit.unitId === (requestedUnitId ?? snapshot.data?.navigation?.position.unitId),
        )
      : -1;
  const activeIndex = activeIndexOverride ?? (restoredIndex >= 0 ? restoredIndex : 0);
  const activeUnit = units[activeIndex] ?? null;
  const lastNavigationUnit = useRef<string | null>(null);
  useEffect(() => {
    if (!snapshot.data || !activeUnit) return;
    if (lastNavigationUnit.current === activeUnit.unitId) return;
    lastNavigationUnit.current = activeUnit.unitId;
    const previous = snapshot.data.navigation;
    void bridge.saveNavigation({
      projectId: snapshot.data.project.projectId,
      view: "LongForm",
      unitId: activeUnit.unitId,
      scrollAnchorUnitId: activeUnit.unitId,
      positionSequence: (previous?.positionSequence ?? 0) + 1,
      clientSessionId: getClientSessionId(),
      updatedAtMs: Date.now(),
    });
  }, [activeUnit, bridge, snapshot.data]);
  const item = useQuery({
    ...workItemQuery(bridge, projectId, activeUnit?.unitId ?? ""),
    enabled: activeUnit !== null,
  });
  const windowedUnits = useMemo(
    () => units.slice(Math.max(activeIndex - 2, 0), Math.min(activeIndex + 3, units.length)),
    [activeIndex, units],
  );

  if (snapshot.isPending) return <CenteredMessage message={t("loading", { ns: "common" })} />;
  if (snapshot.isError) return <CenteredMessage message={snapshot.error.message} error />;
  if (units.length === 0) return <CenteredMessage message={t("emptyContent", { ns: "workbench" })} />;

  const persist = async (document: Parameters<typeof bridge.saveTranslationDocument>[0]["document"]) => {
    if (!activeUnit) return;
    await bridge.saveTranslationDocument({
      projectId,
      unitId: activeUnit.unitId,
      sourceUnitKey: activeUnit.sourceUnitKey,
      commandId: crypto.randomUUID().replace(/-/g, ""),
      expectedRevisionId: item.data?.revisionId ?? null,
      document,
      createdAtMs: Date.now(),
    });
    await Promise.all([
      queryClient.invalidateQueries({ queryKey: ["project", projectId, "snapshot"] }),
      queryClient.invalidateQueries({ queryKey: ["project", projectId, "work-item", activeUnit.unitId] }),
    ]);
  };

  return (
    <div className="grid h-full min-h-0 grid-rows-[1fr_36px] bg-[var(--surface)]">
      <div className="min-h-0 overflow-auto" aria-label={t("longForm", { ns: "workbench" })}>
        <div className="mx-auto w-full max-w-[1500px] px-5 py-4">
          <div className="sticky top-0 z-10 grid grid-cols-2 gap-4 border-b border-[var(--border)] bg-[var(--surface)] py-2 text-xs font-semibold text-[var(--text-secondary)]">
            <span>{t("source", { ns: "editor" })}</span>
            <span>{t("translation", { ns: "editor" })}</span>
          </div>
          <div className="divide-y divide-[var(--border)]">
            {windowedUnits.map((unit) => (
              <UnitRow
                key={unit.unitId}
                unit={unit}
                active={unit.unitId === activeUnit?.unitId}
                onActivate={() =>
                  setActiveIndexOverride(units.findIndex((candidate) => candidate.unitId === unit.unitId))
                }
                editor={
                  unit.unitId === activeUnit?.unitId && item.data ? (
                    <TranslationEditor
                      unitId={unit.unitId}
                      document={item.data.translation ?? plainTextDocument(unit.translation ?? "")}
                      onPersist={persist}
                      onDirtyChange={(dirty) => markTabDirty("content", dirty)}
                      registerFlush={(flusher) => registerTabFlusher("content", flusher)}
                    />
                  ) : null
                }
              />
            ))}
          </div>
        </div>
      </div>
      <footer className="flex items-center justify-center gap-3 border-t border-[var(--border)] bg-[var(--surface-raised)]">
        <Button
          variant="icon"
          disabled={activeIndex === 0}
          onClick={() => setActiveIndexOverride(Math.max(0, activeIndex - 1))}
          aria-label={t("previousUnit", { ns: "editor" })}
        >
          <ChevronUp size={16} />
        </Button>
        <span className="min-w-[120px] text-center text-xs text-[var(--text-muted)]">
          {t("unitPosition", { ns: "workbench", current: activeIndex + 1, total: units.length })}
        </span>
        <Button
          variant="icon"
          disabled={activeIndex >= units.length - 1}
          onClick={() => setActiveIndexOverride(Math.min(units.length - 1, activeIndex + 1))}
          aria-label={t("nextUnit", { ns: "editor" })}
        >
          <ChevronDown size={16} />
        </Button>
      </footer>
    </div>
  );
}

function UnitRow({
  unit,
  active,
  onActivate,
  editor,
}: {
  unit: UnitSummary;
  active: boolean;
  onActivate: () => void;
  editor: React.ReactNode;
}) {
  return (
    <article
      className="grid min-h-[104px] grid-cols-2 gap-4 py-4 data-[active=true]:bg-[color-mix(in_srgb,var(--selection)_24%,transparent)]"
      data-active={active}
      onClick={onActivate}
    >
      <div className="px-2 font-[var(--editor-font)] text-[var(--editor-font-size)] leading-[var(--editor-line-height)] text-[var(--text-secondary)]">
        {unit.sourceText}
      </div>
      <div className="min-w-0 px-2">
        {active ? (
          editor
        ) : (
          <p className="m-0 whitespace-pre-wrap font-[var(--editor-font)] text-[var(--editor-font-size)] leading-[var(--editor-line-height)]">
            {unit.translation}
          </p>
        )}
      </div>
    </article>
  );
}

function CenteredMessage({ message, error = false }: { message: string; error?: boolean }) {
  return (
    <div
      className={`grid h-full place-items-center p-8 text-sm ${error ? "text-[var(--danger)]" : "text-[var(--text-muted)]"}`}
    >
      {message}
    </div>
  );
}

function getClientSessionId() {
  const key = "babel-tower-client-session";
  const existing = sessionStorage.getItem(key);
  if (existing) return existing;
  const created = crypto.randomUUID().replace(/-/g, "");
  sessionStorage.setItem(key, created);
  return created;
}
