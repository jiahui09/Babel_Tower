import { MergeView } from "@codemirror/merge";
import { EditorState } from "@codemirror/state";
import { EditorView, lineNumbers } from "@codemirror/view";
import { useEffect, useRef } from "react";

export function DiffView({ before, after }: { before: string; after: string }) {
  const parent = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!parent.current) return;
    const extensions = [
      lineNumbers(),
      EditorState.readOnly.of(true),
      EditorView.editable.of(false),
      EditorView.lineWrapping,
      EditorView.theme({
        "&": { height: "100%", backgroundColor: "var(--surface)", color: "var(--text)" },
        ".cm-scroller": { fontFamily: "var(--editor-font)", lineHeight: "1.65" },
        ".cm-gutters": { backgroundColor: "var(--surface-inset)", color: "var(--text-muted)", border: "none" },
      }),
    ];
    const view = new MergeView({
      parent: parent.current,
      a: { doc: before, extensions },
      b: { doc: after, extensions },
      highlightChanges: true,
      gutter: true,
      collapseUnchanged: { margin: 3, minSize: 4 },
    });
    return () => view.destroy();
  }, [after, before]);
  return <div ref={parent} className="h-full min-h-0 overflow-hidden" />;
}

