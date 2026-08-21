# ADR 0001: 使用 Tauri + Rust 作为本地核心边界

## 状态

已实现。

## 决策

React 前端通过 Tauri command 访问本地项目能力；业务用例集中在 Rust `babel-application::Kernel`，不在浏览器层实现。

## 依据

- `apps/desktop/src/main.tsx` 创建生产 `TauriDesktopBridge`。
- `apps/desktop/src-tauri/src/lib.rs` 注册 command 并维护活动 Kernel。
- `crates/babel-application/src/lib.rs` 提供导入、保存、校验、导出和资源用例。

## 后果

前端可保持为渲染和交互层；项目数据、revision 和格式安全规则可在单一 Rust 核心验证。新增桌面能力必须维护 Tauri DTO/权限/错误边界。

下一步：[06_IPC.md](../06_IPC.md)。
