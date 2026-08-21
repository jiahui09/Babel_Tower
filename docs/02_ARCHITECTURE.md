# 系统架构

## 一句话模型

Babel Tower 将桌面交互、业务用例、格式处理和持久化分开：React 显示和收集意图，DesktopBridge 传递类型化请求，Tauri 适配桌面能力，Rust `Kernel` 执行业务规则，SQLite/CAS/worker 保存和处理事实。

```text
React UI（routes / components）
  -> React Query / Zustand
  -> DesktopBridge
  -> Tauri command
  -> babel_application::Kernel
  -> Storage / CAS / adapters / workers
  -> filesystem and SQLite
```

## 各层负责什么

| 层              | 负责                                                          | 不负责                       | 主要入口                                          |
| --------------- | ------------------------------------------------------------- | ---------------------------- | ------------------------------------------------- |
| Frontend        | 页面、交互、局部输入、展示 loading/error                      | 权威翻译、revision、导出算法 | `apps/desktop/src/main.tsx`、`routes/`            |
| Query / Zustand | IPC 结果缓存、标签页/面板/临时 UI 状态                        | 项目持久化事实               | `queries/project.ts`、`stores/`                   |
| DesktopBridge   | 前端 DTO、Tauri 调用和错误标准化                              | 业务规则、文件直接读写       | `platform/desktop-bridge/`                        |
| Tauri           | 命令注册、单活动 Kernel、系统对话框、应用设置、受控工作区文件 | 格式解析和 SQLite 领域规则   | `src-tauri/src/lib.rs`                            |
| Application     | 导入、绑定、译文、校验、导出、资源队列、OCR 用例协调          | React 视图/布局              | `crates/babel-application/src/lib.rs`             |
| Storage/workers | SQLite/CAS/revision/recovery，隔离格式与 OCR 进程             | 前端状态                     | `babel-storage`、adapter crates、`tools/*-worker` |

## 通信规则

1. 前端访问本地核心必须使用 `DesktopBridge`。生产入口默认实例化 `TauriDesktopBridge`；fixture 只在显式开发模式下允许。
2. 新业务行为先判断是否属于既有 `Kernel` 用例。若需桌面边界能力，新增 Bridge + Tauri command；不要在路由中实现导出、revision 或格式规则。
3. Rust 用例通过 storage、adapter 或 runtime crate 操作事实；不要反向依赖 React 或 Tauri DTO。
4. UI 通过 Query 刷新和失效缓存显示已提交结果；Zustand 不成为翻译/revision 的第二事实源。

## 当前边界的例外与风险

- Tauri 和 application 均有工作区路径校验/恢复代码，修改前先读 [11_FILE_SYSTEM.md](11_FILE_SYSTEM.md)。
- 工作台会话同时使用浏览器持久化和项目 `.config/workspace-state.json`，目前无正式冲突策略。
- `read_workspace_state` 存在前后端 DTO 不匹配，恢复行为不能视作已验证。
- 部分 Bridge 投影 API 依赖当前活动 Kernel，而不是严格使用传入 `projectId`。

## 典型调用链：保存翻译

```text
TranslationEditor
  -> onPersist(document)
  -> DesktopBridge.saveTranslationDocument(request)
  -> invoke("save_translation")
  -> Tauri save_translation
  -> Kernel.save_translation_document
  -> ProjectStore revision/head transaction
  -> receipt -> Query invalidation -> UI re-render
```

详细接口见 [06_IPC.md](06_IPC.md)，保存语义见 [10_EDITOR.md](10_EDITOR.md)，Rust 分层见 [08_RUST_CORE.md](08_RUST_CORE.md)。
