# 010 First E2E

## Goal

理解 fixture E2E 与真实 Tauri E2E 的证据边界。

## Prerequisites

[009](009_first_unit_test.md)、Playwright 基础。

## Concept

当前配置 `apps/desktop/playwright.config.ts` 使用 fixture bridge；它不能证明 filesystem、重启或安装包。

## Steps

运行 `pnpm test:e2e`（需 Chromium），阅读 fixture app，再把场景拆成真实桌面所需的外部证据清单。

## Files

`apps/desktop/e2e/`、`playwright.config.ts`、`src/test/fixture-app.ts`。

## Expected Result

能明确写出“本测试证明什么/不能证明什么”。

## Common Errors

不要把 fixture 通过写成产品 E2E 通过。

## Acceptance

输出 Create -> Import -> Edit -> Save -> Restart -> Recover 的缺口表。

## Reflection

哪一步必须检查真实文件系统？
