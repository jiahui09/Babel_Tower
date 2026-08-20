import { LoaderCircle, Save } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";

import { CodeMirrorView } from "../editor/code-mirror-view";
import { useDesktopBridge } from "../../platform/desktop-bridge";
import { useWorkbenchStore, type WorkbenchTab } from "../../stores/workbench";
import { Button } from "../ui/button";

export function WorkspaceFileEditor({ tab, readOnly = false }: { tab: WorkbenchTab; readOnly?: boolean }) {
  const { t } = useTranslation("editor");
  const bridge = useDesktopBridge();
  const markTabDirty = useWorkbenchStore((state) => state.markTabDirty);
  const registerTabFlusher = useWorkbenchStore((state) => state.registerTabFlusher);
  const [content, setContent] = useState<string | null>(null);
  const [modifiedAtMs, setModifiedAtMs] = useState<number>();
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const contentRef = useRef("");
  const modifiedRef = useRef<number | undefined>(undefined);

  useEffect(() => {
    if (!tab.nodeId) return;
    let active = true;
    void bridge
      .readWorkspaceFile({ projectId: tab.projectId, nodeId: tab.nodeId })
      .then((file) => {
        if (!active) return;
        contentRef.current = file.content;
        modifiedRef.current = file.modifiedAtMs;
        setContent(file.content);
        setModifiedAtMs(file.modifiedAtMs);
        setError(null);
      })
      .catch((reason) => active && setError(reason instanceof Error ? reason.message : String(reason)));
    return () => {
      active = false;
    };
  }, [bridge, tab.nodeId, tab.projectId]);

  const save = useCallback(async () => {
    if (!tab.nodeId || readOnly) return true;
    setSaving(true);
    setError(null);
    try {
      const file = await bridge.writeWorkspaceFile({
        projectId: tab.projectId,
        nodeId: tab.nodeId,
        content: contentRef.current,
        expectedModifiedAtMs: modifiedRef.current,
      });
      modifiedRef.current = file.modifiedAtMs;
      setModifiedAtMs(file.modifiedAtMs);
      markTabDirty(tab.id, false);
      return true;
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
      return false;
    } finally {
      setSaving(false);
    }
  }, [bridge, markTabDirty, readOnly, tab.id, tab.nodeId, tab.projectId]);

  useEffect(() => registerTabFlusher(tab.id, save), [registerTabFlusher, save, tab.id]);

  if (error && content === null) return <div className="p-5 text-sm text-[var(--danger)]">{error}</div>;
  if (content === null)
    return (
      <div className="grid h-full place-items-center text-sm text-[var(--text-muted)]">
        <LoaderCircle className="animate-spin" size={18} />
      </div>
    );

  return (
    <div className="grid h-full min-h-0 grid-rows-[36px_1fr]">
      <div className="flex items-center gap-2 border-b border-[var(--border)] bg-[var(--surface-inset)] px-2 text-xs text-[var(--text-muted)]">
        <span className="min-w-0 flex-1 truncate">{tab.uri}</span>
        {error && <span className="truncate text-[var(--danger)]">{error}</span>}
        <Button
          variant="icon"
          disabled={readOnly || saving}
          onClick={() => void save()}
          aria-label={t("saveFile")}
          title={t("saveFile")}
        >
          {saving ? <LoaderCircle className="animate-spin" size={14} /> : <Save size={14} />}
        </Button>
      </div>
      <CodeMirrorView
        key={modifiedAtMs}
        value={content}
        readOnly={readOnly}
        ariaLabel={tab.title}
        onChange={(value) => {
          contentRef.current = value;
          markTabDirty(tab.id, true);
        }}
      />
    </div>
  );
}
