# Editor

这里的 Editor 指两类东西：

- `TranslationEditor`：结构化翻译编辑器
- `WorkspaceFileEditor`：工作区文件编辑器

它们共享一部分保存和脏态逻辑，但不是同一个模型。

## 真实入口

- `apps/desktop/src/components/workbench/translation-editor.tsx`
- `apps/desktop/src/components/workbench/workspace-file-editor.tsx`
- `apps/desktop/src/components/workbench/secondary-editor-group.tsx`
- `apps/desktop/src/routes/projects.$projectId.content.tsx`
- `apps/desktop/src/routes/projects.$projectId.units.tsx`
- `apps/desktop/src/routes/projects.$projectId.resources.tsx`

## TranslationEditor

`TranslationEditor` 使用 Tiptap，把 `TranslationDocumentV1` 和可编辑文本互相转换。

关键函数：

- `documentToTiptap()`
- `tiptapToDocument()`
- `projectDocumentText()`

### 保存链

1. 用户输入触发 `onUpdate`
2. 当前文档被转换成 `TranslationDocumentV1`
3. `setSaveState("editing")`
4. 650ms 后自动调用 `onPersist`
5. 成功后标记为 `saved`
6. 失败后标记为 `error` 或 `conflict`

### 关键约束

- 保存时会带 `expectedRevisionId`
- Rust 端会校验修订头
- 冲突不是前端伪造的，它来自后端拒绝

### 脏态

`TranslationEditor` 自己维护：

- `dirty.current`
- `changeVersion.current`
- `latestDocument.current`

它会把脏态同步给工作台 `markTabDirty()`，但这个同步只是协作，不是唯一权威。

## 本地撤销 / 重做

编辑器工具栏里的撤销 / 重做是 Tiptap 本地历史操作。它不是 Rust 的 `undo_translation` / `redo_translation` 命令。

真正的修订级撤销 / 重做命令在：

- `apps/desktop/src/commands/registry.ts`
- `apps/desktop/src/platform/desktop-bridge/tauri-bridge.ts`
- `crates/babel-application/src/lib.rs`
- `crates/babel-storage/src/project.rs`

这两条链不要混为一谈。

## WorkspaceFileEditor

这个编辑器直接操作工作区文件：

- `readWorkspaceFile()`
- `writeWorkspaceFile()`

保存时会传 `expectedModifiedAtMs`，用于检测外部修改。

它的脏态由：

- `CodeMirrorView` 的 `onChange`
- `markTabDirty()`
- `registerTabFlusher()`

共同驱动。

## SecondaryEditorGroup

secondary 分组里，workspace 文件会根据 `mirrorsPrimary` 变成只读镜像。翻译编辑器则复用同一修订工作项，但在 secondary 中按视图规则处理。

这说明分屏是“同一文档的不同视图”，不是复制一份数据。

## 现在已知的限制

- `wordWrap` 已存在于设置 schema，但没有看到编辑器消费链。
- `shortcutOverrides` 已存在于设置 schema，但命令快捷键仍是静态定义。
- `copyDraft` 只是把当前编辑器文档转成纯文本复制到剪贴板，不是持久化草稿。
- `reloadFromCore()` 会放弃本地未保存改动。

## 修改建议

改编辑器时先判断你在改哪层：

- 文本渲染：Tiptap / CodeMirror
- 保存协议：DesktopBridge / Rust
- 脏态与关闭：WorkbenchStore / DocumentTabs

## 下一步阅读

- [05_STATE_OWNERSHIP.md](05_STATE_OWNERSHIP.md)
- [09_WORKBENCH.md](09_WORKBENCH.md)
- [13_SETTINGS.md](13_SETTINGS.md)
- `crates/babel-storage/src/project.rs`
