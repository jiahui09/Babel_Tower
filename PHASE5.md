# Babel Tower Phase 5：EPUB 2/3 纵向闭环

状态：功能与合同型性能通过，带一个已量化的极端文本型限制。

## 本阶段完成

- 新增内置 `babel-epub-adapter` 与隔离 `babel-epub-worker`，支持 EPUB 2 NCX、EPUB 3 nav、多章节 spine 和多资源 ResourceGraph。
- 公共核心只增加通用归档成员 locator 与 staging 写能力；ZIP/OPF/XHTML 私有模型仍留在适配器。
- 应用层不再假定单一 `TextStream`；EPUB 与 TXT/Markdown 共用 SQLite 权威存储、稳定身份、绑定审查、保存、恢复、校验和冻结导出。
- XHTML 以 member-local 字节跨度提取和回写；未修改成员使用 ZIP raw copy，候选 staging 采用有界直写、分块取消检查和流式验证。
- 正式导出通过应用层流式发布到目标路径；发布为原子 no-clobber，既有文件不会被覆盖。原字节返回 API 仅保留兼容用途。
- ZIP 路径、重复成员、加密、压缩方法、展开量、压缩比，以及 XML 深度/属性/文本节点均有明确预算。
- 提供 `babel-phase5` smoke 与可复现 L 语料生成器。

## 关键结果

- 合同型 L：537.2 MB 输入、2 GiB 展开、5000 成员、10 万单元，导入 17.890 s；真实 Kernel 路径对 537.3 MB 输入流式导出 2.147 s；主进程与 worker 峰值保守合计 216.2 MiB，全部门槛通过。
- 极端文本型 L：约 1.6 GiB 可翻译正文时，导入 99.089 s、峰值 RSS 1.825 GiB，明确失败。
- EPUBCheck 5.3.0 与 `unzip -t` 均通过导出候选。
- workspace 测试、Clippy 零警告、fmt 和架构方向检查通过。

## 产品含义

第五阶段证明了核心无需为每种格式重建：EPUB 的容器、阅读顺序、资源引用和重打包都能通过现有 ResourceGraph/TIR/能力句柄接入。普通大型电子书已满足阶段预算；异常大的纯文本 EPUB 仍需要把应用层“全书聚合后提交”改成流式两阶段导入。

## 审查入口

- 支持矩阵：`.omx/phase5/support-matrix.md`
- 依赖决策：`.omx/phase5/dependency-adr.md`
- QA 与命令：`.omx/phase5/qa-report.md`
- 性能 JSON：`.omx/phase5/benchmark-l-r4.json`、`.omx/phase5/benchmark-l-r2.json`
- 外部验证：`.omx/phase5/epubcheck-5.3.0-r2.json`
- 实施计划与原审查清单：`.omx/plans/phase5-epub-vertical-slice.md`

完整 NSIS/Arch 安装、升级、卸载和断网发布闭包仍按新策略留在 Phase 7。
