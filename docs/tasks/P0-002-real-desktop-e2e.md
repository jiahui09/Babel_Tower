# TASK-P0-002 Real Desktop E2E

## Background

Playwright 当前配置使用 fixture bridge，真实 Tauri E2E 缺失。

## Problem

无法证明创建、导入、编辑、保存、重启、恢复、Explorer、OCR 和 Export 的桌面闭环。

## Why It Matters

fixture 不能验证 Tauri command、真实 filesystem、worker 和重启。

## What You Need To Learn

[16_E2E.md](../16_E2E.md)、[11_FILE_SYSTEM.md](../11_FILE_SYSTEM.md)。

## Code To Inspect

`apps/desktop/playwright.config.ts`、`apps/desktop/e2e/`、Tauri 启动脚本。

## Allowed Files

E2E harness、fixture data、测试配置和文档。

## Dangerous Files

真实用户目录、测试隔离目录、安装器和 worker runtime。

## Requirements

隔离临时目录；覆盖 Create -> Import -> Edit -> Save -> Restart -> Recover；断言 filesystem、OCR、Export。

## Suggested Implementation Path

先建立最小 Tauri smoke，再逐步添加文件、恢复和 worker 断言。

## Tests

保留 fixture 测试，并分别标记真实桌面 suite。

## E2E

本任务本身就是 E2E，需在支持桌面依赖的环境运行。

## Acceptance Criteria

报告启动方式、平台、artifact、日志和失败截图；不能用 fixture 结果替代。

## Common Failure Modes

测试复用开发者目录；只断言 DOM；未清理进程。

## Definition of Done

完成 [definition-of-done.md](../acceptance/definition-of-done.md) 中适用的 Desktop/Restart 项。
