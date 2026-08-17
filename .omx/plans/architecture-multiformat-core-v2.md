# Babel Core v2：资源图—译文覆层高性能核心架构

状态：架构师与评论员已批准，可进入 Phase 0 架构证伪  
版本：1.0  
日期：2026-08-17  
范围：架构蓝图，不包含源码实现  
关联 PRD：`.omx/plans/prd-multiformat-core-v2.md`  
关联测试：`.omx/plans/test-spec-multiformat-core-v2.md`

## 0. 结论

Babel Tower 首版采用 **SQLite 权威控制面 + 不可变内容寻址对象库 + 版本化资源图与翻译中间表示（TIR）+ 显式持久化增量产物 DAG + 有界原生工作进程 + 快照式原子导出**。

这套架构的核心判断是：

> 项目真相不是某一种格式解析后的 AST，而是不可变源对象、人工译文覆层和源到译文的绑定账本。解析树、OCR、索引、预览和导出计划全部是可重建产物。

TXT、Markdown、EPUB 和未来游戏资源通过同一适配器协议进入核心；但格式私有的往返结构保留为冷产物，不被强行塞入通用数据库模式。首版适配器是随安装包发布、由核心治理的原生组件，不开放任意第三方插件。

“极致性能”在这里意味着：交互热路径与批处理隔离；没有按键触发的全项目工作；大型数据流式、分区、分页、内容寻址；每项性能目标都有参考硬件、语料和发布门禁。它不是未经实现和基准测试的完成声明。

---

## 1. 架构目标与不变量

### 1.1 架构目标

1. **安全**：源文件永不原地修改；人工译文在解析器、适配器、OCR、缓存和导出故障下仍可恢复。
2. **统一**：所有格式共用项目身份、单元状态、修订、撤销、搜索、任务、诊断和导出事务。
3. **可扩展**：新增资源种类只扩展定位器、资源关系和适配器能力，不重建权威存储或编辑器协议。
4. **高性能**：交互路径成本与项目总规模解耦；批任务增量执行且有背压、配额和取消。
5. **可审计**：格式支持、迁移、导出和性能均由清单、版本、哈希、测试和本地诊断证明。

### 1.2 硬不变量

- `Source Object` 不可变，以 SHA-256 标识；原文件与导出结果永远不是同一路径上的原地更新。
- `Translation Revision` 是人工译文事实；搜索索引、当前头、预览和导出中间物不是事实。
- 只有核心单写者能修改权威数据库和推进 `commit_sequence`。
- 任何格式适配器都不能直接读写权威 SQLite，也不能获得任意文件系统权限。
- 单元重新绑定不得静默跨过歧义；顺序相近不等于身份相同。
- 只有完整、验证成功的导入代可以成为活动代。
- 只有完整、验证成功的导出候选可以原子发布。
- 输入、耐久保存、导航和当前上下文不能等待 OCR、全量索引、导入或导出。
- 删除全部派生缓存后，项目仍可从权威事实重新构建。

---

## 2. RALPLAN-DR 决策记录

### 2.1 决策原则

1. **翻译事实优先于格式实现**：人工译文和绑定由核心掌管，适配器只解释和重建格式。
2. **不可变输入，可重建派生**：大对象内容寻址，缓存不承担数据安全责任。
3. **交互优先，工作有界**：所有跨进程消息、查询、内存、并发和队列都有上限。
4. **先内置一致性，后开放生态**：首版先验证契约，再考虑稳定第三方 ABI。
5. **指标决定复杂度**：不因为想象中的规模引入 LSM、自研日志或零拷贝协议。

### 2.2 三个最高决策驱动

1. 适配器升级和源内容变化时，人工译文不能丢失或错绑。
2. 10 万单元和 2 GiB 展开资源下，输入、保存、导航仍需稳定满足交互预算。
3. 新格式必须复用统一项目真相和交付链，不能形成格式孤岛。

### 2.3 可行方案

#### 方案 A：混合权威控制面 + 对象平面 + 显式增量图（采用）

- SQLite 保存关系事实、修订、绑定、任务状态和小型索引。
- 内容寻址对象库保存源文件和大型解析/图像/导出产物。
- 资源图和 TIR 是版本化公共契约；格式私有往返数据独立保存。
- 显式 DAG 记录派生依赖；原生工作进程执行解析、OCR 和导出。

优点：安全边界清楚，适配器可扩展，热数据和冷数据分离，增量与恢复可验证。  
代价：必须认真设计身份、对象垃圾回收、事务边界和跨进程协议。

#### 方案 B：所有数据与解析 AST 全部关系化到 SQLite

优点：事务、备份和查询模型简单；首个 TXT 垂直切片较快。  
缺点：不同格式会把数据库模式推向无穷联合；大型 token/DOM 产生写放大；适配器升级需要重型模式迁移；格式私有往返信息污染核心。  
结论：拒绝作为长期核心，可继续用 SQLite 保存权威小数据。

#### 方案 C：事件溯源 + RocksDB/LSM + 自建索引

优点：高写入吞吐、天然日志历史、可自定义压缩和列族。  
缺点：当前工作负载是单用户、低并发、耐久小写和大量读；LSM 带来压缩、读放大、空间放大、备份与迁移复杂度；搜索、关系约束和一致快照都需自建。  
结论：拒绝首版。只有真实剖析证明 SQLite 单写者在目标硬件上无法满足预算，且瓶颈不能通过批量、缩短事务或冷热分离解决时再提 ADR。

#### 方案 D：每种格式拥有独立项目库和插件工作流

优点：格式团队可以快速独立开发，格式私有能力表达最自由。  
缺点：译文、搜索、撤销、状态、恢复和导出语义分裂；跨格式嵌套资源无法统一；用户最终面对多个产品。  
结论：拒绝，违反产品核心。

### 2.4 最强反方观点

方案 B 的支持者会指出：首版只有 TXT、Markdown、EPUB 和图片，SQLite BLOB/JSON 加少量格式表就能更快上线，资源图、DAG 和适配器协议可能是提前抽象。这个观点成立的部分是：不能一开始实现通用媒体图平台。

本方案的约束性回应：资源图首版只实现当前格式需要的节点和边；DAG 只覆盖有明确失效成本的解析、抽取、索引、OCR、验证和导出计划；不实现通用图查询语言。我们保留跨格式不变量，但严格限制泛化面积。

### 2.5 预演失败（Pre-mortem）

1. **单元身份不稳定**：Markdown 小改动导致大量单元成为孤立译文。  
   早期信号：黄金语料重抽取后 `Exact` 比例低于 99%，或顺序调整产生错配。  
   缓解：身份版本、结构锚点、内容指纹、邻域指纹分层匹配；歧义必须人工裁决；建立变异语料。

