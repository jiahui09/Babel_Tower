# Phase 5 EPUB 支持矩阵

日期：2026-08-18

| 能力 | 状态 | 约束与证据 |
| --- | --- | --- |
| EPUB 2 OPF/spine/NCX | 支持 | 要求 `spine@toc` 指向 manifest 中的 NCX；UTF-8 成员路径有专项测试。 |
| EPUB 3 OPF/spine/nav | 支持 | 要求 manifest 中存在 `properties="nav"` 的导航文档；内部断链会拒绝。 |
| 多章节阅读顺序 | 支持 | 顺序只来自 spine；生成 `ReadingOrderAfter`，应用遍历全部 `TextStream`。 |
| XHTML 人工翻译 | 支持子集 | 可编辑正文限定 UTF-8 XML/XHTML 文本节点；标签、属性、实体、脚本、样式和链接目标不作为译文。 |
| CSS、图片、字体、二进制资源 | 保留 | 作为 ResourceGraph 节点保存；未修改成员 raw copy。首版不编辑 CSS、字体或图片内容。 |
| XML 引用 | 支持 | XHTML/nav/NCX 的内部 `href`/`src` 形成 `References` 边；外部 URI 保留但不解析。 |
| CSS `url()` 引用语义 | 仅保留 | CSS payload 原样保存，首版不解析 CSS 引用图。 |
| 重导入与稳定身份 | 支持 | 成员路径、结构路径、可见文本和邻域构成 source key；spine 重排可精确继承。改名/歧义走通用审查。 |
| 导出 | 支持 | 只补丁已翻译 XHTML 文本跨度；未改成员保留原压缩 payload；候选先验证，再通过应用层流式、原子 no-clobber 发布到目标路径。 |
| EPUB 3.3 外部验证 | 支持 | EPUBCheck 5.3.0：0 fatal / 0 error / 0 warning；`unzip -t` 无错误。 |
| EPUB 固定版式、媒体叠加、脚本交互 | 不声明支持 | 成员可保留，但首版不承诺可编辑或语义验证。 |
| SVG/MathML 内嵌文字 | 不声明可编辑 | 资源保留，不提取为翻译单元。 |
| DRM、加密 ZIP、未知压缩法 | 拒绝 | 不尝试解密或降级导入。 |

## 安全预算

- 输入：最多 1 GiB；输出：最多 2 GiB。
- ZIP：最多 20,000 成员、总展开 4 GiB、单成员 512 MiB、单成员压缩比不超过 1000。
- XML：单文件最多 32 MiB、深度 256、单元素属性 1024、文本节点 1,000,000。
- IPC：帧最多 4 MiB；抽取页最多 2,000 单元，并预留约 1 MiB 帧开销。
- Worker：EPUB 请求最长 120 秒；任务字节预算为 `min(8 * 输入, 4 GiB)`。
- 大成员读取：64 KiB 分块解压，每块检查取消与 deadline。

“可保留”不等于“可编辑”。此矩阵是首版支持声明的上限。
