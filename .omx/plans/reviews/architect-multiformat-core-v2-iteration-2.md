# 架构师审查：Babel Core v2（第 2 轮）

日期：2026-08-17  
角色：Architect  
结论：`APPROVE`

## 复核范围

第 2 轮逐项复核第 1 轮提出的十项修改：DraftSession、SourceUnitIdentitySpec、绑定负例、DAG fencing、PublishIntent、backup pin、IPC 本地认证、writer 优先级、性能统计口径和 Phase 0 证伪门禁。

## 复核结论

- 草稿不推进 `commit_sequence`、不进入导出且不覆盖已确认修订，恢复对账闭合。
- 身份规范、四类重绑定、不可变裁决账本和错误自动绑定率为 0 的测试形成完整契约。
- DAG 的 build identity、lease、heartbeat、fencing 和 Ready 提交顺序能阻止陈旧 worker 污染缓存。
- PublishIntent 与幂等对账闭合“已发布、未记账”崩溃窗口。
- backup root pin 与 GC 的事务顺序闭合对象备份窗口。
- Unix 0600、Windows SID ACL、nonce 和 capability token 闭合本地 IPC 伪装入口。
- writer P0/P3 批次、FTS 隔离和 checkpoint 调度可执行、可压测。
- `max(run)`、RSS 总和峰值、CPU 归一化和冻结平台快照使性能门禁可机器判定。
- Phase 0 已明确为架构证伪时间盒，不是产品发布日期承诺。

没有发现新的阻塞矛盾。CJK tokenizer、OCR 引擎/许可、第三方 WASI 和对象 pack/辅助指纹作为非阻塞后续 ADR 保留合理。
