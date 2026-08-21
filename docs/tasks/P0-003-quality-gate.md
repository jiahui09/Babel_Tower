# TASK-P0-003 Quality Gate

## Background

当前测试可通过，但前端 `pnpm check` 的 Prettier 门禁仍有历史失败文件。

## Problem

typecheck、eslint、prettier、build、cargo test 不能形成全绿、可重复的门禁。

## Why It Matters

开发者无法区分代码回归和格式/环境噪声。

## What You Need To Learn

[15_TESTING.md](../15_TESTING.md)、[17_BUILD.md](../17_BUILD.md)。

## Code To Inspect

根 `package.json`、`apps/desktop/package.json`、CI workflows。

## Allowed Files

被门禁点名的格式、配置和测试文件。

## Dangerous Files

不要为了通过检查而放宽规则或跳过测试。

## Requirements

typecheck、eslint、prettier、Vite build、cargo test 和架构检查均有命令与 CI 证据。

## Suggested Implementation Path

先复现并分类失败，再逐类修复，最后在干净工作树重跑。

## Tests

`pnpm test`、`pnpm check`、`cargo test --workspace`、`./tools/check-architecture.sh`。

## E2E

记录 Chromium/桌面依赖缺失，不伪造通过。

## Acceptance Criteria

门禁全绿或每项明确状态与阻塞原因。

## Common Failure Modes

只运行单个命令；把历史输出当新鲜证据。

## Definition of Done

所有适用质量门禁和文档证据均完成。
