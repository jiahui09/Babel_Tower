import { useQuery } from "@tanstack/react-query";
import { createFileRoute, useNavigate } from "@tanstack/react-router";
import { GitCompare } from "lucide-react";
import { useTranslation } from "react-i18next";

import { CodeMirrorView } from "../components/editor/code-mirror-view";
import { useDesktopBridge } from "../platform/desktop-bridge";
import { projectSnapshotQuery, workItemQuery } from "../queries/project";
import { useWorkbenchStore } from "../stores/workbench";
import { Button } from "../components/ui/button";

export const Route = createFileRoute("/projects/$projectId/source")({ component: SourcePage });

function SourcePage() {
  const { projectId } = Route.useParams();
  const { t } = useTranslation(["editor", "common"]);
  const bridge = useDesktopBridge();
  const activeTabId = useWorkbenchStore((state) => state.groups[0]?.activeTabId);
  const tab = useWorkbenchStore((state) => state.tabs.find((item) => item.id === activeTabId));
  const snapshot = useQuery(projectSnapshotQuery(bridge, projectId));
  const unitId =
    tab?.unitId ?? snapshot.data?.navigation?.position.unitId ?? snapshot.data?.currentUnit?.unitId ?? "";
  const item = useQuery({ ...workItemQuery(bridge, projectId, unitId), enabled: unitId.length > 0 });
  const openTab = useWorkbenchStore((state) => state.openTab);
  const navigate = useNavigate();
  if (snapshot.isPending || item.isPending) return <Centered text={t("loading", { ns: "common" })} />;
  if (snapshot.isError || item.isError)
    return <Centered text={(snapshot.error ?? item.error)?.message ?? ""} />;
  if (!unitId || !item.data) return <Centered text={t("noSource", { ns: "common" })} />;
  return (
    <div className="grid h-full min-h-0 grid-rows-[36px_1fr]">
      <div className="flex items-center justify-end border-b border-[var(--border)] px-2">
        <Button
          variant="ghost"
          onClick={() => {
            openTab({
              id: "diff",
              projectId,
              kind: "diff",
              title: t("diffView", { ns: "editor" }),
              unitId,
              dirty: false,
            });
            void navigate({ to: "/projects/$projectId/diff", params: { projectId } });
          }}
        >
          <GitCompare size={15} />
          {t("openDiff", { ns: "editor" })}
        </Button>
      </div>
      <CodeMirrorView value={item.data.sourceText} ariaLabel={t("source", { ns: "editor" })} />
    </div>
  );
}

function Centered({ text }: { text: string }) {
  return <div className="grid h-full place-items-center p-8 text-sm text-[var(--text-muted)]">{text}</div>;
}
