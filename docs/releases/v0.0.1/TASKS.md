# TASKS — v0.0.1 可勾选任务清单

- 状态词：`[ ]` 待执行 / `[~]` 在途（已在分支或已开工）/ `[!]` blocked（外部依赖缺失）/ `[x]` 完成（必须可引证据，见 [EVIDENCE.md](./EVIDENCE.md)）。
- 每个任务列出：requirement 映射、依赖、owner（主代理 Main / 切片 Agent / 验证节点）、output（交付物或证据路径）。
- 本清单是 [REQUIREMENTS.md](./REQUIREMENTS.md) 的追踪层：所有 requirement 都有任务；所有任务都有验收（DoD）与验证节点（[VERIFICATION.md](./VERIFICATION.md)）。

## 1. 基线（W0）

- [~] T-P0 preflight-baseline：**执行时点用 `git status`/`git rev-parse HEAD` 实时采集基线事实并冻结**（不硬编码历史 HEAD；历史 `1ddb30e`/`f0a9bed`/`51941806`/`dc647af` 只作过程事实），裁决 untracked `code/crates/dtoken-compress/examples/_diag_tmp.rs` 与 dirty 文件，冻结 commit 后所有切片以其为起点。独立分支**尚未创建**：P0 分支为计划名，执行时由主代理新建（不指向现有分支）。REQ：EVID-7（前置）、VERSION-0（基线事实）｜owner：P0 Agent / Main｜output：`.ai-bridge/release-iteration/preflight-report.md`（更新版，2026-08-09 口径）。依赖：—。验证：N0 前置。

## 2. 版本统一（W9）

- [ ] T-V1：全部功能与证据接线切片完成后，一次性统一 manifest、PRD 版本门、PLAN/status/CHANGELOG/workflow/check script 为 v0.0.1；用户已指定目标版本，无需再次确认。REQ：VERSION-0/1, DOCS-4｜依赖：T-C8B、PR-1..14、CBM2/3/4、A｜验证：N0。
- [ ] T-V2：活跃文档旧版本残留断言（history/archive/CHANGELOG 历史除外）。REQ：VERSION-0｜依赖：T-V1｜验证：N0。

## 3. 在途收口切片（PR-1..9）

