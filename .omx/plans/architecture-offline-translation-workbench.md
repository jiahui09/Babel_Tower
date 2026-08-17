# Babel Tower 技术架构实施蓝图

## 0. 规划状态

- 工作流：`$ralplan --consensus --deliberate`
- 输入规格：`.omx/specs/deep-interview-offline-translation-workbench.md`
- 当前阶段：Architect iteration 2 与 Critic 已按顺序批准，共识门禁完成
- 实现状态：未开始；本文只定义架构与实施边界

## 1. 结论摘要

首版采用 **Tauri 2 + 独立 Rust Core Service + TypeScript/React/Vite**。Rust Core Service 是唯一权威后端，负责项目身份、SQLite 事务、自动保存、撤销、任务、格式 worker 调度、OCR 调度、导出与诊断；Tauri/React 只承担桌面壳、工作空间交互和瞬时视图状态。项目数据采用 **SQLite + 内容寻址对象库**，编辑器采用 **Tiptap/ProseMirror** 作为长文语义编辑内核，结构化单元使用虚拟化列表编辑器，资源模式使用 Canvas 叠层。OCR 和格式解析分别作为无数据库协议能力的 Rust sidecar，OCR 使用 **PaddleOCR PP-OCRv6 ONNX 模型 + ONNX Runtime CPU**。Windows 交付 **Tauri NSIS x64 + WebView2 offline installer**，Arch Linux 以 **AppImage + 明确的 FUSE/WebKitGTK 支持基线**为首选；模型、字体及应用运行时全部进入单一平台包。

该架构刻意不把“视图文档”“解析器输出”或“插件数据”作为项目真相。唯一权威译文是 Rust 核心通过事务命令写入的内容单元修订。

## 2. RALPLAN-DR

### 2.1 原则

1. 数据安全优先于实现便利和安装体积。
2. 共享领域模型，允许三种工作空间使用不同交互内核。
3. 原始字节优先；导出做局部受控替换，不无条件重新序列化。
4. 不可信或易崩溃能力放在核心事务边界之外。
5. 首版内置格式质量优先于公共插件数量。

### 2.2 决策驱动

1. Windows/Arch 完全离线、单体安装与无命令行体验。
2. 长期写作时的可靠保存、恢复、撤销和跨工作空间一致性。
3. TXT/Markdown/EPUB 2/3 的结构保护与可验证回写。

### 2.3 可行选项

#### 选项 A：Tauri + Rust 权威核心（采用）

- 优点：事务、文件安全和解析器可集中在强类型核心；OCR 可用 sidecar 隔离；分发体积低于自带 Chromium 的路线；未来 CLI-free 辅助进程仍由核心监管。
- 缺点：Linux WebKitGTK 与平台 WebView 差异需要 E2E；Rust 富文本生态不直接复用前端模型；Tauri IPC 需要明确命令和事件协议。

#### 选项 B：Electron 壳 + 同一 Rust Core Service

- 优点：Chromium、IME、ProseMirror 和 Playwright 在 Windows/Arch 更一致；保留 Rust 权威存储和 worker 边界。
- 缺点：Chromium/Node 带来更大的包、内存、安全补丁与更新面；一旦 IPC 边界相同，数据安全并不优于 Tauri；Linux 单文件启动的宿主前置条件仍不能完全消失。

#### 选项 C：Qt 6 + Rust/C++ 原生 UI

- 优点：无系统 WebView 差异；原生桌面和图形能力强。
- 缺点：长文富文本定制、结构化 Web 风格虚拟化和前端人才/组件复用成本高；Rust/C++ FFI 和打包复杂度高于首版收益。

### 2.4 选择理由

Tauri 路线把长期价值最高的文件、事务、格式和任务能力放进独立 Rust 服务，同时保留成熟的 ProseMirror 编辑体验。Electron 的主要优势是渲染一致性，但无法抵消更大的运行时和安全维护面；Qt 的原生一致性不值得首版承担富文本与 FFI 成本。Core Service 的 IPC 合同保持壳无关；阶段 0 必须用同一 ProseMirror/IME/离线包 fixture 验证 Tauri。只有 Tauri 未通过客观门禁时，才通过 ADR 改用 Electron 壳，不重写核心。

## 3. 技术栈基线

