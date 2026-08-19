import { plainTextDocument } from "../platform/desktop-bridge";
import { createFixtureBridge } from "../platform/desktop-bridge/fixture-bridge";
import type { DesktopBridge, TranslationDocumentV1, TranslationWorkItem } from "../platform/desktop-bridge";

const projectId = "fixture-project";
const units = [
  {
    unitId: "00000000000000000000000000000001",
    sourceUnitKey: "1111111111111111111111111111111111111111111111111111111111111111",
    sourceText: "The tower keeps the original safe.",
    translation: "",
    localIndex: 0,
  },
  {
    unitId: "00000000000000000000000000000002",
    sourceUnitKey: "2222222222222222222222222222222222222222222222222222222222222222",
    sourceText: "Every translation is saved as a durable revision.",
    translation: "",
    localIndex: 1,
  },
];

export function createFixtureAppBridge(): DesktopBridge {
  const translations = new Map(units.map((unit) => [unit.unitId, unit.translation]));
  const settings = {
    schemaVersion: 1 as const,
    language: "zh-CN" as const,
    theme: "light" as const,
    density: "compact" as const,
    editorFontFamily: '"Noto Serif SC", serif',
    readingFontSize: 18,
    lineHeight: 1.8,
    wordWrap: true,
    shortcutOverrides: {},
    panelWidths: { explorer: 260, inspector: 320 },
  };
  const snapshot = () => ({
    schemaVersion: 1,
    project: { projectId, root: "/fixture/project", commitSequence: 1 },
    navigation: null,
    units: units.map((unit) => ({ ...unit, translation: translations.get(unit.unitId) || null })),
    currentUnit: null,
  });
  const workItem = (unitId: string): TranslationWorkItem => {
    const unit = units.find((candidate) => candidate.unitId === unitId) ?? units[0];
    const translationText = translations.get(unit.unitId) ?? "";
    return {
      unitId: unit.unitId,
      sourceUnitKey: unit.sourceUnitKey,
      sourceText: unit.sourceText,
      translation: plainTextDocument(translationText),
      translationText,
      status: translationText ? "translated" : "untranslated",
      revisionId: translationText ? 1 : null,
      projectCommitSequence: 1,
    };
  };
  return createFixtureBridge({
    bootstrap: async () => ({
      projects: [{ projectId, name: "Fixture Book", root: "/fixture/project", lastOpenedAtMs: Date.now() }],
      settings,
    }),
    openProject: async () => ({ projectId, root: "/fixture/project", commitSequence: 1 }),
    projectSnapshot: async () => snapshot(),
    projectTree: async () => ({
      commitSequence: 1,
      nodes: [
        ...(["source", "workspace", "derived"] as const).map((section) => ({
          id: `${section}-root`,
          parentId: null,
          section,
          kind: "root" as const,
          name: section,
          semanticPath: `.${section}`,
          capabilities: {
            open: false,
            createChild: section === "workspace",
            rename: false,
            move: false,
            delete: false,
            reveal: section !== "source",
            drop: section === "workspace",
          },
        })),
        ...units.map((unit) => ({
          id: unit.unitId,
          parentId: "source-root",
          section: "source" as const,
          kind: "text" as const,
          name: unit.sourceText,
          semanticPath: unit.sourceUnitKey,
          capabilities: {
            open: true,
            createChild: false,
            rename: false,
            move: false,
            delete: false,
            reveal: false,
            drop: false,
          },
        })),
      ],
    }),
    searchProject: async ({ query }) =>
      snapshot().units.filter((unit) =>
        `${unit.sourceText} ${unit.translation ?? ""}`.toLowerCase().includes(query.toLowerCase()),
      ),
    workItem: async (_project, unitId) => workItem(unitId),
    workbenchPage: async () => ({ items: snapshot().units, nextCursor: null, projectCommitSequence: 1 }),
    saveTranslationDocument: async (request) => {
      translations.set(request.unitId, textFromDocument(request.document));
      return { accepted: true, sequence: 1, commitSequence: 1, revisionId: "1" };
    },
    saveNavigation: async () => ({ accepted: true, sequence: 1 }),
    getSettings: async () => settings,
    patchSettings: async () => settings,
    validate: async () => ({ issues: [], checkedAtMs: Date.now(), projectCommitSequence: 1 }),
    termsForUnit: async () => [],
    annotationsForUnit: async () => [],
    listExports: async () => [],
    resourceQueue: async () => ({ items: [], nextCursor: null, projectCommitSequence: 1 }),
  });
}

function textFromDocument(document: TranslationDocumentV1) {
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
    .join("\n");
}
