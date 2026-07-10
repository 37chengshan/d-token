# d-token Optical Compression V6 Core Algorithm & Framework Plan

> 目标：把视觉压缩做成 d-token 自有的、可验证的、默认安全的计划层能力。本文只保留核心算法、框架、数据结构、接口合同、测试门槛和实施顺序。

## 0. 产品边界

视觉压缩不是普通文本压缩，也不是直接把 prompt 画成图片。它是一个面向 AI Coding 长上下文的计划层系统：先判断哪些内容可以安全视觉化，再判断是否经济划算，最后才允许渲染和协议改写。

默认行为：

```text
默认关闭。
第一阶段只做 shadow plan，不改写请求。
第一批 active 只允许 Anthropic /v1/messages 的大块 tool_result。
current user turn、recent tail、secret-like、schema validator、open tool-call、diff/patch 默认保持文本。
unknown model、unknown provider、unknown endpoint 只能 shadow 或 pass-through。
```

用户可见承诺：

```text
d-token 可以把低精确性、稳定、过长、非当前任务核心的上下文块规划为视觉候选，并用 metadata 展示每个候选为什么可压、为什么跳过、预计节省多少、是否会破坏缓存稳定性。
```

禁止承诺：

```text
无损压缩。
保证省钱。
所有模型都支持。
hash、路径、命令、ID 可以只靠图片读取。
可以绕过 provider 规则或计费模型。
```

## 1. 总体架构

V6 的核心是 `OpticalTransformPlan`。任何真实请求改写都必须由它驱动。

```text
Incoming request
↓
ApplicabilityGate
↓
ProtocolLowering
↓
CapabilityResolver
↓
VolatilityPlanner
↓
SchemaSlimPlanner
↓
ConversationInventory
↓
SemanticPriorityPlanner
↓
ClosedToolBoundaryPlanner
↓
CacheStableBoundaryPlanner
↓
RiskPartitionPlanner
↓
ReflowAnchorPlanner
↓
PagingBudgetPlanner
↓
EconomicsPlanner
↓
OpticalTransformPlan
↓
PlanValidator
↓
Shadow metadata OR Active renderer/patcher
```

核心原则：

```text
1. Planner owns decisions. Renderer never decides eligibility.
2. Shadow plan must preserve body unchanged.
3. Active plan must pass validation before renderer runs.
4. Public API only returns summaries, never raw prompt/image/base64/exact values.
5. Estimator、renderer、UI 必须共用同一个 RenderGeometryProfile。
6. Cache-unstable candidates cannot active-transform.
7. Unknown model cannot active-transform.
8. Every candidate must end with exactly one CandidateAction and one CandidateReason.
9. Every failure path must fail closed to original body.
```

## 2. Module Framework

```text
code/crates/dtoken-compress/src/optical/mod.rs
code/crates/dtoken-compress/src/optical/applicability.rs
code/crates/dtoken-compress/src/optical/capability.rs
code/crates/dtoken-compress/src/optical/render_geometry.rs
code/crates/dtoken-compress/src/optical/volatility.rs
code/crates/dtoken-compress/src/optical/schema_slimmer.rs
code/crates/dtoken-compress/src/optical/inventory.rs
code/crates/dtoken-compress/src/optical/semantic_priority.rs
code/crates/dtoken-compress/src/optical/history_boundary.rs
code/crates/dtoken-compress/src/optical/cache_stability.rs
code/crates/dtoken-compress/src/optical/risk.rs
code/crates/dtoken-compress/src/optical/reflow_anchor.rs
code/crates/dtoken-compress/src/optical/paging_budget.rs
code/crates/dtoken-compress/src/optical/economics.rs
code/crates/dtoken-compress/src/optical/decision.rs
code/crates/dtoken-compress/src/optical/validate.rs
code/crates/dtoken-compress/src/optical/render.rs
code/crates/dtoken-compress/src/optical/patch.rs
code/crates/dtoken-compress/src/optical/calibration.rs
code/crates/dtoken-compress/src/optical/telemetry.rs
```

依赖分层：

```text
Phase A shadow-only:
- serde
- serde_json
- regex
- sha2

Phase B renderer feature:
- image png
- fontdue
- base64
- unicode-width
```

规则：

```text
默认 build 不启用 renderer。
shadow mode 不依赖 image/fontdue/base64。
renderer fail closed，不影响原始请求。
```

## 3. Configuration Schema

### 3.1 Global config

```rust
pub struct OpticalGlobalPolicy {
    pub enabled: bool,
    pub default_mode: OpticalMode,
    pub allow_active: bool,
    pub max_images_per_request: usize,
    pub max_candidate_chars_per_request: usize,
    pub max_plan_time_ms: u64,
    pub max_render_time_ms: u64,
    pub public_summary_only: bool,
}
```

Defaults:

```text
enabled = false
default_mode = shadow_preview
allow_active = false
max_images_per_request = 10
max_candidate_chars_per_request = 512000
max_plan_time_ms = 50
max_render_time_ms = 1500
public_summary_only = true
```

### 3.2 Route config

```rust
pub struct RouteOpticalPolicy {
    pub enabled: bool,
    pub mode: OpticalMode,
    pub allow_tool_result: bool,
    pub allow_history: bool,
    pub allow_schema_annotations: bool,
    pub allow_system_static_slab: bool,
    pub min_net_saved_tokens: isize,
    pub confirmed_experimental_lossy: bool,
}
```