- [~] T-PR1 runtime-receipt：OptimizationReceipt 类型 + RequestLog provider usage 字段 + `mark_upstream_outcome` 投影 + claimable 规则 + 脱敏断言。REQ：PRODUCT-1, RUNTIME-1, RUNTIME-4｜依赖：T-C0B, T-C0C｜owner：PR-1 Agent｜output：切片报告 + 单测。验证：N1。
- [~] T-PR2 codex-ocxlocal-acceptance：acceptance 脚本 + 契约测试 + artifact schema（agent_id=codex）。REQ：AGENT-1, RUNTIME-6｜依赖：T-P0｜owner：PR-2 Agent｜output：`support/scripts/run_codex_ocxlocal_routed_acceptance.py` + 契约测试。验证：N2。
- [~] T-PR3 autoconfig-cutover：`needs-source-fix`/`has_resolution_gap` 全链路 + cc-switch 只读暴露 + spec v2.1/PLAN 1A.6 旧口径同步。REQ：AGENT-3, AGENT-5, SECURITY-7｜依赖：T-P0｜owner：PR-3 Agent｜output：cutover diff + grep 无残留输出。验证：N4/N5。
- [~] T-PR4 ui-polish：C1–C8 修复 + badge + 920×640 Playwright 断言 + Reduce Motion。REQ：UI-1, UI-2, UI-3, UI-5, UI-7｜依赖：T-P0｜owner：PR-4 Agent｜output：E2E 通过 + C1–C8 核对表。验证：N6。
- [ ] T-PR5 security-classifier-export：全部 production secret validation path 复用共享 SecretClassifier + export 绝对路径泄漏修复 + 回归。REQ：SECURITY-1, SECURITY-2, SECURITY-3｜依赖：T-PR1｜owner：PR-5 Agent｜output：classifier 覆盖矩阵 + 回归测试。验证：N0/N6。
- [~] T-PR6 config-acceptance：隔离临时根 harness（configure→reread→clear→exact restore；原文件仅 hash/权限；逆序回退）。REQ：AGENT-6, CONFIG-1, CONFIG-2, CONFIG-3｜依赖：T-P0｜owner：PR-6 Agent｜output：`support/scripts/run_agent_config_acceptance.py` + 契约测试。验证：N4。
- [ ] T-PR7 cc-switch-aba + gate wiring：A→B→A artifact harness + 聚合器 `--expected-commit`/`--expected-source-digest` 强绑定 + supplemental evidence + fan-in 接线。REQ：AGENT-5, EVID-1, EVID-3, EVID-6, PLATFORM-3｜依赖：T-PR2, T-PR3, T-PR6｜owner：PR-7 Agent｜output：`run_cc_switch_aba_check.py` + 聚合扩展测试。验证：N5/N9。
- [ ] T-PR8 upgrade-autostart-credential-wiring：完成 macOS upgrade/rollback、LaunchAgent/HKCU、Keychain/Credential Manager producer/schema/dry-run、唯一 namespace、fake/poison fixture 与 cleanup/exact-restore 故障合同；**N0 前不执行任何真实 OS-managed store round-trip**。REQ：PLATFORM-1, PLATFORM-2, CONFIG-4｜依赖：T-PR7｜owner：PR-8 Agent｜output：两个新脚本 + 契约测试。验证：N7/N8 在 N0 后逐次明确授权执行真实 lane。
- [ ] T-PR9 verification-orchestration：编排 N0→N1–N8/N10→N9，候选变化使旧证据失效；三结论模板。REQ：EVID-2/3, PLATFORM-4, DOCS-3｜依赖：T-V1, T-C8B｜验证：dry-run + N0–N10。
- [ ] T-PR10 codex-toml-top-level：`candidate_from_codex_toml` 解析顶层 `openai_base_url`/`model_catalog_json`（无 `[model_providers]` 形态）；两种形态 fixture 单测。REQ：AGENT-8｜依赖：T-P0,T-PR3｜owner：PR-10 Agent｜output：parser 扩展 + fixture 单测。验证：N2。
- [ ] T-PR11 cc-switch-openclaw：discovery SQL/`agent_ids_for_cc_switch_app` 补 openclaw app type + A→B→A 连续识别 fixture 单测 + 只读断言；**cc-switch sync 与 active route 最终比对（`derive_cc_switch_status`/`enrich_upstream_discovery`）下沉 Rust-owned 合同（`cc_switch_sync_state` 补 enriched 状态/`route_snapshot_identity`，agent 矩阵 H4）**。REQ：AGENT-9（含 H4）｜依赖：T-P0,T-PR3,T-PR10｜owner：PR-11 Agent｜output：SQL/映射扩展 + Rust 合同 + 单测。验证：N5。
- [ ] T-PR12 native-responses-single-dispatch：Native Responses（`/v1/responses`）接入 canonical pipeline，parity fixture 含 Responses 协议；发送体=验证体。REQ：AGENT-10, RUNTIME-7｜依赖：T-C1C｜owner：PR-12 Agent｜output：single-dispatch 覆盖 + parity fixture。验证：N10/U0。
- [ ] T-PR13 hot-sync-auth-revocation：review-gated 热同步合同与只读观察 harness（`automatic_apply_allowed=false`）；若开放「无二次确认自动同步」，PRD 授权范围先行 + 撤销门 + backup/verify/receipt/ledger + 整批回滚；**dtoken-agent-bridge 只读身份检查（component/protocol/OS/arch/version + 签名/Team ID/Authenticode）通过后才可在授权验证 lane 开放 MCP/Hook desktop-managed 写面**。6 Agent managed 写面在本切片只使用 PR-6 隔离临时 HOME/配置副本执行 preview→apply→verify→rollback；真实用户环境只保留给 N4 且须 N0 通过与逐次明确授权。REQ：AGENT-11, AGENT-7（含 H3）｜依赖：T-P0, T-PR3, T-PR6｜owner：PR-13 Agent｜output：只读观察 receipt + PRD 授权门 + bridge 身份证据 + 隔离副本合同测试。验证：N4/N5。
- [ ] T-PR14 onboarding-5seg：STEP_KEYS 收敛为 5（welcome/service/agents/config/verify），删除独立 upstream 步骤（信息并入 agents/config）；service 步骤阶段标签 + indeterminate 呼吸动画 + Reduce Motion 静态等价；wizard 顺序门；八页可见文案入 DICT + OMP DS copy gate 断言；**service 运行中≠connected 文案诚实（H-1：service healthy 徽标不得误读为已连接，成功文案「服务已启动，尚未完成 Agent 接入」）+ Install.tsx `window.confirm`→统一 OperationDialog（M-3）**。REQ：PRODUCT-5, UI-8/9/10/11（含 H-1/M-3）｜依赖：T-P0,T-PR4｜owner：PR-14 Agent｜output：Onboarding STEP_KEYS=5 + StepProgress 接入 + 顺序门 + copyPolicy 断言 + confirm→OperationDialog。验证：N6。

