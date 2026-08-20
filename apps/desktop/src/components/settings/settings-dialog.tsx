import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";

import { useTheme } from "../../app/theme-provider";
import { useDesktopBridge } from "../../platform/desktop-bridge";
import type { AppSettingsV1, SettingsPatch } from "../../platform/desktop-bridge";
import { useSettingsStore } from "../../stores/settings";
import { useWorkbenchStore } from "../../stores/workbench";
import { Dialog, DialogContent, DialogDescription, DialogTitle } from "../ui/dialog";
import { Input } from "../ui/input";
import { Label } from "../ui/label";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "../ui/select";
import { Separator } from "../ui/separator";
import { Switch } from "../ui/switch";

export function SettingsDialog() {
  const { t, i18n } = useTranslation("settings");
  const bridge = useDesktopBridge();
  const open = useWorkbenchStore((state) => state.settingsOpen);
  const setOpen = useWorkbenchStore((state) => state.setSettingsOpen);
  const settings = useSettingsStore();
  const { setTheme } = useTheme();
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const persistedSettings = useRef<AppSettingsV1>(currentSettings(settings));

  useEffect(() => {
    void i18n.changeLanguage(settings.language);
    document.documentElement.lang = settings.language;
  }, [i18n, settings.language]);

  useEffect(() => setTheme(settings.theme), [setTheme, settings.theme]);

  useEffect(() => {
    document.documentElement.dataset.density = settings.density;
    document.documentElement.style.setProperty("--editor-font", settings.editorFontFamily);
    document.documentElement.style.setProperty("--editor-font-size", `${settings.readingFontSize}px`);
    document.documentElement.style.setProperty("--editor-line-height", String(settings.lineHeight));
  }, [settings.density, settings.editorFontFamily, settings.lineHeight, settings.readingFontSize]);

  const persist = async (previous: AppSettingsV1, patch: SettingsPatch) => {
    setError(null);
    setSaving(true);
    try {
      const saved = await bridge.patchSettings(patch);
      settings.replaceSettings(saved);
      persistedSettings.current = saved;
    } catch (reason) {
      const message = reason instanceof Error ? reason.message : String(reason);
      settings.replaceSettings(previous);
      persistedSettings.current = previous;
      setError(message);
    } finally {
      setSaving(false);
    }
  };

  const updateSetting = (patch: SettingsPatch, update: () => void) => {
    const previous = persistedSettings.current;
    update();
    void persist(previous, patch);
  };

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogContent>
        <DialogTitle>{t("title")}</DialogTitle>
        <DialogDescription>
          {t("language")} · {t("theme")} · {t("editorFont")}
        </DialogDescription>
        <div className="mt-5 grid grid-cols-[180px_1fr] items-center gap-x-5 gap-y-4">
          <Label>{t("language")}</Label>
          <Select
            value={settings.language}
            disabled={saving}
            onValueChange={(value: "zh-CN" | "en-US") =>
              updateSetting({ language: value }, () => settings.setLanguage(value))
            }
          >
            <SelectTrigger>
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="zh-CN">{t("languageZh")}</SelectItem>
              <SelectItem value="en-US">{t("languageEn")}</SelectItem>
            </SelectContent>
          </Select>
          <Label>{t("theme")}</Label>
          <Select
            value={settings.theme}
            disabled={saving}
            onValueChange={(value: "light" | "dark" | "system") =>
              updateSetting({ theme: value }, () => settings.setTheme(value))
            }
          >
            <SelectTrigger>
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="light">{t("themeLight")}</SelectItem>
              <SelectItem value="dark">{t("themeDark")}</SelectItem>
              <SelectItem value="system">{t("themeSystem")}</SelectItem>
            </SelectContent>
          </Select>
          <Label>{t("density")}</Label>
          <Select
            value={settings.density}
            disabled={saving}
            onValueChange={(value: "compact" | "comfortable") =>
              updateSetting({ density: value }, () => settings.setDensity(value))
            }
          >
            <SelectTrigger>
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="compact">{t("densityCompact")}</SelectItem>
              <SelectItem value="comfortable">{t("densityComfortable")}</SelectItem>
            </SelectContent>
          </Select>
        </div>
        <Separator className="my-5" />
        <div className="grid grid-cols-[180px_1fr] items-center gap-x-5 gap-y-4">
          <Label htmlFor="editor-font">{t("editorFont")}</Label>
          <Input
            id="editor-font"
            value={settings.editorFontFamily}
            onChange={(event) => settings.setEditorFontFamily(event.target.value)}
            onBlur={() =>
              void persist(persistedSettings.current, { editorFontFamily: settings.editorFontFamily })
            }
            disabled={saving}
          />
          <Label htmlFor="reading-size">{t("readingFontSize")}</Label>
          <Input
            id="reading-size"
            type="number"
            min={12}
            max={32}
            value={settings.readingFontSize}
            onChange={(event) => {
              const value = Number(event.target.value);
              if (Number.isFinite(value)) settings.setReadingFontSize(clamp(value, 12, 32));
            }}
            onBlur={() =>
              void persist(persistedSettings.current, { readingFontSize: settings.readingFontSize })
            }
            disabled={saving}
          />
          <Label htmlFor="line-height">{t("lineHeight")}</Label>
          <Input
            id="line-height"
            type="number"
            min={1.2}
            max={2.4}
            step={0.1}
            value={settings.lineHeight}
            onChange={(event) => {
              const value = Number(event.target.value);
              if (Number.isFinite(value)) settings.setLineHeight(clamp(value, 1.2, 2.4));
            }}
            onBlur={() => void persist(persistedSettings.current, { lineHeight: settings.lineHeight })}
            disabled={saving}
          />
          <Label htmlFor="word-wrap">{t("wordWrap")}</Label>
          <Switch
            id="word-wrap"
            checked={settings.wordWrap}
            disabled={saving}
            onCheckedChange={(checked) =>
              updateSetting({ wordWrap: checked }, () => settings.setWordWrap(checked))
            }
          />
        </div>
        {saving && (
          <p className="mb-0 mt-4 text-xs text-[var(--text-muted)]" role="status">
            {t("saving")}
          </p>
        )}
        {error && (
          <p className="mb-0 mt-4 text-xs text-[var(--danger)]" role="alert">
            {error}
          </p>
        )}
      </DialogContent>
    </Dialog>
  );
}

function clamp(value: number, min: number, max: number) {
  return Math.min(Math.max(value, min), max);
}

function currentSettings(settings: AppSettingsV1): AppSettingsV1 {
  return {
    schemaVersion: settings.schemaVersion,
    language: settings.language,
    theme: settings.theme,
    density: settings.density,
    editorFontFamily: settings.editorFontFamily,
    readingFontSize: settings.readingFontSize,
    lineHeight: settings.lineHeight,
    wordWrap: settings.wordWrap,
    shortcutOverrides: settings.shortcutOverrides,
    panelWidths: settings.panelWidths,
  };
}
