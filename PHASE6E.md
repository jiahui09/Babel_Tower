# Phase 6E：资源工作流交付边界

## 已完成：6E1

6E1 完成桌面资源模式的真实边界，不伪造尚未具备的图像能力：

- `tauri-plugin-dialog` 提供系统文件和目录选择器。
- `resource_queue` 从核心读取 `ImageRegion` 工作项，使用 `(reading_order, unit_id)` 游标分页。
- 资源页按同一队列连续切换区域，识别结果来自工作项源文本，人工译文通过既有 `save_translation` 持久化。
- 原始图片路径只作为资源关联显示；原件不被写入，React 不保存第二份权威译文。

## 已完成：6E2 基础核心

- `babel-image` 独立实现原图哈希复核、空间区域边界校验、平色背景清理、字体栅格化、换行和溢出拒绝、确定性 PNG 输出。
- `Kernel::read_object` 只返回通过 CAS 哈希复核的对象；`Kernel::render_image_region` 不修改源对象。
- SQLite schema 10 增加 OCR 可重建缓存与人工区域 revision/head；人工修正使用幂等命令和独立提交序列。
- Tauri/React 接入人工修正原文保存，识别结果、人工修正原文和人工译文保持三层分离。

## 已完成：6E3 Linux x64 OCR 闭环

本轮已完成图片像素读取链路：

- `Kernel::read_image_preview` 只接受活动 generation 中的 `Image` 资源，按资源 locator 读取并再次校验 CAS 对象哈希。
- 支持直接字节范围和 EPUB `ArchiveMemberByteSpan`；不支持的 locator 会明确失败，不回退到任意文件路径。
- 解压成员、图片格式和预览大小均有边界；当前只接受 PNG、JPEG、WebP，预览上限为 20 MiB。
- Tauri `image_preview` 只返回带媒体类型的受控 data URL；React 不接触 CAS 路径、EPUB 路径或项目目录。
- 资源页显示真实原图并叠加当前区域定位，仍沿用现有资源队列和人工译文链路。

本轮已完成 OCR 资产和真实推理闭环：

1. `resources/ocr/ppocrv6-tiny/manifest.json` 固定 ONNX Runtime、PP-OCRv6 tiny 检测/识别模型和模型匹配字典的版本、大小、SHA-256 与许可证标识；worker 启动前逐项验证清单。
2. `babel-ocr-worker` 通过版本化 IPC 提供 `ProbeAssets` 和 `Recognize`，使用受控图片字节输入，不访问网络、不从缓存自动下载模型。
3. worker 将识别文本、置信度和四点多边形返回为候选结果；候选与人工修正原文、人工译文保持独立，未覆盖任何权威译文数据。
4. ONNX Runtime 以相对运行库路径加载，Linux x64 发布目录可不依赖外部 `LD_LIBRARY_PATH`。

验证：离线启动 worker 后，资产探测返回 `verified: true`、`asset_count: 4`；对测试 PNG 识别出 `Babel Tower`，置信度 `0.997559`，并返回有效区域多边形。

Windows 对应运行库和最终安装包仍按既定策略留到发布阶段；Linux 环境不做 Windows 实机验证。

## OCR 扩展契约已固化

`babel-ocr` 现在提供跨引擎的 `OcrProfile`、`OcrRequest`、`OcrEngine`、`OcrDocument` 和能力声明。PP-OCR worker 通过该契约返回带源哈希、Profile、模型 ID、页面尺寸、阅读顺序、语言、置信度和坐标的统一结果；它明确声明当前不提供自动语言检测、版面分析、表格或竖排能力。后续 PDF 文字层解析、扫描页 OCR、多语言路由和 AI 辅助都应新增引擎适配器，不得把分支扩散到翻译核心。

## 6E4：OCR 候选持久化已完成第一刀

- `image_region_ocr_cache` 现在由 `ProjectStore::save_ocr_candidate` 和 `ProjectQuery::ocr_candidate` 统一读写。
- 缓存键为 `generation_id + region_resource_id + model_hash`；不同模型结果并存，同一键重复写入必须内容一致，否则拒绝覆盖。
- 写入前反序列化并校验 `OcrDocument`，只接受活动 generation 下的 `ImageRegion`，不推进翻译提交序列，不触碰人工修订 head。
- `Kernel` 已提供对应的保存和读取边界，后续 Tauri/资源页只需接入 worker 结果，不需要绕过核心写 SQLite。

- Tauri 已通过受控 `ProcessWorker` 调度 OCR：启动握手、超时、响应上限、模型资产路径和运行库路径均由桌面边界负责；识别结果回到 `Kernel` 后才进入缓存。
- 资源页已提供“重新识别”入口，识别结果按统一 `OcrDocument` 展示，并保留区域上下文；失败不会修改人工译文或原始图片。

## 6E5：嵌字派生图第一条闭环

- 资源页将原图、识别结果、人工修正原文和人工译文保持为四个清晰区域，不把 OCR 结果直接当作译文。
- “生成预览”通过 Tauri 读取经过哈希校验的图片对象，调用 `babel-image` 确定性渲染器生成 PNG。
- 派生 PNG 先进入 CAS，再由 `image_region_revision` 记录字体指纹、字号、颜色、内边距和派生对象哈希；原图不会被覆盖。
- 生成结果在左侧画布直接预览，可一键回看原图；嵌字失败只反馈错误，不修改人工内容。
- 当前字体解析优先使用 `BABEL_IMAGE_FONT`、应用资源字体或平台字体；最终离线安装包必须将字体作为受许可的资源资产闭包。

## 6E6：图片导出覆盖闭环

- `babel-adapter-protocol` 新增格式无关的 `ImageOverlay`，导出层可以携带图片资源 locator、派生对象能力句柄和媒体类型，不让适配器直接访问 SQLite。
- 核心在冻结提交序列下读取当前图片区域 revision，校验派生对象仍存在，并将覆盖投影交给格式适配器。
- EPUB 适配器按图片成员聚合派生区域并在原图上确定性合成，未修改成员继续使用 ZIP raw copy；成员缺失、源对象不一致、区域或图片尺寸无效时拒绝导出。
- Markdown 通过文件路径导入时会将可解析的相对图片发布到 CAS，并把图片资源定位升级为对象字节范围；文件导出会预检目标并写出 Markdown 主文档及相对图片闭包。网络图片、越界路径和缺失文件保留为结构定位，并在导出时明确拒绝。
- 同一图片的多个区域不会选择性覆盖：EPUB 和 Markdown 都以原图为底、依据区域多边形按资源稳定顺序合成各派生层，尺寸或格式不一致时拒绝导出。
