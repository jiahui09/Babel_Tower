import { createFileRoute } from "@tanstack/react-router";
import { ChevronLeft, ChevronRight, ImageOff, ScanText, WandSparkles } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";

import { TranslationEditor } from "../components/workbench/translation-editor";
import { Button } from "../components/ui/button";
import { Textarea } from "../components/ui/textarea";
import {
  plainTextDocument,
  projectDocumentText,
  useDesktopBridge,
  type OcrDocument,
  type ResourceQueueItem,
} from "../platform/desktop-bridge";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { workItemQuery } from "../queries/project";
import { useWorkbenchStore } from "../stores/workbench";

export const Route = createFileRoute("/projects/$projectId/resources")({ component: ResourcesPage });

function ResourcesPage() {
  const { projectId } = Route.useParams();
  const { t } = useTranslation(["workbench", "common"]);
  const bridge = useDesktopBridge();
  const queryClient = useQueryClient();
  const registerTabFlusher = useWorkbenchStore((state) => state.registerTabFlusher);
  const markTabDirty = useWorkbenchStore((state) => state.markTabDirty);
  const [items, setItems] = useState<ResourceQueueItem[]>([]);
  const [index, setIndex] = useState(0);
  const [error, setError] = useState<string | null>(null);
  const [imagePreview, setImagePreview] = useState<{ unitId: string; url: string } | null>(null);
  const [ocrDocument, setOcrDocument] = useState<{ unitId: string; document: OcrDocument } | null>(null);
  const [ocrState, setOcrState] = useState<"idle" | "running" | "error">("idle");
  const [renderedPreview, setRenderedPreview] = useState<{ unitId: string; url: string } | null>(null);
  const [renderState, setRenderState] = useState<"idle" | "running" | "error">("idle");
  const [translationDraft, setTranslationDraft] = useState<{ unitId: string; text: string } | null>(null);

  useEffect(() => {
    let active = true;
    void bridge
      .resourceQueue()
      .then((page) => {
        if (active) setItems(page.items);
      })
      .catch((reason) => {
        if (active) setError(reason instanceof Error ? reason.message : String(reason));
      });
    return () => {
      active = false;
    };
  }, [bridge]);

  const item = items[index];
  const workItem = useQuery({
    ...workItemQuery(bridge, projectId, item?.unitId ?? ""),
    enabled: item !== undefined,
  });
  useEffect(() => {
    let active = true;
    if (!item) return;
    if (!item.imageResourceId) return;
    void bridge
      .imagePreview({ generationId: item.generationId, resourceId: item.imageResourceId })
      .then((preview) => {
        if (active) setImagePreview({ unitId: item.unitId, url: preview.dataUrl });
      })
      .catch((reason) => {
        if (active) setError(reason instanceof Error ? reason.message : String(reason));
      });
    return () => {
      active = false;
    };
  }, [bridge, item]);
  if (!item) {
    return (
      <div className="grid h-full place-items-center bg-[var(--surface-inset)] p-8">
        <div className="max-w-[460px] text-center">
          <ImageOff size={28} className="mx-auto text-[var(--text-muted)]" />
          <h1 className="mb-2 mt-4 text-base font-semibold">
            {error ? t("workbench:resourceQueueUnavailable") : t("workbench:noImageRegions")}
          </h1>
          <p className="m-0 text-sm leading-6 text-[var(--text-secondary)]">
            {error ?? t("workbench:noImageRegionsDetail")}
          </p>
        </div>
      </div>
    );
  }

  const region = bounds(item.polygon);
  const imageUrl = imagePreview?.unitId === item.unitId ? imagePreview.url : null;
  const renderedUrl = renderedPreview?.unitId === item.unitId ? renderedPreview.url : null;
  const translationText =
    translationDraft?.unitId === item.unitId
      ? translationDraft.text
      : (workItem.data?.translationText ?? item.translation ?? "");
  const recognizedText =
    ocrDocument?.unitId === item.unitId
      ? ocrDocument.document.pages
          .flatMap((page) => page.regions)
          .map((region) => region.text)
          .join("\n")
      : null;
  const runOcr = async () => {
    setOcrState("running");
    setError(null);
    try {
      if (!item.imageResourceId) throw new Error(t("workbench:imageObjectUnavailable"));
      const result = await bridge.recognizeImageRegion({
        generationId: item.generationId,
        regionId: item.regionId,
        imageResourceId: item.imageResourceId,
      });
      setOcrDocument({ unitId: item.unitId, document: result.document });
      setOcrState("idle");
    } catch (reason) {
      setOcrState("error");
      setError(reason instanceof Error ? reason.message : String(reason));
    }
  };
  const runRender = async () => {
    if (!translationText.trim()) {
      setError(t("workbench:translationRequired"));
      return;
    }
    setRenderState("running");
    setError(null);
    try {
      if (!item.imageResourceId) throw new Error(t("workbench:imageObjectUnavailable"));
      const result = await bridge.renderImageRegion({
        generationId: item.generationId,
        unitId: item.unitId,
        regionId: item.regionId,
        imageResourceId: item.imageResourceId,
        polygon: item.polygon,
        translation: translationText,
      });
      setRenderedPreview({ unitId: item.unitId, url: result.dataUrl });
      setRenderState("idle");
    } catch (reason) {
      setRenderState("error");
      setError(reason instanceof Error ? reason.message : String(reason));
    }
  };
  return (
    <div className="grid h-full min-h-0 grid-rows-[52px_1fr] bg-[var(--surface-inset)]">
      <header className="flex items-center justify-between border-b border-[var(--border)] bg-[var(--surface-raised)] px-5">
        <div className="flex min-w-0 items-center gap-2">
          <ScanText size={16} className="shrink-0 text-[var(--accent)]" />
          <span className="truncate text-sm font-semibold">
            {t("workbench:imageTextTitle")} · {item.regionSemanticPath}
          </span>
        </div>
        <div className="flex items-center gap-1">
          <Button
            variant="secondary"
            onClick={() => void runOcr()}
            disabled={ocrState === "running"}
            aria-label={t("workbench:recognizeCurrent")}
            title={t("workbench:recognizeCurrent")}
          >
            <ScanText size={15} />
            {ocrState === "running" ? t("workbench:recognizing") : t("workbench:recognizeAgain")}
          </Button>
          <Button
            variant="icon"
            disabled={index === 0}
            onClick={() => setIndex((current) => Math.max(0, current - 1))}
            aria-label={t("workbench:previousRegion")}
            title={t("workbench:previousRegion")}
          >
            <ChevronLeft size={17} />
          </Button>
          <span className="min-w-[70px] text-center text-xs text-[var(--text-muted)]">
            {index + 1} / {items.length}
          </span>
          <Button
            variant="icon"
            disabled={index === items.length - 1}
            onClick={() => setIndex((current) => Math.min(items.length - 1, current + 1))}
            aria-label={t("workbench:nextRegion")}
            title={t("workbench:nextRegion")}
          >
            <ChevronRight size={17} />
          </Button>
        </div>
      </header>

      <div className="grid min-h-0 grid-cols-[minmax(0,1fr)_minmax(320px,420px)]">
        <section className="flex min-h-0 flex-col p-6">
          <div className="relative min-h-[360px] flex-1 overflow-hidden border border-[var(--border)] bg-[#1c2024]">
            {renderedUrl || imageUrl ? (
              <img
                src={renderedUrl ?? imageUrl ?? undefined}
                alt={item.imageSemanticPath ?? t("workbench:imageResource")}
                className="h-full w-full object-contain"
              />
            ) : null}
            <div
              className="absolute border-2 border-[var(--accent)] bg-[var(--accent)]/10"
              style={{
                left: `${region.left}%`,
                top: `${region.top}%`,
                width: `${region.width}%`,
                height: `${region.height}%`,
              }}
              aria-label={t("workbench:currentRegion")}
            />
            <div className="absolute inset-x-0 bottom-0 border-t border-white/10 bg-black/35 px-3 py-2 text-xs text-white/75">
              {renderedUrl
                ? t("workbench:derivedPreviewShown")
                : imageUrl
                  ? t("workbench:sourceObjectVerified")
                  : t("workbench:loadingImageObject")}
            </div>
          </div>
          <div className="mt-3 flex items-center justify-between text-xs text-[var(--text-muted)]">
            <span>
              {t("workbench:sourceImage")}: {item.imageSemanticPath ?? t("workbench:unlinked")}
            </span>
            <div className="flex items-center gap-3">
              <span>{item.coordinateSpace}</span>
              {renderedUrl ? (
                <Button
                  variant="ghost"
                  className="h-auto px-0 py-0 text-[var(--accent)] hover:bg-transparent"
                  onClick={() => setRenderedPreview(null)}
                >
                  {t("workbench:viewOriginal")}
                </Button>
              ) : null}
            </div>
          </div>
        </section>

        <aside className="min-h-0 overflow-auto border-l border-[var(--border)] bg-[var(--surface-raised)] p-5">
          <p className="m-0 text-xs font-semibold text-[var(--text-secondary)]">
            {t("workbench:recognitionResult")}
          </p>
          <div className="mt-2 border border-[var(--border)] bg-[var(--surface)] p-3 text-sm leading-6 text-[var(--text-secondary)]">
            {recognizedText || item.sourceText || t("workbench:emptyText")}
          </div>
          <SourceCorrection
            key={item.unitId}
            item={item}
            registerFlush={(flusher) => registerTabFlusher("resources", flusher)}
          />
          <p className="mb-0 mt-6 text-xs font-semibold text-[var(--text-secondary)]">
            {t("workbench:manualTranslation")}
          </p>
          <TranslationEditor
            key={item.unitId}
            unitId={item.unitId}
            document={plainTextDocument(translationText)}
            onPersist={(document) => {
              const text = projectDocumentText(document);
              setTranslationDraft({ unitId: item.unitId, text });
              return bridge
                .saveTranslationDocument({
                  projectId,
                  unitId: item.unitId,
                  sourceUnitKey: item.sourceUnitKey,
                  commandId: crypto.randomUUID().replace(/-/g, ""),
                  expectedRevisionId: workItem.data?.revisionId ?? null,
                  document,
                  createdAtMs: Date.now(),
                })
                .then(async () => {
                  await Promise.all([
                    queryClient.invalidateQueries({ queryKey: ["project", projectId, "snapshot"] }),
                    queryClient.invalidateQueries({
                      queryKey: ["project", projectId, "work-item", item.unitId],
                    }),
                  ]);
                });
            }}
            onDirtyChange={(dirty) => markTabDirty("resources", dirty)}
            registerFlush={(flusher) => registerTabFlusher("resources", flusher)}
          />
          <div className="mt-3 border-t border-[var(--border)] pt-4">
            <div className="flex items-center justify-between">
              <p className="m-0 text-xs font-semibold text-[var(--text-secondary)]">
                {t("workbench:typesetPreview")}
              </p>
              <Button
                variant="secondary"
                onClick={() => void runRender()}
                disabled={renderState === "running" || !translationText.trim()}
                aria-label={t("workbench:generatePreview")}
                title={t("workbench:generatePreview")}
              >
                <WandSparkles size={15} />
                {renderState === "running" ? t("workbench:generating") : t("workbench:generatePreview")}
              </Button>
            </div>
            <p className="mb-0 mt-2 text-xs leading-5 text-[var(--text-muted)]">
              {t("workbench:derivedPreviewDetail")}
            </p>
          </div>
          {error && item ? <p className="mb-0 mt-3 text-xs leading-5 text-[var(--danger)]">{error}</p> : null}
          <p className="mt-4 text-xs leading-5 text-[var(--text-muted)]">
            {t("workbench:resourceWorkflowNote")}
          </p>
        </aside>
      </div>
    </div>
  );
}

