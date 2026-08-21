# TASK-P0-004 Documentation Truth Source

## Background

`DESIGN.md`、`PROJECT_STATUS.md`、代码和验收材料存在过期或互相矛盾表述。

## Problem

新人无法判断哪个状态是真实事实源。

## Why It Matters

错误文档会导致错误实现和错误验收。

## What You Need To Learn

[CURRENT_STATE.md](../CURRENT_STATE.md)、归档的 Phase 1 调查材料、代码测试证据。

## Code To Inspect

`DESIGN.md`、`PROJECT_STATUS.md`、`docs/archive/phase1-analysis/`、CI 配置。

## Allowed Files

文档、文档索引和验收记录；不得为了配合旧文档修改代码。

## Dangerous Files

不要删除历史证据；不要把推测改成 Complete。

## Requirements

指定 CURRENT_STATE 为当前入口，历史文档注明日期/范围，冲突保留证据和决策。

## Suggested Implementation Path

列冲突 -> 逐项回到代码/测试 -> 修正文档 -> 加链接和更新时间。

## Tests

Markdown 链接、格式、命令与文件路径核验。

## E2E

不适用；文档不得宣称 E2E 已存在。

## Acceptance Criteria

新人模拟时每个状态只有一个明确解释。

## Common Failure Modes

只重写 README；删除矛盾材料导致历史不可追溯。

## Definition of Done

覆盖报告更新，且所有新结论有仓库证据。