| 层 | 选择 | 使用边界 |
| --- | --- | --- |
| 桌面壳 | Tauri 2.11.x | 窗口、菜单、能力白名单、IPC、sidecar、更新前置能力 |
| 核心服务 | 独立 Rust stable 进程 | 领域、存储、任务、文件系统、导出、安全校验；与桌面壳使用类型化本地 IPC |
| 前端 | TypeScript + React 19 + Vite | 工作空间、交互、瞬时视图状态；不直接访问 SQLite/源文件 |
| 长文编辑 | Tiptap 3 / ProseMirror | 章节组合视图与语义节点；变更转换为内容单元命令 |
| 结构化编辑 | React + TanStack Virtual（或等价） | 10 万单元虚拟化；字段编辑不复制权威文档 |
| 资源画布 | Canvas 2D 封装 | 图片缩放、区域、选框、文本排版预览；原图只读 |
| 存储 | SQLite bundled + rusqlite 0.40.x | 单项目单库、WAL、FTS5、迁移、Backup API |
| 对象库 | SHA-256 内容寻址文件 | 原件、未修改资源、模型清单、衍生图；数据库存引用和哈希 |
| OCR | Rust sidecar + ONNX Runtime + PP-OCRv6 | 检测/识别，不产生译文；无数据库权限 |
| 图像 | image + imageproc + ab_glyph | 解码、遮罩、平坦背景修补、字体测量和嵌字 |
| TXT 编码 | encoding_rs | BOM/编码处理；模糊编码必须由用户确认 |
| Markdown | pulldown-cmark + offset ranges | 提取 lossless inline token、保存字节跨度、导出局部 patch |
| EPUB | zip + quick-xml event offsets | 容器、OPF、XHTML 混合内容、引用图；未修改 payload 复用 |
| 包管理/构建 | Cargo workspace + pnpm | 锁文件、可重复构建、许可证/供应链扫描 |

版本号是 2026-08 的规划基线。初始化仓库时锁定当日稳定补丁版，并用锁文件、审计和黄金样本确认，不自动漂移大版本。

## 4. 总体边界与数据流

```text
React workspaces
  | typed shell IPC / sequenced events
  v
Rust Core Service (single project writer)
  |-- command bus / undo groups / save acknowledgements
  |-- task supervisor / diagnostics
  |-- format worker supervisor (built-in adapters)
  |-- export coordinator
  v
SQLite authority <----> immutable object store
  ^                         |
  | validated result        | task-scoped read-only copies
  +---- validated worker results <--- OCR worker / format worker

Import: source -> hash/copy -> adapter extract -> transaction commit
Edit: UI transaction -> core command -> SQLite commit -> sequenced event
OCR: resource copy -> worker -> validated regions -> transaction commit
Export: snapshot -> staging patch/repack -> validate -> atomic publish
```

核心原则：任何 sidecar、WebView 或格式解析临时产物都不能直接写项目数据库；只有 application service 能把验证后的结果提交为权威状态。

## 5. 拟议仓库结构

```text
Cargo.toml
pnpm-workspace.yaml
apps/desktop/
  package.json
  src/                    # React workspaces and design system
  src-tauri/              # Tauri shell, commands, capabilities
crates/
  babel-domain/           # IDs, units, revisions, status, commands, errors
  babel-application/      # use cases, writer actor, undo, autosave, tasks
  babel-storage/          # SQLite, migrations, backup, object store
  babel-format-api/       # internal adapter contracts
  babel-format-txt/
  babel-format-markdown/
  babel-format-epub/
  babel-image/            # regions, layout, rendering derivatives
  babel-export/           # staging, validation, atomic publishing
  babel-diagnostics/      # structured local diagnostics and bundles
workers/
  core-service/           # authoritative headless process and typed IPC
  format/                 # built-in parsers/exporters, no database protocol
  ocr/                    # isolated executable, model/runtime integration
assets/
  ocr-models/             # versioned model manifests, packaged binaries
  fonts/                  # redistributable Noto/Source Han subsets/full packs
fixtures/
  txt/
  markdown/
  epub2/
  epub3/
  images/
tests/
  fault-injection/
  desktop-e2e/
```

`babel-format-api` 是仓库内部 Rust trait，不是首版公共插件 ABI。未来外部格式能力若必须隔离，应走版本化 sidecar 协议和能力沙箱，仍不得访问权威存储。

