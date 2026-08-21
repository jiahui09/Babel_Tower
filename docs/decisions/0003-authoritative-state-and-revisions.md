# ADR 0003: 翻译与 revision 由 Rust/SQLite 权威保存

## 状态

已实现。

## 决策

翻译以 `TranslationDocumentV1` 保存；每次 durable save 创建/确认 revision，客户端提交期望 revision ID。草稿独立于 durable revision。

## 依据

- `crates/babel-tir/src/lib.rs` 定义结构化文档和不变量。
- `crates/babel-storage/src/project.rs` 保存 revision、draft、undo/redo。
- `DesktopBridge` 请求包含 `expectedRevisionId`。

## 后果

React 编辑器 JSON 是视图状态而不是事实源。不要用 Zustand 或 route state 维护第二套权威译文/状态；冲突要由核心判断。

下一步：[05_STATE_OWNERSHIP.md](../05_STATE_OWNERSHIP.md)、[10_EDITOR.md](../10_EDITOR.md)。
