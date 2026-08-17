# 第四阶段 QA 报告

日期：2026-08-18

## 验证范围

本报告覆盖 Markdown 适配器、隔离工作进程、统一应用层、存储 v8 翻译辅助、崩溃恢复、差分导出和 M 语料性能。不覆盖 Phase 7 的 Windows/Arch 完整安装包闭包。

## 已运行命令

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --all-targets
./tools/check-architecture.sh
cargo build --release -p babel-markdown-worker
cargo run --release -p babel-phase4 -- smoke
cargo check -p babel-markdown-adapter --lib --target x86_64-pc-windows-msvc
target/release/babel-phase4 benchmark --work-dir <独立工作目录> \
  --output .omx/phase4/perf-evaluator-rN.json
```

格式、Clippy、全工作区测试、架构检查、release 工作进程构建、Markdown 冒烟和新增适配器 Windows 目标检查均通过。100 MiB 基准以同一配置真实运行 5 次；中途磁盘配额失败的运行不计入结果。

全工作区 Windows 目标检查在 bundled SQLite 的 C 构建步骤停止：当前 Linux 环境没有 MSVC `lib.exe`。Markdown 适配器本身已通过 Windows 目标检查；工作进程和整包的 Windows 原生闭包仍按计划留给 Windows CI/Phase 7，不声明 Linux 上完成了 Windows 二进制验证。

## 功能验收矩阵

- Markdown 清单与依赖边界：已实现保守 CommonMark 文本/图片引用切片，Comrak 默认特性关闭。
- 解析与抽取：适配器及 IPC 测试通过。
- 保护结构：链接目标、图片目标、代码/HTML 保护签名和候选输出重解析受测试约束。
- 差分导出：未修改字节保持不变；重叠补丁和变化后的源快照会被拒绝。
- 稳定重导入：应用层覆盖 Markdown 绑定审查，歧义单元不会自动继承。
- 翻译辅助：存储 v8 覆盖术语、批注、标记、历史、重复句和原子批量替换。
- 崩溃恢复：冒烟测试覆盖关闭后重开并恢复译文。
- 发布策略：完整 Windows/Arch 安装包闭包延期到 Phase 7。

## 性能验收

证据文件：`perf-evaluator.json`（5 次 max）及 `perf-evaluator-r5.json` 至 `perf-evaluator-r9.json`（原始结果）。

| 指标 | 实测 | 门槛 | 结果 |
| --- | ---: | ---: | --- |
| 输入字节 | 104,857,600 | 104,857,600 | 符合 |
| 单元数 | 250,000 | 约 250,000 | 符合 |
| 图片引用 | 2,000 | 2,000 | 符合 |
| 完整导入 max | 19,150 ms | ≤20,000 ms | 通过 |
| 冷打开 max | 2 ms | ≤3,000 ms | 通过 |
| 峰值 RSS max | 305.20 MiB | ≤400 MiB | 通过 |
| 可见页查询 p95 max | 0.651 ms | ≤50 ms | 通过 |
| 保存 p95 max | 7.979 ms | ≤300 ms | 通过 |
| 搜索 p95 max | 91.327 ms | ≤150 ms | 通过 |

结论：功能与性能 QA 均可收口。5 次完整导入为 19,150、18,410、18,378、18,093、17,331 ms，全部 `SUPPORTED`。主要改进来自 32 KiB 新项目页布局和 64 MiB writer 有界缓存；没有使用并行 writer。worker 正常退出后 `/tmp` 专用 CAS 残留计数为 0。

## 延期项

- Windows NSIS 生成及 Windows 实机安装验证。
- Arch 干净环境、禁网安装、升级和卸载验证。
- EPUB、OCR、图片嵌字和 GFM 扩展。