## 6. 项目容器与权威存储

### 6.1 项目目录

```text
<name>.babel/
  manifest.json           # 项目 ID、schema、app compatibility、源摘要
  project.sqlite3         # 权威结构化状态
  objects/sha256/aa/...   # 不可变源对象和衍生对象
  recovery/               # 受控备份与迁移快照
  cache/                  # 可删除、可重建
  task-sandboxes/         # worker 临时目录，不是权威数据
  logs/                   # 本地脱敏日志
  exports/                # 默认导出 staging 与历史清单
```

导入首先流式计算 SHA-256 并写临时文件，`fsync` 后原子改名到对象库；只有对象就位后才在 SQLite 事务中创建 source record。对象按哈希不可变，项目删除引用不立即删除对象，清理必须是显式、可恢复维护任务。

### 6.2 核心表

- `project`：项目身份、schema、创建版本、最近安全打开版本。
- `source_object`：源哈希、原始名称、媒体类型、大小、不可变对象路径。
- `resource_entry`：容器内逻辑路径、payload 哈希、顺序、压缩与媒体元数据。
- `content_unit`：稳定 ID、父子/顺序、源文、上下文、source anchor、类型。
- `translation_revision`：追加式译文修订、作者类型（首版恒为 human）、时间和基修订。
- `translation_document`：可编辑文本与不可丢失 inline code 的版本化混合内容 IR。
- `translation_head`：单元当前修订指针。
- `unit_state`：工作状态、阻断原因、审校标记。
- `resource_region`：图片、坐标、OCR 原文、人工译文关联、布局参数。
- `command_log` / `undo_group`：已提交命令和补偿关系。
- `task` / `task_event`：持久任务状态机和重试证据。
- `diagnostic`：格式、导出、恢复和任务诊断，不默认存全文。
- `export_run`：输入快照、适配器版本、产物哈希、验证结果。
- FTS5：源文与当前译文的可重建索引。

### 6.3 保存、撤销与并发

每个项目只允许一个 Core Service writer actor。第二实例以只读模式打开，避免 SQLite WAL 在网络文件系统或同步盘上的多写者风险。数据库打开后强制 `journal_mode=WAL`、`synchronous=FULL`、`foreign_keys=ON`；writer 控制 WAL checkpoint，空闲或 WAL 超过阈值时执行被测量的 PASSIVE/RESTART checkpoint，不把 checkpoint 完成误认为编辑提交。

编辑协议：

1. UI 发送带全局唯一 `command_id`、`project_id`、`unit_id`、`base_revision`、`client_sequence` 的命令。
2. writer 验证权限、修订与领域规则，开始 SQLite 事务。
3. 追加译文修订，更新 head/状态/FTS，写入命令日志和 undo group。
4. `command_id` 有唯一约束；相同命令重试返回已记录结果，不重复追加修订。
5. `synchronous=FULL` 事务提交后返回 `commit_sequence`；UI 此时才显示 `saved`。
6. 核心把事件写入持久事件日志并广播；UI 重连时提交 `last_commit_sequence`，重放缺失事件或刷新快照。

撤销不是数据库回滚，而是由核心生成的补偿命令，因此可持久化、可跨工作空间观察。连续输入可以在 UI/核心间按 50-150ms 窗口合批，但每个 `saved` 指示都必须对应 `synchronous=FULL` 的 durable commit；无法提交时保留本地编辑态并显示明确错误。若事务已提交但 ack 丢失，UI 使用原 `command_id` 重试并取得原 `commit_sequence`。

### 6.4 SQLite 与对象库发布/备份协议

对象写入采用 prepare -> publish -> reference：写入同文件系统临时文件，flush 内容，计算并复核 SHA-256，以 no-clobber 语义发布到哈希路径并同步父目录，再由 SQLite 事务添加引用。中断最多产生无引用对象，不产生悬空引用。哈希路径已存在时必须复核长度和哈希；不一致视为完整性故障。

首版不自动删除无引用对象。显式维护任务只有在数据库可达性扫描、所有备份 pin 和运行任务 pin 都完成后才能隔离对象；隔离期结束后才允许用户确认清理。

