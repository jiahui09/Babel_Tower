import { Node, type JSONContent } from "@tiptap/core";
import { EditorContent, useEditor } from "@tiptap/react";
import StarterKit from "@tiptap/starter-kit";
import { Bold, Code2, Heading2, Italic, List, Quote, Redo2, Strikethrough, Undo2 } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
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
    return [
      "div",
      {
        ...HTMLAttributes,
        "data-translation-unit": HTMLAttributes.unitId,
        "data-block-kind": HTMLAttributes.blockKind,
        class: `translation-block translation-block--${String(HTMLAttributes.blockKind)}`,
      },
      0,
    ];
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
  readOnly?: boolean;
}

export function TranslationEditor({
  unitId,
  document,
  onPersist,
  onDirtyChange,
  registerFlush,
  readOnly = false,
}: TranslationEditorProps) {
  const { t } = useTranslation("editor");
  const setSaveState = useWorkbenchStore((state) => state.setSaveState);
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const composing = useRef(false);
  const dirty = useRef(false);
  const changeVersion = useRef(0);
  const latestDocument = useRef<TranslationDocumentV1>(document);
  const persistRef = useRef(onPersist);
  const [persistError, setPersistError] = useState<string | null>(null);
  useEffect(() => {
    persistRef.current = onPersist;
  }, [onPersist]);

  const editor = useEditor({
    extensions: [StarterKit, TranslationUnitNode, ProtectedTokenNode, PlaceholderNode],
    content: documentToTiptap(document, unitId),
    immediatelyRender: false,
    editable: !readOnly,
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
      changeVersion.current += 1;
      setSaveState("editing");
      onDirtyChange?.(true);
      if (!composing.current) schedulePersist(latestDocument.current, changeVersion.current);
    },
  });

  const persistSnapshot = useCallback(
    async (nextDocument: TranslationDocumentV1, version: number) => {
      setSaveState("saving");
      setPersistError(null);
      try {
        await persistRef.current(nextDocument);
        if (version !== changeVersion.current) return;
        dirty.current = false;
        setSaveState("saved");
        onDirtyChange?.(false);
      } catch (reason) {
        const message = reason instanceof Error ? reason.message : String(reason);
        setPersistError(message);
        setSaveState(message.toLowerCase().includes("revision") ? "conflict" : "error");
      }
    },
    [onDirtyChange, setSaveState],
  );

  const schedulePersist = (nextDocument: TranslationDocumentV1, version: number) => {
    clearTimeout(timer.current);
    timer.current = setTimeout(() => {
      void persistSnapshot(nextDocument, version);
    }, 650);
  };

  useEffect(() => {
    if (!registerFlush || !editor) return;
    return registerFlush(async () => {
      clearTimeout(timer.current);
      while (dirty.current) {
        const version = changeVersion.current;
        await persistSnapshot(latestDocument.current, version);
        if (version === changeVersion.current) return !dirty.current;
      }
      return true;
    });
  }, [editor, onDirtyChange, persistSnapshot, registerFlush, setSaveState]);

  useEffect(() => {
    if (!editor) return;
    if (dirty.current) return;
    const next = documentToTiptap(document, unitId);
    editor.commands.setContent(next, { emitUpdate: false });
    latestDocument.current = document;
  }, [document, editor, unitId]);

  useEffect(() => {
    const handleCompositionCommit = () => {
      if (!editor) return;
      latestDocument.current = tiptapToDocument(editor.getJSON());
      schedulePersist(latestDocument.current, changeVersion.current);
    };
    const element = editor?.view.dom;
    element?.addEventListener("babel-composition-commit", handleCompositionCommit);
    return () => {
      clearTimeout(timer.current);
      element?.removeEventListener("babel-composition-commit", handleCompositionCommit);
    };
  });

  const reloadFromCore = () => {
    editor?.commands.setContent(documentToTiptap(document, unitId), { emitUpdate: false });
    latestDocument.current = document;
    dirty.current = false;
    setPersistError(null);
    onDirtyChange?.(false);
    setSaveState("saved");
  };

  const copyDraft = async () => {
    try {
      await navigator.clipboard.writeText(projectDocument(latestDocument.current));
    } catch (reason) {
      setPersistError(reason instanceof Error ? reason.message : String(reason));
    }
  };

  const setBlockKind = (kind: TranslationBlock["kind"]) =>
    editor?.chain().focus().updateAttributes("translationUnit", { blockKind: kind }).run();

  if (!editor) return null;
  const blockKind = String(editor.getAttributes("translationUnit").blockKind ?? "paragraph");
  return (
    <div className="overflow-hidden rounded-[var(--radius)] border border-[var(--border)] bg-[var(--surface-raised)] focus-within:border-[var(--accent)]">
      {persistError && (
        <div className="flex items-center gap-2 border-b border-[var(--danger)] bg-[color-mix(in_srgb,var(--danger)_8%,var(--surface))] px-3 py-2 text-xs">
          <span className="min-w-0 flex-1 truncate text-[var(--danger)]">{persistError}</span>
          <Button
            variant="ghost"
            onClick={() => schedulePersist(latestDocument.current, changeVersion.current)}
          >
            {t("retry", { ns: "common" })}
          </Button>
          <Button variant="ghost" onClick={reloadFromCore}>
            {t("reload")}
          </Button>
          <Button variant="ghost" onClick={() => void copyDraft()}>
            {t("copyDraft")}
          </Button>
        </div>
      )}
      <div className="flex h-9 items-center gap-0.5 border-b border-[var(--border)] bg-[var(--surface-inset)] px-1.5">
        <FormatButton
          label={t("bold")}
          active={editor.isActive("bold")}
          onPress={() => editor.chain().focus().toggleBold().run()}
          icon={Bold}
          disabled={readOnly}
        />
        <FormatButton
          label={t("italic")}
          active={editor.isActive("italic")}
          onPress={() => editor.chain().focus().toggleItalic().run()}
          icon={Italic}
          disabled={readOnly}
        />
        <FormatButton
          label={t("strike")}
          active={editor.isActive("strike")}
          onPress={() => editor.chain().focus().toggleStrike().run()}
          icon={Strikethrough}
          disabled={readOnly}
        />
        <FormatButton
          label={t("code")}
          active={editor.isActive("code")}
          onPress={() => editor.chain().focus().toggleCode().run()}
          icon={Code2}
          disabled={readOnly}
        />
        <FormatButton
          label={t("heading")}
          active={blockKind === "heading"}
          onPress={() => setBlockKind("heading")}
          icon={Heading2}
          disabled={readOnly}
        />
        <FormatButton
          label={t("quote")}
          active={blockKind === "quote"}
          onPress={() => setBlockKind("quote")}
          icon={Quote}
          disabled={readOnly}
        />
        <FormatButton
          label={t("list")}
          active={blockKind === "listItem"}
          onPress={() => setBlockKind("listItem")}
          icon={List}
          disabled={readOnly}
        />
        <span className="mx-1 h-4 border-l border-[var(--border)]" aria-hidden="true" />
        <FormatButton
          label={t("undo")}
          active={false}
          onPress={() => editor.chain().focus().undo().run()}
          icon={Undo2}
          disabled={readOnly || !editor.can().undo()}
        />
        <FormatButton
          label={t("redo")}
          active={false}
          onPress={() => editor.chain().focus().redo().run()}
          icon={Redo2}
          disabled={readOnly || !editor.can().redo()}
        />
      </div>
      <EditorContent editor={editor} />
    </div>
  );
}

function projectDocument(document: TranslationDocumentV1) {
  return document.blocks
    .map((block) =>
      block.inlines
        .map((inline) =>
          inline.kind === "text"
            ? inline.text
            : inline.kind === "protected"
              ? inline.label
              : `{${inline.name}}`,
        )
        .join(""),
    )
    .join("\n");
}

function FormatButton({
  label,
  active,
  onPress,
  icon: Icon,
  disabled,
}: {
  label: string;
  active: boolean;
  onPress: () => void;
  icon: typeof Bold;
  disabled?: boolean;
}) {
  return (
    <Tooltip label={label}>
      <Button
        variant="icon"
        className={active ? "bg-[var(--selection)] text-[var(--selection-text)]" : undefined}
        aria-label={label}
        aria-pressed={active}
        onClick={onPress}
        disabled={disabled}
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
    text: inline.text,
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
