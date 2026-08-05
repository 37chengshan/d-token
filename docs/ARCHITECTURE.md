# d-token 总体架构

状态：Accepted
最后校准：2026-07-15
关联：`docs/PRD.md`、`docs/TECH_STACK.md`

## 1. 架构目标

1. 由 d-token 自己拥有 Headroom-compatible/derived compression、CCR、memory、output shaping、metrics、history 和 dashboard primitives。
2. 以 Rust 实现本地协议入口、路由、配置、事件账本、Doctor 和业务规则。
3. 以 Tauri 2 + React/TypeScript 提供 Windows/macOS 共享桌面控制面。
4. Provider、Route、Instance、配置变更和请求结果全部可解释、可审计、可回退。
5. 默认仅绑定 loopback，敏感凭据只存在于 OS credential store 和最短出站生命周期。
6. 外部 Headroom/RTK/cc-switch 只能作为只读迁移参考或上游接口，不能成为 App 的 Dashboard/runtime 依赖。

## 2. 总体分层

```text
OpenCode / Claude Code / Codex / Cursor / Cline / other agents
                              │
                              ▼
                 d-token loopback protocol entry
              OpenAI Chat / Responses / Anthropic Messages
                              │
                              ▼
┌──────────────────────────────────────────────────────────────┐
│ d-token-owned Rust runtime                                   │
│ protocol guard → compression/CCR/memory → route decision     │
│ → OS credential resolve → upstream streaming/response        │
│ → RequestLog + Event Ledger + metrics/history/export         │
└──────────────────────────────────────────────────────────────┘
               │                              │
               ▼                              ▼
 official/custom/cc-switch/local       local API / Tauri commands
 upstream interfaces                            │
                                                ▼
┌──────────────────────────────────────────────────────────────┐
│ Tauri 2 desktop control plane                                │
│ shared React/TypeScript UI                                   │
│ Overview / Install / Codebase Memory / Analytics / Visual    │
│ Compression / Doctor / Changes / Settings                    │
└──────────────────────────────────────────────────────────────┘
```

macOS 与 Windows 共享 React UI 和领域契约。Keychain/Credential Manager、进程、路径、签名和安装器差异位于 Rust/Tauri 平台适配层；只有 Rust/Tauri 无可靠替代时，macOS 才允许极少量 Swift 系统桥接。

## 3. 请求链路与失败语义

```text
Agent request
  → loopback/Host/Origin/body guard
  → protocol parse (preserve unsupported fields)
  → configured Route + Provider + Model + Agent/Project scope resolution
  → immutable ingress baseline + live-zone/CacheAligner
  → ContentRouter + codec/CCR/memory/output plan
  → transform artifact + protocol post-validation
  → sealed ReadyToSend
  → upstream request (optional OS credential resolution)
  → streaming or non-streaming response
  → usage/CCR/memory finalize
  → correlated RequestLog / RouteDecision / Event Ledger / query projection
  → Dashboard and export projections
```

未配置 Route/Provider、凭据不可用、timeout、401/403/429/5xx、协议不兼容和流中断必须返回类型化失败，并在脱敏事件中保持同一 request id。正式路径禁止合成成功、静默 provider fallback 或把失败记为 savings 成功。

每条请求的可观测字段至少包括：request id、timestamp、agent/instance、input protocol、provider/route、model、tokens before/after/output/saved、transforms、latency、outcome/error type 和 config source。完整 prompt/response、密钥、Authorization 与 cookie 不入账。

## 4. 模块边界

### 4.1 Desktop：`code/apps/desktop`

- React/TypeScript：页面、设计系统、状态解释、可访问交互和用户任务流程。
- Tauri Rust：native commands、服务生命周期、Provider/Route draft、OS credential store、配置与回退适配。
- `dtoken-proxy` external binary：按 target triple 打包的 d-token 自有 Rust runtime，不是第三方 sidecar。
- 浏览器预览：只验证 UI；不可作为本机状态、路由或运行时功能证据。

### 4.2 Rust workspace：`code/crates/*`

