# 012 First Release

## Goal

学会判断一次发布是否真的可重复。

## Prerequisites

[010](010_first_e2e.md)、[17_BUILD.md](../17_BUILD.md)、[18_RELEASE.md](../18_RELEASE.md)。

## Concept

构建成功不等于安装、运行、OCR、导出和合规完成。

## Steps

按 [release-gates.md](../acceptance/release-gates.md) 逐项收集证据；缺 Windows、runtime、字体或 SBOM 证据时标记 Blocked/Missing。

## Files

`packaging/`、`release/`、发布工作流和 `docs/acceptance/`。

## Expected Result

得到可审计的 gate 表和 artifact 清单，而不是一句“build passed”。

## Common Errors

不要用历史 Phase 3 TXT 产物代表当前桌面产品。

## Acceptance

Release Notes、版本、平台、验证命令和剩余风险完整。

## Reflection

哪一个发布门禁目前最依赖外部 Windows 环境？
