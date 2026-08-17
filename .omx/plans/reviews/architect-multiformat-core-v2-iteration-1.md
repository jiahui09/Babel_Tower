# 架构师审查：Babel Core v2（第 1 轮）

日期：2026-08-17  
角色：Architect  
结论：`ITERATE`

## 总体判断

SQLite + CAS + 资源图 + TIR + 持久化 DAG 的总体路线成立，复杂度与 EPUB、图片、10 万单元、格式升级后保留译文和工作进程隔离等已确认需求相匹配。最强反方是先做全 SQLite 的 TXT/Markdown 单体，但只在格式少、规模小且不要求稳定重绑定时占优，不满足当前 PRD。

## 阻塞与观察项

| 级别 | 问题 | 第 1 轮处理 |
| --- | --- | --- |
| P0 | 未提交输入没有明确安全边界 | 新增 `DraftSession`、恢复候选和不覆盖已确认修订规则 |
| P0 | 身份/重绑定仍是描述而非契约 | 新增 `SourceUnitIdentitySpec`、固定匹配阶段、候选证据和不可变人工裁决账本 |
| P1 | 发布成功、记录提交前有崩溃窗口 | 新增 `PublishIntentRecorded` 状态、发布意图和启动对账 |
| P1 | 备份闭包与 GC 时序不明确 | 新增同事务 backup root pin 和完成校验后释放规则 |
| P1 | IPC 缺 OS 端点认证 | 新增 Unix 0600、Windows SID ACL、nonce 和 capability token |
| P1 | DAG 缺 lease/fencing/构建身份 | 新增 `stage_build_id`、owner、heartbeat、lease、fencing 和 Ready 提交顺序 |
| P1 | writer 的 P0/P3 优先级未落地 | 新增 writer 优先队列、P3 行数/时间上限、FTS 隔离和 checkpoint 调度 |
| P1 | 性能统计口径不一致 | 新增宏基准 max(run)、RSS 总和峰值、CPU 归一化和冻结平台快照 |

## 架构师要求的测试补充

- 绑定负例的错误自动绑定率必须为 0，不能只看覆盖率。
- 测试草稿基准修订变化时不自动覆盖。
- 测试发布后、记录前崩溃。
- 测试并发备份/GC、未授权 IPC、旧 fencing token、P3 writer 批次让出。
- 将 PERF-07 至 PERF-13 变成机器可判定口径。

## 过度设计判断

资源图、绑定账本、对象层和 DAG 属于当前需求的必要边界，但实现必须保持“契约先行、渐进落地”。Phase 0 不实现通用图查询、公共插件 ABI、WASI 宿主、游戏或音视频适配器；2–3 周是证伪时间盒，不是发布日期承诺。

## 第 1 轮修订结果

上述十项已经写回 PRD、架构与测试规范。总体路线未改变，进入第 2 轮架构师复核。
