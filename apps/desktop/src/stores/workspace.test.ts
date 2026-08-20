import { beforeEach, describe, expect, it } from "vitest";

import type { ProjectTreeNode } from "../platform/desktop-bridge";
import { useWorkspaceStore } from "./workspace";

const nodes: ProjectTreeNode[] = [
  {
    id: "workspace-root",
    parentId: null,
    section: "workspace",
    kind: "root",
    name: "workspace",
    semanticPath: ".",
    capabilities: {
      open: false,
      createChild: true,
      rename: false,
      move: false,
      delete: false,
      reveal: true,
      drop: true,
    },
  },
  {
    id: "workspace/drafts",
    parentId: "workspace-root",
    section: "workspace",
    kind: "folder",
    name: "drafts",
    semanticPath: "drafts",
    capabilities: {
      open: false,
      createChild: true,
      rename: true,
      move: true,
      delete: true,
      reveal: true,
      drop: true,
    },
  },
  {
    id: "workspace/drafts/a.md",
    parentId: "workspace/drafts",
    section: "workspace",
    kind: "resource",
    name: "a.md",
    semanticPath: "drafts/a.md",
    capabilities: {
      open: true,
      createChild: false,
      rename: true,
      move: true,
      delete: true,
      reveal: true,
      drop: false,
    },
  },
];

describe("workspace store", () => {
  beforeEach(() => useWorkspaceStore.getState().reset());

  it("reveals a node and expands its ancestors", () => {
    useWorkspaceStore.getState().loadTree("project", nodes);
    useWorkspaceStore.getState().reveal("workspace/drafts/a.md");
    expect(useWorkspaceStore.getState().selectedNodeId).toBe("workspace/drafts/a.md");
    expect(useWorkspaceStore.getState().expandedNodeIds).toEqual(["workspace-root", "workspace/drafts"]);
  });

  it("drops stale selection when the filesystem changes", () => {
    useWorkspaceStore.getState().loadTree("project", nodes);
    useWorkspaceStore.getState().setSelected("workspace/drafts/a.md");
    useWorkspaceStore.getState().loadTree("project", nodes.slice(0, 2));
    expect(useWorkspaceStore.getState().selectedNodeId).toBeNull();
  });
});
