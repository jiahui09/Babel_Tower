# Babel Tower Phase 0 架构证伪报告

日期：2026-08-17

## 结论先行

本轮没有实现产品界面，而是把最可能导致后期推倒重来的五个架构假设做成可运行代码和可重复证据。

| 假设 | 结论 | 证据含义 |
| --- | --- | --- |
| SQLite 可承担 10 万单元项目的权威存储与保存热路径 | 支持 | Btrfs 上 1000 次 `FULL` 耐久保存 p99 为 4.095 ms，完整性检查通过 |
| 单元身份可在大规模重排中安全迁移译文 | 支持 | 10 万单元覆盖全部，误绑 0；100 组重复句自动绑定 0 |
| DAG 与 IPC 可安全支撑 worker 隔离 | 支持 | 2000 节点、1999 依赖边会阻塞未满足的下游；旧 fencing token 被拒；真实本地 socket/named pipe 可用 |
| 导出可在任意关键阶段崩溃后恢复 | 支持 | 6 个子进程硬退出点全部收敛到取消或已发布，损坏产物失败关闭 |
| Windows/Arch 最终产品离线包已闭包 | 未闭包 | Arch 探针真实通过；Linux 只生成 Windows 安装器，不做安装验收，且最终 OCR/桌面资源尚未实现 |

机器证据在 `.omx/phase0/latest.json` 和 `.omx/phase0/package-evidence.json`。前者保留原始微秒样本，不只保留百分位结论。

## 具体做了什么，为什么这样做

### 1. SQLite 热路径

实现了 `babel-storage`：STRICT 表、外键、WAL、`synchronous=FULL`、幂等 command receipt、不可变 revision、独立 draft、keyset 分页和 FTS 脏队列。

保存事务按稳定 `source_unit_key` 寻址，只写权威译文和待索引标记，不更新 FTS。首轮证伪发现逐条刷新 1000 个 FTS 项耗时 15.2 秒，因为每次删除都扫描虚表；改成固定临时批次后，一次删除、一次回填、一次清队列，本次为 79 ms。

10 万单元、1000 次保存结果。数据库明确创建在 `/home` 的 Btrfs，而不是 `/tmp` 的 tmpfs；挂载证据见 `.omx/phase0/filesystem-evidence.txt`：

- 导入 2191 ms，首次 FTS 构建 376 ms。
- 保存 p50/p95/p99：2.055 / 4.050 / 4.095 ms；最大值 12.746 ms。
- keyset 分页 p95：0.105 ms；FTS 查询 p95：0.051 ms。
- 长读事务期间 WAL 增至 44,772,072 bytes；读事务结束后 `TRUNCATE` checkpoint 回到 0。
- `PRAGMA integrity_check = ok`。

因此继续使用 SQLite，不引入 RocksDB 或自研日志。代价也已经看清：后台长读快照必须限时，搜索必须异步批量刷新，不能进入保存事务。

### 2. 稳定身份

实现了版本化 `source_key`，输入包括格式、资源身份、结构路径和规范化内容哈希。重绑定只允许两种自动继承：定位与内容都一致的 `Exact`，或旧内容和新候选都唯一的 `Shifted`。只要新旧任一侧重复，或发生拆分、合并、改写，就不猜测，进入 `Ambiguous/Orphaned`。实际保存 API 也已经改为按 `source_unit_key` 查找单元；回归测试证明本地顺序变化后不会把译文写到另一个单元。

10 万条唯一语料完全反序并改变结构路径后：1 条仍为精确定位，99,999 条安全迁移，误绑 0，耗时 150 ms。另造 100 组重复句，自动绑定为 0；Markdown 有意义空白变化自动绑定也为 0。这证明优先级是“宁可让人确认，也不能静默串译文”。

### 3. 持久化 DAG 与版本化 IPC

DAG 节点和依赖边都存入 SQLite，不依赖进程内内存；有未完成依赖的下游不能 claim，新增边会检查并拒绝环。每次 claim 获得递增 fencing token，租约过期后的旧 worker 即使回来也不能发布结果。2000 个节点、1999 条链式依赖边完成注册、阻塞检查和顺序发布用时 303 ms，数据库重开后仍能读到 Ready，旧 token 被拒。

