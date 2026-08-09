# REQUIREMENTS — v0.0.1 Requirement Registry

- 每个 requirement 有唯一 ID（类别-序号）；`owner` 指向 [EXECUTION.md](./EXECUTION.md) 切片或 [VERIFICATION.md](./VERIFICATION.md) 验证节点。**本表全部 requirement 都是 v0.0.1 MUST/发布阻断**；`external` 只描述证据取得位置，不降低 requirement。
- current = 2026-08-09 可引用的现有证据（引用仓库内文档/脚本）；gap = 尚缺什么；acceptance = 可独立验证的通过标准。
- 本表是 [TASKS.md](./TASKS.md) 与 [VERIFICATION.md](./VERIFICATION.md) 的追踪根源：任何 requirement 必须在 TASKS 中有任务、在 VERIFICATION 中有验证节点，缺一即计划不完整。

## 1. VERSION（版本）

| ID | 需求（来源） | current | gap | owner | acceptance | release claim |
|---|---|---|---|---|---|---|
| VERSION-0 | 目标版本 v0.0.1 一次性统一（用户已指定；统一前 manifest/PRD 为 0.2.0-beta.1） | 已统一为 v0.0.1（2026-08-09 V-1 执行：manifest、PRD 版本门、PLAN、IMPLEMENTATION_STATUS、CHANGELOG、`check_release_version.py`） | 已收敛（旧版本仅存 history/archive/CHANGELOG 历史） | V-1 | 全部功能切片后：manifest、PRD 版本门、PLAN、IMPLEMENTATION_STATUS、CHANGELOG、release workflow、`check_release_version.py` 收敛为 v0.0.1；structure 通过；旧版本只存 history/archive/CHANGELOG 历史 | MUST |
| VERSION-1 | 发布结论绑定版本、C_final 与 source/registry/corpus digests | 旧聚合器 fail-closed 但无完整 candidate binding | 统一 evidence envelope + 显式 artifact manifest | PR-C0C, PR-C8C, N9 | 任一 artifact 缺/mismatch 即 blocked；结论写版本/C_final/digests | MUST |

## 2. PRODUCT（产品功能）

| ID | 需求（来源） | current | gap | owner | acceptance | release claim |
|---|---|---|---|---|---|---|
| PRODUCT-1 | OptimizationReceipt 最小字段 + Local/Provider/claimable 分离（PRD P0.0.7、spec v2 §7.2） | 前端已声明 Provider Token；Rust `RequestLog` 无 provider input/output 字段；无 receipt 类型/投影 | PR-1 实现 | PR-1 → N1 | 新增 PRD 最小 receipt；`mark_upstream_outcome` 写 Provider actual usage；Provider usage 单独不提升 claimable；缺 usage 显示 unavailable | MUST |
| PRODUCT-2 | 首页节省 + 四区间（累计/当周/当月/当天），真实 0 与 unavailable 区分（GOAL G7） | 区间切换已实现（`code/apps/desktop/src/app/lib/overview/savings.ts` + `overview/components.tsx` 的 SavingsWindowCard；`Overview.test.tsx` 有窗口边界单测） | 最终候选统一证据重验 | N1 | 四区间数值均来自同一 runtime/Event Ledger/设置证据；无 mock 补齐 | MUST |
| PRODUCT-3 | 分析页日志子页逐请求真实字段（GOAL G4、PRD §12.5） | 日志子页已实现（`code/apps/desktop/src/app/components/pages/Analytics.tsx` 日志子页；`/analytics/requests` cursor API + Tauri 只读桥接 + 前端分页，见 `docs/product/IMPLEMENTATION_STATUS.md` 已存在的本地 API 节） | 最终候选证据 + 脱敏复核 | N1, N6 | 字段清单与 GOAL G4 一致；无 Provider usage 不显示 Provider Token；无 raw/secret | MUST |
| PRODUCT-4 | 设置页模型价格维护（GOAL G5/G6、PRD） | 价格表已实现（`docs/PLAN.md` 当前坐标 14：Settings `model-pricing` 价格表，Overview/Analytics 据此估算 input cost before/after/delta，unknown/mixed/过期 fail-closed） | 最终候选复验：编辑/保存/删除、币种/单位/来源/更新时间 | N1, N6 | 缺价格≠0；费用仅在 Token+价格证据同时存在时展示 | MUST |
| PRODUCT-5 | 首次配置入口 + 右上角重复进入；**首次流程固定 5 段 welcome/service/agents/config/verify，无独立 upstream 步骤**（GOAL G2、onboarding 审计 B-2） | onboarding 已实现（`Onboarding.tsx` STEP_KEYS 现为 6 步，含独立 `upstream` 步骤 + `lib/onboarding.ts`：首次无完成标记自动进入、右上角可重复进入、完成后回总体概览；`AppShell.onboarding.test.tsx`/`Onboarding.test.tsx` 覆盖） | STEP_KEYS 收敛为 5（welcome/service/agents/config/verify），upstream 信息并入 agents 扫描或 config 批次 diff；无 chooser 契约保持 | PR-14 → N6 | 首次无配置进入；完成后回产品；右上角可重复进入；步骤序列恰 5 段；无独立 upstream 导航步骤；无 chooser | MUST |
| PRODUCT-6 | 代码图谱（实验性）状态诚实区分 package/integration/agent_session/ui_session/index（PRD P0.0.9、1D） | 五维状态 UI 已实现；persistence=false 强制 | CBM Windows 收口 + 候选证据 | PR-CBM2, N6 | 状态不冒充；坏 hash/平台/arch fail-closed | MUST |
| PRODUCT-7 | 不新增一级导航；日志在分析页子页（GOAL G3） | 已满足 | 结构断言 | N6 | 导航八页无新增；Analytics 日志高度无 diff | MUST |
| PRODUCT-8 | 全局安全策略（非 engine chooser）：默认 `safe_lossless`；增强/有损必须显式确认；自动 stage 选择只由 canonical planner 决定（PRD P0.0.4/P0.0.9） | 当前策略与 Optical Shadow UI 分散，尚无统一 planner policy receipt | 冻结 policy enum、确认 revision、per-request policy evidence 与 Settings 单一入口 | PR-C0A, PR-C1A, N1, N6 | 无 engine chooser；未确认有损永不执行；safe 默认可回退 | MUST |

## 3. RUNTIME（运行时代理/事件）

