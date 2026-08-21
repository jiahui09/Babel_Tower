# Commands

命令系统分两层：

- UI 命令注册：`apps/desktop/src/commands/registry.ts`
- DesktopBridge / Tauri 命令：`apps/desktop/src/platform/desktop-bridge/tauri-bridge.ts` -> `apps/desktop/src-tauri/src/lib.rs`

## UI 命令注册

`commandRegistry` 定义了当前前端菜单与命令面板可见的命令：

- `file.import`
- `file.export`
- `edit.undo`
- `edit.redo`
- `view.commandPalette`
- `view.explorer`
- `view.inspector`
- `view.focusMode`
- `view.validation`
- `view.settings`

这些命令的快捷键是静态的，来自 `shortcut` 字段。

## 当前可见但不真正可配的点

- `shortcutOverrides` 已存在于设置 schema
- 但 `useCommandShortcuts()` 只读取 `commandRegistry`
- 因此设置界面里的快捷键相关配置当前不会影响实际快捷键解析

## 命令面板与菜单

- 菜单由 `ApplicationMenubar` 渲染
- 命令面板由 `CommandPalette` 渲染
- 快捷键由 `useCommandShortcuts()` 监听 `keydown`

三者共用同一份 `commandRegistry`

## 何时应该新增前端命令

当命令：

- 只是 UI 行为
- 不需要跨 Rust 边界
- 可以直接触发已有前端状态或路由

例如切换面板、打开设置、打开命令面板。

## 何时应该新增 IPC 命令

当命令：

- 需要访问项目文件
- 需要修改 SQLite / Rust 核心状态
- 需要执行导出、OCR、保存修订、恢复

例如保存翻译、撤销修订、写设置、工作区文件读写。

## 当前要特别注意的命令

### Undo / Redo

前端命令注册里的 Undo / Redo 会走 Rust 的修订级命令，不是编辑器本地历史。

### Export

`file.export` 只是打开导出页。真正的导出创建仍在 IPC / Rust 层。

### Validation

`view.validation` 只是打开验证面板。

### Settings

`view.settings` 打开设置对话框，但设置保存仍然由 `patchSettings()` 接管。

## 新人工作建议

如果你要改命令：

1. 先看 `commandRegistry`
2. 再看对应 `DesktopBridge`
3. 最后看 `src-tauri/src/lib.rs` 的 Tauri 命令

不要只改菜单文案。

## 下一步阅读

- [13_SETTINGS.md](13_SETTINGS.md)
- [06_IPC.md](06_IPC.md)
- `apps/desktop/src/platform/desktop-bridge/tauri-bridge.ts`
