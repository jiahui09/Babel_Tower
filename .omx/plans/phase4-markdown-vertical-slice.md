# Phase 4：Markdown 生产适配器与翻译辅助

状态：完成；功能、数据安全与 M 语料性能门均已签发
日期：2026-08-18
前置阶段：Phase 3 TXT 纵向闭环核心已完成；最终安装包签发延后到 Phase 7 发布硬化。
关联架构：[多格式核心架构](architecture-multiformat-core-v2.md:332)、[Phase 4 路线](architecture-multiformat-core-v2.md:751)
关联测试：[统一适配器契约](test-spec-multiformat-core-v2.md:167)、[Markdown 专项测试](test-spec-multiformat-core-v2.md:217)

## 1. 目标与边界

### 目标

用第二个真实格式证明：Markdown 可以复用 Phase 3 的 CAS、ResourceGraph、TIR、稳定身份、SQLite 单写者、绑定审查、保存/搜索/恢复、校验和冻结导出，不新增格式专用权威数据库，也不把 Markdown AST 变成公共核心模型。

同时补齐首版最直接的人工翻译辅助：术语提示、历史译文检索、重复来源识别、批注/标记和安全的项目级查找替换。

### 本阶段交付

- CommonMark 0.31.2 基线的受支持 Markdown 子集。
- Markdown 资源图、段落/列表项/标题等可翻译单元、图片引用关系和源字节定位。
- 保护代码、链接目标、图片目标、HTML 和结构 token 的 TIR 映射。
- 非破坏式源字节补丁导出；未修改范围保持原始字节。
- 结构变化后的 Exact/Shifted/Ambiguous/Orphaned 绑定审查。
- Markdown 契约、黄金样本、变异绑定、差分导出、恶意输入和 M 语料性能测试。
- 术语、历史译文、重复句、批注、标记、预览式查找替换的核心命令和查询投影。

### 明确不做

- 不做 EPUB 容器编排、OCR、图片嵌字或完整 Markdown 编辑器 UI。
- 不支持任意 GFM/第三方扩展；扩展必须进入清单和测试后才可声明。
- 不让适配器直接写 SQLite、翻译修订或 UI 路由。
- 不在本阶段反复构建 NSIS/Arch 完整安装包；仅在打包脚本或运行时依赖改变时做跨目标编译探针。
- 不实现自动翻译、自动改写、术语自动替换或未经确认的批量修改。

## 2. 关键技术决策

### 2.1 解析与回写

先实现一个 1 周内可丢弃的解析器探针，比较两个候选：

| 候选 | 优点 | 风险 | 判定条件 |
| --- | --- | --- | --- |
| Comrak | AST、节点级 source position、CommonMark 0.31.2 基线，适合保护嵌套结构 | AST 生命周期/回写能力需要额外映射；版本和 MSRV 需锁定 | 能覆盖样本并稳定取得 UTF-8 字节范围，且不迫使核心理解 AST |
| pulldown-cmark | 事件流轻量，`into_offset_iter()` 提供源字节范围 | 缺少完整可编辑 AST，复杂 HTML/扩展结构需自建状态机 | 在结构保护和段落分区测试中显著低于 Comrak 复杂度 |

探针结论采用 `comrak 0.54.0` 作为解析层，并强制 `default-features = false`，避免 CLI、语法高亮和 Oniguruma 等非运行必需依赖进入离线包。该版本 MSRV 为 Rust 1.85，低于项目 Rust 1.97 基线；许可证为 BSD-2-Clause。解析结果立即转换为适配器私有 `MarkdownNodeMap`，只将 TIR、ResourceGraph 和 locator 交给核心。不能让库 AST 跨适配器协议或进入 SQLite。

`comrak::nodes::Sourcepos` 提供行列定位而不是直接字节区间，因此适配器必须维护 `LineIndex`，将行列位置转换为 UTF-8 字节跨度，并用 CJK、组合字符、emoji、CRLF 和行末边界样本锁定语义。若转换后的跨度不能与源切片和 AST 可见文本双向核对，该节点不得进入可导出支持集。

拒绝 `pulldown-cmark 0.13.4` 作为主解析器：它的 offset event stream 适合快速扫描，但缺少本阶段结构映射和验证所需的完整 AST，会把较多结构状态机责任转移到自研代码。未来只读索引性能探针可以重新评估它，但不得与生产适配器同时进入依赖闭包。

导出不依赖整篇 Markdown 重新序列化，而是：

