# BABEL TOWER RELEASE CAPSTONE

## 目标

在没有原作者帮助的情况下，从仓库拉取到生产候选 artifact，证明架构理解、开发、测试、排障、E2E 和发布能力。

## 交付顺序

1. 安装环境并启动项目。
2. 阅读 `docs/00_START_HERE.md` 至状态/IPC 文档。
3. 完成剩余 P0，再选择关键 P1。
4. 编写缺失单元/集成/真实桌面 E2E。
5. 通过质量门禁。
6. 在 Windows 验证安装、启动、OCR、Export、卸载。
7. 生成 Release Notes、artifact、manifest、licenses 和 SBOM。

## 评审证据

提交设计记录、diff、测试日志、E2E 报告、文件系统断言、平台信息和剩余风险。任何缺证据项都不能标记 `RELEASED`。

## 毕业标准

能解释状态 owner 和 IPC 边界；能独立定位并修复真实 bug；能写回归测试；能运行真实桌面 E2E；能按 [release-gates.md](release-gates.md) 复现发布流程。