Defaults:

```text
enabled = false
mode = off
allow_tool_result = false
allow_history = false
allow_schema_annotations = false
allow_system_static_slab = false
min_net_saved_tokens = 1024
confirmed_experimental_lossy = false
```

Compatibility rule:

```text
Existing config without optical fields must deserialize with all optical settings disabled.
```

## 4. CapabilityResolver

### 4.1 Responsibility

CapabilityResolver turns provider/model/endpoint facts into a strict runtime capability object. Active transform is impossible without a positive capability match.

```rust
pub struct OpticalModelCapability {
    pub provider_family: ProviderFamily,
    pub provider_id: Option<String>,
    pub model_base: Option<String>,
    pub endpoint: OpticalEndpoint,
    pub status: CapabilityStatus,
    pub max_mode: OpticalMode,
    pub supports_image_input: bool,
    pub supports_anthropic_image_block: bool,
    pub supports_openai_image_url: bool,
    pub supports_openai_input_image: bool,
    pub supports_data_url: bool,
    pub max_images_per_request: usize,
    pub max_image_width_px: usize,
    pub max_image_height_px: usize,
    pub vision_token_formula: VisionTokenFormula,
    pub exactness_class: VisionExactnessClass,
}

pub enum CapabilityStatus {
    Supported,
    Limited,
    ShadowOnly,
    Blocked,
    Unknown,
}

pub enum VisionExactnessClass {
    SummaryOnly,
    TechnicalReadable,
    ExactStringUnsafe,
    Unknown,
}
```

### 4.2 Active gate

```text
Supported + supports_image_input + endpoint-specific image support + exactness_class != Unknown required.
Limited can only run shadow unless route explicitly allows limited active.
ShadowOnly, Blocked, Unknown cannot active-transform.
```

## 5. ApplicabilityGate

### 5.1 Responsibility

ApplicabilityGate runs before deep parsing. It decides whether a request can enter the optical planner.

```rust
pub struct OpticalApplicabilityInput {
    pub method: Option<String>,
    pub endpoint_path: Option<String>,
    pub body_bytes: Option<usize>,
    pub protocol: Option<Protocol>,
    pub provider_id: Option<String>,
    pub route_id: Option<String>,
    pub model: Option<String>,
    pub global_enabled: bool,
    pub route_enabled: bool,
}

pub struct OpticalApplicabilityDecision {
    pub eligible_for_shadow: bool,
    pub eligible_for_active: bool,
    pub reason: OpticalApplicabilityReason,
    pub normalized_model_base: Option<String>,
    pub endpoint: Option<OpticalEndpoint>,
}

pub enum OpticalApplicabilityReason {
    Eligible,
    GlobalDisabled,
    RouteDisabled,
    UnsupportedMethod,
    UnsupportedPath,
    EmptyBody,
    UnsupportedProtocol,
    UnsupportedModel,
    UnknownModelShadowOnly,
    OverBudget,
}
```

### 5.2 Algorithm

```text
1. If body_bytes is 0 or missing: ineligible.
2. If body_bytes exceeds max_candidate_chars_per_request hard budget: shadow summary only or pass-through.
3. If method is not POST for live transform endpoints: ineligible.
4. Resolve exact endpoint:
   - AnthropicMessages: /v1/messages
   - OpenAiChatCompletions: /v1/chat/completions
   - OpenAiResponses: /v1/responses
5. Normalize model base by removing known bracketed variant suffixes.
6. Resolve provider/model capability.
7. If model unknown: allow shadow, block active.
8. If global or route disabled: allow explicit preview only, block live path.
```

## 6. RenderGeometryProfile

### 6.1 Responsibility

RenderGeometryProfile is the only source for image geometry, image token estimate, paging capacity, renderer dimensions, and UI preview.

```rust
pub struct RenderGeometryProfile {
    pub profile_id: String,
    pub provider_family: ProviderFamily,
    pub width_px: usize,
    pub max_height_px: usize,
    pub pad_x: usize,
    pub pad_y: usize,
    pub header_rows: usize,
    pub cell_w: usize,
    pub cell_h: usize,
    pub cols: usize,
    pub readable_rows: usize,
    pub readable_chars_per_image: usize,
    pub pixels_per_vision_token: f64,
    pub image_cost_safety_margin: f64,
}
```

Default profile:

```text
profile_id = anthropic_dense_v1
width_px = 1568
max_height_px = 728
cell_w = 5
cell_h = 8
cols ≈ 312
readable_chars_per_image ≈ 28,000 before header/wrapping loss
pixels_per_vision_token = 750.0
image_cost_safety_margin = 1.20
```

Rules:

```text
EconomicsPlanner reads RenderGeometryProfile.
PagingBudgetPlanner reads RenderGeometryProfile.
Renderer reads RenderGeometryProfile.
UI displays backend profile summary.
No separate hard-coded geometry constants in estimator, renderer, or UI.
```

## 7. TextRef and Raw Text Containment

### 7.1 Responsibility

Internal planners need access to source text, but raw text must not leak into public summaries, logs, exports, debug strings, or persistent events.

```rust
pub struct TextRef {
    id: TextRefId,
    len_chars: usize,
    len_bytes: usize,
    digest: String,
}

pub struct TextArena {
    request_id: String,
    entries: Vec<TextArenaEntry>,
}

pub struct TextArenaEntry {
    id: TextRefId,
    text: String,
    sensitivity: TextSensitivity,
    expires_at_ms: Option<u64>,
}

pub enum TextSensitivity {
    NormalInternal,
    ExactInternal,
    SecretInternal,
    RenderCandidateInternal,
}
```

