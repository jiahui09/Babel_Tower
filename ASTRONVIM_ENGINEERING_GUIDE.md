# AstroNvim v6 软件工程配置指南

本文给出一套模块化 AstroNvim v6 配置，面向 Python、C/C++、Web 工程以及 README/技术文档维护。设计目标是：诊断清楚、格式化可控、导航高效、AI 不抢按键，并保留 Catppuccin Mocha 深色主题、终端透明背景和完整的块状光标动画。本文不把 Neovim 用作论文写作环境。

> 适用基线：Neovim 0.12、AstroNvim v6、Kitty、Nerd Font。AstroNvim v4/v5 用户不要直接混用本文的 LSP API；v6 已迁移到 Neovim 原生 `vim.lsp.config`。

## 1. 工具总览

| 工具 | 作用 | 使用习惯 |
| --- | --- | --- |
| Pyright | Python 类型检查、跳转、补全 | 在 `pyproject.toml` 中逐步提高类型严格度 |
| Ruff | Python lint、导入排序、格式化 | 提交前执行 `<Leader>lf` 与项目测试 |
| clangd | C/C++ 补全、引用、诊断 | 配合 CMake 导出的 `compile_commands.json` |
| vtsls | JavaScript/TypeScript 语言服务 | 使用项目本地 TypeScript 版本 |
| ESLint | JS/TS 规则与工程规范 | 规则写进项目的 `eslint.config.js` |
| SchemaStore | JSON/YAML 常见配置文件 schema | 提供字段补全和即时校验 |
| Neo-tree | 文件树、缓冲区和 Git 状态浏览 | `<Leader>e` 切换，避免长期占屏 |
| project.nvim | 根据 Git/LSP 根目录识别项目 | Telescope 项目列表快速切换 |
| Trouble | 汇总诊断、符号、LSP 引用 | `<Leader>xx` 查看当前工程问题 |
| Aerial | 函数、类、章节大纲 | 强制 LSP 后端，规避 TS 兼容差异 |
| Gitsigns | 行级新增、修改、删除提示 | hunk 级检查与暂存 |
| Neogit | Git 状态、提交和历史面板 | `<Leader>gg` 打开 |
| Flash | 标签式快速跳转 | `s` 搜索跳转，保留 Vim 原生操作逻辑 |
| Comment.nvim | 行/块注释 | `gcc`、`gc` |
| nvim-autopairs | 自动补全括号和引号 | Insert 模式自动工作 |
| render-markdown | 在 Neovim 内渲染 README/技术文档 | 阅读时启用，不替代源文本 |
| markdown-preview | 浏览器预览项目 Markdown 文档 | `<Leader>mp` 开启 |
| CodeCompanion | 对话、选区润色、解释代码 | Ollama 本地运行；只显式调用 |
| smear-cursor | 光标移动动画 | Insert/Normal 都使用块状光标 |

## 2. 当前配置基线与增量原则

本文对应的实际配置根目录是 `~/.config/nvim`，不是一个需要重新安装的空模板。当前基线如下：

- `lua/lazy_setup.lua` 已使用 AstroNvim `^6`，保留原文件，只在需要时追加 Lazy 选项。
- `lua/community.lua` 当前有 `if true then return {} end`，因此 AstroCommunity 导入暂未启用。
- `lua/plugins/transparent-background.lua` 已负责 Catppuccin Mocha、透明背景和 AstroUI 配色，应继续在此文件上合并主题调整。
- `lua/plugins/smear-cursor.lua` 已有光标动画参数，但当前 `enabled = false`，并且 Insert 模式仍是 `ver25`；优化应只改这一份文件的相关字段。
- `lua/plugins/astrolsp.lua`、`astrocore.lua`、`mason.lua`、`treesitter.lua`、`none-ls.lua`、`astroui.lua`、`user.lua` 是 AstroNvim 模板文件，目前通过提前 `return {}` 禁用。
- 目录中已经存在的配置和用户快捷键优先级高于本文建议；本文新增模块不能覆盖它们。

执行顺序应是：先备份或提交当前配置 -> 只启用一个模块 -> 启动 Neovim 验证 -> 再启用下一个模块。不要把下方代码一次性覆盖到整个 `~/.config/nvim`。

### 最小修改规则