| ID | 需求（来源） | current | gap | owner | acceptance | release claim |
|---|---|---|---|---|---|---|
| RUNTIME-1 | RequestLog Provider usage 字段 + `mark_upstream_outcome` 写 actual input/output（PRD P0.0.7） | 现有 `mark_upstream_outcome` 只写 HybridContextAccounting/Event | 字段 + 投影 | PR-1 | 单测证明写入后投影；serde 兼容旧字段 | MUST |
| RUNTIME-2 | runtime/Tauri command/Event Ledger 是唯一事实源（GOAL B1、INV-1） | 已成立 | 候选证据复核 | N1 | 页面数据无 mock/fixture 冒充 | MUST |
| RUNTIME-3 | 默认只绑定 127.0.0.1（GOAL B3、INV-2） | 已实现（`dtoken-proxy --host` 默认 `127.0.0.1`，`parse_loopback_host` 只接受 loopback） | C_final 门禁断言 | N0 | proxy loopback validation + 非回环 bind 结构断言通过 | MUST |
| RUNTIME-4 | 不复制整份 HybridContextAccounting 进 receipt（INV-7、PRD §10） | 无 receipt 类型 | PR-1 scope | PR-1 | receipt 只含 PRD 最小字段；不携带完整 accounting/raw state | MUST |
| RUNTIME-5 | 事件/Ledger/指标统一；savings 分账不双计（GOAL G7、PRD §10） | ledger 统一；RTK/Relay/cache production buckets 尚未闭合 | typed bucket + producer/consumer 契约 | PR-C0A, PR-C3B, PR-C4C, PR-C8A, N1 | compression/command-output/cache/Relay 四维独立；无直接求和；unknown≠0 | MUST |
| RUNTIME-6 | Codex 动态 model 透传、Responses 协议、无 OpenAI 认证语义（PRD P0.0.10.5） | `ProviderWireApi::OpenAiResponses`、`models=[]` 已实现 | 真实 routed acceptance | PR-2, N2 | 链路 `Codex→18001→10100→动态 model` 有 artifact | MUST |
| RUNTIME-7 | 单一 canonical Context IR 与独占 NodeAction：Preserve/Reuse/HeadroomSemantic/RtkCommandOutput/Optical；**single dispatch 必须包含 Native Responses（`/v1/responses`），不得绕过 canonical pipeline**（unified-compression 审计 §3：Native Responses 当前绕过） | 当前 resolved pipeline、Hybrid IR、Optical IR 与 RTK bridge 分散；memory 在校验后注入；no-op `compress()` 仍是生产 fallback；Native Responses 绕过 canonical IR/NodeAction | 单入口 lowering/planner/arbitration/final envelope validator；每 node 恰一动作；删双路径；Native Responses 经 `prepare_json_pipeline_payload` 接入 | PR-C0A, PR-C1A..C1C, PR-12 | HTTP/Rust/MCP/TS/Proxy 同 fixture decision/reason/artifact 字节一致；发送体=验证体；Native Responses 与 Chat 同 fixture parity | MUST |
| RUNTIME-8 | 不可伪造 type-state：artifact-derived ValidationReceipt、request/source/decision-bound RecoveryReceipt、transport-observable SendCommitReceipt；**生产 transport 必须实际消费 SendCommitReceipt（pending marker/no-op/type existence 不得冒充完成）**；**CCR 必须是真实 durable store→receipt→marker→restore 闭环** | runtime-pipeline 接受 caller boolean validator、caller digest、`ccr:` 前缀和 timestamp sentinel；无系统验证 SendCommitReceipt 被生产 transport 消费的证据 | digest 内部计算；sealed constructors；validator/commit/recovery receipt 验证图；transport 消费点与 marker 落盘/恢复证据 | PR-C0B, PR-C1C, PR-C6C | adapter 不能用任意 bytes/boolean/timestamp 铸造 Optimized/Committed；生产发送路径逐请求消费 SendCommitReceipt；CCR durable 写入→receipt→marker→restore 闭环有 crash 证据 | MUST |
| RUNTIME-9 | 四维独立 accounting 与投影：compression / command-output / provider-cache / relay；Local estimate / Provider actual / claimable 分离 | RequestLog 单 `tokens_saved`；RTK 恒 0；Analytics/Overview 存在求和路径 | typed buckets、double-count guard、旧字段迁移、UI/export 投影规则 | PR-C0A, PR-C4C, PR-C8A, N1 | 四维不可直接相加；unknown 不变 0；claimable 需 paired Provider evidence | MUST |
| RUNTIME-10 | RTK 三平台 hard-Blocked 解封前置：当前 macOS/Windows/Linux 的 Install/Configure/Verify/Rollback/Uninstall 全部 `ManagedRtkTransactionState::Blocked`（unified-compression 审计 §2.4）——这是「实现被关闭」而非「待验证」，必须先移除硬封锁（或按计划语义重写为「未配置即 unavailable」而非 Blocked）再走 C4A→C4B→C4C；解封前不得宣称 RTK production 完成 | preprocessor 测试 `managed_rtk_lifecycle_is_explicitly_blocked_on_macos_and_windows`（L1581+）：三平台全部 Blocked | 解封切片（移除 Blocked 封锁 → unavailable 语义）+ C4A/B/C 生产实现 + 三平台证据 | PR-R1, PR-C4A/B/C, N7/N8/N10 | 解封后 production dispatch 可达；未配置=unavailable（非 Blocked）；解封前任何 RTK 完成声明 blocked | MUST |

## 4. AGENT（Agent 接入/路由）

