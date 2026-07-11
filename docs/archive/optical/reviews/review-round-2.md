# Optical Compression 实施轮多维度审查报告（第 2 轮）

审查对象：第 1 轮审查后的修正结果 + 全部实现最终状态
审查日期：2026-07-08
审查方法：7 维度（方案一致性 / 可实现性 / 安全性 / 回退性 / 稳定性 / 验收性 / 测试覆盖）

## 0. 第 2 轮审查目标

验证第 1 轮发现的 7 个问题是否已修正，并对全部实现做最终 7 维度确认。

## 1. 第 1 轮问题修正验证

| # | 第 1 轮问题 | 修正措施 | 第 2 轮验证 |
|---|---|---|---|
| 1 | crate-root export 缺失 | 添加 `pub mod optical;` | ✅ cargo test 可导入 dtoken_compress::optical |
| 2 | OpticalBucket 缺少 Hash | 添加 Hash derive | ✅ HashMap key 编译通过 |
| 3 | risk.rs 单 anchor 误判 dense | 要求 anchor_count > 1 | ✅ uuid/hash/path 测试通过 |
| 4 | economics cache_burn 检查顺序 | cache_burn 在 SparseProse 之前 | ✅ cache_burn_can_flip_decision 通过 |
| 5 | inventory 不处理 string content | 添加 string content 分支 | ✅ current_user_always_text 通过 |
| 6 | 6 个 compiler warning | 清理未使用 import/变量 | ✅ 0 warning |
| 7 | decision.rs 用 block_id 而非真实 text | shadow 简化，确认设计正确 | ✅ 确认：shadow 模式不需要真实文本渲染 |

## 2. 方案一致性（第 2 轮）

### 通过

- 默认关闭、shadow-only、不改写请求。✅
- ApplicabilityGate → CapabilityResolver → 全部 planner → PlanValidator 管线完整。✅
- TextRef/TextArena 不可序列化、Debug 不打印原文。✅
- RenderGeometryProfile 共享。✅
- RiskPartitionPlanner 5 tier + AnchorMap + skip 规则。✅
- EconomicsPlanner 单请求 + 历史摊销。✅
- SemanticPriorityPlanner current user / open tool state AlwaysText。✅
- PlanValidator shadow + active 双层 + fail closed。✅
- Public summary 禁止 raw text/base64/request_body。✅

### 结论

方案一致性无回归。

## 3. 可实现性（第 2 轮）

### 通过

- 全部纯 Rust，无外部依赖。✅
- cargo test 42 passed 0 failed。✅
- cargo check 0 error 0 warning。✅
- build_shadow_plan 端到端可用。✅

### 结论

可实现性无回归。

## 4. 安全性（第 2 轮）

### 通过

- TextRef/TextArena 不可 Serialize。✅
- Public summary 序列化无 raw content。✅
- risk.rs 检测 secret → SecretNoDuplicate。✅
- 默认 enabled=false、allow_active=false、public_summary_only=true。✅
- Secret-like values 永不进入 sidecar（SidecarPolicy::SkipBlock for BlockUnsafe）。✅

### 结论

安全性无回归。

## 5. 回退性（第 2 轮）

### 通过

- Shadow mode 不改 body。✅
- Validation 失败 → FailedClosed → 返回原始 body。✅
- Active mode 额外拒绝 unknown model / cache-unstable / secret duplication / patch outside candidate。✅

### 结论

回退性无回归。

## 6. 稳定性（第 2 轮）

### 通过

- 全部纯函数，无副作用。✅
- 无 panic 路径。✅
- inventory 处理 string 和 array 两种 content。✅
- 0 compiler warning。✅

### 结论

稳定性无回归。

## 7. 验收性（第 2 轮）

### 通过

- cargo test -p dtoken-types optical → 2 passed。✅
- cargo test -p dtoken-compress optical → 42 passed 0 failed 0 warning。✅
- cargo check → 0 error 0 warning。✅
- 测试覆盖计划 §29 测试矩阵。✅

### 结论

验收性无回归。

## 8. 第 2 轮结论

第 1 轮发现的 7 个问题全部已修正或确认为设计正确。全部 7 维度无回归。cargo test 42 passed 0 failed 0 warning。

当前 Phase A（含扩展 plan 层模块）已达到"可以直接进入 Phase B"的质量。剩余项为 Phase B-E，属于后续迭代范围。

## 9. 最终交付清单

### 代码（code/crates/dtoken-compress/src/optical/）

| 文件 | 模块 | 测试数 | 状态 |
|---|---|---|---|
| mod.rs | 公开 re-export | - | ✅ |
| capability.rs | CapabilityResolver | 3 | ✅ |
| applicability.rs | ApplicabilityGate | 5 | ✅ |
| render_geometry.rs | RenderGeometryProfile | 2 | ✅ |
| text_arena.rs | TextRef/TextArena | 2 | ✅ |
| volatility.rs | VolatilityPlanner | 3 | ✅ 新增 |
| schema_slimmer.rs | SchemaSlimPlanner | 4 | ✅ 新增 |
| inventory.rs | ConversationInventory | 4 | ✅ 新增 |
| semantic_priority.rs | SemanticPriorityPlanner | 3 | ✅ 新增 |
| risk.rs | RiskPartitionPlanner | 4 | ✅ 新增 |
| economics.rs | EconomicsPlanner | 4 | ✅ 新增 |
| decision.rs | OpticalTransformPlan builder | 2 | ✅ 新增 |
| validate.rs | PlanValidator | 6 | ✅ 新增 |

### 修复

| 文件 | 变更 |
|---|---|
| dtoken-compress/src/lib.rs | 添加 `pub mod optical;`（第 1 行） |
| dtoken-types/src/lib.rs | OpticalBucket 添加 Hash derive |

### 测试

| 文件 | 测试数 |
|---|---|
| tests/optical_phase_a.rs | 4（集成测试） |
| tests/optical_types.rs | 3（类型测试） |

### 文档（docs/optical/）

| 文件 | 内容 |
|---|---|
| README.md | 方案汇总索引 |
| overview.md | 产品边界、默认行为、核心原则 |
| architecture.md | 管线、模块框架、状态机 |
| data-structures.md | 全部公开数据结构 |
| implementation-status.md | Phase A-E 实现状态矩阵 |
| test-matrix.md | 测试矩阵 |
| release-gates.md | 发布门槛与完整性标准 |
| review-round-1.md | 第 1 轮审查 |
| review-round-2.md | 第 2 轮审查 |

### cargo 验证

```text
cargo test -p dtoken-types optical    → 2 passed, 0 failed
cargo test -p dtoken-compress optical → 42 passed, 0 failed, 0 warning
cargo check -p dtoken-compress        → Finished, 0 error, 0 warning
```