## 4. Provenance / 迁移差异（W1）

- [x] Headroom 来源已统一为 `v0.34.0` / `9fd5ae3d`；旧基线与 main 只保留为历史和发布后研究参考。验证：N0。
- [x] LeanCTX 来源已统一为 `v3.9.17` / `cf8fce6b`；main 增量已评估。验证：N0。
- [x] RTK 来源已统一为 `v0.45.0` / `b34be37`，许可证与无 NOTICE 事实已登记。验证：N0/N10。
- [x] pxpipe 来源已统一为 `v0.12.0` / `0ceabbed`，差异、许可证及非生产历史 profile 已登记。验证：N0/N10。
- [ ] T-O2：Spleen 2.2.0 provenance 与字形/渲染差异。REQ：OPTICAL-3/4, OSS-2｜依赖：T-O1｜验证：N10。
- [ ] T-CBM1：根 NOTICE 与 v0.9.1-rc.1 manifest 单一化。REQ：OSS-1/4｜验证：N0。
- [ ] T-CBM2：v0.9.0 配置 Bug 先红后绿；Windows SHA/Cosign/thumbprint producer/schema/dry-run；仅在 PR-6 临时 Agent 配置副本验证 preview/apply/verify/rollback 与 exact restore。REQ：PRODUCT-6, OSS-4, CONFIG-1/2｜依赖：T-CBM1,T-PR6｜验证：N4/N8 在 N0 后执行真实 Windows/candidate lane。**N0 前不得生成真实 Windows 或 candidate evidence。**
- [ ] T-CBM3：CBM 多版本 manifest schema v2（五正式版 v0.9.0/v0.8.1/0.8.0/0.7.0/0.6.1 + 批准 prerelease pin v0.9.1-rc.1 并列标注）；每版本 tag+commit+平台 archive/binary SHA+协议适配器+平台矩阵（v0.8.1 无 windows-arm64 → 显式 incompatible）；身份只用 tag/commit/hash，不读 server.json version；构建期对 v2 manifest 摘要并加完整性测试；**promo 生成脚本（`generate-promo-v2.mjs`）改由 v2 manifest 驱动，消除硬编码版本字面量（CBM 审计 M1）**。REQ：OSS-5/7（含 M1）｜依赖：T-CBM1｜验证：N4/N8。**缺上游资产/证据的版本保持 blocked，release 不得放行。**
- [ ] T-CBM4：逐版本验证 harness producer/schema/dry-run（临时工程 + repo zero-write + persistence=false + endpoint allowlist + fake/poison fixture）；v0.9.0/v0.8.1 官方二进制**仅在 N0 后 N4/N8**执行 indexing/graph query/call chain/dependency/impact 并落 C_final evidence；0.8.0/0.7.0/0.6.1 只做 source-tag 登记 + fail-closed（incompatible/untrusted），UI 不开放配置/索引/图谱。REQ：OSS-6, PRODUCT-6｜依赖：T-CBM3,T-PR6｜验证：N4/N8。**N0 前不执行官方二进制；缺上游资产/证据保持 blocked。**
- [ ] T-A1：AiMaMi pin/LICENSE/适配范围/无品牌资产复核。REQ：OSS-1/2｜验证：N0。