Hard rules:

```text
TextRef is not Serialize.
TextArena is not Serialize.
TextArena is not written to Event Ledger.
TextArena is dropped after request processing unless explicit ephemeral recovery is enabled.
Debug output for TextRef shows only id, len, digest prefix, sensitivity.
SecretInternal entries cannot be sidecar duplicated.
```

## 8. Bucket Policy

### 8.1 Responsibility

Different content kinds have different compression value and risk. A single global char threshold is not enough.

```rust
pub struct OpticalBucketThresholds {
    pub min_total_candidate_chars: usize,
    pub min_static_slab_chars: usize,
    pub min_reminder_chars: usize,
    pub min_tool_result_chars: usize,
    pub min_history_chars: usize,
    pub min_history_tokens: usize,
    pub max_images_per_tool_result: usize,
    pub max_images_per_history: usize,
}

pub enum OpticalBucket {
    StaticSlab,
    Reminder,
    ToolResultJson,
    ToolResultLog,
    ToolResultProse,
    ToolResultTable,
    AnthropicHistory,
    GptHistory,
    SchemaAnnotation,
}
```

Defaults:

```text
min_total_candidate_chars = 2000
min_static_slab_chars = 6000
min_reminder_chars = 6000
min_tool_result_chars = 6000
min_history_chars = 12000
min_history_tokens = 2000
max_images_per_tool_result = 10
max_images_per_history = 16
```

## 9. VolatilityPlanner

### 9.1 Responsibility

VolatilityPlanner removes per-turn changing content from visual candidates to avoid prompt-cache churn.

```rust
pub struct VolatilityPlan {
    pub stable_segments: Vec<StableSegment>,
    pub volatile_segments: Vec<VolatileSegment>,
    pub stable_digest: Option<String>,
    pub volatile_chars: usize,
    pub churning_static_tags: Vec<String>,
}

pub enum VolatileKind {
    BillingHeader,
    EnvironmentBlock,
    WorkingDirectory,
    GitStatus,
    GitBranch,
    PlatformInfo,
    CurrentDate,
    SessionHeader,
    UnknownPerTurnTag,
}

pub enum VolatilePolicy {
    KeepTextInPlace,
    RelocateToTailText,
    DropIfKnownRedundant,
}
```

Algorithm:

```text
1. Scan candidate slabs for known volatile headers and environment sections.
2. Split stable/volatile segments without changing original body.
3. Compute stable digest over stable segments only.
4. Mark volatile segments AlwaysText unless safe relocation is explicitly supported.
5. If stable digest changes repeatedly across adjacent turns, mark cache unstable.
```

## 10. SchemaSlimPlanner

### 10.1 Responsibility

SchemaSlimPlanner separates validator structure from annotation text. Validator structure stays text; annotation text is only a shadow/full-experimental candidate.

```rust
pub struct SchemaSlimPlan {
    pub tool_count: usize,
    pub schemas_considered: usize,
    pub schemas_slimmed: usize,
    pub annotations_moved_chars: usize,
    pub validator_chars_before: usize,
    pub validator_chars_after: usize,
    pub annotation_blocks: Vec<SchemaAnnotationBlock>,
    pub skipped: Vec<SchemaSlimSkip>,
}

pub enum SchemaAnnotationPolicy {
    KeepText,
    ShadowOnlyCandidate,
    FullExperimentalCandidate,
}
```

Preserve as validator text:

```text
required
enum
const
type
$ref
properties keys
patternProperties keys
definitions keys
$defs keys
items
additionalProperties
oneOf / anyOf / allOf structure
numeric/string/array constraints
```

Movable annotation fields only when located at a schema-node annotation context:

```text
description
title
examples
default
$schema
$id
$comment
long format strings
```

Walker state:

```rust
pub enum SchemaWalkContext {
    SchemaNode,
    PropertiesMap,
    PatternPropertiesMap,
    DefinitionsMap,
    DefsMap,
    RequiredArray,
    EnumArray,
    ConstValue,
    RefValue,
    Unknown,
}
```

Rules:

```text
Do not remove a property key because it is named description/title/default.
Do not leave required pointing at a removed property.
Do not mutate input_schema in-place.
Do not active-image schema annotations in v1.
If context is Unknown, preserve text.
```

## 11. ConversationInventory

### 11.1 Responsibility

ConversationInventory lowers protocol-specific requests into a uniform conversation/turn/block structure.

```rust
pub struct ConversationInventory {
    pub protocol: Protocol,
    pub endpoint: OpticalEndpoint,
    pub turns: Vec<ConversationTurn>,
    pub blocks: Vec<BlockRef>,
    pub current_user_turn: Option<usize>,
    pub recent_tail_start: usize,
}

pub struct ConversationTurn {
    pub index: usize,
    pub role: RoleKind,
    pub blocks: Vec<BlockRefId>,
    pub opens_tool_ids: Vec<String>,
    pub closes_tool_ids: Vec<String>,
    pub contains_opaque_item: bool,
    pub contains_image: bool,
    pub typed_user_text_chars: usize,
}

pub struct BlockRef {
    pub block_id: String,
    pub path: JsonPath,
    pub kind: BlockKind,
    pub role: RoleKind,
    pub turn_index: Option<usize>,
    pub chars: usize,
    pub text_ref: Option<TextRef>,
    pub protection: InitialProtection,
}
```

