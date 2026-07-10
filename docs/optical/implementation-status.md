# Optical / Hybrid Context Optimization 实现状态矩阵

更新日期：2026-07-11

## 1. 当前定位

Optical 是 Hybrid Context Optimization Engine 中的一种有损 codec。当前交付范围仍是：

```text
shadow planning
metadata-only preview/control plane
capability lifecycle visibility
provider-specific dependency analysis
```

当前仍禁止：

```text
真实图片渲染
请求 mutation
active provider routing
history collapse
persistent raw recovery cache
```

## 2. 阶段状态

| 阶段 | 内容 | 当前状态 |
|---|---|---|
| N0 | applicability、real-text risk、geometry、economics、structured validator | ready，未提交 |
| N1 | 第一版 pending ledger | superseded by N6 |
| N2 | OpticalRuntimeConfig + versioned capability snapshots | ready，未提交 |
| N3 | ContextGraph + immutable Plan IR | ready，未提交 |
| N4 | configured Preview API + OpticalPreview events | ready，未提交 |
| N5 | round-four docs/review | ready，未提交 |
| N6 | pending commit ledger schema v2 + validation rules | blocked：实现完成，Python 命令受工具环境阻塞 |
| N7 | Capability lifecycle/revocation/supersede | ready，未提交 |
| N8 | raw-byte + canonical JSON fingerprints | ready，未提交 |
| N9 | Anthropic/OpenAI provider tool-state fixtures | ready，未提交 |
| N10 | lifecycle endpoint + expanded metadata-only events | ready，未提交 |
| N11 | top-level docs + round-five review | ready，未提交 |

每个阶段的时间、文件、验证、共享文件暂存方式和提交状态以：

```text
support/pending-commits.json
```

为准。账本不设置目标分支；`branch` 只记录该阶段写入时工作区实际所在分支。

## 3. Capability Snapshot 生命周期

已完成：

```text
snapshot schema_version = 1
Active / Revoked / Superseded lifecycle
revoked_at_ms and revocation_reason
supersedes_snapshot_id metadata
exact provider/provider-id/model/endpoint identity
highest active revision selection
duplicate snapshot ID rejection
revoked/superseded revision barrier
newer active revision may restore capability
default registry remains empty and fail closed
metadata-only lifecycle summary endpoint
```

关键安全语义：如果 revision 2 是有效撤销记录，revision 1 的 active rule 不会自动回退生效；只有 revision 大于 2 的新 active snapshot 才可恢复。

未完成：

```text
snapshot authority/signature/trust chain
revocation management write API
source authority validation
snapshot schema migration framework
persistent revocation audit stream
```

## 4. Provider-specific ContextGraph

已完成：

```text
AnthropicMessagesV1 state machine
OpenAiChatV1 state machine
source JSON paths
Sequence edges
ToolUseToResult edges
parallel tool uses and out-of-order results
closed historical tool-result gate
orphan result detection
duplicate tool-use ID fail closed
duplicate result fail closed
recent-tail preservation
open tool-use tracking
OpenAI tool_calls/tool-role inventory lowering
```

OpenAI lowering 当前只用于独立 shadow analysis 和 fixture 验证，没有接入 active transform，也没有获得图片能力。

未完成：

```text
OpenAI Responses state machine
provider-specific malformed sequence corpus
summary-to-source edges
cache-zone edges
schema dependencies
cross-provider normalized execution state
```

## 5. Immutable Plan 与稳定指纹

已完成：

```text
ContextOptimizationPlan
PlanCandidateEvidence
BodyFingerprintEvidence
RawRequestBytes SHA-256 v1
CanonicalJson SHA-256 v1
object keys sorted deterministically
array order preserved
capability snapshot id/revision evidence
provider state-machine graph summary
metadata-only Debug
```

当前 Preview API 经 Axum `Json` extractor 后调用 canonical JSON fallback。Raw-byte 指纹入口已存在，但尚未接入 HTTP ingress 的原始请求字节。

