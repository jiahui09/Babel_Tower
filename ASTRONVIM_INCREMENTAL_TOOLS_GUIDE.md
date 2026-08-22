# 现有 AstroNvim 配置的工具增量指南

本文只描述在当前 `~/.config/nvim` 基础上的扩展方式，不改变现有目录结构，不重写 `lazy_setup.lua`，不替换 Catppuccin、Kitty 透明背景、p10k 或 smear-cursor 配置。

当前配置中已经存在并应继续保留的文件：

- `lua/lazy_setup.lua`：Lazy 与 AstroNvim 启动入口
- `lua/community.lua`：AstroCommunity 入口，目前被提前 `return` 禁用
- `lua/plugins/transparent-background.lua`：Catppuccin Mocha 与透明背景
- `lua/plugins/smear-cursor.lua`：光标动画，目前插件被禁用
- `lua/plugins/astrolsp.lua`：LSP 总配置模板，目前被禁用
- `lua/plugins/astrocore.lua`：核心选项与快捷键模板，目前被禁用
- `lua/plugins/mason.lua`：Mason 工具安装模板，目前被禁用
- `lua/plugins/treesitter.lua`：Treesitter parser 模板，目前被禁用

## 一、建议添加的工具

### 1. Python：Pyright + Ruff

**作用**

- Pyright：类型检查、跳转、补全、参数提示。
- Ruff：代码规范检查、导入排序、快速格式化。

**添加原因**

Python 项目最容易出现“能运行但类型和结构不清晰”的问题。Pyright 提供语义检查，Ruff 提供统一规范，两者分工明确，比只安装一个 LSP 更适合长期工程学习。

**添加方法**

1. 在 `lua/community.lua` 中解除顶部的提前 `return`。
2. 在现有 AstroCommunity 导入列表中追加：

   ```lua
   { import = "astrocommunity.pack.python.base" },
   { import = "astrocommunity.pack.python.ruff" },
   ```

3. 在现有 `lua/plugins/mason.lua` 的 `ensure_installed` 中追加：

   ```lua
   "pyright",
   "ruff",
   ```

4. 在现有 `lua/plugins/astrolsp.lua` 的 `config` 中追加 `pyright` 和 `ruff` 配置。Ruff 只负责诊断和格式化，不要让它覆盖 Pyright 的 hover：

   ```lua
   pyright = {
     settings = {
       python = {
         analysis = {
           diagnosticMode = "workspace",
           typeCheckingMode = "basic",
         },
       },
     },
   },
   ruff = {
     on_attach = function(client)
       client.server_capabilities.hoverProvider = false
     end,
   },
   ```

不启用保存时自动格式化。将格式化绑定到已有快捷键体系中的一个手动命令，例如 `<Leader>lf`，这样可以看到修改发生的时机。

### 2. C/C++：clangd + clang-format

**作用**

- clangd：补全、跳转、引用查找、编译参数诊断。
- clang-format：统一 C/C++ 排版。

**添加原因**

clangd 只有在获得真实编译参数时才可靠。使用 CMake 的 `compile_commands.json` 可以减少“编辑器能补全、编译却失败”的偏差。

**添加方法**

1. 在 `lua/community.lua` 追加：

   ```lua
   { import = "astrocommunity.pack.cpp" },
   ```

2. 在 `lua/plugins/mason.lua` 追加：

   ```lua
   "clangd",
   "clang-format",
   ```

3. CMake 项目生成编译数据库：

   ```bash
   cmake -S . -B build -DCMAKE_EXPORT_COMPILE_COMMANDS=ON
   ```

4. 将 `build/compile_commands.json` 链接到项目根目录，或在 clangd 配置中指定 `--compile-commands-dir=build`。

不建议为 C/C++ 额外安装多个重复的 formatter；保留 clang-format 一个来源即可。

### 3. JavaScript / TypeScript：vtsls + ESLint

**作用**

- vtsls：类型分析、导入补全、跳转、重构。
- ESLint：项目规则、潜在错误和风格约束。

