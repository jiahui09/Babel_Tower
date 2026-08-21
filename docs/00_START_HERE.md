# Babel Tower 开发入口

> 本文是开发知识库的起点。代码是事实源；本文只总结当前已验证的实现和明确限制。

## Babel Tower 是什么？

Babel Tower 是一款本地优先的单用户桌面翻译工作台。用户把 TXT、Markdown 或 EPUB 导入本地项目，按长文、单元或图片资源三种视图编辑人工译文，然后校验并导出新文件。React 负责工作台体验；Rust、SQLite 和 CAS 保存项目事实与安全导出。

它不是通用协作平台、机器翻译产品或已经完成发布闭环的最终安装包。完整状态看 [CURRENT_STATE.md](CURRENT_STATE.md)。

## 学习路线

```text
环境
  -> 产品与代码结构
  -> 第一次修改
  -> 状态所有权
  -> IPC
  -> Rust 核心
  -> Feature 工作流
  -> 测试
  -> E2E
  -> 排障
  -> 发布
```

按此顺序阅读：

1. [01_PRODUCT.md](01_PRODUCT.md)：理解用户和工作流。
2. [03_PROJECT_STRUCTURE.md](03_PROJECT_STRUCTURE.md)：出现问题时先去哪个目录。
3. [02_ARCHITECTURE.md](02_ARCHITECTURE.md)：理解边界。
4. [05_STATE_OWNERSHIP.md](05_STATE_OWNERSHIP.md) 和 [06_IPC.md](06_IPC.md)：在改功能前确认谁拥有状态、请求怎样跨层。
5. [07_FRONTEND.md](07_FRONTEND.md) 或 [08_RUST_CORE.md](08_RUST_CORE.md)：按所改层继续阅读。
6. [15_TESTING.md](15_TESTING.md)、[16_E2E.md](16_E2E.md)、[19_TROUBLESHOOTING.md](19_TROUBLESHOOTING.md)：验证和排障。
7. [tutorials/README.md](tutorials/README.md) 和 [tasks/README.md](tasks/README.md)：按 Level 学习并领取真实缺陷任务。

## 环境和启动

仓库要求 Node `>=22.12.0 <23`，包管理器为 pnpm 11；Rust workspace 声明 Rust 1.97。Tauri 桌面运行还需要本机的系统 WebView/图形依赖。

```bash
pnpm dev
```

这会启动 Vite，并由 Tauri 打开桌面窗口。以下命令只适合界面调试，不能验证桌面核心：

```bash
pnpm dev:web
```

在部分 Linux Wayland/GBM 组合上，如出现 `Failed to create GBM buffer`，使用：

```bash
pnpm dev:linux-safe
```

更多构建前置条件见 [17_BUILD.md](17_BUILD.md)。

## 项目结构速览

| 你要找的事情           | 先看哪里                                                       |
| ---------------------- | -------------------------------------------------------------- |
| 页面、组件、样式       | `apps/desktop/src/routes/`、`components/`、`design/tokens.css` |
| UI 状态、标签页、面板  | `apps/desktop/src/stores/`、`components/workbench/`            |
| 请求缓存、桌面 API     | `apps/desktop/src/queries/`、`platform/desktop-bridge/`        |
| Tauri 命令、文件和设置 | `apps/desktop/src-tauri/src/lib.rs`                            |
| 业务用例               | `crates/babel-application/src/lib.rs`                          |
| SQLite、CAS、恢复      | `crates/babel-storage/src/`                                    |
| 格式导入/导出          | `crates/babel-*-adapter/` 与 `tools/*-worker/`                 |

完整映射看 [03_PROJECT_STRUCTURE.md](03_PROJECT_STRUCTURE.md)。

## 第一次修改建议

选择一个**不跨越权威边界**的小改动，例如为已存在的 UI 状态补充可访问文案，或为已有 Bridge 结果补充 Loading/Error 展示。不要把项目、翻译、revision、导出或状态机复制进 React。

建议步骤：

1. 在 [05_STATE_OWNERSHIP.md](05_STATE_OWNERSHIP.md) 确认状态归属。
2. 沿 [06_IPC.md](06_IPC.md) 的链路查调用者和实现。
3. 做最小改动，并补最近一层测试。
4. 运行目标测试，再运行相应的格式、类型和构建检查。
5. 对照 [architecture/dangerous-changes.md](architecture/dangerous-changes.md) 确认没有触及高风险契约。

## 开发完成以后

- 前端：先运行 `pnpm test`，再按 [15_TESTING.md](15_TESTING.md) 运行 `pnpm check`。当前工作树的 Prettier 门禁曾失败，不能跳过该结果。
- Rust：运行 `cargo test --workspace`；高影响核心改动还应运行 `cargo clippy --workspace --all-targets -- -D warnings` 和格式检查。
- 桌面、文件、OCR、恢复、导出改动不能仅用 fixture E2E 宣称完成，见 [16_E2E.md](16_E2E.md)。

## 遇到问题去哪里

| 症状                     | 先读                                                                   |
| ------------------------ | ---------------------------------------------------------------------- |
| 页面无数据、请求失败     | [06_IPC.md](06_IPC.md)、[19_TROUBLESHOOTING.md](19_TROUBLESHOOTING.md) |
| 保存、冲突、撤销异常     | [10_EDITOR.md](10_EDITOR.md)、[08_RUST_CORE.md](08_RUST_CORE.md)       |
| 标签、分屏、项目切换异常 | [09_WORKBENCH.md](09_WORKBENCH.md)                                     |
| 文件树/工作区问题        | [11_FILE_SYSTEM.md](11_FILE_SYSTEM.md)                                 |
| 恢复、草稿或导出中断     | [12_RECOVERY.md](12_RECOVERY.md)                                       |
| 设置或快捷键看似无效     | [13_SETTINGS.md](13_SETTINGS.md)、[14_COMMANDS.md](14_COMMANDS.md)     |
| 打包/平台问题            | [17_BUILD.md](17_BUILD.md)、[18_RELEASE.md](18_RELEASE.md)             |

下一步：[01_PRODUCT.md](01_PRODUCT.md)。

完成基础阅读后，从 [tutorials/001_first_run.md](tutorials/001_first_run.md) 开始；能启动项目后再进入 [tasks/README.md](tasks/README.md)。
