# 数据模型

## 实体关系

```text
Project
  -> Workspace session (Tabs / Groups / Split)
  -> Document
       -> Translation Unit
            -> Translation -> Revision / Draft
       -> Resource -> OCR candidate / rendered asset
  -> Validation result
  -> Export
  -> Recovery session
```

## 实体说明

| 实体             | 作用                                  | 当前事实来源                                                  |
| ---------------- | ------------------------------------- | ------------------------------------------------------------- |
| Project          | 项目身份、根目录和活动 Kernel 边界    | Tauri registry + Kernel                                       |
| Workspace        | 当前项目的标签、分组、分屏会话        | Workbench store 与 `.config/workspace-state.json`（存在双写） |
| Document         | 导入后的文档身份和结构                | `babel-domain` / storage                                      |
| Translation Unit | 可编辑的最小翻译单元                  | Rust domain 与 workbench 投影                                 |
| Resource         | 图片等非文本资源                      | resource graph / storage                                      |
| Translation      | 单元当前译文                          | SQLite revision/head                                          |
| Status           | 翻译状态和校验投影                    | Rust `TranslationStatus`                                      |
| Revision         | 可追溯的已保存版本                    | `translation_revision`、`unit_head`                           |
| Draft            | 尚未成为正式 revision 的恢复内容      | `draft_session`                                               |
| Export           | 从保存事实生成的目标文件              | application export use cases                                  |
| Recovery         | 启动后发现草稿/异常会话时的恢复上下文 | storage + recovery commands                                   |

## 关键关系

稳定 identity 贯穿原始格式、内部单元、revision 和导出映射；不要使用数组下标替代 identity。保存一次 revision 后，undo/redo 通过新增修订表达，不覆盖历史。

## 当前限制

`workbench_snapshot` 的 UnitSummary 没有完整的 `TranslationStatus` 字段，前端部分页面仍根据译文文本存在与否推导显示状态。Recovery UI 和跨重启真实验收未完成。

下一步阅读：[05_STATE_OWNERSHIP.md](05_STATE_OWNERSHIP.md)、[08_RUST_CORE.md](08_RUST_CORE.md)。
