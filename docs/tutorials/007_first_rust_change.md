# 007 First Rust Change

## Goal

修改一个真实 Rust 行为并保持错误边界。

## Prerequisites

[006](006_first_ipc.md)、Rust `Result`/`Option`/ownership 基础。

## Concept

Domain 定义语义，Application 编排，Storage 持久化；Tauri 不是业务层。

## Steps

选择已有 Application 用例的小边界，先读相关测试和错误类型，添加回归测试，再修改实现。

## Files

`crates/babel-domain/`、`crates/babel-application/src/lib.rs`、`crates/babel-storage/`。

## Expected Result

`cargo test --workspace` 通过，错误仍可诊断。

## Common Errors

不要用字符串吞掉领域错误或绕过 Kernel。

## Acceptance

说明 ownership 选择和失败路径。

## Reflection

哪一层应该拥有新规则？
