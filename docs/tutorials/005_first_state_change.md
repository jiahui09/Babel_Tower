# 005 First State Change

## Goal

修改一个已有 Zustand 状态，并加入测试。

推荐练习：给 `useWorkspaceStore.reveal()` 增加一个保护行为：当要 reveal 的节点不存在时，不改变当前选择。

## Prerequisites

- 已完成 [004 First UI Change](004_first_ui_change.md)
- 已读 [05_STATE_OWNERSHIP.md](../05_STATE_OWNERSHIP.md)
- 已读 [07_FRONTEND.md](../07_FRONTEND.md)
- 能运行 `pnpm test`

## Concept

不是所有状态都应该进 Rust。

`useWorkspaceStore` 管的是 Explorer 树的前端会话状态：

- 当前项目 ID
- 树节点
- 展开节点
- 选中节点
- loading / error

它不是文件系统权威，也不是项目数据权威。它只是把 `ProjectTreeNode[]` 投影成 UI 可用的展开和选中状态。

本练习选择 `reveal()`，因为它是纯前端状态行为，适合新人第一次写 store 测试。

## Steps

1. 读 store：

   ```bash
   sed -n '1,220p' apps/desktop/src/stores/workspace.ts
   ```

2. 读现有测试：

   ```bash
   sed -n '1,220p' apps/desktop/src/stores/workspace.test.ts
   ```

3. 理解现有行为：

   - `loadTree()` 会加载节点，并丢弃不存在的旧选择。
   - `reveal(nodeId)` 会选中节点，并展开祖先。
   - 当前代码没有先确认 `nodeId` 是否存在。

4. 写一个失败测试。测试目标：

   ```text
   当 reveal("missing") 时，selectedNodeId 不应从已有节点变成 missing。
   ```

5. 修改 `reveal()`，让它先检查节点是否存在。

6. 运行目标测试：

   ```bash
   pnpm --dir apps/desktop test -- src/stores/workspace.test.ts
   ```

7. 运行前端测试：

   ```bash
   pnpm test
   ```

## Files

| Path | 用途 |
| --- | --- |
| `apps/desktop/src/stores/workspace.ts` | Explorer 树 UI 状态 |
| `apps/desktop/src/stores/workspace.test.ts` | workspace store 回归测试 |
| `apps/desktop/src/platform/desktop-bridge/types.ts` | `ProjectTreeNode` 类型 |
| `docs/05_STATE_OWNERSHIP.md` | 判断状态归属 |
| `docs/09_WORKBENCH.md` | Workbench 和 Explorer 上下文 |

## Expected Result

- 新测试先能描述 bug，再由实现修复。
- `reveal()` 对真实节点保持原行为。
- `reveal()` 对不存在节点不制造无效选择。
- 目标测试通过。

## Common Errors

| Symptom | Cause | Fix |
| --- | --- | --- |
| 测试之间互相影响 | Zustand store 没 reset | 保持 `beforeEach(() => useWorkspaceStore.getState().reset())` |
| 直接改 `ProjectTreeNode` 类型 | 范围错误 | 本练习只改 UI store 行为 |
| 让 `reveal()` 抛异常 | UI 调用方可能没有错误处理 | 对不存在节点保持无操作更适合作为小练习 |
| 把 Explorer 节点当文件系统权威 | 混淆状态所有权 | 回到 [05_STATE_OWNERSHIP.md](../05_STATE_OWNERSHIP.md) |

## Acceptance

你完成本教程的标准：

- 新增至少一个 workspace store 测试。
- 测试覆盖不存在节点的 reveal 行为。
- 没有修改 Rust、IPC 或真实文件系统代码。
- 目标测试通过，或记录失败的真实输出。

## Reflection

写下你的复盘：

- 这个状态为什么属于 Zustand？
- 哪些状态只是从 tree 派生出来的？
- 如果真实文件系统节点被删除，哪一层负责告诉前端？
