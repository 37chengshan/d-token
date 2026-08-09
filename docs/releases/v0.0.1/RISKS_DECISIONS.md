# RISKS_DECISIONS — v0.0.1 关键决策与风险

- 决策原则：用户已明确要求完整 Headroom、LeanCTX、RTK、Optical 差异化能力全部进入统一 d-token-owned 框架并在 v0.0.1 完成；**不存在 BOUNDARY 或默认不阻断能力**。实现存在不等于完成，缺同候选证据仍 `release_ready=false`。
- 状态词：`已决策` / `执行中` / `external`；`external` 不降低 MUST，只表示证据需外部平台或凭据。

## 1. 关键决策

| # | 决策项 | 状态 | 决定 | 理由与影响 | 关联 |
|---|---|---|---|---|---|
| D1 | **目标版本** | 已决策 | 目标就是 v0.0.1；V-1 在全部功能切片后一次性统一 manifest/PRD/PLAN/status/changelog/workflow | 无需再次询问版本；未统一前不可生成最终候选 | VERSION-0/1, DOCS-4 |
| D2 | **完整迁移阻断 v0.0.1** | 已决策 | Headroom 117 项、LeanCTX 九边界、RTK 差异化生产 stage、Optical exact active 全部是 MUST；任一缺失阻断 | 消除原计划“纳入但不阻断”的范围漏洞 | RUNTIME-7..9, CONTEXT-1..4, OPTICAL-1..6, QUALITY-1..5 |
| D3 | **外部签名** | 已决策（外部依赖） | 无签名凭据不能称正式发布；可生成明确未签名技术候选，但 `release_ready=false`，不算目标完成 | 不绕过系统信誉 | PLATFORM-4, N9 |
| D4 | **Optical production active** | 已决策 | UI 永无 active 开关；planner 在 exact attestation + economics + paired eval + resource + validation 全绿时自动 active，任一门不绿 PreserveText | “自动 stage”与“无用户 engine chooser”同时成立 | OPTICAL-1..6, N10 |
| D5 | **LeanCTX domain 归属** | 已决策 | 新建 `dtoken-context-bus`；从 compress/context-store **搬移**策略，不复制；store 只实现 persistence port，compress 消费 domain decision | 防止双权威；raw CCR 继续私有独立 | CONTEXT-1..4, PR-C3* |
| D6 | **公开 prerelease**（GitHub public unsigned prerelease 聚合 workflow） | 已决策（保持现状边界） | 同一 tag/同一 commit 聚合 macOS ARM64 + Windows x64，双端门禁全绿才创建；产物标 unsigned/unnotarized；不宣称签名、公证或系统信誉；V-1 后 tag 与版本号 v0.0.1 绑定 | workflow 接线只允许 PR-7；不围绕 GitHub PR #6 云端 CI 失败修 CI | PLATFORM-3/4, PR-7, K2 |
| D7 | **no-touch 开发边界** | 已决策（保持） | 开发默认不触碰真实 Agent 配置/凭据；fixture/fake-bin/poison-bin、临时 HOME/PATH/端口为 gate；PR-13 仅在 PR-6 隔离副本验证 managed preview/apply/verify/rollback。真实用户环境写入只允许 N0 通过后的 N4 授权 lane，且每次必须有用户可读摘要/diff/backup/明确确认/verify/exact restore evidence | M1 不变量；任何未授权 managed 写穿视为返工 | INV-4, SECURITY-9, PR-6/13, N0/N4 |
| D8 | **cc-switch 只读** | 已决策（保持） | 只读 DB/WAL/SHM watcher；首次 hash-bound 授权后仅自动更新 d-token 自有 route/runtime；绝不写 cc-switch；A→B→A 由主代理 Computer 完成 | 真实 UI 切换不由实现 Agent 操控 | AGENT-5, SECURITY-7, N5 |
| D9 | **Codex 路由不替代** | 已决策 | Codex 必须独立真实 routed-flow（N2）；配置恢复/MCP/Hook 接入不得替代 | PRD P0.0.7/P0.0.10.5 | K5, AGENT-1 |
| D10 | **证据治理** | 已决策 | 过期证据隔离；全部机器证据绑定 `C_final` + source digest；聚合器 fail-closed；旧 pass artifact 不可穿透 | 2026-08-04 G7、旧 DMG、browser preview 不得绑定最终候选 | EVID-1/2/3, K6 |
| D11 | **CBM 五正式版兼容**（2026-08-09 审计修订） | 已决策 | 支持范围 = 上游最近 5 个正式发布版本 `v0.9.0`/`v0.8.1`/`0.8.0`/`0.7.0`/`0.6.1`；有官方二进制者（v0.9.0/v0.8.1）逐版本真实验证；官方无资产三版只做 source-tag 登记 + fail-closed，绝不冒充 runtime 兼容；v0.9.1-rc.1 保留为已批准 prerelease pin 但不计五正式版；缺上游资产/证据 → 该版本 blocked，release 保持 blocked | CBM 五版本审计 B1/B2/B3；OSS-5/6/7 | PR-CBM3/4, N4/N8 |
| D12 | **RTK 解封前置**（2026-08-09 审计修订） | 已决策 | 当前三平台 managed 操作全部 `ManagedRtkTransactionState::Blocked` 是「实现被关闭」而非「待验证」；先移除硬封锁（或改写为「未配置即 unavailable」），再走 C4A→C4B→C4C；解封前不得宣称 RTK production 完成 | 统一压缩审计 §2.4 | RUNTIME-10, PLATFORM-5, PR-C4U |
| D13 | **热同步授权门**（2026-08-09 审计修订） | 已决策 | 热同步是 review-gated（`automatic_apply_allowed=false`，无自动写路径）——OMP 源码审计证实存在，计划不得误报「自动跟随完全缺失」；若开放「无二次确认自动同步」，必须先改 PRD 明确授权范围与撤销语义，自动同步仍走 backup/verify/receipt/ledger + 整批回滚 | Agent 矩阵审计 §5/§6 | AGENT-11, PR-13 |
| D14 | **压缩节省 floor**（2026-08-09 审计修订） | 已决策 | 产品级固定 corpus 端到端节省 floor ≥10% 是绝对门槛，与 fresh baseline no-regression band 分离（band 只防相对退化）；低于 floor 即 blocked，不得以「无回归」通过 | 任务要求 6 | QUALITY-8, PR-C7A/C7C, N10/U5 |