1. 先删除目标文件最顶部的 `if true then return {} end`，再保留模板结构并修改 `opts`。
2. 已存在的 Catppuccin、透明背景、smear-cursor 文件只做字段级修改，不更换插件。
3. AstroCommunity 只在 `community.lua` 中增加 import；不要重写 `lazy_setup.lua` 的引导逻辑。
4. 新功能优先新增一个独立文件，例如 `lua/plugins/diagnostics.lua`，避免修改互不相关的模块。
5. 任何快捷键新增前先执行 `:which-key` 检查前缀，避免覆盖 AstroNvim 原生映射。

### 建议的增量文件

```text
~/.config/nvim/
├── init.lua
├── lazy-lock.json                 # Lazy 自动生成，不手写
└── lua/
    ├── lazy_setup.lua
    ├── community.lua
    ├── polish.lua
    └── plugins/
        ├── astrocore.lua              # 原文件：只解除禁用并增量修改
        ├── astrolsp.lua               # 原文件：只解除禁用并增量修改
        ├── mason.lua                  # 原文件：追加工具，不替换列表
        ├── treesitter.lua             # 原文件：追加 parser
        ├── none-ls.lua                # 原文件：按需追加 source
        ├── astroui.lua                # 原文件：保留图标与主题入口
        ├── transparent-background.lua # 原文件：保留透明 Catppuccin
        ├── smear-cursor.lua            # 原文件：优化动画参数
        ├── docs.lua                    # 新增：README/技术文档支持
        ├── ai.lua                      # 新增：CodeCompanion + Ollama
        └── project-tools.lua           # 新增：导航、诊断、Git 等扩展
        
```

AstroCommunity 必须在 AstroNvim 核心之后、用户 `plugins/` 之前导入。这样用户配置最后合并，可以覆盖 community 默认值。

## 3. 外部依赖

Arch Linux 示例：

```bash
sudo pacman -S --needed neovim git ripgrep fd nodejs npm pnpm \
  python python-pip clang cmake ninja

# 本地 AI；若已安装可跳过。
sudo pacman -S --needed ollama
ollama pull qwen2.5:7b
```

说明：

- `ripgrep` 为全文搜索后端，`fd` 用于快速文件与虚拟环境查找。
- `node/npm/pnpm` 是 vtsls、ESLint、Markdown Preview 等工具的运行环境。
- `clangd` 和 `clang-format` 通常随 Arch 的 `clang` 包安装。
- LSP、formatter、linter 优先交给 Mason 安装；项目依赖仍应写入项目自己的 lockfile。
- Kitty 必须使用 Nerd Font，否则 Neo-tree、状态栏和 p10k 图标会显示为空白方块。

## 4. 增量配置代码

下面的代码按“修改已有文件”与“新增独立文件”分组。代码块是完整的目标文件内容，适用于把当前对应模板文件解除禁用后进行对照合并；不要将它们机械复制覆盖所有原文件。

### `init.lua`

```lua
-- 禁用 netrw：文件浏览由 Neo-tree 接管。
vim.g.loaded_netrw = 1
vim.g.loaded_netrwPlugin = 1

-- AstroNvim 的 Lazy 引导入口。
require "lazy_setup"
```

`init.lua` 和 `lazy_setup.lua` 属于现有启动链，不是本次扩展的重写对象。下面的 `lazy_setup.lua` 代码仅用于说明当前结构；实际修改时保留你文件中的已有选项，只在对应位置增量合并。

### `lua/lazy_setup.lua`

```lua
local lazypath = vim.fn.stdpath "data" .. "/lazy/lazy.nvim"

if not vim.uv.fs_stat(lazypath) then
  local result = vim.fn.system {
    "git",
    "clone",
    "--filter=blob:none",
    "--branch=stable",
    "https://github.com/folke/lazy.nvim.git",
    lazypath,
  }
  if vim.v.shell_error ~= 0 then error("无法安装 lazy.nvim:\n" .. result) end
end

vim.opt.rtp:prepend(lazypath)

require("lazy").setup({
  {
    "AstroNvim/AstroNvim",
    version = "^6",
    import = "astronvim.plugins",
    opts = {
      mapleader = " ",
      maplocalleader = ",",
      icons_enabled = true,
      pin_plugins = nil,
      update_notifications = true,
    },
  },
  { import = "community" }, -- 必须位于 core 与 plugins 之间。
  { import = "plugins" },
}, {
  install = { colorscheme = { "catppuccin", "astrodark" } },
  ui = { backdrop = 100 },
  performance = {
    rtp = {
      disabled_plugins = { "gzip", "netrwPlugin", "tarPlugin", "tohtml", "zipPlugin" },
    },
  },
})
```

