import i18n from "i18next";
import { initReactI18next } from "react-i18next";

import commonEn from "./locales/en-US/common.json";
import editorEn from "./locales/en-US/editor.json";
import errorsEn from "./locales/en-US/errors.json";
import explorerEn from "./locales/en-US/explorer.json";
import menuEn from "./locales/en-US/menu.json";
import settingsEn from "./locales/en-US/settings.json";
import workbenchEn from "./locales/en-US/workbench.json";
import commonZh from "./locales/zh-CN/common.json";
import editorZh from "./locales/zh-CN/editor.json";
import errorsZh from "./locales/zh-CN/errors.json";
import explorerZh from "./locales/zh-CN/explorer.json";
import menuZh from "./locales/zh-CN/menu.json";
import settingsZh from "./locales/zh-CN/settings.json";
import workbenchZh from "./locales/zh-CN/workbench.json";

export const defaultNamespace = "common";
export const resources = {
  "zh-CN": {
    common: commonZh,
    menu: menuZh,
    workbench: workbenchZh,
    explorer: explorerZh,
    editor: editorZh,
    settings: settingsZh,
    errors: errorsZh,
  },
  "en-US": {
    common: commonEn,
    menu: menuEn,
    workbench: workbenchEn,
    explorer: explorerEn,
    editor: editorEn,
    settings: settingsEn,
    errors: errorsEn,
  },
} as const;

void i18n.use(initReactI18next).init({
  resources,
  lng: "zh-CN",
  fallbackLng: "zh-CN",
  defaultNS: defaultNamespace,
  ns: ["common", "menu", "workbench", "explorer", "editor", "settings", "errors"],
  interpolation: { escapeValue: false },
  returnNull: false,
});

export { i18n };

