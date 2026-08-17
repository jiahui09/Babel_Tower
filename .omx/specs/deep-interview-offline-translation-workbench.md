# 离线翻译工作台：架构规划规格

## 元数据

- Profile：standard
- Rounds：3
- Final ambiguity：0.07
- Threshold：0.20
- Context type：greenfield
- Context snapshot：`.omx/context/offline-translation-workbench-20260816T125753Z.md`
- Transcript：`.omx/interviews/offline-translation-workbench-20260816T132642Z.md`

## 意图

为个人译者提供一个安静、可靠、离线优先的完整翻译工作台，把导入、内容提取、人工翻译、图片文字处理、校验和原格式导出收束为统一项目体验，使用户无需理解解析器、插件进程、运行时或命令行。

## 期望结果

产出一份可立即开工的技术架构实施蓝图，明确桌面框架、语言、权威存储、编辑器内核、OCR、图片处理、格式管线、任务系统、恢复机制、打包发布、模块边界、数据流、风险及可测试验收标准。

## 范围内

- Arch Linux 与 Windows 桌面应用。
- TXT、Markdown、EPUB 2/3 的导入、翻译、校验和尽可能保真导出。
- Markdown 与 EPUB 中受支持图片的 OCR、人工翻译和译文嵌入。
- 长文、结构化单元、资源三种工作空间，共享项目、译文、状态、搜索、撤销、进度与导出结果。
- 离线项目存储、自动保存、崩溃恢复、原件保护、诊断与事务安全。
- 单个完整离线安装包，包含首版 OCR 模型和必需运行时。
- 为未来游戏资源、音频台词、视频文本留出内部扩展边界，但不把插件生态作为首版产品主体。

## 范围外 / 非目标

- 不集成机器翻译或生成式 AI，不自动生成译文。
- 不做通用文本编辑器、IDE 或 VS Code 风格的插件工作台。
- 首版不实现游戏引擎资源、音频或视频处理。
- 首版不提供账号、云同步、协作或在线依赖。
- 不承诺任意 EPUB/Markdown 的像素级原样输出；只承诺受支持结构的可验证保真与明确诊断。
- 不允许扩展直接拥有权威项目存储或绕过核心事务边界。

## 决策边界

- 架构方案可直接确定具体技术栈、模块边界、内部协议、存储模型、打包方式和分阶段路线，无需逐项再次确认。
- 必须保留的产品决策：纯人工译文、完整离线、单体安装包、原件不可变、三种工作空间共享权威状态、首版格式范围。
- 不在本轮创建应用代码、初始化生产依赖或承诺发布日期。

## 约束

- 用户安装和日常使用不需要账号、命令行或网络。
- Windows 和 Arch Linux 必须从同一核心代码与数据模型构建。
- 源文件必须原样保留；导出只能生成新产物。
- OCR、解析器或其他辅助任务崩溃不得破坏项目数据库或丢失已提交译文。
- 编辑、状态变化和映射变更必须进入统一撤销、自动保存和恢复语义。
- 格式处理必须保留不可翻译结构，并能在无法安全回写时阻止导出或给出明确诊断。

## 可测试验收标准

1. 方案明确列出每个技术组件及选择理由、替代方案和主要风险。
2. 方案定义从源文件导入到导出产物的完整数据流，并标出所有事务与故障边界。
3. 三种工作空间读取和修改同一组内容单元 ID、译文版本和状态，不复制权威译文。
4. 任一工作空间提交的译文在重启后可恢复，并在其他工作空间可见。
5. 导入后的源文件以不可变对象保存；任何导出、OCR 或扩展失败都不修改源对象。
6. OCR worker 可被强制终止而不破坏项目数据库；重启后任务可重试或明确标记失败。
7. TXT、Markdown、EPUB 2/3 分别有往返保真测试策略，覆盖未修改导出、局部译文替换、非法结构和中断恢复。
8. Windows 安装包和 Arch Linux 分发包均包含首版 OCR 模型及必需运行时，首次断网启动可完成导入、编辑、保存、恢复、OCR 和导出。
9. 方案给出数据库迁移、备份、恢复、诊断包和向后兼容策略。
10. 方案将未来格式能力限制在核心定义的格式适配器协议内，不允许绕过权威存储和事务 API。

## 已暴露假设与结论

- 假设：完整离线 OCR 可以与轻量安装同时满足。结论：用户接受更大的单体安装包，优先完整离线能力。
- 假设：一种编辑器能自然覆盖三种工作空间。结论：共享领域模型和命令系统，但允许每个工作空间使用不同视图技术。
- 假设：重新序列化即可实现格式保真。结论：方案必须优先保存原始字节、条目顺序和可定位映射，导出采用受控替换与格式级校验。

## 压力测试结果

压力回访聚焦“一键安装 + 完整离线 OCR”的隐含分发成本。结果将架构约束从模糊的“离线可用”收紧为“关键 OCR 模型和运行时随单个安装包分发，首次运行不下载”。

## 资料与术语台账

- 已检查：用户提供的 AGENTS.md；仓库中没有 README、docs、代码、已有规划或术语表。
- 规范术语：权威项目存储、内容单元、源对象、格式适配器、工作空间、OCR worker、导出产物。
- 避免术语：把格式适配器称为用户插件；把 OCR 结果称为译文；把视图状态当作权威内容。
- 无文档/代码冲突；项目是 greenfield。

## 外部证据摘要

- Tauri 2 官方支持外部 sidecar、Windows NSIS/MSI 和 Linux AppImage 分发，适合 Rust 权威核心与隔离 OCR worker。
- SQLite 官方 WAL、FTS5 和 Backup API 支持本地事务、全文检索与在线备份需求。
- Tiptap 基于 ProseMirror，适合长文语义编辑；结构化单元和图片资源应使用独立视图，而非强行共用一个编辑器实例。
- PaddleOCR 支持多语言离线 OCR；ONNX Runtime 可作为跨平台 CPU 推理后端。Rust `ort` 仍需隔离在 worker 边界内以降低绑定版本风险。

主要来源：

- https://v2.tauri.app/develop/sidecar/
- https://v2.tauri.app/distribute/windows-installer/
- https://v2.tauri.app/distribute/appimage/
- https://www.sqlite.org/wal.html
- https://www.sqlite.org/fts5.html
- https://www.sqlite.org/backup.html
- https://tiptap.dev/docs/editor/getting-started/overview
- https://github.com/PaddlePaddle/PaddleOCR
- https://github.com/microsoft/onnxruntime

## 可选的长期文档建议

进入实现前可由用户选择将最终共识架构转写为仓库公开的 `docs/architecture.md` 与 ADR；本次访谈不自动创建公开文档。

## 推荐交接

使用 `$ralplan --direct .omx/specs/deep-interview-offline-translation-workbench.md` 完成 Planner → Architect → Critic 共识规划。此规格是需求事实来源，后续不得重新解释纯人工译文、完整离线、单体安装、源对象不可变和首版格式边界。