备份分两类：迁移 checkpoint 使用 SQLite Backup API 加“可达对象 pin 清单”，对象仍留在当前不可变库；可移植项目备份在固定 `commit_sequence` 读快照上复制数据库、生成可达对象闭包清单并复制全部对象，验证每个哈希后才发布。恢复必须在新临时项目目录验证数据库、对象闭包和清单，再原子发布为新项目，绝不覆盖原项目。

项目身份和 schema 的唯一权威来源是 SQLite `project/meta`。`manifest.json` 只是可重建的启动提示，携带数据库 UUID、最近提交序号和校验和；若与数据库不一致，Core Service 在验证数据库后重建 manifest，不用 manifest 覆盖数据库。

## 7. 三种工作空间

### 7.1 长文模式

Tiptap/ProseMirror 只构造当前章节的组合文档。每个块节点携带稳定 `unit_id`，内部内容来自版本化 `TranslationDocument` IR；ProseMirror transaction 被适配为内容单元命令，编辑器 JSON 不是权威存储。章节按窗口加载，避免把整本书放入单个 DOM。

### 7.2 结构化单元模式

使用虚拟化列表查询 `UnitView` 投影，支持状态筛选、源译对照、批量状态操作和快速导航。每个编辑框发送同一 `UpdateTranslation` 命令，不维护第二份译文。

### 7.3 资源模式

Canvas 展示只读原图及区域叠层。OCR 结果是候选源文；用户确认/修改源文并人工填写译文。排版参数和衍生图片作为新对象保存，原图对象不变。跨模式导航通过 `unit_id` / `resource_region_id` 建立。

### 7.4 一致能力

- 全局搜索读取 FTS 投影。
- 进度由 `unit_state` 聚合。
- Undo/redo 调用核心命令，不在各视图私有维护。
- 任务、保存和诊断由全局事件流驱动。
- 工作空间只保留光标、滚动、面板开合等非权威 UI 状态。

## 8. 格式管线

### 8.1 内部适配器合同

```text
probe(source) -> confidence/media kind
inventory(source) -> immutable entries + safety diagnostics
extract(snapshot) -> units + anchors + resources
validate_translation(snapshot) -> diagnostics
export(snapshot, staging) -> candidate artifact
validate_artifact(candidate) -> diagnostics + manifest
```

适配器输入是不可变 source snapshot，输出是声明式结果；适配器不能访问 SQLite。核心负责事务提交、取消、限额、诊断持久化和发布。

### 8.2 TXT

保存原始编码、BOM、换行、最终换行和 byte ranges。译文不能由原编码表示时默认阻止导出，用户可显式选择升级为 UTF-8，且导出报告必须记录格式变化。

### 8.3 混合内容 IR

Markdown 与 XHTML 单元使用同一个版本化 `TranslationDocument`：`Text` 节点保存可编辑译文和源 span；`InlineCode` 保存稳定 code ID、原始 opening/closing/standalone lexeme、配对关系、源 span 与可移动策略。实体、硬换行、图片、链接边界、强调、ruby 和受支持行内 HTML/XHTML 都作为不可静默删除的 code 或带原始 lexeme 的文本表示。

Tiptap 将 inline code 渲染为受保护 atom/mark；结构化模式显示 CAT 风格占位符。用户可移动允许移动的成对 code，但不能删除、复制、改变 ID 或破坏嵌套。提交和导出都验证 code multiset、成对关系、嵌套、required order 和 escaping。

当 code 顺序不变时，导出只 patch 原始 leaf text spans；当受支持 code 合法移动时，只从保存的原始 lexeme 和新文本重组该 unit envelope，envelope 外字节保持不变。无法建立完整 offset、遇到不受支持嵌套或格式恢复语义时，单元进入 `blocked`，不得悄悄规范化整文件。

### 8.4 Markdown

用 pulldown-cmark 提取事件、原始 lexeme 和 offset range，但导出不通过 renderer 重建整文档。锚点包含逻辑路径、unit envelope、leaf byte ranges、源片段哈希和上下文哈希；导出从尾到头替换非重叠跨度。扩展语法若不能安全形成 IR，保留原文并给出阻断或跳过诊断。

### 8.5 EPUB 2/3

