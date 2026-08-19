# Babel Tower Phase 6：统一编辑器与连续资源工作流（进行中）

## 当前完成：A-D + E1-E5，E6 联合导出仍在进行

- `babel-domain` 新增格式无关的工作空间、翻译状态、导航筛选和导航位置值对象。
- `babel-storage` 新增活动 generation 的只读工作项查询、资源关系查询和图片区域 keyset 队列。
- `babel-application` 新增 `TranslationWorkItem`、`ResourceQueuePage`、资源关联投影和三种观察方式查询入口。
- 工作项由 SQLite 当前修订、TIR、Locator 和 ResourceGraph 关系按需组合，不保存第二份译文。
- 图片区域队列只选择 `ImageRegion`，同时返回 `RegionOf` 原图，普通插图不会被误当成 OCR 工作项。
- `project_navigation` 已迁移到 schema 9，由核心单写者保存，并按会话与单调序列拒绝陈旧写入。
- 关闭后可恢复工作空间、工作项、图片区域、滚动位置、缩放和筛选条件；恢复不改变翻译 `commit_sequence`。
- `apps/desktop` 已建立 Tauri 2 + React + TanStack Router + Radix/Tailwind/Lucide 的中文工作台壳、路由和组件基线。
- 首页、长文、单元、资源、校验、导出、恢复页面已连通；资源空态不伪造 OCR 结果。
- Tauri IPC schema v1 已固定桌面边界：项目登记/打开、导入、快照、资源队列、图片预览、OCR、嵌字预览、译文保存、图片区域修正和导航保存；不暴露核心内部 Command/Response 总线。
- 长文和单元页面优先读取核心快照；编辑器保存状态由 Rust 持久化 ACK 驱动，浏览器预览在无 Tauri 环境时保留降级内容。
- 桌面层新增本地项目登记簿、项目列表、项目打开和 TXT/Markdown/EPUB 文件导入命令；登记簿位于应用数据目录，不进入项目权威 SQLite。
- 导入命令按扩展名选择核心 Kernel 的三种格式入口，所有解析、generation、绑定和激活仍由核心负责。
- Tauri 接入原生文件/目录选择器，导入页不再要求用户手写路径；路径仍只作为导入命令输入，不改变核心权威边界。
- 桌面端接入 `resource_queue` IPC，按核心稳定游标加载图片文字区域，资源页支持图片区域连续前后切换，并从同一 `source_unit_key` 保存人工译文。
- 资源页明确区分“识别结果”和“人工译文”；当前已接入经过哈希校验的原图和嵌字派生预览，不用区域框或占位色块冒充图片结果。
- 图片区域修正使用独立 `image_region_revision` / `image_region_head`，OCR 候选缓存使用独立 `image_region_ocr_cache`；两者均不覆盖文本译文修订。
- `babel-image` 提供原图哈希校验、区域边界校验、平色背景填充和基于指定字体对象的确定性 PNG 渲染；字体字节不写入 SQLite。

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
- OCR 候选跨重启后的桌面端自动回显尚未接入；当前核心缓存读写已完成，资源页识别后可立即回显。
- 图片导出覆盖模型已接入核心，EPUB 与 Markdown 均支持按图片资源聚合多个区域并确定性合成；Markdown 文件路径导入会把相对图片纳入 CAS，文件导出会写出完整资源闭包。
- Windows 对应 OCR 运行库、双平台真实安装包和发布闭包随最终安装包阶段补齐；Linux 不做 Windows 实机验证。

下一阶段进入 Phase 7：权威翻译状态机、OCR 跨重启回显、真实校验/导出页面和桌面端工作流验收；安装包构建留到最终阶段。
