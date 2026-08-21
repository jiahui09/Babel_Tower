# 状态所有权

本文是 Babel Tower 的权威状态分工说明。下面的结论基于当前代码与 Phase 1 事实，不把“已经存在的字段”误写成“已经接通的行为”。

## 先看结论

- Rust Core + SQLite 是翻译、修订、草稿、恢复、导出这些业务状态的权威源。
- React / Zustand 主要承载工作台会话和界面状态。
- TanStack Query 只做读缓存，不应被当成业务权威。
- 归档的 [state-ownership-findings.md](archive/phase1-analysis/state-ownership-findings.md) 记录了仍存在的双源问题，尤其是 `WorkspaceStateV1`、`panelWidths`、`TranslationStatus` 和部分设置项。

## 状态矩阵

| 状态                 | 权威拥有者                                  | 持久化位置                                                           | 谁会修改                                                               | 谁会读取                    | 当前备注                         |
| -------------------- | ------------------------------------------- | -------------------------------------------------------------------- | ---------------------------------------------------------------------- | --------------------------- | -------------------------------- |
| `Project` / 项目身份 | 桌面注册表 + 当前 `Kernel`                  | `app_data_dir/projects.json`                                         | `create_project`、`open_project`                                       | 库页、所有项目命令          | 正常                             |
| `WorkspaceStateV1`   | 目前是 Tauri + 前端双写                     | 项目目录 `.config/workspace-state.json` + `useWorkbenchStore` 持久化 | `read_workspace_state` / `write_workspace_state` + `AppShell` 定时写入 | `AppShell`、恢复流程        | 双源，且读写 DTO 需严格匹配      |
| `TranslationStatus`  | Rust Domain / Storage 投影                  | SQLite 与工作项查询投影                                              | Rust 保存路径                                                          | Units、Validation、部分页面 | 前端不应自行推导为权威           |
| `Revision`           | SQLite `translation_revision` / `unit_head` | SQLite                                                               | `save_translation`、`undo_translation`、`redo_translation`             | 编辑器、冲突检测、历史      | 权威                             |
| `Draft`              | SQLite `draft_session`                      | SQLite                                                               | `save_draft`、恢复逻辑                                                 | 编辑器、恢复                | 权威，但当前用户恢复 UI 仍不完整 |
| `Dirty`              | 工作台 store + 编辑器局部状态               | 浏览器内存 / 持久化工作台状态                                        | 编辑器更新、flusher 注册                                               | 关闭、保存指示器、标签点    | 由多个地方协同，不是单点权威     |
| `Settings`           | 桌面设置文件是主权威；Zustand 是镜像        | `app_config_dir/settings-v1.json` + `babel-tower-settings-v1`        | `patch_settings`、设置对话框                                           | 主题、语言、排版、面板宽度  | 桌面返回值会覆盖本地镜像         |
| `Tabs` / `Groups`    | 目前是工作台 store，并通过 IPC 另存一份     | 浏览器持久化 + `.config/workspace-state.json`                        | `openTab`、`closeTab`、`splitTab`、`replaceProjectSession`             | 标签栏、分屏                | 双写，且缺少冲突规则             |
| `Split` / 分屏比例   | `useWorkbenchStore`                         | 浏览器持久化                                                         | `setSplitRatio`、分屏面板 resize                                       | `AppShell`                  | 目前只在前端生效                 |
| `Query Cache`        | TanStack QueryClient                        | 内存                                                                 | query invalidation / refetch                                           | 路由、编辑器、面板          | 只能缓存，不是业务状态           |

## 重点说明

### TranslationStatus

`crates/babel-domain/src/workbench.rs` 定义了 `TranslationStatus`。前端 `apps/desktop/src/platform/desktop-bridge/types.ts` 也有同名类型，但它只是跨层 DTO 的枚举形状。新人写功能时，应把 Rust 状态当作唯一业务来源，前端展示只做投影。

### Revision

修订链的权威实现落在 SQLite：

- `crates/babel-storage/src/project.rs`
- `crates/babel-storage/src/schema.rs`

`save_translation_internal` 会写入 `translation_revision`、`unit_head`、`command_receipt` 和 `search_dirty`。`undo_translation` / `redo_translation` 不是“覆盖历史”，而是再写一条修订记录。

### Dirty

脏态不是单一字段。

- `TranslationEditor` / `WorkspaceFileEditor` 会在局部输入后标脏。
- `useWorkbenchStore` 里还有 `saveState` 与 `tabs[].dirty`。
- `DocumentTabs` 会在关闭脏标签时调用注册的 flusher。

这意味着“看起来脏”与“已经可安全关闭”不是同一件事。

### Settings

设置文件的权威路径是 `app_config_dir/settings-v1.json`，通过 `get_settings` / `patch_settings` 读写。`useSettingsStore` 只是前端镜像。

当前已确认的限制：

- `wordWrap` 目前只在设置界面里持久化，没有看到编辑器消费链。
- `shortcutOverrides` 写入 schema，但命令注册仍使用静态快捷键。
- `panelWidths` 同时出现在设置和工作台 store，双源未收敛。

### Tabs / Split

`apps/desktop/src/components/workbench/app-shell.tsx` 会把项目会话写入 `.config/workspace-state.json`，`useWorkbenchStore` 也会做浏览器持久化。当前没有明确的“谁赢”协议。新人不要把这块当成已经完成的恢复逻辑。

### Query Cache

TanStack Query 只适合：

- 项目快照
- work item
- tree / search / validation / export 列表

不要把它当成保存工作台会话的地方。

## 不能这样做

- 不要在第二个地方再维护 `TranslationStatus` 的业务规则。
- 不要把 `workspace-state.json` 当成工作台唯一权威，同时保留浏览器持久化而不写冲突规则。
- 不要让 UI 直接推导修订、脏态或恢复结果。

## 下一步阅读

- [09_WORKBENCH.md](09_WORKBENCH.md)
- [10_EDITOR.md](10_EDITOR.md)
- [13_SETTINGS.md](13_SETTINGS.md)
- [14_COMMANDS.md](14_COMMANDS.md)
- [06_IPC.md](06_IPC.md)
