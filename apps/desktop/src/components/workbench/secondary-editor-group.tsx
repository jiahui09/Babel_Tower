import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";

import { useDesktopBridge } from "../../platform/desktop-bridge";
import { workItemQuery } from "../../queries/project";
import { useWorkbenchStore } from "../../stores/workbench";
import { DocumentTabs } from "./document-tabs";
import { TranslationEditor } from "./translation-editor";

export function SecondaryEditorGroup({
  projectId,
  fallbackUnitId,
}: {
  projectId: string;
  fallbackUnitId: string | null;
}) {
  const { t } = useTranslation("editor");
  const bridge = useDesktopBridge();
  const queryClient = useQueryClient();
  const tabs = useWorkbenchStore((state) => state.tabs);
  const group = useWorkbenchStore((state) => state.groups.find((item) => item.id === "secondary"));
  const activeTab = tabs.find((tab) => tab.id === group?.activeTabId);
  const unitId = activeTab?.unitId ?? fallbackUnitId;
  const item = useQuery({
    ...workItemQuery(bridge, projectId, unitId ?? ""),
    enabled: Boolean(unitId),
  });
  const markTabDirty = useWorkbenchStore((state) => state.markTabDirty);
  const registerTabFlusher = useWorkbenchStore((state) => state.registerTabFlusher);

  const persist = async (document: Parameters<typeof bridge.saveTranslationDocument>[0]["document"]) => {
    if (!item.data || !unitId) return;
    await bridge.saveTranslationDocument({
      projectId,
      unitId,
      sourceUnitKey: item.data.sourceUnitKey,
      commandId: crypto.randomUUID().replace(/-/g, ""),
      expectedRevisionId: item.data.revisionId,
      document,
      createdAtMs: Date.now(),
    });
    await Promise.all([
      queryClient.invalidateQueries({ queryKey: ["project", projectId, "snapshot"] }),
      queryClient.invalidateQueries({ queryKey: ["project", projectId, "work-item", unitId] }),
    ]);
  };

  return (
    <section className="grid h-full min-h-0 grid-rows-[32px_1fr]" aria-label={t("secondaryPreview")}>
      <DocumentTabs groupId="secondary" />
      <div className="min-h-0 overflow-auto bg-[var(--surface)] p-4">
        {item.isPending ? (
          <p className="text-xs text-[var(--text-muted)]">{t("loading", { ns: "common" })}</p>
        ) : item.isError ? (
          <p className="text-xs text-[var(--danger)]">{item.error.message}</p>
        ) : item.data ? (
          activeTab?.kind === "source" || activeTab?.kind === "diff" ? (
            <div className="grid min-h-full grid-cols-2 divide-x divide-[var(--border)] border border-[var(--border)]">
              <article className="p-4">
                <h2 className="mb-3 text-xs font-semibold text-[var(--text-muted)]">{t("source")}</h2>
                <p className="whitespace-pre-wrap text-sm leading-7">{item.data.sourceText}</p>
              </article>
              <article className="p-4">
                <h2 className="mb-3 text-xs font-semibold text-[var(--text-muted)]">{t("translation")}</h2>
                <p className="whitespace-pre-wrap text-sm leading-7 text-[var(--text-secondary)]">
                  {item.data.translationText}
                </p>
              </article>
            </div>
          ) : (
            <div className="min-h-full border border-[var(--border)] bg-[var(--surface-raised)]">
              <TranslationEditor
                unitId={unitId ?? ""}
                document={item.data.translation}
                onPersist={persist}
                onDirtyChange={(dirty) => activeTab && markTabDirty(activeTab.id, dirty)}
                registerFlush={activeTab ? (flusher) => registerTabFlusher(activeTab.id, flusher) : undefined}
              />
            </div>
          )
        ) : (
          <p className="text-xs text-[var(--text-muted)]">{t("secondaryPreview")}</p>
        )}
      </div>
    </section>
  );
}
