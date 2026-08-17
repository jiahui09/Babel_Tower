# Phase 3 TXT 纵切离线包验证

这些入口验证发布链路、原生二进制装载、TXT worker 随包分发和离线安装闭包。它们证明 Phase 3 TXT 纵切可打包，不代表最终桌面产品已经包含完整编辑器、OCR 模型或多格式资源链路。

## Arch

```bash
./packaging/build-arch-phase0.sh
```

脚本生成 `release/arch/*.pkg.tar.zst`，然后用 Docker 的 `--network none` 启动干净 Arch 基础镜像，安装后实际执行 `babel-phase3 smoke`，覆盖 TXT 导入、人工译文保存、搜索、校验、保真导出与重开恢复。
同时生成 `release/arch/release-manifest.json`，记录产物哈希、依赖、`babel-phase3` 和 `babel-txt-worker` 离线探针事实。

Ubuntu CI 使用 `./packaging/build-arch-ci.sh`：第一只 `archlinux:base-devel` 容器负责编译和 `makepkg`，第二只禁网基础容器只负责安装与运行验证。构建失败时 manifest 保持 `clean_image.performed=false`，不会沿用旧证据。

## Windows

```bash
./packaging/build-windows-phase0.sh
```

脚本用 `cargo xwin` 构建 `babel-phase3.exe` 与 `babel-txt-worker.exe`，优先调用 Linux 原生 `makensis` 生成单文件安装器，本地没有原生 NSIS 时才回退到 Wine。Wine 仅是构建工具；Linux 流程不安装、不启动、不验收 Windows 包。
同时生成 `release/windows/release-manifest.json`；其中 `runner_kind=linux-cross-build` 明确表示只可发布，不能进入 Windows 实机闭包结论。安装、启动、worker IPC 和离线性必须由 Windows runner 单独验证。

若 PE 已构建但安装器封装失败，脚本只保留 `pe-build-manifest.json`，并在构建开始时清除旧安装器和旧 release manifest，避免新二进制继承历史签发结果。

## 闭包判定

```bash
cargo run --release -p babel-phase0 -- package-closure --release-dir release
```

输出包含两层结论：Phase 3 TXT 纵切发布门禁应为 `SUPPORTED`，其中 Windows 为 `BUILT_UNVERIFIED`；最终生产整包因缺少桌面壳、OCR worker/模型和完整多格式资源链路而保持 `FALSIFIED`。不能用前者替代后者。
