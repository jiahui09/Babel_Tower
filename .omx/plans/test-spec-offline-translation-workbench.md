# Babel Tower 首版测试规格

## 1. 测试目标

证明权威数据不会因 UI、解析器、OCR 或导出失败而丢失；证明三种工作空间共享一致状态；证明 TXT、Markdown、EPUB 2/3 的受支持往返行为；证明 Windows 与 Arch Linux 在断网环境中完成完整链路。

## 2. 单元测试

### 2.1 领域与命令（拟议：`crates/babel-domain/`）

- 内容单元 ID 在重复导入同一源对象时保持稳定。
- 状态转换只接受允许的迁移，进度统计与状态一致。
- 译文修订为追加写；撤销生成补偿命令，不删除历史。
- 乐观修订号冲突返回结构化错误，不覆盖较新译文。
- 工作空间命令投影到相同 `translation_head`。

### 2.2 存储（拟议：`crates/babel-storage/`）

- 每个迁移在空库和上一版本 fixture 上成功且幂等受控。
- 写事务原子更新修订、head、状态、命令日志和 FTS 索引。
- 模拟提交前/后故障，重开数据库后只能看到完整旧状态或完整新状态。
- 新版本数据库被旧应用只读拒绝，不进入写模式。
- Backup API 生成的快照可恢复并通过完整性检查。
- `synchronous=FULL` 下 commit 后强制终止/模拟掉电，所有已 ack 修订存在。
- commit 成功但 ack 丢失时，相同 `command_id` 重试不产生重复修订。
- 可移植备份的数据库可达对象闭包全部存在且哈希一致；缺一对象时拒绝发布/恢复。

### 2.3 格式适配器（拟议：`crates/formats/*`）

- TXT 保留 BOM、编码、CRLF/LF 和最终换行；不可编码字符触发阻断诊断。
- Markdown span patch 从文件末尾逆序应用，重叠、漂移或哈希不符时拒绝导出。
- ZIP 路径穿越、绝对路径、符号链接逃逸和解压炸弹被拒绝。
- EPUB `mimetype`、container、OPF、manifest、spine、nav/NCX 和资源链接校验。
- XHTML 解析禁用外部实体；未修改条目 payload hash 保持一致。
- 嵌套强调/链接、实体、ruby、bidi、tail text 和受支持行内 HTML/XHTML 的 code multiset、配对、嵌套与 escaping 保持有效。

### 2.4 OCR 与图片（拟议：`workers/ocr/`、`crates/babel-image/`）

- worker 协议拒绝未知版本、越界路径、超限图片和错误模型哈希。
- OCR 输出坐标标准化、旋转和裁剪映射可逆。
- 排版计算在不同 DPI、缩放和长文本下不越出指定区域。
- 复杂背景不满足安全修补条件时，系统要求人工处理而非自动破坏图像。

## 3. 属性与模糊测试

- 对任意非重叠字节跨度集合，受控替换不改变跨度外字节。
- 对随机 ZIP entry 名称和嵌套路径，解包结果始终位于任务沙箱。
- 对随机 Unicode、组合字符、RTL 和 CJK 文本，单元切分与重组不丢码点。
- 对随机合法 inline code 序列，重组后 code 身份、多重集和嵌套不变量成立；非法移动被拒绝。
- 对随机命令序列，undo/redo 后领域状态等于对应历史前缀。
- 对 Markdown/XHTML 解析入口运行 fuzz，崩溃只产生任务失败诊断。

## 4. 集成测试

1. 导入 fixture -> 提取 -> 编辑 -> 状态更新 -> 搜索 -> 重启 -> 导出。
2. 同一内容单元在长文、结构化和资源工作空间命令路径下得到相同修订 ID。
3. OCR worker 正常、超时、被终止、输出损坏、模型缺失五种情形下数据库一致。
4. 导出到 staging 后在写入、校验、fsync、发布各阶段注入失败，目标文件和源对象保持预期状态。
5. 迁移前自动备份、迁移中断、恢复旧快照、再次迁移完整闭环。
6. FTS5 索引与译文 head 在失败重试后无幽灵或缺失记录。
7. 对象写入、文件 flush、父目录同步、发布、DB 引用各阶段故障最多留下可隔离孤儿，不产生悬空引用。
8. Core Service 与 UI 断连/重连后按 `commit_sequence` 重放事件，三个工作空间收敛。

