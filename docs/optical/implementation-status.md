# Optical Compression 实现状态矩阵

更新日期：2026-07-08（本轮完整实施后）

## Phase A: Plan-only foundation

| 编号 | 任务 | 文件 | 状态 |
|---|---|---|---|
| A0 | crate-root export `pub mod optical;` | dtoken-compress/src/lib.rs | ✅ 完成（本轮修复） |
| A1 | dtoken-types 公开 optical enums/stats/policy/public summary | dtoken-types/src/lib.rs | ✅ 完成 |
| A2 | dtoken-compress optical module skeleton | optical/mod.rs | ✅ 完成 |
| A3 | capability.rs | optical/capability.rs | ✅ 完成 |
| A4 | applicability.rs | optical/applicability.rs | ✅ 完成 |
| A5 | render_geometry.rs | optical/render_geometry.rs | ✅ 完成 |
| A6 | TextRef/TextArena raw text containment | optical/text_arena.rs | ✅ 完成 |
| A7 | unit tests | tests/optical_phase_a.rs, tests/optical_types.rs | ✅ 完成 |

## Phase A 扩展: 剩余 plan 层模块（本轮实施）

| 模块 | 文件 | 测试 | 状态 |
|---|---|---|---|
| volatility.rs | VolatilityPlanner | 3 tests | ✅ 完成 |
| schema_slimmer.rs | SchemaSlimPlanner | 4 tests | ✅ 完成 |
| inventory.rs | ConversationInventory | 4 tests | ✅ 完成 |
| semantic_priority.rs | SemanticPriorityPlanner | 3 tests | ✅ 完成 |
| risk.rs | RiskPartitionPlanner + AnchorMap | 4 tests | ✅ 完成 |
| economics.rs | EconomicsPlanner | 4 tests | ✅ 完成 |
| decision.rs | OpticalTransformPlan builder (shadow-only) | 2 tests | ✅ 完成 |
| validate.rs | PlanValidator (shadow + active) | 6 tests | ✅ 完成 |

cargo 验证结果：

```text
cargo test -p dtoken-types optical    → 2 passed, 0 failed
cargo test -p dtoken-compress optical → 42 passed, 0 failed
cargo check -p dtoken-compress        → Finished (warnings only, no errors)
```

## Phase B: API visibility

| 端点 | 状态 |
|---|---|
| /compression/optical/preview | 待实施 |
| /compression/optical/status | 待实施 |
| /compression/optical/capabilities | 待实施 |

## Phase C-E

| Phase | 状态 |
|---|---|
| C: Desktop UI | 待实施 |
| D: Renderer and patcher | 待实施（feature flag） |
| E: History compression | 待实施 |
