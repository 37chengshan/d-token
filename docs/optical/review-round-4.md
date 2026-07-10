# Optical / Hybrid Context Optimization 多维度审查报告（第 4 轮）

审查日期：2026-07-11

审查范围：

```text
N0 Shadow safety foundation
N1 Pending commit ledger
N2 Versioned capability snapshots
N3 ContextGraph and immutable Plan IR
N4 Configured Preview API and metadata-only events
```

## 1. 最终结论

本轮将 Optical 从“模块齐全的 shadow planner”推进到“具有版本化配置、依赖图、不可变证据和审计事件的 shadow control plane”。

当前判断：

```text
Shadow planner code quality                 PASS
Capability default safety                  PASS
Context dependency fail-closed behavior    PASS
Preview API privacy                        PASS
Rust workspace regression                  PASS
Clippy -D warnings                         PASS
Pending-commit Python test environment      BLOCKED
Formatter check                            NOT EXECUTED by tool policy
Production active transform                BLOCKED by design
```

可以按 `support/pending-commits.json` 中的阶段计划进行精确暂存和提交，但不得一次性提交整个脏工作区。

## 2. 产品与架构审查

### 2.1 产品定位

V7 定位保持正确：Optical 是 Hybrid Context Optimization Engine 的一种有损 codec，而不是整个优化系统。

本轮没有实现 renderer 或 request mutation，符合实施顺序：

```text
Capability lifecycle
→ Context dependency graph
→ Immutable plan evidence
→ Metadata-only observability
→ Fidelity tests
→ Renderer/Patcher
```

结论：通过。

### 2.2 分层边界

当前职责分层：

```text
dtoken-types
  public policy/config/snapshot/event enums

dtoken-config
  backward-compatible runtime config loading

dtoken-compress
  registry, graph, risk, economics, plan and validation

dtoken-core-api
  configured preview, status, capabilities and recent events

dtoken-event-ledger
  persistent metadata-only event storage
```

Core API 不负责实现 risk、geometry 或 graph 算法；compress crate 不负责持久化 Event Ledger。边界清楚。

结论：通过。

## 3. 提交记录系统审查

### 3.1 发现的问题

原有 `support/git-operations.json` 的语义是“记录已经准备执行或执行过的真实 Git 操作”。如果把尚未提交的计划写进去，会造成审计记录失真。

### 3.2 本轮设计

新增独立账本：

```text
support/pending-commits.json
```

新增命令：

```text
plan
mark-plan
query-plans
```

状态：

```text
pending
ready
blocked
committed
superseded
```

实际 Git 操作仍必须写入：

```text
support/git-operations.json
```

两个文件不可互相替代。

### 3.3 当前限制

`support/scripts/test_git_operation_audit.py` 已编写，但本机没有 `pytest`：

```text
pytest: command not found
exit code 127
```

因此 N1 正确标记为 `blocked`，没有虚报 ready。

结论：设计通过；自动化测试环境未满足。

## 4. Capability Snapshot 审查

### 4.1 新增能力

配置结构：

```text
OpticalRuntimeConfig
OpticalGlobalPolicy
RouteOpticalPolicyEntry
OpticalCapabilitySnapshot
```

Snapshot 身份：

```text
snapshot_id
revision
enabled
provider_family
provider_id
model_base
endpoint
verified_at_ms
source
capability
```

Registry 仅接受：

```text
enabled = true
revision > 0
verified_at_ms > 0
non-empty snapshot id/model/source
capability status = Supported
provider/provider_id/model/endpoint exact identity match
```

相同身份存在多个 snapshot 时，选择最高 revision。

### 4.2 安全默认值

```text
Default registry = empty
Default global enabled = false
Default allow_active = false
Default route mode = off
Legacy config without optical section = safe default
```

配置示例不包含任何 active snapshot。

### 4.3 剩余风险

1. Snapshot `source` 目前只是字符串，没有签名或信任链。
2. 没有管理 API 对 snapshot 进行新增、撤销和审计。
3. `register_verified` 和 experimental constructors 为兼容测试保留；active 发布前应移动到测试 fixture 或限制可见性。
4. Disabled snapshot 当前计入 rejected count，后续可拆分为 ignored/rejected 两个指标。
5. Snapshot schema 本身尚无独立 schema version。

