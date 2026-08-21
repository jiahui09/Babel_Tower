# P1 State and Workbench

## Background

当前存在多个真实 P1 缺陷：设置字段未消费、`panelWidths` 双源、状态投影不统一、bulk close 绕过 dirty flush、项目切换隔离风险。

## Problem

状态 schema、store、UI 和 Rust 投影可能互相漂移。

## Why It Matters

会造成数据丢失、错误显示或重启后布局串项目。

## What You Need To Learn

[05_STATE_OWNERSHIP.md](../05_STATE_OWNERSHIP.md)、[09_WORKBENCH.md](../09_WORKBENCH.md)、[13_SETTINGS.md](../13_SETTINGS.md)。

## Code To Inspect

`stores/workbench.ts`、`stores/settings.ts`、`components/workbench/document-tabs.tsx`、`app-shell.tsx`、units route 和 Rust workbench projection。

## Allowed Files

对应 store、组件、Bridge、Rust DTO 与回归测试。

## Dangerous Files

Identity、revision、workspace persistence、TranslationStatus。

## Requirements

为每项行为指定唯一 owner；bulk close 复用 dirty flush；设置真实驱动运行时并持久化；项目切换隔离；状态由 Rust 投影。

## Suggested Implementation Path

先写状态矩阵和失败测试，再收敛 owner，最后做 UI 和重启验证。

## Tests

store、组件、IPC DTO、Rust projection 和 persistence tests。

## E2E

真实桌面重启、dirty close、项目切换和设置生效。

## Acceptance Criteria

无第二权威源；所有改动有回归证据。

## Common Failure Modes

只修 UI 显示；保留两个 persist 却不定义冲突规则。

## Definition of Done

按危险修改清单逐项验证。
