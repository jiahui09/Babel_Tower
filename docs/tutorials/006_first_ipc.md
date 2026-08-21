# 006 First IPC

## Goal

学会沿着 Frontend -> DesktopBridge -> Tauri command -> Rust Core 查 IPC，并设计一个最小 IPC 练习。

推荐练习：新增一个只读 health / version 类命令，用来返回应用侧的固定诊断信息。这个练习的重点是理解契约和测试，不是加入业务功能。

## Prerequisites

- 已完成 [005 First State Change](005_first_state_change.md)
- 已读 [06_IPC.md](../06_IPC.md)
- 已读 [08_RUST_CORE.md](../08_RUST_CORE.md)
- 能运行 `pnpm test`
- 能运行 `cargo test --workspace`

## Concept

Babel Tower 的生产 IPC 链路是：

```text
React
-> useDesktopBridge()
-> TauriDesktopBridge
-> invoke(command, args)
-> #[tauri::command] in apps/desktop/src-tauri/src/lib.rs
-> babel-application::Kernel / filesystem / settings / workers
```

新增 IPC 前必须先判断：

- 前端是否真的需要 Rust 才有的权威数据？
- 是否要读写项目、SQLite、文件系统或 worker？
- 是否已有 bridge 方法可以复用？

本教程的 health / version 命令只适合学习 IPC 形状。真实功能任务应来自 `docs/tasks/`，不能把这个练习当成产品需求。

## Steps

1. 读 IPC 文档：

   ```bash
   sed -n '1,220p' docs/06_IPC.md
   ```

2. 读前端类型：

   ```bash
   sed -n '1,220p' apps/desktop/src/platform/desktop-bridge/types.ts
   ```

3. 读 Tauri bridge：

   ```bash
   sed -n '1,220p' apps/desktop/src/platform/desktop-bridge/tauri-bridge.ts
   ```

4. 找 Rust command 注册：

   ```bash
   rg "#\\[tauri::command\\]|generate_handler" apps/desktop/src-tauri/src/lib.rs
   ```

5. 设计 DTO。示例形状：

   ```ts
   interface AppHealth {
     status: "ok";
     appName: string;
   }
   ```

   注意：这是教程示例。实际命名和字段要与当前代码风格一致。

6. 修改前端契约：

   - 在 `types.ts` 增加响应类型。
   - 在 `DesktopBridge` interface 增加方法。
   - 在 `fixture-bridge.ts` 增加默认 missing 实现。
   - 在 `tauri-bridge.ts` 调用新的 command。

7. 修改 Rust command：

   - 在 `apps/desktop/src-tauri/src/lib.rs` 添加 `#[tauri::command]`。
   - 把 command 加入 `tauri::generate_handler![...]`。
   - 如果响应需要序列化，使用当前文件已有的 serde 模式。

8. 加测试：

   - 前端 fixture 测试可以证明 DesktopBridge interface 需要显式 opt-in。
   - Rust 单元或 workspace 测试可以证明响应结构稳定。

9. 运行验证：

   ```bash
   pnpm test
   cargo test --workspace
   ```

## Files

| Path | 用途 |
| --- | --- |
| `apps/desktop/src/platform/desktop-bridge/types.ts` | 前端看到的 IPC DTO 和 `DesktopBridge` interface |
| `apps/desktop/src/platform/desktop-bridge/tauri-bridge.ts` | 前端 command 名称和 invoke 参数 |
| `apps/desktop/src/platform/desktop-bridge/fixture-bridge.ts` | 测试 bridge 默认未实现行为 |
| `apps/desktop/src/platform/desktop-bridge/fixture-bridge.test.ts` | fixture bridge 测试 |
| `apps/desktop/src-tauri/src/lib.rs` | Tauri command 和 handler 注册 |
| `docs/06_IPC.md` | IPC 规则和已知 DTO 风险 |

## Expected Result

- 你能说出新方法在 TypeScript interface、Tauri bridge、Rust command 三处的名字。
- 缺少任意一处时，TypeScript 或运行时会给出可解释的失败。
- `pnpm test` 和 `cargo test --workspace` 作为局部验证通过。

这仍然不能证明真实桌面 E2E。真实 Tauri E2E 当前在文档中标记为 Missing。

## Common Errors

| Symptom | Cause | Fix |
| --- | --- | --- |
| 前端方法存在但运行时报 command not found | Rust 没加入 `generate_handler!` | 检查 `apps/desktop/src-tauri/src/lib.rs` |
| Rust command 存在但前端类型不匹配 | DTO 字段名不一致 | 对照 `types.ts` 和 Rust request/response |
| fixture 测试全部默认通过 | fixture bridge 没显式 opt-in | 保持 `createFixtureBridge()` 的 missing 默认行为 |
| 把 health 命令接进产品 UI | 教程范围扩大 | 本练习只学 IPC，不新增产品入口 |

## Acceptance

你完成本教程的标准：

- 能画出 React 到 Rust command 的调用链。
- 能指出 `TauriDesktopBridge` 的 command 字符串。
- 能说明什么时候应该新增 IPC，什么时候不应该新增。
- 如果真的做了练习实现，必须运行 `pnpm test` 和 `cargo test --workspace`，并记录真实结果。

## Reflection

写下你的复盘：

- 你的 IPC 是否真的需要 Rust？如果不需要，它应该留在前端哪一层？
- 哪个文件最容易出现 DTO 不匹配？
- 这次验证为什么不能替代真实桌面 E2E？
