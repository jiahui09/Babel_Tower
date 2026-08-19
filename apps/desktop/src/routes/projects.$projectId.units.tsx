import { useQuery, useQueryClient } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import { useVirtualizer } from "@tanstack/react-virtual";
import { useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";

import { Input } from "../components/ui/input";
import { Switch } from "../components/ui/switch";
import { plainTextDocument, useDesktopBridge, type UnitSummary } from "../platform/desktop-bridge";
import { projectSnapshotQuery } from "../queries/project";
import { useWorkbenchStore } from "../stores/workbench";

export const Route = createFileRoute("/projects/$projectId/units")({ component: UnitsPage });

function UnitsPage() {
  const { projectId } = Route.useParams();
  const { t } = useTranslation(["workbench", "editor", "common"]);
  const bridge = useDesktopBridge();
  const queryClient = useQueryClient();
  const snapshot = useQuery(projectSnapshotQuery(bridge, projectId));
  const [query, setQuery] = useState("");
  const [unfinishedOnly, setUnfinishedOnly] = useState(false);
  const [drafts, setDrafts] = useState<Record<string, string>>({});
  const setSaveState = useWorkbenchStore((state) => state.setSaveState);
  const markTabDirty = useWorkbenchStore((state) => state.markTabDirty);
  const rows = useMemo(() => {
    const normalized = query.trim().toLocaleLowerCase();
    return (snapshot.data?.units ?? []).filter((unit) => {
      if (unfinishedOnly && unit.translation) return false;
      if (!normalized) return true;
      return `${unit.sourceText}\n${unit.translation ?? ""}`.toLocaleLowerCase().includes(normalized);
    });
  }, [query, snapshot.data?.units, unfinishedOnly]);
  const parentRef = useRef<HTMLDivElement>(null);
  const virtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => 66,
    overscan: 8,
  });

  const saveRow = async (unit: UnitSummary) => {
    const text = drafts[unit.unitId];
    if (text === undefined || text === (unit.translation ?? "")) return;
    setSaveState("saving");
    try {
      const item = await bridge.workItem(projectId, unit.unitId);
      await bridge.saveTranslationDocument({
        projectId,
        unitId: unit.unitId,
        sourceUnitKey: unit.sourceUnitKey,
        commandId: crypto.randomUUID().replace(/-/g, ""),
        expectedRevisionId: item.revisionId,
        document: plainTextDocument(text),
        createdAtMs: Date.now(),
      });
      setSaveState("saved");
      markTabDirty("units", false);
      await queryClient.invalidateQueries({ queryKey: ["project", projectId] });
    } catch (reason) {
      setSaveState(reason instanceof Error && reason.message.includes("revision") ? "conflict" : "error");
    }
  };

  if (snapshot.isPending) return <Centered text={t("loading", { ns: "common" })} />;
  if (snapshot.isError) return <Centered text={snapshot.error.message} error />;
  return (
    <div className="grid h-full min-h-0 grid-rows-[42px_32px_1fr]">
      <div className="flex items-center gap-3 border-b border-[var(--border)] bg-[var(--surface-raised)] px-3">
        <Input
          className="max-w-[320px]"
          type="search"
          value={query}
          onChange={(event) => setQuery(event.target.value)}
          placeholder={t("filterUnits", { ns: "workbench" })}
          aria-label={t("filterUnits", { ns: "workbench" })}
        />
        <label className="flex items-center gap-2 text-xs text-[var(--text-secondary)]">
          <Switch checked={unfinishedOnly} onCheckedChange={setUnfinishedOnly} />
          {t("showUnfinished", { ns: "workbench" })}
        </label>
        <span className="ml-auto text-xs text-[var(--text-muted)]">{rows.length}</span>
      </div>
      <div className="grid grid-cols-[96px_minmax(240px,1fr)_minmax(260px,1fr)] items-center border-b border-[var(--border)] bg-[var(--surface-inset)] px-3 text-xs font-semibold text-[var(--text-secondary)]">
        <span>{t("status", { ns: "workbench" })}</span>
        <span className="px-3">{t("source", { ns: "editor" })}</span>
        <span className="px-3">{t("translation", { ns: "editor" })}</span>
      </div>
      {rows.length === 0 ? (
        <Centered text={t("noMatchingUnits", { ns: "workbench" })} />
      ) : (
        <div ref={parentRef} className="min-h-0 overflow-auto">
          <div style={{ height: virtualizer.getTotalSize(), position: "relative" }}>
            {virtualizer.getVirtualItems().map((virtualRow) => {
              const unit = rows[virtualRow.index];
              const value = drafts[unit.unitId] ?? unit.translation ?? "";
              return (
                <div
                  key={unit.unitId}
                  className="absolute left-0 top-0 grid w-full grid-cols-[96px_minmax(240px,1fr)_minmax(260px,1fr)] items-center border-b border-[var(--border)] px-3 text-sm"
                  style={{ height: virtualRow.size, transform: `translateY(${virtualRow.start}px)` }}
                >
                  <span className="text-xs text-[var(--text-muted)]">
                    {t(value ? "draft" : "untranslated", { ns: "workbench" })}
                  </span>
                  <p className="m-0 line-clamp-2 border-x border-[var(--border)] px-3 leading-5 text-[var(--text-secondary)]">
                    {unit.sourceText}
                  </p>
                  <Input
                    className="mx-3 w-[calc(100%-24px)] border-transparent bg-transparent focus:border-[var(--accent)]"
                    value={value}
                    onChange={(event) => {
                      setDrafts((current) => ({ ...current, [unit.unitId]: event.target.value }));
                      setSaveState("editing");
                      markTabDirty("units", true);
                    }}
                    onBlur={() => void saveRow(unit)}
                    onKeyDown={(event) => {
                      if (event.key === "Enter" && !event.nativeEvent.isComposing) {
                        event.currentTarget.blur();
                      }
                    }}
                    aria-label={`${t("translation", { ns: "editor" })} ${unit.localIndex + 1}`}
                  />
                </div>
              );
            })}
          </div>
        </div>
      )}
    </div>
  );
}

function Centered({ text, error = false }: { text: string; error?: boolean }) {
  return (
    <div
      className={`grid h-full place-items-center p-8 text-sm ${error ? "text-[var(--danger)]" : "text-[var(--text-muted)]"}`}
    >
      {text}
    </div>
  );
}
