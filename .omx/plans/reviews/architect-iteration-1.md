# Architect Review - Iteration 1

- Verdict: `ITERATE`
- Critic gate ready: no
- Confidence: high

## Blocking findings

1. Markdown/EPUB 缺少可逆的混合内容 IR、占位符规则和对应 fixtures。
2. SQLite 与对象库缺少完整的发布、闭包备份、恢复和 GC 协议。
3. `saved` 未绑定 `synchronous=FULL`、命令去重、lost-ack 恢复和事件重放。
4. AppImage 在 FUSE 缺失时无法在应用内给出 GUI 诊断，Arch 单包发布只能作为带前置条件的发布门禁。
5. OCR 的进程隔离只证明故障隔离，未证明 OS 级恶意代码沙箱；内置解析器也需要故障边界。

## Antithesis and synthesis

最强反方不是 Electron 全 TypeScript，而是 Electron 壳配同一 Rust 权威核心。它以更大运行时和更新面换取 Chromium/IME/ProseMirror 跨平台一致性。保留壳无关的类型化 IPC，在阶段 0 用同一 Rust 核心做 Tauri 首选方案的客观平台门禁；只有门禁失败才通过 ADR 切换 Electron 壳。

## Sources checked

- https://sqlite.org/pragma.html#pragma_synchronous
- https://v2.tauri.app/distribute/appimage/
- https://docs.appimage.org/user-guide/troubleshooting/fuse.html
- https://v2.tauri.app/develop/sidecar/

