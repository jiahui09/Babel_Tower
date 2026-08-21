# Babel Tower 构建说明

本文讲日常开发怎么起、怎么编、怎么检查。它不把 Phase 3 TXT 纵切包当成当前桌面产品的正式发布闭包。

## 先看结论

- 开发启动：`pnpm dev`
- 仅前端开发：`pnpm dev:web`
- Linux 兼容启动：`pnpm dev:linux-safe`
- 前端构建：`pnpm build`
- 前端质量检查：`pnpm check`
- Rust workspace 检查：`pnpm rust:check`

## 常用命令

| 目的                      | 命令                  | 说明                                                                                                            |
| ------------------------- | --------------------- | --------------------------------------------------------------------------------------------------------------- |
| 启动完整桌面开发环境      | `pnpm dev`            | 根脚本会调用 desktop 的 `tauri dev`，进入真实桌面壳                                                             |
| 启动更稳的 Linux 开发环境 | `pnpm dev:linux-safe` | 给 WebKit 关闭 DMABUF 相关路径，适合部分 Linux 环境                                                             |
| 只看前端页面              | `pnpm dev:web`        | 只启动 Vite，不进 Tauri                                                                                         |
| 构建前端产物              | `pnpm build`          | 运行 `vite build && tsc --noEmit`                                                                               |
| 前端检查                  | `pnpm check`          | typecheck、lint、format:check、vite build                                                                       |
| Rust 检查                 | `pnpm rust:check`     | `cargo fmt --all -- --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace` |

## 这些构建各自负责什么

### 开发构建

`apps/desktop/src-tauri/tauri.conf.json` 里，Tauri 的 `beforeDevCommand` 是 `pnpm dev`，`beforeBuildCommand` 是 `pnpm build`。这表示桌面壳启动前会先准备前端，正式构建时也会先产出前端 bundle。

### 前端构建

`apps/desktop/package.json` 的 `build` 只覆盖前端 bundle 和 TypeScript 校验。它能证明页面代码能编译，但不能证明真实桌面安装态可用。

### Rust 构建

根脚本 `pnpm rust:check` 负责 Rust 侧的格式、lint 和测试。当前它是可以作为核心可信度检查的。

## 发布相关的构建

仓库里还有 `packaging/build-arch-phase0.sh` 和 `packaging/build-windows-phase0.sh`，但它们验证的是历史 Phase 3 TXT 纵切包，不是当前桌面产品的完整发布闭包。

仓库当前有的 OCR 资源在 `resources/ocr/ppocrv6-tiny/`。代码会尝试读取这些资源，但 Windows 实机闭包和完整字体闭包还没有被验证。

## 新人该先看哪里

1. [15_TESTING.md](15_TESTING.md)
2. [18_RELEASE.md](18_RELEASE.md)
3. [19_TROUBLESHOOTING.md](19_TROUBLESHOOTING.md)