| ID | 需求（来源） | current | gap | owner | acceptance | release claim |
|---|---|---|---|---|---|---|
| AGENT-1 | Codex 真实 routed-flow（PRD P0.0.7/P0.0.10.5、验收 45） | 代码链路已实现；无 acceptance 脚本 | `run_codex_ocxlocal_routed_acceptance.py` + artifact | PR-2 → N2 | artifact 含 agent_id=codex、provider/model/protocol、route verified、匹配数、ledger/metrics、失败分类、恢复、隐私断言；不写真实 Codex 配置 | MUST |
| AGENT-2 | OpenCode 真实 routed-flow 当前候选重跑：**OpenCode Free 与 Go dsv4flash 两条长工程验收**（GOAL G7 语义、PLAN G7、N3 审计要求 7） | 2026-08-04 G7 通过（candidate_id=G7，bounded replay）；2026-08-09 `support/artifacts/opencode-real-routed/` 31 份证据最新 pass | **两条链在 `C_final` 上真实长工程验收：真实 routed path、长时/长上下文工程 workload、C_final/source digest、same-model baseline/optimized paired、exact restore；旧 bounded replay 不替代** | N3, PR-C8B | 每条链（OpenCode Free `opencode/deepseek-v4-flash-free` 与 Go `dsv4flash` 工程链）都有真实 routed 证据：请求经 d-token 全链路、长时/长上下文 workload、baseline/optimized same-model paired、C_final/source digest 绑定、原始配置字节/hash/权限精确恢复 | MUST |
| AGENT-3 | 无 chooser；`needs-source-fix`/`has_resolution_gap` 全链路（PRD P0.0.10.1–3、验收 42/43） | `needs-choice`/`selection_required` 残留于 presentation/installMachine/connectionSetup | clean cutover + 文案同步（spec v2.1 F-01、PLAN 1A.6） | PR-3 | 全仓生产无残留；cc-switch 未知 schema blocked；无 chooser 断言保留 | MUST |
| AGENT-4 | 23 Agent 检测/transport matrix 诚实（PLAN、PRD §12.4；agent-integration 审计） | 注册表 `KNOWN_AGENT_ADAPTERS` 23 条（desktop 投影断言 len==23）；**15 个 verified Route 写面（can_configure=can_clear=true）、6 个条件 MCP/Hook 写面（cursor/commandcode/claude-desktop/copilot-cli/windsurf/cortex-code，需 desktop-managed）、2 个 detection-only（vscode-copilot-chat/antigravity）**；无自身 parser 的 Agent（cursor/gemini/hermes/roo/continue/aider/openhands/pi/mistral-vibe）fail-closed | 候选复验 + 新 Agent 变化复核 + detection-only 官方依据落盘（`advanced_required_reason`） | PR-10, PR-11, N4 | 23/15/6/2 矩阵可核对；detection-only 不冒充 configured/connected；无 parser Agent 缺字段→无候选不猜 | MUST |
| AGENT-8 | Codex 顶层 `openai_base_url` + `model_catalog_json` 形态解析（agent-integration 审计 B1：本机 `~/.codex/config.toml` 正是此形态，无 `[model_providers]` 表；`candidate_from_codex_toml` 只解析 `[model_providers]`+`model_provider` → 真实 Codex 形态无法被 discovery 解析为上游候选） | 代码链路已实现（`ProviderWireApi::OpenAiResponses`、动态 model 透传）；本机 Codex 顶层形态只读核对存在 | `candidate_from_codex_toml` 增加顶层 `openai_base_url`/`model_catalog_json` 解析 + fixture 单测（两种形态） | PR-10 → N2 | 两种 Codex 配置形态（顶层 base_url + model_catalog_json / `[model_providers]`）都产生上游候选；fixture 测试覆盖；不写真实配置 | MUST |
| AGENT-9 | cc-switch openclaw app type A→B→A 连续识别（agent-integration 审计 B2：本机 cc-switch DB user_version=16 含 openclaw app type，但 discovery SQL 只查 `('claude','codex','gemini','opencode')`，`agent_ids_for_cc_switch_app` 无 openclaw 分支）；**cc-switch sync 与 active route 最终比对（`derive_cc_switch_status`/`enrich_upstream_discovery`）下沉为 Rust-owned 合同（H4）** | cc-switch 只读快照 + A→B→A 静态识别已有（claude/codex/gemini/opencode）；openclaw 不可见；sync 比对逻辑在 desktop 侧未下沉 Rust | discovery SQL/映射补 openclaw app type + fixture 连续识别单测 + 只读断言；`cc_switch_sync_state` 补 enriched 状态与 `route_snapshot_identity`（PR-11） | PR-11 → N5 | openclaw A→B→A 连续识别可见；`wrote_cc_switch=false`；未知 schema blocked；desktop 与 Rust 输出一致单测；`route_snapshot_identity`/`last_receipt` 非空 | MUST |
| AGENT-10 | Native Responses（`/v1/responses` 原生端点）不得绕过 canonical pipeline（unified-compression 审计 §3：Native Responses 当前绕过 canonical IR/NodeAction，与 OpenAI Chat 三协议分散） | Native Responses 端点存在但未进 single dispatch | 纳入 single-dispatch：`prepare_json_pipeline_payload` 覆盖 Native Responses handler；same fixture decision/reason/artifact parity | PR-C1C, PR-12, N10/U0 | Native Responses 请求经 canonical pipeline；HTTP parity fixture 含 Responses 协议；发送体=验证体 | MUST |
| AGENT-11 | 热同步 review-gated 事实与「无二次确认自动同步」授权撤销门（agent-integration 审计 §5/§6：watcher/coordinator 只产生 `external_config_review_required`，`automatic_apply_allowed=false`，**当前无任何自动写路径**；「后续自动同步无二次确认」是缺失的新增产品授权，须先改 PRD 再实现，且 ReauthorizationRequired 永不自动、每次自动同步仍走 backup/verify/receipt/ledger、提供一键撤销与整批回滚） | 热同步检测已实现（notify 非递归 + 10s polling + 750ms debounce → review_required；cc-switch follow：Unchanged/SyncRequired/ReauthorizationRequired 全回到 planned 需用户确认）；**不得误报「自动跟随完全缺失」** | 真实证据落证（外部配置变更→review_required→plan/confirm/apply 闭环）+ 授权撤销门切片（PRD 授权范围先行） | PR-13, N4/N5 | 证据证明 review-gated 热同步存在且无自动写；若实现 `automatic_apply_allowed=true`，PRD 已明确授权范围与撤销语义，自动同步仍走 backup/verify/receipt/ledger + 整批回滚 | MUST |
| AGENT-5 | cc-switch 只读 + A→B→A（GOAL G12、PRD 44） | `read_cc_switch_snapshot` 只读、未知 schema blocked；2026-07-31 静态 A→B→A 已识别 | 结构化 artifact harness + 真实 UI 切换 | PR-3（只读暴露）, PR-7（harness）→ N5 | artifact：route identity 前后相同、`wrote_cc_switch=false`、ledger events；真实切换由主代理 Computer 执行 | MUST |
| AGENT-6 | 配置复制/应用/复读/恢复验收方法（GOAL G12） | ConfigPatcher preview/apply/verify/rollback 已存在 | 隔离临时根 harness | PR-6 → N4 | 副本上 configure→reread→clear→exact restore；原文件仅 hash/权限证据 | MUST |
| AGENT-7 | MCP/Hook 诚实区分（PRD P0.0.10.6、验收 46）；**dtoken-agent-bridge 存在性与身份验证（component/protocol/OS/arch/version + 签名/Team ID/Authenticode）通过后才开放 MCP/Hook desktop-managed 写面（agent 矩阵 H3）** | Install MCP/Hook 分支已接线；bridge 身份验证尚未落证 | 候选复验 + bridge 身份验证证据（PR-13） | PR-13, N4 | MCP/Hook 可用不显示为 Active route；bridge component/protocol/OS/arch/version + 签名验证通过后才开放 managed 写面；本机 6 Agent（cursor/commandcode/claude-desktop/copilot-cli/windsurf/cortex-code）managed 写面真实 preview→apply→verify→rollback | MUST |

## 5. CONFIG（配置事务）

| ID | 需求（来源） | current | gap | owner | acceptance | release claim |
|---|---|---|---|---|---|---|
| CONFIG-1 | 配置写入事务 detect→parse→plan→redacted diff→backup→confirm→apply→reread→verify→receipt；失败精确回滚（GOAL B5、INV-4） | ConfigPatcher/InstallationTransaction 已存在；Claude/Codex/Cline 真实临时事务已精确恢复 | 候选全链路复验 + 故障注入 | PR-6, N4 | 每个失败点可恢复原文件；receipt 脱敏 | MUST |
| CONFIG-2 | 原文件只取 hash/权限证据且不输出内容；不读 auth/密钥（GOAL G9/G12） | 2026-07-31 恢复 SHA-256 与 0600 权限 | harness 断言 | PR-6 | 断言脚本证明不读 auth、不输出内容 | MUST |
| CONFIG-3 | whole-batch reverse rollback（PRD P0.0.10.4、验收 44） | 批次事务设计已存在 | 多 Agent 批次失败注入测试 | PR-6 | 批次内任一失败 → 逆序回退已写入目标 | MUST |
| CONFIG-4 | 凭据只进 OS 凭据库；配置只保存不透明引用（INV-3、GOAL B4） | Keychain/Credential Manager 实现存在 | 候选 round-trip 证据 | PR-8, N7 | set/get/delete round-trip 唯一隔离 entry；无明文 fallback | MUST |

## 6. CONTEXT（LeanCTX / Context Bus / CCR）