## 5. Unified Compression C0 — 合同与 corpus（W2–W3）

- [ ] T-C0A schemas-policy：冻结 canonical IR、NodeAction、manifest、RouteDecision digest、全局 safety policy 与四维 accounting。REQ：PRODUCT-8,RUNTIME-7/9｜依赖：provenance pins｜验证：N10 ContractTest。
- [ ] T-C0B sealed-receipts：Validation/Recovery/SendCommit receipts 不可伪造；compile-fail + forged negatives。REQ：RUNTIME-8,CONTEXT-2,QUALITY-7｜依赖：T-C0A｜验证：N10 Property/Fault。
- [ ] T-C0C registry-corpus：创建恰 117 项机器 registry、固定 benchmark/replay/fuzz/fault/eval corpus manifest。REQ：QUALITY-1/3｜依赖：T-C0A,H1/L0/R1/O1｜验证：N10 RegistryTest。
- [ ] T-C0D registry-117-start：按 C0C 合同创建 117 项 registry 内容，用「9 codec 真实项（`production_registry_codec_ids`）+ 12 聚合 ID 映射」起步，再逐领域扩充；每项 `id/source_capability/owner_crate/dispatch_symbol/tests/real_evidence/gates/status` 必填，status 在 N0 前只允许 `implemented_unverified/blocked`；C2 等切片以 registry 条目为 DoD，C8A 只校验路径，N10/U9 才在 N0 后密封 117/117 verified。REQ：QUALITY-1｜依赖：T-C0C｜验证：N10 RegistryTest/U9。

## 6. C1 — 单一 production pipeline（W4）

- [ ] T-C1A route-lowering：RouteDecision 唯一权威；三协议 parse/lower once。REQ：RUNTIME-7,QUALITY-6｜依赖：T-C0B,T-C0C｜验证：N10 Contract/Parity。
- [ ] T-C3A context-bus-core：新 `dtoken-context-bus`，搬移 Session Cache/map/signature/self-hit policy，store 仅 persistence。REQ：CONTEXT-1｜依赖：T-C1A,T-C0A,T-L0｜验证：N10 ContextBus。
- [ ] T-C1B reuse-before-plan：删除 post-validation memory injection；Reuse 在 transform planning 前入 IR。REQ：CONTEXT-4,RUNTIME-7｜依赖：T-C1A,T-C3A｜验证：send digest=validation digest。
- [ ] T-C1C single-dispatch：HTTP/Rust/MCP/TS/Proxy 全面切到 resolved pipeline；删 no-op/第二 fallback path。REQ：RUNTIME-7/8｜依赖：T-C0B,T-C1A,T-C1B,T-PR1｜验证：N10 SurfaceParity。

## 7. C2/C3/C4/C5 stage 内核（W5–W7）

