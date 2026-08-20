import { useQuery } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import { useTranslation } from "react-i18next";

import { DiffView } from "../components/editor/diff-view";
import { useDesktopBridge } from "../platform/desktop-bridge";
import { projectDocumentText } from "../platform/desktop-bridge/document";
import { projectSnapshotQuery, workItemQuery } from "../queries/project";
import { useWorkbenchStore } from "../stores/workbench";

export const Route = createFileRoute("/projects/$projectId/diff")({ component: DiffPage });

function DiffPage() {
  const { projectId } = Route.useParams();
  const { t } = useTranslation("common");
  const bridge = useDesktopBridge();
  const activeTabId = useWorkbenchStore((state) => state.groups[0]?.activeTabId);
  const tab = useWorkbenchStore((state) => state.tabs.find((item) => item.id === activeTabId));
  const snapshot = useQuery(projectSnapshotQuery(bridge, projectId));
  const unitId =
    tab?.unitId ?? snapshot.data?.navigation?.position.unitId ?? snapshot.data?.currentUnit?.unitId ?? "";
  const item = useQuery({ ...workItemQuery(bridge, projectId, unitId), enabled: unitId.length > 0 });
  if (snapshot.isPending || item.isPending)
    return (
      <div className="grid h-full place-items-center text-sm text-[var(--text-muted)]">{t("loading")}</div>
    );
  if (snapshot.isError || item.isError)
    return (
      <div className="grid h-full place-items-center text-sm text-[var(--danger)]">
        {(snapshot.error ?? item.error)?.message}
      </div>
    );
  if (!unitId || !item.data)
    return (
      <div className="grid h-full place-items-center text-sm text-[var(--text-muted)]">{t("noSource")}</div>
    );
  return <DiffView before={item.data.sourceText} after={projectDocumentText(item.data.translation)} />;
}
