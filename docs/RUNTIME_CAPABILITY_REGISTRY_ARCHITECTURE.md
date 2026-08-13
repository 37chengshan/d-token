# d-token Runtime Capability Registry 架构（R2.E 落地设计）

更新时间：2026-08-13（v2，按 arch-review 审查修正）
状态：ACTIVE（产品合同 `docs/PRD.md` P0.0.11 已冻结；本文档是 R2.E 落地架构，不替代 PRD/PLAN）

> v2 修订记录：修复 arch-review 发现的 5 个阻断（crate 依赖边界、cc-switch 授权语义、未知模型分层失败、receipt/evidence 契约、Router::decide 契约）与 12 个次级问题（协议/策略类型对齐、Instant 持久化、seed 供应链、容量治理、字段级证据、请求解析失败语义、并发写 CAS、agent_managed_auth 观测、core-api 接入点、展示层映射、生态共识限定、fallback_strategy 字段）。

## 1. 目标

一次配置后，无论用户切换什么模型（登录/账号管理的、cc-switch 切换的、OpenCodex/Codex 动态路由的），d-token 都能：

1. 自动识别当前请求的 `Agent → Provider → Model → Protocol → Endpoint → Capability`；
2. 自动匹配唯一上游（三类运行模型，P0.0.10.6）；
3. 自动选择压缩策略（capability 驱动，未知模型不因能力未知而失败）；
4. 生成绑定 `model identity + capability + strategy + route + ledger evidence` 的完整 receipt。

新增模型 / 新增 Provider / 用户修改配置 / cc-switch 切换 / 新增 route，均不要求重新配置，也不要求重新发布 d-token。

## 2. 开源调研结论（2026-08-13，5 个项目，源码实证）

| 项目 | 核心机制 | 可借鉴 | 与 d-token 约束的冲突 |
|---|---|---|---|
| **LiteLLM** | 双层能力表：内置 `model_prices_and_context_window.json` + per-deployment `model_info` 覆盖并反向注册；通配 model_name（`anthropic/*`）命中后把真实 model 名注入 deployment；fallback/cooldown（默认 3 次/5s）/负载均衡；未知模型默认 fail-closed（`RouterRateLimitError`） | ① 能力双层结构（内置兜底 + 覆盖）；② 通配 pattern =「一次配置、新模型零重配」核心，命中后保留真实 model 名透传；③ 失败路径显式可解释；④ `/model/info` 未知模型降级返回基础字段 | 依赖 Redis/DB 共享状态（d-token 需内存态）；`pass_through_all_models` 静默放行与 fail-closed 冲突；cc-switch 只读跟随禁止运行期改写外部注册表 |
| **new-api / one-api** | Ability 物化表（group×model×channel×priority×weight）纯索引路由；`FormatMatchingModelName` 归一化 + `*` 通配键动态适配；ModelMapping 链式重定向（Origin vs Upstream 名分离）；未知模型 503 `model_not_found` fail-closed | ① Ability 物化索引；② 通配键动态适配；③ 重定向与计费/日志解耦（d-token 的「用户选择 model」与「透传 model」天然分离）；④ Priority 硬分层 + Weight 层内加权 | 隐式归一化 = 静默猜测（d-token 需显式声明式通配）；多租户计费/自动禁用/渠道亲和粘性不适用；能力白名单在渠道上，无独立 route 表 |
| **claude-code-router** | 本地 HTTP gateway（127.0.0.1:3456），Agent 一次指向永久解耦；RoutePolicyEngine 每请求决策链，决策原因写入 `x-ccr-route-reason/source/routed-model` 响应头；ModelRegistry 单一解析入口（歧义→undefined）；未知 model 原样透传（非 fail-closed） | ① 配置与流量解耦；② 决策链 + 响应头透出 route reason（receipt 借鉴）；③ ModelRegistry 单一解析入口；④ 编译期诊断未配置 model；⑤ 每请求独立性 | 未知 model 透传而非 fail-closed（违反 d-token）；重写 body.model 与「Codex/OpenCodex 原样透传」冲突——只借鉴路由解析/决策 trace，不借鉴 model 改写；任意 JS 脚本规则需门禁 |
| **cc-switch** | SQLite：`providers(id+app_type 复合主键, settings_config 任意 JSON, is_current)`；无独立 model 表，model 是 settings_config 内字段；切换 = snapshot → 覆写 live 文件 → backfill；additive vs switch 模式 | ① app_type→provider 复合主键 + 任意 JSON（一次配置多客户端、零迁移）；② SSOT 分层（DB 事实，live 文件可重建投影）；③ is_current 标记 | 直接覆写用户配置文件（d-token 只读跟随，只能订阅/轮询 is_current/文件 mtime）；切换是「全局替换当前 provider」而非按请求路由；凭据明文存 SQLite |
| **models.dev + OpenRouter** | models.dev：TOML 即数据库，zod 校验，`shape: responses\|completions`、`reasoning_options`、cost tiers、`base_model` 继承；npm `@opencode-ai/models/snapshot` 离线快照。OpenRouter `/models`：`context_length`、`architecture`、`supported_parameters[]`、`pricing.overrides` | ① 完整能力字段集（Registry 种子 schema 蓝本）；② `@opencode-ai/models/snapshot` 离线种子；③ `supported_parameters` = 能力门禁模型；④ `shape` 与 Responses 透传对齐 | 社区维护的「模型事实」非「路由事实」（只能作种子不能作门禁）；运行时拉取需外网（vendor 快照 + 显式刷新）；OpenRouter 字段只描述其路由面 |
| **Envoy AI Gateway + Higress** | Envoy：ext_proc 解析 body 提取 model → `x-ai-eg-model` 头 → 按模型选路；显式 schema 对 translator（从不猜测）；未知模型 404 fail-closed；`/v1/models` 本地合成。Higress：静态能力表 + modelMapping（精确/前缀/正则/`*` 兜底，未命中透传） | ① 「模型即路由键」解耦；② 显式 translator 注册表；③ `/v1/models` 只读展示层；④ 流式 include_usage 防计量绕过 | K8s/网关部署形态；新增模型=改配置；无动态能力探测；Higress 未知模型静默透传（非 fail-closed） |