### `lua/community.lua`

```lua
return {
  "AstroNvim/astrocommunity",

  -- 语言包：负责 Treesitter、基础 LSP 和相关工具衔接。
  { import = "astrocommunity.pack.python.base" },
  { import = "astrocommunity.pack.python.ruff" },
  { import = "astrocommunity.pack.cpp" },
  { import = "astrocommunity.pack.typescript" },
  { import = "astrocommunity.pack.eslint" },
  { import = "astrocommunity.pack.html-css" },
  { import = "astrocommunity.pack.xml" },
  { import = "astrocommunity.pack.yaml" },
  { import = "astrocommunity.pack.json" },
  { import = "astrocommunity.pack.markdown" },

  -- AstroNvim 集成模块。
  { import = "astrocommunity.editing-support.conform-nvim" },
  { import = "astrocommunity.diagnostics.trouble-nvim" },
  { import = "astrocommunity.project.project-nvim" },
  { import = "astrocommunity.git.neogit" },
  { import = "astrocommunity.motion.flash-nvim" },
  { import = "astrocommunity.markdown-and-latex.render-markdown-nvim" },
  { import = "astrocommunity.markdown-and-latex.markdown-preview-nvim" },
  { import = "astrocommunity.ai.codecompanion-nvim" },
}
```

这里没有导入完整的 `astrocommunity.pack.python`，因为当前完整 pack 会额外选择 BasedPyright、Black 和 isort；本方案明确采用 Pyright + Ruff，故使用粒度模块。

### `lua/plugins/core.lua`

```lua
return {
  {
    "AstroNvim/astrocore",
    opts = {
      features = {
        large_buf = { size = 1024 * 500, lines = 10000 },
        autopairs = true,
        cmp = true,
        diagnostics_mode = 3,
        highlighturl = true,
        notifications = true,
      },
      options = {
        opt = {
          number = true,
          relativenumber = true,
          signcolumn = "yes",
          cursorline = true,
          wrap = false,
          linebreak = true,
          breakindent = true,
          scrolloff = 6,
          sidescrolloff = 8,
          splitbelow = true,
          splitright = true,
          undofile = true,
          ignorecase = true,
          smartcase = true,
          termguicolors = true,
          timeoutlen = 400,
          updatetime = 250,
          -- 使用系统剪贴板；远程服务器上可删除此项。
          clipboard = "unnamedplus",
        },
      },
      mappings = {
        n = {
          ["<C-h>"] = { "<C-w>h", desc = "转到左侧窗口" },
          ["<C-j>"] = { "<C-w>j", desc = "转到下方窗口" },
          ["<C-k>"] = { "<C-w>k", desc = "转到上方窗口" },
          ["<C-l>"] = { "<C-w>l", desc = "转到右侧窗口" },
          ["<Leader>w"] = { "<Cmd>write<CR>", desc = "保存文件" },
        },
        v = {
          ["<"] = { "<gv", desc = "左缩进并保持选区" },
          [">"] = { ">gv", desc = "右缩进并保持选区" },
        },
      },
    },
  },
}
```

### `lua/plugins/lsp.lua`