Anthropic rules:

```text
body.system -> SystemPrompt, AlwaysText in v1.
body.tools[] -> ToolDefinition, validator AlwaysText.
messages[].content text -> role/age based block.
content[].type=tool_use -> structural block, no image.
content[].type=tool_result -> ToolResult bucket candidate if large and closed.
last N messages -> recent tail, AlwaysText.
existing image blocks -> preserve as opaque.
```

OpenAI rules:

```text
system/developer messages -> AlwaysText in v1.
tool messages -> ToolResult candidate if large.
function/tool call items -> structural block.
function/tool output items -> close tool IDs.
input_image/image_url -> opaque.
last N messages -> AlwaysText.
OpenAI active remains disabled until capability supports image input and economics.
```

## 12. SemanticPriorityPlanner

### 12.1 Responsibility

SemanticPriorityPlanner preserves current task semantics, recent user intent, unfinished tool state, and critical user pointers.

```rust
pub struct SemanticPriorityPlan {
    pub always_text_blocks: Vec<String>,
    pub prior_context_blocks: Vec<String>,
    pub candidate_blocks: Vec<String>,
    pub demoted_opening_turns: Vec<usize>,
    pub latest_collapsed_user_pointer: Option<CollapsedUserPointer>,
}

pub struct CollapsedUserPointer {
    pub turn_index: usize,
    pub policy: CollapsedUserPointerPolicy,
    pub chars_carried_text: usize,
}

pub enum CollapsedUserPointerPolicy {
    PreviewOnly,
    VerbatimCappedHeadTail,
    None,
}
```

Rules:

```text
current user turn = AlwaysText.
recent tail = AlwaysText.
latest assistant/tool state = AlwaysText if open or not fully closed.
opening user text can be demoted only when clearly no longer live instruction.
most recent collapsed user turn gets a text pointer when history is imaged.
tool IDs and tool result IDs stay exact text.
```

## 13. ClosedToolBoundaryPlanner

### 13.1 Responsibility

Any history compression must find a closed tool-call boundary first. Tool-use and tool-result sequences cannot be split.

```rust
pub struct ClosedToolBoundaryPlan {
    pub eligible: bool,
    pub protected_prefix_turns: usize,
    pub keep_tail_turns: usize,
    pub raw_cutoff_exclusive: usize,
    pub snapped_cutoff_exclusive: usize,
    pub closed_boundary_exclusive: Option<usize>,
    pub collapse_start: usize,
    pub collapse_end_exclusive: usize,
    pub reason: BoundaryReason,
    pub open_tool_ids_at_cutoff: usize,
}

pub enum BoundaryReason {
    Eligible,
    NoHistory,
    PrefixTooShort,
    NoClosedPrefix,
    BelowMinChars,
    BelowMinTokens,
    OpaqueBarrier,
    CurrentUserWouldBeCollapsed,
    NotProfitable,
}
```

Algorithm:

```text
1. Build turn-level tool ID open/close sets.
2. Scan prefix until keep_tail boundary.
3. Track open_tool_ids.
4. Candidate boundary is valid only when open_tool_ids is empty.
5. Stop at opaque barrier.
6. Snap boundary to collapse_chunk/freeze_chunk grid.
7. If snapped boundary would include current user turn, reject.
8. If below bucket threshold, reject.
```

Defaults:

```text
Anthropic:
keep_tail = 4
min_collapse_prefix = 10
collapse_chunk = 50
freeze_chunk = 10

OpenAI/GPT:
keep_tail = 6
min_collapse_prefix = 10
min_collapse_tokens = 2000
collapse_chunk = 10
freeze_chunk = 10
section_tokens = 2000
max_images = 16
```

## 14. CacheStableBoundaryPlanner

### 14.1 Responsibility

CacheStableBoundaryPlanner keeps already-visualized history sections stable across later requests.

```rust
pub struct CacheStableBoundaryPlan {
    pub stable_prefix_digest: Option<String>,
    pub cache_prefix_bytes: usize,
    pub frozen_sections: Vec<FrozenSectionPlan>,
    pub newest_partial_section: Option<PartialSectionPlan>,
    pub carry_over_image_ordinal: Option<usize>,
    pub cache_tokens_at_risk: usize,
    pub anti_flap_penalty_tokens: usize,
    pub expected_cache_mode: ExpectedCacheMode,
    pub warnings: Vec<CacheStabilityWarning>,
}

pub struct FrozenSectionPlan {
    pub section_id: String,
    pub start_turn: usize,
    pub end_turn_exclusive: usize,
    pub digest: String,
    pub estimated_tokens: usize,
    pub cache_anchor_policy: CacheAnchorPolicy,
}

pub enum CacheAnchorPolicy {
    NoAnchor,
    MarkLastImage,
    PreserveExistingBreakpoint,
}
```

Rules:

```text
FrozenSection uses absolute turn range.
Completed frozen sections remain byte-identical as new turns append.
Newest partial section may remain text until sealed.
If stable digest cannot be proven, active mode skips.
Digest prefixes may be logged; raw text and base64 cannot be logged.
```

## 15. RiskPartitionPlanner

### 15.1 Risk tiers