**生态共识（限定表述）**：在「路由/上游选择」层面，主流网关（LiteLLM/one-api/new-api/Envoy）对未知模型一律 fail-closed；「一次配置适配新模型」的正解是显式通配 pattern（LiteLLM `anthropic/*`、new-api 通配键、Higress `*` 兜底）。**注意：此共识只适用于路由层；压缩/能力层可以（且应当）用默认策略兜底不失败**（见 §11 分层失败语义）。d-token 的通配必须显式、可审计，不做隐式归一化。

## 3. 设计原则

1. **模型即路由键**：每请求从 body/path 提取 model，作为路由与 registry 查询的主键；与现有 `decide(instance_id, input_protocol)` 正交叠加（不破坏旧签名，见 §6）。
2. **能力三层表**：Seed（vendored 快照，只读）→ Observed（真实请求观测回写，本地）→ Learned（未知模型推断沉淀，本地）。合并时 seed 兜底、observed 覆盖、learned 精确；每字段带 source+confidence+证据。
3. **显式通配，禁止隐式归一化**：`opencode/*`、`claude/*` 等 pattern 只能由配置/agent 声明（跟随 Agent 实际配置产生）；命中进 receipt 的 route reason。
4. **分层失败语义**：路由层 fail-closed（无法唯一解析上游 → 可诊断错误 + 来源展示）；压缩层永不因能力未知而失败（默认策略 + `capability: unknown` 标记）。两语义分离，见 §11。
5. **决策可观测**：每请求决策链写入 receipt 与 Event Ledger（reason/source/model identity）；借鉴 CCR 但只读透出，不改写请求 model。
6. **协议转换显式**：只有明确 schema 对才转换；`responses`/`chat` 协议对 Codex/OpenCodex **原样透传**，不重写 model。
7. **只读跟随**：cc-switch 变化通过只读轮询/观测进入 Observed 层；授权边界 = **app 级**（agent_id），已授权 app 内 provider/model/endpoint 变化自动跟随（P0.0.10.6），不要求再次确认；未授权 app 或 identity 变化需重新授权（见 §9 冲突 C1 处理）。
8. **种子离线 vendor**：`@opencode-ai/models/snapshot`（或 models.dev api.json）**固定版本 + commit hash** 随包分发；更新必须显式用户确认后下载替换，禁止静默后台更新。