| ID | 需求（来源） | current | gap | owner | acceptance | release claim |
|---|---|---|---|---|---|---|
| CONTEXT-1 | LeanCTX 九边界 Rust-owned domain：Context Bus / Session Cache / Context Relay / Map-signature reuse / Query-aware Retrieval / Recovery Pointer-Manifest / Stale-Superseded / SharedContext authorization / self-hit-size boundary | SharedContext typed store 主体存在；其余多数 missing/partial；策略横跨 compress/context-store | 新 `dtoken-context-bus`，搬移而非复制 ProviderCacheEvidence policy 与 memory capability state machine；完整九边界 | PR-C3A..C3C | 两 Agent 同项目命中；跨项目污染=0；self-hit=0；文件/TTL/权限/来源变化拒绝；失败正常读取 | MUST |
| CONTEXT-2 | CCR/private recovery：AES-256-GCM、TTL/quota、ACL/DACL、request/source/decision binding、journal、crash recovery、精确 retrieve/delete | 核心实现存在；Receipt 只校验前缀；跨 JSONL/密文/Event/SQLite fault evidence、tool retrieve 与 Windows MSVC 证据缺 | 强 Receipt、跨存储 crash points、自动 tool recovery、双平台 durability | PR-C0B, PR-C2C, PR-C6B/C6C, N1/N8 | 每个 pre-send fault 原文只发送一次；post-commit 无 retry；过期/错 scope 拒绝 | MUST |
| CONTEXT-3 | Relay/cache 命中与压缩、RTK 分账隔离 | 无 relay avoided-retransmission production bucket | RelayHitReceipt + CacheReuseReceipt + double-count guard | PR-C0A, PR-C3B, PR-C4C, N1 | Relay/Cache 不计入 compression savings；四维 UI/export 独立 | MUST |
| CONTEXT-4 | Reuse/memory 必须在 transform plan 前进入 IR，final send envelope 全量复验 | core-api 在 compression/pipeline seal 后 `inject_memory_into_body` | 删除 post-validation injection；Reuse NodeAction 与 provenance/permission/freshness proof | PR-C1B, PR-C3C | 发送 bytes digest 等于 ValidationReceipt final envelope digest | MUST |

## 7. OPTICAL（视觉压缩）

| ID | 需求（来源） | current | gap | owner | acceptance | release claim |
|---|---|---|---|---|---|---|
| OPTICAL-1 | Optical 是统一 planner 的自动 stage；产品 UI 无 active switch；门不全请求级 PreserveText | Anthropic-only renderer/patcher/post-validator/resource path 真实存在；config normalize、economics None、registry 空使生产 active 不可达 | exact attestation 自动门与生产 route 接线 | PR-C5C/C5D, N10 | exact supported route 同候选 active；门缺失/过期/unknown 同请求 passthrough | MUST |
| OPTICAL-2 | 仅代码/上下文→PNG；普通图片处理不在范围；OpenAI/Responses/跨协议 active 在各自 contract 未完成前保持 shadow | 范围合同存在 | negative surface assertion | PR-C5A, N10 | 无普通图片入口；unsupported protocol 永不 active | MUST |
| OPTICAL-3 | pxpipe v0.12.0/`0ceabbed` 与 Spleen 2.2.0 唯一 pin、许可证/NOTICE/行为差异 | pxpipe 已统一到 v0.12.0/`0ceabbed`，旧 pin 仅作历史；Spleen 保持 2.2.0/`0493c34` | renderer 与双平台证据仍由后续实现和 N10 提供 | PR-O1/O2, PR-C5A | pin 唯一、分发声明完整、不引入运行依赖 | MUST |
| OPTICAL-4 | 确定性跨平台 renderer：Spleen + Unicode/CJK fallback、profile/font/render digests、self-decode、视觉增强 | Spleen ASCII renderer 真实；Unicode/CJK 与双平台同 bytes 缺 | 逐 glyph 校验、macOS/Windows PNG SHA 对比、fallback/role/grid/dilate/marker 评估 | PR-C5A, N10 | 同 fixture 双平台 PNG digest 一致；secret/exact 不入图 | MUST |
| OPTICAL-5 | active capability/economics/eval：exact route/provider/model/API revision；Ed25519；renderer/font/profile/corpus/tokenizer/pricing/cache digests；paired fidelity/cost/latency | capability verifier 已实现；生产 economics None；registry/evals 空；V3 dense hex 0/15 | lifecycle authority、attested economics、固定 holdout paired eval、可信 snapshot | PR-C5C, PR-C7B, N10 | eval 失败/过期/wildcard/unknown 不能 active；至少一 exact route 全绿 | MUST |
| OPTICAL-6 | history/static slab/cache integration：history collapse、system+tools+CLAUDE.md slab、byte-stable freeze grid、cache anchors/billing header boundary、pin semantics、provider counterfactual usage、profitability calibration | d-token 仅 closed historical tool_result；active patch 无 cache anchor；LocalEstimate only | d-token-owned IR 设计与安全实现；不得复制上游源码 | PR-C5B/C5C, PR-C7B | frozen prefix byte-identical；current/recent/open/exact protected；paired usage 可归因 | MUST |

## 8. OSS（开源轨道与 provenance）

| ID | 需求（来源） | current | gap | owner | acceptance | release claim |
|---|---|---|---|---|---|---|
| OSS-1 | 每个外部来源 pin 唯一且无漂移（GOAL G8/G9、[OSS_MIGRATION.md](./OSS_MIGRATION.md)） | Headroom、LeanCTX、RTK、pxpipe、AiMaMi 已完成唯一 pin 决议与 NOTICE/license 登记；Codebase Memory 根 NOTICE 已统一到批准的 `v0.9.1-rc.1` pin | Codebase Memory 多版本 manifest 与候选 N0 provenance artifact 仍需完成 | PR-CBM3, N0 | 全轨道 pin 唯一；漂移清单清零；结构检查通过 | MUST |
| OSS-5 | CBM 支持上游最近 5 个正式发布版本（GOAL G9 扩展、CBM 五版本审计 B1）：`v0.9.0`/`v0.8.1`/`0.8.0`/`0.7.0`/`0.6.1`（按 published_at 倒序、非 prerelease、immutable）；当前 pin `v0.9.1-rc.1` 是用户已批准的 prerelease，**保留但不计入五正式版**；**promo/文案版本由 v2 manifest 驱动（CBM 五版本审计 M1：`generate-promo-v2.mjs` L99/L175 硬编码 v0.9.1-rc.1）** | 现状支持数 = 1：`artifact-manifest.json`（schema v1 单版本）、`lib.rs ARTIFACT_SPECS`、`codebase_memory_control.rs UPSTREAM_RELEASE_URL` 均为 v0.9.1-rc.1 单 pin；promo 硬编码版本字面量 | 多版本 manifest schema v2（逐版本 tag+commit+平台 archive/binary SHA+协议适配器+平台矩阵）；UI 对 v0.9.1-rc.1 持续标注 pre-release；promo 生成改读 v2 manifest（PR-CBM3） | PR-CBM3, N4/N8 | v2 manifest 表达五正式版 + 批准 prerelease pin；manifest 可寻址 5 official + 1 prerelease（**不写「支持数=5+1」：无资产三版不能算 runtime supported**）；`v0.9.1-rc.1` 明确标注 prerelease 且不与正式五版本并列计数；runtime-verified 仅资产可得版本（v0.9.0/v0.8.1），0.8.0/0.7.0/0.6.1 仅 source-tag 登记 + fail-closed/blocked；promo 无硬编码版本字面量 | MUST |
| OSS-6 | CBM 逐版本真实验证按官方资产可得性 fail-closed（CBM 五版本审计 B2）：有官方二进制（v0.9.0/v0.8.1/v0.9.1-rc.1）逐版本执行 indexing/graph query/call chain/dependency/impact 真实验证；官方无资产版本（0.8.0/0.7.0/0.6.1，release 已归档 assets=0）只做 source-tag 级登记（tag+commit+官方归档链接）并 fail-closed（incompatible/untrusted），**绝不冒充 runtime 兼容**；上游资产缺失时对应版本保持 blocked | GitHub Releases API 2026-08-09 快照：v0.9.0（35 资产）、v0.8.1（31 资产、无 windows-arm64）、0.8.0/0.7.0/0.6.1（assets=0，已归档）；本地 clone 无 tag，供应链锁定只能来自 GitHub API/raw | 逐版本 acceptance harness（临时工程 + repo zero-write + persistence=false + endpoint allowlist + macOS/Windows 证据）；无资产版本登记为 source-tag 兼容状态 | PR-CBM3, PR-CBM4, N4/N8 | v0.9.0/v0.8.1 逐版本真实验证 artifact 同 C_final；0.8.0/0.7.0/0.6.1 只以 source-tag 登记 + fail-closed 状态呈现，UI 不提供配置/索引/图谱能力；任一版本证据缺 → 该版本 blocked，release 保持 blocked | MUST |
| OSS-7 | CBM 版本身份只来自 release tag + commit + 资产 hash，不用 server.json 的 version 字段（CBM 五版本审计 H3）；逐版本平台矩阵（v0.8.1 无 windows-arm64）与协议/config 表面按版本参数化（H1/H2） | raw server.json：v0.9.1-rc.1/v0.9.0 的 version 字段仍写 0.8.1、v0.8.1 写 0.8.0（上游元数据滞后）；`artifact_spec_for_current_platform` 无版本维度；`proxy.rs base_command` 固定 CBM_* 环境集 | v2 manifest 以 tag/commit/hash 为身份源；逐版本 `tools/list` 实测后才登记工具面；平台不支持 → incompatible 显式表达 | PR-CBM3, N4/N8 | 任何版本判定不读 server.json version；v0.8.1 在 windows-arm64 上显式 incompatible；每版本工具/config 表面有实测证据 | MUST |
| OSS-2 | NOTICE/许可证副本随分发（GOAL G8、AGENTS.md） | root + docs/legal 双份 NOTICE；AiMaMi LICENSE 副本在仓 | 随包分发清单复验 | PR-A, N0 | SBOM/license 门通过且包含全部轨道声明 | MUST |
| OSS-3 | 外部仓库只读研究；不复制进生产；不新增运行依赖（GOAL G8、INV-4） | 本地副本全部在 `/Users/cc/Documents/d-token-external/`（仓库外） | 依赖扫描断言 | N0 | Cargo/npm 依赖无外部轨道运行时依赖；无复制源码 | MUST |
| OSS-4 | CBM v0.9.1-rc.1 固定 tag/commit + 平台 archive/binary SHA + Sigstore/Cosign；不得放宽校验（GOAL G9） | manifest 已 pin `ee9833df` + SHA；Cosign blob bundle 校验流程已定义 | 根 NOTICE 漂移修复 + Windows 收口实机 | PR-CBM1, PR-CBM2 | 根/内 notice 一致；四平台（macOS/Windows 各目标）archive+binary SHA 与 Cosign 校验证据 | MUST |