| Crate | 当前职责 |
|---|---|
| `dtoken-proxy` | loopback HTTP 进程入口与 d-token-owned service |
| `dtoken-core-api` | 协议/API router、请求处理、upstream 转发、Dashboard projection |
| `dtoken-router` | Provider/Route/Instance 决策模型与规则测试；生产路径仍需继续收敛为单一决策权威 |
| `dtoken-compress` | Protected Context IR、共享 SecretClassifier、HybridPlanV2、确定性 PNG renderer、全请求 byte permit、sealed patcher 与 post-transform validator |
| `dtoken-event-ledger` | append-only 脱敏事件账本 |
| `dtoken-config` / `dtoken-config-patcher` | 配置模型、计划、patch 和 rollback primitives |
| `dtoken-process` / `dtoken-doctor` | 受管进程与诊断 primitives |
| `dtoken-headroom-adapter` | Headroom-derived compatibility boundary，不调用外部 Headroom dashboard |
| `dtoken-preprocessor` / `dtoken-project-policy` | 预处理与项目级策略 |
| `dtoken-types` | 共享领域类型 |
| `dtoken-learning` | Agent session/project discovery、安全索引、结构化学习与 managed writer |

Headroom 后端全能力迁移的目标边界（计划中，未实现完成）新增：

| Crate | 目标职责 |
|---|---|
| `dtoken-context-store` | SQLite migration/query projection、CCR private store、memory/SharedContext、cache state |
| `dtoken-runtime-pipeline` | proxy/library/MCP 共用的 prepare → validate → send outcome → finalize 生命周期 |
| `dtoken-mcp` | stdio JSON-RPC transport；只调用共用 pipeline/store/metrics，不复制业务逻辑 |

依赖方向必须是 `types/domain/store → runtime-pipeline → core-api/MCP/proxy → Tauri/SDK`。`dtoken-core-api` 保留协议/transport/public projection 职责，不继续承载 codec、memory、CCR、learning 或 integrated RTK 的领域实现。

`code/runtimes/headroom/` 记录 Headroom-derived runtime 的迁移/provenance 边界。目前没有 Python sidecar，也没有 `code/sidecars/headroom-compatible` 或 `code/sidecars/chain-bridge`；不得把不存在的目录写入当前架构。

### 4.3 Route adapter

Route adapter 属于 Rust runtime：读取 active ProviderProfile/RouteProfile/InstanceProfile，校验协议与模型，按显式 route 转发 official/custom/cc-switch/local upstream。cc-switch 是可读取和同步的上游接口，不是 d-token 的运行时事实源；禁止静默 fallback。

## 5. Runtime API 与 Tauri bridge

当前 Rust local service 的主要接口：

```text
GET  /health
POST /v1/chat/completions
POST /v1/responses
POST /v1/messages
GET  /dashboard/payload
GET  /stats | /stats-history | /metrics
GET  /providers | /routes | /instances | /config/active
GET  /route-decisions/recent | /logs/recent | /events
POST /doctor/run
GET  /export/json | /export/csv
GET  /rtk/* | /headroom/* | /ccr/* | /memory/*
POST /compression/optical/preview
GET  /compression/optical/status
GET  /compression/optical/capabilities
GET  /compression/optical/capabilities/lifecycle
GET  /compression/optical/events/recent
```

Tauri commands 管理 desktop-only 权限：服务启停、OS credential 写删、配置 draft/apply/rollback、设置与本机检测。UI 不应跨过这些窄接口直接读写系统文件。

Doctor 桌面闭环使用窄命令合同：

```text
get_doctor_snapshot
→ run_doctor_scan (DoctorSnapshotV3 + dynamic freshness/coverage)
→ get_doctor_repair_modules (dtoken-doctor registry + executor capability)
→ preview_doctor_repair (snapshot revision + selection + redacted per-Agent diff)
→ apply_doctor_repair (explicit confirm + plan digest/hash/TTL revalidation)
→ verify_doctor_repair (post-scan + DoctorRepairReceipt)
```

