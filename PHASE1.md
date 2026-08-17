# Babel Tower Phase 1：权威内核与恢复

日期：2026-08-17

## 阶段结论

Phase 1 已把 Phase 0 的架构原型收敛为可供后续资源图、适配器宿主和 TXT 纵向链路调用的权威内核。当前完成的是“项目真相和故障安全”，不是桌面界面，也不宣称已经支持 TXT、Markdown、EPUB 或 OCR。

阶段退出门禁已经满足：独立代码审查为 `APPROVE`，独立架构审查为 `CLEAR`。Linux 本轮验证 Rust 内核；Windows 仍遵守既定规则：在 Linux 只生成发布产物，不把交叉环境结果称为 Windows 实机验证。

## 做了什么，以及为什么

### 1. 项目身份、命令与修订

- `project_id`、`unit_id` 和 `task_id` 使用 128 位随机永久身份；内容哈希和 `source_unit_key` 继续使用 SHA-256。
- 每个已确认编辑生成不可变 `translation_revision`，`unit_head` 只是在同一事务内更新的当前投影。
- `command_id` receipt 保证超时或进程重连后的重放只返回原结果，不重复生成修订。
- 撤销和重做恢复指定旧修订的内容，但自身仍生成新修订；调用必须携带预期当前头，陈旧操作会失败关闭。

这样做是为了让“已确认译文”成为可审计事实，而不是被编辑器状态或撤销栈覆盖的可变文本。

### 2. SQLite schema 与迁移

- Phase 1 签发时 schema 为 v5；Phase 2 已通过同一迁移机制提升为 v6，用于导入代、资源图、抽取单元和绑定账本。
- 打开旧项目时先在 `migration-backups/` 生成一致数据库快照和可达对象闭包；备份缺对象或验哈希失败会在任何 schema 写入前阻断迁移。
- 备份成功后，每一步迁移在 `IMMEDIATE` 事务中执行并记录完成标记；高于当前核心版本的项目拒绝打开。
- 表使用 STRICT、外键、定长 BLOB 约束、WAL、`synchronous=FULL`、显式 busy timeout 和手动 checkpoint。
- 迁移测试覆盖全新数据库、重复执行、旧 v1 逐步升级到 v5，以及未来版本拒绝降级打开。

格式私有产物不会进入这些权威表；Phase 2 的适配器只能依赖协议/TIR 类型，不能直接依赖本 schema。

### 3. 单写者与只读投影

- `babel-application::Kernel` 持有唯一项目文件锁，并启动唯一 SQLite writer 线程。
- P0 译文、草稿和撤销命令使用交互队列；后台任务使用独立队列，writer 每次先检查交互工作。
- 大对象流式写入、备份复制和 GC 文件扫描都不占用 SQLite writer；writer 只执行短权威事务。
- 页面、提交序号、草稿和诊断通过 `query_only` 的短连接投影读取，不允许反向写库。
- 已耐久提交通过 `CommitEvent` 发布；幂等重放不会再次广播同一逻辑提交。

这使 UI 将来只面对命令、查询和订阅，不接触 SQLite、项目目录或适配器内部类型。

### 4. 草稿、任务和诊断恢复

- DraftSession 独立保存，不推进 `commit_sequence`，也不进入导出快照。
- 恢复草稿时比较 `base_revision_id` 和当前头；基准变化返回 `BaseChanged`，不覆盖已确认译文。
- 任务状态迁移有显式许可图；完成或取消的任务不能静默重开，失败任务只能先回到 Pending。
- 核心重启时，遗留 Running 任务统一恢复为 Paused，并记录 `interrupted` 原因。
- 本地诊断分离用户消息和技术细节，由只读投影提供，不把内部异常直接暴露给译者。

### 5. 不可变对象、备份和 GC