2. **SQLite WAL 尾延迟恶化**：后台长读事务阻塞检查点，保存 p99 超预算。  
   早期信号：WAL 持续增长、checkpoint busy、保存延迟与读会话长度相关。  
   缓解：单写者、分页读、禁止跨 `await` 持有读事务、可观测手动检查点、后台快照隔离。

3. **所谓增量仍然全量**：一个译文字符变化触发整本 EPUB 校验或序列化。  
   早期信号：DAG 失效节点数随项目规模线性增长。  
   缓解：投影键、按资源/分区构建节点、变更集合传递、发布门禁检查热路径复杂度。

4. **TIR 变成万能 AST**：每新增格式都修改核心枚举和数据库。  
   早期信号：格式私有属性进入公共 token，核心发版与单个适配器绑定。  
   缓解：TIR 只表达可翻译文本、受保护结构、占位符和引用；私有往返状态放版本化冷产物。

5. **并行导致更慢**：Tokio、Rayon、OCR 和压缩各自占满 CPU/内存。  
   早期信号：输入延迟与后台吞吐同时恶化，RSS 超预算，频繁换页。  
   缓解：中央资源治理器、交互保留通道、有界工作池、任务内存令牌、OCR 独立进程和自适应并发。

6. **格式支持宣传超过事实**：不常见 EPUB/Markdown 结构导出后损坏。  
   早期信号：适配器清单声明 A 级但缺少相应黄金/差分/规范测试。  
   缓解：支持矩阵从清单与测试结果生成；未覆盖结构自动降级或阻止导出。

7. **对象库泄漏或误回收**：多次导入/导出后磁盘无限增长，或 GC 删除仍被快照引用的对象。  
   早期信号：不可达对象比例持续升高、备份恢复缺对象。  
   缓解：保守标记清扫、代际保留、租约/活动快照根、先 dry-run 清单、故障注入。

8. **首版范围被资源编辑器吞噬**：图片排版消耗核心交付时间。  
   早期信号：高级修补功能阻塞文本链路发布。  
   缓解：首版只做 OCR 修正、区域顺序、基本字体布局和预览；专业图像编辑明确后置。

---

## 3. 总体系统形态

```text
┌──────────────────── Babel Desktop / Tauri 2 ────────────────────┐
│ React 工作台：长文 / 单元 / 资源（同一 project_id + unit_id）   │
│ 命令、查询、订阅；不直接访问项目文件、SQLite 或适配器            │
└─────────────────────────────┬────────────────────────────────────┘
                              │ 有类型的 Core API
┌─────────────────────────────▼────────────────────────────────────┐
│                         Babel Core (Rust)                        │
│ 会话与命令 │ 单写者 │ 查询投影 │ 撤销 │ 调度 │ 迁移 │ 诊断       │
├──────────────┬───────────────┬──────────────┬────────────────────┤
│ SQLite 权威面│ 不可变对象库   │ 增量产物 DAG │ 导出事务协调器      │
├──────────────┴───────┬───────┴──────────────┴────────────────────┤
│ 资源图 + 绑定账本 + TIR│ 格式注册表 / 能力清单 / 支持等级         │
└───────────────────────┬───────────────────────────────────────────┘
                        │ Protobuf 控制流 + 对象句柄数据流
┌───────────────────────▼───────────────────────────────────────────┐
│ 有界工作进程：TXT/Markdown/EPUB/图片 │ OCR │ 预览/导出/验证       │
│ 首版内置、版本锁定、无数据库权限；未来可增加 WASI Component Host │
└───────────────────────────────────────────────────────────────────┘
```

### 3.1 六个责任平面

1. **不可变源对象平面**：源文件、归档成员、大型 AST、原图、OCR 模型输出、预览和导出候选。
2. **权威译文覆层平面**：项目身份、单元、译文修订、状态、术语、批注、命令结果和导出记录。
3. **绑定与资源图平面**：资源之间的关系、源定位器、阅读顺序、单元身份和重新映射结果。
4. **增量派生平面**：解析、抽取、搜索索引、缩略图、OCR、校验和导出计划的依赖图。
5. **适配器执行平面**：内置格式实现和 OCR 等高风险/高成本工作，以进程隔离并接受资源配额。
6. **交付平面**：冻结快照、计划补丁、暂存写入、验证、同步、原子发布和清单记录。

---

## 4. 项目真相与存储

### 4.1 项目目录

```text
project.babel/
├── project.sqlite3
├── objects/
│   └── sha256/ab/cdef...
├── staging/
│   ├── import/<generation-id>/
│   └── export/<export-id>/
├── runtime/
│   ├── sockets/
│   └── leases/
└── diagnostics/
    └── local-traces/
```

`runtime` 与 `diagnostics` 可丢弃；`staging` 可在事务状态指导下恢复或清理。备份只需 SQLite 一致快照和由活动项目根可达的对象闭包。

### 4.2 SQLite 职责

SQLite 只保存适合事务、关联和分页查询的权威小数据：

- `project`、`project_schema_version`、`active_generation`
- `source_snapshot`、`resource`、`resource_edge`
- `unit`、`unit_source_binding`、`unit_head`
- `translation_revision`、`translation_state_transition`
- `term`、`term_variant`、`annotation`、`marker`
- `command_receipt`、`undo_group`
- `task_record`、`artifact_record`、`artifact_dependency`
- `export_record`、`migration_record`、`diagnostic_event`

约束：

- 使用 `STRICT` 表和外键；稳定哈希/标识用定长 BLOB，不使用 UUID 文本做热连接。
- 采用 WAL、`synchronous=FULL`、单写者 actor、短事务和显式繁忙策略。
- UI 查询只取投影与分页，不把整个项目常驻内存。
- 禁止在异步等待、跨进程请求或长时间渲染期间持有数据库读事务。
- 自动检查点关闭或设为保守值，由核心根据 WAL 页数、活动读者和交互负载调度；发布前必须基准确定参数。
- `WITHOUT ROWID` 只在具体表基准证明收益后采用，不能全局套用。

### 4.3 修订模型：追加事实 + 物化当前头

不采用完整事件溯源。每次人工译文变更写入不可变 `translation_revision`，`unit_head` 在同一事务中指向当前修订。命令包含全局唯一 `command_id`，`command_receipt` 保证进程重试幂等。

撤销/重做通过显式命令产生新修订并记录 `undo_group`，不删除历史。保存确认只在事务耐久提交后发送。这样既保留恢复和审计能力，也避免用通用事件回放重建整个项目。

### 4.4 不可变对象库

对象发布协议：

1. 在同文件系统暂存目录流式写临时文件，同时计算 SHA-256 和长度。
2. `fsync` 文件并校验声明长度/内容类型上限。
3. 按哈希确定最终路径；已存在则验证长度后复用。
4. 使用不覆盖的原子重命名发布并同步父目录。
5. 只有发布成功后，SQLite 事务才能引用该对象。