- [ ] T-C2A headroom-codecs：九类 codecs/mixed router/exclusive action/property+fuzz 完整。REQ：QUALITY-2,QUALITY-1 117 core｜依赖：T-C1C,T-C0D,T-H1｜验证：N10 Codec/Property。
- [ ] T-C2B cache-live-zone：stable prefix/live-zone/cache identity/TTL/strategy；stale/mismatch miss。REQ：117 CacheAligner｜依赖：T-C2A｜验证：N10 Cache。
- [ ] T-C2C ccr-memory-learning：CCR tool/retrieve/crash、memory/learning/output/holdout 残余矩阵。REQ：CONTEXT-2,QUALITY-1｜依赖：T-C2B,T-C0B,T-C0D｜验证：N1/N10/N8。
- [ ] T-C3B relay-retrieval：Context Relay/query-aware retrieval/跨 Agent 两实例证据/独立 accounting。REQ：CONTEXT-1/3｜依赖：T-C3A,T-C1C｜验证：N1/N10。
- [ ] T-C3C recovery-shared：Recovery Pointer/Manifest、Stale/Superseded、SharedContext grant/revoke。REQ：CONTEXT-1/4｜依赖：T-C3B,T-C2C｜验证：N10。
- [ ] T-C4A rtk-filters：git/cargo/npm/pytest/json classification + TOML DSL；secret/error/unknown/negative Preserve。REQ：RUNTIME-7,PLATFORM-5｜依赖：T-C0A,T-R1,T-C4U｜验证：N10 RTK property/fuzz。
- [ ] T-C4U rtk-unblock：移除三平台 `ManagedRtkTransactionState::Blocked` 硬封锁（改为「未配置即 unavailable」语义）；先红后绿；未解封前不得宣称 RTK production 完成。REQ：RUNTIME-10,PLATFORM-5｜依赖：T-R1,T-C0A｜验证：N7/N8/N10。
- [ ] T-C4B rtk-hook-tee：真实支持 Agent hook 指向 bridge；tee private；exit/timeout/truncation；三平台。REQ：PLATFORM-5,CONFIG-1｜依赖：T-C4A,T-PR6｜验证：N4/N8/N10。
- [ ] T-C4C rtk-planner-accounting：RtkCommandOutput production dispatch、卸载、四维 bucket、双计断言。REQ：RUNTIME-5/7/9,CONTEXT-3｜依赖：T-C4B,T-C1C｜验证：N1/N10。
- [ ] T-C5A optical-renderer：pxpipe/Spleen 单 pin、Unicode/CJK fallback、跨平台 deterministic/self-decode/resource limits。REQ：OPTICAL-2/3/4｜依赖：T-O1,T-O2,T-C0C｜验证：N8/N10。
- [ ] T-C5B optical-history-cache：history collapse/static slab/freeze grid/cache anchors/billing boundary/pin semantics。REQ：OPTICAL-6｜依赖：T-C2B,T-C5A｜验证：N10。
- [ ] T-C5C optical-attestation：exact Ed25519 capability + economics + paired eval digests/lifecycle。REQ：OPTICAL-5/6｜依赖：T-C5B,T-C7B｜验证：N10。
- [ ] T-C5D optical-active：至少一个 exact Anthropic route 自动 active；其他 route PreserveText；无 UI switch。REQ：OPTICAL-1/2/5｜依赖：T-C5C,T-C6C｜验证：N10 exact E2E。

## 8. C6 — 并发、持久化、恢复（W6）

- [ ] T-C6A：request-wide deadline/byte permit、有界 worker/queue、stable order、饱和 Preserve。REQ：QUALITY-5/6｜依赖：T-C2C,T-C3C,T-C4C,T-C5B｜验证：N10 Perf/Fault。
- [ ] T-C6B：CCR FULL durability、可重建 projection、JSONL/SQLite/private store failpoints。REQ：CONTEXT-2,QUALITY-5｜依赖：T-C6A｜验证：N10 Fault。
- [ ] T-C6C：pre-commit baseline once；post-commit no retry/provider switch；唯一 terminal receipt。REQ：RUNTIME-8,QUALITY-2/5/7｜依赖：T-C6B｜验证：N10 Fault/Replay。

## 9. C7 — 质量/性能优化（W7）

- [ ] T-C7A fresh-baseline：resolved path fixed corpus 的 p50/p95/p99/CPU/throughput/alloc/RSS/queue/cancel/benefit/fallback；冻结 noise/no-regression band；**并采集 product-level corpus 端到端节省 baseline（QUALITY-8 floor ≥10% 的门槛基准，与 no-regression band 分离）**。REQ：QUALITY-3/6/8｜依赖：T-C1C,T-C6C,T-C0C｜验证：N10 Perf。
- [ ] T-C7B replay-quality-eval：decision/body replay、coding-task same-model paired outcome、Optical fidelity/cost/latency、holdout。REQ：QUALITY-4,OPTICAL-5｜依赖：T-C7A｜说明：不回指 Optical attestation；attestation 切片在本任务后消费 eval digests｜验证：N3/N10。
- [ ] T-C7C optimize-SLO：parse/digest/token once、lazy candidate/Cow、去整树 clone；只提升超噪声且质量全绿者；**端到端节省 floor ≥10% 断言（product-level corpus，QUALITY-8）；低于 floor 不得以「无回归」通过**。REQ：QUALITY-3/6/8｜依赖：T-C7A,T-C7B｜验证：N10 Perf/Quality。