## 4. 架构分层

```text
Agent 请求（127.0.0.1 listener，dtoken-core-api 入口）
  │
  ▼
┌─────────────────────────────────────────────────────┐
│ 1. Model Identity Resolver（接入 core-api 请求管线）  │
│    从 body/path 提取 model；解析失败语义见 §6.1       │
│    从 listener 身份解析 agent_id → 配置来源          │
└──────────────────────┬──────────────────────────────┘
                       ▼
┌─────────────────────────────────────────────────────┐
│ 2. Runtime Capability Registry（新 crate）           │
│    精确 → 显式通配 pattern → seed 继承 → 未知推断     │
│    输出 CapabilitySummary（dtoken-types 纯数据）      │
└──────────────────────┬──────────────────────────────┘
                       ▼
┌─────────────────────────────────────────────────────┐
│ 3. Upstream Chain Resolver（三类运行模型）            │
│    agent_managed_auth / dynamic_local_proxy /        │
│    cc_switch_managed（app 级授权边界）→ 唯一上游链    │
│    无法唯一解析 → fail-closed（可诊断错误+来源展示）  │
└──────────────────────┬──────────────────────────────┘
                       ▼
┌─────────────────────────────────────────────────────┐
│ 4. Protocol Adapter                                  │
│    schema 对 translator（显式）；responses/chat 透传  │
└──────────────────────┬──────────────────────────────┘
                       ▼
┌─────────────────────────────────────────────────────┐
│ 5. Compression Strategy Selector                     │
│    capability 已知 → 精确策略；未知 → 默认档+unknown  │
│    内容类型维度叠加（现有 dtoken-compress 保留）      │
│    CompressionStrategy → CompressionPolicy 映射       │
└──────────────────────┬──────────────────────────────┘
                       ▼
┌─────────────────────────────────────────────────────┐
│ 6. Adaptive Router（decide 扩展，旧签名保留）         │
│    不静默 fallback；cooldown 内存态（如启用）         │
└──────────────────────┬──────────────────────────────┘
                       ▼
                   upstream
                       │
┌──────────────────────▼──────────────────────────────┐
│ 7. Receipt / Event Ledger（持久化 schema 见 §8）      │
│    request_id 原子绑定；capability+strategy+route+    │
│    evidence；unknown/inferred 触发 learned 沉淀       │
└─────────────────────────────────────────────────────┘
```

## 5. 数据模型（Rust，含依赖边界修正）

### 5.1 依赖方向（修复 arch-review 阻断 1）

```text
dtoken-types（纯数据：Protocol 扩展、CapabilitySummary、CompressionStrategy、Receipt 结构）
   ▲          ▲          ▲
registry      router      compress（映射函数）
(dtoken-capability-registry：依赖 dtoken-types 与本地 store，不依赖 router/compress)
```

- `RouteDecision` 保持定义在 `dtoken-types`；新增字段类型 `CapabilitySummary` 也定义在 `dtoken-types`（纯数据，零依赖）→ **无循环**。
- `dtoken-capability-registry` 只依赖 `dtoken-types` + 本地 store（rusqlite/serde_json）+ seed 数据；不依赖 router/compress/config-patcher。
- `dtoken-compress` 增加 `CompressionStrategy → CompressionPolicy` 映射函数（strategy 枚举在 types，policy 在 compress 已有）；registry 不直接调用 compress。
- `dtoken-router` 依赖 registry（把 CapabilitySummary 填进 RouteDecision），保持对旧 `decide` 签名与测试的兼容。

### 5.2 类型