对象内部可在后续 ADR 中加入 BLAKE3 辅助指纹以加速缓存键，但持久外部身份保持 SHA-256。大型解析产物按资源或分区切块，避免单对象小改导致整体重写。

### 4.5 垃圾回收

采用保守标记清扫：项目活动代、保留迁移快照、导出记录、备份租约和运行中任务为根。GC 先生成 dry-run 清单并经过宽限期；活动导入/导出/备份持有租约。首版不做实时引用计数，避免崩溃窗口导致误删。

---

## 5. 资源图、定位器和翻译中间表示

### 5.1 资源图

资源图描述“作品由什么组成、互相如何引用”，不是通用图数据库。

首版节点：

- `Container`：EPUB/ZIP 等容器。
- `Document`：TXT、Markdown、XHTML、OPF、NCX。
- `TextStream`：可按阅读顺序抽取的文本流。
- `Image`：位图或可栅格化资源。
- `ImageRegion`：图片中的文字区域。
- `Font`、`Stylesheet`、`BinaryAttachment`。

预留节点：`AudioTrack`、`VideoTrack`、`SubtitleTrack`、`TimelineRegion`。预留只定义身份与定位能力，不承诺首版解析或导出实现。

首版边：`Contains`、`References`、`ReadingOrderAfter`、`DerivedFrom`、`RegionOf`、`AlternateOf`。未来可添加 `TimedTo`、`FrameOf`，未知边必须可被旧核心保守忽略或明确拒绝。

### 5.2 版本化定位器

```text
Locator =
  ByteSpan(object_hash, start, end)
  | StructuralPath(resource_id, path_segments, attribute?)
  | TextRange(resource_id, node_key, grapheme_start, grapheme_end)
  | SpatialRegion(resource_id, polygon, coordinate_space)
  | TemporalRange(resource_id, start_ns, end_ns)
  | FrameRange(resource_id, start_frame, end_frame, timebase)
  | OpaqueAdapterLocator(adapter_id, schema_version, bytes_hash)
```

公共定位器只表达跨格式稳定的空间、时间和结构概念。语义导出仍由具体适配器负责；增加音频/视频并不意味着只添加定位器就能完成支持。

文本边界按 Unicode UAX #29 的扩展字素簇处理，避免光标和范围切断组合字符。源字节范围用于差分写回，展示范围使用标准化文本投影，两者不能混为同一偏移。

### 5.3 TIR 的边界

TIR 只表达译者需要看见或保护的内容：

```text
UnitContent = [Token]
Token =
  Text(text, style_hint?)
  | ProtectedOpen(tag_key, display_hint?)
  | ProtectedClose(tag_key)
  | ProtectedAtom(atom_key, display_hint?)
  | Placeholder(name, validation_rule)
  | Break(kind)
  | Reference(resource_id, relation)
```

不变量：

- `ProtectedOpen/Close` 必须正确嵌套，标识在译文中不可伪造或重复。
- 占位符可重排与否由规则声明；丢失、重复或类型变化进入校验问题。
- 文本可以编辑，受保护 token 只能通过受控命令移动或删除。
- TIR 不保存完整 CSS、XML 命名空间、ZIP 元数据或游戏引擎内部对象；这些属于适配器私有往返产物。

### 5.4 单元身份和绑定账本

标识层次：

- `object_hash`：源字节事实，SHA-256。
- `resource_key`：`source_snapshot + adapter_id + adapter_identity_version + semantic_path`。
- `source_unit_key`：适配器稳定键，结合结构定位、内容指纹和邻域指纹。
- `unit_id`：项目内永久、不复用的 128 位随机标识。
- `binding_id`：某一活动代中 `source_unit_key/locator -> unit_id` 的版本化映射。

重新抽取不是覆盖 `unit`，而是创建新 `generation_id` 并计算绑定差异：

- `Exact`：强身份与规范化内容均符合，可自动继承。
- `Shifted`：内容/结构可信但定位移动，需在差异界面可审阅。
- `Ambiguous`：多个候选相似，禁止自动选择。
- `Orphaned`：旧单元无新来源；译文保留在项目历史中。

每个适配器必须提供版本化 `SourceUnitIdentitySpec`：

```text
SourceUnitIdentitySpec {
  identity_version
  normalize(text, structure) -> canonical_bytes
  strong_key(structure, canonical_bytes) -> optional_key
  neighborhood_key(previous, next, reading_order) -> optional_key
  candidate_policy(max_candidates, max_distance)
  auto_inherit_policy   # 首版仅 Exact；Shifted 默认不继承
}
```

规范化规则必须明确 Unicode 规范化、扩展字素边界、空白、大小写、标点、受保护 token 和结构标签；不得由实现者临时决定。匹配按固定阶段执行：完整源对象/规范化内容哈希、唯一结构键、唯一内容加邻域键，最后才生成有限候选。候选分数相同或低于阈值一律进入 `Ambiguous`，不以当前位置或顺序作为唯一决定。

匹配器输出候选、分数、规范化哈希、结构/邻域证据和策略版本。人工裁决作为不可变 `binding_decision` 命令写入绑定账本，包含用户选择、前置代、候选集合、理由代码和可撤销关系；后续任务必须尊重已裁决结果，不能静默覆盖。任何启发式升级必须用变异语料回放，并能够预览结果后再激活新代。

---

## 6. 格式适配器协议 v2

### 6.1 适配器不是插件数据库

适配器是无状态或显式状态的转换组件，接收对象句柄、限定配置和任务预算，返回资源描述、TIR 块、绑定候选、问题或声明式补丁计划。它不能拥有项目真相、不能决定 UI 路由，也不能直接提交译文。

### 6.2 能力清单

```text
AdapterManifest {
  adapter_id
  adapter_build
  protocol_range
  identity_version
  supported_signatures / mime_types / extensions
  resource_kinds
  import_capabilities
  export_fidelity_tier
  patch_granularity
  private_artifact_schema_versions
  deterministic_stages
  safety_limits
}
```

文件扩展名只参与提示，`probe` 必须检查签名字节/结构并在预算内返回置信度。UI 支持矩阵由清单和最近契约测试结果共同生成。

### 6.3 流式协议

```text
probe(input_handle, budget) -> ProbeResult
inventory(snapshot_handle, cursor, budget) -> ResourcePage
partition_plan(snapshot_handle, budget) -> PartitionPlan
extract_partition(partition_handle, cursor, budget) -> TirChunk
validate_overlay(snapshot_ref, changed_set, cursor, budget) -> IssuePage
plan_export(snapshot_ref, changed_set, budget) -> ExportPlanHandle
materialize(export_plan_handle, staging_handle, cursor, budget) -> Progress
verify_output(candidate_handle, budget) -> VerificationReport
```

规则：