- 源字节先写同文件系统临时对象，文件与目录同步后按 SHA-256 发布；SQLite 只能引用已完成发布的对象。
- 对象文件写入在 writer 外完成，因此大型导入不会阻塞译文的 FULL 耐久事务。
- 启动时清理进程中断留下的 `.tmp` 对象；已完整发布但未引用的对象只由保守 GC 回收。
- 备份在同一个 writer 命令窗口内先用权威事务固定提交序号、对象闭包和 backup lease/root pin，再在释放 writer 前建立对应序号的 SQLite 读快照；随后复制数据库和对象闭包并逐个验哈希。
- 备份从不覆盖已有目标；GC 同时把活动 backup pin 视为根。
- 导出目标路径只由项目根目录和 `export_id` 推导，不接受数据库中的绝对路径；发布通过同文件系统无覆盖链接完成，已有目标和重复记录都失败关闭。
- 产品 GC 固定 24 小时宽限期；扫描在 writer 外，删除前在 writer 内重新检查可达性，每批最多 2000 项或 50 ms。

### 6. 故障注入证据

测试使用当前测试可执行文件启动真实子进程，并在指定窗口直接退出，不依赖 Rust 析构：

- 译文事务提交前退出：恢复后 `commit_sequence = 0`，没有部分修订。
- 事务提交后、确认返回前退出：恢复后 `commit_sequence = 1`，同一 command 重放返回原 receipt，文本不被重放参数覆盖。
- 对象发布后、SQLite 引用前退出：数据库没有悬空引用，完整孤儿对象可被 GC 回收。
- 对象引用提交后退出：数据库引用和对象文件同时存在，GC 不会删除。
- 活动备份 pin 建立后移除普通引用：GC 仍保留该对象；备份完成释放 pin 后才可回收。
- 导出在 Preparing、候选写入、候选同步时退出：恢复只取消并清理暂存，不产生半成品。
- 导出在发布意图、无覆盖发布、Published 记录前后退出：恢复只接受哈希匹配的候选或成品，最终输出一致；损坏成品被拒绝。

原始输入路径另有回归测试，发布源对象前后的内容、长度、权限和修改时间保持不变。

## 模块边界

```text
babel-domain
    ↑
babel-storage
    ↑
babel-application

babel-runtime   # 独立的 DAG / IPC 基础设施，Phase 2 再由上层编排
```

`tools/check-architecture.sh` 和 CI 会拒绝 domain 反向依赖 storage/application/runtime、storage 依赖 application/runtime，以及 runtime 直接依赖 storage/application。格式适配器、UI 和项目数据库之间尚未建立任何直连通道。

## 验证命令

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
./tools/check-architecture.sh
```

当前本地结果：58 项测试通过，Clippy 零警告，格式检查和依赖方向检查通过；独立 recovery 探针覆盖 6 个真实子进程退出点并返回 `SUPPORTED`。

Linux 上的 `x86_64-pc-windows-msvc` 交叉检查停在 SQLite C 构建依赖缺少 MSVC `lib.exe`，因此不计为 Windows 验证。按照既定发布边界，Windows 产物后续由发布闭包生成，本阶段不声称经过 Windows 实机验证。

## 明确未完成

- Phase 2 的资源图、TIR、绑定账本、适配器流式协议和宿主隔离。
- Phase 3 的 TXT 导入、编辑、校验、导出和安装包 E2E。
- 大型真实项目上的查询池、writer 公平性和备份吞吐性能回归。
- 真正断电、磁盘写满、文件系统 I/O 错误和跨平台备份恢复；当前故障矩阵是进程硬退出。
- Windows 本机锁、备份和故障注入复跑；Linux 不替代 Windows 实机结论。

## 用户审查清单

- [ ] 是否接受“编辑、撤销、重做都追加修订，永不原地改写历史”。
- [ ] 是否接受草稿基准变化时必须人工裁决，不能自动覆盖当前译文。
- [ ] 是否接受单项目只允许一个权威 writer，第二个进程只能提示项目已打开。
- [ ] 是否接受源对象先发布、后引用；中间崩溃允许留下可回收孤儿，但绝不允许悬空引用。
- [ ] 是否接受备份目标必须是新目录，核心绝不覆盖唯一可用项目。
- [ ] 是否接受 GC 的 24 小时宽限期、活动备份 pin 和 2000 项/50 ms 批次上限。
- [ ] 是否接受 Phase 1 不提前加入格式私有表，TXT/Markdown/EPUB 必须通过 Phase 2 公共契约进入。
- [ ] 是否要求在进入 Phase 2 前追加真断电、磁盘写满或 Windows 本机故障注入。