## 9. UI（界面/侧栏/可访问性）

| ID | 需求（来源） | current | gap | owner | acceptance | release claim |
|---|---|---|---|---|---|---|
| UI-1 | 顶部重复标题/留白修复（audit C1/C2） | Topbar 上下文标签与展开侧栏重复；Overview ~44pt 留白 | PR-4 | PR-4 | 折叠态显示上下文标签、展开态隐藏；留白回收；sr-only H1 保留 | MUST |
| UI-2 | 术语统一 + i18n key（audit C3/C4） | 活动/活跃术语不一；Changes/Doctor 硬编码 | PR-4 | PR-4 | Heatmap 术语统一；Changes/Doctor 用 i18n key | MUST |
| UI-3 | 920×640 无横滚 + Reduce Motion chevron（audit C6） | StatusBar `flex-nowrap+overflow-x-auto` | PR-4 | PR-4 → N6 | Playwright 真实 layout engine 断言 `scrollWidth <= clientWidth`；Reduce Motion 下无 transform | MUST |
| UI-4 | 侧栏专项（GOAL 侧栏 S1–S4） | 侧栏修复与小猫图标已落地（`docs/PLAN.md` 当前坐标 6：侧栏图标 enter/leave 可逆动效与点击 scale 按 PRD §12.5.7 落地；`docs/product/IMPLEMENTATION_STATUS.md` §6：折叠/Reduce Motion 关闭 transform；产品自有小猫图标 `code/apps/desktop/src/app/components/brand/BrandMark.tsx` 使用 `assets/dtoken-cat-icon-96.png`，接入 `components/shell/Sidebar.tsx`） | 真实 Tauri 窗口一轮复核 | N6 | S1–S4 全过；命中矩形/焦点/hover/点击一致；图标为产品自有 SVG | MUST |
| UI-5 | Analytics 日志高度无 diff（PR-4 边界） | 已压缩 | diff 断言 | PR-4 | PR-4 diff 不含 Analytics 日志高度 | MUST |
| UI-6 | 1180×780/920×640、浅/深、中/英、Reduce Motion 可读（INV-12） | 2026-08-07 视觉审计部分实测 | 全组合真实窗口 | N6 | 全组合通过；浏览器 preview 不冒充 | MUST |
| UI-7 | 代码图谱 experimental badge（audit C4） | badge 未实现 | PR-4 | PR-4 | nav/sidebar/i18n 双语可访问 badge | MUST |
| UI-8 | service 步骤可视化：阶段标签（检查→备份→启动→验证）+ indeterminate 呼吸动画 + `prefers-reduced-motion` 下静态等价（onboarding 审计 B-1/H-2）；**service 运行中≠connected 文案诚实（H-1）：service healthy 徽标不得让用户误读「服务运行中=已连接」** | service 步骤仅按钮 + aria-live 文本，无阶段标签/动画；`StepProgress.tsx` 存在但无引用；`run_install_transaction` 同步阻塞无 stage 回调；serviceOnline（runtime 状态）与 verify connected/verified（snapshot claim）两套来源文案落差 | 复用 StepProgress（或同模式）；native 侧 emit `dtoken:install-progress`（stage/percent/indeterminate）或轮询 `get_dtoken_service_status` 里程碑；Reduce Motion 静态等价；service 成功文案补充「服务已启动，尚未完成 Agent 接入」（PR-14） | PR-14 → N6 | 服务启动显示阶段标签 + 动画；reduce 下静态信息可读；新/更新单测断言动画标记与 reduce-motion 属性；service 步骤成功态明确「服务运行中≠已连接」，connected/verified 仍以 verify 步骤 evidence 为准 | MUST |
| UI-9 | wizard 顺序门：完成前置步骤才解锁下一步，不允许跳过 service（onboarding 审计 M-5） | stepper 可点任意 step；welcome 可直连 config | 顺序门（完成前置才解锁下一步）或在 config 前置 `serviceOnline` 检查 | PR-14 → N6 | 未启动服务不能进入 config/一键配置；stepper 按 5 段顺序推进 | MUST |
| UI-10 | 八页可见文案入 DICT（zh/en）+ OMP DS 审核门（Draft→Review→Sign-off）：禁止组件内 `lang === "zh" ? "…" : "…"` 字面量逃逸；失败提示不含 raw error/path；release 附 copy sign-off 清单（onboarding 审计 §6、M-2） | i18n DICT 719 key；copyPolicy.test.ts 有禁语断言；Changes/Overview/Settings 仍有字面量逃逸 | 迁移到 DICT + copyPolicy 增「组件内不得含 zh/en 字面量」断言；OMP DS 门纳入 release 门禁 | PR-14, PR-4, N6 | 组件内无 zh/en 字面量逃逸（grep 断言）；copy sign-off 清单随 release；copyPolicy 保持绿 | MUST |
| UI-11 | Install 工作台 920×640 可读：4 标签（h-20 双行网格）不截断、StatusBar 无横向滚动（eight-page 审计 C5/C6）；**Install.tsx `window.confirm`（L821/L880）改用统一 OperationDialog 确认（M-3）** | InstallWorkbench.tsx:78-95 标签描述 920 截断风险；StatusBar `flex-nowrap+overflow-x-auto`；managed transport / MCP configure 用浏览器原生 confirm | <1024px 隐藏标签描述（仅图标+标题）或等效收缩；StatusBar `flex-wrap`/按宽度收缩；window.confirm → OperationDialog（PR-14） | PR-14 → N6 | 920×640 下 4 标签可读不截断；`scrollWidth <= clientWidth` Playwright 断言通过；Install 写操作确认与其余页面统一走 OperationDialog，无原生 confirm 残留 | MUST |

