import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { TooltipProvider } from "@radix-ui/react-tooltip";
import type { PropsWithChildren } from "react";
import { useEffect } from "react";
import { I18nextProvider } from "react-i18next";

import { i18n } from "../i18n";
import { DesktopBridgeProvider, type DesktopBridge } from "../platform/desktop-bridge";
import { AppErrorBoundary } from "./error-boundary";
import { ThemeProvider } from "./theme-provider";
import { useSettingsStore } from "../stores/settings";
import { BridgeError } from "../platform/desktop-bridge";

const queryClient = new QueryClient({
  defaultOptions: {
    queries: { retry: 1, staleTime: 5_000, refetchOnWindowFocus: false },
    mutations: { retry: 0 },
  },
});

export function AppProviders({ bridge, children }: PropsWithChildren<{ bridge: DesktopBridge }>) {
  return (
    <AppErrorBoundary>
      <I18nextProvider i18n={i18n}>
        <DesktopBridgeProvider bridge={bridge}>
          <QueryClientProvider client={queryClient}>
            <ThemeProvider>
              <SettingsHydrator bridge={bridge} />
              <TooltipProvider delayDuration={450}>{children}</TooltipProvider>
            </ThemeProvider>
          </QueryClientProvider>
        </DesktopBridgeProvider>
      </I18nextProvider>
    </AppErrorBoundary>
  );
}

function SettingsHydrator({ bridge }: { bridge: DesktopBridge }) {
  const replaceSettings = useSettingsStore((state) => state.replaceSettings);
  useEffect(() => {
    let active = true;
    void bridge
      .getSettings()
      .then((settings) => {
        if (active) replaceSettings(settings);
      })
      .catch((reason) => {
        if (!(reason instanceof BridgeError) || reason.code !== "not_implemented") {
          console.error("Failed to load application settings", reason);
        }
      });
    return () => {
      active = false;
    };
  }, [bridge, replaceSettings]);
  return null;
}