- 所有可变长度结果分页或流式返回；调用支持取消、deadline、最大字节数和最大节点数。
- `probe/inventory/extract/plan_export` 在相同输入、版本和配置下必须确定性输出。
- `materialize` 和对象发布等副作用必须幂等，但绝不能被 DAG 当作纯计算结果跳过。
- 适配器私有产物由 `(adapter_id, schema_version, object_hash)` 标记；核心不解释其内容。
- 嵌套资源通过适配器链处理，但上层格式束负责规范一致性。例如 EPUB 适配器编排 ZIP、XML/XHTML 和图片能力，通用 ZIP 适配器不能宣称 EPUB 语义。

### 6.4 首版内置适配器

- TXT：编码探测需保守，保存原换行与 BOM 策略；无法无歧义判断编码时询问用户。
- Markdown：以 CommonMark 0.31.2 为语法基线，保护代码、链接目标、HTML 块等不可翻译结构；声明扩展子集。
- EPUB 2/3：以 EPUB 3.3 与 W3C EPUB Test Suite 为主要基线，同时覆盖 EPUB 2 导航/OPF；保持容器成员、媒体类型、清单、spine 和引用一致性。
- 图片：解码、区域、OCR 识别结果、人工译文和衍生渲染分层；不修改原图。

### 6.5 未来第三方边界

只有当内置适配器协议经过至少两个版本和多种格式验证后，才评估公共 SDK。候选运行面为 Wasmtime + WASI Component Model/WIT：默认无网络、无数据库、无任意目录访问，只授予对象读、暂存写、日志、时钟和有限随机数等显式能力。宿主设置燃料、epoch 中断、内存/表/实例限制和输出配额。

公共插件开放必须另立 ADR，包含 ABI 演进、签名、撤销、许可和恶意输入策略。首版不以此为依赖。

---

## 7. 持久化增量产物 DAG

### 7.1 目的

DAG 管理可重建结果，不管理人工译文。它回答“哪些派生结果因为哪些输入变化而需要重算”，从而避免一次编辑触发全项目解析、索引或导出准备。

### 7.2 产物键

```text
ArtifactKey = hash(
  stage_id,
  stage_schema_version,
  stage_build_id,
  input_object_hashes,
  adapter_id + adapter_build,
  normalized_config_hash,
  dependency_output_hashes,
  platform_semantics_if_relevant
)
```

产物记录状态为 `Missing/Queued/Running/Ready/Failed/Stale`。失败结果可带退避时间，但不把错误永久缓存为成功。实现逻辑发生变化必须提升 `stage_build_id`；CI 检查 stage 源码指纹与声明版本，防止旧缓存误复用。

每个 `Running` 节点持有带过期时间的 lease、heartbeat 和 fencing token。同一 `ArtifactKey` 只有一个 owner；其他任务等待或复用已发布结果。只有对象发布成功、对象哈希校验完成且 SQLite 元数据事务提交后，节点才能变为 `Ready`。旧 worker 使用过期 token 写入会被拒绝；启动恢复会把无 heartbeat 的节点标记为 `Stale`。

### 7.3 投影防火墙

借鉴增量编译的查询投影思想：下游依赖尽量读取最小稳定投影，而不是整个上游对象。

- 术语校验依赖 `UnitTextProjection(unit_id, source_hash, translation_head_hash)`，不依赖 EPUB 完整 AST。
- 缩略图依赖图像对象和渲染配置，不依赖文本译文。
- EPUB 某 XHTML 的导出补丁依赖该资源内变更单元集合，不依赖全书所有单元正文。
- 项目进度依赖状态计数增量，不扫描全部单元。

重新计算后若产物内容哈希不变，下游保持 `Ready`，阻断无意义失效传播。

### 7.4 纯计算与副作用

可缓存纯阶段：探测、清单、解析、抽取、绑定候选、索引分片、OCR 识别、校验和导出计划。  
不可缓存副作用：SQLite 提交、对象发布、文件同步、导出最终发布、备份落盘。副作用通过幂等命令和状态机恢复。

### 7.5 导入代事务

1. 核心创建不可见 `generation_id` 和任务图。
2. 适配器流式清点与分区抽取；核心分批提交资源、边、TIR 句柄和绑定候选。
3. 批次提交只对该代可见，避免单个超大事务和内存聚合。
4. 完成后运行结构、引用、身份唯一性和对象可达性验证。
5. 单个短事务切换 `active_generation`。
6. 旧代保留到撤销窗口/备份完成，未激活代可安全清理。

首批内容可在导入进度视图中只读预览；正式编辑以活动代为边界。首版选择整代激活，避免局部活动代带来的绑定与导出复杂度。

---

## 8. 并发、调度与资源治理

### 8.1 工作等级

| 优先级 | 工作 | 服务约束 |
| --- | --- | --- |
| P0 | 输入、保存、撤销、导航、当前单元 | 保留执行槽与数据库写配额，不可被批任务排队阻塞 |
| P1 | 可见页、当前上下文、交互搜索 | 短任务、可取消、分页 |
| P2 | 当前资源校验、缩略图、预览 | 有界并发，失焦可降级/取消 |
| P3 | 导入、全量索引、OCR、导出、GC、备份 | 后台配额，接受暂停与背压 |

使用加权公平队列而不是严格优先级，避免 P3 永久饥饿；但至少保留一个 P0/P1 执行通道。

### 8.2 中央资源治理器

- Tokio 负责 IPC、文件流和任务编排；CPU 密集工作进入有界 Rayon 池或独立进程。
- 总 CPU 令牌由核心统一分配，默认后台并发不超过 `max(1, logical_cpu - 2)`，具体值由平台基准校准。
- 每个任务声明预估内存、临时磁盘、CPU 类和可抢占点；无令牌不得启动重型分区。
- OCR 进程延迟加载模型，有独立 RSS 上限；空闲超时后可退出释放内存。
- 队列使用信用/背压，生产者不能无限积累未消费的 TIR、OCR 区域或日志。
- 取消是协议能力；适配器必须在页/块边界检查取消和 deadline。

### 8.3 SQLite 单写者

所有权威写入经单写者 actor 串行化，并按命令优先级排队。P3 导入/索引/任务提交每批最多 2,000 行且事务墙钟时间不超过 50 ms，达到任一上限就提交或让出 writer；实际阈值由 Phase 0 基准校准但不得取消上限。P0 保存命令插入下一批边界前的队首，必须先完成译文修订和 `FULL` 同步；FTS、统计和大型派生索引不得与 P0 修订耐久提交绑定在同一长事务。读连接池只用于短快照查询，checkpoint 由 WAL 页数、活动读者和 writer 队列共同调度。

---

## 9. IPC 与进程隔离

### 9.1 传输

