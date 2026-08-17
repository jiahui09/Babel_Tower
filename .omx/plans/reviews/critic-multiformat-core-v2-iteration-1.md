# 评论员终审：Babel Core v2（第 1 轮）

日期：2026-08-17  
角色：Critic  
结论：`ITERATE`

## 结论

核心架构、RALPLAN 决策、测试证伪性和实施阶段均通过；唯一阻塞项是“单个完整离线安装包”的运行时边界未闭合。测试规范中的“WebView 之外”例外与用户要求冲突，执行者无法判断 Windows WebView2、Arch WebKitGTK、OCR 模型和其他运行依赖由安装包还是系统提供。

## 最小修改

1. Windows 使用包含 WebView2 Evergreen 离线安装器的单一安装包，不使用联网 bootstrapper 或 `skip`。
2. Arch 使用自包含 AppImage，随包携带应用运行依赖；固定最低内核/glibc/图形会话基线，并在干净离线镜像验证。
3. 两个平台的适配器、OCR runtime/模型、字体、许可证、清单和 SBOM 都在发布产物内。
4. 删除 PKG-01 的 WebView 例外，增加运行时依赖枚举与联网监测。
5. 记录 ADR-006，并明确 Phase 0 时间盒可以延长但退出门禁不能跳过。

## 非阻塞意见

架构偏重，但已经用 Phase 0 的 2–3 周证伪时间盒控制。若时间不足，可延长时间盒，不能以跳过绑定、DAG、writer 或安全原型换取表面进度。
