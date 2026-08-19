import { describe, expect, it } from "vitest";

import { commandRegistry, findShortcutConflicts } from "./registry";

describe("command registry", () => {
  it("has unique ids and default shortcuts", () => {
    expect(new Set(commandRegistry.map((command) => command.id)).size).toBe(commandRegistry.length);
    expect(findShortcutConflicts(commandRegistry)).toEqual([]);
  });

  it("disables project commands outside a project", () => {
    const exportCommand = commandRegistry.find((command) => command.id === "file.export");
    expect(
      exportCommand?.getAvailability({ projectId: null, activeUnitId: null } as never),
    ).toMatchObject({ enabled: false });
  });
});

