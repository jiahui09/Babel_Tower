import { describe, expect, it } from "vitest";

import { plainTextDocument, projectDocumentText } from "./document";

describe("translation document projection", () => {
  it("round trips legacy paragraph text without storing editor JSON", () => {
    const document = plainTextDocument("first paragraph\n\nsecond paragraph");
    expect(document.schemaVersion).toBe(1);
    expect(projectDocumentText(document)).toBe("first paragraph\n\nsecond paragraph");
  });

  it("projects protected and placeholder tokens into searchable text", () => {
    expect(
      projectDocumentText({
        schemaVersion: 1,
        blocks: [
          {
            kind: "paragraph",
            inlines: [
              { kind: "protected", tokenId: "p1", label: "<em>", signature: "sig" },
              { kind: "text", text: "name: ", marks: [] },
              { kind: "placeholder", name: "player", rule: "required" },
            ],
          },
        ],
      }),
    ).toBe("<em>name: player");
  });
});

