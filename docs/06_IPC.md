# 06_IPC

这一层回答的是：前端怎么把动作送到 Rust，Rust 怎么回数据，哪些地方最容易出错。

## 调用链

```text
React 组件
→ `useDesktopBridge()`
→ `TauriDesktopBridge`
→ `invoke(command, args)`
→ `#[tauri::command]` in `apps/desktop/src-tauri/src/lib.rs`
→ `babel-application::Kernel`
→ `babel-storage` / filesystem / worker
```

## 主要入口

| 层         | 文件                                                       | 作用                                            |
| ---------- | ---------------------------------------------------------- | ----------------------------------------------- |
| 前端契约   | `apps/desktop/src/platform/desktop-bridge/types.ts`        | 定义前端看到的请求/响应 DTO                     |
| 前端实现   | `apps/desktop/src/platform/desktop-bridge/tauri-bridge.ts` | 把 TS 方法映射到 Tauri command 名称             |
| 运行时检查 | `apps/desktop/src/platform/tauri-runtime.ts`               | 在浏览器环境下阻止误用 Tauri IPC                |
| Rust 命令  | `apps/desktop/src-tauri/src/lib.rs`                        | 接收 DTO、校验项目会话、读写文件、调用 `Kernel` |
| 应用层     | `crates/babel-application/src/lib.rs`                      | 真正的业务行为和一致性规则                      |

## 重要命令

| Command                                                 | 前端方法                                                        | Rust 入口                               | 用途                         |
| ------------------------------------------------------- | --------------------------------------------------------------- | --------------------------------------- | ---------------------------- |
| `list_projects`                                         | `bootstrap()`                                                   | `list_projects`                         | 启动时读项目注册表           |
| `create_project`                                        | `createProject()`                                               | `create_project`                        | 新建项目                     |
| `open_project`                                          | `openProject(root)`                                             | `open_project`                          | 打开/切换项目                |
| `import_file`                                           | `importFile()`                                                  | `import_file`                           | 导入 TXT / Markdown / EPUB   |
| `project_tree`                                          | `projectTree()`                                                 | `project_tree`                          | 资源树和工作区树             |
| `workbench_snapshot`                                    | `projectSnapshot()` / `workbenchPage()`                         | `workbench_snapshot`                    | 项目快照、导航、当前单元     |
| `work_item`                                             | `workItem()`                                                    | `work_item`                             | 打开翻译编辑器时取当前工作项 |
| `save_translation`                                      | `saveTranslationDocument()`                                     | `save_translation`                      | 持久化修订                   |
| `save_draft`                                            | `saveDraft()`                                                   | `save_draft`                            | 保存草稿                     |
| `undo_translation` / `redo_translation`                 | `undo()` / `redo()`                                             | `undo_translation` / `redo_translation` | 撤销/重做                    |
| `save_navigation`                                       | `saveNavigation()`                                              | `save_navigation`                       | 保存导航位置                 |
| `validate_project`                                      | `validate()`                                                    | `validate_project`                      | 验证当前项目                 |
| `create_export` / `list_exports`                        | `createExport()` / `listExports()`                              | `create_export` / `list_exports`        | 导出与导出记录               |
| `get_settings` / `patch_settings`                       | `getSettings()` / `patchSettings()`                             | `get_settings` / `patch_settings`       | 全局设置                     |
| `mutate_workspace`                                      | `mutateWorkspace()`                                             | `mutate_workspace`                      | 工作区文件/文件夹操作        |
| `resource_queue` / `image_preview` / `ocr_image_region` | `resourceQueue()` / `imagePreview()` / `recognizeImageRegion()` | 同名 command                            | 图片资源、预览、OCR          |

## 什么时候该新增 IPC

适合新增 IPC 的情况：

- 前端需要一个 Rust 才有的权威数据源。
- 需要读写项目文件、SQLite、CAS 或 worker。
- 需要把一次用户操作记录成可恢复、可验证的持久化动作。

不适合新增 IPC 的情况：

- 只是把已有查询结果重新筛一遍。
- 只是界面状态切换、面板展开、选中行变化。
- 只是本地派生值，前端能从现有 store/query 算出来。

## 已知 DTO 问题

### `read_workspace_state` 的请求类型错了

前端发送的是：

```ts
bridge.readWorkspaceState(projectId);
// → { request: { projectId } }
```

但 Rust 端声明的是：

```rust
fn read_workspace_state(request: WorkspaceFileRequest, ...)
```

`WorkspaceFileRequest` 需要 `node_id`，而这个命令实际只检查 `project_id`。结果是：

- 前端正常调用时，反序列化会缺字段；
- 这个命令无法按当前桥契约稳定工作；
- `AppShell` 的 workspace/tab 恢复会落到错误分支。

修这个问题时，先对齐 DTO，再改调用点，最后补一个序列化测试。

### `projectSnapshot(projectId)` 的参数没有真正参与 Rust 请求

`TauriDesktopBridge.projectSnapshot(projectId)` 现在只调用 `workbench_snapshot`，Rust 端依赖当前 active kernel。这个参数在契约层并不构成独立的项目选择权，要按“当前打开项目”理解，不要当成多项目隔离 API。

## 命令层的边界

- `TauriDesktopBridge` 负责命令名、DTO 映射和错误归一化。
- `apps/desktop/src-tauri/src/lib.rs` 负责项目会话、文件系统、设置文件、工作区状态文件。
- `Kernel` 不知道 React、路由或 Zustand。

下一步建议阅读 `docs/07_FRONTEND.md`，因为很多 IPC 问题会先表现成 UI 状态或缓存问题。
