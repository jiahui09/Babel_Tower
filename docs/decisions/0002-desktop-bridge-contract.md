# ADR 0002: 前端通过 DesktopBridge 访问桌面核心

## 状态

已实现，存在契约缺陷。

## 决策

UI 不直接调用 `invoke` 或读写项目文件，而是依赖 `DesktopBridge` 接口。生产实现为 `TauriDesktopBridge`，测试可显式注入 fixture。

## 依据

- `platform/desktop-bridge/types.ts` 定义接口和 DTO。
- `tauri-bridge.ts` 集中映射命令和错误。
- `main.tsx` 限制 fixture 仅在 development 环境启用。

## 当前限制

`read_workspace_state` 的前端请求和 Rust DTO 不一致，且部分读取 API 忽略传入的 project ID。新增接口时必须添加序列化/注册/真实端到端契约测试。

下一步：[06_IPC.md](../06_IPC.md)。