结论：Shadow 与配置读取阶段通过；Production active lifecycle 未完成。

## 5. ContextGraph 审查

### 5.1 已实现关系

```text
Sequence
ToolUseToResult
```

节点包含：

```text
block id
source JSON path
role
block kind
turn index
tool id
character count
initial protection
```

### 5.2 Fail-closed 边界

以下结果不能成为 Optical candidate：

```text
没有对应 tool_use 的 tool_result
同一 tool ID 对应多个 tool_use
同一 tool_use 对应多个 tool_result
tool_result 出现在 tool_use 之前或同 turn
仍位于 recent tail 的 tool_result
```

重复 tool ID 或重复 result 会移除已建立的 dependency edge，并将相关结果标记为 orphan/ambiguous。

### 5.3 剩余限制

1. 当前只实现 Anthropic message shape。
2. 尚未表达 summary→source、cache zone、schema dependency 等边。
3. Parallel tool use 仅按 ID 和 turn 顺序判断，没有 provider-specific state machine。
4. Source path 是内部结构路径，尚未经过 active patcher 的路径重验证。

结论：Phase A closed historical tool-result gate 通过；完整 ContextGraph 尚未完成。

## 6. Immutable Plan IR 审查

新增：

```text
ContextOptimizationPlan
PlanCandidateEvidence
```

保存：

```text
plan id
original body fingerprint
planned body fingerprint
capability snapshot id/revision
graph summary
candidate evidence
public summary
```

安全属性：

```text
不保存 raw body
不实现公开序列化
Debug 不打印原文
SHA-256 用于 body evidence
```

### 剩余限制

当前 fingerprint 对 `serde_json::Value` 的序列化结果计算 SHA-256，适合同一请求生命周期内比较，但不是跨实现 canonical JSON hash。进入 active patcher 前应使用：

```text
原始请求字节 fingerprint
或规范化 JSON canonicalization
```

结论：Shadow evidence 通过；跨进程/跨序列化一致性未完成。

## 7. Decision Pipeline 审查

当前管线：

```text
Applicability with Registry
→ Capability Resolution
→ TextArena Inventory
→ ContextGraph
→ Semantic Priority
→ Real-text Risk Classification
→ Geometry
→ Economics
→ Candidate Decision
→ Structured Validation
→ Immutable Plan
```

重要修复：

```text
只有 closed historical tool_result 可进入 shadow estimate
orphan/ambiguous tool result fail closed
Capability gate 和 resolver 使用同一个 registry
Capability gate 记录 verified snapshot revision 状态
```

结论：数据一致性显著提升，未发现 shadow body mutation 路径。

## 8. Preview API 与 Event Ledger 审查

### 8.1 配置读取

以下端点不再使用函数内硬编码默认值：

```text
POST /compression/optical/preview
GET  /compression/optical/status
GET  /compression/optical/capabilities
```

它们读取：

```text
state.config.optical.global_policy
route-scoped optical policy
versioned capability snapshots
```

### 8.2 新事件端点

```text
GET /compression/optical/events/recent
```

只返回 `LayeredStage::OpticalPreview` 事件。

当前事件 metadata：

```text
kind
candidate_blocks
route_policy_found
verified_rules
```

不包含：

```text
request body
candidate raw text
provider id value
route id value
model value
TextRef
image bytes
base64
```

集成测试使用私有 marker、provider、route 和 model 值进行泄漏检查，全部未出现在事件中。

### 8.3 剩余限制

事件尚未包含完整 graph/economics 汇总。由于 Core API 文件中的既有敏感测试 fixture 会触发 CodexPro 大补丁扫描，本轮保留了最小安全 metadata。后续可在本地精确修改后增加：

```text
graph node/edge counts
closed/orphan/open counts
estimated image count
net savings
final plan status
```

不得加入任何具体 identity 值。

结论：隐私合同通过；可观测字段仍可扩充。

## 9. 测试与工具链证据

### 9.1 Optical Compress

```text
cargo test -p dtoken-compress
67 unit passed
4 integration passed
0 failed
```

### 9.2 Config

```text
cargo test -p dtoken-config
4 passed
0 failed
```

### 9.3 Core API

```text
cargo test -p dtoken-core-api
35 unit passed
5 Optical API integration passed
0 failed
```

### 9.4 Types

```text
cargo test -p dtoken-types
3 passed
0 failed
```