## 10. SECURITY（安全）

| ID | 需求（来源） | current | gap | owner | acceptance | release claim |
|---|---|---|---|---|---|---|
| SECURITY-1 | 全部 production secret validation path 统一复用共享 SecretClassifier（PRD §12.4.18、AGENTS.md §4.6） | `dtoken-compress/src/optical/secret.rs` 共享实现存在（bearer/JWT/Google/GitHub/PEM/cookie/credential/high-entropy） | main.rs active/merged/route-draft 路径枚举与复用 | PR-5 | 无并行弱分类器；无 fork 副本；覆盖矩阵+回归单测 | MUST |
| SECURITY-2 | runtime JSON/CSV export 无绝对 config path 泄漏（GOAL B4） | core-api export 有 `redacted=true` 断言，path 泄漏无系统回归 | 修复+回归 | PR-5 | 含绝对路径 fixture 下 export 无泄漏 | MUST |
| SECURITY-3 | secret/token/cookie/auth 不进日志/配置/导出/提交（GOAL B4、INV-3） | grep 断言惯例已存在 | 候选 grep 断言脚本 | PR-5, N0 | 全仓断言通过；报告附输出 | MUST |
| SECURITY-4 | 不泄漏 prompt/response/源码/大段原始输出（GOAL B4） | 脱敏契约已定 | 候选断言 | N0 | 断言通过 | MUST |
| SECURITY-5 | 默认 loopback only（GOAL B3） | 已实现 | 门禁断言 | N0 | N0 断言通过（同 RUNTIME-3：`cargo test -p dtoken-proxy loopback_host_validation` + 非回环 bind grep 零命中） | MUST |
| SECURITY-6 | detected/configured/supported ≠ connected/verified/healthy/active（GOAL B6、INV-6） | fail-closed 状态模型已建立 | 候选 UI/API 复核 | N4, N6 | 无冒充 | MUST |
| SECURITY-7 | cc-switch 只读（GOAL B2、INV-11） | 只读实现 + 测试 | 候选复验 | PR-3, PR-7, N5 | `wrote_cc_switch=false` 断言 | MUST |
| SECURITY-8 | 进程停止前验证 PID/exe/端口/identity（INV-5） | 契约已定 | 候选复验 | N0 | 断言 | MUST |
| SECURITY-9 | no-touch 开发边界（fake-bin/poison-bin、临时 HOME/PATH/端口） | M1 基础已交付 | 候选 N0 复跑 | N0 | fixture/fake-bin smoke 通过；无真实调用 | MUST |

## 11. PLATFORM（平台/打包/签名）

| ID | 需求（来源） | current | gap | owner | acceptance | release claim |
|---|---|---|---|---|---|---|
| PLATFORM-1 | macOS 安装/升级/回滚/autostart/credential round-trip（GOAL K3、G6/N7） | 包审计 ad-hoc 通过（SHA-256 60f7d063…，未签名）；LaunchAgent 控制面已接通 | 候选 upgrade/rollback + round-trip 脚本与证据 | PR-8 → N7 | DMG 安装→升级→卸载/回滚 hash/权限证据；LaunchAgent/Keychain 唯一隔离 round-trip | MUST |
| PLATFORM-2 | Windows NSIS 安装/卸载/DACL/MSVC/G6 PowerShell（GOAL K1/K2、N8） | DACL 测试接入 runner；NSIS/upgrade/rollback 契约脚本存在；本机无 pwsh | 实机/runner 证据 | PR-8, PR-C8B, N8 | Windows 全项同 C_final 通过；无证据则 external + blocked | MUST |
| PLATFORM-3 | 双平台同 commit/source digest fan-in（G8） | fan-in contract 存在但无 C_final 实证 | 显式 artifact manifest + 逐 artifact SHA | PR-7, PR-C8B/C8C, N8 | fan-in ok=true、commit/source digest/registry/corpus 一致 | MUST |
| PLATFORM-4 | 签名三结论判定（PRD P0.2） | 判定规则已定义 | 最终候选执行 | PR-9, PR-C8C, N9 | 三结论之一落盘；unsigned 保持 `release_ready=false` 且不算目标完成 | MUST |
| PLATFORM-5 | d-token-owned RTK production stage：structured filters、managed hook/tee、planner/accounting、macOS/Windows/WSL lifecycle；**前置：先解封三平台 hard-Blocked（RUNTIME-10）** | macOS 旧 probe 只证明基础 lifecycle；**三平台 managed 操作全部 `ManagedRtkTransactionState::Blocked`**；无 production hook/planner/tee/Windows evidence | 解封（RUNTIME-10 前置）+ C4A–C4C 完整实现 + 三平台同候选证据 | PR-R1, PR-C4A/B/C, PR-C4U, N7/N8/N10 | production dispatch 可达；secret/error/unknown 保留；hook exact restore；tee ACL/TTL；exit/timeout；卸载；无双计 | MUST |

## 12. EVIDENCE（证据治理）

| ID | 需求（来源） | current | gap | owner | acceptance | release claim |
|---|---|---|---|---|---|---|
| EVID-1 | 过期证据隔离 + candidate/source/registry/corpus binding（INV-13） | 聚合器依赖 mtime/latest，旧 pass 可穿透 | 显式 artifact manifest + expected digests 强绑定 | PR-7, PR-9, PR-C8C | 旧/null/mismatch artifact 全拒绝；不得目录扫描选证 | MUST |
| EVID-2 | N0→N1–N8/N10→N9 verification DAG 与候选失效 | 概念存在；N10 与完整失效图未接线 | 编排器/schema/失效测试 | PR-9, PR-C8C | dry-run DAG 无环；新 commit 按影响范围失效；关键 digest 变化全重跑 | MUST |
| EVID-3 | 机器验证 same commit + source/registry/corpus digests | fan-in 仅部分锚定 | 所有 producer/consumer 统一 envelope | PR-C0C, PR-C8A/C8C | N0–N10 每个机器 artifact 精确匹配 C_final | MUST |
| EVID-4 | 证据索引维护（本包 [EVIDENCE.md](./EVIDENCE.md)） | 本包建立 | 执行期滚动更新 | DOCS-3 | 每次切片/节点同步索引 | MUST |
| EVID-5 | 真实 Tauri/Computer 证据（GOAL 侧栏 S4、INV-1） | 2026-08-07 原生截图是旧候选部分组合 | N6 全组合 | N6 | 主代理 Computer 截图/坐标/窗口/时间 evidence；browser 不能替代 | MUST |
| EVID-6 | G0–G9 + N10 + 117 proof 聚合 fail-closed | 旧聚合器只覆盖 G0–G8，未强绑定 N10/117 | 显式 paths、expected digests、G5 RTK 独立 receipt、G7 exact candidate | PR-C8A/C8C | 缺任一 MUST exit 1；G9 不提前开放 | MUST |
| EVID-7 | PR 治理：不同分支不同 PR、审查+返工、合并 main（GOAL G13、AGENTS.md §9） | 既有 PR 流程与 git 审计账本 | 执行期持续遵守 | Main | 每 PR 独立分支/worktree；审计记录完整；返工经审查 | MUST |

