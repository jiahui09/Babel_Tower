# Phase 3 QA 证据

生成日期：2026-08-17

## 结论

- TXT 核心纵向能力：`SUPPORTED`。
- Phase 3 发布闭包：`FALSIFIED`。
- 最终生产桌面整包：`FALSIFIED`。

## 已执行门禁

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --all-targets
./tools/check-architecture.sh
cargo build --release -p babel-phase3 -p babel-txt-worker -p babel-phase0
target/release/babel-phase3 smoke
target/release/babel-phase3 benchmark --output .omx/phase3/s-corpus.json
```

结果：111 项测试通过，Clippy 零警告，格式与依赖方向门禁通过。smoke 为 `SUPPORTED`。

## S 语料

| 指标 | 结果 | 门槛 |
| --- | ---: | ---: |
| 输入 | 10,000,000 bytes / 100,000 units | 固定语料 |
| 首次导入 | 5,198 ms | 记录项，不作为热路径门禁 |
| 冷打开 | 0 ms | 3,000 ms |
| 峰值 RSS | 92.70 MiB | 400 MiB |
| 分页 p95 | 0.290 ms | 50 ms |
| 保存 p95 | 2.521 ms | 300 ms |
| 搜索 p95 | 31.615 ms | 150 ms |

## 发布证据

Arch 最新包：

```text
611ae5ce4d540bd6cec96f055d2ce39c81c3ecc1fd095e4e33374feb753f73de  release/arch/babel-tower-phase3-txt-0.1.0-1-x86_64.pkg.tar.zst
```

本机解包后 smoke、worker 执行位、SBOM 和 ELF `NEEDED` 白名单均通过。Docker socket 返回 `permission denied`，所以最新包没有干净禁网容器安装/启动证据，manifest 正确保持 `clean_image.performed=false`。

Windows 最新 PE：

```text
ce97f84f1798ca540cfb1d818421267fe527591795b47fa0cb0fabe6aedcd690  target/x86_64-pc-windows-msvc/release/babel-phase3.exe
8fa04e5d16975f367f210ce1a508c955c7a83c33ad0e897e012ebcab5e14d720  target/x86_64-pc-windows-msvc/release/babel-txt-worker.exe
```

cargo-xwin 交叉编译成功。当前系统没有原生 `makensis`，Wine 因 `wineserver: bind: Operation not permitted` 不能封装安装器；旧安装器及旧 release manifest 已删除，`release/windows/pe-build-manifest.json` 保留诚实的中间状态。

## 发布硬化阶段关闭动作

1. 在可访问 Docker 的 Linux 发布 runner 执行 `./packaging/build-arch-ci.sh`，由 Arch builder 容器构建，再由独立禁网 Arch 容器安装和运行验证。
2. 在安装原生 `nsis` 的 Linux 发布 runner 执行 `./packaging/build-windows-phase0.sh`；只构建，不做 Windows 实机验证。
3. 重新运行 `target/release/babel-phase0 package-closure --release-dir release` 与 `./tools/phase3-audit.sh`；两者通过后才能签发 Phase 3 发布闭包。

上述动作是最终发布硬化证据，不阻塞 Phase 4 Markdown、Phase 5 EPUB 或后续核心功能开发；期间不得把当前 `FALSIFIED` 改写为发布通过。

CI 已声明这两条发布路径；工作流 YAML 通过本地解析，shell 脚本通过 `bash -n`。
