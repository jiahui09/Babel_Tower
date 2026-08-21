# Babel Tower 故障排查

本文按 `Symptom / Cause / Diagnosis / Fix / Verification` 写。先别猜，先对照仓库事实。

## 1. `pnpm check` 失败，卡在 Prettier

- Symptom: `pnpm check` 不能通过，格式检查先报错。
- Cause: 当前仓库还有 21 个文件没有通过 Prettier。
- Diagnosis: 先跑 `pnpm --dir apps/desktop format:check`，看具体是哪几个文件。
- Fix: 只修被报出来的文件；如果你在写文档，就把文档本身格式对齐。
- Verification: `pnpm --dir apps/desktop format:check` 通过后，再跑 `pnpm check`。

## 2. 打开项目后，工作台状态恢复失败或报 `read_workspace_state` 相关错误

- Symptom: 项目内容页能进，但 tab / split / explorer 状态没有按预期恢复。
- Cause: 前端发送的读取请求和 Rust 侧期望的 DTO 不一致，当前还有双份 workspace state 持久化。
- Diagnosis: 对照 `apps/desktop/src/platform/desktop-bridge/tauri-bridge.ts` 和 `apps/desktop/src-tauri/src/lib.rs` 的 `read_workspace_state`。
- Fix: 这是代码级问题，不是文档能修掉的；当前只能把它当成已知缺口。
- Verification: 修复后，重新打开项目时不应再出现该命令错误，且 tabs / groups / explorer 状态应可恢复。

## 3. 关闭其他标签页或关闭右侧标签页后，未保存内容消失

- Symptom: bulk close 之后，dirty 内容被直接移走。
- Cause: `closeOtherTabs` 和 `closeTabsToRight` 直接删 tab，没有走单个关闭按钮的 flush 保护。
- Diagnosis: 看 `apps/desktop/src/stores/workbench.ts` 里的 bulk close reducer。
- Fix: 现在先手动保存再批量关闭；代码修复后应统一走 flush / confirmation。
- Verification: dirty tab 在批量关闭前应先保存、确认或阻止关闭，而不是直接丢失。

## 4. 设置里能看到 `wordWrap` 或 `shortcutOverrides`，但行为没变

- Symptom: 你改了设置，看起来保存成功，但编辑器换行或快捷键没有变化。
- Cause: schema 里有字段，不代表运行时真的消费了它们。
- Diagnosis: 对照 `apps/desktop/src/stores/settings.ts`、命令注册和编辑器消费点。
- Fix: 目前把它当成 partial feature；不要把 schema 存在理解成行为存在。
- Verification: 真正接上后，重启或即时应用时，编辑器行为应该真的变化。

## 5. 恢复页看起来像说明页，不像真正的恢复页

- Symptom: 进入 `/recovery/$projectId` 只看到两颗链接按钮。
- Cause: 当前路由只是静态解释页，没有把 recoverable items 做成真正的决策界面。
- Diagnosis: 看 `apps/desktop/src/routes/recovery.$projectId.tsx`。
- Fix: 这是产品缺口，不是使用姿势问题。
- Verification: 完整后，页面应列出可恢复项，并提供恢复、放弃、重试等动作。

## 6. 你想做真实桌面 E2E，但只有 fixture 测试能跑

- Symptom: Playwright 能通过，但你仍然不敢把它当真实桌面验收。
- Cause: 当前 E2E 启动的是 `VITE_DESKTOP_BRIDGE=fixture`，不是安装后的 Tauri 应用。
- Diagnosis: 看 `apps/desktop/playwright.config.ts` 和 `apps/desktop/src/main.tsx`。
- Fix: 先把它当 fixture 测试；真实桌面 E2E 需要额外的 installed-app 驱动。
- Verification: 真实桌面 E2E 通过后，必须能同时证明 IPC、文件系统、重启恢复和导出结果。

## 下一步阅读

1. [15_TESTING.md](15_TESTING.md)
2. [16_E2E.md](16_E2E.md)
3. [CURRENT_STATE.md](CURRENT_STATE.md)
