# 003 Project Structure

## Goal

学会从一个问题定位到第一批代码文件。

完成后，你应该能回答：

- 页面问题先看哪个目录？
- 状态问题先看哪个 store？
- IPC 问题从哪个 TypeScript 文件查到哪个 Rust 文件？

## Prerequisites

- 已完成 [001 First Run](001_first_run.md)
- 已读 [03_PROJECT_STRUCTURE.md](../03_PROJECT_STRUCTURE.md)
- 能使用 `rg`

## Concept

Babel Tower 是 monorepo。你不应该把所有问题都塞进 `src/`。

主链路是：

```text
apps/desktop/src/main.tsx
-> AppProviders
-> routes/components
-> DesktopBridge
-> apps/desktop/src-tauri/src/lib.rs
-> crates/babel-application/src/lib.rs
-> crates/babel-storage/src/
```

按问题类型定位：

- UI：`apps/desktop/src/routes/`、`apps/desktop/src/components/`
- UI 状态：`apps/desktop/src/stores/`
- 请求缓存：`apps/desktop/src/queries/project.ts`
- IPC：`apps/desktop/src/platform/desktop-bridge/` 和 `apps/desktop/src-tauri/src/lib.rs`
- Rust 业务：`crates/babel-application/src/lib.rs`
- SQLite / revision / draft：`crates/babel-storage/src/`

## Steps

1. 列出前端入口：

   ```bash
   rg --files apps/desktop/src | sort | sed -n '1,120p'
   ```

2. 找项目库首页：

   ```bash
   rg 'createFileRoute\("/")' apps/desktop/src/routes
   ```

3. 找 workbench store：

   ```bash
   rg "useWorkbenchStore" apps/desktop/src
   ```

4. 找 IPC bridge：

   ```bash
   rg "class TauriDesktopBridge|interface DesktopBridge" apps/desktop/src/platform/desktop-bridge
   ```

5. 找 Rust Tauri commands：

   ```bash
   rg "#\\[tauri::command\\]|generate_handler" apps/desktop/src-tauri/src/lib.rs
   ```

6. 找 Rust 应用层 Kernel：

   ```bash
   rg "struct Kernel|impl Kernel" crates/babel-application/src/lib.rs
   ```

## Files

| Path | 用途 |
| --- | --- |
| `apps/desktop/src/main.tsx` | 前端入口和 bridge 选择 |
| `apps/desktop/src/app/providers.tsx` | i18n、QueryClient、Theme、Bridge provider |
| `apps/desktop/src/routes/index.tsx` | 项目库首页 |
| `apps/desktop/src/components/workbench/app-shell.tsx` | 项目工作台外壳 |
| `apps/desktop/src/stores/workbench.ts` | tabs、groups、面板、保存态等 UI 会话状态 |
| `apps/desktop/src/platform/desktop-bridge/types.ts` | 前端 IPC DTO |
| `apps/desktop/src/platform/desktop-bridge/tauri-bridge.ts` | 前端 invoke 映射 |
| `apps/desktop/src-tauri/src/lib.rs` | Tauri command |
| `crates/babel-application/src/lib.rs` | 应用层业务编排 |

## Expected Result

你不需要理解每一行代码，但应该能画出一条最小查找路径。

例子：

```text
Settings 快捷键不生效
-> docs/13_SETTINGS.md
-> docs/14_COMMANDS.md
-> apps/desktop/src/components/settings/settings-dialog.tsx
-> apps/desktop/src/stores/settings.ts
-> apps/desktop/src/commands/registry.ts
```

## Common Errors

| Symptom | Cause | Fix |
| --- | --- | --- |
| 一上来全仓库搜索一个普通词 | 搜索词太宽 | 先按目录缩小范围 |
| UI 问题直接改 Rust | 没判断状态归属 | 先读 [05_STATE_OWNERSHIP.md](../05_STATE_OWNERSHIP.md) |
| IPC 问题只改 TS 类型 | 忘了 Rust command DTO | 同时查 `types.ts`、`tauri-bridge.ts`、`src-tauri/src/lib.rs` |
| 把 Query 当权威状态 | 混淆缓存和持久化 | 读 [07_FRONTEND.md](../07_FRONTEND.md) |

## Acceptance

你完成本教程的标准：

- 能用 `rg` 找到项目库首页。
- 能指出 workbench store 的文件。
- 能沿一个 IPC 方法找到 TypeScript bridge 和 Rust command。
- 能说出 `crates/babel-storage/src/` 大致负责什么。

## Reflection

选择一个真实问题并写下你的定位路线：

- 问题是什么？
- 第一站文件是哪一个？
- 第二站文件是哪一个？
- 你为什么不从别的目录开始？
