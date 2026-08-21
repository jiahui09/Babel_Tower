# Glossary

| 术语              | Babel Tower 中的含义                                       |
| ----------------- | ---------------------------------------------------------- |
| Project           | 有根目录、注册身份和活动 Kernel 的本地翻译项目             |
| Workspace         | 项目内的标签、分组、分屏和当前面板会话                     |
| Document          | 导入后可被工作台打开的文档实体                             |
| Translation Unit  | 文档中可独立编辑、保存和校验的翻译单元                     |
| Resource          | 图片等非文本资产及其派生数据                               |
| Revision          | 已保存且可追溯的译文版本；undo/redo 通过修订链表达         |
| Draft             | 尚未提交为正式 revision 的编辑恢复内容                     |
| Dirty             | 当前编辑器或标签存在未安全提交的变更                       |
| TranslationStatus | Rust domain 的业务状态，不等同于前端文本是否为空           |
| DesktopBridge     | React 与 Tauri 命令之间的类型化接口                        |
| Kernel            | `babel_application` 的应用服务入口，协调领域、存储和适配器 |
| CAS               | Content-addressed storage，按内容寻址保存对象              |
| Recovery          | 从草稿或异常会话中恢复用户工作的流程                       |
| Fixture bridge    | 仅用于开发/测试的内存 DesktopBridge，不代表真实桌面能力    |
| Validation        | 对译文、资源或导出前条件生成的检查结果                     |
| Export            | 从已保存项目事实生成目标格式文件的流程                     |

术语边界和状态归属见 [04_DATA_MODEL.md](04_DATA_MODEL.md) 与 [05_STATE_OWNERSHIP.md](05_STATE_OWNERSHIP.md)。
