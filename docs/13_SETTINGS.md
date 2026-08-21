# Settings

Settings 是 Babel Tower 的桌面偏好设置系统。它有 schema、IPC、前端镜像和局部行为，但这些层并不等价。

## 真实入口

- `apps/desktop/src/components/settings/settings-dialog.tsx`
- `apps/desktop/src/stores/settings.ts`
- `apps/desktop/src/platform/desktop-bridge/types.ts`
- `apps/desktop/src/platform/desktop-bridge/tauri-bridge.ts`
- `apps/desktop/src-tauri/src/lib.rs`

## 当前 schema

`AppSettingsV1` 目前包含：

- `language`
- `theme`
- `density`
- `editorFontFamily`
- `readingFontSize`
- `lineHeight`
- `wordWrap`
- `shortcutOverrides`
- `panelWidths`

## 持久化路径

- 桌面配置文件：`app_config_dir/settings-v1.json`
- 前端持久化：`babel-tower-settings-v1`

前端 store 是镜像，不是权威。

## 已接通的行为

- 语言会驱动 `i18n.changeLanguage()`
- 主题会驱动 `useTheme()`
- density 会写到 `documentElement.dataset.density`
- 字体、字号、行高会写成 CSS 变量
- `patchSettings()` 失败时会回滚前端镜像

## 当前缺口

### `wordWrap`

字段存在、开关存在，但当前代码里没有找到编辑器消费链。也就是说，用户能切换这个选项，但未见它改变编辑器行为。

### `shortcutOverrides`

字段存在、会写入/读出，但命令注册仍由 `apps/desktop/src/commands/registry.ts` 的静态 `shortcut` 决定。没有看到运行时重映射。

### `panelWidths`

`panelWidths` 现在出现在两处：

- `AppSettingsV1.panelWidths`
- `useWorkbenchStore` 的 `explorerWidth` / `inspectorWidth`

这是一条已确认的双源问题。文档上不能把它说成“统一配置”。

## 读写流程

`SettingsDialog` 的模式是：

1. 本地先改前端 store
2. 立刻调用 `bridge.patchSettings()`
3. 成功后用后端返回值覆盖镜像
4. 失败则回滚到上一个持久化快照

这说明它不是“先编辑、后统一提交”的批量表单，而是逐项提交。

## 命令与设置的关系

不要误读 schema：

- schema 存在，不等于命令已经接通
- 界面控件存在，不等于运行时行为已经生效

这对 `shortcutOverrides` 和 `wordWrap` 尤其重要。

## 下一步阅读

- [14_COMMANDS.md](14_COMMANDS.md)
- [05_STATE_OWNERSHIP.md](05_STATE_OWNERSHIP.md)
- `apps/desktop/src/commands/registry.ts`