```lua
return {
  {
    "AstroNvim/astrolsp",
    dependencies = { "b0o/schemastore.nvim" },
    opts = function(_, opts)
      -- 禁止保存时自动格式化。格式化必须由 <Leader>lf 明确触发。
      opts.formatting = opts.formatting or {}
      opts.formatting.format_on_save = { enabled = false }

      opts.config = opts.config or {}

      opts.config.pyright = {
        settings = {
          python = {
            analysis = {
              autoSearchPaths = true,
              diagnosticMode = "workspace",
              typeCheckingMode = "basic",
              useLibraryCodeForTypes = true,
            },
          },
        },
      }

      -- Ruff 只负责 lint/格式化；悬停文档交给 Pyright，避免重复窗口。
      opts.config.ruff = {
        on_attach = function(client) client.server_capabilities.hoverProvider = false end,
      }

      opts.config.clangd = {
        cmd = {
          "clangd",
          "--background-index",
          "--clang-tidy",
          "--completion-style=detailed",
          "--header-insertion=iwyu",
        },
      }

      opts.config.vtsls = {
        settings = {
          typescript = { updateImportsOnFileMove = { enabled = "always" } },
          javascript = { updateImportsOnFileMove = { enabled = "always" } },
        },
      }

      -- 直接传入 schema，避开旧 on_new_config 写法在 Neovim 0.12 的差异。
      opts.config.jsonls = {
        settings = { json = { validate = { enable = true }, schemas = require("schemastore").json.schemas() } },
      }
      opts.config.yamlls = {
        settings = {
          yaml = {
            validate = true,
            schemaStore = { enable = false, url = "" },
            schemas = require("schemastore").yaml.schemas(),
          },
        },
      }

      return opts
    end,
  },
}
```

### `lua/plugins/tools.lua`

```lua
return {
  {
    "WhoIsSethDaniel/mason-tool-installer.nvim",
    opts = function(_, opts)
      opts.ensure_installed = require("astrocore").list_insert_unique(opts.ensure_installed or {}, {
        "pyright", "ruff",
        "clangd", "clang-format",
        "vtsls", "eslint-lsp", "eslint_d", "prettier",
        "html-lsp", "css-lsp", "lemminx", "xmlformatter",
        "yaml-language-server", "yamlfmt", "json-lsp",
        "marksman", "markdownlint-cli2",
        "stylua",
      })
    end,
  },
  {
    "stevearc/conform.nvim",
    opts = {
      notify_on_error = true,
      format_on_save = nil,
      formatters_by_ft = {
        python = { "ruff_fix", "ruff_organize_imports", "ruff_format" },
        c = { "clang_format" },
        cpp = { "clang_format" },
        javascript = { "prettier" },
        javascriptreact = { "prettier" },
        typescript = { "prettier" },
        typescriptreact = { "prettier" },
        html = { "prettier" },
        css = { "prettier" },
        scss = { "prettier" },
        json = { "prettier" },
        jsonc = { "prettier" },
        yaml = { "yamlfmt" },
        markdown = { "prettier" },
        xml = { "xmlformatter" },
        lua = { "stylua" },
      },
    },
  },
  {
    "mfussenegger/nvim-lint",
    event = { "BufReadPost", "BufWritePost", "InsertLeave" },
    config = function()
      local lint = require "lint"
      lint.linters_by_ft = {
        python = { "ruff" },
        javascript = { "eslint_d" },
        javascriptreact = { "eslint_d" },
        typescript = { "eslint_d" },
        typescriptreact = { "eslint_d" },
        markdown = { "markdownlint-cli2" },
      }
      vim.api.nvim_create_autocmd({ "BufWritePost", "InsertLeave" }, {
        group = vim.api.nvim_create_augroup("user_lint", { clear = true }),
        callback = function() lint.try_lint() end,
      })
    end,
  },
  {
    "AstroNvim/astrocore",
    opts = {
      mappings = {
        n = {
          ["<Leader>lf"] = {
            function() require("conform").format { async = true, lsp_format = "fallback" } end,
            desc = "格式化当前文件（手动确认）",
          },
          ["<Leader>ll"] = {
            function() require("lint").try_lint() end,
            desc = "立即运行 linter",
          },
        },
        v = {
          ["<Leader>lf"] = {
            function() require("conform").format { async = true, lsp_format = "fallback" } end,
            desc = "格式化选区",
          },
        },
      },
    },
  },
}
```

注意：Mason 中若 `xmlformatter` 不可用，执行 `npm install -g xml-formatter`。格式化器只改变排版，linter/LSP 负责指出问题，两者职责不要混淆。

### `lua/plugins/editing.lua`

```lua
return {
  {
    "folke/flash.nvim",
    opts = {
      modes = { search = { enabled = true }, char = { enabled = true } },
      label = { uppercase = false },
    },
  },
  {
    "numToStr/Comment.nvim",
    event = "User AstroFile",
    opts = {},
  },
  {
    "windwp/nvim-autopairs",
    event = "InsertEnter",
    opts = { check_ts = true, fast_wrap = {} },
  },
}
```

