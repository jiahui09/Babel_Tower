import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";

import { useDesktopBridge } from "../../platform/desktop-bridge";
import { workItemQuery } from "../../queries/project";
import { useWorkbenchStore } from "../../stores/workbench";
import { DocumentTabs } from "./document-tabs";

export function SecondaryEditorGroup({ projectId, fallbackUnitId }: { projectId: string; fallbackUnitId: string | null }) {
  const { t } = useTranslation("editor");
  const bridge = useDesktopBridge();
  const tabs = useWorkbenchStore((state) => state.tabs);
  const group = useWorkbenchStore((state) => state.groups.find((item) => item.id === "secondary"));
  const activeTab = tabs.find((tab) => tab.id === group?.activeTabId);
  const unitId = activeTab?.unitId ?? fallbackUnitId;
  const item = useQuery({
    ...workItemQuery(bridge, projectId, unitId ?? ""),
    enabled: Boolean(unitId),
  });

  return (
    <section className="grid h-full min-h-0 grid-rows-[32px_1fr]" aria-label={t("secondaryPreview")}>
      <DocumentTabs groupId="secondary" onActivate={() => undefined} />
      <div className="min-h-0 overflow-auto bg-[var(--surface)] p-4">
        {item.isPending ? (
          <p className="text-xs text-[var(--text-muted)]">{t("loading", { ns: "common" })}</p>
        ) : item.isError ? (
          <p className="text-xs text-[var(--danger)]">{item.error.message}</p>
        ) : item.data ? (
          <div className="grid min-h-full grid-cols-2 divide-x divide-[var(--border)] border border-[var(--border)]">
            <article className="p-4">
              <h2 className="mb-3 text-xs font-semibold text-[var(--text-muted)]">{t("source")}</h2>
              <p className="whitespace-pre-wrap text-sm leading-7">{item.data.sourceText}</p>
            </article>
            <article className="p-4">
              <h2 className="mb-3 text-xs font-semibold text-[var(--text-muted)]">{t("translation")}</h2>
              <p className="whitespace-pre-wrap text-sm leading-7 text-[var(--text-secondary)]">{item.data.translationText}</p>
            </article>
          </div>
        ) : (
          <p className="text-xs text-[var(--text-muted)]">{t("secondaryPreview")}</p>
        )}
      </div>
    </section>
  );
}
