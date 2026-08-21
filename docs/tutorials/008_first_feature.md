# 008 First Feature

## Goal

完成一个从需求到 E2E 证据的跨层小功能。

## Prerequisites

[001-007](001_first_run.md)、[02_ARCHITECTURE.md](../02_ARCHITECTURE.md)。

## Concept

Requirement -> Design -> Core -> IPC -> UI -> Test -> E2E。

## Steps

写一页设计，列状态 owner 和危险文件；沿现有模式实现；为 Core、Bridge、UI 分别列证据。桌面能力缺少真实 E2E 时必须标记缺口。

## Files

按功能查 [03_PROJECT_STRUCTURE.md](../03_PROJECT_STRUCTURE.md)。

## Expected Result

功能可定位、可测试、可解释，而不是只画出 UI。

## Common Errors

不要先写组件再猜 IPC 和持久化。

## Acceptance

完成任务卡的 Definition of Done 子集。

## Reflection

哪一项证据仍需要真实桌面环境？
