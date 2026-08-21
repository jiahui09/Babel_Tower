# 03_PROJECT_STRUCTURE

这是 Babel Tower 的新手定位图。仓库不是单体 `src/` 项目，而是一个桌面端前端 + Tauri 桥 + Rust 核心 + 存储/worker 的 monorepo。

先记住一条主线：

`apps/desktop/src/main.tsx` → `AppProviders` → 路由页/Workbench → `DesktopBridge` → `apps/desktop/src-tauri/src/lib.rs` → `babel-application::Kernel` → `babel-storage` / worker / 文件系统。

## 先去哪里

| 问题类型                                    | 第一站                                                                             | 原因                                  |
| ------------------------------------------- | ---------------------------------------------------------------------------------- | ------------------------------------- |
| 页面不渲染、交互错位、按钮失效              | `apps/desktop/src/components/`、`apps/desktop/src/routes/`                         | 这些目录放 React 视图、路由和局部交互 |
| 数据拉取、缓存、刷新不对                    | `apps/desktop/src/queries/project.ts`                                              | TanStack Query 的查询入口都在这里     |
| UI 会话状态不对，比如标签页、分栏、面板开关 | `apps/desktop/src/stores/`                                                         | 这些状态主要在 Zustand 里             |
| 桌面桥调用失败、命令名不对、DTO 不匹配      | `apps/desktop/src/platform/desktop-bridge/` 和 `apps/desktop/src-tauri/src/lib.rs` | 这是前端和 Rust 的契约边界            |
| 保存、导入、验证、导出、恢复逻辑不对        | `crates/babel-application/src/lib.rs`                                              | `Kernel` 是应用层编排中心             |
| SQLite、修订链、草稿、恢复记录不对          | `crates/babel-storage/src/`                                                        | 这里是持久化和恢复事实源              |
| ID、状态枚举、工作台状态语义不对            | `crates/babel-domain/src/`                                                         | 这里放稳定领域类型                    |
| IPC 帧、worker 握手、进程控制不对           | `crates/babel-runtime/src/`                                                        | 这里是运行时协议和 worker 控制        |

## 关键文件

| Path                                        | Purpose            | Responsibilities                                                         | Dependencies                                                     | Callers                          | Related Concepts                                 | Modification Risk            |
| ------------------------------------------- | ------------------ | ------------------------------------------------------------------------ | ---------------------------------------------------------------- | -------------------------------- | ------------------------------------------------ | ---------------------------- |
| `apps/desktop/src/main.tsx`                 | 前端入口           | 创建 hash router，选择 Tauri bridge 或 fixture bridge，挂载全局 provider | React、TanStack Router、`AppProviders`                           | Vite 启动的页面入口              | 启动路径、开发/生产差异                          | 高：桥选择会改变整个平台行为 |
| `apps/desktop/src/app/providers.tsx`        | 全局 provider 组合 | 注入 i18n、主题、QueryClient、DesktopBridge 和 settings hydration        | React Query、i18next、ThemeProvider                              | `main.tsx`                       | 查询缓存、语言、主题、设置                       | 中高：会影响所有页面初始化   |
| `apps/desktop/src/routes/`                  | 路由层             | 把 project/import/recovery 等页面接到 Workbench                          | TanStack Router、workbench 组件                                  | Router                           | 页面级流程                                       | 中：路由改动容易影响导航     |
| `apps/desktop/src/components/workbench/`    | 工作台 UI          | 标签页、双栏、编辑器、侧栏、保存态、命令面板                             | React、Zustand、Query、DesktopBridge                             | 项目路由                         | Tabs、Split、Dirty、Recovery、Editor             | 高：跨页面状态耦合多         |
| `apps/desktop/src/stores/`                  | UI 会话状态        | 保存 workbench、workspace、settings 的本地状态                           | Zustand                                                          | UI 组件                          | 标签、分栏、选中态、布局、设置                   | 高：这里有持久化和双写       |
| `apps/desktop/src/platform/desktop-bridge/` | 前端桥             | 定义 DTO 和命令方法，封装 Tauri invoke，处理 fixture                     | `@tauri-apps/api/core`、`BridgeError`                            | 所有生产核心访问                 | IPC、DTO、错误归一化                             | 关键：这是前后端契约边界     |
| `apps/desktop/src-tauri/src/lib.rs`         | Tauri 命令层       | 注册 command、管理 active Kernel、处理项目注册表/设置/工作区文件         | `babel-application`、`tauri_plugin_dialog`、文件系统             | `TauriDesktopBridge`             | 项目会话、workspace-state、settings、worker 发现 | 关键：这里既是桥又碰文件系统 |
| `crates/babel-application/src/lib.rs`       | 应用层             | 编排导入、保存、验证、导出、OCR、资源队列、恢复                          | `babel-storage`、`babel-domain`、`babel-runtime`、adapter crates | Tauri 命令、测试、worker 流程    | `Kernel`、`SaveReceipt`、`FormatImportReport`    | 关键：业务行为大多在这里     |
| `crates/babel-storage/src/project.rs`       | 项目存储           | SQLite 读写、修订链、草稿、导航、工作区操作、导出记录                    | `rusqlite`、`babel-domain`                                       | `Kernel`                         | revision、draft、navigation、workspace recovery  | 关键：数据一致性直接依赖这里 |
| `crates/babel-storage/src/schema.rs`        | Schema/migration   | 管理 SQLite schema 版本和迁移                                            | `rusqlite`                                                       | `ProjectStore` 初始化            | migrations、user_version                         | 高：迁移错误会影响所有项目   |
| `crates/babel-domain/src/`                  | 稳定领域类型       | ID、状态、工作台/导航语义                                                | `serde`、`uuid`                                                  | storage/application/frontend DTO | `ProjectId`、`UnitId`、`TranslationStatus`       | 高：这是语义边界             |
| `crates/babel-runtime/src/`                 | worker/IPC 运行时  | frame、handshake、worker process、取消、超时                             | `prost`、`interprocess`、`uuid`                                  | application、worker tests        | `MAX_FRAME_BYTES`、`ProcessWorker`               | 高：协议变化要同步双方       |

## 读代码的顺序

1. `apps/desktop/src/main.tsx`
2. `apps/desktop/src/app/providers.tsx`
3. `apps/desktop/src/routes/projects.$projectId.tsx`
4. `apps/desktop/src/components/workbench/app-shell.tsx`
5. `apps/desktop/src/platform/desktop-bridge/tauri-bridge.ts`
6. `apps/desktop/src-tauri/src/lib.rs`
7. `crates/babel-application/src/lib.rs`
8. `crates/babel-storage/src/project.rs`

## 这份图里最重要的边界

- React 只负责展示和局部交互，不持有修订权威。
- `DesktopBridge` 是前端唯一的生产 IPC 入口。
- `Kernel` 是业务编排中心，不应该被 UI 逻辑穿透。
- SQLite 和工作区文件是持久化事实源，React Query 和 Zustand 只是缓存/会话层。

下一步建议阅读 `docs/06_IPC.md`，因为真正最容易出错的是前后端 DTO 和命令边界。
