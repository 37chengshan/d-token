# Optical Compression 总体架构

## 管线

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

## 模块框架

```text
code/crates/dtoken-compress/src/optical/
  mod.rs              — 公开 re-export
  applicability.rs    — ApplicabilityGate
  capability.rs       — CapabilityResolver
  render_geometry.rs  — RenderGeometryProfile
  text_arena.rs       — TextRef/TextArena raw text containment
  volatility.rs       — VolatilityPlanner
  schema_slimmer.rs   — SchemaSlimPlanner
  inventory.rs        — ConversationInventory
  semantic_priority.rs— SemanticPriorityPlanner
  history_boundary.rs — ClosedToolBoundaryPlanner
  cache_stability.rs  — CacheStableBoundaryPlanner
  risk.rs             — RiskPartitionPlanner + AnchorMap
  reflow_anchor.rs    — ReflowAnchorPlanner
  paging_budget.rs    — PagingBudgetPlanner
  economics.rs        — EconomicsPlanner
  decision.rs         — OpticalTransformPlan builder
  validate.rs         — PlanValidator
  render.rs           — PNG renderer (feature flag)
  patch.rs            — Protocol patcher
  calibration.rs      — CalibrationStore
  telemetry.rs        — Telemetry
```

## 依赖分层

```text
Phase A shadow-only (默认 build):
  serde, serde_json, regex, sha2

Phase B renderer feature:
  image png, fontdue, base64, unicode-width
```

## 状态机

```text
New → ApplicabilityChecked → CapabilityResolved → ProtocolLowered
→ VolatilityPlanned → SchemaSlimPlanned → InventoryBuilt
→ SemanticPriorityPlanned → BoundariesPlanned → CacheStabilityPlanned
→ RisksPartitioned → ReflowPagingPlanned → EconomicsPlanned
→ ValidatedShadowPlan → ValidatedActivePlan → Rendered → Patched
→ Applied OR FailedClosed
```

状态约束：

```text
Cannot enter Rendered without ValidatedActivePlan.
Cannot enter ValidatedActivePlan if capability.status is Unknown/Blocked/ShadowOnly.
Cannot enter ValidatedActivePlan if current user turn is not AlwaysText.
Cannot enter ValidatedActivePlan if cache is unstable.
Cannot enter Patched if patch path is outside active candidate actions.
Any validation failure returns original body unchanged.
```
