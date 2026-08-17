# Babel Tower 脚手架与工具链

## 结论

第一阶段采用“单仓库、双语言、一个桌面壳、一个权威核心”的工具链：

| 层 | 固定选择 | 责任边界 |
| --- | --- | --- |
| 桌面脚手架 | Tauri 2 官方 `create-tauri-app` | 生成桌面壳、权限、窗口、打包入口；不承载翻译业务规则。 |
| 前端 | React 19 + TypeScript + Vite | 页面、交互、瞬时界面状态和三种观察方式。 |
| 包管理 | Node.js 22.12+ + pnpm 11.22.0，版本写入 `packageManager` | 前端依赖、脚本、工作区和锁文件。 |
| 组件 | `shadcn/ui` + Radix UI + Tailwind CSS + `class-variance-authority` | 可访问的基础组件和统一视觉变量；组件源码进入仓库。 |
| 图标 | `lucide-react` | 唯一图标来源。 |
| 路由 | `@tanstack/react-router` 文件路由 + `createHashHistory` | 类型安全的路径参数和查询参数；桌面端不依赖服务器回退。 |
| 服务状态 | `@tanstack/react-query` | 从本地核心读取的查询、缓存、加载和失效；不保存权威译文。 |
| 界面状态 | `zustand` | 面板开合、光标、滚动、当前观察方式等非权威状态。 |
| 表单与校验 | `react-hook-form` + `zod` | 导入确认、导出选项、筛选和设置的表单校验。 |
| 长文编辑 | Tiptap / ProseMirror | 当前章节组合视图；每次改动转换为核心命令。 |
| 大列表 | TanStack Virtual | 单元列表、章节树和资源缩略图窗口化。 |
| 核心语言 | Rust stable，`rust-toolchain.toml` 锁定版本 | 领域模型、事务存储、自动保存、撤销、任务、格式和导出。 |
| Rust 工作区 | Cargo workspace | `babel-domain`、`babel-application`、`babel-storage`、格式适配器、核心服务和工作进程。 |
| 核心进程 | `babel-core` 独立可执行文件 | 唯一项目写入者；由桌面壳监管，但不把业务逻辑写入 Tauri 命令。 |
| 核心通信 | 版本化本地 IPC；Linux 使用 Unix socket，Windows 使用命名管道 | 类型化命令、提交确认、事件序列、断线重连；不开放任意本地网络端口。 |
| 测试 | Vitest + React Testing Library + Playwright | 单元/组件交互、桌面端流程、输入法、截图和安装包冒烟。 |
| 代码质量 | TypeScript 严格模式、ESLint、Prettier、`cargo fmt`、`cargo clippy` | 提交前阻断类型、格式和高风险静态问题。 |
| 依赖与供应链 | pnpm 锁文件、Cargo.lock、许可证清单、依赖审计 | 离线构建和单体安装包的可追溯性。 |

## 为什么这样组合

- 使用 Tauri 官方脚手架可以直接获得官方支持的 React/TypeScript/Vite 入口，再把 Rust 核心接入同一仓库；不手工拼接多个桌面初始化方案。
- 使用 TanStack Router 是因为当前产品有项目、翻译现场、观察方式、查询参数和恢复守卫等路由关系；类型化参数可以在编译期减少“跳到了不存在的单元”这类错误。
- 使用 React Query 和 Zustand 分开两类状态：核心返回的数据是可失效查询，面板和选中状态是界面临时状态；任何一个都不能代替 SQLite 权威存储。
- 使用 `shadcn/ui` 而不是黑盒组件包：基础组件源码进入仓库，能严格执行中文文案、焦点、密度、禁用态和设计变量。
- 使用 Vitest 做快速逻辑反馈，Playwright 做真实浏览器布局、键盘、输入法、路由和截图；不依赖 Storybook 才能验证交互，组件故事由 Playwright 组件测试或轻量展示页承载。

## 开工时的初始化顺序

下面顺序是一次性的仓库初始化，不是用户运行应用的命令。版本号以初始化当天的稳定版本锁入文件，之后不得使用 `latest` 漂移。

