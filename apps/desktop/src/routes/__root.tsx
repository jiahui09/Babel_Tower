import { createRootRoute, Outlet } from "@tanstack/react-router";

import { SettingsDialog } from "../components/settings/settings-dialog";

export const Route = createRootRoute({
  component: () => (
    <>
      <Outlet />
      <SettingsDialog />
    </>
  ),
});
