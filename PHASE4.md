# 第四阶段证据：Markdown 纵向闭环与翻译辅助核心

日期：2026-08-18

## 阶段结论

第四阶段的功能闭环已经完成：Markdown 作为 TXT 之后的第二种真实格式，进入同一套离线核心，复用 ResourceGraph、TIR、稳定身份、SQLite 单写者、保存、恢复、校验、绑定审查和冻结导出链路。

阶段性能门已经通过。同一 Linux 参考环境连续 5 次 100 MiB M 语料完整评估全部为 `SUPPORTED`，完整导入 max 为 19.150 秒，低于 20 秒门槛。优化严格依据阶段计时，未增加并行 SQLite writer，也未重构公共协议或权威数据模型。

完整 NSIS、Arch 干净环境安装、断网安装、升级和卸载验证仍按既定策略留到 Phase 7。

## 已实现

- `crates/babel-markdown-adapter`
  - 使用关闭默认特性的 `comrak 0.54.0`；解析 AST 仅存在于适配器内部。
  - 生成文档、文本流和图片引用资源节点。
  - 将连续可译文本跨度映射为带稳定源键和字节定位的 TIR 单元。
  - 保护代码、链接/图片目标、HTML 和 Markdown 结构。
  - 使用非重叠字节补丁导出，未修改字节原样复制。
- `tools/markdown-worker`
  - 在隔离进程中实现 load、probe、inventory、extract 分页协议。
  - 限制每页单元数并覆盖畸形请求、预览和分页抽取。
- `crates/babel-application`
  - TXT 与 Markdown 统一走格式无关的导入、重导入、绑定审查和导出路径。
  - 导入前校验 worker 报告的 adapter ID、build 和 identity version 与应用内置适配器一致。
  - Markdown 支持人工保存、缺失译文校验、冻结快照导出和重启恢复。
  - generation 写入批次由 1,000 调整为 5,000；保留环境变量控制的分阶段导入计时。
- `crates/babel-storage`
  - schema 升级至 v8。
  - 增加项目术语及变体、批注及过期检测、标记、历史译文查询、重复来源派生查询。
  - 增加先预览、再以单事务提交的精确批量替换；任一 head revision 过期则整体拒绝。
  - 新项目使用 32 KiB SQLite 页，writer/query 分别使用 64/16 MiB 有界页缓存。
  - Building generation 批次使用可重放的 WAL `NORMAL` 提交，并在让出 writer 前恢复 `FULL`；seal、激活和译文写入保持 `FULL`。
- `crates/babel-runtime`
  - 正常关闭 worker 时先关闭 stdin 并等待清理，超时才强制终止，避免临时 CAS 残留占满 `/tmp`。
- `tools/phase4-runner`
  - 提供 Markdown 冒烟闭环和可复现的 M 语料基准。
  - 机器可读最差值保存在 `.omx/phase4/perf-evaluator.json`，5 次原始结果保存在 `perf-evaluator-r5.json` 至 `perf-evaluator-r9.json`。

## 核心技术决策

- Markdown 解析采用 Comrak，不使用行分割器；核心数据库和公共协议不保存 Markdown AST。
- 当前覆层只有一段 `translated_text`，因此首版以连续可译文本跨度作为原子单元，避免跨保护标记的字符串替换。
- 导出不重新序列化全文，而是根据冻结源快照生成字节补丁，再重新解析候选输出并校验保护签名。
- 翻译辅助中的术语、批注、标记是权威事实；历史译文复用既有 revision；重复句保持为可重建派生查询。
- 性能优化集中在 SQLite 页布局、有界缓存、集合式活动代投影和可重放 Building 批次；不增加并行 writer，不削弱活动代、译文、幂等性或导出验证。

## 验证证据

本轮最终签发命令及输出见 `.omx/phase4/qa-report.md`。针对性验证已经通过：

- 应用层：21 个单元测试和 1 个 G2 合同测试。
- Markdown 适配器：5 个单元测试。
- Markdown 工作进程：3 个 IPC 集成测试。
- 存储层：35 个单元测试。
- Markdown 冒烟闭环：7 个可译单元，图片目标和受保护内联结构保持，重启后译文可恢复。
- 独立代码与架构审查：见 `.omx/phase4/final-review.md`。

## M 语料性能结果

语料：100 MiB、250,000 个单元、2,000 个图片引用。下表采用同一配置 5 次评估中的最差值：

| 指标 | 实测 | 门槛 | 结论 |
| --- | ---: | ---: | --- |
| 完整导入 max | 19,150 ms | 20,000 ms | 通过 |
| 冷打开 max | 2 ms | 3,000 ms | 通过 |
| 峰值 RSS max | 305.20 MiB | 400 MiB | 通过 |
| 可见页查询 p95 max | 0.651 ms | 50 ms | 通过 |
| 保存 p95 max | 7.979 ms | 300 ms | 通过 |
| 搜索 p95 max | 91.327 ms | 150 ms | 通过 |

5 次完整导入分别为 19,150、18,410、18,378、18,093 和 17,331 ms。相对首次 49,577 ms 基准，保守 max 降低约 61.4%。多值 `INSERT` 和 96 MiB writer 缓存均经探针证伪后撤回，没有把无效复杂度留在实现中。

## 明确未声明

- 不声明 Windows NSIS 或 Arch 离线安装包已经完成或实机验证。
- 不声明支持 EPUB、OCR、图片嵌字、GFM 或完整 Markdown 编辑器能力。
- 不声明所有 CommonMark 边界均已覆盖；当前是保守的文本和图片引用纵向切片。

## 审查清单

- [ ] 确认 Markdown AST 没有进入公共协议或 SQLite。
- [ ] 确认导出只修改允许的字节范围，链接/图片目标、代码和 HTML 不被静默改写。
- [ ] 确认重导入歧义不会自动继承译文，旧活动 generation 在审查完成前仍可使用。
- [ ] 确认术语、批注、标记、历史和批量替换的权威/派生边界正确。
- [ ] 确认批量替换任一前置 revision 失效时不会部分提交。
- [ ] 确认 5 次 M 语料 max 采用 19,150 ms 的保守签发口径。
- [ ] 确认完整安装包闭包继续留在 Phase 7。
