# Workbench

Workbench 是 Babel Tower 的桌面工作台外壳，负责标签页、分屏、面板开关、命令入口和会话持久化。

## 你先记住这几个真实入口

- `apps/desktop/src/components/workbench/app-shell.tsx`
- `apps/desktop/src/components/workbench/document-tabs.tsx`
- `apps/desktop/src/components/workbench/secondary-editor-group.tsx`
- `apps/desktop/src/stores/workbench.ts`

## 它负责什么

- 打开、激活、关闭、拖动标签页
- 主区 / 次区分屏
- Explorer / Inspector / Problems 面板开关
- 保存状态指示
- 项目会话写回 `workspace-state.json`

## 它不负责什么

- 不负责翻译修订的权威存储
- 不负责项目树和工作区文件的真实内容
- 不负责业务级恢复决策

## 标签页模型

`useWorkbenchStore` 里的 `WorkbenchTab` 目前包含：

- `id`
- `projectId`
- `kind`
- `title`
- `unitId`
- `resourceId`
- `uri`
- `nodeId`
- `isReadonly`
- `pinned`
- `dirty`

分组模型是：

- `primary`
- `secondary`

每个分组维护：

- `tabIds`
- `activeTabId`

## 打开与激活

`openTab` 会把标签加入当前分组或指定分组，并把该分组的 `activeTabId` 指向它。`activateTab` 只改激活态，不会创建新标签。

主区路径标签在 `AppShell` 里由路由自动打开，`tabForPath()` 负责把路径映射成工作台标签。

## 关闭逻辑

单个关闭按钮会检查 `dirty`：

- 干净标签直接关闭
- 脏标签先调用对应 flusher

这条路径在 `apps/desktop/src/components/workbench/document-tabs.tsx`。

当前缺口：

- “关闭其他标签”
- “关闭右侧标签”

这两条上下文菜单路径会直接删标签，不走脏态 flusher。Phase 1 已把它记录成 `P0` 数据丢失风险。

## 分屏

`splitTab(tabId)` 会把同一个标签复制到 secondary 分组里，而不是创建第二份文档身份。

这意味着：

- 分屏是视图复制，不是新文档
- secondary 里 `WorkspaceFileEditor` / `TranslationEditor` 可能是只读镜像
- 主次两边共享同一条修订链

`apps/desktop/src/components/workbench/secondary-editor-group.tsx` 会在 secondary 里判断 `mirrorsPrimary`，并把 workspace file 设为只读镜像。

## 面板与布局

当前工作台保存的布局字段有：

- `explorerOpen`
- `inspectorOpen`
- `problemsOpen`
- `focusMode`
- `explorerPanel`
- `inspectorPanel`
- `selectedExplorerNodeId`
- `explorerWidth`
- `inspectorWidth`
- `splitRatio`
- `focusedGroupId`
- `groups`
- `tabs`

但 `panelWidths` 也存在于设置文件里，所以面板宽度现在不是单点权威。

## 持久化

`AppShell` 会在项目打开后：

1. 读取 `readWorkspaceState(projectId)`
2. 恢复前端 workspace store
3. 再由 `replaceProjectSession()` 替换工作台标签与分组
4. 通过 300ms debounce 写回 `writeWorkspaceState()`

当前没有看到明确的窗口关闭 flush，也没有冲突解决规则。

## 新人常问

### 我什么时候该改 WorkbenchStore？

当你改的是：

- 标签行为
- 分屏行为
- 面板开关
- 工作台保存状态

### 我什么时候不该改 WorkbenchStore？

当你改的是：

- 修订历史
- 真实文件内容
- 项目级状态权威

这些应该先查 Rust / IPC。

## 下一步阅读

- [05_STATE_OWNERSHIP.md](05_STATE_OWNERSHIP.md)
- [10_EDITOR.md](10_EDITOR.md)
- [06_IPC.md](06_IPC.md)
- `apps/desktop/src/components/workbench/app-shell.tsx`
