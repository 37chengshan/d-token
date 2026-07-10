# d-token Hybrid Context Optimization / Optical Codec Plan V7

更新日期：2026-07-10

## 0. 定位变化

V7 不再把 Optical Compression 定义为“把长文本转换成图片的独立压缩器”。

新的产品定位是：

```text
Hybrid Context Optimization Engine
混合上下文优化引擎
```

Optical 只是其中一种有损表示策略，与原样保留、缓存稳定化、无损结构压缩、语义文本压缩、原生上下文清理和外部检索并列。

核心问题从：

```text
这段文本能否变成图片？
```

改为：

```text
在不破坏任务正确性、安全性、协议结构和缓存收益的前提下，
这个上下文节点应采用哪一种表示策略？
```

## 1. 不可违反的产品边界

1. 默认关闭 active transform。
2. Shadow Preview 可以分析，但不得修改请求 body。
3. 默认 capability registry 不包含任何 active-supported 模型。
4. 只有显式注册并验证的 provider/model/endpoint rule 才可进入 active gate。
5. 当前用户输入、recent tail、system/developer instruction、开放中的 tool state 永远保留文本。
6. Secret-like 内容不得进入图片、sidecar、日志、事件或公开摘要。
7. Diff、patch、代码结构、schema、tool IDs 默认不可进行 Optical 转换。
8. 任何未知能力、缺失原文引用、几何缺失、预算超限或验证失败都 fail closed。
9. Public summary 只允许 metadata，不允许 raw text、request body、base64、图片字节或精确敏感值。
10. Phase D 之前禁止真实 request mutation。

## 2. 总体架构

```text
Raw Request
    ↓
Protocol Applicability Gate
    ↓
Versioned Capability Registry
    ↓
Request-local TextArena
    ↓
Conversation / Context Inventory
    ↓
Semantic Priority + Volatility + Cache Zones
    ↓
Exactness / Secret / Structural Risk Firewall
    ↓
Strategy Candidate Generator
    ↓
Geometry + Economics Simulation
    ↓
Immutable Transform Plan
    ↓
Structured Shadow Validator
    ↓
Public Metadata Summary
    ↓
[future] Active Validator → Renderer → Protocol Patcher → Post Validator
```

## 3. 核心模块

### 3.1 Applicability Gate

职责：只判断请求是否可以进入 shadow 分析或 active 候选阶段。

必须检查：

```text
HTTP method
endpoint path
protocol ↔ endpoint compatibility
non-empty body
global enabled
route enabled
request budget
active master switch
route mode
allow_tool_result
confirmed_experimental_lossy
```

Shadow 与 Active 必须分开：

```text
eligible_for_shadow = 可以进行只读分析
eligible_for_active = 通过全部显式 active 条件
```

Active 禁用不应阻止安全的 shadow preview。

### 3.2 Versioned Capability Registry

生产默认注册表为空：

```text
Unknown / Limited / ShadowOnly → 可以 shadow，不能 active
Supported → 仅来自显式验证规则
Blocked → shadow 和 active 都拒绝
```

规则键：

```text
provider family
provider id（可选）
endpoint
normalized model base
capability revision
verified timestamp
```

能力内容：

```text
image input format
max image count
max dimensions
vision token formula
exactness class
maximum optical mode
```

禁止通过模型名前缀自动授予 active。

### 3.3 Request-local TextArena

原文只存在于请求生命周期内：

```text
TextArena owns String
BlockRef owns TextRef
Public summary never owns TextRef or String
```

要求：

```text
TextArena / TextRef 不实现 Serialize
Debug 不打印原文
fingerprint 仅用于 request-local identity
persistent cache identity 必须使用独立稳定加密摘要
```

### 3.4 Context Inventory

当前 Phase A 实现 Anthropic messages inventory。

节点至少包含：

```text
block_id
JSON path / structural position
role
block kind
turn index
character count
initial protection
request-local TextRef
```

后续升级为 Context Graph 时增加：

```text
tool_use → tool_result dependency
summary → source dependency
cache zone
lifecycle
closed/open tool boundary
```

### 3.5 Risk Firewall

分类优先级：

```text
SecretNoDuplicate
StructuralUnsafe
BlockUnsafe
ExactTextRequired
RenderSafe
```

Secret 检测必须扫描整个 block，而不是只检查开头。

当前保守检测包括：

```text
Bearer / Authorization
Cookie / Set-Cookie
x-api-key / api_key / api-key / apikey
sk-* credential-like token
UUID
hash
Unix / Windows path
diff headers / hunks
```

