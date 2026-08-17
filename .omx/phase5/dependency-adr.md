# ADR：EPUB 低层依赖选择

状态：已采用
日期：2026-08-18

## 决策

- ZIP：`zip 8.6.0`，MIT，MSRV 1.88；关闭默认特性，只启用纯 Rust `deflate-flate2-zlib-rs`。
- XML/XHTML：`quick-xml 0.41.0` + `encoding`，MIT，MSRV 1.79。
- 不引入完整 EPUB 框架，不引入 DOM 库。

仓库 MSRV 为 Rust 1.97，因此两项依赖均满足工具链约束。许可证数据来自本地 Cargo registry manifest，版本由 `Cargo.lock` 固定。

## 原因

`zip` 提供 central directory 读取、加密/压缩信息、`raw_copy_file` 和可写 `ZipWriter`，能实现路径、展开量与未改 payload 的控制。`quick-xml` 提供有界事件流和字节位置，允许只为可编辑文本建立 member-local span，不需要重排 XHTML DOM。

完整 EPUB 库通常面向阅读或生成，无法提供 Babel Tower 所需的稳定身份、原成员复用、保护跨度和候选验证控制面。`roxmltree` 曾作为小 XML DOM 候选，但当前事件流实现已足够，拒绝增加第三项解析依赖。

## 约束

- 依赖的默认行为不构成安全边界；成员数、路径、压缩比、XML 深度等由适配器显式校验。
- 只接受 Stored/Deflated；加密、symlink、重复路径和根目录逃逸均拒绝。
- 依赖必须进入最终离线安装包闭包，但 Phase 7 才执行双平台发布闭包签发。

## 外部依据

- EPUB 3.3：`https://www.w3.org/TR/epub-33/`
- EPUBCheck：`https://github.com/w3c/epubcheck`
- zip：`https://docs.rs/zip/8.6.0/zip/`
- quick-xml：`https://docs.rs/quick-xml/0.41.0/quick_xml/`
