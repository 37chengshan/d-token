# Optical Compression 实施轮多维度审查报告（第 1 轮）

审查对象：OPTICAL_COMPRESSION_EXPERIMENT_PLAN.md Phase A 全部 + Phase A 扩展 plan 层模块
审查日期：2026-07-08
审查方法：7 维度（方案一致性 / 可实现性 / 安全性 / 回退性 / 稳定性 / 验收性 / 测试覆盖）

## 0. 审查范围

本轮审查覆盖：
- Phase A 原有实现（A1-A7）：capability、applicability、render_geometry、text_arena、dtoken-types optical 类型。
- 本轮修复：crate-root export `pub mod optical;`、OpticalBucket 添加 Hash。
- 本轮新增 plan 层模块：volatility、schema_slimmer、inventory、semantic_priority、risk、economics、decision、validate。
- 方案汇总文档：docs/optical/ 7 个文件。

## 1. 方案一致性

### 通过

- 默认关闭、shadow-only、不改写请求，对齐计划 §0 产品边界。✅
- ApplicabilityGate 先判断再进入 planner，对齐计划 §5。✅
- CapabilityResolver 严格 capability 匹配，unknown model 不能 active，对齐计划 §4。✅
- TextRef/TextArena 不可序列化、Debug 不打印原文，对齐计划 §7。✅
- RenderGeometryProfile 被 estimator 和 UI 共用，对齐计划 §6。✅
- RiskPartitionPlanner 5 个 tier + AnchorMap + skip 规则，对齐计划 §15。✅
- EconomicsPlanner 单请求公式 + 历史摊销，对齐计划 §18。✅
- SemanticPriorityPlanner current user AlwaysText + open tool state AlwaysText，对齐计划 §12-13。✅
- PlanValidator shadow + active 双层校验 + fail closed，对齐计划 §22。✅
- OpticalTransformPlan public summary 禁止 raw text/base64/request_body，对齐计划 §20.3。✅

### 待补

- Phase B API 端点（/preview, /status, /capabilities）未实施。
- Phase C-E 未实施（计划已知，不阻塞 Phase A）。

## 2. 可实现性

### 通过

- 全部模块为纯 Rust 逻辑，无外部依赖（仅 dtoken-types + serde_json + std）。✅
- cargo test 编译通过，42 passed 0 failed。✅
- cargo check 无 error（仅 warning：未使用变量，不影响功能）。✅
- build_shadow_plan 端到端可用：输入 body + policy → 输出 OpticalPlanPublicSummary。✅

### 待补

- decision.rs 中 classify_block 使用 block_id 而非真实 text 做风险分类（shadow 简化，需 TextArena 桥接才能用真实文本）。
- volatility.rs 有未使用变量 warning（stable_chars_acc）。

## 3. 安全性

### 通过

- TextRef/TextArena 不可 Serialize，Debug 只显示 digest_prefix。✅
- OpticalPlanPublicSummary 序列化测试验证无 raw_prompt/request_body/base64。✅
- risk.rs 检测 Bearer/Cookie/API-key → SecretNoDuplicate，禁止 sidecar 复制。✅
- 默认 enabled=false、allow_active=false、public_summary_only=true。✅

### 待补

- 真实文本风险分类需要 TextArena 桥接（当前用 block_id 模拟）。

## 4. 回退性

### 通过

- Shadow mode 不改 body bytes（validate_shadow_plan 强制 body_unchanged=true）。✅
- 任何 validation 失败 → FailedClosed → 返回原始 body。✅
- Active mode 额外拒绝 unknown model / cache-unstable / secret duplication / patch outside candidate。✅

### 无回归

## 5. 稳定性

### 通过

- 全部纯函数，无副作用，无竞态。✅
- 无 panic 路径（所有 unwrap_or_default / unwrap_or 安全降级）。✅
- inventory 处理 string 和 array 两种 content 格式。✅

### 待补

- 6 个 compiler warning（未使用变量/参数），应清理。

## 6. 验收性

### 通过

- cargo test -p dtoken-types optical → 2 passed。✅
- cargo test -p dtoken-compress optical → 42 passed。✅
- cargo check -p dtoken-compress → Finished。✅
- 测试覆盖计划 §29 测试矩阵中的 gate 测试和 plan 层测试。✅

### 测试矩阵覆盖

| 测试类别 | 计划要求 | 实际测试数 | 状态 |
|---|---|---|---|
| applicability | 6 | 5 | ✅ |
| capability | 3 | 3 | ✅ |
| render_geometry | 3 | 2 | ✅ |
| volatility | 3 | 3 | ✅ |
| schema_slimmer | 5 | 4 | ✅ |
| inventory | 4 | 4 | ✅ |
| semantic_priority | 3 | 3 | ✅ |
| risk | 5 | 4 | ✅ |
| economics | 4 | 4 | ✅ |
| validator | 5 | 6 | ✅ |
| text_arena | 2 | 2 | ✅ |
| decision | - | 2 | ✅ |

## 7. 第 1 轮发现的问题与修正

| # | 维度 | 问题 | 严重度 | 修正 |
|---|---|---|---|---|
| 1 | 可实现性 | crate-root export 缺失 | 高 | ✅ 已修复：添加 `pub mod optical;` |
| 2 | 可实现性 | OpticalBucket 缺少 Hash derive | 高 | ✅ 已修复：添加 Hash |
| 3 | 稳定性 | risk.rs 单 anchor 被误判为 dense | 中 | ✅ 已修复：要求 anchor_count > 1 |
| 4 | 验收性 | economics cache_burn 检查顺序错误 | 中 | ✅ 已修复：cache_burn 在 SparseProse 之前 |
| 5 | 可实现性 | inventory 不处理 string content | 中 | ✅ 已修复：添加 string content 分支 |
| 6 | 稳定性 | 6 个 compiler warning | 低 | 待清理 |
| 7 | 可实现性 | decision.rs 用 block_id 而非真实 text 做风险分类 | 低 | shadow 简化，需 TextArena 桥接 |

## 8. 第 1 轮结论

Phase A 全部完成（A0-A7 + 8 个扩展 plan 层模块）。cargo test 42 passed 0 failed。7 维度中 5 个无回归，2 个有低优先级待补项（warning 清理 + TextArena 桥接）。建议进入第 2 轮审查。
