# 产品与核心工作流

## 面向谁

Babel Tower 面向需要在本地处理长文、章节和图片资源的个人译者。它优先保证本地文件可控、译文可追溯、导出不覆盖源文件。

## 用户工作流

```text
创建项目 -> 导入 TXT/Markdown/EPUB -> 浏览文件树 -> 打开文档
-> 编辑译文 -> 保存 revision -> 校验 -> 导出新文件
```

图片资源可进入 OCR 候选与区域编辑流程；OCR 是候选输入，不是自动发布的译文。项目、工作区和导出均按项目隔离。

## 生命周期

- 项目：由桌面注册表创建并绑定一个项目根目录，随后由 Kernel 打开。
- 文档：导入器把外部格式转换为内部稳定身份和可编辑单元。
- 翻译：编辑产生 revision；草稿用于恢复未完成输入；导出读取已保存事实。
- 校验：对译文和资源生成问题列表，结果不应替代 revision。
- 导出：通过安全导出链写入新文件或目标目录，不能把 UI 草稿直接当作导出源。

## 当前边界

Recovery 页面目前主要承担导航，完整的用户恢复决策 UI 尚未形成闭环。真实 Tauri E2E、Windows 安装包和 OCR runtime 发布验证也未完成，见 [CURRENT_STATE.md](CURRENT_STATE.md)。

## 下一步

先读 [04_DATA_MODEL.md](04_DATA_MODEL.md)，再读 [09_WORKBENCH.md](09_WORKBENCH.md) 和 [10_EDITOR.md](10_EDITOR.md)。
