# Optical Architecture V2 Iteration

更新日期：2026-07-10

## 1. 架构结论

Optical 不再作为独立压缩主流程，而是 Hybrid Context Optimization Engine 中的一个 codec。

```text
Context Node
  ├─ PreserveText
  ├─ CacheStabilize
  ├─ StructuralMinify
  ├─ SemanticCompress
  ├─ OpticalCapsule
  └─ ExternalizeWithRetrieval
```

当前实现只落地了安全的 Optical shadow planner，不包含 renderer 或 request mutation。

## 2. 当前代码管线

```text
OpticalPreviewRequest
    ↓
ApplicabilityGate
    ↓
OpticalCapabilityRegistry
    ↓
TextArena + Anthropic Inventory
    ↓
Semantic Priority
    ↓
Risk Firewall
    ↓
Geometry / Image Count
    ↓
Economics
    ↓
CandidateDecision
    ↓
OpticalPlanPublicSummary
    ↓
Structured Shadow Validator
```

对应文件：

```text
code/crates/dtoken-compress/src/optical/applicability.rs
code/crates/dtoken-compress/src/optical/capability.rs
code/crates/dtoken-compress/src/optical/text_arena.rs
code/crates/dtoken-compress/src/optical/inventory.rs
code/crates/dtoken-compress/src/optical/semantic_priority.rs
code/crates/dtoken-compress/src/optical/risk.rs
code/crates/dtoken-compress/src/optical/render_geometry.rs
code/crates/dtoken-compress/src/optical/economics.rs
code/crates/dtoken-compress/src/optical/decision.rs
code/crates/dtoken-compress/src/optical/validate.rs
```

## 3. 重要设计变化

### 3.1 Raw text containment

旧问题：Decision 对合成 block 描述做 risk classification，无法识别真实 secret/diff/path。

新合同：

```text
TextArena owns raw String.
BlockRef owns non-serializable TextRef.
Decision resolves TextRef only inside the request.
Missing TextRef fails closed.
```

### 3.2 Capability fail-closed

旧问题：代码硬编码一个模型为 active-supported。

新合同：

```text
Default registry = no active rules.
Supported = exact verified registry match only.
Limited/Unknown/ShadowOnly = no active transform.
```

### 3.3 Real plan evidence

旧问题：Validator 依赖调用方提供多个 bool。

新合同：

```text
Shadow validator reads actual original/planned body,
public summary and candidate evidence.
Active validator reads policies, capability, image count,
net savings, cache and patch evidence.
```

兼容 bool API 暂时保留，但新代码应使用 structured input API。

### 3.4 Selected-only economics

旧问题：被 secret/structural/exactness gate 跳过的节点仍可能进入收益汇总。

新合同：

```text
Only CandidateAction::ShadowEstimateOnly contributes:
- estimated images
- text tokens before
- image tokens after
- sidecar tokens
- net saved tokens
```

### 3.5 Geometry as single truth source

```text
width/height
padding
cell dimensions
header rows
capacity
vision token formula
safety margin
```

容量和 token estimate 均从 profile 派生。Renderer 和 UI 在 Phase D/C 必须复用同一 profile。

## 4. 安全不变量

```text
Shadow never mutates body.
Current user stays text.
Recent tail stays text.
System/tool schema stays text.
Secret-like content is skipped.
Diff/patch is skipped.
Unknown capability cannot active-transform.
No public raw text, TextRef, image bytes or base64.
Validation failure returns FailedClosed.
```

## 5. 仍需架构升级

### 5.1 ContextGraph

当前 Inventory 是线性 turns/blocks。下一步需要显式 dependency edges：

```text
tool_use → tool_result
summary → source nodes
open tool state
closed history boundary
cache stable prefix
```

### 5.2 Immutable internal plan

当前已经有结构化 validation input，但完整内部 Plan IR 尚未统一。

建议：

```rust
ContextOptimizationPlan {
  request_fingerprint,
  capability_snapshot_id,
  inventory,
  candidates,
  selected_actions,
  economics,
  validation,
  rollback_ref,
}
```

### 5.3 Estimate ranges

当前 economics 仍是 scalar heuristic。active 前必须升级为：

```text
P10 conservative
Expected
P90 optimistic
Confidence
```

### 5.4 Provider-specific capability snapshots

实验 geometry 不能作为永久 provider 合同。未来 capability rules 必须：

```text
versioned
source-attributed
verified-at timestamp
config persisted
revocable
```

## 6. 实施依赖顺序

```text
Capability snapshot config
→ ContextGraph
→ Unified immutable plan
→ Preview events
→ Optical Lab UI
→ Fidelity fixtures
→ Renderer
→ Protocol patcher
→ Post validator
→ Active route rollout
```

Renderer 不得提前于 ContextGraph、plan evidence 和 capability snapshot 实施。