1. 解析源对象并生成带前置源哈希的 node map。
2. 将译文覆层转换成非重叠字节补丁。
3. 按源偏移排序写入 staging，未命中补丁的字节原样复制。
4. 重新解析候选并验证结构、引用、保护 token 和输出哈希。

若任意修改无法表达为安全、非重叠补丁，导出阻断并指出具体单元，不降级为整篇重写。

### 2.2 Markdown 单元与保护规则

显示上下文边界是标题、段落、引用块内段落和列表项文本；权威可编辑单元则是这些块内的最大连续可译文本跨度。原因是当前适配器覆层契约只携带一段译文字符串，不能安全表达同一块内多个可移动结构 token 的重排。应用层用共享 `block_path`/上下文将原子跨度组合成连续长文体验，保存、身份、审查和 byte patch 仍以原子跨度为边界。

本阶段不引入 Markdown 专用占位符文本语法，也不把格式 AST 塞进译文修订。未来 EPUB 混合内容需要可移动 code 时，应以版本化结构化译文覆层统一演进协议和存储；在该契约落地前，任何跨 protected token 的修改必须拆分为安全跨度或阻断，而不能猜测 token 位置。

- 可翻译：普通文本、标题文本、列表/引用文本、链接标签、图片 alt 文本。
- `ProtectedAtom`：链接 URL、图片 URL、HTML 标签属性、行内代码、代码块内容、引用标识。
- `ProtectedOpen/Close`：强调、强强调、删除线以及其他明确嵌套结构；本阶段用于上下文投影和保护校验，不允许纯字符串覆层跨越这些边界。
- `Reference`：图片资源节点和已确认的本地引用；目标文件不因文字翻译改变。
- 不安全或无法定位的原始 HTML：保留为受保护内容；若改变会破坏结构则阻止导出。

### 2.3 稳定身份

Markdown `SourceUnitIdentitySpec` 使用：适配器身份版本、语义文档路径、节点类型/结构路径、规范化可见文本、前后邻域指纹。不得把完整源文件哈希或当前序号作为唯一身份。

结构变异必须进入现有绑定账本：只有 Exact 自动继承；移动、重复和拆分/合并产生人工审查。适配器升级提升 `identity_version` 时创建新 generation，不覆盖旧活动代。

### 2.4 翻译辅助数据边界

- 术语、变体、首选译法和备注是项目权威事实，使用独立迁移和幂等命令。
- 历史译文复用现有 `translation_revision`，只增加受控查询投影，不复制译文事实。
- 重复句是可重建派生分组；源文本哈希变化时局部失效，不参与身份绑定。
- 批注和标记挂在 `unit_id + revision/范围`，不修改源对象；基准修订变化时显示差异。
- 查找替换先产生候选预览和影响单元集合，用户确认后作为一个可撤销命令组提交；任何 protected token 变化直接拒绝。

## 3. 实施步骤

### Step 1：解析器与样本基线

涉及：新增 `crates/babel-markdown-adapter/`、工作区 `Cargo.toml`、`Cargo.lock`、`tools/phase4-runner/`、`.omx/phase4/fixtures/`。

- 固定 CommonMark 0.31.2 样本集和项目自有恶意样本。
- 实现 Comrak/pulldown 探针，测量节点边界、字节范围、结构保护、内存和解析确定性。
- 输出机器可读比较结果；选择一个解析库并写入 ADR，未选方案不得进入依赖。

停止条件：无法为支持子集生成稳定 UTF-8 字节范围，立即暂停适配器实现，修正解析策略。

### Step 2：Markdown 资源图与 TIR 抽取

涉及：`crates/babel-markdown-adapter/src/lib.rs`、`babel-adapter-protocol`、`babel-tir`、`babel-resource-graph`。

- 注册 manifest：扩展名只作提示，probe 必须检查内容；声明能力、支持等级、身份版本和安全上限。
- 实现 probe/inventory/extract，全部分页、可取消、受 deadline/字节/节点预算约束。
- 生成 Document/TextStream/Image 节点及 `Contains`、`References`、`ReadingOrderAfter` 边。
- 将 node map 和私有结构产物存 CAS，核心只接收资源、locator、TIR 和绑定候选。
- 为所有单元调用 `UnitContent::validate`，禁止无效保护嵌套进入 generation。

验收：同一输入、版本、配置重复 100 次，资源图哈希、TIR 哈希、阅读顺序和 source unit key 完全一致。

### Step 3：非破坏式 Markdown 导出

涉及：`babel-markdown-adapter` 导出实现、`babel-application` 适配器注册/任务路径、`babel-storage` 导出记录复用。

