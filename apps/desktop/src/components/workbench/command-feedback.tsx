import { X } from "lucide-react";
import { useTranslation } from "react-i18next";

import { useWorkbenchStore } from "../../stores/workbench";
import { Button } from "../ui/button";

export function CommandFeedback() {
  const { t } = useTranslation("common");
  const error = useWorkbenchStore((state) => state.commandError);
  const setCommandError = useWorkbenchStore((state) => state.setCommandError);
  if (!error) return null;
  return (
    <div
      className="fixed bottom-8 right-4 z-50 flex max-w-[min(480px,calc(100vw-32px))] items-start gap-2 border border-[var(--danger)] bg-[var(--surface-raised)] px-3 py-2 text-xs shadow-lg"
      role="alert"
    >
      <span className="min-w-0 flex-1 break-words text-[var(--danger)]">{error}</span>
      <Button variant="icon" className="size-6" aria-label={t("close")} onClick={() => setCommandError(null)}>
        <X size={14} />
      </Button>
    </div>
  );
}
