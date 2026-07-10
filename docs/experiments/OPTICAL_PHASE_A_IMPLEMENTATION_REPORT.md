# Optical Phase A / Phase B Implementation Report

更新日期：2026-07-10

> 本报告已替代早期“crate export 未完成、cargo 不可用”的状态。那些阻塞已经解决。

## 1. 当前状态

```text
Branch: optical
Phase A shadow planner: completed and optimized
Phase B visibility API: basic endpoints completed
Active renderer/patcher: not implemented
```

## 2. Phase A 已完成

```text
pub mod optical crate export
public optical types and policies
explicit capability registry
protocol/path/policy applicability gates
request-local TextArena and TextRef
Anthropic conversation inventory
semantic priority protection
secret/exact/diff risk firewall
capability-driven render geometry
selected-only economics aggregation
shadow plan builder
structured shadow/active validation inputs
redacted public summary
```

## 3. Phase B 已完成

```text
POST /compression/optical/preview
GET  /compression/optical/status
GET  /compression/optical/capabilities
```

这些端点只提供 metadata-only visibility。当前没有 active config、renderer 或 request mutation。

## 4. 本轮重要修复

```text
真实候选文本进入 risk pipeline
TextRef 代替 inventory raw String
secret/diff 扫描整个 block
skipped candidate 不进入 economics totals
capability registry 默认不授予 active
geometry 使用 capability formula 和 safety margin
validator 验证真实 body/summary/candidate evidence
protocol/endpoint mismatch fail closed
active requires explicit confirmation and budgets
```

## 5. 测试结果

```text
cargo test -p dtoken-compress
58 unit passed
4 integration passed
0 failed

cargo test -p dtoken-core-api
35 unit passed
4 Optical API integration passed
0 failed

cargo test -p dtoken-types
3 passed
0 failed

cargo check -p dtoken-compress
Finished, 0 error

cargo check -p dtoken-core-api
Finished, 0 error

cargo test --workspace --exclude d-token-desktop
all non-desktop Rust tests passed
0 failed
```

完整 workspace 仍被以下桌面 sidecar 缺失阻塞：

```text
code/apps/desktop/src-tauri/binaries/dtoken-proxy-aarch64-apple-darwin
```

该问题与 Optical 代码无关。

## 6. 尚未完成

```text
capability snapshot persistence/revisioning
real route policy loading
preview event ledger
Desktop Optical Lab
ContextGraph dependency edges
estimate ranges and calibration
renderer
paging/reflow
Anthropic protocol patcher
post-execution validator
rollback executor
history compression
```

## 7. 当前发布判断

```text
Phase A shadow planner: can merge
Phase B visibility API: can merge
Production active transform: must not release
```

## 8. 当前参考文档

```text
docs/experiments/OPTICAL_HYBRID_CONTEXT_OPTIMIZATION_PLAN_V7.md
docs/experiments/OPTICAL_COMPRESSION_EXPERIMENT_PLAN.md
docs/optical/architecture-v2-iteration.md
docs/optical/implementation-status.md
docs/optical/review-round-3.md
```