`dtoken-doctor` 拥有模块注册、快照/计划/回执领域模型；Tauri 采集平台证据并复用 Service、Route、Agent Integration、Optical executor，不实现第二套 patcher。旧 `run_doctor` / v2 preview/apply 仅保留兼容；`run_product_readiness_check` 仍是内部/候选聚合，不是用户 Doctor 页主结果。扫描可以持久化脱敏快照和审计事件，但不得发起 Provider 请求或读取凭据。当前已接写执行器为身份完整验证的服务生命周期和已有管理证据的 Claude Code/Codex/OpenCode Agent Integration；其余注册模块未具备 executor 时 fail closed。

## 6. 配置与凭据

```text
ProviderProfile
  id / protocol / base_url / timeout / models / opaque credential ref

RouteProfile
  id / input protocol / provider / instance / policy / loopback port

InstanceProfile
  id / agent / route / managed identity / status / log path
```

配置事务目标状态机：

```text
scan → plan → diff → durable backup → explicit confirm
→ atomic apply → parse/semantic verify → health verify
→ commit rollback point
```

Agent integration 使用持久 `AgentIntegrationTransactionManifest` 绑定 review digest、目标顺序、planned-after hash、rollback IDs、目录与 apply/verify/rollback 状态；verify/rollback 只接受原 transaction ID，并由跨进程事务锁串行。manifest 缺失、损坏、stale hash 或验证异常时 UI 必须显示 failed/partial/changed，不能保留旧 verified。真实 Agent 配置仍须单独授权与候选验收。

Changes 的通用回退入口使用：

```text
list rollback points
→ preview_rollback: integrity + scope + current SHA-256 + revision
→ explicit confirm bound to revision/SHA-256
→ reverse safety backup bound to the post-rollback hash or missing state
→ dispatch by executor_kind: generic / agent_integration / optical
→ transaction-level exact restore
→ file/transaction verification + optional managed runtime restart/route/Optical verification
→ persistent rollback receipt
```

备份完整性与当前可恢复性是两个独立状态。legacy/unknown scope、损坏 manifest、已回退事务和并发 hash 变化必须在 preview 阶段拒绝。当前桌面层通过验证过的 Agent Integration manifest 将多个 rollback ID 聚合为一个事务点，先为每个当前目标创建反向安全备份，再调用原 Agent transaction executor；Optical-only active-config 事务调用 Optical executor 并复检 runtime。Route/安装等未接来源 executor 的 scope 继续锁定，底层单文件 ConfigPatcher backup 不能替代整笔事务语义。

## 7. 数据与观测

当前事实：JSON/JSONL 保存配置、RequestLog、RouteDecision、Event Ledger、CCR metadata 与回退证据；Dashboard 通过 local service/Tauri 聚合读取。部分桌面聚合状态仍需消除硬编码并统一 freshness/source contract。

当前增量状态：`dtoken-context-store` 已实现 SQLite schema v3 migration，并持久化 capability、request、CCR metadata、redacted Agent detection projection，以及 exact route/provider/model/protocol/config identity 绑定的 Provider cache counter observation；Agent snapshot 与 capability evidence 在同一事务写入。JSONL 仍是审计原始账本。坏行隔离、轮转、文件锁、损坏诊断、保留期、重建与导出白名单尚未全部闭环，因此不能把完整 persistence gate 宣称为已交付。

CCR 原文、RTK tee（如启用）和 ordinary runtime state 必须使用不同 storage namespace/权限/TTL/quota。SQLite 只保存 CCR metadata，不保存原文或密钥；memory 只保存允许的结构化脱敏内容，不把 raw session/prompt/response 建索引。

