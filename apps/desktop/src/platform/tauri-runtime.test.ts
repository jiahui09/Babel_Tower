import { afterEach, describe, expect, it } from "vitest";

import { BridgeError } from "./desktop-bridge/error";
import { TauriDesktopBridge } from "./desktop-bridge/tauri-bridge";
import { isTauriRuntime, requireTauriRuntime } from "./tauri-runtime";

const initialInternals = Object.getOwnPropertyDescriptor(window, "__TAURI_INTERNALS__");

afterEach(() => {
  if (initialInternals) {
    Object.defineProperty(window, "__TAURI_INTERNALS__", initialInternals);
  } else {
    Reflect.deleteProperty(window, "__TAURI_INTERNALS__");
  }
});

describe("Tauri runtime boundary", () => {
  it("returns a typed bridge error outside the Tauri webview", () => {
    Reflect.deleteProperty(window, "__TAURI_INTERNALS__");

    expect(isTauriRuntime()).toBe(false);
    expect(() => requireTauriRuntime("list_projects")).toThrow(
      expect.objectContaining<Partial<BridgeError>>({ code: "ipc_unavailable" }),
    );
  });

  it("does not call the Tauri API when the host is absent", async () => {
    Reflect.deleteProperty(window, "__TAURI_INTERNALS__");

    await expect(new TauriDesktopBridge().bootstrap()).rejects.toEqual(
      expect.objectContaining<Partial<BridgeError>>({ code: "ipc_unavailable" }),
    );
  });

  it("accepts the Tauri webview invoke capability", () => {
    Object.defineProperty(window, "__TAURI_INTERNALS__", {
      configurable: true,
      value: { invoke: () => undefined },
    });

    expect(isTauriRuntime()).toBe(true);
    expect(() => requireTauriRuntime("list_projects")).not.toThrow();
  });
});