- Linux 使用 Unix Domain Socket，Windows 使用 Named Pipe。
- Linux endpoint 使用随机项目会话路径和 `0600` 权限；Windows Named Pipe 使用当前用户 SID ACL，禁止低权限/跨用户连接。
- 消息为 32 位长度前缀 + Protocol Buffers envelope。
- 控制协议与 SQLite 存储模式完全分离；不得把数据库行结构直接生成 IPC 消息。

### 9.2 Envelope

```text
Envelope {
  protocol_major
  protocol_minor
  request_id
  command_id?
  project_id
  trace_id
  deadline_monotonic_ns
  oneof body
}
```

Core 启动时生成一次性会话 nonce；握手必须证明 nonce、协议身份和请求方角色。worker 使用短期 capability token，token 限定项目、对象范围、staging 目录和过期时间，不能只凭 `project_id` 授权。删除字段必须保留 tag；不复用枚举值；新增字段保持可选和安全默认。

### 9.3 有界数据流

- 初始最大 frame：4 MiB；普通结果页目标不超过 512 KiB 或 256 个单元，先到者为准。
- 大型对象只传 `ObjectHandle(hash, length, media_type, allowed_ranges)`，不把 EPUB、图片或模型字节塞进 Protobuf。
- 流使用序号、结束标记、信用窗口和内容哈希；乱序、重复、超额或哈希不符立即终止任务。
- OCR 旧方案中的长度前缀 JSON 被此协议替代，避免第二套不可演进协议。

所有初始上限均是安全默认，必须在不放宽内存上限的前提下通过基准调整。

### 9.4 故障域

解析/OCR/导出工作进程崩溃时，核心记录任务失败、回收租约和临时目录，权威事务不受影响。连续失败触发熔断并把问题翻译成用户可行动的诊断；不得自动无限重试恶意输入。

---

## 10. 查询、搜索与编辑热路径

### 10.1 UI 查询模型

核心提供任务导向投影，不暴露数据库：

- `get_workspace_page(project, mode, anchor_unit, window)`
- `get_unit_context(unit_id, before, after)`
- `search_units(query, filters, cursor)`
- `get_resource_context(unit_id)`
- `get_validation_issues(scope, cursor)`
- `subscribe_project_changes(after_commit_sequence)`

返回值包含 `commit_sequence`。UI 若收到更旧序列的异步结果必须丢弃，避免切换页面后旧查询覆盖新状态。

### 10.2 搜索

- FTS5 是可重建派生索引，原文、当前译文和批注按分片更新。
- 术语精确匹配、规范化去重和重复句使用专用规范化哈希/B-tree，不强行通过全文索引解决。
- CJK tokenizer 策略先用真实中日文语料比较：索引体积、单字/双字召回、短查询延迟和增量更新成本。未完成原型前不承诺自定义 tokenizer。
- 搜索结果只返回必要摘要、匹配范围和 `unit_id`，正文按需取。

### 10.3 编辑命令

UI 发送语义命令而非整页文档：`ReplaceTextRange`、`InsertToken`、`MoveProtectedToken`、`SetTranslationState`、`ApplyTerm`、`AddAnnotation`。核心校验基于版本的前置条件；冲突返回当前头与可重放建议。

按键到画面由前端本地编辑状态完成，不等待磁盘；耐久保存独立显示。无论项目大小，单次输入不得触发全项目状态计算、全文序列化或全索引刷新。

### 10.4 草稿与未提交编辑

前端本地编辑状态不是“永不丢失”的权威事实。为闭合模式切换、核心断线和窗口关闭窗口，核心提供 `DraftSession`：

```text
DraftSession {
  project_id
  unit_id
  client_session_id
  base_revision_id
  draft_sequence
  patch_buffer_object?
  last_acknowledged_command_id?
  expires_at
}
```

小草稿以内存和短期 SQLite session 表保存；超过阈值的 patch buffer 进入 CAS 临时对象。草稿不推进 `commit_sequence`，不进入导出快照，也不能覆盖已确认修订。模式切换只转移同一 `DraftSession`；关闭项目时核心先尝试耐久提交，失败则把草稿和基准修订写入恢复候选。启动恢复按 `base_revision_id` 重放；若当前头已变化，必须显示差异并由用户选择，不自动覆盖。草稿过期后进入宽限期清理，但在清理前可从恢复界面取回。

---

## 11. 安全导出事务

### 11.1 状态机

```text
Requested -> SnapshotFrozen -> Validated -> PlanReady
  -> Materializing -> CandidateVerified -> PublishIntentRecorded
  -> Synced -> Published -> RecordCommitted

任一阶段 -> FailedRecoverable / FailedTerminal
```

### 11.2 流程

1. 冻结 `commit_sequence`、活动 `generation_id`、适配器版本和所需对象集合。
2. 只针对变更集合运行公共校验与格式校验；高危全局约束仍运行全局投影检查。
3. 适配器生成声明式 `ExportPlan`：目标资源、前置哈希、补丁、未改对象复用和期望结果约束。
4. 核心验证计划不会触碰源路径、越界路径、未知对象或超出能力等级的结构。
5. 工作进程流式写同文件系统暂存候选。ZIP/EPUB 可能需要顺序重写整个容器，但未变成员直接复用原 payload，不重新解析/序列化其语义。
6. 运行容器结构、引用、清单、TIR 映射覆盖、占位符和适配器专用验证。
7. 在发布前以 P0 事务写入 `publish_intent`：目标路径、候选哈希、快照、计划哈希、临时路径、覆盖策略和会话 nonce。
8. 同步候选文件和父目录；以不覆盖或显式用户选择的策略原子发布。
9. 发布后通过幂等事务将 intent 标记为已发布并写入 `export_record`：快照、计划哈希、适配器构建、结果 SHA-256、验证摘要和路径。

用户在导出期间继续翻译不影响本次结果，下次导出会基于新快照。失败候选保留有限时间用于诊断，但不会成为正式结果。

---

## 12. 迁移、备份与恢复

### 12.1 版本维度

- `project_schema_version`
- `core_protocol_version`
- `resource_graph_version`
- `tir_version`
- `adapter_build`
- `adapter_identity_version`
- `private_artifact_schema_version`

不能用单个“项目版本”掩盖不同兼容边界。

### 12.2 数据库迁移

- 打开项目先做只读兼容检查；需要迁移时先生成 SQLite 一致备份和对象可达清单。
- 迁移使用小步骤、事务和完成标记；大表迁移支持影子表/分批回填，不能长时间阻塞 UI 而无进度。
- 适配器私有产物不做昂贵就地迁移，优先按新版本重建。
- 身份算法升级通过新导入代和绑定差异审阅，不原地重写旧绑定证据。

### 12.3 备份

备份执行 `backup_snapshot_begin`：在同一权威事务中固定 `commit_sequence`、创建 backup lease/root pin 并记录可达根；事务提交后再使用 SQLite Backup API 复制数据库快照和对象闭包。复制期间 GC 必须尊重 pin；所有对象复制并校验完成后才释放 pin。恢复先在新目录验证数据库、对象哈希和版本，再切换项目入口；绝不覆盖唯一可用项目。

