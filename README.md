# Babel Tower

Babel Tower 是一个面向个人译者的离线桌面翻译工作台。它支持 TXT、Markdown 和 EPUB 的人工翻译工作流，并包含图片区域 OCR 与派生图片渲染能力。

开发文档入口是 [docs/00_START_HERE.md](docs/00_START_HERE.md)。这里说明环境、启动方式、代码定位、状态/IPC/Rust 边界、测试、排障和发布现状。

当前仓库是开发版，不是已签发的生产发布物。权威状态见 [docs/CURRENT_STATE.md](docs/CURRENT_STATE.md)。

## Quick Start

```bash
pnpm dev
```

该命令启动 Vite 并由 Tauri 打开桌面窗口。浏览器模式 `pnpm dev:web` 不提供 Tauri IPC，不能用于验证文件、项目存储、OCR 或导出。

## Validation

```bash
pnpm test
cargo test --workspace
```

完整开发与验证流程见 [docs/15_TESTING.md](docs/15_TESTING.md) 和 [docs/17_BUILD.md](docs/17_BUILD.md)。

新开发者按 [docs/00_START_HERE.md](docs/00_START_HERE.md) 进入，再从 [docs/tutorials/001_first_run.md](docs/tutorials/001_first_run.md) 开始渐进学习；真实缺陷任务见 [docs/tasks/README.md](docs/tasks/README.md)。
