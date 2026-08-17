# 第四阶段独立审查记录

日期：2026-08-18

## 代码审查

角色：独立 `code-reviewer`

结论：功能闭环 `APPROVE`，性能门单独处理。

已修复项：

- Markdown 抽取现在只接受 inventory 生成的 TextStream resource ID；Document/Image ID 会被拒绝。
- Markdown 预览同样先取得 TextStream，不再使用随机资源 ID。
- TXT/Markdown worker 探测响应携带 adapter ID、build 和 identity version；应用层比对一致后才记录 generation。

复验证据：Markdown 适配器 5 项测试、Markdown worker 3 项 IPC 测试、应用层 21 项单元测试和 G2 合同测试通过；相关 Clippy 通过。

## 架构审查

角色：独立 `architect`

原结论：功能边界成立；阶段性能签发 `BLOCK`。

原阻断原因只有 M 语料完整导入：39.134 秒高于 20 秒门槛。Markdown AST 私有边界、IPC/CAS、SQLite 单写者、崩溃恢复、冻结导出和 Phase 7 发布延期策略未发现高严重度架构破坏。

2026-08-18 签发复核：同一配置 5 次完整评估 max 为 19.150 秒，原性能阻断已由机器可读证据解除。优化保持单写者、活动代原子切换和 `FULL` 权威写入；新项目页布局与缓存预算没有改变公共格式模型。

## 版本化后续项

- Adapter protocol 下一版本应让 `verify_output` 取得源对象/导出计划上下文，或使用与 staging 绑定的可信验证元数据，使其可独立证明保护结构未变化。当前应用路径在 `materialize` 阶段已经执行该校验，因此不阻断本阶段功能闭环。
- Markdown 身份路径目前包含全局文本序号。它不会自动错绑，但前置插入会让后续唯一单元进入 Shifted 人工审查。下一轮应改为父节点内的局部结构路径，并在保持“重复内容不猜测”的前提下使用邻域指纹辅助候选排序。
- 5,000 条 generation 批次、32 KiB 页和有界缓存改善了吞吐；后续仍需增加单批事务/取消响应时间探针，防止只优化总时间。

## 最终口径

- 第四阶段功能闭环：通过。
- 第四阶段数据安全边界：当前测试范围内通过。
- 第四阶段 M 语料性能签发：通过（5 次 max 19.150 秒）。
- Windows/Arch 完整安装包签发：按计划延期到 Phase 7。