### 1. 生成桌面前端

在仓库根目录执行官方脚手架，选择：

- 前端语言：TypeScript。
- 包管理器：pnpm。
- 前端模板：React。
- 前端风格：TypeScript。
- 应用标识：`com.babeltower.desktop`。

官方入口为：

```bash
pnpm create tauri-app@latest
```

生成后，桌面前端固定放在 `apps/desktop/`；如果脚手架生成的是根目录应用，应在第一次初始化时整理到该目录，不在后续阶段再搬迁。

### 2. 固定 Node 与 pnpm

根目录 `package.json` 必须包含类似以下字段，具体补丁版本以初始化当天的锁定结果为准：

```json
{
  "packageManager": "pnpm@11.22.0",
  "engines": {
    "node": ">=22.12.0 <23"
  }
}
```

提交 `pnpm-lock.yaml`。不允许在开发文档中要求用户先全局安装任意前端包；开发环境使用 Corepack/pnpm，运行时安装包不依赖 Node。

### 3. 初始化组件和样式

在 `apps/desktop/` 中执行：

```bash
pnpm dlx shadcn@latest init
pnpm dlx shadcn@latest add button dialog dropdown-menu input label popover scroll-area separator tabs tooltip
```

初始化后立即把默认主题改为 [设计说明](/home/jiahui/project/Babel_Tower/DESIGN.md) 中的语义变量；不得先用默认示例颜色搭页面再大面积返工。所有业务组件放在 `src/components/workbench/`，基础组件放在 `src/components/ui/`。

### 4. 初始化路由和状态

前端依赖固定为：

```bash
pnpm add @tanstack/react-router @tanstack/react-query zustand react-hook-form zod lucide-react
pnpm add @tiptap/react @tiptap/starter-kit @tanstack/react-virtual
pnpm add -D @tanstack/router-plugin @tanstack/react-router-devtools vitest @testing-library/react @testing-library/user-event @playwright/test eslint prettier typescript-eslint
```

路由使用文件结构，并以翻译现场作为父路由：

```text
apps/desktop/src/routes/
  __root.tsx
  index.tsx                         # 项目库
  import.tsx                        # 导入作品
  projects.$projectId.tsx           # 翻译现场父布局
  projects.$projectId.content.tsx   # 长文观察方式
  projects.$projectId.units.tsx     # 单元观察方式
  projects.$projectId.resources.tsx # 资源观察方式
  projects.$projectId.status.tsx    # 按需项目状态
  projects.$projectId.validate.tsx  # 问题与校验
  projects.$projectId.exports.tsx   # 导出记录
  recovery.$projectId.tsx           # 恢复决定
```

`projects.$projectId.tsx` 只创建一次 `AppShell` 和翻译现场上下文；三个观察方式路由通过同一项目查询和同一选中对象服务，不分别创建项目状态。

### 5. 初始化 Rust 工作区

根目录建立 Cargo workspace，第一批只创建边界，不一次实现所有格式：

```text
Cargo.toml
crates/
  babel-domain/
  babel-application/
  babel-storage/
  babel-ipc/
  babel-format-api/
workers/
  core-service/
  format/
  ocr/
apps/desktop/src-tauri/
```

`babel-domain` 不依赖 Tauri、React、SQLite 或具体格式；`babel-application` 依赖领域和存储接口；`babel-storage` 实现 SQLite 与对象库；`core-service` 暴露版本化 IPC。Tauri 的 `src-tauri` 只负责窗口、能力白名单、启动/监管核心进程和转发通信。

Rust 初始化时提交 `rust-toolchain.toml`、`Cargo.lock`、`rustfmt.toml`，CI 固定执行 `cargo fmt --check`、`cargo clippy --workspace --all-targets --all-features -- -D warnings` 和测试。

## 开发脚本契约

根目录 `package.json` 的脚本名称固定如下：

