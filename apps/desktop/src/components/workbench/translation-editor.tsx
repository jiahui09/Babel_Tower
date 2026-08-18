import { EditorContent, useEditor } from "@tiptap/react";
import StarterKit from "@tiptap/starter-kit";
import { useEffect, useRef } from "react";

import { useWorkbenchStore } from "../../stores/workbench";

export function TranslationEditor({
  initialText = "",
  onPersist,
}: {
  initialText?: string;
  onPersist?: (text: string) => Promise<void>;
}) {
  const setSaveState = useWorkbenchStore((state) => state.setSaveState);
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const editor = useEditor({
    extensions: [StarterKit],
    content: initialText,
    immediatelyRender: false,
    editorProps: {
      attributes: {
        class:
          "min-h-[280px] px-1 py-2 font-serif text-[18px] leading-[1.8] text-[var(--text)] focus:outline-none",
        "aria-label": "译文编辑器",
      },
    },
    onUpdate: ({ editor: currentEditor }) => {
      setSaveState("editing");
      clearTimeout(timer.current);
      timer.current = setTimeout(() => {
        setSaveState("saving");
        void (onPersist?.(currentEditor.getText()) ?? Promise.resolve())
          .then(() => setSaveState("saved"))
          .catch(() => setSaveState("error"));
      }, 500);
    },
  });

  useEffect(() => () => clearTimeout(timer.current), []);

  return <EditorContent editor={editor} />;
}
