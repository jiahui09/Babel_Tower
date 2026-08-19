# Babel Tower 项目现状与目标对齐审计

审计日期：2026-08-18

这份文档是当前仓库状态的事实摘要。它区分已经由代码和验证证明的能力、已经开始但未闭环的能力，以及仍停留在计划中的能力。阶段文档继续保留详细设计和历史证据，本文件负责回答“现在到底做到哪里了”。

## 结论先行

Babel Tower 已经从架构验证阶段进入桌面工作台纵向开发阶段。当前已经打通：

```text
项目登记/打开
  -> TXT / Markdown / EPUB 导入
  -> 稳定 generation、unit_id、ResourceGraph、TIR
  -> 长文 / 单元 / 资源三种观察方式
  -> 人工译文保存、撤销基础、恢复和安全文本导出
  -> 图片资源读取与真实预览
  -> 离线 OCR worker 与 OCR 候选缓存
  -> 人工译文 -> 嵌字派生 PNG -> CAS -> 图片区域 revision -> 预览
```

当前尚未形成完整交付闭环的部分是：

```text
图片派生对象
  -> EPUB 图片成员替换（单成员已接通）
  -> Markdown 资源闭包与图片成员替换
  -> 文本与图片联合导出
  -> 导出前完整校验
  -> Windows 运行库与最终双平台离线安装包
```

因此，当前产品是“可运行的离线翻译工作台开发版”，还不是可以签发给最终用户的完整生产版。

## 与产品目标对齐情况

| 产品目标 | 当前状态 | 事实与边界 |
| --- | --- | --- |
| 面向个人译者、人工翻译优先 | 对齐 | 核心只保存人工译文；OCR 结果、人工修正原文、人工译文分层保存；未接入机器翻译或生成式 AI |
| 离线优先、无账号 | 基本对齐 | worker、OCR 模型和本地项目存储均可离线运行；最终安装包闭包尚未完成 |
| 长文模式 | 基础闭环已实现 | 路由、快照、工作项和译文编辑已接入；长篇窗口化、完整上下文工具仍需继续完善 |
| 结构化单元模式 | 基础闭环已实现 | 与长文共享 `unit_id`、译文和核心保存边界；完整状态筛选和批量操作尚未完成 |
| 资源模式 | 已实现开发闭环 | 图片区域队列、真实图片预览、OCR、人工修正、人工译文、嵌字派生预览已接通 |
| 三种模式共享同一项目状态 | 对齐 | 核心通过 `TranslationWorkItem`、稳定身份和统一 revision 投影，避免第二套译文 |
| 支持 TXT / Markdown / EPUB 2/3 | 基本对齐 | 三种格式已有文本导入、解析、绑定、保存、校验和导出；EPUB/Markdown 图片资源覆盖与多区域合成已接入 |
| 图片译文按原图风格嵌入 | 基础能力已实现 | 当前是确定性平色背景 + 字体渲染 + 溢出拒绝；不承诺 Photoshop 级背景修复或自动风格还原 |
| 原文件不被破坏 | 对齐 | CAS 哈希复核、不可变源对象、派生对象独立保存、导出 no-clobber |
| 安全导出 | 基本对齐 | 文本与图片均使用冻结快照、CAS 能力句柄和安全发布；外链/缺失图片会在导出前拒绝 |
| 多格式扩展潜力 | 架构对齐 | ResourceGraph、Locator、适配器协议、OCR 契约和格式无关工作项已形成扩展边界 |

## 阶段进度

### Phase 0：架构证伪

状态：已完成架构证伪，最终产品包不在本阶段范围内。

- SQLite 权威存储、稳定身份、DAG/IPC、崩溃恢复均有可运行证据。
- Linux 可构建 Arch/Windows 方向的阶段性产物，但 Windows 安装包不能在 Linux 上实机验收。
- 最终桌面产品包、OCR 资产闭包和多格式桌面工作流当时仍未进入发布产物。

详见 [PHASE0.md](PHASE0.md)。

### Phase 3：TXT 纵向闭环

状态：核心能力已完成，发布门禁按原策略保留未签发状态。

- TXT worker、稳定身份、导入、保存、搜索、恢复、校验和保真导出已通过验证。
- 无 BOM 非 UTF-8 不自动猜测；GB18030/GBK 尚未进入产品导入参数，因此不宣称支持。

详见 [PHASE3.md](PHASE3.md)。

### Phase 5：EPUB 2/3 纵向闭环

状态：文本内容链路已完成，图片资源链路尚未纳入最终导出。

- EPUB 2 NCX、EPUB 3 nav、多 spine、成员 locator、XHTML 文本跨度和安全重打包已有实现。
- 大型普通 EPUB 已有阶段性能证据；极端文本型 EPUB 的全书聚合限制已记录。

详见 [PHASE5.md](PHASE5.md)。

### Phase 6：统一编辑器与连续资源工作流

状态：6A-6E6 核心闭环已完成；桌面人工 E2E、导出记录和最终安装包属于后续阶段。

- 6A-6D：统一工作项、导航恢复、桌面路由、项目登记、TXT/Markdown/EPUB 导入和基础 IPC 已完成。
- 6E1：资源队列、图片区域连续导航、人工修正和真实图片预览已完成。
- 6E2：图片哈希保护、区域校验、平色背景处理、确定性 PNG 渲染核心已完成。
- 6E3：Linux x64 OCR worker、PP-OCRv6 tiny 资产验证、Tauri 调度和资源页识别入口已完成。
- 6E4：OCR 候选缓存核心读写、模型指纹隔离、幂等和覆盖保护已完成；桌面端跨重启自动回显尚未接入。
- 6E5：人工译文到派生 PNG、CAS、图片区域 revision 和资源页预览已完成。
- 6E6：已接入格式无关 `ImageOverlay` 和冻结提交投影；Markdown 相对图片进入 CAS 闭包，EPUB/Markdown 均支持同图多区域合成和联合导出。

