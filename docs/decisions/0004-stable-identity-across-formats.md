# ADR 0004: 使用稳定身份连接跨格式翻译工作项

## 状态

已实现。

## 决策

TXT、Markdown、EPUB 文本和图片区域以同一类翻译工作项及稳定 `unit_id`/source key 进入搜索、revision、校验和导出，而不是每种格式各自维护译文系统。

## 依据

- `crates/babel-domain/src/identity.rs` 实现稳定身份和重新绑定规则。
- `crates/babel-application/src/lib.rs` 定义 `TranslationWorkItem` 与资源队列。
- Rust 测试覆盖重排、重复和导入绑定。

## 后果

不要用位置、数组下标或显示文本作为新持久化身份。更改导入/绑定/locator 前必须跑完整核心测试。

下一步：[04_DATA_MODEL.md](../04_DATA_MODEL.md)、[architecture/dangerous-changes.md](../architecture/dangerous-changes.md)。
