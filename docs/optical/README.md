# Optical / Hybrid Context Optimization 文档入口

更新时间：2026-07-12

Optical 是 Hybrid Context Optimization Engine 中的一种有损 codec，不是独立产品，也不是当前生产请求的默认压缩路径。

## 当前结论

当前后端代码已经实现：

```text
shadow planning
metadata-only preview
capability lifecycle visibility
provider-specific dependency analysis
redacted recent events
protected Context IR and safe semantic codec
deterministic bounded PNG renderer
sealed Anthropic patcher and artifact-derived validator
direct Anthropic JSON/SSE forwarding and raw-byte fallback
signed capability/eval gate and multimodal accounting
```

当前明确禁止宣称默认可用或已验证节省：

```text
history collapse
OpenAI Optical active patching
unsigned or unbound model capability
production active without real-provider signed eval
post-commit automatic retry/rollback
provider-actual savings without paired evidence
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

这些控制面端点只返回规划状态和脱敏元数据。真正的 Hybrid 执行位于 `/v1/messages` 同协议路由热路径；控制面本身不生成图片，也不证明真实模型质量或成本收益。

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

在真实模型 fidelity/cost/latency eval、精确签名 Capability 和同候选版 routed evidence 全部通过前，`active transform` 必须保持默认关闭。