- 实现 plan_export：绑定活动 generation、冻结 `commit_sequence`、源对象哈希、node map 哈希和逐单元 overlay 哈希。
- 实现 materialize：只生成非重叠 byte patch，支持分页游标、幂等重试和 staging 已写前缀校验。
- 实现 verify_output：重新解析候选、验证 Markdown 结构、链接/图片引用、保护 token、源/译文映射覆盖和输出哈希。
- 输出仅在验证通过后原子发布；失败不得覆盖旧导出。

验收：未修改范围逐字节不变；修改一个单元不触发全篇重写；篡改源对象、node map 或 staging 时导出拒绝。

### Step 4：稳定重导入与变异绑定

涉及：`babel-application` 重导入组合、`babel-domain/src/identity.rs` 测试扩展、`tools/phase4-runner`。

- 建立 Markdown B 语料：插入/删除/移动/重命名标题、重复句、列表重排、段落拆分/合并、图片重排、换行和强调变化。
- 把结构变异结果接入现有 binding review/decision/reject-as-new API。
- 验证旧活动代在新代审查期间继续可读写；全部决定后才原子激活。
- 记录绑定覆盖率、错误自动绑定率和未决数量；不以覆盖率抵消任何错绑。

验收：重复内容不得自动猜测；错误自动绑定率为 0；稳定语义变异的 Exact+Shifted 覆盖率目标 ≥99.9%，失败则重做身份算法，不继续堆辅助功能。

### Step 5：翻译辅助核心

涉及：`babel-storage/src/schema.rs` 新迁移、`babel-storage/src/project.rs`、`babel-application/src/lib.rs`、必要的领域类型。

- 术语表：项目级 source term、variant、preferred translation、状态、备注；命令幂等、审计可追踪。
- 历史译文：按源文本/规范化文本/当前项目过滤查询 revision，不复制权威修订。
- 重复句：由 source canonical hash 生成可重建 group，提供跳转到 unit 的查询。
- 批注/标记：绑定 unit、范围和基准 revision；修订变化显示 stale/conflict，不静默迁移。
- 查找替换：精确匹配先行，正则作为后续受限能力；先 preview，再一个 undo group 提交，逐单元验证 TIR 保护规则。

验收：缓存/FTS 删除后术语、批注、标记和译文事实仍可恢复；重复命令不产生重复记录；批量替换失败时全部不提交。

### Step 6：契约、性能和交付证据

涉及：`tools/phase4-runner`、`.omx/phase4/`、`PHASE4.md`、`.omx/phase4/qa-report.md`、架构检查。

- 运行 AC-01..AC-07、Markdown 专项、BI-01/BI-02、DAG 投影和 EX-01..EX-04 的受影响子集。
- 构造 M 语料：100 MiB Markdown、约 250,000 单元、2,000 张图片引用；图片只验证引用关系，不进入 OCR。
- 记录首批/完整抽取、冷打开、分页、保存、搜索、导出、峰值 RSS 和 DAG 触达集合。
- 每次提交运行 Rust 单测、Clippy、格式、架构依赖和 Markdown 快速 smoke；只有适配器/运行时依赖变化才运行跨目标编译探针。
- 不运行完整 NSIS/Arch 发布闭包；把最终安装器、断网安装、升级、卸载留给 Phase 7。

## 4. 阶段验收标准

### 必须通过

- [x] Markdown 清单只声明实际实现的保守 CommonMark 文本/图片引用子集。
- [x] 当前支持子集、恶意输入和 UTF-8 字节定位测试通过。
- [x] 受保护结构、链接/图片引用、代码和 HTML 不被译文破坏。
- [x] 未修改源字节保持不变；修改只落在允许的非重叠补丁范围。
- [x] 导出冻结快照、可恢复、可验证、失败不覆盖旧结果。
- [x] 结构变异绑定不对重复/歧义内容自动猜测。
- [x] M 语料交互和内存门槛达到 PRD/测试规范，且没有通过增加并发来掩盖瓶颈。
- [x] 术语、历史译文、重复句、批注、标记和查找替换均有权威/派生边界及回归测试。
- [x] 权威翻译辅助事实和 Markdown 项目恢复路径有回归测试。
- [x] `cargo fmt`、Clippy、workspace tests、架构检查通过。

### 本阶段不作为失败

- Arch 干净禁网安装器验证未运行。
- Windows NSIS 安装器未生成或未在 Windows 实机验证。
- EPUB、OCR、图片嵌字和完整桌面 UI 尚未支持。

