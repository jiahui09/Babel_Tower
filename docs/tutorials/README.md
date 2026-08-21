# Babel Tower Tutorials

这些教程只做一件事：把新人带进真实仓库，学会定位、修改、验证和复盘。

每篇都遵循同一结构：

- Goal
- Prerequisites
- Concept
- Steps
- Files
- Expected Result
- Common Errors
- Acceptance
- Reflection

这里的练习都刻意做小。它们帮助你学会操作真实文件和真实状态，不会把 fixture 浏览器测试说成真实桌面验收，也不会把未验证的 release 能力写成已完成。

## 学习路线

| Level | 主题 | 教程 | 你应该学会什么 |
| --- | --- | --- | --- |
| 0 | 环境、Shell、Git、启动 | [001 First Run](001_first_run.md)、[002 Git Basics](002_git_basics.md) | 能安装依赖、启动项目、看懂工作树状态 |
| 1 | TypeScript / React / UI | [003 Project Structure](003_project_structure.md)、[004 First UI Change](004_first_ui_change.md) | 能定位页面入口，做一个小 UI 修改并验证 |
| 2 | Zustand、Query、本地状态 | [005 First State Change](005_first_state_change.md) | 能判断状态归属，修改一个 UI store 并写测试 |
| 3 | Tauri / IPC / Bridge | [006 First IPC](006_first_ipc.md) | 能沿 Bridge -> Tauri command 查 IPC，并设计一个小命令 |

## 开始前先读

1. [00_START_HERE.md](../00_START_HERE.md)
2. [03_PROJECT_STRUCTURE.md](../03_PROJECT_STRUCTURE.md)
3. [05_STATE_OWNERSHIP.md](../05_STATE_OWNERSHIP.md)
4. [06_IPC.md](../06_IPC.md)
5. [15_TESTING.md](../15_TESTING.md)

## 学习规则

- 每次只处理一个小目标。
- 先读文档，再读真实文件。
- 先确认状态归属，再写代码。
- 任何命令通过前，只能说“局部通过”，不能扩大成“项目完成”。
- 真实桌面、恢复、OCR、导出和发布相关结论必须继续看 [16_E2E.md](../16_E2E.md)、[17_BUILD.md](../17_BUILD.md)、[18_RELEASE.md](../18_RELEASE.md)。

## 当前覆盖状态

Phase 3 当前只完成 Level 0 到 Level 3 的入门路径。更高等级的教程仍待补齐，不能把本目录当成完整培训系统。
