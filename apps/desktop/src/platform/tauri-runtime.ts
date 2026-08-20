import { BridgeError } from "./desktop-bridge/error";

type TauriInternals = {
  invoke?: unknown;
};

function tauriInternals(): TauriInternals | undefined {
  if (typeof window === "undefined") return undefined;
  return (window as Window & { __TAURI_INTERNALS__?: TauriInternals }).__TAURI_INTERNALS__;
}

export function isTauriRuntime(): boolean {
  return typeof tauriInternals()?.invoke === "function";
}

export function requireTauriRuntime(operation: string): void {
  if (isTauriRuntime()) return;
  throw new BridgeError(
    "ipc_unavailable",
    `Tauri IPC is unavailable for ${operation}. Start the desktop application with "pnpm dev"; "pnpm dev:web" is browser-only.`,
    { operation },
  );
}
