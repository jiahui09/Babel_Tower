# 011 Debugging Real Bug

## Goal

用证据修复一个已知缺陷。

## Prerequisites

[003](003_project_structure.md)、[06_IPC.md](../06_IPC.md)、[19_TROUBLESHOOTING.md](../19_TROUBLESHOOTING.md)。

## Concept

假设 -> 日志/源码证据 -> 根因 -> 最小修复 -> 回归测试。

## Steps

从任务 [P1-state-and-workbench](../tasks/P1-state-and-workbench.md) 选择一个问题，记录复现路径、观测、根因和验证。

## Files

按具体任务的 Code To Inspect 和 Dangerous Files 操作。

## Expected Result

修复可复现，且没有绕过状态所有权。

## Common Errors

不要把猜测写成根因；不要顺手重构无关模块。

## Acceptance

任务卡、回归测试、命令输出齐全。

## Reflection

什么证据会推翻你的初始假设？