### 12.4 崩溃恢复

- 已提交数据库但对象未引用：对象由后续 GC 处理。
- 暂存对象未发布：删除或按任务恢复。
- 导入代未激活：旧代继续使用，新代可恢复/清理。
- 导出候选已同步未发布：重新验证后允许用户继续发布。
- `publish_intent` 已写但 `export_record` 未写：启动扫描目标路径/候选哈希，完成幂等对账；不匹配则标记为待裁决，不猜测发布成功。
- `command_id` 已有 receipt：返回原结果，不重复写修订。

---

## 13. 性能模型与发布预算

### 13.1 参考环境

- 4 核 8 线程、16 GiB RAM、NVMe SSD。
- Windows 11 与 Arch Linux 使用发布时冻结的 OS 镜像/快照、内核/构建号和固件记录；兼容性仍覆盖当前支持版本。
- 发布构建、冷/热缓存按测试规范分别测量；测试机无其他重型任务。

### 13.2 参考语料

- S：10 MiB TXT，约 10 万行/单元。
- M：100 MiB Markdown 项目，25 万单元，2,000 张关联图片。
- L：500 MiB EPUB 压缩包，5,000 个成员，展开 2 GiB，10 万翻译单元。
- I：200 张 2000×3000 图片、每张 20 个文字区域，用于 OCR/资源调度。

语料生成器和固定哈希纳入测试资产；另保留去标识的真实结构语料，避免只优化合成数据。

### 13.3 预算

| 指标 | 发布预算 |
| --- | --- |
| 输入到画面 | p95 ≤ 16.7 ms，p99 ≤ 33 ms |
| FULL 耐久保存确认 | p95 ≤ 300 ms，p99 ≤ 750 ms |
| 热/冷模式切换 | p95 ≤ 100/250 ms |
| 可见页查询 | p95 ≤ 50 ms |
| 10 万单元搜索首屏 | p95 ≤ 150 ms |
| 已导入 10 万单元冷打开至可编辑 | p95 ≤ 3 s |
| S/M/L 首批可翻译内容 | ≤ 1/3/8 s |
| S/M/L 完整导入 | ≤ 3/20/90 s |
| M/L 导出 | ≤ 15/60 s |
| 10 万单元空闲 RSS（无 OCR） | ≤ 400 MiB |
| L 导入峰值 RSS（无 OCR） | ≤ 1.5 GiB |
| OCR 工作进程峰值 RSS | ≤ 1.5 GiB，超过配额受控失败 |
| 空闲 10 秒后 CPU | ≤ 单个逻辑核 1% |

### 13.4 性能硬约束

- 按键、导航、可见页、保存和单元状态更新不得有 `O(project_size)` 工作。
- 不通过降低 `synchronous=FULL`、跳过哈希/验证或允许无界内存来达标。
- 单次 IPC frame、查询页、任务并发和产物块均有上限。
- 微基准出现统计显著且超过 10% 的退化必须审查；宏基准必须同时满足绝对预算。
- 预算变更只能经 ADR，记录硬件、语料、原始结果和产品影响。

---

## 14. 可观测性、安全与隐私

- 所有日志和 trace 默认本地，不上传。
- 诊断记录请求/任务/适配器/阶段/耗时/字节数/队列深度/缓存命中/RSS/检查点结果，但不记录源文或译文正文。
- 结构化错误边界保留内部 cause chain；UI 映射为用户问题、影响和可执行操作。
- 解析器输入视为不可信：路径规范化、防 ZIP Slip、解压比/成员数/深度/尺寸上限、XML 外部实体关闭、图片像素上限、递归深度限制。
- 安装包内组件和 OCR 模型有构建清单与哈希；发布流水线生成 SBOM。第三方许可证必须在选型阶段验收。
- 项目锁区分活动编辑会话与只读恢复；多进程不能同时成为权威写者。

---

## 15. 技术栈落位

| 层 | 选择 | 说明 |
| --- | --- | --- |
| 桌面壳 | Tauri 2 | Windows/Arch 单包，权限面小于通用浏览器壳 |
| 前端 | React + TypeScript + 统一组件库 | UI 只消费核心投影与命令 |
| 核心 | Rust stable | 所有权、并发、格式生态和跨平台部署 |
| 异步/编排 | Tokio | IPC、流和任务协调 |
| CPU 计算 | 有界 Rayon/工作进程 | 受中央配额，不默认无限并行 |
| 权威存储 | SQLite WAL/STRICT/FTS5 | 单写者、FULL 持久性、可备份 |
| 大对象 | SHA-256 CAS | 原件与大型派生冷热分离 |
| IPC | Protocol Buffers | 版本化控制协议；大数据走对象句柄 |
| OCR | 独立本地工作进程 | 模型随包发布、延迟加载、可回收 |
| 未来扩展 | Wasmtime + WIT（非首版） | 能力隔离后再开放第三方适配器 |

不采用 RocksDB、FlatBuffers、Cap’n Proto 或 Salsa 作为首版基础设施。若剖析证明 Protobuf 反序列化在完成分页与对象句柄优化后仍占热路径主要成本，才比较零拷贝协议；若 SQLite 实测失败，再评估存储替代。Salsa 可用于进程内实验性计算，但不能成为跨进程权威依赖图。

### 15.1 单文件离线分发边界

“单个安装包”按平台解释为一个用户下载产物，而不是跨 Windows/Linux 的同一二进制文件：

- **Windows**：Tauri 安装器使用 `webviewInstallMode.type = "offlineInstaller"`，把 Microsoft WebView2 Evergreen 离线安装器嵌入同一个 Babel Tower 安装器。禁止默认的联网 `downloadBootstrapper`、仍需联网的 `embedBootstrapper` 和不检查运行时的 `skip`。若未来选择 `fixedVersion`，必须承担 WebView2 安全更新并另立 ADR；首版不采用。发布流水线对安装器内全部 EXE/DLL 和解包后的 worker/OCR/适配器执行 PE 静态导入枚举，并在干净镜像通过系统加载事件采集运行时实际加载集合；两者都必须收敛到“包内文件 + 嵌入式 WebView2 安装结果 + 版本化 Windows 系统 DLL/API-set 白名单”。MSVC/UCRT 等非白名单运行时必须应用本地携带或嵌入官方离线可再发行组件，任何未声明 DLL 都阻塞签发。
- **Arch Linux**：发布单一自包含 AppImage，随应用捆绑 WebKitGTK 4.1 及 Tauri 应用所需运行库、内置 worker、OCR runtime/模型和资源。构建使用冻结且可复现的最旧支持基线，声明最低内核、glibc/系统 ABI、图形会话和驱动要求；生成 ELF/动态加载依赖闭包并与外部 ABI 白名单比较。发布渠道必须保留可执行位；若渠道不能保留，则单一下载产物采用保留 Unix mode 的签名 `.tar.zst`，其中只有已签名 AppImage 与校验清单。用户通过图形归档工具解包并启动，不需要运行 `pacman`、`chmod` 或命令行。首次启动可在用户目录创建桌面入口，但失败不影响直接运行。
- **共同内容**：TXT/Markdown/EPUB/图片适配器、OCR 模型、必要且许可允许的 CJK 字体资源、许可证、组件版本清单、SBOM 与 SHA-256 全部在产物内。安装与运行不得隐式下载语言包、模型、WebView 或格式组件。
- **更新**：首版手动下载安装新的完整离线包；不依赖在线更新服务。项目迁移先备份再执行，安装程序不得删除用户项目。