导入记录 ZIP entry 顺序、路径、压缩方式、payload 哈希、OPF manifest/spine 关系。XHTML 必须是可由 quick-xml 事件流建立精确 offset 的良构 XML；对受支持混合内容使用 byte/结构双锚点和 `TranslationDocument` 受控 patch。未修改 entry 直接复用 payload。重打包保证 `mimetype` 首条且不压缩，更新被替换图片的条目，验证 container、OPF、spine、nav/NCX、内部引用、XML 可解析性和 ZIP CRC。

不支持或加密/DRM 资源在导入阶段明确拒绝；不得尝试绕过保护。

## 9. OCR 与图片嵌字

### 9.1 worker 边界

OCR worker 由 Core Service 监管，使用版本化 length-prefixed JSON 协议。首版威胁模型保护项目免受受信任内置 worker 的崩溃、panic、ABI 错误和畸形输出，不宣称能抵御同用户权限下的恶意 worker。协议不给 worker 数据库句柄或项目根路径，只传任务沙箱和只读模型路径；Linux 在可用时使用 Landlock/seccomp，Windows 使用 Job Object/restricted token，能力未启用时在诊断中记录降级。核心验证协议版本、模型哈希、输出大小、坐标和文件哈希后才提交结果。

`ort` Rust binding 若仍处于 RC，只存在于 worker。主应用只依赖稳定协议；后续替换为 ONNX Runtime C API/C++ worker 不改变项目模型。

### 9.2 模型和人工边界

- 打包 PP-OCRv6 检测、方向和多语言识别模型，CPU 为首版基线。
- 模型、ONNX Runtime 和字体均带版本、许可证及 SHA-256 清单。
- OCR 只产生 `recognized_source_candidate`，不能写 `translation_revision`。
- 每个 OCR 区域允许用户修正源文、手工输入译文、选择字体/方向/对齐。

### 9.3 嵌字

首版支持平坦或近似平坦背景的遮罩扩张、颜色估计、填充、描边、阴影、自动字号与换行。复杂纹理背景不做生成式修补；系统保留原图并标记需要人工蒙版/外部修图。所有结果写成衍生对象，导出时按资源映射替换。

## 10. 任务、故障与恢复

持久任务统一使用状态机。应用启动时把遗留 `running` 转为 `interrupted`，按任务幂等性提供重试。格式解析/导出和 OCR 分别在独立 worker 与任务沙箱运行；主 Core Service 不加载 OCR native runtime，也不直接解析不可信容器内容。结果只有通过哈希和结构验证后才提交。

故障边界：

- UI/桌面壳崩溃：独立 Core Service 在受控宽限期内保持项目并接受重连；核心已确认提交的数据存在，未确认编辑由 UI 恢复态明确标识。
- OCR 崩溃：任务失败或中断，数据库无部分 OCR 结果。
- 解析失败：源对象仍存在，项目记录诊断，不生成半完整单元集。
- 导出失败：staging 可清理，旧导出和源对象不变。
- 迁移失败：打开前备份，事务回滚；无法恢复则只读打开并指导恢复。

发布产物默认不覆盖已有文件。显式覆盖时在目标同目录写临时文件、校验、`fsync`，再使用平台安全替换；跨卷时采用复制到目标同目录临时文件后再替换。

## 11. 安全与隐私

- Tauri capability 最小化；前端无任意 shell、任意路径或数据库能力。
- CSP 禁止远程脚本和远程导航；应用不加载在线内容。
- ZIP 防 path traversal、绝对路径、符号链接逃逸、entry 数量/总大小/压缩比炸弹。
- XML/HTML 解析关闭外部实体和网络获取，限制深度、节点数与单条文本大小。
- 所有 sidecar 参数结构化传递，不拼接 shell 命令。
- 日志默认不包含原文、译文、图片内容或完整用户路径；诊断包由用户主动生成。
- 不实现 DRM 绕过；检测到保护内容时停止并解释。

## 12. 迁移、备份与兼容

- SQLite meta 记录 `schema_version`、`min_reader_version`、适配器版本和源摘要；manifest 是带校验和的可重建提示。
- SQLite 迁移只前进，逐版本执行；迁移前使用 Backup API 生成带哈希快照。
- 新应用首次成功打开并完成完整性检查前不删除旧快照。
- 旧应用遇到更高 schema 只能拒绝写入；不得尝试降级。
- 缓存、FTS 和缩略图必须可从权威表与对象库重建。

## 13. 打包与发布

### 13.1 Windows