## 2. 风险登记

| # | 风险 | 等级 | 缓解 | owner |
|---|---|---|---|---|
| R1 | 版本统一（V-1）被用户搁置 → 所有发布结论无法以 v0.0.1 名义落盘 | High | V-1 前置在 PR-9/N0 之前；发布门 fail-closed 检查版本 | Main |
| R2 | Windows 实机/MSVC/G6 PowerShell/签名凭据不可获得 → 只能未签名预览或 blocked | High | 显式 external 标注；fan-in/runner 证据先接线；最终三结论如实判定 | Main/外部 |
| R3 | provenance 漂移（D1–D5）未闭合 → 分发声明不可信 | High | OSS-1 MUST；漂移任务独立切片；N0 grep 断言 | PR-CBM1/H1/O1/L0/R1 |
| R4 | “代码存在/测试数量”被误读为完整统一框架 | High | 117 项机器矩阵 + production dispatch node + candidate-bound verification DAG；不存在 boundary | Main/PR-C0C/C8A |
| R5 | 并行切片触碰共享文件（dtoken-types/core-api、gate 聚合、workflows、根 NOTICE、PLAN） | High | EXECUTION §5 串行化规则；主代理合并期统一维护；git 审计账本 | Main |
| R6 | 旧证据冒充（mtime/latest/pass-only 穿透） | High | PR-7/PR-9 candidate binding；EVIDENCE.md 过期清单 | PR-7/9 |
| R7 | 多文档版本事实源继续漂移（PRD/PLAN/本包/IMPLEMENTATION_STATUS） | Medium | 文档优先级（README §4）；本包随执行滚动；收口统一更新（DOCS-3） | Main |
| R8 | 侧栏/UI 真实窗口证据不足（浏览器 preview 冒充） | Medium | N6 主代理 Computer 全组合；PR-4 Playwright 只作 layout 前置 | Main/PR-4 |
| R9 | LeanCTX/Headroom/RTK/Optical 被部分实现或 metadata 误报完成 | High | 全部 MUST；任何 no-op/preview-only/无 hook/无 exact active 仍阻断 | PR-C0..C8 |
| R10 | 配置/凭据/平台验收污染真实用户环境或产生 pre-candidate evidence | High | PR-6/8/13/CBM4 仅接 producer/schema/dry-run 与隔离 fake/poison 合同；N0 通过后 N4/N7/N8 才可逐次明确授权真实 lane，使用唯一 namespace + exact restore，失败阻断 | PR-6/8/13/CBM4, N4/N7/N8 |
| R11 | 当前分支 HEAD 仍在变动（CI 稳定化提交链） | Medium | P0 复核冻结 commit；N0 以 `C_final` 为准；证据绑定禁止漂移 | Main/P0 |

## 3. 执行期固定裁决

1. 版本：v0.0.1。
2. 完整统一框架：全部 requirement 阻断，无 BOUNDARY。
3. Optical：无 UI active 开关；exact 全门自动 active，否则同请求 PreserveText。
4. 未签名产物可用于技术验证，但不是“完成/可发布”结论。
5. 上游 pin 由对应 provenance 切片依据官方 release、许可证与行为差异做单一化；未单一化时阻断。

## 4. 决策变更协议

- 执行 Agent 不得自行改写本文件决策；变更由主代理记录并同步本文件与 [EXECUTION.md](./EXECUTION.md) §7。
- 任何用户后续裁决只允许增加约束或改变外部证据取得方式；不得把既定 MUST 降级为 BOUNDARY。
