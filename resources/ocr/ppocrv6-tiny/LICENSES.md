# OCR 资产来源与许可证

本目录的模型来自 PaddlePaddle 在 Hugging Face 发布的 PP-OCRv6 tiny ONNX 模型仓库，运行库来自 ONNX Runtime 的官方发布包。运行时不会联网下载或更新这些文件；`manifest.json` 中的字节长度和 SHA-256 是启动时的完整性边界。

- PP-OCRv6 tiny detection: `PaddlePaddle/PP-OCRv6_tiny_det_onnx`
- PP-OCRv6 tiny recognition: `PaddlePaddle/PP-OCRv6_tiny_rec_onnx`
- ONNX Runtime: `onnxruntime-linux-x64-1.20.0` (Rust binding `ort 2.0.0-rc.9`)
- 代码/运行库许可证标识：`MIT`、`Apache-2.0`
- 模型许可证标识：`Apache-2.0`、`PaddleOCR-Model-License`

`rec_inference.yml` 仅作为字典来源记录，运行时使用由其 `character_dict` 提取出的 `dict.txt`，确保字典顺序与识别模型输出层一致。