详见 [PHASE6.md](PHASE6.md)、[PHASE6E.md](PHASE6E.md) 和 [第六阶段计划](.omx/plans/phase6-unified-editor.md)。

## 当前技术边界

### 已形成的稳定核心

- Rust 核心负责项目身份、generation、SQLite 权威存储、revision、恢复、任务和导出安全。
- 格式适配器通过统一协议接入，解析和导出不下沉到 React。
- Tauri 仅作为桌面边界，通过受控 IPC 调用 `Kernel` 和 worker。
- 原始对象进入 CAS 后按哈希读取；图片派生对象不覆盖原图。
- OCR 通过 `babel-ocr` 统一契约扩展，未来可以添加 PDF 文字层、扫描页、多语言路由和 AI 辅助引擎，不需要把分支扩散到翻译核心。

### 当前明确不承诺

- 自动翻译、生成式 AI、自动改写。
- Photoshop 级图层、复杂蒙版、透视变换和自动背景重建。
- 自动字体识别和完全自动的原图风格还原。
- Windows 实机安装、启动、OCR runtime 和 named pipe 验收。
- PDF、游戏资源、音频和视频资源的首版支持。

## UI 与交互现状

当前桌面端已经使用 Tauri 2、React、TanStack Router、Radix、Tailwind 和 Lucide。路由结构为项目库、导入、项目总览、长文、单元、资源、校验、导出和恢复。

资源页目前按工作者的连续操作划分为：

1. 左侧原图/派生图预览区：只负责查看和区域定位。
2. 右侧识别结果区：只读 OCR 候选。
3. 右侧人工修正原文区：修正 OCR，不修改候选本身。
4. 右侧人工译文区：唯一可提交译文的位置。
5. 右侧嵌字预览区：生成独立派生图，不覆盖原图。

这与“让译者只面对需要翻译的内容”的目标一致，但还缺少真实 Tauri 环境下的 Playwright 冒烟证据、连续 100 个图片区域的恢复验证，以及跨重启 OCR 缓存回显。

## 验证状态

最近一次本地验证通过：

- `cargo test --workspace`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo check --manifest-path apps/desktop/src-tauri/Cargo.toml`
- `pnpm --dir apps/desktop check`
- `pnpm --dir apps/desktop test`
- `git diff --check`

尚未完成或受环境限制的验证：

- `pnpm --dir apps/desktop test:e2e`：当前 Linux 环境未安装 Chromium。
- Windows 安装器实机安装、启动、OCR runtime 和导出验证：按约定留到 Windows 环境和最终发布阶段。
- EPUB/Markdown 图片联合导出的完整桌面端回归：核心单元测试已覆盖，桌面端人工回归仍待最终 UI 测试阶段。

## 当前优先级

1. 完成 Phase 7：跨重启 OCR 缓存回显与权威翻译状态机。
2. 将校验页、导出页接入真实 IPC、文件选择器和导出记录。
3. 在真实 Tauri 环境补充 Playwright 工作台冒烟和连续图片工作流测试。
4. 最终阶段再补齐受许可字体、Windows OCR runtime、格式 worker、模型、许可证/SBOM 和单文件安装包闭包。

## 审查清单

- [ ] 是否接受当前产品仍处于开发版，而非最终可发布版。
- [ ] 是否接受文本与图片联合导出核心已完成，但桌面人工 E2E 尚未执行。
- [ ] 是否接受 OCR 只提供识别候选，不代替人工翻译。
- [ ] 是否接受平台字体当前只是开发期回退方案，最终包必须携带受许可字体资产。
- [ ] 是否接受 Linux 只构建 Windows 包，不做 Windows 实机验收。
- [ ] 是否同意下一阶段进入 OCR 缓存回显、翻译状态机与桌面 E2E。

## 前端重构 v2 增量（本轮）

- 导出页已通过 DesktopBridge 调用真实导出路径选择、格式导出和导出记录查询；生产 IPC 失败不会回退到预览数据。
- Tauri 已补齐草稿保存命令；草稿与 durable translation commit 分离，保留核心恢复协议。
- 导航快照的 UUID 已在 Tauri DTO 边界转换为前端使用的十六进制字符串；长文、状态栏和命令上下文会跟随恢复的 `unit_id`。
- 资源页已完成中文/英文语言包接入，并使用同一工作项 revision 保存人工译文，避免跨视图陈旧覆盖。
- Explorer 已显示来源、工作区、派生资源三层根节点；来源节点保持只读。
- 增加仅在显式 `VITE_DESKTOP_BRIDGE=fixture` 且开发模式下启用的 Fixture Bridge，并更新 Playwright 路由断言；生产环境仍强制 Tauri Bridge。
- 本轮验证：前端 typecheck、ESLint、Vitest（4 files / 6 tests）、Rust `babel-application`（26 tests）、Tauri `cargo check`、`git diff --check` 通过。
- Playwright 未完成：当前 Linux 缺少 Chromium 可执行文件；Windows 安装包和实机验收仍按计划留到最终发布阶段。
