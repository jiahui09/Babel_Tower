# 危险修改区

下面这些内容改动前，必须先做更严格的验证。它们会影响数据一致性、恢复能力或 UI 与 Rust 的契约。

## Revision

涉及文件：

- `crates/babel-storage/src/project.rs`
- `crates/babel-storage/src/schema.rs`
- `crates/babel-application/src/lib.rs`

为什么危险：

- 修订链是翻译历史的权威来源
- 错误修改会破坏撤销、重做、冲突检测和导出

改之前至少验证：

- 保存会写入新修订，不会覆盖旧修订
- 冲突路径仍然拒绝过期 `expectedRevisionId`
- `undo` / `redo` 仍然只追加历史

## Identity

涉及文件：

- `crates/babel-domain/src/core.rs`
- `crates/babel-domain/src/identity.rs`
- `crates/babel-application/src/lib.rs`
- `crates/babel-storage/src/project.rs`

为什么危险：

- `ProjectId`、`UnitId`、`ResourceId`、`sourceUnitKey` 是跨层主键
- 错了会让缓存、工作项、标签与数据库记录对不上

改之前至少验证：

- 新旧 ID 编码规则一致
- 读写路径能通过现有测试
- 跨层 DTO 没有出现新的隐式转换

## Persistence

涉及文件：

- `apps/desktop/src/components/workbench/app-shell.tsx`
- `apps/desktop/src/stores/workbench.ts`
- `apps/desktop/src/stores/settings.ts`
- `apps/desktop/src-tauri/src/lib.rs`

为什么危险：

- 当前就存在前端持久化与 Tauri 持久化并存
- 改错会导致会话恢复、设置恢复或窗口布局恢复失真

改之前至少验证：

- 谁是唯一权威源
- 读写顺序不会互相覆盖
- 项目切换不会串状态

## IPC contracts

涉及文件：

- `apps/desktop/src/platform/desktop-bridge/types.ts`
- `apps/desktop/src/platform/desktop-bridge/tauri-bridge.ts`
- `apps/desktop/src-tauri/src/lib.rs`

为什么危险：

- DTO 结构一旦不一致，前端会直接解码失败或静默错配
- `read_workspace_state` / `write_workspace_state` 已经暴露过契约问题

改之前至少验证：

- 前后端字段完全一致
- 新增字段有默认值或迁移策略
- 至少有一次真实 invoke 路径测试

## Recovery semantics

涉及文件：

- `crates/babel-storage/src/recovery.rs`
- `crates/babel-application/src/lib.rs`
- `apps/desktop/src/routes/recovery.$projectId.tsx`

为什么危险：

- 恢复语义不是“显示一个页面”
- 核心恢复、导出恢复、工作区操作恢复和用户决策 UI 现在是不同层次

改之前至少验证：

- 自动恢复不丢数据
- 用户可见恢复流程没有伪装成完整功能
- 恢复失败能被诊断

## Export formats

涉及文件：

- `crates/babel-application/src/lib.rs`
- `crates/babel-tir`
- 各格式 worker / adapter

为什么危险：

- 导出格式是外部可见结果
- 变化可能破坏下游工具和历史包

改之前至少验证：

- 生成文件内容稳定
- 哈希 / 输出路径 / no-clobber 行为仍正确
- 至少覆盖一次真实导出路径

## TranslationStatus

涉及文件：

- `crates/babel-domain/src/workbench.rs`
- `crates/babel-application/src/lib.rs`
- `apps/desktop/src/platform/desktop-bridge/types.ts`
- `apps/desktop/src/routes/projects.$projectId.units.tsx`

为什么危险：

- 这是 UI 与 Rust 共享的工作项语义
- 如果前端自己推导状态，展示会和 Rust 偏离

改之前至少验证：

- Rust 是唯一状态来源
- 前端只做展示，不重写规则
- 相关列表页仍能刷新正确状态

## 下一步阅读

- [05_STATE_OWNERSHIP.md](../05_STATE_OWNERSHIP.md)
- [06_IPC.md](../06_IPC.md)
- [12_RECOVERY.md](../12_RECOVERY.md)