当前请求路径由 `ContextEnvelope` 保存受限原始 ingress，并按 `Prepared → Validated → ReadyToSend` 类型状态推进；Chat、Responses、Anthropic direct/bridge 只能发送 `ReadyToSend`。CCR 优化在 pre-send 阶段必须取得 durable store receipt，否则发送 immutable original。G2 已把 Hybrid IR 派生的 FrozenPrefix/TransformableWindow/ProtectedTail、identity-bound cache key、类型化 ContentRouter、逐目标 manifest 与 mutation/semantic post-validator 接入 resolved compression path；JSON/Log/Search/Build/Diff/Text/HTML 仅开放有精确验证证据的安全子集。Anthropic/OpenAI allowlisted cache counters 已在成功的非流式响应后持久化；显式 cache policy registry 提供 TTL/min-prefix/strategy，但只有 exact fresh observation 同时存在时才生成并传入 evidence。Rust Code parser candidate 仅属于 evaluation API，不进入 ContentRouter/production transform。Direct Anthropic Optical active provider breakpoint、stream final usage、raw-wire prefix、其余 Code 与完整质量/故障/性能 gate 仍未闭环。Optical code/context → PNG 是唯一图像压缩方向；普通图片文件重编码不属于架构范围。

所有 Dashboard/API projection 应逐步统一：

```text
source + generated_at + freshness + partial + warnings + schema_version
```

## 8. Desktop 与 Dashboard 原则

1. 承接 Headroom Dashboard 的产品能力，但不依赖外部 `127.0.0.1:8787`。
2. 正式 UI 不用 fixture 填充“健康/已连接/已配置”；不可用就显示不可用与下一步。
3. 浏览器预览和 Tauri runtime 必须有显式证据状态，不得互相冒充。
4. 多 Agent 控制面是产品 IA；OpenCode 是当前真实验收入口，不是导航中心。
5. Windows/macOS UI 1:1，共享组件、token、文案与交互；平台说明仅在确有差异的操作处出现。
6. Optical active 后端只允许同协议 Anthropic closed historical low-risk tool result，并受签名 Capability/eval、资源、经济、post-validation 和 durable pre-send Ledger 门控；默认关闭，跨协议及未知模型不可达。
7. shadow/passthrough/pre-send fallback 转发 ingress 原始 bytes；active 只有在最终 patched body 验证通过后才重新编码。Ledger 按 `planned → artifact_validated → send_committed → upstream_completed/failed/fallback` durable-first 记录；stream EOF、错误和 body drop 都产生唯一终态。upstream send commit 后禁止自动原文重试。
8. `dtoken-agent-bridge` 是 metadata-only Hook/MCP sidecar，不参与压缩数据路径。release runtime 必须先验证平台签名/Team ID 或 Authenticode identity，再允许执行 `identity`；identity 还需匹配 component、protocol、OS、arch 与版本。该 loopback identity 不是加密式进程所有权证明。
9. `dtoken-codebase-memory-launcher` 是可选图谱能力的受限执行边界，不进入请求压缩/路由链。大型 upstream 不进入 App bundle；用户从固定官方 release 导入默认或自定义目录后，Rust control plane 自动发现并按 pinned platform/arch/hash 校验。Agent MCP 会话由 Agent 拥有；索引与安全图页进程由 d-token 拥有，两类实例使用独立 cache namespace、receipt 和停止权限。launcher 只允许版本化只读工具及绑定 canonical root、强制 `persistence=false` 的索引调用。
10. 图谱网页由 d-token-owned loopback host 提供短期 token、Host/Origin/路径/方法 allowlist，只投影授权项目的只读架构与搜索结果；上游 filesystem browse、RPC、日志、进程、mutation、ADR 和 delete 端点不进入产品网络面。

## 9. 生命周期与发布边界

1. 所有监听默认且强制 loopback。
2. 受管 runtime 的启动 commit 条件必须包含 identity manifest、health 和可安全停止证据。
3. Desktop 关闭后 runtime 是否继续运行必须是明确设置，并保持可观察、可停止、可恢复。
4. macOS 已配置 `.app`/DMG 构建和可选签名/notarization workflow；这不等于候选版已经完成安装后实机验收。
5. Windows 共享产品 UI 与 Rust 核心，但仍需安装包、凭据、路径、回退和视觉一致性的独立实机证据。
6. Headroom backend G0–G8 通过前，前端只能保留现状，不得用页面状态、endpoint 名称或 fixture 证明能力完成；G9 后再统一接入 capability revision/freshness/action-result。