**添加原因**

TypeScript 的类型服务和 ESLint 不是同一个职责。vtsls 负责语言理解，ESLint 负责团队规则。分开配置更容易定位问题。

**添加方法**

1. 在 `lua/community.lua` 追加：

   ```lua
   { import = "astrocommunity.pack.typescript" },
   { import = "astrocommunity.pack.eslint" },
   ```

2. 在 `lua/plugins/mason.lua` 追加：

   ```lua
   "vtsls",
   "eslint-lsp",
   ```

3. ESLint 规则放在项目中，而不是写死在 Neovim：

   ```bash
   pnpm add -D eslint typescript typescript-eslint
   ```

4. 如果项目使用 flat config，保留项目根目录的 `eslint.config.js`；Neovim 只负责连接语言服务器。

不要同时启用 tsserver、vtsls 两套 TypeScript LSP，否则会出现重复诊断和重复补全。

### 4. HTML / CSS / XML / YAML / JSON

**作用**

- HTML/CSS：标签、属性、样式补全和诊断。
- XML：标签结构检查。
- YAML/JSON：语法检查、字段补全和 schema 校验。

**添加原因**

这些文件经常是工程配置的一部分。Schema 校验可以在运行程序前发现 CI、容器、编辑器配置中的拼写错误。

**添加方法**

在 `lua/community.lua` 追加：

```lua
{ import = "astrocommunity.pack.html-css" },
{ import = "astrocommunity.pack.xml" },
{ import = "astrocommunity.pack.yaml" },
{ import = "astrocommunity.pack.json" },
```

在 `lua/plugins/mason.lua` 追加：

```lua
"html-lsp",
"css-lsp",
"lemminx",
"yaml-language-server",
"json-lsp",
```

如需 SchemaStore，将 `b0o/schemastore.nvim` 作为 `astrolsp.lua` 中 `AstroNvim/astrolsp` 的依赖，再把 schemas 放入现有的 `config.jsonls` 和 `config.yamlls`。不要改变 `astrolsp.lua` 的外围结构。

### 5. Trouble.nvim

**作用**

将当前文件、工作区和 LSP 引用集中到一个可筛选面板中。

**添加原因**

单个浮动诊断窗口适合快速查看，Trouble 适合系统性清理工程问题，能帮助形成“先清诊断，再提交代码”的习惯。

**添加方法**

新增 `lua/plugins/trouble.lua`，只负责这个插件；或者在已有插件集合文件中追加同一段：

```lua
{
  "folke/trouble.nvim",
  opts = { focus = true },
}
```

映射建议使用不占用原有键位的 `<Leader>xx`，不要改动 AstroNvim 默认 LSP 键位。

### 6. Aerial.nvim

**作用**

显示当前文件的函数、类、方法或 Markdown 标题层级。

**添加原因**

阅读大型代码和技术文档时，大纲比反复搜索更容易建立结构理解。当前 Neovim 0.12 环境建议使用 LSP backend，减少 Treesitter API 变化带来的兼容问题。

**添加方法**

新增独立插件文件或追加到现有插件文件：

```lua
{
  "stevearc/aerial.nvim",
  cmd = { "AerialToggle", "AerialNavToggle" },
  opts = {
    backends = { "lsp", "markdown", "man" },
    layout = { min_width = 28 },
  },
}
```

建议只增加一个 `<Leader>pa` 映射用于打开，不修改窗口导航键。

### 7. Git：Gitsigns + Neogit

**作用**

- Gitsigns：显示行级修改、导航和 hunk 操作。
- Neogit：提供状态、暂存、提交和历史面板。

**添加原因**

这两个工具分别解决“局部修改检查”和“完整 Git 工作流”。它们能帮助把代码格式化、功能修改和修复拆成可审查提交。

**添加方法**

在独立插件文件中增加：

```lua
{ "lewis6991/gitsigns.nvim", opts = { current_line_blame = false } },
{ "NeogitOrg/neogit", dependencies = { "sindrets/diffview.nvim" } },
```

