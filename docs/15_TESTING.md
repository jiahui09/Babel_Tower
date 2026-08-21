# Babel Tower 测试说明

本文只整理已经验证过的测试层级和当前缺口。它的目标是让新人先知道该跑什么，再知道哪些结果能信，哪些结果只能当作局部证据。

## 先看结论

- 单元和集成测试：`pnpm test`、`cargo test --workspace`。
- fixture E2E：`pnpm test:e2e`。
- 真实桌面 E2E：当前缺失。
- 全量前端质量门禁：`pnpm check`，目前会被 Prettier 问题阻断。
- 架构方向检查：`./tools/check-architecture.sh`。

## 测试分层

| 层级             | 解决什么问题                                                | 当前状态 | 入口                                                           | 不能证明什么                                       |
| ---------------- | ----------------------------------------------------------- | -------- | -------------------------------------------------------------- | -------------------------------------------------- |
| Unit / component | 组件、store、转换函数、Rust 逻辑的局部回归                  | Complete | `pnpm test`，`cargo test --workspace`                          | 不能证明真实桌面、真实文件系统和安装包             |
| Integration      | Rust workspace、worker IPC、SQLite / storage / adapter 链路 | Complete | `cargo test --workspace`                                       | 不能证明浏览器 UI 和 Tauri 安装后的行为            |
| Fixture E2E      | 在浏览器里跑桌面骨架，验证页面和局部交互                    | Complete | `pnpm test:e2e`                                                | 不能证明真实 Tauri、原生文件对话框、系统级重启恢复 |
| Real desktop E2E | 安装后的真实桌面应用端到端流程                              | Missing  | 目前没有可重复的 installed-app / WebDriver / tauri-driver 方案 | 不能把 fixture 结果当成真实桌面验收                |

## 当前已验证的测试事实

- 前端 Vitest 通过了 7 个文件、14 个测试。
- Rust workspace 测试通过。
- `./tools/check-architecture.sh` 通过。
- `pnpm check` 当前会被 21 个 Prettier 问题阻断。
- Playwright 只覆盖 fixture 浏览器环境，不覆盖真实 Tauri。

## 新人该先看哪里

1. 先看 [CURRENT_STATE.md](CURRENT_STATE.md)，确认哪些能力是 Complete，哪些只是 Partial。
2. 再看 [16_E2E.md](16_E2E.md)，分清 fixture E2E 和真实桌面 E2E。
3. 最后看 [19_TROUBLESHOOTING.md](19_TROUBLESHOOTING.md)，对照常见失败。
