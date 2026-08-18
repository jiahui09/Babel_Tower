import { act, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it } from "vitest";

import { useWorkbenchStore } from "../../stores/workbench";
import { SaveIndicator } from "./save-indicator";

describe("SaveIndicator", () => {
  beforeEach(() => useWorkbenchStore.setState({ saveState: "saved" }));

  it("uses explicit Chinese text for durable state", () => {
    render(<SaveIndicator />);
    expect(screen.getByRole("status")).toHaveTextContent("已保存");
    act(() => useWorkbenchStore.setState({ saveState: "error" }));
    expect(screen.getByRole("status")).toHaveTextContent("保存失败");
  });
});