建议只添加 `<Leader>gg` 打开 Neogit，hunk 快捷键沿用 Gitsigns 默认键位或放在 `<Leader>gh` 前缀下。

### 8. Flash.nvim、Comment.nvim、nvim-autopairs

**作用**

- Flash：快速跳转到屏幕内任意位置。
- Comment.nvim：`gcc`、`gc` 快速注释。
- nvim-autopairs：自动补全括号和引号。

**添加原因**

它们减少重复移动和重复输入，但不会改变 Vim 的核心编辑模型，适合渐进式加入。

**添加方法**

AstroNvim 已有部分同类能力时不要重复安装。先用 `:Lazy` 检查是否已存在；只有缺失时再追加：

```lua
{ "folke/flash.nvim", opts = {} },
{ "numToStr/Comment.nvim", event = "User AstroFile", opts = {} },
{ "windwp/nvim-autopairs", event = "InsertEnter", opts = {} },
```

不要覆盖 AstroNvim 已有的 autopairs `config`，否则可能导致默认补全联动失效。

### 9. CodeCompanion + Ollama：1.5B / 7B 双模型

**作用**

通过本地 Ollama 模型完成两类任务：1.5B 模型负责低延迟的短代码补全和小范围改写，7B 模型负责代码片段分析、解释、算法指导和较长上下文推理。

**添加原因**

本地模型不需要把源代码发送到第三方服务；显式快捷键也不会干扰原生插入、普通模式和补全体验。

**添加方法**

1. 安装并准备两个模型：

   ```bash
   ollama pull qwen2.5:1.5b
   ollama pull qwen2.5:7b
   ollama list
   ```

2. 新增 `lua/plugins/codecompanion.lua`，不要修改已有主题或核心文件：

   ```lua
   {
     "olimorris/codecompanion.nvim",
     opts = {
       adapters = {
         http = {
           -- 低延迟模型：短补全、当前行改写、简单样板代码。
           ollama_fast = function()
             return require("codecompanion.adapters").extend("ollama", {
               schema = {
                 model = { default = "qwen2.5:1.5b" },
                 num_ctx = { default = 4096 },
                 temperature = { default = 0.1 },
               },
             })
           end,
           -- 分析模型：代码解释、算法指导、错误定位和较长片段总结。
           ollama_deep = function()
             return require("codecompanion.adapters").extend("ollama", {
               schema = {
                 model = { default = "qwen2.5:7b" },
                 num_ctx = { default = 8192 },
                 temperature = { default = 0.2 },
               },
             })
           end,
         },
       },
       interactions = {
         -- Inline 交互优先低延迟模型。
         inline = { adapter = "ollama_fast" },
         -- Chat/Cmd 用于解释和算法问题，使用 7B 模型。
         chat = { adapter = "ollama_deep" },
         cmd = { adapter = "ollama_deep" },
       },
     },
   }
   ```

3. 只增加 `<Leader>A` 前缀下的映射，例如 `<Leader>Ac` 对话、可视模式 `<Leader>Ai` 改写选区。建议把普通模式 `<Leader>Ai` 作为 1.5B 行内补全，把可视模式 `<Leader>Ac` 作为 7B 片段分析入口：

   ```lua
   -- 普通模式：CodeCompanion inline 使用 ollama_fast。
   ["<Leader>Ai"] = { "<Cmd>CodeCompanion<CR>", desc = "快速代码补全/改写" },
   -- 可视模式：CodeCompanionChat 使用 ollama_deep。
   ["<Leader>Ac"] = { "<Cmd>CodeCompanionChat Add<CR>", desc = "分析代码片段" },
   ```

   如果当前 CodeCompanion 版本不允许同一映射在不同交互中指定 adapter，可在调用前用 `:CodeCompanionChat` 的 adapter 选择器切换到 `ollama_deep`；不要修改 `<Tab>` 或补全确认键。