- Tauri NSIS x64 单体安装包。
- `webviewInstallMode=offlineInstaller`，把 WebView2 Evergreen 离线安装器放入包内。
- 打包 OCR worker、ONNX Runtime DLL、模型、字体、许可证和哈希清单。
- 代码签名进入正式发布门禁；未签名内部构建不得称为正式一键安装版本。

### 13.2 Arch Linux

- x86_64 AppImage 为首选用户包，包含应用依赖、Core/format/OCR workers、ONNX Runtime、模型、字体和许可证；宿主仍必须满足文档化的内核、glibc、WebKitGTK 和 FUSE 基线。
- 应用内 GUI 无法处理 AppImage 运行时启动前的 FUSE 失败，因此不再作此承诺。阶段 0 必须在 FUSE present/absent、默认下载权限、glibc/WebKitGTK 组合上做门禁；FUSE absent 只能通过发行页/图形文件属性或受支持 GUI 包管理器说明处理，不能伪装为零前置条件。
- 若门禁证明目标 Arch 基线不能满足“无命令行的一次打开”，发布不得继续宣称该能力；必须 ADR 选择经验证的图形安装载体，或切换 Electron 壳后重新验证。AUR/PKGBUILD 是后续渠道，不是断网能力前提。

### 13.3 供应链

Cargo/pnpm 锁文件、Rust/JS 漏洞扫描、许可证 SBOM、sidecar/model/font 哈希清单是发布工件。所有首次运行关键资源必须在构建时验证，不运行时下载。

## 14. 实施顺序

### 阶段 0：仓库与质量基线

建立 Cargo/pnpm workspace、独立 Core Service、Tauri 壳、类型化 IPC、CI、许可证/SBOM、错误码和 fixture 约定。完成 Tauri/同 Rust 核心门禁：Windows/Arch IME、ProseMirror、离线包、FUSE/WebKitGTK、崩溃重连；未通过时记录 ADR 并验证 Electron 壳备选。

### 阶段 1：项目安全内核

实现领域 ID、SQLite schema/migrations、对象库、单 writer、命令日志、修订、撤销、任务状态和备份恢复。先用内存/fixture 命令验证，无格式 UI 依赖。

### 阶段 2：TXT 纵向切片

完成 TXT 导入、内容单元、长文/结构化两个视图、保存/撤销/搜索/进度、校验和保真导出。该阶段必须证明共享权威状态和崩溃恢复。

### 阶段 3：Markdown 受控回写

加入 offset anchor、图片资源映射、未知扩展保留、黄金往返与失败诊断。不得用 Markdown renderer 重建整文件。

### 阶段 4：EPUB 2/3 容器

实现安全解包、OPF/spine/nav/NCX、XHTML 单元、资源映射、重打包与容器验证。用多个真实结构 fixture 锁定兼容边界。

### 阶段 5：OCR 与资源工作空间

集成隔离 worker、打包模型、区域编辑、人工译文和非生成式嵌字；实现 worker 故障注入与原图保护。

### 阶段 6：发布硬化

完成断网安装 E2E、大项目性能预算、低磁盘/权限/中断测试、诊断包、签名与发布清单。只有全部数据安全门禁通过才进入首版候选。

## 15. 可测试验收标准

1. 每种工作空间的写入都走同一 Rust `UpdateTranslation`/`ChangeUnitState` 命令，并产生单一修订序列。
2. 任何显示为 `saved` 的修订在立即终止应用并重启后仍存在。
3. 导入、OCR、迁移、对象发布、备份和导出任一阶段注入故障，源对象哈希保持不变，SQLite 通过 `integrity_check`，可移植备份拥有完整对象闭包。
4. TXT/Markdown 未修改导出 byte-identical；EPUB 未修改 entry payload 哈希一致且容器验证通过。
5. 修改导出只改变目标文本跨度、必需的 XHTML/图片 entry 和容器元数据，不重排无关内容。
6. Windows/Arch 最终包在断网干净系统首次运行可完成完整链路，不请求下载模型或运行时。
7. 10 万单元项目不一次渲染全部 DOM；达到测试规格中的启动、滚动、保存和搜索预算。
8. OCR worker 被强制终止后任务可诊断/重试，项目数据库没有部分结果。
9. 旧 schema fixture 可备份迁移；新 schema 被旧版本安全拒绝写入。
10. 日志和默认诊断不含源文、译文或图片内容。
11. lost-ack 后重试同一 `command_id` 不产生重复修订，UI 能从持久事件日志补齐序号。
12. Markdown/XHTML 的嵌套 inline code、实体、ruby、bidi 和 tail text fixtures 通过 code 保全与 envelope 外字节不变断言。