Windows 安装器和 Arch AppImage 必须在禁网、无开发工具、无预装应用依赖的干净图形系统镜像验收。两个平台都生成机器可读的静态导入、运行时加载、包内组件与外部 ABI 白名单差集报告；非空未知差集直接失败。Linux 的内核、glibc/基础系统 ABI 和图形驱动是明确的操作系统先决条件，不伪称被 AppImage 替代。

---

## 16. 交付阶段与验收门

### Phase 0：架构适应性原型（2–3 周）

- 建立 S/M/L/I 固定语料和基准工具。
- 验证 SQLite FULL 保存、WAL 检查点、10 万单元分页/搜索。
- 验证 CAS 发布/GC、Protobuf 分页 IPC、分区抽取和资源治理。
- 用 Markdown 结构变异验证身份与绑定算法。
- 从发布流水线的真实下载产物验证最小双平台离线包：Windows 在无可用 WebView2/非白名单运行时的干净镜像用包内组件安装并启动；Arch 验证 AppImage 或签名 `.tar.zst` 的执行位、图形启动和 WebKitGTK 闭包。两者都运行内置 worker、OCR runtime/模型和适配器加载探针，冻结首版外部 ABI 白名单，并在安装前禁网记录全部联网尝试。

退出条件：关键预算无结构性阻塞；不确定项形成 ADR；完成 DraftSession 恢复原型、SourceUnitIdentitySpec 负例、PublishIntent 崩溃对账、backup pin/GC 并发、IPC 未授权连接、DAG fencing 和 writer 批次优先级验证；双平台真实下载产物在干净禁网镜像完成安装、启动和组件加载探针，Windows PE/运行时 DLL 与 Linux ELF/运行时库闭包均无白名单外依赖，Arch 下载件无需 `chmod`。任一项未通过不得进入大规模 UI 或 Phase 1 开发。2–3 周是初始证伪时间盒，可以延长但不能跳过退出条件，不是保证所有指标必然通过的发布日期承诺。

### Phase 1：权威内核与恢复（3–4 周）

- 项目身份、SQLite 模式、修订/命令、CAS、备份、迁移、任务状态机。
- 单写者、查询投影、提交订阅和本地诊断。

退出条件：故障注入证明源文件、译文和对象引用不变量。

### Phase 2：资源图、TIR、DAG 与适配器宿主（4–5 周）

- 资源图/定位器/TIR schema 与验证器。
- 持久化 DAG、调度、IPC、工作进程沙箱边界。
- 通用适配器契约测试工具。

退出条件：模拟适配器可流式导入、失败恢复、重抽取与快照导出。

### Phase 3：TXT 纵向闭环（2–3 周）

- 从导入、长文/单元编辑、搜索、保存、恢复、校验到导出完整贯通。
- 将 Phase 0 的双平台离线包原型升级为 TXT 全链路安装包 E2E。

退出条件：S 语料全部预算和断电恢复门禁通过。

### Phase 4：Markdown 与翻译辅助（4–5 周）

- CommonMark 结构保护、图片引用、术语、历史检索、重复句、批注、查找替换。
- 格式变异与差分导出测试。

退出条件：M 语料、CommonMark 契约、绑定升级和 A/B 支持等级通过。

### Phase 5：EPUB 2/3（5–7 周）

- 容器、OPF、spine、导航、XHTML、样式/字体/图片关系和顺序重打包。
- W3C EPUB Test Suite 选定覆盖、恶意容器限制和阅读器 smoke test。

退出条件：L 语料预算、结构验证、未改成员复用和失败不覆盖通过。

### Phase 6：图片 OCR 与基础嵌字（4–6 周）

- 区域、阅读顺序、识别结果、人工译文、基本排版和衍生图。
- OCR 模型打包、资源治理和隔离故障。

退出条件：I 语料内存/取消/恢复门禁通过，OCR 不影响 P0/P1。

### Phase 7：发布硬化（3–5 周）

- 全量跨平台、离线、迁移、备份恢复、模糊测试、故障注入和性能回归。
- 支持矩阵、SBOM、安装升级与用户诊断。

退出条件：PRD 发布成功标准全部有新鲜证据，未满足项不得包装成“已支持”。

---

## 17. 实施边界与模块建议

```text
crates/
├── babel-domain          # ID、命令、修订、状态，不依赖具体格式
├── babel-storage         # SQLite 单写者、迁移、备份
├── babel-object-store    # CAS、租约、GC
├── babel-resource-graph  # 资源、边、定位器、绑定
├── babel-tir             # Token、验证、投影
├── babel-incremental     # 产物 DAG、失效、缓存
├── babel-scheduler       # 优先级、配额、取消、资源治理
├── babel-protocol        # 独立 Protobuf 协议
├── babel-adapter-host    # 工作进程生命周期与能力清单
├── babel-export          # 快照、计划验证、发布事务
├── adapters/             # txt / markdown / epub / image
└── babel-core-api        # 面向 Tauri 的命令/查询/订阅
```

依赖方向只朝领域内核：适配器依赖协议/TIR/资源类型，不依赖 storage；UI API 不暴露适配器私有类型；storage 不依赖具体格式。用架构测试或 `cargo metadata` 规则阻止反向依赖。

---

## 18. ADR-001：采用资源图—译文覆层混合核心

### Decision

采用方案 A：SQLite 权威控制面、不可变 CAS、资源图/TIR、绑定账本、显式持久化产物 DAG、Protobuf 对象句柄 IPC、内置原生适配器和快照式安全导出。

### Drivers

- 人工译文在格式演进和故障下的安全性。
- 大项目上的可预测交互和有界资源消耗。
- 多格式共享同一产品与交付语义。

### Alternatives

- 全 SQLite AST 单体。
- 事件溯源 + RocksDB/LSM。
- 格式插件拥有独立存储。

### Why

混合方案把强事务用于它擅长的关系事实，把大而冷的对象放入不可变存储；用资源图与 TIR 约束跨格式公共面，同时允许适配器保存私有往返信息；DAG 让批处理增量且可重建。它比单体复杂，但复杂度对应已确认的多格式、数据安全和性能需求。

