# Recovery 与草稿恢复

## 已实现的核心

Rust/storage 层存在 draft session、revision、恢复相关命令和项目级持久化基础。草稿与正式 revision 是不同概念：草稿用于恢复编辑中的内容，revision 才是可追溯的正式保存事实。

## 预期流程

```text
启动/打开项目 -> 查询恢复上下文 -> 展示可恢复草稿
-> 用户选择恢复或丢弃 -> 写入编辑器/清理草稿 -> 继续保存或导出
```

## 当前状态：Partial

`apps/desktop/src/routes/recovery.$projectId.tsx` 目前主要是恢复页导航，尚未提供完整的恢复决策、冲突呈现和验证闭环。工作区会话也同时由浏览器 Zustand persist 与项目 `.config/workspace-state.json` 保存，缺少明确冲突优先级。

## 修改要求

任何恢复语义修改都必须覆盖：异常退出、重复启动、恢复后再次保存、丢弃后不可回显、跨项目隔离和文件系统验证。不要把“读取到 draft”写成“用户已经恢复成功”。

下一步阅读：[10_EDITOR.md](10_EDITOR.md)、[architecture/dangerous-changes.md](architecture/dangerous-changes.md)。