```rust
// dtoken-types（扩展，非破坏）
pub enum Protocol { OpenAi, Anthropic, Responses, Gemini, /* 现有扩展 */ }

pub enum CapabilitySource { Seed, Observed, Learned, Inferred }
pub enum Modality { Text, Image, Audio, Pdf, File }

/// 压缩策略（能力维度，与 dtoken-compress 的 CompressionPolicy 映射）
pub enum CompressionStrategy {
    SemanticV2, SlidingWindow, FrozenFloor, ToolSchema, Default,
}
impl CompressionStrategy {
    /// 映射到现有 CompressionPolicy（dtoken-compress 实现）
    pub fn to_policy(self) -> CompressionPolicy { /* 由 compress 提供 impl */ }
}

/// RouteDecision 的 capability 投影（纯数据，脱敏）
pub struct CapabilitySummary {
    pub model_id: String,                       // 请求原始 model 名
    pub canonical_id: Option<String>,           // seed 继承解析后的规范化 id
    pub provider: Option<String>,
    pub protocol: Option<Protocol>,
    pub context_window: Option<u64>,
    pub max_output_tokens: Option<u64>,
    pub supports_tool_call: Option<bool>,
    pub supports_stream: Option<bool>,
    pub supports_reasoning: Option<bool>,
    pub supports_structured_output: Option<bool>,
    pub fallback_strategy: FallbackStrategy,    // P0.0.11.3 要求；见 §5.3
    pub compression_strategy: CompressionStrategy,
    pub source: CapabilitySource,
    pub revision: u64,
    pub unknown: bool,                          // true = 推断/未知，压缩走默认档
}

/// 每字段证据（Observed/Learned 层用；Seed 层 None）
pub struct FieldEvidence {
    pub source: CapabilitySource,
    pub confidence: f32,                        // 0..1
    pub observed_request_ids: Vec<String>,      // 引用的真实请求（脱敏 id）
}

// RouteDecision（dtoken-types 扩展字段）
pub struct RouteDecision {
    // …现有字段不变…
    pub model_identity: Option<String>,
    pub capability: Option<CapabilitySummary>,  // 脱敏投影
    pub route_reason: String,                   // 解析链原因
}
```

### 5.3 FallbackStrategy（P0.0.11.3 补全）

```rust
pub enum FallbackStrategy {
    None,                       // 不 fallback（默认；d-token 不静默 fallback）
    SameAgentRetry,             // 同 agent 同上游重试（限次）
    ExplicitChain,              // 显式声明的上游链（唯一授权集合内）
}
```
约束：`ExplicitChain` 只能在已授权 app 的 cc-switch identity 集合内；跨 Provider 静默 fallback 仍被 P0.0.9 禁止。默认 `None`。

### 5.4 Registry（dtoken-capability-registry crate）

```rust
pub struct CapabilityRegistry {
    seed: SeedStore,            // vendored 快照（只读，pin 版本+commit）
    observed: LocalStore,       // 请求观测（字段级证据+confidence）
    learned: LocalStore,        // 推断沉淀
}

impl CapabilityRegistry {
    pub fn resolve(&self, model: &str, ctx: &RequestIdentity) -> ResolveOutcome;
    // Known(cap) | PatternHit(cap, pattern) | Inferred(cap) | Unknown(cap_default)
    pub fn record_observation(&mut self, req: &RequestObservation) -> Result<u64>; // CAS，返回新 revision
    pub fn reconcile_seed(&mut self, new_seed: SeedSnapshot) -> Result<u64>;       // 显式更新
}
```

**容量治理（修复「无界」）**：
- 观测/learned 每模型保留最近 N=100 条证据，LRU 淘汰；模型条目上限 5000（可配置）。
- 未观测 TTL：learned 90 天无命中降级为 seed 兜底；observed 30 天未命中清理。
- 启动加载预算：registry 快照加载 ≤ 200ms；按需分页，不整表入内存。
- 写放大控制：请求观测异步批量提交（≤ 50ms 延迟），每 revision 只写变更字段。

**持久化（修复 Instant）**：时间戳一律 `u64 unix_ms`；`learned_at` 用该类型。

**并发与崩溃（修复 arch-review 阻断 4）**：
- 写 CAS：`record_observation` 带 `expected_revision`，冲突返回 `RevisionConflict` 由调用方重读合并。
- 崩溃安全：先写 journal（WAL）再 commit；启动时回放未完成 journal。
- 旧 revision 拒绝：观察事件若 `observed_at < 已提交 revision 的 observed_at` 则丢弃（防乱序）。

## 6. 每请求管线（修复 Router::decide 契约）

### 6.1 Identity Resolver 解析失败语义