## 16. 风险与缓解

| 风险 | 缓解 |
| --- | --- |
| 系统 WebView 在 Windows/Linux 表现不一致 | 约束浏览器 API、Playwright + 真机 E2E、保持编辑命令在 Rust 核心 |
| Markdown/EPUB 重新序列化破坏格式 | 原字节/entry payload 保存、双锚点 patch、黄金样本和阻断诊断 |
| `ort` Rust binding 版本不稳定 | 只置于 sidecar、固定协议和版本、可替换 C API 实现 |
| 大模型/字体使包显著膨胀 | 用户已选择单体离线包；按平台压缩、明确体积预算、构建期去除无用架构 |
| SQLite WAL 在同步盘/网络盘异常 | 单 writer、检测不安全文件系统、备份、只读降级、明确支持边界 |
| WebView 崩溃导致未确认输入丢失 | 独立 Core Service、即时异步命令、短合批、核心 ack 驱动保存状态、故障注入测量 |
| EPUB 兼容面过大 | 明确支持矩阵、保守导出、未知/DRM 内容拒绝、不断扩充 fixtures |
| 图片复杂背景无法安全修补 | 首版只做传统可解释处理，保留原图并提供人工蒙版/外部修图路径 |

## 17. Deliberate 失败预演

### 场景 A：应用显示已保存，但断电后最后一段丢失

根因可能是 UI 以 debounce 完成代替数据库 commit。门禁：保存指示只接受核心 commit sequence；持续输入期间随机 kill 进程，所有已确认序号必须恢复。

### 场景 B：EPUB 导出可打开，但无关 CSS/metadata 被重写

根因可能是整文档 serializer。门禁：保存 entry payload hash 和 byte anchor；黄金样本断言无关 entry 不变，修改跨度外字节不变。

### 场景 C：OCR 原生库崩溃导致项目无法再打开

根因可能是 native runtime 与主进程/数据库同址。门禁：worker 没有数据库权限，核心只提交完整验证结果；强杀、畸形输出和 ABI 错误均只改变任务状态。

### 补充故障场景

- inline code 丢失/乱序：提交和导出双重验证 code multiset 与嵌套，黄金 fixture 覆盖实体、ruby、bidi、tail text。
- DB 备份存在但对象缺失：可移植备份只有在可达对象闭包全部哈希通过后发布。
- commit 成功但 ack 丢失：相同 `command_id` 重试返回旧结果，事件序号重放。
- AppImage 在应用启动前失败：阶段 0 宿主矩阵阻断发布，不依赖应用内诊断。

## 18. 扩展测试计划

- 单元：领域不变量、修订/撤销、路径/编码、锚点 patch、布局算法。
- 集成：SQLite 事务、任务状态机、sidecar 协议、每种格式完整管线、迁移恢复。
- E2E：Windows/Arch 断网最终安装包、三工作空间交叉编辑、故障注入、导出打开。
- 可观测性：commit/task/export sequence 可追踪，日志脱敏，诊断包能定位阶段但不泄露内容。

详细矩阵见 `.omx/plans/test-spec-offline-translation-workbench.md`。

## 19. ADR-001：首版桌面与核心架构

### Decision

采用 Tauri 2 + React/TypeScript UI + 独立 Rust Core Service + SQLite/对象库；OCR 与内置格式适配器使用故障隔离 worker。Tauri 是通过阶段 0 平台门禁的既定首选，Core IPC 保持桌面壳可替换。

### Drivers

- 完全离线与单体安装。
- 译文、原件和导出事务安全。
- 长文编辑体验与未来格式扩展边界。

### Alternatives considered

- Electron 壳 + 同一 Rust Core Service。
- Qt 6 + C++/Rust 原生 UI。
- 把 OCR/格式能力作为首版用户插件。

### Why chosen

该组合让成熟 Web 编辑器负责体验、Rust 负责不可替代的数据和文件职责，并通过 sidecar 隔离原生 OCR 风险。它比 Electron 更贴合权威核心，比 Qt 更快获得专业长文编辑能力，也比插件优先路线更一致。

