# Babel Tower Phase 6：统一编辑器与连续资源工作流

## 当前完成：A 统一领域契约 + B 导航与桌面基线 + C IPC + D 项目库导入闭环

- `babel-domain` 新增格式无关的工作空间、翻译状态、导航筛选和导航位置值对象。
- `babel-storage` 新增活动 generation 的只读工作项查询、资源关系查询和图片区域 keyset 队列。
- `babel-application` 新增 `TranslationWorkItem`、`ResourceQueuePage`、资源关联投影和三种观察方式查询入口。
- 工作项由 SQLite 当前修订、TIR、Locator 和 ResourceGraph 关系按需组合，不保存第二份译文。
- 图片区域队列只选择 `ImageRegion`，同时返回 `RegionOf` 原图，普通插图不会被误当成 OCR 工作项。
- `project_navigation` 已迁移到 schema 9，由核心单写者保存，并按会话与单调序列拒绝陈旧写入。
- 关闭后可恢复工作空间、工作项、图片区域、滚动位置、缩放和筛选条件；恢复不改变翻译 `commit_sequence`。
- `apps/desktop` 已建立 Tauri 2 + React + TanStack Router + Radix/Tailwind/Lucide 的中文工作台壳、路由和组件基线。
- 首页、长文、单元、资源、校验、导出、恢复页面已连通；资源空态不伪造 OCR 结果。
- Tauri IPC schema v1 已固定为四个工作台命令：打开项目、读取快照、保存译文、保存导航；不暴露核心内部 Command/Response 总线。
- 长文和单元页面优先读取核心快照；编辑器保存状态由 Rust 持久化 ACK 驱动，浏览器预览在无 Tauri 环境时保留降级内容。
- 桌面层新增本地项目登记簿、项目列表、项目打开和 TXT/Markdown/EPUB 文件导入命令；登记簿位于应用数据目录，不进入项目权威 SQLite。
- 导入命令按扩展名选择核心 Kernel 的三种格式入口，所有解析、generation、绑定和激活仍由核心负责。

## 已验证

- 真实 TXT 导入后，同一 `unit_id` 在长文、单元和资源观察方式中返回相同来源、译文和修订，只改变视图标识。
- 保存译文后，工作项投影读取同一耐久修订与 `commit_sequence`。
- 两个图片区域按稳定阅读顺序分页，游标不会依赖 offset，并携带原图关联。
- `cargo test --workspace` 通过。
- `cargo clippy --workspace --all-targets -- -D warnings` 通过。
- `cargo fmt --all -- --check`、`bash tools/check-architecture.sh` 和 `git diff --check` 通过。
- 前端 `check` 通过：Vite 构建、TypeScript、ESLint、Prettier。
- 前端 Vitest 通过：保存状态测试。
- Playwright 工作台冒烟用例已配置；当前 Linux 未安装 Chromium，未完成浏览器执行。

## 尚未完成

- “已翻译、已审校、受阻”状态的权威存储与转换命令；当前投影只区分未翻译和草稿。
- 当前导入页仍使用路径输入，尚未接入系统文件选择器；项目库和导入命令已经存在，可先在 Tauri 环境验证真实路径闭环。
- OCR、背景清理、文字排版、派生图片与 EPUB/Markdown 联合导出。

下一切片是 Phase 6E：接入系统文件选择器、补真实 Tauri 交互验证，并开始图片 OCR、背景清理和嵌字派生图。
