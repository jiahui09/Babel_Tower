# Phase 5 QA 报告

日期：2026-08-18

## 结论

EPUB 2/3 功能纵向闭环与合同型 L 性能门通过。极端文本型 L 暴露应用层仍聚合全书 TIR 的限制，结果保留为 `FALSIFIED`，不纳入首版常规 EPUB 性能声明。

## 功能与静态门

```text
cargo test --workspace
结果：全部通过；application 22、storage 35、runtime 17、EPUB adapter 7、EPUB worker IPC 3，其他 workspace 测试均通过。

cargo clippy --workspace --all-targets -- -D warnings
结果：通过，零警告。

cargo fmt --all -- --check
结果：通过。

bash tools/check-architecture.sh
结果：architecture dependency direction: ok。
```

## 外部兼容

- `unzip -t .omx/phase5/smoke-r2-work/smoke-export.epub`：所有 7 个成员通过。
- EPUBCheck 5.3.0：0 fatal、0 error、0 warning、0 info；机器结果见 `epubcheck-5.3.0-r2.json`。
- smoke：导入 3 单元、人工译文保存、重启恢复、应用层路径导出、CSS 保留全部通过；见 `smoke-r2.json`。

## 性能

合同型 L，文件 `benchmark-l-r4.json`：

- 实际输入 537,207,515 B；展开 2,147,483,648 B；5000 成员；100,000 单元；100 章节。
- 可翻译文本目标 64 MiB，其余为受压缩比守卫约束的资源 payload。
- 首批内容 148 ms（门槛 8 s）。
- 完整导入 17.890 s（门槛 90 s）。
- 导出使用独立大资源语料：537,281,545 B 输入、2 GiB 展开、5000 成员、1 个已翻译单元。这样不绕过“缺译文阻止导出”的产品规则，同时覆盖真实 `Kernel::export_active_epub_to_path` 路径；结果 2.147 s（门槛 60 s）。
- runner 主进程峰值 204.91 MiB，worker 峰值 11.27 MiB；二者峰值直接相加得到保守上界 216.18 MiB（门槛 1.5 GiB）。
- 输出哈希和长度由 Kernel 报告与落盘文件重新计算交叉确认；导出 API 不返回完整 EPUB `Vec<u8>`。

极端文本型 L，文件 `benchmark-l-r2.json`：

- 同为 2 GiB 展开/5000 成员/100,000 单元，但约 1.6 GiB 是可翻译文本。
- 完整导入 99.089 s，峰值 RSS 1,824.57 MiB，结论 `FALSIFIED`。
- 根因：应用层在提交 SQLite 前聚合全部 `ExtractedUnit` 与 binding plan。Worker 已做到单章有界；应用层流式导入留作后续明确优化项。

## 安全与恢复专项

- staging 候选验证后以同目录临时文件流式复制，再用原子 hard-link no-clobber 发布；目标已存在时返回错误且内容不变。
- 同一成员内乱序 overlay 会先按字节跨度排序，再检查重叠并补丁。
- ZIP 成员解压按 64 KiB 分块，每块执行 deadline/cancellation checkpoint；专项测试证明处理中取消会返回 `Cancelled`。
- 多 `TextStream` worker IPC 测试连续抽取两个章节，并验证 worker RSS 字段可读取。

## 未执行

- 未进行 Windows 实机验证，符合既定策略。
- 未重复执行 NSIS、Arch 安装包与离线发布闭包；统一留到 Phase 7。
