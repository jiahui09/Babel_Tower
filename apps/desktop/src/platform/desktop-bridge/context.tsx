import { createContext, useContext, type PropsWithChildren } from "react";

import type { DesktopBridge } from "./types";

const DesktopBridgeContext = createContext<DesktopBridge | null>(null);

export function DesktopBridgeProvider({
  bridge,
  children,
}: PropsWithChildren<{ bridge: DesktopBridge }>) {
  return <DesktopBridgeContext.Provider value={bridge}>{children}</DesktopBridgeContext.Provider>;
}

export function useDesktopBridge(): DesktopBridge {
  const bridge = useContext(DesktopBridgeContext);
  if (!bridge) throw new Error("DesktopBridgeProvider is missing");
  return bridge;
}

