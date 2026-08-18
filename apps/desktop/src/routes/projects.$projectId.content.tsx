import { createFileRoute } from "@tanstack/react-router";
import { BookMarked, MessageSquareText } from "lucide-react";
import { useEffect, useState } from "react";

import { TranslationEditor } from "../components/workbench/translation-editor";
import { getWorkbenchSnapshot, saveTranslation, type UnitSummary } from "../lib/ipc";

export const Route = createFileRoute("/projects/$projectId/content")({ component: LongFormPage });

function LongFormPage() {
  const [unit, setUnit] = useState<UnitSummary | null>(null);

  useEffect(() => {
    let active = true;
    void getWorkbenchSnapshot()
      .then((snapshot) => {
        if (active) setUnit(snapshot.units[0] ?? null);
      })
      .catch(() => {
        // Browser preview remains usable when Tauri IPC is unavailable.
      });
    return () => {
      active = false;
    };
  }, []);

  const sourceText =
    unit?.sourceText ?? "The harbor lights were already fading when she unfolded the last letter.";
  const translation = unit?.translation ?? "她展开最后一封信时，港口的灯火已经渐渐暗去。";

  return (
    <div className="h-full overflow-auto">
      <div className="mx-auto max-w-[920px] px-10 py-8">
        <div className="mb-6 flex items-center gap-2 text-xs text-[var(--text-muted)]">
          <BookMarked size={14} />
          第一章 港口 · 第 {unit ? unit.localIndex + 1 : 18} 单元
        </div>
        <section aria-labelledby="source-heading" className="border-b border-[var(--border)] pb-6">
          <h1 id="source-heading" className="m-0 mb-3 text-sm font-semibold text-[var(--text-secondary)]">
            原文
          </h1>
          <p className="m-0 font-serif text-[17px] leading-8 text-[var(--text-secondary)]">{sourceText}</p>
        </section>
        <section aria-labelledby="translation-heading" className="pt-6">
          <div className="flex items-center justify-between">
            <h2 id="translation-heading" className="m-0 text-sm font-semibold">
              译文
            </h2>
            <button className="flex h-8 items-center gap-1.5 rounded-[6px] px-2 text-xs text-[var(--text-secondary)] hover:bg-[var(--surface-inset)]">
              <MessageSquareText size={15} />
              批注
            </button>
          </div>
          <TranslationEditor
            initialText={translation}
            onPersist={
              unit ? (text) => saveTranslation(unit.sourceUnitKey, text).then(() => undefined) : undefined
            }
          />
        </section>
        <footer className="mt-10 flex justify-between border-t border-[var(--border)] pt-4 text-xs text-[var(--text-muted)]">
          <span>上一段</span>
          <span>18 / 42</span>
          <span>下一段</span>
        </footer>
      </div>
    </div>
  );
}