function SourceCorrection({
  item,
  registerFlush,
}: {
  item: ResourceQueueItem;
  registerFlush: (flusher: () => Promise<boolean>) => () => void;
}) {
  const bridge = useDesktopBridge();
  const { t } = useTranslation(["workbench", "common"]);
  const [text, setText] = useState(item.correctedSourceText ?? item.sourceText);
  const [state, setState] = useState<"idle" | "saving" | "saved" | "error">("idle");
  const [error, setError] = useState<string | null>(null);
  const markTabDirty = useWorkbenchStore((store) => store.markTabDirty);

  const persist = useCallback(async (): Promise<boolean> => {
    if (text === (item.correctedSourceText ?? item.sourceText)) {
      markTabDirty("resources", false);
      return true;
    }
    setState("saving");
    setError(null);
    try {
      await bridge.saveImageRegionEdit({
        generationId: item.generationId,
        unitId: item.unitId,
        regionId: item.regionId,
        correctedSourceText: text,
      });
      setState("saved");
      markTabDirty("resources", false);
      return true;
    } catch (reason) {
      setState("error");
      setError(reason instanceof Error ? reason.message : String(reason));
      return false;
    }
  }, [bridge, item, markTabDirty, text]);

  useEffect(() => registerFlush(persist), [persist, registerFlush]);

  return (
    <div className="mt-4">
      <label className="text-xs font-semibold text-[var(--text-secondary)]" htmlFor="corrected-source">
        {t("workbench:correctedSource")}
      </label>
      <Textarea
        id="corrected-source"
        value={text}
        onChange={(event) => {
          setText(event.target.value);
          setState("idle");
          markTabDirty("resources", true);
        }}
        className="mt-2"
        aria-label={t("workbench:correctedSource")}
      />
      <div className="mt-2 flex items-center justify-between">
        <span className="text-xs text-[var(--text-muted)]">
          {state === "saved"
            ? t("common:saved")
            : state === "error"
              ? t("common:saveFailed")
              : t("workbench:ocrOnly")}
        </span>
        <Button variant="secondary" onClick={() => void persist()} disabled={state === "saving"}>
          {state === "saving" ? t("common:saving") : t("workbench:saveCorrection")}
        </Button>
      </div>
      {error && (
        <p className="mb-0 mt-2 text-xs text-[var(--danger)]" role="alert">
          {error}
        </p>
      )}
    </div>
  );
}

function bounds(polygon: [number, number][]) {
  const xs = polygon.map(([x]) => x);
  const ys = polygon.map(([, y]) => y);
  const left = Math.min(...xs, 0);
  const top = Math.min(...ys, 0);
  const right = Math.max(...xs, 1);
  const bottom = Math.max(...ys, 1);
  const scale = Math.max(right, bottom, 1);
  return {
    left: (left / scale) * 100,
    top: (top / scale) * 100,
    width: ((right - left) / scale) * 100,
    height: ((bottom - top) / scale) * 100,
  };
}