规则：

```text
Secret → 整块跳过，不生成 sidecar
Diff/Patch → 整块跳过
密集 exact anchors → 整块跳过
少量 exact anchors → Phase A 仍保留文本；未来可使用安全 anchor sidecar
```

### 3.6 Strategy Portfolio

未来统一策略枚举：

```text
PreserveText
CacheStabilize
StructuralMinify
Deduplicate
NativeContextClear
SemanticTextCompress
ExtractiveSummary
OpticalCapsule
OpticalCapsuleWithAnchors
ExternalizeWithRetrieval
DropAfterConsumption
```

策略优先级：

```text
Preserve + Cache
→ Provider Native Context Management
→ Lossless Structural Compression
→ Semantic Text Compression
→ Optical Capsule
→ External Retrieval
```

Optical 不是默认第一选择。

### 3.7 Render Geometry

Geometry 必须成为 estimator、renderer 和 UI 的单一真理源。

当前字段：

```text
width / max height
padding
cell width / height
header rows
cols / readable rows
readable chars per image
pixels per token
safety margin
```

要求：

```text
容量由几何字段推导
尺寸受 capability 上限裁剪
token 公式来自 capability rule
估算使用向上取整和安全余量
非法公式返回不可用，不允许静默除零
```

当前 `anthropic_dense_v1` 只是实验 profile，不代表任何 provider 的永久合同。

### 3.8 Economics Planner

经济性只统计最终被选为 shadow/active candidate 的节点。

不得把以下节点计入预计收益：

```text
secret skipped
structural skipped
exactness skipped
preserved recent tail
unsupported model
image budget exceeded
```

基础模型：

```text
net_saved = text_tokens_before
          - image_tokens_after
          - sidecar_tokens
          - cache_burn_tokens
          - output_risk_tokens
```

未来 V7 优化：

```text
EstimateRange instead of scalar
P10 / expected / P90
preprocessing latency cost
provider cache read/write cost
quality risk cost
recovery risk cost
```

Active 使用保守下界，而不是只看期望值。

### 3.9 Immutable Transform Plan

计划应包含真实 evidence，而不是由调用方传入“安全=true”的布尔声明。

Shadow validator 当前已读取：

```text
original body
planned body
public summary
candidate action/reason evidence
current user protection
recent tail protection
geometry sufficiency
```

Active validator 当前已读取：

```text
global and route policies
capability
cache stability
secret sidecar duplication
patch path membership
image budget
minimum net savings
```

后续内部 Plan IR 应增加：

```text
request fingerprint
capability snapshot id
context graph summary
selected actions
patch operations
rollback journal reference
post-execution evidence
```

## 4. Optical Capsule 合同（Phase D）

Optical 不直接用一张无说明图片替换文本，而应形成自描述 capsule：

```text
tool_result
├── metadata intro block
├── image page 1
├── image page 2
└── optional safe exact-anchor sidecar
```

必须保留：

```text
tool_use_id
tool_result type
role
message order
source candidate path
page order
```

禁止包含：

```text
secret value
raw source in metadata
base64 in logs/events
unbounded exact sidecar
```

## 5. 分阶段实施

### Phase A — Plan Foundation（已完成并优化）

```text
public types and policies
capability registry
applicability gate
request-local TextArena
inventory
risk firewall
geometry estimator
economics
shadow plan
structured validator
```

### Phase B — API Visibility（基础端点已完成）

```text
POST /compression/optical/preview
GET  /compression/optical/status
GET  /compression/optical/capabilities
```

仍需：

```text
events/recent
config endpoint
route mode endpoint
real route policy loading
capability snapshot management
```

### Phase C — Desktop Optical Lab（未完成）

只展示 public summary：

```text
master status
applicability
capability
candidate decisions
risk skips
economics
cache impact
validation gates
```

### Phase D — Renderer and Patcher（未完成）

顺序：

```text
reflow_anchor
paging_budget
renderer behind feature flag
Anthropic tool_result patcher
post-execution validator
rollback journal
```

第一版 active 只允许：

```text
Anthropic /v1/messages
closed historical tool_result
verified capability registry rule
dense RenderSafe log/JSON/table
explicit route confirmation
```

### Phase E — History and Cache Stability（未完成）

```text
closed tool boundary
frozen stable prefix
cache burn planner
history amortized economics
latest collapsed user pointer
```

### Phase F — Calibration（未完成）

