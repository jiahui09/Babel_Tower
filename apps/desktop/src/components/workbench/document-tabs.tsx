import { Columns2, LoaderCircle, X } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import * as ContextMenu from "@radix-ui/react-context-menu";

import { useWorkbenchStore, type WorkbenchTab } from "../../stores/workbench";
import { Button } from "../ui/button";
import { Dialog, DialogContent, DialogDescription, DialogTitle } from "../ui/dialog";

export function DocumentTabs({
  groupId = "primary",
  onActivate,
}: {
  groupId?: "primary" | "secondary";
  onActivate?: (tab: WorkbenchTab) => void;
}) {
  const { t } = useTranslation(["editor", "common"]);
  const tabs = useWorkbenchStore((state) => state.tabs);
  const group = useWorkbenchStore((state) => state.groups.find((item) => item.id === groupId));
  const activateTab = useWorkbenchStore((state) => state.activateTab);
  const closeTab = useWorkbenchStore((state) => state.closeTab);
  const flushers = useWorkbenchStore((state) => state.flushers);
  const splitTab = useWorkbenchStore((state) => state.splitTab);
  const closeOtherTabs = useWorkbenchStore((state) => state.closeOtherTabs);
  const closeTabsToRight = useWorkbenchStore((state) => state.closeTabsToRight);
  const moveTab = useWorkbenchStore((state) => state.moveTab);
  const [pendingClose, setPendingClose] = useState<WorkbenchTab | null>(null);
  const [closeError, setCloseError] = useState<string | null>(null);
  const [flushing, setFlushing] = useState(false);
  const visible = group?.tabIds
    .map((id) => tabs.find((tab) => tab.id === id))
    .filter(Boolean) as WorkbenchTab[];
  return (
    <>
      <div className="flex h-8 min-w-0 overflow-x-auto border-b border-[var(--border)] bg-[var(--surface-raised)]">
        {visible.map((tab, index) => (
          <ContextMenu.Root key={tab.id}>
          <ContextMenu.Trigger asChild>
          <div
            draggable
            onDragStart={(event) => event.dataTransfer.setData("application/x-babel-tab", tab.id)}
            onDragOver={(event) => event.preventDefault()}
            onDrop={(event) => {
              event.preventDefault();
              const tabId = event.dataTransfer.getData("application/x-babel-tab");
              if (tabId) moveTab(tabId, index, groupId);
            }}
            className="group flex h-8 min-w-[120px] max-w-[220px] items-center gap-1 border-r border-[var(--border)] px-1 text-xs text-[var(--text-secondary)] data-[active=true]:bg-[var(--surface)] data-[active=true]:text-[var(--text)]"
            data-active={group?.activeTabId === tab.id}
          >
            <button
              type="button"
              className="flex h-full min-w-0 flex-1 items-center gap-2 px-1 text-left outline-none"
              aria-current={group?.activeTabId === tab.id ? "page" : undefined}
              onClick={() => {
                activateTab(tab.id, groupId);
                onActivate?.(tab);
              }}
            >
              {tab.dirty && <span className="size-1.5 shrink-0 rounded-full bg-[var(--accent)]" />}
              <span className="min-w-0 flex-1 truncate">{tab.title}</span>
            </button>
            {groupId === "primary" && (
              <button
                type="button"
                className="grid size-5 shrink-0 place-items-center rounded-[4px] opacity-0 hover:bg-[var(--surface-inset)] group-hover:opacity-100 focus:opacity-100"
                onClick={(event) => {
                  event.stopPropagation();
                  splitTab(tab.id);
                }}
                aria-label={t("openInOtherGroup")}
              >
                <Columns2 size={12} />
              </button>
            )}
            <button
              type="button"
              className="grid size-5 shrink-0 place-items-center rounded-[4px] opacity-0 hover:bg-[var(--surface-inset)] group-hover:opacity-100 focus:opacity-100"
              onClick={(event) => {
                event.stopPropagation();
                if (tab.dirty) {
                  setCloseError(null);
                  setPendingClose(tab);
                } else closeTab(tab.id, groupId);
              }}
              aria-label={t("common:close")}
            >
              <X size={13} />
            </button>
          </div>
          </ContextMenu.Trigger>
          <ContextMenu.Portal>
            <ContextMenu.Content className="z-50 min-w-44 rounded-[var(--radius)] border border-[var(--border)] bg-[var(--surface-raised)] p-1 text-xs shadow-lg">
              <ContextMenu.Item className="cursor-default rounded-[4px] px-2 py-1.5 outline-none focus:bg-[var(--surface-inset)]" onSelect={() => closeOtherTabs(tab.id, groupId)}>{t("closeOthers")}</ContextMenu.Item>
              <ContextMenu.Item className="cursor-default rounded-[4px] px-2 py-1.5 outline-none focus:bg-[var(--surface-inset)]" onSelect={() => closeTabsToRight(tab.id, groupId)}>{t("closeRight")}</ContextMenu.Item>
            </ContextMenu.Content>
          </ContextMenu.Portal>
          </ContextMenu.Root>
        ))}
      </div>
      <Dialog
        open={pendingClose !== null}
        onOpenChange={(open) => {
          if (!open) {
            setCloseError(null);
            setPendingClose(null);
          }
        }}
      >
        <DialogContent className="max-w-[480px]">
          <DialogTitle>{t("closeDirtyTitle")}</DialogTitle>
          <DialogDescription>{t("closeDirtyDescription")}</DialogDescription>
          {closeError && (
            <p className="mt-3 text-xs text-[var(--danger)]" role="alert">
              {closeError}
            </p>
          )}
          <div className="mt-5 flex justify-end gap-2">
            <Button disabled={flushing} onClick={() => setPendingClose(null)}>
              {t("cancel", { ns: "common" })}
            </Button>
            <Button
              variant="danger"
              disabled={flushing}
              onClick={async () => {
                if (pendingClose) {
                  setFlushing(true);
                  setCloseError(null);
                  const flushed = await (flushers[pendingClose.id]?.() ?? Promise.resolve(false));
                  setFlushing(false);
                  if (!flushed) {
                    setCloseError(t("saveFailed", { ns: "common" }));
                    return;
                  }
                  closeTab(pendingClose.id, groupId);
                }
                setPendingClose(null);
              }}
            >
              {flushing ? <LoaderCircle className="animate-spin" size={14} /> : t("close", { ns: "common" })}
            </Button>
          </div>
        </DialogContent>
      </Dialog>
    </>
  );
}
