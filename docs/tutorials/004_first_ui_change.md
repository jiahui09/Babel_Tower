# 004 First UI Change

## Goal

完成一次不改变业务语义的 React UI 修改。

这个练习建议只改一个可见文案或空状态，不碰业务数据流。

## Prerequisites

- 已完成 [003 Project Structure](003_project_structure.md)
- 已读 [07_FRONTEND.md](../07_FRONTEND.md)
- 能运行 `pnpm test`

## Concept

React 页面通常由三部分组成：

- 数据：例如 `useQuery(bootstrapQuery(bridge))`
- 状态分支：loading、error、empty、success
- 展示：组件、按钮、文案、链接

项目库首页在 `apps/desktop/src/routes/index.tsx`。它已经有 loading、error、empty 和项目列表分支。这个练习只允许改 UI 文案，不允许改项目数据流。

因为项目使用 i18n，用户可见文案不应该直接硬编码在组件里。你应修改语言资源：

- `apps/desktop/src/i18n/locales/zh-CN/workbench.json`
- `apps/desktop/src/i18n/locales/en-US/workbench.json`

## Steps

1. 读项目库首页：

   ```bash
   sed -n '1,180p' apps/desktop/src/routes/index.tsx
   ```

2. 找空状态文案 key：

   ```bash
   rg "noProjects|noProjectsDetail" apps/desktop/src
   ```

3. 打开中英文资源文件：

   ```bash
   sed -n '1,200p' apps/desktop/src/i18n/locales/zh-CN/workbench.json
   sed -n '1,200p' apps/desktop/src/i18n/locales/en-US/workbench.json
   ```

4. 修改文案。建议只改 `noProjectsDetail`，保持 key 不变。

5. 启动 Web 调试看 UI：

   ```bash
   pnpm dev:web
   ```

6. 运行前端测试：

   ```bash
   pnpm test
   ```

7. 如果你只改了文案，也可以额外运行格式检查，但注意当前全量 `pnpm check` 已知会被既有 Prettier 问题阻断：

   ```bash
   pnpm --dir apps/desktop format:check
   ```

## Files

| Path | 用途 |
| --- | --- |
| `apps/desktop/src/routes/index.tsx` | 项目库页面，包含空状态分支 |
| `apps/desktop/src/i18n/locales/zh-CN/workbench.json` | 中文文案 |
| `apps/desktop/src/i18n/locales/en-US/workbench.json` | 英文文案 |
| `apps/desktop/src/queries/project.ts` | `bootstrapQuery` 来源；本练习不要改 |
| `apps/desktop/src/platform/desktop-bridge/fixture-bridge.ts` | 测试可用的 fixture bridge；本练习通常不用改 |

## Expected Result

- 项目库为空时显示更新后的说明。
- 导入按钮仍然链接到 `/import`。
- loading、error、项目列表分支没有被改坏。
- `pnpm test` 仍能运行。

## Common Errors

| Symptom | Cause | Fix |
| --- | --- | --- |
| 页面显示 key 名而不是文案 | 资源文件缺少对应 key | 同时检查 zh-CN 和 en-US |
| 直接在 JSX 写中文 | 绕开 i18n | 把文案放回 locale JSON |
| 改了 `bootstrapQuery` | 范围扩大 | 回到 UI 文案练习，不碰数据流 |
| `pnpm check` 失败 | 可能碰到既有 Prettier 阻断 | 记录真实输出，不宣称全量门禁通过 |

## Acceptance

你完成本教程的标准：

- 只修改 UI 文案相关文件。
- 中英文资源都保持可解析 JSON。
- 能指出 `projects.isPending`、`projects.isError`、empty、success 四个分支。
- 能运行 `pnpm test`，或记录无法运行的真实原因。

## Reflection

写下你的复盘：

- 这个 UI 分支依赖哪个 Query？
- 为什么文案要放进 i18n 文件？
- 这次修改是否触碰了业务权威状态？