### `lua/plugins/navigation.lua`

```lua
return {
  {
    "nvim-neo-tree/neo-tree.nvim",
    opts = {
      filesystem = {
        follow_current_file = { enabled = true },
        use_libuv_file_watcher = true,
        filtered_items = {
          hide_dotfiles = false,
          hide_gitignored = false,
          never_show = { ".DS_Store" },
        },
      },
      window = { width = 32 },
    },
  },
  {
    "jay-babu/project.nvim",
    main = "project_nvim",
    opts = {
      detection_methods = { "lsp", "pattern" },
      patterns = { ".git", "pyproject.toml", "package.json", "CMakeLists.txt", "Cargo.toml", "Makefile" },
      silent_chdir = true,
    },
  },
  {
    "stevearc/aerial.nvim",
    cmd = { "AerialToggle", "AerialNavToggle" },
    opts = {
      backends = { "lsp", "markdown", "man" },
      layout = { min_width = 28, default_direction = "prefer_right" },
      show_guides = true,
      filter_kind = false,
    },
    dependencies = { "nvim-tree/nvim-web-devicons" },
  },
  {
    "AstroNvim/astrocore",
    opts = {
      mappings = {
        n = {
          ["<Leader>e"] = { "<Cmd>Neotree toggle reveal<CR>", desc = "文件树" },
          ["<Leader>pa"] = { "<Cmd>AerialToggle!<CR>", desc = "代码/文章大纲" },
          ["<Leader>fp"] = { "<Cmd>Telescope projects<CR>", desc = "项目列表" },
        },
      },
    },
  },
}
```

Aerial 使用 LSP 作为代码结构后端，而不是强依赖 Treesitter；Markdown 则使用其专用后端。这在 Neovim 0.12 / 新 Treesitter API 下更稳妥。

### `lua/plugins/diagnostics.lua`

```lua
return {
  {
    "folke/trouble.nvim",
    opts = { focus = true, auto_close = false },
  },
  {
    "AstroNvim/astrocore",
    opts = {
      mappings = {
        n = {
          ["<Leader>xx"] = { "<Cmd>Trouble diagnostics toggle<CR>", desc = "工作区诊断" },
          ["<Leader>xX"] = { "<Cmd>Trouble diagnostics toggle filter.buf=0<CR>", desc = "当前文件诊断" },
          ["<Leader>xs"] = { "<Cmd>Trouble symbols toggle focus=false<CR>", desc = "文档符号" },
          ["<Leader>xl"] = { "<Cmd>Trouble lsp toggle focus=false win.position=right<CR>", desc = "LSP 引用/定义" },
          ["]d"] = { function() vim.diagnostic.jump { count = 1, float = true } end, desc = "下一诊断" },
          ["[d"] = { function() vim.diagnostic.jump { count = -1, float = true } end, desc = "上一诊断" },
        },
      },
    },
  },
}
```

### `lua/plugins/git.lua`

```lua
return {
  {
    "lewis6991/gitsigns.nvim",
    opts = {
      current_line_blame = false,
      signs = {
        add = { text = "▎" }, change = { text = "▎" }, delete = { text = "" },
        topdelete = { text = "" }, changedelete = { text = "▎" }, untracked = { text = "▎" },
      },
      on_attach = function(bufnr)
        local gs = package.loaded.gitsigns
        local function map(lhs, rhs, desc)
          vim.keymap.set("n", lhs, rhs, { buffer = bufnr, desc = desc })
        end
        map("]h", gs.next_hunk, "下一处 Git 修改")
        map("[h", gs.prev_hunk, "上一处 Git 修改")
        map("<Leader>ghp", gs.preview_hunk, "预览修改")
        map("<Leader>ghs", gs.stage_hunk, "暂存修改块")
        map("<Leader>ghr", gs.reset_hunk, "还原修改块")
        map("<Leader>ghb", gs.blame_line, "查看本行提交")
      end,
    },
  },
  {
    "NeogitOrg/neogit",
    opts = { integrations = { diffview = true, telescope = true } },
    dependencies = { "sindrets/diffview.nvim" },
  },
  {
    "AstroNvim/astrocore",
    opts = { mappings = { n = { ["<Leader>gg"] = { "<Cmd>Neogit<CR>", desc = "Neogit" } } } },
  },
}
```