## 13. DOCS（文档/发布收口）

| ID | 需求（来源） | current | gap | owner | acceptance | release claim |
|---|---|---|---|---|---|---|
| DOCS-1 | v0.0.1 九文件计划包（本任务） | 本包已建 | 执行期保持同步 | Main | 九文件存在且无断链；`check_structure.py` 通过 | N/A（本包） |
| DOCS-2 | 入口更新：`docs/index.md`、`docs/DOCS_ORGANIZATION.md`、`docs/PLAN.md`（本任务） | 本任务同步修改 | — | Main | 入口指向本包；DOCS_ORGANIZATION 允许 `docs/releases/<version>`；PLAN 顶部标注本包定位 | N/A（本包） |
| DOCS-3 | 发布收口文档更新（GOAL 完成定义） | 历史快照 | 最终候选后同步 PRD 版本门/PLAN/IMPLEMENTATION_STATUS/README/发布报告 | PR-9 → N9 | 收口文档与 `C_final` 一致；本包滚动更新 | MUST |
| DOCS-4 | 版本统一后文档同步（VERSION-0 配套） | 已统一（V-1 已执行，2026-08-09） | 已执行（2026-08-09） | V-1 | 活跃文档无 0.2.0-beta.1 残留（除历史） | MUST |

## 14. QUALITY / PERFORMANCE（统一压缩证明门）

| ID | 需求（来源） | current | gap | owner | acceptance | release claim |
|---|---|---|---|---|---|---|
| QUALITY-1 | 117 项能力矩阵必须是可数、机器可读工件，每项有 production dispatch 与证据 DAG | PLAN 只有领域散文，无 117 行 artifact | tracked JSON registry + validator + generated coverage；每项 `id/source_capability/owner_crate/dispatch_symbol/tests/real_evidence/gates/status` 必填 | PR-C0C, PR-C0D, PR-C8A | 恰 117 个唯一 ID；N0 前仅 `implemented_unverified/blocked`；N10/U9 最终密封 117/117 verified；零 placeholder/missing/not-in-scope | MUST |
| QUALITY-2 | codec/IR property + fuzz：lossless reconstruction、protected/secret/exact、malformed protocol | 仅 3 个通用 proptest；无统一 fuzz；各 codec evidence 不能替代跨 NodeAction 证明 | 每个 NodeAction/codec 的重建、exclusive ownership、malformed、secret/exact/open-tool corpus；有界 fuzz runner | PR-C2A, PR-C6C | crash/panic=0；lossless byte-exact；每 node 恰一 action；critical privacy/meaning failure=0 | MUST |
| QUALITY-3 | fixed-corpus release benchmark：resolved production 入口、p50/p95/p99、CPU、throughput、alloc/RSS、queue/cancel、ratio/benefit/fallback | 现有 benchmark 调 no-op `compress()`；无 criterion/fixed corpus/throughput/replay | tracked corpus manifest + release runner + corpus digest；先采 fresh baseline，再冻结 absolute cap 与 noise/no-regression band；passthrough floor 单列 | PR-C0C, PR-C7A/C7C | 阈值由同硬件/OS/release profile fresh baseline 产生并版本化；C_final 全过；禁止从 UI 指标或旧 evidence 编造 | MUST |
| QUALITY-4 | deterministic replay / coding-task quality / provider paired eval | 无 replay harness、无真实 paired runner；NUEDC 仅计划/手工 | decision/body digest replay；same-model same-task baseline/optimized paired run；task outcome judge 与 holdout；Optical exact fidelity/cost/latency | PR-C7B, N3, N10 | replay byte/digest 一致；任务成功率/关键事实不退化；Optical paired eval 全绿；claimable 仅 Provider actual | MUST |
| QUALITY-5 | pre/post-send fault graph、durability 与性能 barrier | 无系统 failpoint；Ledger per-record fsync、SQLite durability/全表 replace 未决 | failpoint registry；bounded queues/group commit；CCR private state 用 FULL durability；普通 projection 可 NORMAL 但可重建；transport-observable commit | PR-C6A..C6C | 每 fault 唯一安全终态；pre-commit 至多发送一次 baseline；post-commit 零自动 retry；barrier 通过 C_final SLO | MUST |
| QUALITY-6 | hot-path parse-once 与分配预算 | 当前多次 `serde_json::to_string`/clone/token measure；每请求重复整树序列化 | protocol parse once、node borrowed view/Cow、source digest/token count once、lazy candidate clone；无 FS/network/process/plugin lookup | PR-C1B, PR-C7A/C7C | fixed benchmark 报告 alloc count/bytes、RSS、CPU、throughput；无回归带通过；profiler 证明无每 stage 整树 clone | MUST |
| QUALITY-7 | stage selection 与 fallback 不能被调用方或 adapter 伪造 | 当前 caller validator/digest/timestamp 可铸造状态 | compile-fail/API privacy tests + negative construction tests；sealed internal constructors | PR-C0B, PR-C1C, PR-C6C | 外部 crate/adapter 无法构造 Optimized/Committed；false receipt corpus 100% 拒绝 | MUST |
| QUALITY-8 | **产品级固定 corpus 端到端压缩节省 floor ≥10%**：以 resolved production 入口对 product-level fixed corpus 的端到端（入站→plan→send）token 节省比率（baseline tokens − final send tokens）/baseline tokens ≥ 10%；**该 floor 与 fresh baseline no-regression band 分离**——no-regression band 只防相对退化（相对 C7A fresh baseline 不劣化），floor 是绝对产品级门槛（低于 10% 即 blocked，不得以「无退化」通过） | 现有 benchmark 调 no-op `compress()`；无端到端 floor 定义；无 product-level corpus | tracked product-level corpus manifest + 端到端节省计算（decision/body digest 绑定）+ floor 断言 | PR-C7A, PR-C7C, N10/U5 | product-level corpus 端到端节省 ≥10%（同 C_final/source_digest/corpus digest 绑定）；fresh baseline 无回归 band 单独通过；低于 floor → N10 blocked | MUST |

## 15. 覆盖核对

- 用户 GOAL 13 项 → G1..G13 全部映射（见 [GOALS.md](./GOALS.md) 第 1 节表）。
- 侧栏专项 4 项 → UI-4；硬边界 8 项 → B1..B8 映射；发布阻断 6 项 → K1..K6 映射。
- PRD P0.0.7 发布门 MUST → PRODUCT-1/RUNTIME-1/AGENT-1/AGENT-2/PLATFORM-*；P0.0.10 硬合同 → AGENT-3/AGENT-5/AGENT-7/CONFIG-3。
- Headroom 完整迁移（PRD P0.0.6、§8、PLAN 1C）→ RUNTIME-7/8/9、CONTEXT-1..4、QUALITY-1..7、PR-C0..C8、G0–G9；全部为 v0.0.1 MUST。
- LeanCTX 九边界 → CONTEXT-1/3/4 与 PR-C3*；pxpipe/Optical 差异化能力 → OPTICAL-1..6 与 PR-C5*/PR-O1/O2；RTK 差异化能力 → RUNTIME-7/9 与 PR-C4*。