### Consequences

- 必须维护严格 IPC schema 和平台 WebView E2E。
- Linux 构建基线、WebKitGTK/AppImage 与 Windows WebView2 都进入发布测试矩阵。
- 安装包因 OCR 模型、字体和离线 WebView2 较大，这是已接受取舍。

### Follow-ups

- 初始化时记录精确锁定版本和许可证。
- 对 PP-OCRv6 模型覆盖语言与体积做构建实测。
- 阶段 2 完成后以真实 TXT 纵向切片复核领域模型，再冻结 EPUB 适配器接口。

## 20. 外部依据（2026-08）

- Tauri sidecar：https://v2.tauri.app/develop/sidecar/
- Tauri Windows installer / WebView2：https://v2.tauri.app/distribute/windows-installer/
- Tauri AppImage：https://v2.tauri.app/distribute/appimage/
- SQLite WAL：https://www.sqlite.org/wal.html
- SQLite FTS5：https://www.sqlite.org/fts5.html
- SQLite Backup API：https://www.sqlite.org/backup.html
- Tiptap / ProseMirror：https://tiptap.dev/docs/editor/getting-started/overview
- PaddleOCR：https://github.com/PaddlePaddle/PaddleOCR
- ONNX Runtime：https://github.com/microsoft/onnxruntime

## 21. 可用代理与后续执行编制

可用角色：`architect`、`executor`、`test-engineer`、`debugger`、`verifier`、`code-reviewer`、`dependency-expert`、`designer`、`explore`、`writer`、`git-master`、`code-simplifier`。

推荐执行路径是 `$ultragoal` 持有长期目标与证据账本，阶段内需要并行时使用 `$team`：

- Rust 核心 lane：1 个 `executor`，medium reasoning，负责 `crates/babel-domain`、`application`、`storage`。
- 格式 lane：1 个 `executor`，medium reasoning，负责 TXT/Markdown/EPUB，按阶段启用而非同时铺开。
- 桌面体验 lane：1 个 `executor` + 必要时 `designer`，负责 Tauri/React 和三工作空间。
- 测试 lane：1 个 `test-engineer`，medium reasoning，独立维护 fixtures、故障注入和 E2E。
- 阶段门禁：`verifier` high reasoning 验证数据安全声明，`code-reviewer` high reasoning 审查 diff。

Team 启动提示（仅在用户选择实施后）：

```text
$ultragoal create-goals --brief-file .omx/plans/architecture-offline-translation-workbench.md
$team .omx/plans/architecture-offline-translation-workbench.md
```

Team 关闭前必须提供目标阶段的测试输出、平台/fixture 范围、未验证项和变更文件；Ultragoal 再把这些证据记入阶段 checkpoint。`$ralph` 只在用户明确选择单人持续修复/验证回路时作为备选，不是默认执行路径。

## 22. Goal-Mode 后续建议

- 默认：`$ultragoal`，把阶段 0-6 转成持久目标并顺序验收。
- 并行阶段：`$ultragoal` + `$team`，由 Team 返回可检查证据，Ultragoal 保持账本所有权。
- `$autoresearch-goal` 不适用：本任务是工程交付，不是研究成果。
- `$performance-goal` 仅在阶段 2 后需要专门优化 10 万单元预算时使用。
- `$ralph` 仅为明确选择的单 owner 持续验证备选。

## 23. 共识审议与修订记录

审议顺序严格为 Architect -> Critic：

1. Architect iteration 1：`ITERATE`。补齐混合内容 IR、SQLite/对象库原子发布与闭包备份、durable ack 与 lost-ack 幂等重试、worker 真实故障边界、Arch 启动前平台门禁。
2. Architect iteration 2：`APPROVE`。确认五项阻断均已在正文和测试规格中闭环，并保留 Electron 壳 + 同一 Rust Core Service 作为阶段 0 的公平回退选项。
3. Critic：`APPROVE`。确认 12/12 验收标准可验证，ADR、失败预演、测试矩阵与执行交接完整，无实质阻断项。

本轮 Critic 未要求继续修改技术决策；共识审议记录位于 `.omx/plans/reviews/`，可机读交接状态位于 `.omx/state/ralplan-offline-translation-workbench.json`。
