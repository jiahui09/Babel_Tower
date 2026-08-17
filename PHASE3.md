# Babel Tower Phase 3：TXT 纵向闭环

日期：2026-08-17

## 阶段结论

Phase 3 的核心实现已经贯通：TXT 源文件进入不可变对象库后，由真实独立 worker 流式探测、清点和抽取，经资源图/TIR 写入权威 SQLite；译文保存、搜索、恢复、校验和冻结导出共用稳定 `unit_id`。10 MB/10 万单元 S 语料的全部性能门槛通过。

阶段退出签发当前仍为 `FALSIFIED`，原因不是核心功能或性能失败，而是本轮受限执行环境不能访问 Docker socket，也不能运行 Wine/NSIS socket。最新 Arch 包已构建并完成本机解包 smoke，但没有重新完成干净禁网容器安装；最新 Windows PE 已交叉构建，但安装器没有用最新二进制重新封装。旧包和旧清单已经移除，避免把历史证据冒充本轮结果。

最终生产桌面整包同样保持 `FALSIFIED`：桌面壳、Markdown、EPUB、OCR 与图片资源链路不属于本阶段产物。

## 技术路线

```text
源文件
  -> 流式 SHA-256 + CAS 原件发布
  -> TXT worker（1 MiB 分块、有界 Protobuf frame、nonce/版本/相关 ID）
  -> ResourceGraph + TIR + 稳定源单元身份
  -> PreparedTxtImport（writer 外准备）
  -> SQLite 单写者（每批 1000 单元，批间让出 P0 保存）
  -> Building 代完整构建
  -> 绑定审查 / seal / 单事务原子激活
  -> 修订、搜索、恢复
  -> 冻结 commit_sequence + 校验 + 暂存哈希验证 + 导出
```

这样安排的原因是把高成本、可失败的解析放在 writer 外，把权威变更收敛到短事务；任何 worker 崩溃、超时、取消或导入中断都不会替换旧活动代。SQLite 继续承担项目真相，不把大源文件或导出文件塞进数据库。

## 关键实现

### TXT 与格式保真

- 产品路径支持严格 UTF-8、UTF-8 BOM、UTF-16LE BOM、UTF-16BE BOM。
- 混合 `LF / CRLF / CR`、BOM 和末尾换行按源定位器保留。
- 无 BOM 的非 UTF-8 输入失败关闭；适配器具备显式 GB18030/GBK 能力，但尚未接入产品导入参数，因此不把它声明为产品支持。
- 空文件不制造虚假单元，二进制 NUL、畸形 UTF-16 和超预算输入会被拒绝。

### 稳定身份与重导入

- 资源图节点绑定不可变源快照；TXT 单元身份不再包含整个文件哈希，而由适配器身份版本、语义路径、结构位置和规范化内容决定。
- 只有 `Exact` 自动继承旧 `unit_id`；唯一同文异位产生 `Shifted`，重复候选产生 `Ambiguous`，无可靠候选产生 `Orphaned`。
- `Shifted / Ambiguous` 保持待审，新代不激活。确认命令携带候选集合哈希并幂等记录；全部解决后显式 seal/activate。
- 测试证明：插行重导入期间旧代继续服务，确认后原子切换，已有译文随原 `unit_id` 保留。

### worker、IPC 与恢复

- worker 不接收任意宿主路径；核心将 CAS 对象按 1 MiB 分块送入会话，worker 重新校验长度和 SHA-256。
- IPC 为 4 MiB 上限的长度前缀 Protobuf envelope，验证版本、nonce、能力、请求相关 ID 和响应上限。
- 超时或取消会 kill + wait；异常退出返回状态与有界 stderr 诊断。测试覆盖错误握手、错相关 ID、超大响应、超时、取消和崩溃。
- SQLite 使用 `WAL + synchronous=FULL + 单写者`；对象先同步后引用，译文命令幂等，启动时暂停中断任务，导出以冻结提交序号重现。

### 有限优化

- worker 每个会话只完成一次全量 TXT 抽取，之后按 2000 单元分页返回，避免每页重复扫描源文件。
- generation 写入按 1000 单元/批提交，批前预解析绑定并预分配 `unit_id`，消除逐行查询；批间最多服务 32 个交互命令。
- 没有引入并行 SQLite writer、零拷贝 IPC、自定义 allocator 或格式专用缓存。当前数据没有证明这些复杂度有必要。

## 验证证据

- `cargo fmt --all -- --check`：通过。
- `cargo clippy --workspace --all-targets -- -D warnings`：通过，零警告。
- `cargo test --workspace --all-targets`：111 项通过。
- `./tools/check-architecture.sh`：依赖方向通过。
- smoke：4 单元导入、搜索、关闭重开恢复、混合换行导出均通过。
- S 语料：10,000,000 bytes / 100,000 units；首次导入 5.198 s（记录项）；冷打开小于 1 ms（记录为 0 ms）；峰值 RSS 92.70 MiB；分页 p95 0.290 ms；保存 p95 2.521 ms；搜索 p95 31.615 ms。

详细原始证据位于 `.omx/phase3/`。

## 发布状态

| 范围 | 状态 | 证据 |
| --- | --- | --- |
| TXT 核心纵向能力 | `SUPPORTED` | 全量测试、smoke、S 语料 |
| 最新 Arch 包本机解包运行 | `SUPPORTED` | SHA-256 `611ae5ce...73f73de`；smoke、ELF NEEDED、worker 与 SBOM 探针 |
| 最新 Arch 干净禁网镜像 | `FALSIFIED` | Docker socket 在当前沙箱被拒绝 |
| 最新 Windows PE | `PE_BUILT_INSTALLER_PENDING` | 应用 `ce97f84f...cd690`；worker `8fa04e5d...4d720` |
| 最新 Windows 安装器 | `FALSIFIED` | 无原生 NSIS，Wine socket 被拒绝 |
| Windows 实机验证 | `BUILT_UNVERIFIED` 规则保留 | 按约定 Linux 不执行 Windows 实机验收 |
| 最终生产桌面整包 | `FALSIFIED` | 桌面壳及后续格式/OCR 尚未进入产物 |

## 用户审查清单

- [ ] 是否接受“CAS 原件 -> worker -> ResourceGraph/TIR -> 短事务代构建 -> 原子激活”作为后续格式统一主链。
- [ ] 是否接受只有 `Exact` 自动继承，位置漂移和重复文本必须进入人工审查。
- [ ] 是否接受新导入代审查完成前继续使用旧活动代，而不是展示半成品。
- [ ] 是否接受 TXT 首版产品编码范围为 UTF-8/BOM UTF-16，GB18030/GBK 必须等显式编码选择进入产品 API 后再声明。
- [ ] 是否接受 1000 单元 writer 批次、2000 单元 IPC 页和 1 MiB 上传块作为当前证据支持的默认值。
- [ ] 是否接受首次导入时间只记录，不与冷打开/保存/搜索热路径门槛混为一谈。
- [ ] 是否接受不在本阶段投入零拷贝 IPC、并行 writer 或自定义内存分配器。
- [ ] 是否同意发布 runner 必须重新执行 Arch 干净禁网容器与 Windows NSIS 构建，成功前阶段退出不得签发。
- [ ] 是否同意 Windows 状态只能由 Linux 标记为 `BUILT_UNVERIFIED`，不能提升为实机 `SUPPORTED`。
- [ ] 是否同意将上述两项包证据缺口延期到 Phase 7 发布硬化，并不阻塞 Markdown/EPUB/OCR 核心开发。
