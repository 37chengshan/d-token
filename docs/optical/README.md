# Optical / Hybrid Context Optimization 文档入口

更新时间：2026-07-11 晚间

Optical 是 Hybrid Context Optimization Engine 中的一种有损 codec，不是独立产品，也不是当前生产请求的默认压缩路径。

## 当前结论

当前代码只允许：

```text
shadow planning
metadata-only preview
capability lifecycle visibility
provider-specific dependency analysis
redacted recent events
```

当前明确禁止宣称可用：

```text
PNG renderer
active request mutation
protocol patcher
history collapse
post-execution validator
rollback executor
production active routing
```

## 当前文档

1. [`../experiments/OPTICAL_HYBRID_CONTEXT_OPTIMIZATION_PLAN_V7.md`](../experiments/OPTICAL_HYBRID_CONTEXT_OPTIMIZATION_PLAN_V7.md) — 当前实验架构和实施计划。
2. [`implementation-status.md`](implementation-status.md) — 按 2026-07-11 源码整理的实现状态。
3. [`architecture-v2-iteration.md`](architecture-v2-iteration.md) — V7 架构与代码映射。
4. [`data-structures.md`](data-structures.md) — 当前公共结构说明。
5. [`test-matrix.md`](test-matrix.md) — 测试合同与攻击面。
6. [`release-gates.md`](release-gates.md) — active transform 的发布门槛。
7. [`review-round-5.md`](review-round-5.md) — 当前最新审查。
8. [`overview.md`](overview.md) — 产品定位与默认行为。

## 已归档材料

- V6 基础规格：`docs/archive/optical/v6/OPTICAL_COMPRESSION_EXPERIMENT_PLAN.md`。
- V6 架构：`docs/archive/optical/v6/architecture.md`。
- 早期 Phase A/B 报告：`docs/archive/optical/phase-a/` 与 `docs/history/OPTICAL_PHASE_A_IMPLEMENTATION_REPORT.md`。
- Review round 1–4：`docs/archive/optical/reviews/`。

归档文件中的旧测试数、旧分支、未提交状态、工具阻塞和旧路径不代表当前工作区。

## 当前 API

```text
POST /compression/optical/preview
GET  /compression/optical/status
GET  /compression/optical/capabilities
GET  /compression/optical/capabilities/lifecycle
GET  /compression/optical/events/recent
```

这些端点只返回规划状态和脱敏元数据。它们不生成图片、不改写请求体，也不证明真实模型质量或成本收益。

## 验证原则

Optical 的当前验证以源码和本次执行的命令为准，不在入口文档长期硬编码旧测试数量：

```bash
cd code
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p dtoken-types
cargo test -p dtoken-config
cargo test -p dtoken-compress
cargo test -p dtoken-core-api
cargo test --workspace
```

在 renderer、patcher、post-validator、rollback 和真实模型 fidelity gate 全部通过前，`active transform` 必须保持关闭。