```rust
pub enum RiskTier {
    RenderSafe,
    ExactTextRequired,
    SecretNoDuplicate,
    StructuralUnsafe,
    BlockUnsafe,
}
```

Classification:

```text
RenderSafe:
- prose summary
- large logs without secrets/exact anchors
- verbose tool output that does not require byte-exact recall

ExactTextRequired:
- hash
- UUID
- file path
- line anchor
- command flag
- package version
- route/model ID

SecretNoDuplicate:
- authorization header
- cookie
- API key-like token
- bearer token
- private credential material

StructuralUnsafe:
- schema validator
- open tool-call state
- diff/patch
- provider protocol object

BlockUnsafe:
- dense exact content
- unsupported glyph density
- too many anchors
- unknown encoding risk
```

### 15.2 AnchorMap

```rust
pub struct AnchorMap {
    pub anchors: Vec<ExactAnchor>,
    pub sidecar_policy: SidecarPolicy,
    pub render_text_ref: TextRef,
    pub sidecar_text_ref: Option<TextRef>,
}

pub struct ExactAnchor {
    pub anchor_id: String,
    pub risk_kind: ExactRiskKind,
    pub source_span: SourceSpan,
    pub placeholder: String,
}

pub enum SidecarPolicy {
    None,
    TextAdjacent,
    PreserveWholeBlockText,
    SkipBlock,
}
```

Rules:

```text
Anchors are built before reflow.
Reflow operates on placeholder text.
Sidecar order must match placeholder order.
Secret-like values never enter sidecar.
Event Ledger stores counts and risk kinds only.
If exact_chars / total_chars > 0.15, skip block.
If anchors > 100, skip block.
Diff/patch stays text in active v1.
```

## 16. ReflowAnchorPlanner

### 16.1 Responsibility

ReflowAnchorPlanner transforms renderable text into a visual layout protocol while preserving anchors and newline semantics.

```rust
pub struct ReflowPlan {
    pub enabled: bool,
    pub sentinel: char,
    pub preexisting_sentinel_count: usize,
    pub neutralized_count: usize,
    pub input_chars: usize,
    pub output_chars: usize,
    pub visual_rows_before: usize,
    pub visual_rows_after: usize,
    pub anchor_alignment_ok: bool,
}
```

Algorithm:

```text
1. Replace existing sentinel characters with neutralized marker.
2. Trim trailing whitespace per line.
3. Collapse excessive blank lines.
4. Replace hard newlines with visible sentinel where safe.
5. Keep anchor placeholders atomic.
6. Compute visual rows using RenderGeometryProfile.cols.
7. If anchor alignment fails, candidate action = PreserveText.
```

## 17. PagingBudgetPlanner

### 17.1 Responsibility

PagingBudgetPlanner limits images per candidate and inserts explicit omitted markers when content exceeds budget.

```rust
pub struct PagingBudgetPlan {
    pub content_shape: ContentShape,
    pub max_images: usize,
    pub estimated_images: usize,
    pub truncated: bool,
    pub omitted_chars: usize,
    pub omitted_lines: usize,
    pub visible_head_chars: usize,
    pub visible_tail_chars: usize,
    pub marker_text_ref: Option<TextRef>,
}

pub enum ContentShape {
    Structured,
    LogLike,
    ProseOrOther,
    HistoryTranscript,
}
```

Rules:

```text
No silent truncation.
Marker enters rendered text.
LogLike uses head + tail retention.
Structured preserves head first.
HistoryTranscript follows frozen section boundaries.
Omitted raw text is never logged.
```

## 18. EconomicsPlanner

### 18.1 Responsibility

EconomicsPlanner evaluates text tokens, image tokens, sidecar tokens, cache burn, output risk, latency risk, and history amortization.

```rust
pub struct BlockEconomicsPlan {
    pub bucket: OpticalBucket,
    pub text_tokens_before: f64,
    pub image_tokens_after: f64,
    pub sidecar_tokens: f64,
    pub cache_burn_image_side: f64,
    pub cache_burn_text_side: f64,
    pub output_risk_tokens: f64,
    pub latency_risk_level: LatencyRiskLevel,
    pub net_saved_tokens: f64,
    pub profitable: bool,
    pub reason: EconomicsReason,
}
```

Single-request formula:

```text
text_tokens_before = bucket_chars / bucket_chars_per_token
image_tokens_after = rendered_pixel_area / pixels_per_vision_token * safety_margin
sidecar_tokens = sidecar_chars / sidecar_chars_per_token
cache_burn_image_side = prior_warm_text_tokens * max(cache_create_rate - cache_read_rate, 0)
cache_burn_text_side = prior_warm_image_tokens * max(cache_create_rate - cache_read_rate, 0)
output_risk_tokens = (image_tokens_after + sidecar_tokens) * output_risk_multiplier
net_saved = text_tokens_before + cache_burn_text_side - image_tokens_after - sidecar_tokens - cache_burn_image_side - output_risk_tokens
```

History amortization formula:

```text
image_lifetime = image_tokens * (cache_create_rate + cache_read_rate * (N - 1))
text_lifetime = text_tokens * cache_read_rate * N
profitable_history = image_lifetime + burn_image_side < text_lifetime + burn_text_side
```

Rules:

```text
Small/sparse prose usually stays text.
Dense logs/JSON/tool output may become candidates.
History requires amortized profitability, not one-shot profitability.
If provider usage data exists, actual usage calibrates future estimates.
```

## 19. CalibrationStore

### 19.1 Responsibility