| 场景 | 行为 |
|---|---|
| body/path 缺失 model | 400 可诊断错误（Anthropic `/v1/messages` 必填；OpenAI 系缺失同样拒绝） |
| model 非字符串 | 400（类型错误，不猜测） |
| body.model 与 path 冲突 | body 优先 + ledger 记录 `model_ambiguity`（不静默二选一） |
| 协议无法识别 | fail-closed：按 listener 路径 + 内容头判定；未知 → 406 可诊断错误 |

### 6.2 管线

```text
1. Identity Resolver（接入 dtoken-core-api 请求入口：/v1/responses、/v1/chat/completions、
   /v1/messages）：model = body.model 或 path；agent_id 由 listener 身份解析；
   来源 = 该 agent 当前配置解析结果。
2. Registry.resolve(model)：
   a. 精确命中 → Known
   b. 显式通配 pattern（agent 配置/跟随生成）→ PatternHit（真实 model 名透传）
   c. seed 继承（base_model 链）→ Known（source=Seed）
   d. 未命中 → Inferred（协议来自上游链声明 + agent 类型；压缩=Default；unknown=true）
3. Upstream Chain Resolver：
   - agent_managed_auth：agent 持有凭据 → d-token 只代理透传，不读凭据；model 变化从
     请求观测（Observed 层记录，不读 Agent 配置）
   - dynamic_local_proxy：ocxlocal 等 → 固定链 + model 原样透传（R2.D）
   - cc_switch_managed：cc-switch is_current → 只读镜像（R2.C）；app 级授权边界内自动跟随
   - 多候选/字段不足/凭据不可复用 → fail-closed：可诊断错误 + 来源 +「修复来源配置/重新扫描」
4. Protocol Adapter：schema 对存在才转换；responses/chat 原样透传
5. Compression Selector：capability 已知 → 精确策略；未知 → Default + unknown 标记
   （不阻塞请求；映射 CompressionStrategy → CompressionPolicy 由 dtoken-compress 提供）
6. Adaptive Router：
   - 保留 `decide(instance_id, input_protocol)` 原签名（现有集成测试 tests/route_decision.rs
     不动）；新增 `decide_with_model(instance_id, input_protocol, model, capability)`，
     旧方法委托新方法（model=None）。
   - RouteDecision 增加 model_identity/capability/route_reason。
   - 不静默 fallback；cooldown 内存态（如启用）。
7. Receipt：全部绑定 request_id 原子写入 ledger；unknown/inferred 触发 learned 沉淀。
```

## 7. Registry 更新策略

| 层 | 来源 | 写入 | 更新触发 |
|---|---|---|---|
| Seed | `@opencode-ai/models/snapshot`（pin 版本 + commit hash，vendored） | 随包只读 | 显式用户确认后 `reconcile_seed` 替换 |
| Observed | 真实请求观测：成功/失败特征、token 用量、协议行为 | 本地 d-token-owned store（WAL journal + CAS） | 每请求后异步批量回写；每字段带 confidence + 观测请求引用；置信度 < 0.6 不覆盖已知 |
| Learned | 未知模型推断沉淀 | 同上 | 首次推断写入；后续同 model 请求的观测确认后升级（source=Learned，保留原推断证据链） |

约束：registry 只写 d-token-owned 本地 store；永不写 Agent 配置文件、cc-switch DB、外部服务；每次更新 revision++ 并留 ledger 审计；时间戳 u64 unix_ms。

## 8. Receipt / Evidence 契约（修复 arch-review 阻断 4）

```rust
// dtoken-types（持久化 schema）
pub struct CapabilityReceipt {
    pub request_id: String,                 // 与 RouteDecision.request_id 同一 id（原子绑定）
    pub agent_id: String,
    pub model_identity: String,
    pub canonical_id: Option<String>,
    pub capability: CapabilitySummary,      // 含 source/revision/unknown/fallback_strategy
    pub compression: CompressionPolicy,     // 实际采用
    pub route: RouteReceiptProjection,      // provider/endpoint/protocol/chain_mode（脱敏）
    pub ledger_evidence: Vec<String>,       // Event Ledger event ids
    pub created_at_ms: u64,
    pub redaction: RedactionScope,          // 脱敏声明（无凭据/无 prompt/无响应）
}
```

