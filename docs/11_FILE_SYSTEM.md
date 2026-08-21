# 文件系统与项目存储

## 路径边界

- 项目注册表：应用数据目录下的 `projects.json`。
- 项目配置：项目根目录 `.config/`，包括工作区会话文件。
- 领域数据：SQLite 数据库和 CAS 对象，由 `babel-storage` 管理。
- 导入/导出：通过 application 与 adapter 层处理，不能由 React 直接读写。

## 导入链

```text
系统文件选择 -> DesktopBridge -> Tauri command
-> import adapter -> Kernel / ProjectStore -> identity + SQLite/CAS
-> Query invalidation -> 文件树与工作项更新
```

文件名、项目根目录和导出目标必须经过边界校验。TXT、Markdown、EPUB 适配器负责格式解析；OCR worker 通过受控进程边界工作。

## 开发规则

新增文件能力时先查 `apps/desktop/src/platform/desktop-bridge/`、`apps/desktop/src-tauri/src/lib.rs` 和 `crates/babel-application/src/lib.rs`，再决定是否需要新 IPC。不要在组件中拼接项目绝对路径或直接调用 Node 文件 API。

## 当前限制

工作区恢复存在前后端 DTO 不匹配风险：TS 调用 `read_workspace_state` 只传 `projectId`，Rust 请求结构还要求 `node_id`。Windows 原生路径、OCR runtime 和发布产物尚未完成验证。

下一步阅读：[06_IPC.md](06_IPC.md)、[17_BUILD.md](17_BUILD.md)。
