# Babel Tower Desktop

## 开发启动

从仓库根目录启动桌面应用：

```bash
pnpm dev
```

此命令会先运行 Vite，再由 Tauri 打开桌面窗口；桌面核心、文件选择器和本地项目数据都只能在这个窗口中使用。

`pnpm dev:web` 仅用于浏览器界面调试，不提供 Tauri IPC。它会明确提示桌面核心不可用，也不会加载演示数据。

在部分 Linux Wayland/GBM 驱动组合上，WebKit 可能无法分配 DMA-BUF 图形缓冲。出现 `Failed to create GBM buffer` 时，使用软件合成回退启动：

```bash
pnpm dev:linux-safe
```

或者从本目录执行：

```bash
pnpm tauri:linux-safe
```

该回退只影响 Linux 开发期的图形渲染路径，不改变项目数据、IPC 或最终 Windows 发布配置。

## 验证

```bash
pnpm check
pnpm test
```
