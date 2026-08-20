import { history, historyKeymap } from "@codemirror/commands";
import { markdown } from "@codemirror/lang-markdown";
import { defaultHighlightStyle, syntaxHighlighting } from "@codemirror/language";
import { EditorState } from "@codemirror/state";
import { EditorView, highlightActiveLine, keymap, lineNumbers } from "@codemirror/view";
import { useEffect, useRef } from "react";

export function CodeMirrorView({ value, readOnly = true, ariaLabel, onChange }: { value: string; readOnly?: boolean; ariaLabel: string; onChange?: (value: string) => void }) {
  const parent = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!parent.current) return;
    const view = new EditorView({
      parent: parent.current,
      state: EditorState.create({
        doc: value,
        extensions: [
          lineNumbers(),
          highlightActiveLine(),
          history(),
          keymap.of(historyKeymap),
          markdown(),
          syntaxHighlighting(defaultHighlightStyle, { fallback: true }),
          EditorState.readOnly.of(readOnly),
          EditorView.editable.of(!readOnly),
          EditorView.lineWrapping,
          EditorView.contentAttributes.of({ "aria-label": ariaLabel }),
          EditorView.updateListener.of((update) => {
            if (update.docChanged) onChange?.(update.state.doc.toString());
          }),
          EditorView.theme({
            "&": { height: "100%", backgroundColor: "var(--surface)", color: "var(--text)" },
            ".cm-scroller": { fontFamily: "var(--editor-font)", lineHeight: "1.65" },
            ".cm-gutters": { backgroundColor: "var(--surface-inset)", color: "var(--text-muted)", border: "none" },
            ".cm-activeLine, .cm-activeLineGutter": { backgroundColor: "var(--selection)" },
          }),
        ],
      }),
    });
    return () => view.destroy();
  }, [ariaLabel, onChange, readOnly, value]);
  return <div ref={parent} className="h-full min-h-0 overflow-hidden" />;
}
