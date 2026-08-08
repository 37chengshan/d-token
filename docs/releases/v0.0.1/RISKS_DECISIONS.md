# RISKS_DECISIONS — v0.0.1 关键决策与风险

- 决策默认原则（用户口径）：**「所有要完成都考虑」= 纳入 v0.0.1 计划，不代表已完成；不能因为 PRD 把某项标为 v0.3 就把它从计划删除，只能标注 release claim boundary，并由用户最终决定是否阻断 v0.0.1。**
- 状态词：`已决策`（有结论）/ `待用户裁决`（已给默认建议，需用户确认）/ `执行中`（切片在途）。

## 1. 关键决策

| # | 决策项 | 状态 | 决定 | 理由与影响 | 关联 |
|---|---|---|---|---|---|
| D1 | **版本冲突**：目标 v0.0.1 vs 当前 0.2.0-beta.1 | 已决策（默认，需用户确认目标版本） | VERSION-0 为发布阻断；由切片 V-1 一次性统一；本计划包不直接改版本 | 三处以上版本事实源并存；任何未统一版本号的发布结论无效；旧版本号只保留在 history/archive | VERSION-0/1, DOCS-4 |
| D2 | **完整迁移是否纳入 v0.0.1**（Headroom §8 / LeanCTX 九边界 / CCR 等 PRD 标 v0.3+ 的能力） | 已决策（默认纳入计划） | 全部列入 v0.0.1 计划（PR-H*/PR-L* 切片），但 release claim 标注 BOUNDARY：默认不阻断 v0.0.1（PRD P0.0.7 只要求核心可信压缩子集），最终是否阻断由用户裁决；「纳入计划」≠「已完成」 | 避免两个错误：a) 因 PRD 标 v0.3 就删掉计划（用户明确反对）；b) 把计划存在写成完成（GOAL 完成定义禁止）。发布报告必须逐项标注 claim boundary | CONTEXT-1/2/3, PR-H*, PR-L*, K6 |
| D3 | **外部签名**（macOS Developer ID/notarization/Gatekeeper、Windows Authenticode） | 已决策（外部依赖） | 签名凭据是外部证据，仓库内无法证明；不具备时结论只能是「未签名技术预览（明确标注）」或 blocked；不允许绕过系统安全保护 | 公开 prerelease 不等于签名/公证/系统信誉；结论文案受控（VERIFICATION §6） | K3, PLATFORM-4, PR-8/9 |
| D4 | **Optical production active** | 已决策（不开启） | v0.0.1 只允许 shadow/preview；active 默认空且关闭，UI 无开关，wildcard/未知模型拒绝；不扩大为普通图片压缩 | PRD 12.5 硬边界；active 门槛（renderer/font/profile hash、eval digest、可信 authority）未全绿前不可达 | OPTICAL-1/2/5, G10 |
| D5 | **LeanCTX storage 形态** | 已决策（d-token-owned Rust） | LeanCTX 对齐层由 d-token-owned Rust service 提供（Context Bus/Session Cache 等九边界），不加载 Python sidecar、不引入 lean-ctx 运行依赖；cache 只保存脱敏结构化元数据与引用，不保存 secret/未脱敏原文；CCR 原文存储独立（加密/TTL/配额/权限） | 来源仅作行为对照（OSS_MIGRATION §3）；无真实跨 Agent 证据时状态只能 partial/unavailable | CONTEXT-1, PR-L*, OSS-3 |
| D6 | **公开 prerelease**（GitHub public unsigned prerelease 聚合 workflow） | 已决策（保持现状边界） | 同一 tag/同一 commit 聚合 macOS ARM64 + Windows x64，双端门禁全绿才创建；产物标 unsigned/unnotarized；不宣称签名、公证或系统信誉；V-1 后 tag 与版本号 v0.0.1 绑定 | workflow 接线只允许 PR-7；不围绕 GitHub PR #6 云端 CI 失败修 CI | PLATFORM-3/4, PR-7, K2 |
| D7 | **no-touch 开发边界** | 已决策（保持） | 开发默认不触碰真实 Agent 配置/凭据；fixture/fake-bin/poison-bin、临时 HOME/PATH/端口为 gate；真实写操作只在用户明确授权与受控边界内 | M1 不变量；任何 managed 写穿视为返工 | INV-4, SECURITY-9, PR-6 |
| D8 | **cc-switch 只读** | 已决策（保持） | 只读 DB/WAL/SHM watcher；首次 hash-bound 授权后仅自动更新 d-token 自有 route/runtime；绝不写 cc-switch；A→B→A 由主代理 Computer 完成 | 真实 UI 切换不由实现 Agent 操控 | AGENT-5, SECURITY-7, N5 |
| D9 | **Codex 路由不替代** | 已决策 | Codex 必须独立真实 routed-flow（N2）；配置恢复/MCP/Hook 接入不得替代 | PRD P0.0.7/P0.0.10.5 | K5, AGENT-1 |
| D10 | **证据治理** | 已决策 | 过期证据隔离；全部机器证据绑定 `C_final` + source digest；聚合器 fail-closed；旧 pass artifact 不可穿透 | 2026-08-04 G7、旧 DMG、browser preview 不得绑定最终候选 | EVID-1/2/3, K6 |