CalibrationStore compares estimated usage with observed provider usage and improves later estimates without storing raw prompt or rendered images.

```rust
pub struct OpticalCalibrationRow {
    pub timestamp_ms: u64,
    pub provider_family: ProviderFamily,
    pub model_base: String,
    pub endpoint: OpticalEndpoint,
    pub mode: OpticalMode,
    pub bucket: OpticalBucket,
    pub estimated_text_tokens_before: f64,
    pub estimated_image_tokens_after: f64,
    pub estimated_sidecar_tokens: f64,
    pub actual_input_tokens: Option<f64>,
    pub actual_output_tokens: Option<f64>,
    pub actual_cache_write_tokens: Option<f64>,
    pub actual_cache_read_tokens: Option<f64>,
    pub latency_ms: Option<u64>,
    pub quality_signal: Option<QualitySignal>,
}
```

Rules:

```text
No raw text.
No image bytes.
No base64.
No exact values.
Rows aggregate by provider/model/bucket.
Calibration cannot make blocked models active.
Calibration can only adjust estimator parameters within bounded ranges.
```

## 20. OpticalTransformPlan

### 20.1 Internal plan

```rust
pub struct OpticalTransformPlan {
    pub plan_id: String,
    pub request_fingerprint: String,
    pub created_at_ms: u64,
    pub mode: OpticalMode,
    pub applicability: OpticalApplicabilityDecision,
    pub capability: OpticalModelCapability,
    pub protocol: Protocol,
    pub endpoint: OpticalEndpoint,
    pub thresholds: OpticalBucketThresholds,
    pub geometry: RenderGeometryProfile,
    pub volatility: VolatilityPlan,
    pub schema: SchemaSlimPlan,
    pub inventory: ConversationInventory,
    pub semantic_priority: SemanticPriorityPlan,
    pub boundary: Option<ClosedToolBoundaryPlan>,
    pub cache: CacheStableBoundaryPlan,
    pub candidates: Vec<CandidatePlan>,
    pub economics: OpticalEconomicsSummary,
    pub render_plan: Option<RenderPlan>,
    pub patch_plan: Option<ProtocolPatchPlan>,
    pub validation: PlanValidationResult,
    pub final_status: OpticalPlanStatus,
    pub gates: Vec<OpticalGateDecision>,
}
```

### 20.2 Candidate plan

```rust
pub struct CandidatePlan {
    pub block_id: String,
    pub bucket: OpticalBucket,
    pub block_kind: BlockKind,
    pub path: JsonPath,
    pub chars_original: usize,
    pub chars_render_candidate: usize,
    pub risk: RiskPartitionPlan,
    pub anchors: Option<AnchorMap>,
    pub reflow: Option<ReflowPlan>,
    pub paging: Option<PagingBudgetPlan>,
    pub economics: Option<BlockEconomicsPlan>,
    pub action: CandidateAction,
    pub reason: CandidateReason,
}

pub enum CandidateAction {
    PreserveText,
    ShadowEstimateOnly,
    RenderAsImage,
    RenderWithTextSidecar,
    RenderAsFrozenHistorySection,
    MoveSchemaAnnotationsToImageCandidate,
    SkipTooSmall,
    SkipUnprofitable,
    SkipCacheUnstable,
    SkipExactnessRisk,
    SkipSecretRisk,
    SkipStructuralRisk,
    SkipUnsupportedModel,
    SkipUnsupportedProtocol,
    SkipOverBudget,
}
```

### 20.3 Public summary

```rust
pub struct OpticalPlanPublicSummary {
    pub plan_id: String,
    pub mode: OpticalMode,
    pub final_status: OpticalPlanStatus,
    pub gates: Vec<OpticalGateDecision>,
    pub counters: OpticalPlanCounters,
    pub buckets: Vec<OpticalBucketSummary>,
    pub cache: CachePublicSummary,
    pub economics: OpticalEconomicsPublicSummary,
    pub warnings: Vec<String>,
}
```

Public summary forbidden fields:

```text
raw block text
schema annotation text
sidecar exact text
rendered text
PNG bytes
base64
request body
response body
exact values
secret values
```

## 21. Plan State Machine

```text
New
↓
ApplicabilityChecked
↓
CapabilityResolved
↓
ProtocolLowered
↓
VolatilityPlanned
↓
SchemaSlimPlanned
↓
InventoryBuilt
↓
SemanticPriorityPlanned
↓
BoundariesPlanned
↓
CacheStabilityPlanned
↓
RisksPartitioned
↓
ReflowPagingPlanned
↓
EconomicsPlanned
↓
ValidatedShadowPlan
↓
ValidatedActivePlan
↓
Rendered
↓
Patched
↓
Applied OR FailedClosed
```

State constraints:

```text
Cannot enter Rendered without ValidatedActivePlan.
Cannot enter ValidatedActivePlan if capability.status is Unknown, Blocked, or ShadowOnly.
Cannot enter ValidatedActivePlan if current user turn is not AlwaysText.
Cannot enter ValidatedActivePlan if cache is unstable.
Cannot enter Patched if patch path is outside active candidate actions.
Any validation failure returns original body unchanged.
```

## 22. PlanValidator

### 22.1 Shadow validator

```text
body unchanged.
public summary contains no raw text.
all candidates have action and reason.
current user turn is AlwaysText.
recent tail is AlwaysText.
unknown model active disabled.
geometry exists.
TextRef does not serialize.
```