IPC 使用 protobuf 长度帧、主次版本、session nonce、capability token 和 4 MiB 上限。Linux 真实 namespaced local socket 的 1000 次 4 KiB 往返共 13.34 ms。Windows named pipe 的实现和测试可在 Windows runner 执行；Linux 发布流程不把 Wine 结果当作验收。超大帧在分配前拒绝，错误版本、nonce、capability 均失败关闭。

### 4. 崩溃恢复

导出采用 `Preparing -> PublishIntentRecorded -> Published`。候选文件先写入 staging，`fsync` 文件和目录；数据库记录发布意图后才 rename 到最终路径，再同步目录并标记 Published。

测试启动当前可执行文件，在 6 个阶段直接 `exit(77)`：Preparing、候选写后、候选同步后、意图记录后、最终 rename 后、Published 后。前三种恢复为取消且无最终文件；后三种恢复为 Published 且哈希正确。把已发布文件改坏后，恢复会拒绝继续。

### 5. 双平台离线包

Arch 生成单文件 pacman 包，在新拉取的 `archlinux:base` 容器中以 `--network none` 安装并启动。动态依赖扫描曾发现漏报 `libgcc_s.so.1`，包依赖已补为 `glibc + gcc-libs` 后重测通过。

构建脚本现在为每个平台生成带产物 SHA-256、依赖、组件和探针范围的 `release-manifest.json`。Phase 0 发布门禁会把 Arch 核心探针判为 `SUPPORTED`，把 Linux 交叉构建的 Windows 包判为 `BUILT_UNVERIFIED`；只有 Windows 原生 runner 才能把后者提升为 `SUPPORTED`。最终生产整包另行判定，缺少桌面壳、格式 worker 或 OCR 组件时保持 `FALSIFIED`。

Windows 使用 `cargo-xwin` 生成原生 MSVC PE，使用官方 NSIS 3.12 生成单文件安装器。动态 CRT 最初暴露 `VCRUNTIME140.dll` 外部依赖，已改为静态 CRT；当前只依赖 Windows 系统 DLL。Linux 到此停止，只发布安装器和哈希清单，不执行安装、启动或 named-pipe 验收。

Windows 安装验收由 Windows runner 独立完成；未取得该结果前，Linux 产物只能标记为“已构建、可发布、未验证”。而且这些是 Phase 0 架构探针包，不包含最终桌面壳、生产格式 worker、OCR runtime 和模型，所以最终产品包仍是阻塞项。

## 可复跑命令

```bash
cargo test --workspace
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo run --release -p babel-phase0 -- all --units 100000 --saves 1000 \
  --work-dir .omx/phase0/benchmark-work --release-dir release \
  --output .omx/phase0/latest.json
./packaging/build-arch-phase0.sh
./packaging/build-windows-phase0.sh
```

## 投资/技术审查清单

- [ ] 是否接受 SQLite 继续作为 Phase 1 权威存储，不提前引入第二数据库。
- [ ] 是否接受保存 p99 20 ms、分页 p95 50 ms、搜索 p95 150 ms 作为暂定合成门槛，并要求真实 EPUB/Markdown 语料复测。
- [ ] 是否接受“重复、拆分、合并不自动继承”作为零误绑原则。
- [ ] 是否接受 DAG 的 lease/fencing 和 IPC 的版本/nonce/capability/frame limit 为核心协议不变量。
- [ ] 是否要求导出状态机后续增加真正断电/文件系统故障注入，而不仅是进程硬退出。
- [ ] 是否接受 Arch Phase 0 核心探针包已在断网干净容器安装并启动，但最终产品闭包仍要等格式 worker 与 OCR 组件补齐。
- [ ] 是否把真实 Windows 干净镜像禁网安装列为 Windows 发布流水线硬门禁，并禁止 Linux/Wine 结果替代它。
- [ ] 是否把最终 OCR runtime、模型、格式 worker、许可证和 SBOM 的单包闭包列为 Phase 1 发布前硬门禁。
