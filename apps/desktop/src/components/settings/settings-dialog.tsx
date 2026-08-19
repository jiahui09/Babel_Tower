import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";

import { useTheme } from "../../app/theme-provider";
import { useDesktopBridge } from "../../platform/desktop-bridge";
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

  const persist = (patch: Parameters<typeof bridge.patchSettings>[0]) => {
    setError(null);
    void bridge.patchSettings(patch).catch((reason) => {
      const message = reason instanceof Error ? reason.message : String(reason);
      if (!message.includes("not available") && !message.includes("not implemented")) setError(message);
    });
  };

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogContent>
        <DialogTitle>{t("title")}</DialogTitle>
        <DialogDescription>{t("language")} · {t("theme")} · {t("editorFont")}</DialogDescription>
        <div className="mt-5 grid grid-cols-[180px_1fr] items-center gap-x-5 gap-y-4">
          <Label>{t("language")}</Label>
          <Select
            value={settings.language}
            onValueChange={(value: "zh-CN" | "en-US") => {
              settings.setLanguage(value);
              persist({ language: value });
            }}
          >
            <SelectTrigger><SelectValue /></SelectTrigger>
            <SelectContent>
              <SelectItem value="zh-CN">简体中文</SelectItem>
              <SelectItem value="en-US">English</SelectItem>
            </SelectContent>
          </Select>
          <Label>{t("theme")}</Label>
          <Select
            value={settings.theme}
            onValueChange={(value: "light" | "dark" | "system") => {
              settings.setTheme(value);
              persist({ theme: value });
            }}
          >
            <SelectTrigger><SelectValue /></SelectTrigger>
            <SelectContent>
              <SelectItem value="light">{t("themeLight")}</SelectItem>
              <SelectItem value="dark">{t("themeDark")}</SelectItem>
              <SelectItem value="system">{t("themeSystem")}</SelectItem>
            </SelectContent>
          </Select>
          <Label>{t("density")}</Label>
          <Select
            value={settings.density}
            onValueChange={(value: "compact" | "comfortable") => {
              settings.setDensity(value);
              persist({ density: value });
            }}
          >
            <SelectTrigger><SelectValue /></SelectTrigger>
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
            onBlur={() => persist({ editorFontFamily: settings.editorFontFamily })}
          />
          <Label htmlFor="reading-size">{t("readingFontSize")}</Label>
          <Input
            id="reading-size"
            type="number"
            min={12}
            max={32}
            value={settings.readingFontSize}
            onChange={(event) => settings.setReadingFontSize(Number(event.target.value))}
            onBlur={() => persist({ readingFontSize: settings.readingFontSize })}
          />
          <Label htmlFor="line-height">{t("lineHeight")}</Label>
          <Input
            id="line-height"
            type="number"
            min={1.2}
            max={2.4}
            step={0.1}
            value={settings.lineHeight}
            onChange={(event) => settings.setLineHeight(Number(event.target.value))}
            onBlur={() => persist({ lineHeight: settings.lineHeight })}
          />
          <Label htmlFor="word-wrap">{t("wordWrap")}</Label>
          <Switch
            id="word-wrap"
            checked={settings.wordWrap}
            onCheckedChange={(checked) => {
              settings.setWordWrap(checked);
              persist({ wordWrap: checked });
            }}
          />
        </div>
        {error && <p className="mb-0 mt-4 text-xs text-[var(--danger)]">{error}</p>}
      </DialogContent>
    </Dialog>
  );
}