## 10. C8 — 117 proof 与候选接线（W8–W9）

- [ ] T-C8A capability-proof-wiring：校验 117 registry 的 owner/dispatch/test/evidence producer/gate 路径全部可解析；N0 前状态只允许 `implemented_unverified`/`blocked`，不得生成 117/117 real-evidence 结论。REQ：QUALITY-1,EVID-6｜依赖：T-C2C,T-C3C,T-C4C,T-C5D,T-C6C,T-C7C｜验证：N10/U1/U9 在 N0 后消费真实 artifact。
- [ ] T-C8B real-routes-platform-wiring：完成 Provider/Agent/Codex/OpenCode/Optical/macOS/Windows evidence producer、schema、dry-run 与 no-touch fixture；**不在 N0 前执行 N2/N3/N8 或任何真实外部副作用 lane**。REQ：AGENT-1/2,PLATFORM-1..5｜依赖：T-C8A,T-PR2,T-PR6,T-PR7,T-PR8,T-PR10,T-PR11,T-PR12,T-PR13,T-PR14,T-CBM3,T-CBM4｜验证：N2/N3/N7/N8/N10 在 N0 后运行。
- [ ] T-C8C final-gate：聚合器强绑定 candidate/source/corpus/registry/eval/platform；N10 进入 G8/G9 required evidence；N0 前 real-evidence artifact 一律拒绝。REQ：EVID-1/2/3/6｜依赖：T-V1,T-PR9,T-C8B｜验证：N9。

## 11. 真实验证节点（W10）

- [ ] T-N0：冻结 `C_final`，structure/version/full Rust+frontend/no-touch/security/OSS preflight。任一修复使受影响证据失效。
- [ ] T-N1：真实 Receipt、四维 accounting、CCR/Relay/cache/RTK 投影、UI/export 脱敏。
- [ ] T-N2：Codex `18001→10100→dynamic model` 真实 routed。
- [ ] T-N3：OpenCode 两条长工程验收（OpenCode Free `opencode/deepseek-v4-flash-free` 与 Go dsv4flash 工程链）：真实 routed path + 长时/长上下文工程 workload + C_final/source digest + same-model baseline/optimized paired + exact restore；旧 bounded replay 不替代。
- [ ] T-N4：多 Agent 配置副本/apply/reread/clear/exact restore + hook/tee。
- [ ] T-N5：主代理 Computer 执行 cc-switch A→B→A，只读且 route identity 恢复。
- [ ] T-N6：真实 Tauri 八页/双尺寸/双主题/双语/Reduce Motion；Optical UI 无 active switch且状态可解释。
- [ ] T-N7：macOS install/upgrade/uninstall/rollback/autostart/credential/RTK。
- [ ] T-N8：Windows NSIS/DACL/MSVC/PowerShell/CBM/RTK + 双平台 fan-in。
- [ ] T-N10：统一压缩 proof：Contract→Registry→Property/Fuzz→Fault→Replay→Perf→Provider Paired→Optical Exact Active→Platform，全部同 C_final/source_digest/corpus revisions。
- [ ] T-N9：仅 N1–N8/N10 全结算后聚合；任一 MUST 缺失则 blocked。

## 12. 收口与完整性

- [ ] T-DOC：N9 后同步 PRD/PLAN/IMPLEMENTATION_STATUS/README/CHANGELOG/发布报告/EVIDENCE。
- [ ] 每个 REQUIREMENTS owner 必须在本清单存在；117 registry 的每一项必须由 T-C8A 机器校验；任何 `boundary/deferred/not-in-scope` 状态不得通过。

