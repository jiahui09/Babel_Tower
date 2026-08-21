# TASK-P0-001 Recovery State Machine

## Background

Storage 已有 draft/recovery 基础，但 `recovery.$projectId.tsx` 仍主要是导航。

## Problem

缺少 Detection -> Candidate -> User Decision -> Restore/Discard/Retry -> Verification -> Return 的用户闭环。

## Why It Matters

恢复错误可能造成未保存译文丢失或错误项目串入。

## What You Need To Learn

[12_RECOVERY.md](../12_RECOVERY.md)、[05_STATE_OWNERSHIP.md](../05_STATE_OWNERSHIP.md)。

## Code To Inspect

`apps/desktop/src/routes/recovery.$projectId.tsx`、`crates/babel-storage/src/recovery.rs`、`crates/babel-application/src/lib.rs`。

## Allowed Files

上述恢复 UI、Bridge、Application、Storage 及对应测试。

## Dangerous Files

revision、draft schema、项目隔离和导出路径。

## Requirements

显式状态机；恢复/丢弃/重试有成功、错误、空状态；项目隔离；恢复后可验证并返回原位置。

## Suggested Implementation Path

先画状态转移和 DTO，再锁定 storage 行为测试，最后接 Bridge/UI。

## Tests

Rust recovery tests、组件状态转移测试和失败路径测试。

## E2E

需要真实桌面：异常退出/重启、恢复后文件系统与 revision 验证。

## Acceptance Criteria

每个状态可达、不可逆动作有确认、失败不丢候选；证据记录在 acceptance matrix。

## Common Failure Modes

把 draft 当 revision；只显示页面不执行动作；跨项目恢复。

## Definition of Done

按 [definition-of-done.md](../acceptance/definition-of-done.md) 标注适用项。