## 5. 黄金样本矩阵

| 格式 | 必需 fixtures | 关键断言 |
| --- | --- | --- |
| TXT | UTF-8/BOM、UTF-16、CRLF、无最终换行、不可表示字符 | 编码与换行保真、明确阻断 |
| Markdown | CommonMark、嵌套强调/链接、实体、硬换行、行内 HTML、图片、重复段落、未知扩展 | code 不丢失；envelope 外字节不变；未知语法阻断 |
| EPUB 2 | NCX、多个 spine、CSS/字体/图片、嵌套路径 | 未修改 entry payload 一致、可打开 |
| EPUB 3 | nav、XHTML mixed content、ruby、bidi、tail text、SVG/图片、metadata、非线性 spine | code/清单/引用一致、修改 XHTML 合法 |
| 图片 | 横排、竖排、旋转、低对比、平坦/复杂背景 | 区域可编辑、译文来自用户、原图不变 |

## 6. 端到端测试

- Windows x64：使用最终 NSIS 包在无网络虚拟机安装，首次启动完成完整链路。
- Arch Linux x86_64：使用最终 AppImage 在无网络干净镜像完成完整链路。
- Arch 包矩阵：FUSE present/absent、默认下载无执行位、受支持 glibc/WebKitGTK 组合；任何启动前失败必须在发布支持声明中被准确覆盖。
- 进程故障：输入持续写入时终止 UI；恢复后已显示 `saved` 的修订全部存在。
- 磁盘故障：分别模拟空间耗尽、只读目标、跨卷目标和目标已存在。
- 大项目：10 万文本单元、2 GB EPUB 资源 fixture（性能场景，不是硬上限）完成导入、搜索和增量保存，不要求一次加载全部内容。安全上限另以 entry 数、展开总量和压缩比策略测试。

## 7. 性能预算

- 10 万单元项目冷启动到可操作界面：参考硬件 p95 <= 5 秒。
- 结构化列表滚动：参考硬件 p95 帧时间 <= 20 ms。
- 编辑命令从 UI 发出到 `synchronous=FULL` durable ack：p95 <= 300 ms，p99 <= 750 ms；输入呈现不得等待 ack。
- 普通全文搜索（10 万单元）：p95 <= 300 ms。
- 参考硬件：4 核 x86_64、16 GB RAM、NVMe SSD，使用当期 Windows 11 与 Arch Linux；更慢存储以数据安全优先，不降级 `synchronous=FULL`。

## 8. 可观测性与诊断

- 本地结构化日志记录任务 ID、适配器、阶段、耗时和错误码，默认不记录源文、译文或图片内容。
- 每个导入/OCR/导出任务保留状态机事件，可生成用户主动导出的脱敏诊断包。
- UI 保存指示器与核心 ack 序号关联；不得根据 debounce 计时器推断已保存。

## 9. 发布门禁

- `cargo fmt --check`、`cargo clippy --all-targets --all-features -D warnings`、Rust tests。
- TypeScript typecheck、ESLint、前端单元测试和 Playwright 桌面 smoke。
- `cargo audit`、JS 锁文件审计、许可证清单、模型和 sidecar SHA-256 清单。
- Windows/Arch 断网安装 E2E、数据库故障注入、格式黄金样本全部通过。
- Tauri 只有通过 Arch FUSE/WebKitGTK、Windows WebView2、IME 与 ProseMirror 平台门禁才可冻结；否则触发 Electron 壳 ADR 复审。
- 没有已知 P0/P1 数据损坏缺陷；任何未验证格式限制写入发布说明和 UI 诊断。
