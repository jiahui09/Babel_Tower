import { createFileRoute } from "@tanstack/react-router";

import { AppShell } from "../components/workbench/app-shell";

export const Route = createFileRoute("/projects/$projectId")({
  component: ProjectWorkbench,
});

function ProjectWorkbench() {
  const { projectId } = Route.useParams();
  return <AppShell projectId={projectId} />;
}
