import { queryOptions } from "@tanstack/react-query";

import type { DesktopBridge } from "../platform/desktop-bridge";

export const bootstrapQuery = (bridge: DesktopBridge) =>
  queryOptions({ queryKey: ["bootstrap"], queryFn: () => bridge.bootstrap() });

export const projectSnapshotQuery = (bridge: DesktopBridge, projectId: string) =>
  queryOptions({
    queryKey: ["project", projectId, "snapshot"],
    queryFn: () => bridge.projectSnapshot(projectId),
  });

export const openProjectQuery = (
  bridge: DesktopBridge,
  project: { projectId: string; root: string } | undefined,
) =>
  queryOptions({
    queryKey: ["project", project?.projectId, "open"],
    queryFn: () => bridge.openProject(project!.root),
    enabled: Boolean(project),
    staleTime: Infinity,
  });

export const projectTreeQuery = (bridge: DesktopBridge, projectId: string) =>
  queryOptions({
    queryKey: ["project", projectId, "tree"],
    queryFn: () => bridge.projectTree({ projectId }),
  });

export const projectSearchQuery = (bridge: DesktopBridge, projectId: string, query: string) =>
  queryOptions({
    queryKey: ["project", projectId, "search", query],
    queryFn: () => bridge.searchProject({ projectId, query, limit: 50 }),
    enabled: query.trim().length >= 2,
  });

export const workItemQuery = (bridge: DesktopBridge, projectId: string, unitId: string) =>
  queryOptions({
    queryKey: ["project", projectId, "work-item", unitId],
    queryFn: () => bridge.workItem(projectId, unitId),
  });

export const validationQuery = (bridge: DesktopBridge, projectId: string) =>
  queryOptions({
    queryKey: ["validation", projectId],
    queryFn: () => bridge.validate(projectId),
  });
