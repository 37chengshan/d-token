# d-token 总体架构

状态：Accepted
最后校准：2026-07-11
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
│ Overview / Connect / Context / Analytics / Doctor / Changes  │
└──────────────────────────────────────────────────────────────┘
```

macOS 与 Windows 共享 React UI 和领域契约。Keychain/Credential Manager、进程、路径、签名和安装器差异位于 Rust/Tauri 平台适配层；只有 Rust/Tauri 无可靠替代时，macOS 才允许极少量 Swift 系统桥接。

## 3. 请求链路与失败语义

```text
Agent request
  → loopback/Host/Origin/body guard
  → protocol parse (preserve unsupported fields)
  → configured Route + Provider selection
  → compression policy executor
  → upstream request (optional OS credential resolution)
  → streaming or non-streaming response
  → correlated RequestLog / RouteDecision / Event Ledger
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
| `dtoken-compress` | Protected Context IR、semantic/Optical hybrid planner、确定性 PNG renderer、sealed patcher 与 post-transform validator |
| `dtoken-event-ledger` | append-only 脱敏事件账本 |
| `dtoken-config` / `dtoken-config-patcher` | 配置模型、计划、patch 和 rollback primitives |
| `dtoken-process` / `dtoken-doctor` | 受管进程与诊断 primitives |
| `dtoken-headroom-adapter` | Headroom-derived compatibility boundary，不调用外部 Headroom dashboard |
| `dtoken-preprocessor` / `dtoken-project-policy` | 预处理与项目级策略 |
| `dtoken-types` | 共享领域类型 |

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

当前已有 draft、preview、OS credential、backup/rollback primitives，但真实 Agent 配置、原子写入、并发修改与故障恢复尚未全部达到发布门禁。UI 只能依据真实事务结果显示成功。

## 7. 数据与观测

当前事实：JSON/JSONL 保存配置、RequestLog、RouteDecision、Event Ledger、CCR metadata 与回退证据；Dashboard 通过 local service/Tauri 聚合读取。部分桌面聚合状态仍需消除硬编码并统一 freshness/source contract。

目标状态：JSONL 作为审计原始账本，SQLite 作为可重建查询索引；坏行隔离、轮转、文件锁、损坏诊断、保留期和导出白名单全部可验证。SQLite 未实现前不能宣称该目标已交付。

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
7. shadow/passthrough/pre-send fallback 转发 ingress 原始 bytes；active 只有在最终 patched body 验证通过后才重新编码。upstream send commit 后禁止自动原文重试。

## 9. 生命周期与发布边界

1. 所有监听默认且强制 loopback。
2. 受管 runtime 的启动 commit 条件必须包含 identity manifest、health 和可安全停止证据。
3. Desktop 关闭后 runtime 是否继续运行必须是明确设置，并保持可观察、可停止、可恢复。
4. macOS 已配置 `.app`/DMG 构建和可选签名/notarization workflow；这不等于候选版已经完成安装后实机验收。
5. Windows 共享产品 UI 与 Rust 核心，但仍需安装包、凭据、路径、回退和视觉一致性的独立实机证据。
