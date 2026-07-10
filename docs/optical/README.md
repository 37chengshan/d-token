# d-token Optical / Hybrid Context Optimization 文档索引

本目录汇总 Optical Codec 与 Hybrid Context Optimization 的架构、实现状态、测试合同和审查记录。

## 当前真理源

1. 当前架构与实施计划：
   `docs/experiments/OPTICAL_HYBRID_CONTEXT_OPTIMIZATION_PLAN_V7.md`
2. 最新实现状态：
   `docs/optical/implementation-status.md`
3. 最新测试矩阵：
   `docs/optical/test-matrix.md`
4. 最新发布门槛：
   `docs/optical/release-gates.md`
5. 最新多维度审查：
   `docs/optical/review-round-5.md`
6. 分阶段改动和提交状态：
   `support/pending-commits.json`
7. V6 基础完整规格：
   `docs/experiments/OPTICAL_COMPRESSION_EXPERIMENT_PLAN.md`

V7 和第五轮审查优先于 V6 中与产品定位、Capability lifecycle、fingerprint、Provider state machine、events 和 commit ledger 冲突的内容。

## 文件说明

```text
README.md                       本索引
architecture.md                 V6 基础架构
architecture-v2-iteration.md    V7 架构迭代和代码映射
overview.md                     产品边界和默认行为
data-structures.md              公共数据结构
implementation-status.md        最新实现状态矩阵
test-matrix.md                  最新测试矩阵
release-gates.md                最新发布门槛
review-round-1.md               第一轮审查
review-round-2.md               第二轮审查
review-round-3.md               第三轮安全架构审查
review-round-4.md               Capability/ContextGraph/Event 审查
review-round-5.md               Lifecycle/Fingerprint/Provider/Commit 审查
```

## 当前产品边界

Optical 不是独立的“文本截图器”，而是 Hybrid Context Optimization Engine 中的一种有损 codec。

当前已完成：

```text
plan-only shadow planner
safe global/route policy gates
versioned capability snapshots
active/revoked/superseded lifecycle and revision barrier
request-local TextArena and real-text risk classification
AnthropicMessagesV1 tool state machine
OpenAiChatV1 inventory/graph lowering
raw-byte and canonical JSON SHA-256 evidence
preview/status/capabilities/lifecycle/events APIs
metadata-only event aggregates
pending commit ledger schema v2
```

当前未完成且禁止宣称可用：

```text
Desktop Optical Lab
PNG renderer
protocol patcher
active request mutation
history collapse
post-execution validator
rollback executor
capability authority/signature/trust chain
HTTP ingress raw-byte evidence wiring
production capability lifecycle management
```

## 当前验证

```text
cargo test -p dtoken-compress      76 unit + 4 integration passed
cargo test -p dtoken-config        4 passed
cargo test -p dtoken-core-api      35 unit + 6 integration passed
cargo test -p dtoken-types         3 passed
cargo test --workspace             passed, exit 0
cargo clippy --workspace --all-targets -- -D warnings  passed
```

未完成：

```text
cargo fmt --all -- --check
Python standard-library pending-ledger tests
```

两者均受当前工具环境限制，详情见 `implementation-status.md`。

## 提交记录规则

`support/pending-commits.json` 不设置目标分支。每个阶段记录：

```text
时间
阶段
文件
验证
阻塞项
shared files / staging mode
submitted
submitted_at
commit_hash
```

`branch` 只表示记录时工作区实际所在分支。真实 Git 操作单独写入 `support/git-operations.json`。