- 持久化：ledger 表（`capability_receipts(request_id PRIMARY KEY, payload JSON, created_at_ms)`），与现有 Event Ledger 同一事务提交，保证 request_id 原子绑定。
- 脱敏边界：receipt 只含 model identity、能力字段、策略、route 投影；**不含** prompt/response/凭据/token/完整配置。
- learned 升级：升级动作本身产生 ledger 事件（`capability_learned`），绑定证据请求 id。

## 9. 三类上游链解析与授权边界（修复 arch-review 阻断 2）

| 运行模型 | 解析规则 | 授权边界 | 免重配保证 |
|---|---|---|---|
| agent_managed_auth | agent 持有登录/API Key；d-token 透传，只读观测请求 model | 首次授权该 agent | 模型切换在 agent 侧，d-token 始终透传 |
| dynamic_local_proxy | 固定接入本地代理链；请求 model 原样透传 | 首次授权该 agent | 通配 pattern `agent/*` 由首次配置生成 |
| cc_switch_managed | cc-switch is_current 只读跟随 | **app 级授权（agent_id）** | 已授权 app 内 provider/model/endpoint 变化自动跟随（P0.0.10.6） |

**冲突 C1 处理（必须与 R2.C 一起落实）**：现有 `follow_decision` 按 upstream signature（provider+source+protocol+endpoint）判 `ReauthorizationRequired`，粒度比合同的 app 级授权更细。按 P0.0.10.6「同一已授权 app 的 Provider/model/protocol/endpoint/profile 变化后自动更新，不再弹第二次确认」，R2.C 必须把授权边界改为 app 级（agent_id），已授权 app 内 identity 变化 → `SyncRequired` 自动跟随；未授权 app → 拒绝并提示授权。**E3 验收场景「新增 Provider B 自动走 B」成立的前提是 R2.C 完成该语义调整**；未完成前 B 属于未授权 identity，按现状走 ReauthorizationRequired（fail-closed，不静默）。

## 10. 压缩策略选择矩阵

| 能力已知度 | context_window | tool_call | 策略 | 备注 |
|---|---|---|---|---|
| 已知 | 大/中 | 支持 | semantic-v2 / sliding / frozen floor / tool-schema 按内容类型叠加 | 现有内容维度保留 |
| 已知 | 小 | 支持 | frozen floor + tool-schema | 保守 |
| 未知 | — | — | Default（最保守无损档）+ `unknown=true` | 不失败；ledger 标记 |
| 明确不支持 tool_call | — | 否 | 跳过 tool-schema，只内容折叠 | 能力来自 seed/observed |

## 11. 分层失败语义（修复 arch-review 阻断 3）

- **路由层 fail-closed（永远先于请求完成）**：无法唯一解析上游 → 请求**拒绝**，返回可诊断错误（含来源与影响），UI 显示「配置失败 / 修复来源配置 / 重新扫描」；**绝不静默猜测、绝不静默 fallback**。
- **压缩层永不失败**：capability 未知 → 默认策略 + unknown 标记。但「压缩不失败」**不等于「请求必然完成」**——请求完成的前提是路由层已唯一解析上游。
- 明确表述：未知模型请求**只有在路由层可唯一解析时才完成**（透传链场景下成立）；路由层无法解析时被拒，与压缩无关。

## 12. 与现有代码映射（修复 arch-review 阻断 5 + 接入点）

| 现有代码 | 扩展点 | 契约保护 |
|---|---|---|
| `dtoken-types` | 扩展 `Protocol`（加 Responses 等）；新增 `CapabilitySummary`/`CompressionStrategy`/`CapabilityReceipt`/`FieldEvidence`（纯数据） | 只加字段/枚举变体，不删不改旧字段 |
| `dtoken-router::Router` | 新增 `decide_with_model`；旧 `decide` 委托之（model=None 行为完全一致） | **保留 `decide(instance_id, input_protocol)` 签名**，`tests/route_decision.rs` 不动 |
| `dtoken-compress` | 新增 `CompressionStrategy::to_policy` 映射；`select` 增加可选 capability 输入（默认 None=现行为） | 旧 `select(text, budget)` 签名保留 |
| `dtoken-core-api` | 请求入口（/v1/responses、/v1/chat/completions、/v1/messages）接入 Identity Resolver | 新管线在现有 handler 前插桩，不改响应语义 |
| `upstream_discovery.rs` | 快照发现保持不变；新增「每请求 model identity 解析」出口供 core-api 调用 | 快照/发现契约不动 |
| `cc_switch.rs` | `follow_decision` 授权粒度调整为 app 级（R2.C 一并做） | 与 P0.0.10.6 对齐 |
| 新增 `dtoken-capability-registry` | 三层 registry + seed vendor + resolve/record/reconcile API | 独立 crate，依赖仅 dtoken-types + store |
| Event Ledger | `capability_receipts` 表 + `capability_learned` 事件 | 与现有事件同一事务 |

