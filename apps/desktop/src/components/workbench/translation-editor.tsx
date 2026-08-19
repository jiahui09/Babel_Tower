import { Node, type JSONContent } from "@tiptap/core";
import { EditorContent, useEditor } from "@tiptap/react";
import StarterKit from "@tiptap/starter-kit";
import { Bold, Code2, Italic, Strikethrough } from "lucide-react";
import { useEffect, useRef } from "react";
import { useTranslation } from "react-i18next";

import type {
  TextMark,
  TranslationBlock,
  TranslationDocumentV1,
  TranslationInline,
} from "../../platform/desktop-bridge";
import { useWorkbenchStore } from "../../stores/workbench";
import { Button } from "../ui/button";
import { Tooltip } from "../ui/tooltip";

const TranslationUnitNode = Node.create({
  name: "translationUnit",
  group: "block",
  content: "inline*",
  defining: true,
  addAttributes() {
    return {
      unitId: { default: null },
      blockKind: { default: "paragraph" },
    };
  },
  parseHTML() {
    return [{ tag: "div[data-translation-unit]" }];
  },
  renderHTML({ HTMLAttributes }) {
    return ["div", { ...HTMLAttributes, "data-translation-unit": HTMLAttributes.unitId }, 0];
  },
});

const ProtectedTokenNode = Node.create({
  name: "protectedToken",
  group: "inline",
  inline: true,
  atom: true,
  selectable: false,
  addAttributes() {
    return { tokenId: {}, label: {}, signature: {} };
  },
  parseHTML() {
    return [{ tag: "span[data-protected-token]" }];
  },
  renderHTML({ HTMLAttributes }) {
    return [
      "span",
      {
        ...HTMLAttributes,
        "data-protected-token": HTMLAttributes.tokenId,
        contenteditable: "false",
        class: "translation-token",
      },
      HTMLAttributes.label,
    ];
  },
});

const PlaceholderNode = Node.create({
  name: "translationPlaceholder",
  group: "inline",
  inline: true,
  atom: true,
  selectable: false,
  addAttributes() {
    return { name: {}, rule: {} };
  },
  parseHTML() {
    return [{ tag: "span[data-placeholder]" }];
  },
  renderHTML({ HTMLAttributes }) {
    return [
      "span",
      {
        ...HTMLAttributes,
        "data-placeholder": HTMLAttributes.name,
        contenteditable: "false",
        class: "translation-token",
      },
      `{${String(HTMLAttributes.name)}}`,
    ];
  },
});

export interface TranslationEditorProps {
  unitId: string;
  document: TranslationDocumentV1;
  onPersist(document: TranslationDocumentV1): Promise<void>;
  onDirtyChange?(dirty: boolean): void;
  registerFlush?(flusher: () => Promise<boolean>): () => void;
}

export function TranslationEditor({
  unitId,
  document,
  onPersist,
  onDirtyChange,
  registerFlush,
}: TranslationEditorProps) {
  const { t } = useTranslation("editor");
  const setSaveState = useWorkbenchStore((state) => state.setSaveState);
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const composing = useRef(false);
  const dirty = useRef(false);
  const latestDocument = useRef<TranslationDocumentV1>(document);
  const persistRef = useRef(onPersist);
  useEffect(() => {
    persistRef.current = onPersist;
  }, [onPersist]);

  const editor = useEditor({
    extensions: [StarterKit, TranslationUnitNode, ProtectedTokenNode, PlaceholderNode],
    content: documentToTiptap(document, unitId),
    immediatelyRender: false,
    editorProps: {
      attributes: {
        class: "tiptap min-h-[260px] px-5 py-5 text-[var(--text)] focus:outline-none",
        "aria-label": t("translation"),
      },
      handleDOMEvents: {
        compositionstart: () => {
          composing.current = true;
          return false;
        },
        compositionend: (_view, event) => {
          composing.current = false;
          const current = event.currentTarget;
          if (current instanceof HTMLElement) current.dispatchEvent(new Event("babel-composition-commit"));
          return false;
        },
      },
    },
    onUpdate: ({ editor: currentEditor }) => {
      latestDocument.current = tiptapToDocument(currentEditor.getJSON());
      dirty.current = true;
      setSaveState("editing");
      onDirtyChange?.(true);
      if (!composing.current) schedulePersist(currentEditor.getJSON());
    },
  });

  const schedulePersist = (json: JSONContent) => {
    clearTimeout(timer.current);
    timer.current = setTimeout(() => {
      setSaveState("saving");
      void persistRef
        .current(tiptapToDocument(json))
        .then(() => {
          dirty.current = false;
          setSaveState("saved");
          onDirtyChange?.(false);
        })
        .catch((reason) => {
          const message = reason instanceof Error ? reason.message : String(reason);
          setSaveState(message.toLowerCase().includes("revision") ? "conflict" : "error");
        });
    }, 650);
  };

  useEffect(() => {
    if (!registerFlush || !editor) return;
    return registerFlush(async () => {
      if (!dirty.current) return true;
      clearTimeout(timer.current);
      try {
        await persistRef.current(latestDocument.current);
        dirty.current = false;
        onDirtyChange?.(false);
        setSaveState("saved");
        return true;
      } catch (reason) {
        setSaveState(
          reason instanceof Error && reason.message.toLowerCase().includes("revision") ? "conflict" : "error",
        );
        return false;
      }
    });
  }, [editor, onDirtyChange, registerFlush, setSaveState]);

  useEffect(() => {
    if (!editor) return;
    const next = documentToTiptap(document, unitId);
    editor.commands.setContent(next, { emitUpdate: false });
  }, [document, editor, unitId]);

  useEffect(() => {
    const handleCompositionCommit = () => {
      if (editor) schedulePersist(editor.getJSON());
    };
    const element = editor?.view.dom;
    element?.addEventListener("babel-composition-commit", handleCompositionCommit);
    return () => {
      clearTimeout(timer.current);
      element?.removeEventListener("babel-composition-commit", handleCompositionCommit);
    };
  });

  if (!editor) return null;
  return (
    <div className="overflow-hidden rounded-[var(--radius)] border border-[var(--border)] bg-[var(--surface-raised)] focus-within:border-[var(--accent)]">
      <div className="flex h-9 items-center gap-0.5 border-b border-[var(--border)] bg-[var(--surface-inset)] px-1.5">
        <FormatButton
          label={t("bold")}
          active={editor.isActive("bold")}
          onPress={() => editor.chain().focus().toggleBold().run()}
          icon={Bold}
        />
        <FormatButton
          label={t("italic")}
          active={editor.isActive("italic")}
          onPress={() => editor.chain().focus().toggleItalic().run()}
          icon={Italic}
        />
        <FormatButton
          label={t("strike")}
          active={editor.isActive("strike")}
          onPress={() => editor.chain().focus().toggleStrike().run()}
          icon={Strikethrough}
        />
        <FormatButton
          label={t("code")}
          active={editor.isActive("code")}
          onPress={() => editor.chain().focus().toggleCode().run()}
          icon={Code2}
        />
      </div>
      <EditorContent editor={editor} />
    </div>
  );
}