### 9.5 Full Workspace

```text
cargo test --workspace
all tests passed
exit code 0
```

桌面测试输出：

```text
kill: 999999: No such process
```

但测试总体退出码为 0。该输出应作为桌面测试卫生问题单独清理，不能算作测试失败。

### 9.6 Clippy

```text
cargo clippy --workspace --all-targets -- -D warnings
passed
0 warnings
```

### 9.7 Formatter

```text
cargo fmt --all -- --check
```

被 CodexPro safe bash allowlist 拒绝，未执行。不得声明格式检查通过。

### 9.8 Python Ledger Test

```text
pytest support/scripts/test_git_operation_audit.py
```

失败原因：本机未安装 pytest，exit 127。不是测试断言失败。

## 10. 提交可审计性审查

`support/pending-commits.json` 当前阶段：

```text
N0 ready    Shadow safety foundation
N1 blocked  Pending ledger test environment missing
N2 ready    Capability snapshots/config
N3 ready    ContextGraph/Plan IR
N4 ready    Preview events
N5 ready    Documentation and round-four review
```

多个阶段涉及同一个文件：

```text
code/crates/dtoken-types/src/lib.rs
code/crates/dtoken-compress/src/optical/mod.rs
```

因此实际提交必须使用 hunk-level staging，不能按整文件粗暴暂存。

在执行每一个真实 commit 前，必须先运行：

```text
python3 support/scripts/git_operation_audit.py record --operation commit ...
```

本轮没有执行任何 Git commit，因此所有 `commit_hash` 正确保持 null。

最终核对时发现工作区分支已被外部操作从本轮目标 `optical` 切换为：

```text
codex/prd-macos-polish
```

本轮没有擅自切换分支。`support/pending-commits.json` 已记录当前分支、预期提交分支和警告；真实暂存或提交前必须先确认目标分支，并重新检查工作树中其他人的并行改动。

## 11. 代码质量评分

| 维度 | 评分 | 说明 |
|---|---:|---|
| 产品定位 | 9.2/10 | Optical 保持 codec 定位 |
| 安全默认值 | 9.4/10 | 配置、registry、route 全部默认关闭 |
| Capability lifecycle | 8.0/10 | 已版本化，缺签名/撤销管理 |
| Dependency correctness | 8.7/10 | orphan、重复 use/result、recent 全部 fail closed |
| Raw content containment | 9.3/10 | TextArena + metadata-only API/event |
| Plan evidence | 8.5/10 | SHA-256 evidence，缺 canonical/raw-byte hash |
| API privacy | 9.1/10 | 集成泄漏测试通过 |
| Observability | 7.5/10 | 事件可用，字段仍偏少 |
| Test coverage | 9.0/10 | 完整 workspace 和 Clippy 全绿 |
| Commit auditability | 8.0/10 | 账本完整，Python 环境阻塞 |
| Active readiness | 3.5/10 | renderer/patch/post-validator 未实现 |

## 12. 发布判断

```text
N0 Shadow foundation                  可以提交
N2 Capability snapshots/config        可以提交
N3 ContextGraph/Plan IR                可以提交
N4 Preview events                      可以提交
N1 Pending ledger                      暂缓，等待 Python test 环境
N5 Documentation                       完成后可以提交
Production active transform            禁止发布
```

## 13. 下一轮优先级

```text
P0 运行并修复 pending ledger Python tests
P0 Capability snapshot schema version + revocation/audit API
P0 Raw-byte or canonical JSON plan fingerprints
P0 Provider-specific closed tool state machine fixtures
P1 Event metadata 扩展为完整 graph/economics aggregate
P1 EstimateRange: P10 / Expected / P90
P1 Fidelity and adversarial fixture corpus
P2 Desktop Optical Lab consuming public summary/events
P2 Renderer behind feature flag
P3 Anthropic patcher + post-execution validator + rollback
```

## 14. 审查结语

本轮最大的价值不是“增加了更多 Optical 代码”，而是把未来 active transform 所需要的证据链向前推进：

```text
配置来源可追踪
能力规则有 revision
工具依赖可验证
候选路径可定位
请求变更可 fingerprint
预览行为可审计
提交计划可分阶段追踪
```

在 renderer 开发之前，仍应优先完成 capability trust、canonical evidence 和 provider-specific dependency fixtures。