只记录 metadata：

```text
estimated vs actual tokens
cache usage
latency
strategy/model/content kind
quality outcome
```

禁止存 raw text 或 image bytes。

## 6. Release Gates

任何 active release 必须满足：

```text
Default registry grants no active model.
Shadow body is byte/structure unchanged.
Current user and recent tail stay text.
System/developer/tool schema stay unchanged.
Open tool state cannot collapse.
Secret-like content is never imaged or sidecar duplicated.
Unknown/blocked/shadow-only model cannot active-transform.
Every selected action has a non-empty reason.
Geometry comes from a verified capability snapshot.
Selected economics exceeds route threshold under conservative estimate.
Image count is within global and model limits.
Patch paths match approved candidates.
Post-patch protocol structure validates.
Any failure returns original body.
Logs/events/exports contain no raw text or base64.
```

## 7. 当前算法局限

当前实现仍属于 shadow control plane，不是生产 active renderer：

1. Economics 仍使用启发式 token 比例和固定 cache rates，尚无 P10/Expected/P90 区间。
2. Limited capability 的 fallback geometry 只允许 shadow estimate。
3. Risk detector 是保守启发式，不替代专用 secret scanner。
4. ContextGraph 已支持 AnthropicMessagesV1 和 OpenAiChatV1，但尚未覆盖 OpenAI Responses、cache/schema/summary 边和完整异常序列。
5. Raw-byte 与 Canonical JSON SHA-256 v1 已实现；Preview 目前仍使用 canonical fallback，HTTP ingress 未保留原始 bytes。
6. Canonical JSON v1 是内部规则，不宣称 RFC 8785/JCS 兼容。
7. Capability Snapshot 已支持 schema v1、Active/Revoked/Superseded 和 revision barrier，但缺 authority、signature、trust chain 与管理 API。
8. Registry lifecycle tombstone 目前由批量 `from_snapshots()` 建立；active registration 与 lifecycle ingest 尚未拆分。
9. Preview Event 已包含 graph/economics/capability aggregates，但仍必须保持 metadata-only。
10. Schema slimmer、volatility、cache planner 尚未全部接入统一 Plan IR。
11. 没有 renderer、patcher、post validator 和 rollback executor。
12. Pending commit ledger 已升级 schema v2，但 Python 标准库测试受当前工具白名单阻塞。

## 8. 2026-07-11 实施进展

```text
Versioned capability config                       completed
Active/Revoked/Superseded lifecycle               completed
Revocation revision barrier                       completed
Duplicate snapshot ID fail-closed                 completed
Legacy config safe default                        completed
AnthropicMessagesV1 state machine                 completed
OpenAiChatV1 inventory/graph lowering             completed, shadow-only
RawRequestBytes SHA-256 v1                        completed as internal API
CanonicalJson SHA-256 v1                          completed and tested
Configured preview/status/capabilities            completed
Capability lifecycle summary endpoint             completed
Expanded metadata-only OpticalPreview events      completed
Pending commit ledger schema v2                   completed, execution test blocked
Full cargo test --workspace                       passed
Clippy -D warnings                                passed
```

阶段提交状态：

```text
support/pending-commits.json
```

账本不设置目标分支，只记录阶段写入时实际 branch、时间、文件、验证和提交状态。

真实 Git 操作审计仍使用：

```text
support/git-operations.json
```

## 9. 下一实施顺序

```text
1. Run pending-ledger unittest in a permitted Python environment
2. Capability authority/signature/trust source and revocation audit
3. Split lifecycle ingest from active capability registration
4. Wire HTTP ingress raw bytes into Plan evidence
5. Add canonical number/Unicode/cross-language fixtures
6. Add provider malformed/adversarial state-machine fixtures
7. Integrate volatility/schema/cache planners into Plan IR
8. Replace scalar economics with P10/Expected/P90 ranges
9. Desktop Optical Lab and fidelity fixture suite
10. Renderer + Anthropic patcher + post validator + rollback
```

## 10. 参考文档

```text
docs/experiments/OPTICAL_COMPRESSION_EXPERIMENT_PLAN.md       V6 基础规格
docs/optical/architecture-v2-iteration.md                     V7 架构说明
docs/optical/review-round-5.md                                 最新深度审查
docs/optical/implementation-status.md                          最新状态矩阵
docs/optical/test-matrix.md                                    最新测试合同
docs/optical/release-gates.md                                  最新发布门槛
support/pending-commits.json                                   分阶段改动/提交状态
```