Canonical JSON v1 是项目内部确定性编码，不宣称符合 RFC 8785/JCS。

未完成：

```text
HTTP ingress raw-byte retention
JCS/RFC 8785 compatibility decision
patch operation evidence
rollback journal reference
post-execution validation evidence
```

## 6. API 与 Event Ledger

已完成端点：

```text
POST /compression/optical/preview
GET  /compression/optical/status
GET  /compression/optical/capabilities
GET  /compression/optical/capabilities/lifecycle
GET  /compression/optical/events/recent
```

Preview Event 当前包含：

```text
final_status and mode
candidate/preserved/skipped counts
estimated image count
public economics summary
graph node/edge/tool-dependency counts
closed/orphan/open tool counts
route-policy presence
configured/verified/rejected capability counts
snapshot revision presence
```

事件不包含：

```text
request body
candidate raw text
provider/route/model values
source paths
candidate reasons
TextRef
image bytes
base64
```

未完成：

```text
config mutation endpoints
snapshot management write endpoints
signed lifecycle audit
active route execution
```

## 7. 提交记录系统 schema v2

`support/pending-commits.json` 记录：

```text
阶段和说明
created_at / updated_at
记录时实际 branch（仅观察）
精确 files
staging_mode
shared_files
verification status/checks
blockers
submitted / submitted_at / commit_hash
```

真实 Git 操作继续单独写入：

```text
support/git-operations.json
```

没有执行 commit 时必须保持：

```text
submitted = false
submitted_at = null
commit_hash = null
```

## 8. 最新测试证据

```text
cargo test -p dtoken-compress
→ 76 unit passed
→ 4 integration passed
→ 0 failed

cargo test -p dtoken-config
→ 4 passed
→ 0 failed

cargo test -p dtoken-core-api
→ 35 unit passed
→ 6 Optical API integration passed
→ 0 failed

cargo test -p dtoken-types
→ 3 passed
→ 0 failed

cargo test --workspace
→ all tests passed
→ exit code 0

cargo clippy --workspace --all-targets -- -D warnings
→ passed
→ 0 warnings
```

桌面测试仍输出非失败噪声：

```text
kill: 999999: No such process
```

但完整 workspace 总退出码为 0。

未完成的工具验证：

```text
cargo fmt --all -- --check
→ CodexPro safe allowlist 拒绝，未执行

python3 -m unittest support.scripts.test_git_operation_audit
→ CodexPro safe allowlist 拒绝，未执行

pytest support/scripts/test_git_operation_audit.py
→ pytest 未安装，exit 127
```

## 9. Phase C–F

| Phase | 状态 | 主要剩余内容 |
|---|---|---|
| C Desktop Optical Lab | 未完成 | lifecycle、events、risk、economics、gates UI |
| D Renderer/Patcher | 未完成 | reflow、paging、renderer、Anthropic patch、post validator、rollback |
| E History/Cache | 未完成 | cache zones、history amortization、closed history policies |
| F Calibration | 未完成 | actual usage、latency、quality metadata calibration |

## 10. 当前发布判断

```text
Shadow planner                         可以按阶段提交
Capability lifecycle/control plane     可以按阶段提交
Stable fingerprint foundation          可以按阶段提交
Provider-specific state fixtures       可以按阶段提交
Metadata-only APIs/events              可以按阶段提交
Pending ledger implementation          代码完成，执行验证受环境阻塞
Production active transform            不可发布
```

## 11. 下一实施顺序

```text
1. 在可运行 Python 标准库测试的环境验证 ledger schema v2
2. Capability authority/signature/trust source + read-only revocation audit
3. HTTP ingress raw-byte fingerprint wiring
4. Provider-specific malformed/adversarial state-machine fixtures
5. Economics P10 / Expected / P90 ranges
6. Desktop Optical Lab
7. Fidelity fixture suite
8. Renderer feature flag
9. Anthropic patcher + post validator + rollback
```
