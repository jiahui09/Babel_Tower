import { describe, expect, it } from "vitest";

import { BridgeError } from "./error";
import { createFixtureBridge } from "./fixture-bridge";

describe("fixture bridge", () => {
  it("requires each test to opt into implemented behavior", async () => {
    const bridge = createFixtureBridge({ bootstrap: async () => ({ projects: [], settings: {} as never }) });
    await expect(bridge.bootstrap()).resolves.toMatchObject({ projects: [] });
    await expect(bridge.projectSnapshot("missing")).rejects.toEqual(
      expect.objectContaining<Partial<BridgeError>>({ code: "not_implemented" }),
    );
  });
});

