# 002 Git Basics

## Goal

学会在不破坏别人改动的前提下查看工作树、理解 diff，并为一次小修改做准备。

完成后，你应该能回答：

- 当前有哪些未提交文件？
- 哪些文件是你准备修改的？
- 为什么不能随手运行 `git reset --hard`？

## Prerequisites

- 已完成 [001 First Run](001_first_run.md)
- 能在仓库根目录运行 shell 命令
- 先读 [03_PROJECT_STRUCTURE.md](../03_PROJECT_STRUCTURE.md)

## Concept

Git 是项目记忆。你在 Babel Tower 里开发时，第一件事不是写代码，而是确认工作树状态。

本仓库当前可能已有文档变更，例如 `README.md` 和 `docs/`。这些改动可能来自上一阶段工作或其他协作者。你必须保护它们。

安全原则：

- 可以查看状态、查看 diff、创建新分支。
- 不要回滚你不理解的文件。
- 不要用破坏性命令清空工作区。

## Steps

1. 查看当前分支和工作树：

   ```bash
   git status --short
   git branch --show-current
   ```

2. 查看某个文件的变更：

   ```bash
   git diff -- docs/tutorials/README.md
   ```

3. 查看未跟踪文件：

   ```bash
   git ls-files --others --exclude-standard
   ```

4. 如果你要开始一个练习分支：

   ```bash
   git switch -c tutorial-first-ui-change
   ```

5. 修改前先记录你的目标文件。例如 Level 1 UI 练习通常只需要：

   ```text
   apps/desktop/src/routes/index.tsx
   apps/desktop/src/i18n/locales/zh-CN/workbench.json
   apps/desktop/src/i18n/locales/en-US/workbench.json
   ```

## Files

| Path | 用途 |
| --- | --- |
| `.git/` | Git 的本地数据，不要手动编辑 |
| `README.md` | 新人入口，可能已有阶段性文档变更 |
| `docs/` | 开发知识库和教程 |
| `apps/desktop/src/routes/index.tsx` | 后续 UI 练习会看的页面入口 |

## Expected Result

你能得到三类信息：

- 当前分支名。
- 哪些文件已修改或未跟踪。
- 你准备改动的文件是否已经有别人改动。

## Common Errors

| Symptom | Cause | Fix |
| --- | --- | --- |
| 不知道 `??` 是什么 | `git status --short` 用 `??` 表示未跟踪文件 | 用 `git ls-files --others --exclude-standard` 查看 |
| diff 很大看不懂 | 一次看了整个仓库 | 对单个文件运行 `git diff -- path` |
| 改到别人文件 | 修改前没看状态 | 先看 `git status --short`，再限定目标文件 |
| 想“恢复干净” | 误以为未提交文件都没用 | 停下，先确认每个文件来源 |

## Acceptance

你完成本教程的标准：

- 能解释 `git status --short` 中 `M` 和 `??` 的意义。
- 能查看单个文件 diff。
- 能说出本次练习准备修改哪些文件。
- 没有运行破坏性 Git 命令。

## Reflection

记录你的答案：

- 当前工作树里有哪些未提交文件？
- 其中哪些与你的练习有关？
- 如果发现目标文件已经被别人修改，你会怎么做？