推荐工作流：先用 Gitsigns 逐 hunk 检查，再运行测试，最后通过 Neogit 暂存和提交。不要把格式化、重构与功能修改混在同一提交中。

### `lua/plugins/docs.lua`

```lua
return {
  {
    "MeanderingProgrammer/render-markdown.nvim",
    opts = {
      file_types = { "markdown", "codecompanion" },
      heading = { enabled = true, sign = true },
      code = { enabled = true, sign = false, width = "block", border = "thin" },
      checkbox = { enabled = true },
    },
  },
  {
    "iamcco/markdown-preview.nvim",
    ft = { "markdown" },
    build = "cd app && npm install",
    init = function()
      vim.g.mkdp_auto_close = 1
      vim.g.mkdp_refresh_slow = 0
      vim.g.mkdp_theme = "dark"
    end,
  },
  {
    "AstroNvim/astrocore",
    opts = {
      mappings = {
        n = {
          ["<Leader>mr"] = { "<Cmd>RenderMarkdown toggle<CR>", desc = "切换 Markdown 渲染" },
          ["<Leader>mp"] = { "<Cmd>MarkdownPreviewToggle<CR>", desc = "浏览器预览 Markdown" },
        },
      },
    },
  },
}
```

Markdown 在这里仅服务于仓库的 `README.md`、架构说明、开发日志和变更记录。正式论文写作、排版与文献管理不纳入 Neovim 配置范围。

### `lua/plugins/ai.lua`

```lua
return {
  {
    "olimorris/codecompanion.nvim",
    opts = {
      adapters = {
        http = {
          ollama = function()
            return require("codecompanion.adapters").extend("ollama", {
              schema = {
                model = { default = "qwen2.5:7b" },
                num_ctx = { default = 8192 },
                temperature = { default = 0.2 },
              },
            })
          end,
          opts = { show_model_choices = false },
        },
      },
      interactions = {
        chat = { adapter = "ollama" },
        inline = { adapter = "ollama" },
        cmd = { adapter = "ollama" },
        background = { adapter = "ollama" },
      },
      display = {
        chat = { window = { layout = "vertical", width = 0.38 } },
      },
    },
  },
  {
    "AstroNvim/astrocore",
    opts = {
      mappings = {
        n = {
          ["<Leader>A"] = { desc = "AI" },
          ["<Leader>Ac"] = { "<Cmd>CodeCompanionChat Toggle<CR>", desc = "切换 AI 对话" },
          ["<Leader>Aa"] = { "<Cmd>CodeCompanionActions<CR>", desc = "AI 动作面板" },
          ["<Leader>Ai"] = { "<Cmd>CodeCompanion<CR>", desc = "行内 AI 指令" },
        },
        v = {
          ["<Leader>A"] = { desc = "AI" },
          ["<Leader>Ac"] = { "<Cmd>CodeCompanionChat Add<CR>", desc = "把选区加入对话" },
          ["<Leader>Aa"] = { "<Cmd>CodeCompanionActions<CR>", desc = "对选区执行 AI 动作" },
          ["<Leader>Ai"] = { "<Cmd>CodeCompanion<CR>", desc = "行内改写选区" },
        },
      },
    },
  },
}
```

启动 AI 前运行：

```bash
sudo systemctl enable --now ollama
ollama list
```

若发行版没有 system service，可在单独终端运行 `ollama serve`。6 GB 显存运行 7B 模型可能部分卸载到内存；若响应明显卡顿，将模型改为 `qwen2.5:3b`。AI 生成的代码必须经过人工审查、静态检查和测试。

### `lua/plugins/theme.lua`

```lua
return {
  {
    "catppuccin/nvim",
    name = "catppuccin",
    priority = 1000,
    opts = {
      flavour = "mocha",
      transparent_background = true,
      term_colors = true,
      integrations = {
        aerial = true,
        cmp = true,
        gitsigns = true,
        mason = true,
        native_lsp = { enabled = true },
        neogit = true,
        neotree = true,
        treesitter = true,
        trouble = true,
        which_key = true,
      },
    },
  },
  {
    "AstroNvim/astroui",
    opts = { colorscheme = "catppuccin-mocha" },
  },
}
```