function FormatButton({
  label,
  active,
  onPress,
  icon: Icon,
}: {
  label: string;
  active: boolean;
  onPress: () => void;
  icon: typeof Bold;
}) {
  return (
    <Tooltip label={label}>
      <Button
        variant="icon"
        className={active ? "bg-[var(--selection)] text-[var(--selection-text)]" : undefined}
        aria-label={label}
        aria-pressed={active}
        onClick={onPress}
      >
        <Icon size={15} />
      </Button>
    </Tooltip>
  );
}

export function documentToTiptap(document: TranslationDocumentV1, unitId: string): JSONContent {
  return {
    type: "doc",
    content: document.blocks.map((block) => ({
      type: "translationUnit",
      attrs: { unitId, blockKind: block.kind },
      content: block.inlines.map(inlineToTiptap),
    })),
  };
}

function inlineToTiptap(inline: TranslationInline): JSONContent {
  if (inline.kind === "protected") {
    return { type: "protectedToken", attrs: inline };
  }
  if (inline.kind === "placeholder") {
    return { type: "translationPlaceholder", attrs: inline };
  }
  return {
    type: "text",
    text: inline.text || " ",
    marks: inline.marks.map((mark) => ({
      type: mark.kind,
      attrs: mark.kind === "link" ? { href: mark.href } : undefined,
    })),
  };
}

export function tiptapToDocument(json: JSONContent): TranslationDocumentV1 {
  const content = json.content ?? [];
  return {
    schemaVersion: 1,
    blocks: content.map((node) => ({
      kind: normalizeBlockKind(node.attrs?.blockKind),
      inlines: (node.content ?? []).map(tiptapToInline),
    })),
  };
}

function tiptapToInline(node: JSONContent): TranslationInline {
  if (node.type === "protectedToken") {
    return {
      kind: "protected",
      tokenId: String(node.attrs?.tokenId ?? ""),
      label: String(node.attrs?.label ?? ""),
      signature: String(node.attrs?.signature ?? ""),
    };
  }
  if (node.type === "translationPlaceholder") {
    return {
      kind: "placeholder",
      name: String(node.attrs?.name ?? ""),
      rule: String(node.attrs?.rule ?? ""),
    };
  }
  return {
    kind: "text",
    text: node.text ?? "",
    marks: (node.marks ?? []).flatMap(markFromTiptap),
  };
}

function markFromTiptap(mark: { type: string; attrs?: Record<string, unknown> }): TextMark[] {
  if (mark.type === "link") return [{ kind: "link", href: String(mark.attrs?.href ?? "") }];
  if (mark.type === "bold" || mark.type === "italic" || mark.type === "strike" || mark.type === "code") {
    return [{ kind: mark.type }];
  }
  return [];
}

function normalizeBlockKind(value: unknown): TranslationBlock["kind"] {
  if (value === "heading" || value === "quote" || value === "listItem" || value === "codeBlock") return value;
  return "paragraph";
}
