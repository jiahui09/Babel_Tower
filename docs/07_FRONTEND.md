# 07_FRONTEND

前端是 Babel Tower 的呈现层和会话层，不是业务权威层。它负责把项目状态、工作台状态和编辑输入组织成可操作的桌面界面。

## 入口链

```text
`apps/desktop/src/main.tsx`
→ `AppProviders`
→ `RouterProvider`
→ route module
→ workbench / editor / settings component
```

关键文件：

- `apps/desktop/src/main.tsx`
- `apps/desktop/src/app/providers.tsx`
- `apps/desktop/src/routes/`
- `apps/desktop/src/components/workbench/`
- `apps/desktop/src/platform/desktop-bridge/`
- `apps/desktop/src/queries/project.ts`
- `apps/desktop/src/stores/`

## 组件分层

| 层              | 代表文件                                     | 作用                               | 不负责什么               |
| --------------- | -------------------------------------------- | ---------------------------------- | ------------------------ |
| Routes          | `apps/desktop/src/routes/*.tsx`              | 页面入口、路由参数、页面级数据装配 | 业务持久化               |
| Components      | `apps/desktop/src/components/**`             | 具体界面、编辑器、面板、对话框     | 权威数据存储             |
| Hooks / helpers | `apps/desktop/src/platform/**`、`src/lib/**` | 桥、环境判断、文档转换、小工具     | 全局状态                 |
| Stores          | `apps/desktop/src/stores/*.ts`               | UI 会话状态和本地偏好              | SQLite / Kernel 权威状态 |
| Query           | `apps/desktop/src/queries/project.ts`        | 后端投影缓存                       | 修改权威数据             |

## React Query 和 Zustand 的分工

### React Query

放在 `apps/desktop/src/queries/project.ts` 的内容，通常是“从桥拿一份可缓存的后端投影”：

- `bootstrapQuery`
- `projectSnapshotQuery`
- `openProjectQuery`
- `projectTreeQuery`
- `projectSearchQuery`
- `workItemQuery`
- `validationQuery`

适合放进 Query 的状态是：

- 项目快照
- work item
- 资源树
- 验证结果
- 搜索结果

### Zustand

`apps/desktop/src/stores/` 里主要是 UI 会话状态：

- `useWorkbenchStore`
- `useWorkspaceStore`
- `useSettingsStore`

适合放进 Zustand 的状态是：

- 标签页、分组、分栏、面板开关
- 当前选中的 Explorer 节点
- 保存状态、脏态、命令面板、设置对话框
- 已输入但还没提交的本地偏好

不要把数据库里的修订、翻译正文、导出记录放进 Zustand 当权威。

## 关键状态

| 状态                                        | 位置                                 | 当前语义                     |
| ------------------------------------------- | ------------------------------------ | ---------------------------- |
| `tabs` / `groups` / `splitRatio`            | `useWorkbenchStore`                  | 工作台会话，浏览器侧持久化   |
| `expandedNodeIds` / `selectedNodeId`        | `useWorkspaceStore`                  | 当前项目的树展开与选中       |
| `language` / `theme` / `density` / 排版参数 | `useSettingsStore`                   | 用户设置镜像，启动时由桥填充 |
| `saveState` / `dirty`                       | `useWorkbenchStore` + 编辑器本地 ref | 编辑器保存态和脏态显示       |
| `projectSnapshot` / `workItem`              | React Query                          | 只读后端投影                 |

## 工作台主链路

`apps/desktop/src/components/workbench/app-shell.tsx` 负责：

- 打开项目
- 读取 `projectSnapshot`
- 从 `readWorkspaceState()` 恢复 tabs/groups/tree 状态
- 在 300ms debounce 后写回 `writeWorkspaceState()`
- 组装命令上下文

`apps/desktop/src/components/workbench/translation-editor.tsx` 负责：

- 把 `TranslationDocumentV1` 和 Tiptap 编辑器互转
- 管理本地 dirty 状态
- 注册 flush 函数
- 自动保存时区分 `saved`、`error`、`conflict`

`apps/desktop/src/components/workbench/document-tabs.tsx` 负责：

- 打开、激活、拖拽排序、关闭标签
- dirty 标签关闭前先走 flusher
- 但 `closeOtherTabs` / `closeTabsToRight` 目前不会先 flush，这是已知风险点

`apps/desktop/src/components/workbench/secondary-editor-group.tsx` 负责：

- 次分栏渲染
- source/diff/translation/workspaceFile 的镜像展示
- 当次分栏和主分栏是同一标签时，`WorkspaceFileEditor` 或 `TranslationEditor` 会进入只读模式

## 表单和编辑器

- 设置对话框在 `apps/desktop/src/components/settings/settings-dialog.tsx`
- 富文本编辑器在 `apps/desktop/src/components/workbench/translation-editor.tsx`
- 纯文本/Markdown 编辑器在 `apps/desktop/src/components/editor/code-mirror-view.tsx`

现在要记住的一点：

`wordWrap`、`shortcutOverrides` 和部分 `panelWidths` 字段已经进入设置 DTO，但不代表前端行为一定已经接上。看到 schema 不等于看到功能。

## 加载、错误、成功

- 加载态通常由 Query 的 `isPending` 驱动。
- 错误态通常直接展示 `error.message`，或者在 `AppShell` 里写进 command error。
- 成功态更多是“数据已刷新”或“保存态变成 `saved`”，而不是单独的 toast 系统。

## i18n

入口在 `apps/desktop/src/i18n/index.ts`，资源文件在：

- `apps/desktop/src/i18n/locales/zh-CN/*`
- `apps/desktop/src/i18n/locales/en-US/*`

设置对话框会把 `settings.language` 同步到 `document.documentElement.lang`，并驱动主题和排版 CSS 变量。

## 下一步阅读

1. `docs/03_PROJECT_STRUCTURE.md`
2. `docs/06_IPC.md`
3. `apps/desktop/src/components/workbench/app-shell.tsx`
4. `apps/desktop/src/components/workbench/document-tabs.tsx`
5. `apps/desktop/src/components/workbench/translation-editor.tsx`

下一步建议阅读 `docs/08_RUST_CORE.md`，因为前端的所有持久化最终都落到 Rust 核心和存储层。
