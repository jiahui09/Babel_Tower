import { describe, expect, it } from "vitest";

import { validateImportRequest } from "./import";

describe("validateImportRequest", () => {
  it("requires a project storage directory before a selected source can be imported", () => {
    expect(validateImportRequest("", "/books/source.epub")).toBe("missingProjectDirectory");
  });

  it("requires a source file after a project storage directory is selected", () => {
    expect(validateImportRequest("/projects/book", "")).toBe("missingSourceFile");
  });

  it("accepts a complete import request", () => {
    expect(validateImportRequest("/projects/book", "/books/source.epub")).toBeNull();
  });
});
