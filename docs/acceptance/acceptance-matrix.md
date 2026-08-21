# Acceptance Matrix

状态只能使用：`MISSING`、`PARTIAL`、`IMPLEMENTED`、`TESTED`、`VERIFIED`、`RELEASED`。

| Feature                   | Core        | UI          | IPC         | Integration | E2E     | Restart | Release | Evidence                         | Status  |
| ------------------------- | ----------- | ----------- | ----------- | ----------- | ------- | ------- | ------- | -------------------------------- | ------- |
| Project create/open       | IMPLEMENTED | IMPLEMENTED | IMPLEMENTED | TESTED      | PARTIAL | PARTIAL | MISSING | `CURRENT_STATE.md`, tests        | PARTIAL |
| Explorer/import           | IMPLEMENTED | IMPLEMENTED | IMPLEMENTED | TESTED      | PARTIAL | MISSING | MISSING | `11_FILE_SYSTEM.md`, `16_E2E.md` | PARTIAL |
| Translation save/revision | IMPLEMENTED | IMPLEMENTED | IMPLEMENTED | TESTED      | PARTIAL | PARTIAL | MISSING | `04_DATA_MODEL.md`, Rust tests   | PARTIAL |
| Recovery decision UI      | PARTIAL     | MISSING     | PARTIAL     | PARTIAL     | MISSING | MISSING | MISSING | `12_RECOVERY.md`, P0-001         | MISSING |
| Workbench tabs/split      | IMPLEMENTED | IMPLEMENTED | PARTIAL     | PARTIAL     | MISSING | PARTIAL | MISSING | `09_WORKBENCH.md`                | PARTIAL |
| Settings runtime behavior | PARTIAL     | IMPLEMENTED | IMPLEMENTED | PARTIAL     | MISSING | PARTIAL | MISSING | `13_SETTINGS.md`, P1             | PARTIAL |
| OCR                       | PARTIAL     | PARTIAL     | IMPLEMENTED | PARTIAL     | MISSING | MISSING | MISSING | `18_RELEASE.md`                  | PARTIAL |
| Export                    | IMPLEMENTED | PARTIAL     | IMPLEMENTED | TESTED      | MISSING | MISSING | MISSING | `17_BUILD.md`, adapters          | PARTIAL |