## 13. 落地切片（R2.E 细化）

- **R2.E1 registry**：新 crate 类型（CapabilitySummary/FieldEvidence/ResolveOutcome）、seed store（vendor `@opencode-ai/models/snapshot` 固定版本+commit）、observed/learned 本地 store（WAL + CAS + 容量治理）、resolve/record/reconcile；领域测试通过。
- **R2.E2 inference**：Identity Resolver（core-api 插桩 + 解析失败语义）、显式通配 pattern、未知模型推断（协议来自 chain、压缩=Default、unknown 标记）、Compression Selector 扩展、`decide_with_model`；测试钉住「未知模型压缩不失败」与「路由 fail-closed 不猜上游」与「旧 decide 签名兼容」。
- **R2.E3 acceptance**：真实请求链路——一次配置后新增模型 A / Provider B（需 R2.C 授权语义就绪）/ route C 均免重配；真实请求自动识别/匹配/压缩 + 完整 `CapabilityReceipt`。

## 14. 验收场景（P0.0.11.7 展开）

1. 新增模型 A：cc-switch 或 agent 侧新增 model（如新 thinking 变体）→ 下一次请求自动命中通配/推断 → 默认压缩 → receipt 含 unknown/inferred 标记。
2. 新增 Provider B：cc-switch 切到 B（R2.C 完成 app 级授权后自动跟随）→ 请求自动走 B 上游链 → 压缩按 B 能力评估；R2.C 未完成前 B=未授权 → fail-closed 提示授权（不静默）。
3. 修改 route C：用户改配置/cc-switch 改 profile → 只读跟随更新 → 请求自动匹配新 route。
4. 全程无重新配置、无重新发布；每次请求有完整 `CapabilityReceipt`。

## 15. 风险与取舍

| 风险 | 缓解 |
|---|---|
| Seed 数据陈旧/缺失模型 | seed 只作兜底；observed/learned 补偿；未知模型走默认策略不失败 |
| Observed 误判能力 | 每字段带 source+confidence+观测请求引用；置信度不足不覆盖；ledger 可回滚 |
| 通配 pattern 过宽误路由 | pattern 只由首次配置按 agent 实际配置生成；命中进 receipt reason；可审计 |
| R2.C 授权语义未就绪时 E3 场景 2 不成立 | 架构已声明依赖：E3 在 R2.C 完成后执行；未完成时场景 2 显式 fail-closed |
| 本地 store 膨胀 | 容量治理：模型上限/LRU/TTL/启动预算；只存 identity+能力字段不存 prompt |
| 旧契约破坏 | decide/select 旧签名保留；types 只加字段；core-api 插桩不改响应语义 |

## 16. 附录：对 arch-review 阻断的逐条响应

| # | 阻断 | v2 处理 |
|---|---|---|
| 1 | crate 依赖循环 | §5.1：CapabilitySummary/CompressionStrategy 放 dtoken-types 纯数据；registry 只依赖 types+store；router 依赖 registry 单向 |
| 2 | cc-switch 授权语义冲突 | §9：授权边界改 app 级（与 P0.0.10.6 一致），随 R2.C 落实；未授权 identity fail-closed |
| 3 | 未知模型「仍可完成」与路由 fail-closed 矛盾 | §11：明确「压缩层不失败 ≠ 请求必然完成」；路由唯一解析是前提 |
| 4 | receipt/evidence 契约缺失 | §8：完整持久化 schema + request_id 原子绑定 + 字段级证据/CAS + learned 升级事件 + 脱敏边界 |
| 5 | Router::decide 契约破坏 | §6.2/§12：旧签名保留，新增 decide_with_model 委托；tests 不动 |
