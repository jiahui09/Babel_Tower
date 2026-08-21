# 009 First Unit Test

## Goal

为真实缺陷写一个最小回归测试。

## Prerequisites

[005](005_first_state_change.md) 或 [007](007_first_rust_change.md)。

## Concept

测试要锁定行为，不是只覆盖行数。

## Steps

复现失败输入，写最小失败断言，修复后运行 `pnpm test` 或 `cargo test --workspace`。

## Files

`apps/desktop/src/test/`、Rust 模块内 `#[test]`。

## Expected Result

测试在修复前失败、修复后通过。

## Common Errors

不要只测试 happy path；不要伪造未运行的结果。

## Acceptance

记录命令、输出和未覆盖边界。

## Reflection

测试失败时你先看数据、调用链还是断言？为什么？