| 脚本 | 用途 |
| --- | --- |
| `pnpm dev` | 启动前端与 Tauri 开发窗口，并监管本地核心服务。 |
| `pnpm dev:web` | 只启动 Vite，供组件和路由开发。 |
| `pnpm check` | TypeScript、ESLint、Prettier 检查和路由生成检查。 |
| `pnpm test` | Vitest 单元与组件交互测试。 |
| `pnpm test:e2e` | Playwright 桌面流程测试。 |
| `pnpm test:visual` | 固定窗口尺寸的截图基线测试。 |
| `pnpm rust:check` | Rust 格式、Clippy、编译和测试。 |
| `pnpm build` | 前端生产构建 + Tauri 构建检查，不等于正式发布。 |
| `pnpm bundle:windows` | Windows NSIS 单体包。 |
| `pnpm bundle:arch` | Arch Linux AppImage。 |

每个脚本都必须可在断网环境运行；需要下载的依赖只在开发机和构建机准备阶段获取，不能在应用启动时下载。

## 首个纵向切片

脚手架完成后不先做项目库视觉抛光，而是打通一条最小真实链路：

1. 项目库展示一个本地演示项目。
2. 进入项目直接进入长文翻译现场。
3. 从核心服务读取一个章节和内容单元。
4. 在 Tiptap 中输入中文译文。
5. 通过版本化 IPC 提交 `UpdateTranslation`，收到权威保存确认。
6. 切换到单元观察方式，定位同一个 `unit_id`。
7. 关闭并重新打开项目，恢复已保存译文。
8. 用测试数据生成一个导出结果，并显示校验状态。

首个切片暂不接入真实 OCR、EPUB 复杂回写或高级图片排版；这些能力必须建立在翻译对象、保存和路由上下文已经真实可用之后。

## 第一阶段禁止事项

- 不同时引入 React Router、TanStack Router 或自定义路由器；只保留 TanStack Router。
- 不同时引入 Redux、Zustand 以外的全局状态库，且 Zustand 不能保存权威译文。
- 不使用 Next.js、Electron、React Native 或服务端渲染框架。
- 不把 SQLite 读写、源文件路径或导出逻辑放进 React 组件。
- 不把 Tauri command 当作领域服务；复杂业务必须进入 Rust 核心。
- 不在组件中直接使用十六进制颜色、临时 CSS 阴影或未经设计审查的基础控件。
- 不在第一个切片中加入登录、联网、AI、远程字体、在线图标或埋点。

## 工具链验收门槛

脚手架阶段完成的标准不是“窗口能打开”，而是：

- [ ] `pnpm install --frozen-lockfile` 在干净环境成功。
- [ ] `pnpm check`、`pnpm test`、`pnpm rust:check` 首次通过。
- [ ] Tauri 开发窗口能启动核心服务，核心服务断开后界面显示中文错误和重试入口。
- [ ] 路由覆盖项目库、导入、翻译现场三种观察方式、问题、导出和恢复，不存在重定向循环。
- [ ] `Ctrl/Cmd+S`、中文输入法组合输入、撤销/重做和观察方式切换有自动化测试。
- [ ] 1280x800 和 1440x900 下默认长文现场不出现常驻辅助面板、重叠、截断或布局跳动。
- [ ] 组件基础层、领域组件、路由和核心通信目录已经分离；没有把示例代码当作业务代码继续扩展。
- [ ] Windows 与 Arch 的本地开发前置条件和离线构建缓存策略已记录。

## 官方依据

- Tauri 创建项目：https://v2.tauri.app/zh-cn/start/create-project/
- Tauri 前置环境：https://v2.tauri.app/start/prerequisites/
- Tauri 构建与发布：https://v2.tauri.app/distribute/
- Vite 脚手架：https://vite.dev/guide/
- shadcn/ui 的 Vite 安装：https://ui.shadcn.com/docs/installation/vite
- shadcn/ui 的单仓库安装：https://ui.shadcn.com/docs/monorepo
- TanStack Router 文件路由：https://tanstack.com/router/latest/docs/routing/file-based-routing
- TanStack Router 类型安全：https://tanstack.com/router/latest/docs/guide/type-safety
- Playwright 视觉比较：https://playwright.dev/docs/test-snapshots
