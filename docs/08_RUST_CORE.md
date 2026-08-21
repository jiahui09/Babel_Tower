# 08_RUST_CORE

Rust 是 Babel Tower 的权威业务层。前端可以决定“用户点了什么”，但真正的保存、恢复、导入、导出、验证、OCR 和资源处理都在这里收口。

## 分层

| 层                    | 代表目录/文件                         | 职责                                    | 不负责什么            |
| --------------------- | ------------------------------------- | --------------------------------------- | --------------------- |
| Domain                | `crates/babel-domain/src/`            | 稳定 ID、状态枚举、工作台语义           | IO、UI、SQL           |
| Application / Service | `crates/babel-application/src/lib.rs` | `Kernel`，把用例串起来                  | React、路由、窗口状态 |
| Storage               | `crates/babel-storage/src/`           | SQLite、CAS、恢复、导航、草稿、导出记录 | UI 布局               |
| Runtime               | `crates/babel-runtime/src/`           | IPC frame、worker handshake、进程控制   | 业务规则本身          |
| Tauri adapter         | `apps/desktop/src-tauri/src/lib.rs`   | 把 UI 请求翻译成 `Kernel` 调用          | 领域规则              |

## 主要模块

### `crates/babel-domain`

这里放稳定且可序列化的领域语义：

- `ProjectId`、`UnitId`、`TaskId`、`GenerationId`、`ResourceId`
- `WorkPriority`
- `TaskState`
- `RevisionKind`
- `WorkspaceView`
- `TranslationStatus`
- `NavigationPosition`

如果一个概念会被前端、存储层和应用层共同使用，而且语义要长期稳定，优先放这里。

### `crates/babel-application`

这是最常修改的业务层。`Kernel` 的典型职责包括：

- 打开项目
- 导入 TXT / Markdown / EPUB
- 保存翻译、草稿、图片区域编辑
- 撤销 / 重做
- 读取 work item、验证、导出
- OCR 结果落库
- 资源队列和绑定决策
- 工作区操作恢复

顶层结构上可以把它理解成：

- 输入：前端命令、worker 结果、文件路径
- 处理：校验、编排、重试、受控文件读写
- 输出：`SaveReceipt`、导入报告、验证报告、导出记录、查询结果

### `crates/babel-storage`

这是 SQLite 和恢复的事实源。关键对象包括：

- `ProjectStore`
- `SaveReceipt`
- `DraftRecovery`
- `NavigationSaveReceipt`
- `WorkspaceOperationRecord`
- `ImageRegionEditRecord`
- `ExportRecord`

核心表/概念包括：

- translation revision / unit head / command receipt
- draft session
- object record / object reference
- import generation / resource / unit / binding
- export record
- navigation position
- workspace operation

### `crates/babel-runtime`

这是 worker 和 IPC 的低层运行时：

- `ipc.rs`：帧大小限制、握手、消息结构
- `process_worker.rs`：子进程启动、握手、请求/响应、取消、超时、错误诊断
- `dag.rs`：持久化 DAG 相关工具

## 修改一个业务能力该去哪里

| 需求                   | 先改哪里                                          | 再改哪里                                                  |
| ---------------------- | ------------------------------------------------- | --------------------------------------------------------- |
| 新增一个翻译保存规则   | `crates/babel-application/src/lib.rs`             | `crates/babel-storage/src/project.rs`，必要时加 Rust 测试 |
| 新增一个领域状态枚举   | `crates/babel-domain/src/`                        | `babel-storage`、`apps/desktop` 的 DTO                    |
| 新增一个 Tauri 命令    | `apps/desktop/src-tauri/src/lib.rs`               | `apps/desktop/src/platform/desktop-bridge/`               |
| 调整草稿/撤销/重做语义 | `babel-storage/src/project.rs`                    | `babel-application/src/lib.rs`                            |
| 调整导入/导出/验证流程 | `babel-application/src/lib.rs`                    | 对应 worker adapter crate                                 |
| 调整 worker 帧协议     | `babel-runtime/src/ipc.rs` 和 `process_worker.rs` | 所有 worker 端实现                                        |

## 错误处理

当前结构是：

- storage/application 里大量使用 `Result<T, ErrorType>`；
- `Kernel` 再把很多错误向上传成自己的 `KernelError`；
- Tauri 命令大多再转成 `Result<T, String>` 给前端。

这意味着：

- Rust 内部可以保留结构化错误；
- 到 IPC 边界后，很多错误会被字符串化；
- 前端要做冲突判断时，不能只依赖类型，常常还要看错误文案。

## Async / worker

不是所有耗时逻辑都在 async runtime 里。这个仓库大量使用：

- worker 子进程
- 同步数据库事务
- Rust 线程
- 有界 channel

`ProcessWorker` 和 `Kernel` 的设计目标是把长任务隔离出去，同时维持可恢复、可验证的边界。

## 现在的实际边界

- `src-tauri/lib.rs` 不是业务核心，它是 adapter 层。
- `Kernel` 才是应用层的统一入口。
- `ProjectStore` 是数据一致性的根。
- worker crate 只能做协议内的事，不能直接改项目权威存储。

下一步建议阅读 `apps/desktop/src-tauri/src/lib.rs` 和 `crates/babel-storage/src/project.rs`，因为前端看到的大部分数据最终都从这里流出。
