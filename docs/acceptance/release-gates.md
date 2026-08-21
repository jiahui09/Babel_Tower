# Release Gates

发布只有在证据齐全时推进；历史 Phase 3 TXT 产物不能替代桌面发布验证。

## Code

- [ ] `pnpm test`
- [ ] typecheck / eslint / prettier
- [ ] `pnpm build`
- [ ] `cargo test --workspace`
- [ ] `./tools/check-architecture.sh`

## Product

- [ ] Project
- [ ] Explorer / Import
- [ ] Editor / Save
- [ ] Recovery
- [ ] Settings
- [ ] OCR
- [ ] Validation
- [ ] Export

## Desktop

- [ ] Real Tauri E2E
- [ ] isolated filesystem
- [ ] restart and recovery
- [ ] worker/runtime

## Windows

- [ ] native build
- [ ] install / launch
- [ ] OCR / export
- [ ] uninstall

## Compliance

- [ ] fonts
- [ ] licenses / third-party notices
- [ ] SBOM
- [ ] artifact hash and manifest validation

每项记录环境、命令、日期、产物路径和结论。缺证据状态为 `MISSING` 或 `BLOCKED`。