## 5. 性能与资源门槛

沿用 Phase 3 的交互门槛，并新增 Markdown 语料门槛：

| 指标 | 门槛 |
| --- | --- |
| M 首批可见内容 | ≤ 3 s（记录 max/median） |
| M 完整抽取 | ≤ 20 s（固定参考机，5 次，max） |
| M 冷打开至可编辑 | ≤ 3 s |
| 可见页查询 p95 | ≤ 50 ms |
| 10 万单元搜索首屏 p95 | ≤ 150 ms |
| M 项目空闲 RSS（不含 OCR） | ≤ 400 MiB |
| 单译文修改触达解析/DAG 节点 | 与可见窗口或受影响资源相关，不得全项目线性增长 |

首次探针若显示解析库或 AST 映射占用主导，先优化分区、缓存和私有产物布局；没有 profile 证据不得引入零拷贝协议、并行 SQLite writer 或自定义 allocator。

## 6. 风险与缓解

| 风险 | 早期信号 | 缓解/停止条件 |
| --- | --- | --- |
| Markdown 回写破坏未改结构 | 黄金差分出现非声明字节变化 | 改为 byte patch；无法表达的修改阻止导出 |
| 解析库升级改变 source position | 同一 fixture 哈希或绑定覆盖率变化 | 锁版本、适配器 build/identity version 递增、重新绑定审查 |
| TIR 变成 Markdown AST | 核心枚举开始出现 Markdown 节点 | 私有 node map 留在 CAS，公共面只保留保护 token/引用 |
| 批量替换误改 protected token | preview 与提交集合不一致 | 全量预览哈希 + 单事务 undo group + TIR 校验 |
| 翻译辅助扩大 schema/查询热路径 | 保存延迟或 WAL 行数随项目规模增长 | 派生表延后刷新；只在用户查询时分页读取 |
| Markdown 范围吞噬 EPUB/OCR | 未完成项阻塞核心验收 | 明确支持矩阵；Phase 4 只处理文本和图片引用 |

## 7. 验证命令与停止条件

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --all-targets
./tools/check-architecture.sh
cargo build --release -p babel-markdown-worker
cargo run --release -p babel-phase4 -- smoke
cargo run --release -p babel-phase4 -- benchmark \
  --work-dir .omx/phase4/benchmark-m-final-work-20260817 \
  --output .omx/phase4/benchmark-m-final.json
```

停止条件：任一数据安全不变量、错误自动绑定、保护 token 校验、导出快照隔离或 M 语料绝对门槛失败，停止新增翻译辅助，先修复核心链路。发布闭包不在本阶段重复执行；只有打包脚本、运行时依赖或安装布局变化时执行相关跨目标编译探针。

## 8. ADR 摘要

**决定**：Markdown 作为第二个生产适配器，采用解析器私有 AST/node map、TIR 公共翻译单元和源字节补丁导出；翻译辅助以核心权威事实加可重建投影实现。

**驱动因素**：复用统一核心；保护未修改 Markdown 字节；阻止结构破坏和译文错绑；保持交互优先和最终发布延期策略。

**考虑过的替代方案**：整篇 Markdown 重新序列化；仅按换行切单元；把 Markdown AST 存进 SQLite；现在就重复完整安装包验证。

**拒绝原因**：整篇序列化会产生无关 diff；按换行无法保护嵌套结构；AST 进入 SQLite 会污染公共模型；安装包最终闭包尚未受本阶段代码变化影响。

**后果**：适配器需要维护 source byte/node map 和补丁计划；支持等级必须由差分测试生成；最终发布阶段仍需对全部格式统一验收。

## 9. 后续执行建议

本计划适合单线持续执行；若后续并行资源充足，可使用 Team + Ultragoal：

- `explore`：fixture、CommonMark 覆盖和现有接口事实。
- `architect`：解析库探针和补丁模型审查。
- `executor`：Markdown 适配器与应用接入。
- `test-engineer`：契约、变异绑定、M 语料和故障测试。
- `verifier`：阶段证据、支持矩阵和停止条件复核。

默认持久跟进建议 `$ultragoal`；这是普通工程交付，不使用 `$autoresearch-goal` 或 `$performance-goal`。只有用户明确要求持续单人验证循环时才使用 `$ralph`。

若使用 Team，应保持适配器协议/迁移/导出事务串行审查；Team 关闭前必须证明测试、性能和支持矩阵证据齐全，Ultragoal 记录最终阶段检查点、未验证项和下一阶段入口。
