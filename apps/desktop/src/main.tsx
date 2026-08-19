import React from "react";
import ReactDOM from "react-dom/client";
import { RouterProvider, createHashHistory, createRouter } from "@tanstack/react-router";

import { AppProviders } from "./app/providers";
import "./i18n";
import { TauriDesktopBridge } from "./platform/desktop-bridge";
import { createFixtureAppBridge } from "./test/fixture-app";
import { routeTree } from "./routeTree.gen";
import "./design/tokens.css";

const router = createRouter({
  routeTree,
  history: createHashHistory(),
  defaultPreload: "intent",
});

declare module "@tanstack/react-router" {
  interface Register {
    router: typeof router;
  }
}

const bridge =
  import.meta.env.VITE_DESKTOP_BRIDGE === "fixture"
    ? import.meta.env.DEV
      ? createFixtureAppBridge()
      : (() => {
          throw new Error("Fixture bridge is only available in development mode");
        })()
    : new TauriDesktopBridge();

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <AppProviders bridge={bridge}>
      <RouterProvider router={router} />
    </AppProviders>
  </React.StrictMode>,
);
