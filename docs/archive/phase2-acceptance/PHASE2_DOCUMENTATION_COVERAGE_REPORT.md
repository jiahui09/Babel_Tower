# Documentation Coverage Report

## 已覆盖知识

- 新人入口、环境、运行与验证路径。
- 产品工作流、分层架构、目录定位和领域模型。
- 状态所有权、Bridge/Tauri/Rust 调用链、工作台和编辑器语义。
- 文件系统、恢复、设置、命令、测试、构建、发布、排障和术语。
- 已实现架构决策及高风险修改的验证要求。

## 缺失知识

- 尚未存在的真实桌面 E2E 实现，无法提供可执行的自动化脚本。
- 尚未完成的恢复决策 UI、快捷键覆盖和 word-wrap 行为，无法提供使用文档，只能记录其限制。
- 最终 Windows 安装包、字体许可和 SBOM 闭包的发布步骤尚无事实基础。

## 无法验证的知识

- Windows 原生安装、启动、OCR runtime 和导出。
- 真实 Tauri 长流程恢复、跨重启 OCR 缓存回显和多图资源连续编辑。
- 当前环境缺少 Chromium 时的 Playwright 行为。

## 仍存在的文档断点

- 根 README 已指向开发入口，但历史 `PROJECT_STATUS.md` 和 `DESIGN.md` 含有过期或互相矛盾的状态表述；开发判断以 `CURRENT_STATE.md`、代码和测试为准。
- 此报告是 Phase 2 的历史基线。Phase 3 已补充 `tutorials/`、`tasks/` 与 acceptance 体系；真实桌面验收仍未实现，当前限制见 [DEVELOPER_ONBOARDING_READINESS_REPORT.md](../../acceptance/DEVELOPER_ONBOARDING_READINESS_REPORT.md)。

## 新人路径模拟

`README.md` -> `docs/00_START_HERE.md` -> 产品/结构 -> 架构/状态/IPC -> 前端或 Rust -> 测试/E2E/排障/发布。

该路径能定位常见代码层、识别不可随意修改的区域、运行基础测试，并明确真实桌面验证和发布尚未完成。

## 阶段边界

本文记录 Phase 2 完成时的覆盖结论；Phase 3 的当前接管评估见 [DEVELOPER_ONBOARDING_READINESS_REPORT.md](../../acceptance/DEVELOPER_ONBOARDING_READINESS_REPORT.md)。