## 16. 审计发现映射（2026-08-09 四份审计 + OMP 交叉评审 → requirement → task → slice → verification）

> 本表把四份审计（统一压缩 / Agent 全量矩阵 / Onboarding 八页 / CBM 五版本）与 OMP 交叉评审的全部 Blocker/High 收敛为可追踪链；任何行断裂即计划不完整。切片列中 `PR-*` 见 [EXECUTION.md](./EXECUTION.md)，任务列中 `T-*` 见 [TASKS.md](./TASKS.md)，验证列中 `N*/U*` 见 [VERIFICATION.md](./VERIFICATION.md)。

| 审计发现（Blocker/High） | requirement | task | slice | verification |
|---|---|---|---|---|
| 统一压缩：117 项能力矩阵无机器工件（QUALITY-1/U1/U9/G8 不可执行） | QUALITY-1 | T-C0C, T-C0D, T-C8A | PR-C0C, PR-C0D, PR-C8A | N10/U1, U9 |
| 统一压缩：发送路径无 canonical IR/NodeAction；memory 校验后注入；no-op compress() 生产 fallback（RUNTIME-7/CONTEXT-4 违反） | RUNTIME-7, CONTEXT-4, QUALITY-6 | T-C0A, T-C1A, T-C1B, T-C1C | PR-C0A, PR-C1A, PR-C1B, PR-C1C | N10/U0, N1 |
| 统一压缩：type-state 可伪造（caller validator/digest/ccr: 前缀/时间戳）（RUNTIME-8/QUALITY-7 违反） | RUNTIME-8, QUALITY-7 | T-C0B, T-C1C, T-C6C | PR-C0B, PR-C1C, PR-C6C | N10/U0, U3 |
| 统一压缩：生产 transport 未消费 SendCommitReceipt；CCR store→receipt→marker→restore 未闭环 | RUNTIME-8 | T-C0B, T-C6C | PR-C0B, PR-C6C | N10/U3 |
| 统一压缩：LeanCTX 九边界 8/9 缺失、context-bus 不存在（CONTEXT-1/3 违反） | CONTEXT-1, CONTEXT-3 | T-C3A, T-C3B, T-C3C | PR-C3A, PR-C3B, PR-C3C | N10, N1 |
| 统一压缩：RTK 三平台 hard-Blocked（PLATFORM-5/RUNTIME-9 违反） | RUNTIME-10, PLATFORM-5 | T-C4U, T-C4A, T-C4B, T-C4C | PR-C4U, PR-C4A, PR-C4B, PR-C4C | N7/N8/N10/U2 |
| 统一压缩：Optical exact active 不存在（shadow-only）（P0.0.7 违反） | OPTICAL-1..6 | T-C5A..C5D, T-C7B | PR-C5A..C5D, PR-C7B | N10/U7 |
| 统一压缩：P0 基线未冻结、HEAD 漂移、dirty 未裁决（DAG 起点未锁定） | EVID-1, VERSION-0 | T-P0 | P0 | N0 |
| 统一压缩：DAG 图/表漂移、RTK 解封前置缺失、117 registry 顺序 | EVID-2, RUNTIME-10, QUALITY-1 | T-C0D, T-C4U, T-PR9 | PR-C0D, PR-C4U, PR-9 | N10/U1 |
| 统一压缩：Native Responses 绕过 canonical pipeline | AGENT-10, RUNTIME-7 | T-PR12, T-C1C | PR-12, PR-C1C | N10/U0 |
| 统一压缩：压缩节省 floor 与 fresh baseline 未分离、benchmark 调 no-op | QUALITY-3, QUALITY-8 | T-C7A, T-C7C | PR-C7A, PR-C7C | N10/U5 |
| Agent：Codex 顶层 openai_base_url/model_catalog_json 无法解析（B1） | AGENT-8 | T-PR10 | PR-10 | N2 |
| Agent：cc-switch openclaw app type 不可见（B2） | AGENT-9 | T-PR11 | PR-11 | N5 |
| Agent：23/15/6/2 矩阵口径（AGENT-4=23 检测、15 Route、6 条件 MCP/Hook、2 detection-only） | AGENT-4 | T-PR10, T-PR11 | PR-10, PR-11 | N4 |
| Agent：热同步 review-gated 事实 + 自动同步授权撤销门（OMP 源码审计证实存在，不得误报缺失） | AGENT-11 | T-PR13 | PR-13 | N4/N5 |
| Agent：dtoken-agent-bridge 存在性与身份验证（component/protocol/OS/arch/version + 签名/Team ID/Authenticode）通过后才开放 MCP/Hook desktop-managed 写面（agent 矩阵 H3） | AGENT-7 | T-PR13 | PR-13 | N4 |
| Agent：cc-switch sync 与 active route 最终比对未下沉 Rust-owned 合同（derive_cc_switch_status/enrich_upstream_discovery；agent 矩阵 H4） | AGENT-9, AGENT-5 | T-PR11 | PR-11 | N5 |
| Agent：原生配置事务/无 chooser/cc-switch 只读/MCP-Hook 诚实区分 | AGENT-3/5/6/7, SECURITY-7 | T-PR3, T-PR6, T-PR7 | PR-3, PR-6, PR-7 | N4/N5 |
| Onboarding：6 步 vs 5 段、独立 upstream 步骤（B-2） | PRODUCT-5 | T-PR14 | PR-14 | N6 |
| Onboarding：service 运行中≠connected 文案落差（service healthy 徽标 vs verify claim/unknown，H-1） | UI-8 | T-PR14 | PR-14 | N6 |
| Onboarding：service 无阶段标签/呼吸动画/Reduce Motion（B-1） | UI-8 | T-PR14 | PR-14 | N6 |
| Onboarding：wizard 可跳过 service、无顺序门（M-5） | UI-9 | T-PR14 | PR-14 | N6 |
| Onboarding/OMP：八页文案 DICT 逃逸、缺 OMP DS copy gate（M-2） | UI-10 | T-PR14, T-PR4 | PR-14, PR-4 | N6 |
| 视觉审计：Install.tsx `window.confirm` 未走统一 OperationDialog（L821/L880，M-3） | UI-11 | T-PR14 | PR-14 | N6 |
| 视觉审计：Install 920×640 截断、StatusBar 横滚（C5/C6） | UI-11 | T-PR4 | PR-4 | N6 |
| CBM：单版本 manifest 无法表达五正式版（B1）；v0.9.1-rc.1 prerelease 与正式口径冲突（B3） | OSS-5 | T-CBM3 | PR-CBM3 | N4/N8 |
| CBM：三个正式版本官方无资产无法逐版本真实验证（B2） | OSS-6 | T-CBM4 | PR-CBM4 | N4/N8 |
| CBM：promo/文案硬编码版本（generate-promo-v2.mjs L99/L175 硬编码 v0.9.1-rc.1），未由 manifest 驱动（M1） | OSS-5, OSS-7 | T-CBM3 | PR-CBM3 | N4 |
| CBM：平台矩阵（v0.8.1 无 windows-arm64）、server.json 版本滞后（H1/H2/H3） | OSS-7 | T-CBM3, T-CBM4 | PR-CBM3, PR-CBM4 | N8 |
| N3：旧 bounded replay 不替代 OpenCode Free/Go dsv4flash 长工程验收 | AGENT-2, QUALITY-4 | T-N3 | PR-C8B | N3 |
