import { X } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";

import { useWorkbenchStore, type WorkbenchTab } from "../../stores/workbench";
import { Button } from "../ui/button";
import { Dialog, DialogContent, DialogDescription, DialogTitle } from "../ui/dialog";

export function DocumentTabs({ onActivate }: { onActivate: (tab: WorkbenchTab) => void }) {
  const { t } = useTranslation(["editor", "common"]);
  const tabs = useWorkbenchStore((state) => state.tabs);
  const group = useWorkbenchStore((state) => state.groups.find((item) => item.id === "primary"));
  const activateTab = useWorkbenchStore((state) => state.activateTab);
  const closeTab = useWorkbenchStore((state) => state.closeTab);
  const flushers = useWorkbenchStore((state) => state.flushers);
  const [pendingClose, setPendingClose] = useState<WorkbenchTab | null>(null);
  const visible = group?.tabIds
    .map((id) => tabs.find((tab) => tab.id === id))
    .filter(Boolean) as WorkbenchTab[];
  return (
    <>
      <div className="flex h-8 min-w-0 overflow-x-auto border-b border-[var(--border)] bg-[var(--surface-raised)]">
        {visible.map((tab) => (
          <button
            key={tab.id}
            type="button"
            className="group flex h-8 min-w-[120px] max-w-[220px] items-center gap-2 border-r border-[var(--border)] px-2 text-xs text-[var(--text-secondary)] data-[active=true]:bg-[var(--surface)] data-[active=true]:text-[var(--text)]"
            data-active={group?.activeTabId === tab.id}
            onClick={() => {
              activateTab(tab.id, "primary");
              onActivate(tab);
            }}
          >
            {tab.dirty && <span className="size-1.5 shrink-0 rounded-full bg-[var(--accent)]" />}
            <span className="min-w-0 flex-1 truncate text-left">{tab.title}</span>
            <span
              role="button"
              tabIndex={0}
              className="grid size-5 shrink-0 place-items-center rounded-[4px] opacity-0 hover:bg-[var(--surface-inset)] group-hover:opacity-100"
              onClick={(event) => {
                event.stopPropagation();
                if (tab.dirty) setPendingClose(tab);
                else closeTab(tab.id);
              }}
              onKeyDown={(event) => {
                if (event.key === "Enter") {
                  event.stopPropagation();
                  if (tab.dirty) setPendingClose(tab);
                  else closeTab(tab.id);
                }
              }}
              aria-label={t("closeDirtyTitle")}
            >
              <X size={13} />
            </span>
          </button>
        ))}
      </div>
      <Dialog open={pendingClose !== null} onOpenChange={(open) => !open && setPendingClose(null)}>
        <DialogContent className="max-w-[480px]">
          <DialogTitle>{t("closeDirtyTitle")}</DialogTitle>
          <DialogDescription>{t("closeDirtyDescription")}</DialogDescription>
          <div className="mt-5 flex justify-end gap-2">
            <Button onClick={() => setPendingClose(null)}>{t("cancel", { ns: "common" })}</Button>
            <Button
              variant="danger"
              onClick={async () => {
                if (pendingClose) {
                  const flushed = await (flushers[pendingClose.id]?.() ?? Promise.resolve(false));
                  if (!flushed) return;
                  closeTab(pendingClose.id);
                }
                setPendingClose(null);
              }}
            >
              {t("close", { ns: "common" })}
            </Button>
          </div>
        </DialogContent>
      </Dialog>
    </>
  );
}