### Consequences

- 必须维护对象可达性、绑定差异和多个独立 schema 版本。
- 首版开发需要先完成契约、基准和故障注入基础设施，短期功能速度低于直接做 TXT 编辑器。
- 未来格式开发成本集中在适配器与契约语料，而不是改动整个产品。
- SQLite 保持权威存储，除非真实基准触发替换门槛。

### Follow-ups

- ADR-002：CJK 搜索 tokenizer。
- ADR-003：OCR 引擎、模型与许可。
- ADR-004：第三方 WASI 适配器开放门槛。
- ADR-005：对象压缩/pack 与内部快速指纹。

### ADR-006：桌面运行时离线分发

**Decision**：Windows 采用嵌入 WebView2 Evergreen offline installer 的单一 Tauri 安装器；Arch 采用自包含 AppImage。两者都随包提供 worker、OCR runtime/模型、受支持字体资源、许可证、清单和 SBOM。  
**Drivers**：用户要求单文件离线分发、无命令行、干净系统可安装/启动；同时避免首版自行维护固定 WebView2 安全补丁。  
**Rejected**：联网 bootstrapper（断网失败）、`skip`/依赖预装 WebView（承诺不闭合）、Windows fixed runtime（应用团队承担浏览器安全更新）、Arch 原生包仅声明 pacman 依赖（需要额外安装步骤）。  
**Consequences**：安装包显著增大；Linux 仍需声明不可捆绑的内核/glibc/驱动基线；发布流水线必须做双平台禁网镜像测试和组件漏洞跟踪。  
**Evidence gate**：签发产物在网络命名空间阻断条件下完成安装、首次启动、OCR 和 TXT/Markdown/EPUB 导出，并证明没有子安装器或运行时访问网络；Linux 同时证明静态/动态依赖闭包不超出白名单，且从实际下载产物开始无需命令行即可启动。

---

## 19. 团队编制与执行建议

| 责任 | 建议人数 | 首要产物 |
| --- | ---: | --- |
| 核心/存储负责人 | 1 | SQLite、修订、CAS、恢复、迁移 |
| 格式与交付负责人 | 1 | TIR、适配器、绑定、导出事务 |
| 性能/运行时工程师 | 1 | DAG、IPC、调度、基准、故障注入 |
| 桌面/编辑体验工程师 | 1–2 | 三视图、编辑器、查询投影、状态反馈 |
| 质量工程师 | 1 | 契约语料、跨平台 E2E、模糊/混沌测试 |
| 产品设计/译者研究 | 0.5–1 | 工作流验证、支持矩阵、用户语言 |

若资源受限，可由核心负责人兼性能，但格式/交付与 UI 不宜由同一人同时主责。OCR 高级排版不得挤占 TXT→Markdown→EPUB 主链。

### 19.1 持久目标建议

1. `G1-kernel-truth-safety`：权威存储、修订、CAS、备份恢复。
2. `G2-adapter-contract`：资源图、TIR、绑定和模拟适配器契约。
3. `G3-txt-vertical-slice`：TXT 全链路与安装包。
4. `G4-markdown`：Markdown、图片引用与翻译辅助。
5. `G5-epub`：EPUB 2/3 与安全重打包。
6. `G6-image-ocr`：图片区域、OCR 和基础嵌字。
7. `G7-release-hardening`：跨平台、性能和故障门禁。

每个目标必须绑定对应测试规范条目；未完成依赖目标不得并行修改公共 schema。

### 19.2 并行执行提示

- 适合并行：格式黄金语料、Windows/Arch CI、UI 只读原型、基准工具、OCR 许可证评估。
- 必须串行签发：ID/绑定算法、TIR schema、SQLite 迁移、IPC major 变更、导出发布事务。
- 团队验证路径：实现者提交证据 → 测试工程师复跑目标门禁 → 架构/代码审查确认不变量 → 负责人签发阶段退出。

---

## 20. 架构审查清单（用户预留）

### 产品中心

- [ ] 用户是否始终感觉在翻译同一个对象，而不是进入五个平级子系统？
- [ ] 资源和交付能力是否默认退居翻译工作之后？
- [ ] 翻译辅助是否比系统管理界面拥有更高产品优先级？

### 数据安全

- [ ] 能否删除全部缓存而不丢译文？
- [ ] 源文件、导入代和导出结果是否永不原地覆盖？
- [ ] 崩溃窗口是否逐阶段有恢复或清理规则？
- [ ] 备份是否同时覆盖 SQLite 一致快照和对象闭包？

### 多格式扩展

- [ ] 新格式是否只需实现清单、资源映射、TIR 抽取、验证和导出计划？
- [ ] 格式私有 AST/元数据是否没有渗入核心公共模型？
- [ ] 嵌套资源是否有明确所有权，避免通用 ZIP 与 EPUB 语义冲突？
- [ ] 支持等级是否由契约测试生成并对用户透明？

### 身份与迁移

- [ ] `unit_id`、源定位、适配器版本和身份算法版本是否分离？
- [ ] 重新抽取是否明确区分 Exact/Shifted/Ambiguous/Orphaned？
- [ ] 歧义是否绝不静默继承译文？

### 性能

- [ ] 输入、导航、保存、可见页是否不存在全项目复杂度？
- [ ] 大对象是否流式、分区、分页并通过句柄传递？
- [ ] Tokio、CPU 池、OCR 和压缩是否受同一个资源治理器约束？
- [ ] 性能目标是否有固定硬件、语料、轮次和回归门禁？
- [ ] 是否拒绝用降低持久性、跳过验证或无界内存换性能？

### 协议与扩展

- [ ] IPC 与存储 schema 是否独立演进？
- [ ] 消息、任务、内存、输出和递归是否全部有硬上限？
- [ ] 首版是否避免承诺不成熟的公共插件 ABI？
- [ ] 未来 WASI 扩展是否采用默认拒绝的能力授权？

### 交付

- [ ] 导出是否冻结快照并使用带前置哈希的声明式计划？
- [ ] 候选是否验证、同步后才发布？
- [ ] 失败是否不覆盖旧结果，记录是否可追溯？

### 工程可执行性

- [ ] Phase 0 是否能在 2–3 周内证伪最危险假设？
- [ ] 每个阶段是否有测试规范中的明确退出门？
- [ ] 未决技术选型是否有触发条件，而非无限讨论？

---

## 21. 停止条件

本架构规划在以下条件同时满足后可交给实现：

- PRD、架构和测试规范内容一致。
- 架构师与评论员按顺序给出 `APPROVE`。
- 所有阻塞意见已修订并记录。
- Phase 0 的原型、语料和基准任务足够明确，可以直接排期。
- 没有把尚未实现的性能或格式能力写成已完成事实。
