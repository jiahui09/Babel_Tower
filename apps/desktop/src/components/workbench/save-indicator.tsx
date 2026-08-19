import { AlertCircle, Check, LoaderCircle, PencilLine } from "lucide-react";
import { useTranslation } from "react-i18next";

import { useWorkbenchStore, type SaveState } from "../../stores/workbench";

const labelKeys: Record<SaveState, "statusEditing" | "statusSaving" | "statusSaved" | "statusError"> = {
  editing: "statusEditing",
  saving: "statusSaving",
  saved: "statusSaved",
  error: "statusError",
  conflict: "statusError",
};

export function SaveIndicator() {
  const { t } = useTranslation("workbench");
  const state = useWorkbenchStore((value) => value.saveState);
  const Icon =
    state === "saved"
      ? Check
      : state === "saving"
        ? LoaderCircle
        : state === "error" || state === "conflict"
          ? AlertCircle
          : PencilLine;

  return (
    <div
      className="flex w-[92px] items-center gap-1.5 text-xs text-[var(--text-secondary)]"
      role="status"
      aria-live="polite"
    >
      <Icon size={14} className={state === "saving" ? "animate-spin" : undefined} aria-hidden="true" />
      <span>{t(labelKeys[state])}</span>
    </div>
  );
}
