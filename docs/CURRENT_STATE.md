# CURRENT_STATE

更新时间：2026-08-20。

本文只根据仓库代码、当前配置和归档的 Phase 1 验证结果写结论，不把历史文档当事实源。Phase 1 材料位于 [docs/archive/phase1-analysis/](archive/phase1-analysis/)。

## 总结

Babel Tower 的核心翻译、导入、保存、验证、导出和 worker 链路已经存在；桌面壳的状态恢复、真实 E2E 和最终发布闭包还没有完成。

## 状态矩阵

| 领域                    | 状态     | 事实                                                                     |
| ----------------------- | -------- | ------------------------------------------------------------------------ |
| Rust workspace 测试     | Complete | `cargo test --workspace` 已通过                                          |
| 前端单元 / 组件测试     | Complete | `pnpm test` 已通过，覆盖 7 个文件、14 个测试                             |
| Fixture E2E             | Partial  | 配置和用例存在且只启动 fixture bridge；当前环境缺 Chromium，未能新鲜执行 |
| 真实桌面 E2E            | Missing  | 没有 installed-app / WebDriver / tauri-driver 链路                       |
| 前端质量门禁            | Partial  | `pnpm check` 仍会被 Prettier 阻断                                        |
| 架构方向检查            | Complete | `./tools/check-architecture.sh` 已通过                                   |
| 工作区会话恢复          | Partial  | 实现存在，但 `read_workspace_state` DTO 和双份持久化仍有风险             |
| Dirty / bulk close 保护 | Partial  | 单个关闭有保护，bulk close 仍会绕过 flush                                |
| Settings / commands     | Partial  | schema 存在，但 `wordWrap`、`shortcutOverrides` 等行为并未完全接上       |
| Recovery UI             | Missing  | 路由存在，但只是说明页，不是恢复决策页                                   |
| OCR 开发路径            | Partial  | Linux 资源和代码路径存在，真实桌面 / release 闭环未证实                  |
| Windows 原生验证        | Blocked  | 需要 Windows 机器或 runner 才能完成                                      |
| 发布闭包                | Missing  | 字体、许可证、SBOM、安装器和实机验收都没有完整闭环                       |

## 现在最重要的已知风险

1. `read_workspace_state` 的请求/响应边界不一致，会影响项目重开后的状态恢复。
2. bulk close 会绕过 dirty flush 保护，有数据丢失风险。
3. `wordWrap` 和 `shortcutOverrides` 是 schema 里存在的字段，不等于行为已存在。
4. fixture E2E 不能替代真实桌面 E2E。
5. 发布材料不能被历史 Phase 3 TXT 产物冒充。

## Phase 2 文档已补上的知识

- 新人入口、产品工作流、目录定位、架构边界和数据模型。
- 状态所有权、IPC、前端、Rust、工作台、编辑器、文件系统、恢复、设置与命令。
- 如何跑现有测试，fixture E2E 与真实桌面 E2E 的区别，以及构建、发布、排障边界。
- ADR、高风险修改的验证要求及统一术语。

## 仍然缺失的产品与验证能力

- 真正的 installed-app E2E 还没有。
- Windows 实机闭环还没有。
- 完整的用户恢复决策 UI、快捷键覆盖和 word-wrap 行为还没有。

## 下一步阅读

1. [15_TESTING.md](15_TESTING.md)
2. [16_E2E.md](16_E2E.md)
3. [18_RELEASE.md](18_RELEASE.md)