### 22.2 Active validator

```text
global enabled.
route enabled.
confirmed_experimental_lossy is true.
model capability supported.
endpoint active supported.
candidate action is active-compatible.
secret-like content not duplicated.
current user/recent tail unchanged.
system/developer instruction unchanged in v1.
tool_use/tool_result structural IDs preserved.
image count <= policy max.
patch body valid for protocol.
renderer output count matches plan.
```

## 23. Protocol Patch Contracts

### 23.1 Anthropic /v1/messages active v1

Allowed:

```text
Replace large tool_result text content with:
1. intro text block
2. one or more image blocks
3. optional exact sidecar text block when SidecarPolicy::TextAdjacent and no secret-like values
```

Preserve:

```text
tool_use_id
tool_result type
message role
message order
tool_use blocks
system
tools
recent tail
current user text
```

Forbidden:

```text
No image insertion into system in v1.
No mutation of tools schema in active v1.
No mutation of tool_use blocks.
No deletion of tool_result structural IDs.
No OpenAI active patch in v1.
```

### 23.2 Patch validation

```text
PatchOperation path must match a CandidatePlan with active action.
PatchOperation must not target AlwaysText path.
PatchOperation must preserve protocol-valid JSON.
PatchOperation must be reversible to original body inside request-local memory.
```

## 24. Core API

```text
GET  /compression/optical/status
GET  /compression/optical/capabilities
POST /compression/optical/preview
GET  /compression/optical/events/recent
POST /compression/optical/config
POST /compression/optical/route/{route_id}/mode
```

`/compression/optical/preview` returns `OpticalPlanPublicSummary` only.

Active mode config requires explicit confirmation:

```json
{
  "confirmed_experimental_lossy": true,
  "route_id": "...",
  "mode": "tool_results_only"
}
```

## 25. Optical Lab UI

Page structure:

```text
Header
MasterSwitchCard
ApplicabilityCard
CapabilityCard
DecisionGraph
BucketOpportunityTable
CacheStabilityCard
SchemaSlimmingCard
ClosedToolBoundaryCard
ExactnessGuardCard
EconomicsCard
RecentMetadataEvents
```

UI rules:

```text
Default view is shadow status.
Visible not-lossless warning.
Estimated and actual metrics are separate.
Every skipped candidate has a reason.
No raw prompt, rendered text, PNG, base64, exact values, secret values.
Active enable requires confirmation.
Capability status must be visible before active controls.
```

## 26. Event Ledger

Events:

```text
optical_plan_created
optical_plan_shadowed
optical_plan_skipped
optical_plan_applied
optical_plan_failed_closed
```

Allowed fields:

```json
{
  "event_type": "optical_plan_shadowed",
  "plan_id": "...",
  "request_id": "...",
  "route_id": "...",
  "provider_id": "...",
  "model_base": "...",
  "protocol": "anthropic",
  "mode": "shadow_preview",
  "final_status": "validated_shadow_plan",
  "buckets": {
    "tool_result_log": {
      "blocks": 2,
      "chars": 18000,
      "estimated_net_saved": 6200
    }
  },
  "cache": {
    "stable_prefix_digest_present": true,
    "frozen_sections": 0,
    "cache_tokens_at_risk": 0
  },
  "risk": {
    "exact_spans": 12,
    "secret_spans": 0,
    "structural_skips": 1
  },
  "economics": {
    "text_tokens_before": 9000,
    "image_tokens_after": 2200,
    "sidecar_tokens": 120,
    "cache_burn_tokens": 0,
    "net_saved_tokens": 6680
  }
}
```

Forbidden fields:

```text
raw_prompt
raw_response
rendered_text
schema_annotation_text
sidecar_text
exact_values
secret_values
png
base64
request_body
response_body
```

## 27. Resource Budget and Abuse Resistance

Hard budgets:

```text
max_plan_time_ms
max_render_time_ms
max_images_per_request
max_images_per_tool_result
max_candidate_chars_per_request
max_anchor_count_per_block
max_exact_chars_ratio
max_sidecar_chars_per_block
```

Rules:

```text
Over-budget request remains pass-through or shadow-summary-only.
Renderer cannot allocate based on untrusted dimensions.
Geometry profile is fixed by backend, not user input.
Image count budget applies before rendering.
Sidecar budget applies before rendering.
Exact span scanning uses bounded regex and line scanners.
```

## 28. Quality Evaluation

Eval dimensions:

```text
Cost: estimated vs actual input/image/cache/output tokens.
Latency: local planning/rendering and upstream latency.
Task quality: coding task success signal or user-visible retry signal.
Vision readability: summary question accuracy from image blocks.
Exactness safety: exact string tasks must rely on text sidecar or skip.
Privacy: export scan finds no raw prompt/image/base64/exact values.
```

Required fixtures:

```text
anthropic_tool_result_dense_log.json
anthropic_tool_result_sparse_prose.json
anthropic_tool_result_secret.json
anthropic_tool_result_hash_path_uuid.json
anthropic_tool_result_diff_patch.json
anthropic_tool_result_cjk.json
anthropic_history_closed_prefix.json
anthropic_history_open_tool_call.json
openai_chat_tool_output_shadow.json
schema_description_property_name.json
schema_annotation_heavy.json
```

## 29. Test Matrix

Phase A tests:

```text
dtoken-types:
- optical enums snake_case serde
- default policy off
- legacy route deserializes without optical fields
- public summary serializes without raw content fields
- TextRef is not serializable

applicability:
- POST /v1/messages eligible for Anthropic shadow
- GET /v1/messages ineligible
- empty body ineligible
- unknown model shadow-only
- unsupported path ineligible
- over-budget request not active

capability:
- unknown model cannot active-transform
- blocked provider cannot active-transform
- supported Anthropic image block allows tool_result active

render_geometry:
- profile has nonzero dimensions
- readable_chars_per_image derived from cols/rows
- estimator consumes profile rather than constants
```

Plan-layer tests:

```text
volatility:
- environment block becomes volatile
- stable digest ignores volatile segment
- churning stable tag becomes warning

schema_slimmer:
- property named description is preserved
- required remains valid
- enum/const/type/$ref preserved
- annotation chars moved to annotation block
- unknown schema context preserves text

inventory:
- Anthropic tool_result candidate detected
- recent tail protected
- system prompt protected
- existing images opaque

semantic_priority:
- current user AlwaysText
- latest collapsed user pointer generated for imaged history
- open tool state AlwaysText

history_boundary:
- open tool call prevents collapse
- closed prefix eligible
- opaque item stops collapse
- current user cannot be collapsed

risk:
- UUID/hash/path become ExactTextRequired
- bearer/cookie/API-key become SecretNoDuplicate
- diff becomes StructuralUnsafe
- dense exact spans skip block
- anchor count over budget skips block

reflow_anchor:
- existing sentinel neutralized
- anchor placeholder preserved
- sidecar order matches placeholder order

paging_budget:
- log keeps head/tail
- structured keeps head
- omitted marker inserted
- omitted raw text not logged

economics:
- sparse prose unprofitable
- dense log profitable
- cache burn can flip decision
- history uses amortized gate

validator:
- shadow body unchanged
- active rejects unknown model
- active rejects cache-unstable candidate
- active rejects secret sidecar duplication
- patch path outside candidate rejected
```

## 30. Implementation Order

### Phase A: Plan-only foundation

```text
1. dtoken-types: add optical public enums/stats/policy/public summary.
2. dtoken-compress: add optical module skeleton.
3. Implement capability.rs.
4. Implement applicability.rs.
5. Implement render_geometry.rs.
6. Implement TextRef/TextArena with non-serializable raw text containment.
7. Implement volatility.rs.
8. Implement schema_slimmer.rs.
9. Implement inventory.rs for Anthropic only.
10. Implement semantic_priority.rs.
11. Implement risk.rs and AnchorMap.
12. Implement economics.rs.
13. Implement decision.rs to build shadow-only OpticalTransformPlan.
14. Tests prove body unchanged and public summary redacted.
```

### Phase B: API visibility

```text
1. /compression/optical/preview.
2. /compression/optical/status.
3. /compression/optical/capabilities.
4. All responses use public summary only.
```

### Phase C: Desktop UI

```text
1. Add Optical Lab nav.
2. Add fixture data.
3. Render public plan summary.
4. Add capability card.
5. Add active-mode confirmation.
```

### Phase D: Renderer and patcher

```text
1. Implement reflow_anchor.rs.
2. Implement paging_budget.rs.
3. Implement render.rs behind feature flag.
4. Implement patch.rs for Anthropic tool_result only.
5. Implement validate.rs fail-closed.
```

### Phase E: History compression

```text
1. Implement history_boundary.rs for Anthropic.
2. Add cache-stable frozen sections.
3. Add latest collapsed user pointer.
4. Add OpenAI/GPT planner only after Anthropic history is stable.
```

## 31. Release Gates

```text
Shadow mode changes no request body bytes.
Current user turn stays text.
Recent tail stays text.
Unknown model cannot active-transform.
Schema validator structure remains valid.
Open tool-call sequence cannot collapse.
Secret-like values are never sidecar duplicated.
Geometry is shared by estimator, renderer, and UI.
Cache-unstable candidates skip active mode.
Logs/exports contain no raw text/image/base64/exact values.
Renderer failure returns original body unchanged.
Patch validation preserves protocol structure.
Capability status is visible before active enable.
Calibration stores no raw content.
Over-budget inputs fail closed or pass-through.
```

## 32. Next Coding Task

Implement the smallest verifiable loop:

```text
A1 dtoken-types public optical enums/stats/policy/public summary
A2 dtoken-compress optical module skeleton
A3 capability.rs
A4 applicability.rs
A5 render_geometry.rs
A6 TextRef/TextArena raw text containment
A7 unit tests for serde/defaults/exact path/model applicability/capability/redaction
```

Not allowed in Phase A:

```text
PNG renderer
UI active switch
request mutation
OpenAI active transform
history active transform
persistent raw recovery cache
```

## 33. Completeness Criteria

The design is ready for Phase A coding when all statements are true:

```text
Every active path has a fail-closed rule.
Every public output has a raw-content forbidden list.
Every candidate action has a skip/apply reason.
Every model/endpoint active path depends on explicit capability.
Every image estimate depends on RenderGeometryProfile.
Every raw text reference is request-local and non-serializable.
Every history compression path depends on closed tool boundary and cache stability.
Every schema slimming path preserves validator structure.
Every sidecar path excludes secret-like values.
Every budget overflow path avoids active mutation.
```

Current Phase A target satisfies these criteria at design level. Further confidence must come from implementing the listed tests and fixture evals.