不要绑定 `i`、`<Tab>`、`<CR>` 等 Insert 模式按键，也不要启用自动补全式 AI ghost text。这里的“低延迟补全”指显式触发的短代码生成，不是持续后台预测；这样不会破坏现有 blink/cmp 补全体验。

### 双模型选择原则

| 任务 | 模型 | 原因 |
| --- | --- | --- |
| 当前行补全、函数签名、样板代码 | `qwen2.5:1.5b` | 上下文短、响应快、显存占用低 |
| 选区改写、代码解释、错误定位 | `qwen2.5:7b` | 需要更强的上下文理解 |
| 算法复杂度、方案比较、边界分析 | `qwen2.5:7b` | 需要多步推理，不应追求即时返回 |
| 大型文件全局重构 | 不建议直接交给 1.5B | 先手动缩小选区，再使用 7B 分阶段处理 |

3060 Laptop 6GB 显存上，1.5B 通常适合保持常驻；7B 可能部分卸载到系统内存。若 7B 响应过慢，保留 1.5B 做日常短补全，并在需要深度分析时手动启动 7B：

```bash
ollama run qwen2.5:1.5b
ollama run qwen2.5:7b
```

## 二、已有配置的三项优化

### 1. smear-cursor

只修改 `lua/plugins/smear-cursor.lua`：

- 将 `enabled = false` 改为 `enabled = true`。
- 将 `guicursor` 的 `i-ci:ver25` 改为与 Normal 一致的 `i-ci:block`。
- 保留现有动画参数，不要一次修改 stiffness、damping、time_interval 等全部参数。
- 如果块状光标尾迹太重，只降低 `trailing_stiffness_insert_mode`，不要把 Insert 动画关闭。

原因是细竖线只有一个字符列，终端绘制面积很小；改细后动画并非失效，而是拖影像素不足。块状光标能在相同动画参数下保持清晰。

### 2. Catppuccin 与透明背景

继续使用 `lua/plugins/transparent-background.lua`：

- 保留 `flavour = "mocha"`。
- 保留 `transparent_background = true`。
- 不要设置白色 `Normal` 背景。
- Kitty 的透明度和模糊负责“磨砂”，Neovim 只负责不绘制实色背景。

若图标丢失，先检查 Kitty 是否使用 Nerd Font，不要通过删除主题集成来修复。

### 3. AstroCommunity

只解除 `lua/community.lua` 顶部的提前返回，然后按需要逐项加入语言包。不要把所有 community 模块一次性导入；每加入一个模块后执行 `:Lazy sync` 和 `:checkhealth`。

## 三、推荐接入顺序

1. 先启用 AstroCommunity 和 Treesitter parser。
2. 再启用 Mason 工具安装。
3. 接入 Pyright/Ruff 或 clangd 等一组语言工具。
4. 验证 `:LspInfo`、`:Mason` 和实际诊断。
5. 添加 Trouble、Aerial、Gitsigns 等 UI 工具。
6. 最后添加 CodeCompanion，单独验证 Ollama。

每一步只解决一个问题。出现错误时，先回退最后一个增量文件，而不是恢复整个 Neovim 配置。

## 四、验证命令

```bash
nvim --headless "+Lazy! sync" +qa
nvim --headless "+checkhealth" +qa
```

Neovim 内重点检查：

```vim
:Lazy
:Mason
:LspInfo
:checkhealth
:set guicursor?
```

保存文件时不应自动改变格式；按你配置的手动格式化键后才执行 formatter。这样可以确认“工具已添加”与“编辑体验被接管”是两个独立问题。

## 五、外部资料

- [AstroCommunity](https://docs.astronvim.com/astrocommunity)
- [AstroNvim v6 迁移说明](https://docs.astronvim.com/configuration/v6_migration/)
- [CodeCompanion Ollama/HTTP adapter](https://codecompanion.olimorris.dev/configuration/adapters-http)
- [smear-cursor.nvim](https://github.com/sphamba/smear-cursor.nvim)