## 2. 风险登记

| # | 风险 | 等级 | 缓解 | owner |
|---|---|---|---|---|
| R1 | 版本统一（V-1）被用户搁置 → 所有发布结论无法以 v0.0.1 名义落盘 | High | V-1 前置在 PR-9/N0 之前；发布门 fail-closed 检查版本 | Main |
| R2 | Windows 实机/MSVC/G6 PowerShell/签名凭据不可获得 → 只能未签名预览或 blocked | High | 显式 external 标注；fan-in/runner 证据先接线；最终三结论如实判定 | Main/外部 |
| R3 | provenance 漂移（D1–D5）未闭合 → 分发声明不可信 | High | OSS-1 MUST；漂移任务独立切片；N0 grep 断言 | PR-CBM1/H1/O1/L0/R1 |
| R4 | 「纳入计划」被误读为「v0.0.1 必须完成」，或反过来被当作「已完成」 | Medium | 本文件 D2 + requirement release claim 列 + 发布报告 boundary 标注 | Main |
| R5 | 并行切片触碰共享文件（dtoken-types/core-api、gate 聚合、workflows、根 NOTICE、PLAN） | High | EXECUTION §5 串行化规则；主代理合并期统一维护；git 审计账本 | Main |
| R6 | 旧证据冒充（mtime/latest/pass-only 穿透） | High | PR-7/PR-9 candidate binding；EVIDENCE.md 过期清单 | PR-7/9 |
| R7 | 多文档版本事实源继续漂移（PRD/PLAN/本包/IMPLEMENTATION_STATUS） | Medium | 文档优先级（README §4）；本包随执行滚动；收口统一更新（DOCS-3） | Main |
| R8 | 侧栏/UI 真实窗口证据不足（浏览器 preview 冒充） | Medium | N6 主代理 Computer 全组合；PR-4 Playwright 只作 layout 前置 | Main/PR-4 |
| R9 | LeanCTX/Headroom 长期能力被整体误报完成 | High | 逐能力矩阵 + release claim boundary；K6 不关闭 | PR-H*/L* |
| R10 | 配置/凭据验收污染真实用户环境 | High | 隔离临时根副本 + 唯一 namespace + exact restore；故障注入 | PR-6/8 |
| R11 | 当前分支 HEAD 仍在变动（CI 稳定化提交链） | Medium | P0 复核冻结 commit；N0 以 `C_final` 为准；证据绑定禁止漂移 | Main/P0 |

## 3. 待用户裁决清单（进入 W4 前必须确认）

1. 目标版本确认为 **v0.0.1**（D1；确认后 V-1 才能执行）。
2. Headroom 完整迁移 / LeanCTX 九边界等 **BOUNDARY 能力**是否阻断 v0.0.1（D2 默认不阻断，用户可改判）。
3. 未签名技术预览作为 v0.0.1 交付形态是否可接受（D3/D6；默认：无签名凭据时只能预览或 blocked）。
4. GitHub public prerelease 的版本命名与 tag 策略（D6，与 V-1 联动）。
5. RTK v0.45.0、pxpipe v0.12.0、lean-ctx main 等**新 pin 评估结论**是否采纳（OSS-1，切片评估后提交用户确认）。

## 4. 决策变更协议

- 执行 Agent 不得自行改写本文件决策；变更由主代理记录并同步本文件与 [EXECUTION.md](./EXECUTION.md) §7。
- 用户裁决结果落盘后，对应 requirement 的 release claim 与 [TASKS.md](./TASKS.md) 状态同步更新。
