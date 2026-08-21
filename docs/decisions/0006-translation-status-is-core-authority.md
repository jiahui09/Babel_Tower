# ADR 0006: TranslationStatus 应以核心为唯一权威

## 状态

目标已由领域模型表达，前端投影部分未完成。

## 决策

`TranslationStatus`（Untranslated、Draft、Translated、Reviewed、Blocked）应由 Rust 工作项/投影提供，UI 只读取和呈现。

## 当前限制

单位列表在部分位置从译文是否为空推导草稿/未翻译，丢失 reviewed、blocked 等语义。这是已知偏离，不应被新代码复制。

下一步：[05_STATE_OWNERSHIP.md](../05_STATE_OWNERSHIP.md)。
