import { createFileRoute } from "@tanstack/react-router";
import { useVirtualizer } from "@tanstack/react-virtual";
import { useEffect, useRef, useState } from "react";

import { getWorkbenchSnapshot, type UnitSummary } from "../lib/ipc";

export const Route = createFileRoute("/projects/$projectId/units")({ component: UnitsPage });

const previewRows = Array.from({ length: 42 }, (_, index) => ({
  id: index + 1,
  source:
    index === 17
      ? "The harbor lights were already fading when she unfolded the last letter."
      : `Source unit ${index + 1}`,
  translation: index === 17 ? "她展开最后一封信时，港口的灯火已经渐渐暗去。" : "",
}));

function UnitsPage() {
  const [rows, setRows] = useState<UnitSummary[] | typeof previewRows>(previewRows);
  useEffect(() => {
    let active = true;
    void getWorkbenchSnapshot("Units")
      .then((snapshot) => {
        if (active && snapshot.units.length > 0) setRows(snapshot.units);
      })
      .catch(() => undefined);
    return () => {
      active = false;
    };
  }, []);
  const parentRef = useRef<HTMLDivElement>(null);
  const virtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => 58,
    overscan: 8,
  });
  return (
    <div className="grid h-full grid-rows-[44px_1fr]">
      <div className="grid grid-cols-[72px_1fr_1fr] items-center border-b border-[var(--border)] bg-[var(--surface-raised)] px-4 text-xs font-semibold text-[var(--text-secondary)]">
        <span>状态</span>
        <span>原文</span>
        <span>译文</span>
      </div>
      <div ref={parentRef} className="overflow-auto">
        <div style={{ height: virtualizer.getTotalSize(), position: "relative" }}>
          {virtualizer.getVirtualItems().map((item) => {
            const row = rows[item.index];
            return (
              <div
                key={"unitId" in row ? row.unitId : row.id}
                className="absolute left-0 top-0 grid w-full grid-cols-[72px_1fr_1fr] border-b border-[var(--border)] px-4 text-sm"
                style={{ height: item.size, transform: `translateY(${item.start}px)` }}
              >
                <span className="flex items-center text-xs text-[var(--text-muted)]">
                  {row.translation ? "草稿" : "未翻译"}
                </span>
                <span className="flex items-center border-x border-[var(--border)] px-3 text-[var(--text-secondary)]">
                  {"sourceText" in row ? row.sourceText : row.source}
                </span>
                <span className="flex items-center px-3">{row.translation || ""}</span>
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
}
