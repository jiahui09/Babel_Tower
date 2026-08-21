# ADR 0007: Fixture Bridge 仅用于显式开发测试

## 状态

已实现。

## 决策

fixture Bridge 用于浏览器级 UI 测试和开发演示，生产模式始终使用 Tauri Bridge，IPC 故障不会回退为 fixture 数据。

## 后果

fixture Playwright 只能证明路由和 UI 投影；不能证明文件系统、SQLite、worker、OCR 或导出。真实桌面 E2E 仍是缺口。

下一步：[16_E2E.md](../16_E2E.md)。
