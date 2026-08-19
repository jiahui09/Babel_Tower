import type { TranslationDocumentV1 } from "./types";

export function plainTextDocument(text: string): TranslationDocumentV1 {
  const paragraphs = text.split(/\n{2,}/);
  return {
    schemaVersion: 1,
    blocks: (paragraphs.length > 0 ? paragraphs : [""]).map((paragraph) => ({
      kind: "paragraph" as const,
      inlines: [{ kind: "text" as const, text: paragraph, marks: [] }],
    })),
  };
}

export function projectDocumentText(document: TranslationDocumentV1): string {
  return document.blocks
    .map((block) =>
      block.inlines
        .map((inline) => {
          if (inline.kind === "text") return inline.text;
          if (inline.kind === "protected") return inline.label;
          return inline.name;
        })
        .join(""),
    )
    .join("\n\n");
}