`transparent_background = true` 的含义是 Neovim 不绘制实色底；真正的半透明与磨砂由 Kitty/桌面合成器负责。不要在 Neovim 中设置白色背景，否则会破坏 Catppuccin 深色主题。

### `lua/plugins/smear-cursor.lua`

```lua
return {
  {
    "sphamba/smear-cursor.nvim",
    event = "VeryLazy",
    init = function()
      -- 所有主要模式均使用块状光标。Insert 与 Normal 形态一致，动画也更可见。
      vim.opt.guicursor = table.concat({
        "n-v-c:block-Cursor/lCursor",
        "i-ci:block-Cursor/lCursor",
        "r-cr:block-Cursor/lCursor",
        "o:block-Cursor/lCursor",
        "sm:block-Cursor/lCursor",
        "a:blinkwait700-blinkoff400-blinkon250",
      }, ",")
    end,
    opts = {
      -- 平衡“可见的拖影”和“输入不黏滞”。
      stiffness = 0.68,
      trailing_stiffness = 0.42,
      stiffness_insert_mode = 0.68,
      trailing_stiffness_insert_mode = 0.42,
      damping = 0.78,
      damping_insert_mode = 0.78,
      distance_stop_animating = 0.1,
      hide_target_hack = false,
      cursor_color = "none",
      smear_between_buffers = true,
      smear_between_neighbor_lines = true,
      smear_insert_mode = true,
      scroll_buffer_space = true,
      legacy_computing_symbols_support = false,
    },
  },
}
```

细竖线光标动画不明显是几何原因：轨迹的可绘制面积远小于块状光标，而且终端按字符单元渲染。若一定要使用细光标，可把 Insert 项改为 `i-ci:ver25`，同时降低 `stiffness_insert_mode`、`trailing_stiffness_insert_mode` 来延长拖影，但效果仍不会等同块状光标。本方案按你的要求统一为块状。

### `lua/polish.lua`

```lua
-- 仅对项目文档和提交消息启用换行显示；代码文件仍保持 nowrap。
vim.api.nvim_create_autocmd("FileType", {
  pattern = { "markdown", "text", "gitcommit" },
  group = vim.api.nvim_create_augroup("user_prose", { clear = true }),
  callback = function()
    vim.opt_local.wrap = true
    vim.opt_local.linebreak = true
    vim.opt_local.breakindent = true
    vim.opt_local.spell = true
    vim.opt_local.spelllang = { "en_us", "cjk" }
  end,
})

-- 高亮复制内容，提供明确但短暂的操作反馈。
vim.api.nvim_create_autocmd("TextYankPost", {
  group = vim.api.nvim_create_augroup("user_yank_highlight", { clear = true }),
  callback = function() vim.highlight.on_yank { higroup = "IncSearch", timeout = 150 } end,
})
```

## 5. 首次启动与验证

建议先隔离验证，避免破坏当前编辑器：

```bash
# 将以上结构放在 ~/.config/astronvim_v6 后：
NVIM_APPNAME=astronvim_v6 nvim

# 无界面安装与启动检查：
NVIM_APPNAME=astronvim_v6 nvim --headless "+Lazy! sync" +qa
NVIM_APPNAME=astronvim_v6 nvim --headless "+checkhealth" +qa
```

进入 Neovim 后依次检查：

1. `:Lazy`：所有插件应为已安装状态。
2. `:Mason`：所需 LSP/formatter/linter 应已安装；失败项可按 `i` 重试。
3. `:checkhealth`：检查 clipboard、Treesitter、Node 和 provider。
4. 打开 `.py`，执行 `:LspInfo`，应看到 Pyright 与 Ruff。
5. 打开 `.ts`，应看到 vtsls 与 ESLint；确保项目内存在 `package.json`。
6. 打开 CMake C++ 项目，应存在 `build/compile_commands.json` 或根目录链接。
7. 保存故意排版错误的文件，文件不应被自动格式化；按 `<Leader>lf` 后才变化。
8. `ollama run qwen2.5:7b "只回答 OK"` 成功后，再测试 `<Leader>Ac`。

常用项目验证命令：

