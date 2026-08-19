import { create } from "zustand";
import { persist } from "zustand/middleware";

import type { AppLanguage, AppSettingsV1, AppTheme, InterfaceDensity } from "../platform/desktop-bridge";

interface SettingsState extends AppSettingsV1 {
  setLanguage(language: AppLanguage): void;
  setTheme(theme: AppTheme): void;
  setDensity(density: InterfaceDensity): void;
  setEditorFontFamily(editorFontFamily: string): void;
  setReadingFontSize(readingFontSize: number): void;
  setLineHeight(lineHeight: number): void;
  setWordWrap(wordWrap: boolean): void;
  replaceSettings(settings: AppSettingsV1): void;
}

const defaults: AppSettingsV1 = {
  schemaVersion: 1,
  language: "zh-CN",
  theme: "system",
  density: "compact",
  editorFontFamily: '"Noto Serif SC", "Source Han Serif SC", serif',
  readingFontSize: 18,
  lineHeight: 1.8,
  wordWrap: true,
  shortcutOverrides: {},
  panelWidths: { explorer: 260, inspector: 320 },
};

export const useSettingsStore = create<SettingsState>()(
  persist(
    (set) => ({
      ...defaults,
      setLanguage: (language) => set({ language }),
      setTheme: (theme) => set({ theme }),
      setDensity: (density) => set({ density }),
      setEditorFontFamily: (editorFontFamily) => set({ editorFontFamily }),
      setReadingFontSize: (readingFontSize) => set({ readingFontSize }),
      setLineHeight: (lineHeight) => set({ lineHeight }),
      setWordWrap: (wordWrap) => set({ wordWrap }),
      replaceSettings: (settings) => set(settings),
    }),
    { name: "babel-tower-settings-v1" },
  ),
);

