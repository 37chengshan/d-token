# Optical / Hybrid Context Optimization 文档入口

更新时间：2026-07-15

Optical 是 Hybrid Context Optimization Engine 中的一种有损 codec，不是独立产品，也不是当前生产请求的默认压缩路径。

## 当前结论

当前后端代码已经实现：

```text
shadow planning
metadata-only preview
capability lifecycle visibility
provider-specific dependency analysis
OpenAI Responses shadow inventory and OpenAiResponsesV1 state graph
redacted recent events
protected Context IR with content/risk/action/cache evidence and safe semantic codec
deterministic bounded PNG renderer with dynamic final-page geometry
sealed Anthropic patcher and artifact-derived validator
direct Anthropic JSON/SSE forwarding and raw-byte fallback
signed capability/eval gate, per-page multimodal accounting and economics ranges
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

1. [`../experiments/HYBRID_CONTEXT_OPTIMIZATION_ENGINE_PLAN_V8.md`](../experiments/HYBRID_CONTEXT_OPTIMIZATION_ENGINE_PLAN_V8.md) — 当前完整后端迭代合同。
2. [`../experiments/OPTICAL_HYBRID_CONTEXT_OPTIMIZATION_PLAN_V7.md`](../experiments/OPTICAL_HYBRID_CONTEXT_OPTIMIZATION_PLAN_V7.md) — 上一轮架构记录。
3. [`implementation-status.md`](implementation-status.md) — 按实际源码整理的实现状态。
4. [`architecture-v2-iteration.md`](architecture-v2-iteration.md) — V7 架构与代码映射。
5. [`data-structures.md`](data-structures.md) — 当前公共结构说明。
6. [`test-matrix.md`](test-matrix.md) — 测试合同与攻击面。
7. [`release-gates.md`](release-gates.md) — active transform 的发布门槛。
8. [`review-round-5.md`](review-round-5.md) — 当前最新审查。
9. [`overview.md`](overview.md) — 产品定位与默认行为。

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

桌面正式 UI 提供「视觉压缩（Beta）」一级页（Windows/macOS 共享），信息架构为「一键启用 + 高级配置 + 状态与预览」；经 Tauri 代理上述端点做 Shadow 安装配置与观测，不渲染 PNG/base64，不默认开启 active，估算节省不得标为账单。一键区同时提供独立的 Claude Code/Codex/OpenCode 接入事务；规则、Hook/Plugin、Skill 与可选 metadata-only MCP 不参与请求压缩，也不改变 Shadow/Active 状态。

真实请求的 Optical capsule 由 proxy 热路径直接构造为：`RenderedContextPointer → PNG pages → optional ExactFactSheet → HistoryBoundaryGuard`。这套合同不依赖全局提示词、Hook、Skill 或 MCP；后四者只用于 Agent 解释、状态诊断和不确定精确值的原生回取。

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

2026-07-15 发布前加固已完成共享 SecretClassifier、统一 renderer/retained-request byte budget、stream 终态 Ledger、持久 Agent transaction、bridge 平台签名先验校验与 UI fail-closed 恢复。本地最终门为 Rust `467/467`、Vitest `175/175`、Chrome Playwright `10/10`；这些结果不构成真实模型、账单、Windows 实机或 production Active 证据。Capability registry 保持为空。
