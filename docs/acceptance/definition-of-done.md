# Definition of Done

每个任务必须在任务卡中标记适用项，并附命令、日志、截图或文件证据。

| 项目          | 检查                                        |
| ------------- | ------------------------------------------- |
| Core          | [ ] 业务规则在正确层实现                    |
| IPC / Bridge  | [ ] DTO、成功和错误契约一致                 |
| UI            | [ ] 真实事件、loading、error、empty/success |
| Persistence   | [ ] 持久化、项目隔离、重启语义明确          |
| Tests         | [ ] Unit [ ] Integration [ ] E2E（适用时）  |
| Documentation | [ ] 路径、限制、下一步阅读已更新            |

涉及桌面、文件、恢复、OCR 或导出时，必须额外考虑 Restart、Filesystem、Worker、Runtime 和 Release；缺少环境时标记 `BLOCKED`，不得跳过。