```bash
# Python
ruff check .
ruff format --check .
pyright
pytest

# JavaScript / TypeScript
pnpm eslint .
pnpm tsc --noEmit
pnpm test

# CMake / C++
cmake -S . -B build -DCMAKE_EXPORT_COMPILE_COMMANDS=ON
cmake --build build
ctest --test-dir build --output-on-failure

```

## 6. 快捷键速查

| 快捷键 | 功能 |
| --- | --- |
| `<Leader>e` | Neo-tree 文件树 |
| `<Leader>fp` | 项目切换 |
| `<Leader>pa` | Aerial 大纲 |
| `<Leader>xx` / `<Leader>xX` | 全局/当前文件诊断 |
| `[d` / `]d` | 上一/下一诊断 |
| `<Leader>lf` | 手动格式化 |
| `<Leader>ll` | 手动 lint |
| `s` | Flash 跳转 |
| `gcc` / `gc` | 行注释/选区注释 |
| `[h` / `]h` | Git 修改块导航 |
| `<Leader>gg` | Neogit |
| `<Leader>mr` | 内嵌 Markdown 渲染 |
| `<Leader>mp` | 浏览器 Markdown 预览 |
| `<Leader>Ac` | AI 对话或加入选区 |
| `<Leader>Aa` | AI 动作面板 |
| `<Leader>Ai` | 行内 AI |

## 7. 工程能力的使用方法

工程方面，LSP 负责尽早发现类型、符号和接口错误；formatter 负责机械排版；linter 负责项目规则；测试负责行为。建议始终按“读诊断 -> 小步修改 -> 手动格式化 -> lint/typecheck -> 测试 -> 查看 Git diff -> 提交”的顺序操作。配置只提供反馈工具，真正的规范应进入仓库，例如 `pyproject.toml`、`.clang-format`、`.clang-tidy`、`eslint.config.js` 和 `.prettierrc`。

Markdown 只用于维护 README、架构决策、接口说明和开发记录。CodeCompanion 可用于解释代码、生成测试思路和减少重复劳动，但其输出不能替代代码审查、类型检查和测试。

## 8. 常见坑

- **图标丢失**：不是 Catppuccin 问题。确认 Kitty 的 `font_family` 是 Nerd Font，并执行 `kitty +list-fonts`。
- **透明但没有磨砂**：Neovim 只负责透明；模糊由 Kitty（如 `background_opacity`、`background_blur`）和桌面合成器共同决定。
- **Pyright 与 Ruff 重复诊断**：二者职责有交集但不完全相同；已关闭 Ruff hover，不要关闭 Pyright 类型诊断。
- **clangd 找不到头文件**：先生成 `compile_commands.json`，这通常不是 LSP 安装问题。
- **ESLint 不工作**：优先在项目中安装 ESLint 与配置文件；全局 `eslint_d` 不能替代项目规则。
- **YAML/JSON 没有 schema**：检查网络、SchemaStore 插件以及文件名是否匹配 schema 规则。
- **Markdown Preview 安装失败**：确认 Node/npm 可用，在插件目录执行构建或从 `:Lazy build` 重试。
- **按键冲突**：AstroNvim/which-key 中先查看同前缀映射。本文把 AI 全部放在大写 `<Leader>A`，避免覆盖 LSP 与小写应用键位。
- **升级后 API 失效**：先看 AstroNvim v6 migration、`:Lazy log` 和插件 release notes；不要无条件删除 lockfile。

## 9. 配置维护建议

把 `~/.config/nvim` 建成独立 Git 仓库，提交 `init.lua`、`lua/` 和 `lazy-lock.json`。升级前建立分支，执行 `:Lazy update` 后至少打开 Python、TypeScript、C++ 和 Markdown 各一个样例，再提交 lockfile。这样出现插件回归时可以准确定位版本，而不是反复重装整个编辑器。

参考资料：

- [AstroCommunity 官方文档](https://docs.astronvim.com/astrocommunity)
- [AstroNvim v6 迁移说明](https://docs.astronvim.com/configuration/v6_migration/)
- [CodeCompanion HTTP/Ollama adapter 配置](https://codecompanion.olimorris.dev/configuration/adapters-http)
- [smear-cursor.nvim 文档](https://github.com/sphamba/smear-cursor.nvim)
