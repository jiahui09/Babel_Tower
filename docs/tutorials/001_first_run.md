# 001 First Run

## Goal

完成第一次本地启动，并知道每个启动命令证明了什么、不证明什么。

你完成后应该能回答：

- Babel Tower 用什么包管理器？
- Web 调试启动和真实桌面启动有什么区别？
- 哪些测试只能证明局部逻辑，哪些不能证明真实桌面流程？

## Prerequisites

- 已在仓库根目录：`/home/jiahui/project/Babel_Tower`
- Node 版本满足 `>=22.12.0 <23`
- 已安装 pnpm 11
- 已安装 Rust 工具链
- 真实 Tauri 桌面运行还需要系统 WebView / 图形依赖

先读：

- [00_START_HERE.md](../00_START_HERE.md)
- [17_BUILD.md](../17_BUILD.md)
- [15_TESTING.md](../15_TESTING.md)

## Concept

Babel Tower 是桌面应用，不是纯 Web 应用。

启动分两类：

- `pnpm dev`：走 Tauri，目标是打开桌面窗口。
- `pnpm dev:web`：只启动 Vite Web 调试，适合看 UI，但不能证明桌面 IPC、真实文件系统、重启恢复或安装包。

测试也分层：

- `pnpm test`：前端 Vitest。
- `cargo test --workspace`：Rust workspace。
- `pnpm test:e2e`：当前是 fixture 浏览器 E2E，不是真实 Tauri E2E。

## Steps

1. 确认你在仓库根目录：

   ```bash
   pwd
   ```

2. 查看项目脚本：

   ```bash
   sed -n '1,120p' package.json
   sed -n '1,140p' apps/desktop/package.json
   ```

3. 安装依赖：

   ```bash
   pnpm install
   ```

4. 启动 Web 调试：

   ```bash
   pnpm dev:web
   ```

   这个命令只证明前端开发服务器能启动。

5. 启动真实桌面开发模式：

   ```bash
   pnpm dev
   ```

6. 如果 Linux 图形栈出现 GBM / Wayland 相关错误，改用：

   ```bash
   pnpm dev:linux-safe
   ```

7. 跑最小测试：

   ```bash
   pnpm test
   cargo test --workspace
   ```

## Files

| Path | 用途 |
| --- | --- |
| `package.json` | 根脚本，定义 `dev`、`dev:web`、`test`、`build`、`rust:check` |
| `apps/desktop/package.json` | 桌面前端脚本和依赖 |
| `apps/desktop/src/main.tsx` | 选择 Tauri bridge 或 fixture bridge 的前端入口 |
| `apps/desktop/src-tauri/src/lib.rs` | Tauri command 注册和桌面后端入口 |
| `docs/15_TESTING.md` | 测试层级和当前缺口 |
| `docs/16_E2E.md` | E2E 的真实状态 |

## Expected Result

- `pnpm dev:web` 能启动 Vite 页面。
- `pnpm dev` 能尝试启动 Tauri 桌面窗口。
- `pnpm test` 能运行前端单元测试。
- `cargo test --workspace` 能运行 Rust workspace 测试。

如果 `pnpm check` 因 Prettier 失败，不要把它当成你的环境错误。Phase 2 文档已经记录：当前前端质量门禁存在 Prettier 阻断。

## Common Errors

| Symptom | Cause | Fix |
| --- | --- | --- |
| `pnpm` 版本不对 | 仓库要求 pnpm 11 | 使用 corepack 或安装 pnpm 11 |
| `node` 版本不对 | `package.json` 限定 Node 22 | 切换到 Node `>=22.12.0 <23` |
| Tauri 窗口打不开 | 系统 WebView / 图形依赖缺失 | 先读 [17_BUILD.md](../17_BUILD.md)，Linux 可试 `pnpm dev:linux-safe` |
| Web 页面能开但 IPC 报错 | `dev:web` 没有 Tauri runtime | 只用它看 UI；需要 IPC 时用 `pnpm dev` |

## Acceptance

你完成本教程的标准：

- 能说清 `pnpm dev` 和 `pnpm dev:web` 的区别。
- 能指出 `apps/desktop/src/main.tsx` 是 bridge 选择入口。
- 能运行或解释为什么无法运行 `pnpm test` 与 `cargo test --workspace`。
- 没有把 fixture / Web 调试结果当成真实桌面验收。

## Reflection

记录你的答案：

- 今天哪个命令失败了？失败信息是什么？
- 这个失败属于环境、依赖、测试，还是项目已知缺口？
- 你下一次遇到启动问题会先查哪个文档？
