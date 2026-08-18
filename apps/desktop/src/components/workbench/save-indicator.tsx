import { AlertCircle, Check, LoaderCircle, PencilLine } from "lucide-react";

import { useWorkbenchStore, type SaveState } from "../../stores/workbench";

const labels: Record<SaveState, string> = {
  editing: "正在编辑",
  saving: "正在保存",
  saved: "已保存",
  error: "保存失败",
};

export function SaveIndicator() {
  const state = useWorkbenchStore((value) => value.saveState);
  const Icon =
    state === "saved"
      ? Check
      : state === "saving"
        ? LoaderCircle
        : state === "error"
          ? AlertCircle
          : PencilLine;

  return (
    <div
      className="flex w-[92px] items-center gap-1.5 text-xs text-[var(--text-secondary)]"
      role="status"
      aria-live="polite"
    >
      <Icon size={14} className={state === "saving" ? "animate-spin" : undefined} aria-hidden="true" />
      <span>{labels[state]}</span>
    </div>
  );
}
