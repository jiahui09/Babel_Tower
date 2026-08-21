# Babel Tower 发布说明

本文只记录当前仓库里能被证实的发布事实。结论先说清楚：当前桌面产品还不能当作完整可发布闭包。

## 先看结论

- 现有 `release/` 里的材料主要证明历史 Phase 3 TXT 纵切包。
- 真实桌面产品的 Windows 安装、启动、OCR、导出和文件系统闭环还没有完成。
- 字体、许可证和 SBOM 的最终闭包也没有完成。

## 现有发布材料

| 材料                                     | 当前状态                                   | 说明                                               |
| ---------------------------------------- | ------------------------------------------ | -------------------------------------------------- |
| `release/arch/release-manifest.json`     | Existing, but for historical phase3 slice  | 证明 Arch 方向曾经能打包，不证明当前桌面产品已发布 |
| `release/windows/pe-build-manifest.json` | Existing, unverified for native acceptance | 说明交叉构建过 PE，但不等于安装器和实机验收完成    |
| `release/windows/wine-probe.json`        | Existing probe evidence                    | 只能说明某个探针跑过，不能替代 Windows 实机闭环    |
| `packaging/arch/*.pkg.tar.zst`           | Historical artifact                        | 属于 Phase 3 TXT 纵切，不是当前桌面正式发布物      |
| `resources/ocr/ppocrv6-tiny/`            | Source asset exists                        | 资源文件存在，但完整 runtime / release 证据还不够  |

## 还缺什么

| 发布项              | 状态    | 说明                                                       |
| ------------------- | ------- | ---------------------------------------------------------- |
| Windows 原生安装    | Blocked | 需要 Windows 环境和安装态验证                              |
| Windows 原生启动    | Blocked | 需要安装后的真实桌面验证                                   |
| Windows OCR runtime | Blocked | 需要实机和资源闭包证据                                     |
| Windows 导出验证    | Blocked | 需要真实桌面导出与文件校验                                 |
| 字体闭包            | Missing | 仓库里没有完成的生产字体闭包证据                           |
| 许可证闭包          | Partial | 有 workspace AGPL 和 OCR LICENSES.md，但完整产品包还没闭合 |
| SBOM                | Missing | 没有当前桌面产品的最终验证版 SBOM                          |
| 真实桌面 E2E        | Missing | 没有安装态自动化，不能替代发布验收                         |

## 可以用哪些脚本看发布材料

- `./packaging/build-arch-phase0.sh`
- `./packaging/build-windows-phase0.sh`
- `./packaging/verify-arch-phase0.sh`
- `cargo run --release -p babel-phase0 -- package-closure --release-dir release`

这些脚本适合看历史纵切包，不适合把当前桌面产品直接判成可发布。

## 发布验收的最低门槛

1. 前端质量门禁通过。
2. 真实桌面 E2E 通过。
3. Windows 和 Linux 的原生验证各自完成。
4. OCR、导出、字体、许可证、SBOM 都有同一版本的证据链。

## 下一步阅读

1. [17_BUILD.md](17_BUILD.md)
2. [15_TESTING.md](15_TESTING.md)
3. [CURRENT_STATE.md](CURRENT_STATE.md)
