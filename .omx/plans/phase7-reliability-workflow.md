# Phase 7：可持续工作与交付验证

## 目标

把 Phase 6 的“核心功能已打通”提升为“译者可以连续工作、重启后可信恢复、能够完成一次可追溯交付”。本阶段不构建 Linux/Windows 安装包，也不做 Windows 实机验收。

## 当前事实

- 三种工作空间共享 `TranslationWorkItem`，状态投影目前主要区分未翻译和草稿：`crates/babel-application/src/lib.rs:260`、`crates/babel-application/src/lib.rs:2677`。
- 领域层已经定义 `Untranslated`、`Draft`、`Translated`、`Reviewed`、`Blocked`，但缺少权威状态修订和转换命令：`crates/babel-domain/src/workbench.rs:37`。
- OCR 候选已写入 `image_region_ocr_cache`，但资源队列尚未在重启后主动投影最新缓存：`crates/babel-storage/src/schema.rs:363`、`crates/babel-storage/src/query.rs:187`。
- 校验与导出核心 API 已存在，桌面校验/导出页面仍是静态壳：`crates/babel-application/src/lib.rs:1984`、`apps/desktop/src/routes/projects.$projectId.validate.tsx:1`、`apps/desktop/src/routes/projects.$projectId.exports.tsx:1`。
- Playwright 用例已配置，但当前 Linux 环境没有 Chromium；Windows 安装包和实机验证按既定策略延期。

## 实施顺序

### 1. 权威翻译状态机

- 新增状态修订表和单写者命令，至少支持：完成翻译、标记已审校、标记受阻、解除受阻。
- 每次状态转换带 `command_id`、期望当前状态/译文修订、操作者时间和可选原因；重复命令必须幂等，陈旧期望必须拒绝。
- `Reviewed` 或 `Translated` 绑定到当时的译文修订；译文再次修改后投影自动降级为 `Draft`，不篡改历史。
- `Blocked` 必须携带用户可理解的原因，并在校验/导出中成为可定位问题。
- 验收：单元测试覆盖合法转换、非法转换、重放、陈旧写入、译文修改后降级；冻结导出只接受可交付状态。

### 2. OCR 跨重启恢复

- 在资源队列投影中返回最新匹配模型/Profile 的 OCR 候选摘要，保持“识别结果”和“人工修正原文”分离。
- 启动/打开项目时不重新执行 OCR；只从缓存读取，并校验源图片哈希、generation、region、模型指纹。
- 模型不匹配或缓存损坏时返回可解释的未命中，不覆盖现有人工内容。
- 验收：识别 -> 关闭 -> 重开 -> 资源页回显；模型切换隔离；损坏缓存被拒绝；不产生新的 OCR 进程。

### 3. 校验、导出与导出记录 UI

- 将真实校验结果接入校验页，支持从问题定位到对应工作项并保留返回筛选条件。
- 将 TXT/Markdown/EPUB 文件导出命令接入导出页和原生保存对话框；Markdown 必须使用文件路径导出以保留图片闭包。
- 使用现有 `export_record` 恢复边界记录准备中、已发布和崩溃取消状态，不把路径写入权威项目数据。
- 导出页显示格式、冻结提交序列、输出路径、结果哈希、资源数量和失败原因。
- 验收：阻塞状态不能导出；成功导出可重开验证；目标已存在时 no-clobber；导出崩溃后可恢复诊断。

### 4. 真实桌面工作流验证

- 准备最小离线 fixture：TXT、含相对图片的 Markdown、含图片文字的 EPUB。
- 覆盖项目创建/导入、长文翻译、单元切换、资源连续翻译、OCR 回显、图片嵌字预览、校验、导出、重启恢复。
- 在 Linux 安装 Chromium 后执行 Playwright；Tauri 原生 IPC 仍需至少一次手工冒烟。
- 记录资源限制：不宣称 Windows 安装、Windows OCR runtime 或 Windows named pipe 已验证。

## 不做

- 不构建安装包，不做 Windows 实机验收。
- 不加入机器翻译、生成式 AI、自动改写。
- 不扩展 PDF、游戏资源、音频、视频格式。
- 不在本阶段做大规模 SQLite/IPC 性能重构；只有发现验收阻塞才修复局部问题。

## 阶段停止条件

1. 状态机所有权威转换和冻结导出规则有 Rust 回归测试。
2. OCR 缓存跨重启回显有集成测试。
3. 校验页、导出页不再使用静态示例数据。
4. Linux Playwright 工作流通过，Tauri 手工冒烟记录完整。
5. `cargo test --workspace`、`cargo clippy --workspace --all-targets -- -D warnings`、Tauri `cargo check`、前端 `check` 和 Vitest 全部通过。

## 后续衔接

Phase 7 完成后再进入最终发布阶段：受许可字体、OCR/worker 资产闭包、Windows 安装包构建与双平台发布验收。
