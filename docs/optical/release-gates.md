# Optical / Hybrid Context Optimization 发布门槛

更新日期：2026-07-11

## 1. Shadow Release Gates

Shadow planner 和可见性 API 必须满足：

```text
Shadow mode does not mutate the request body.
Public output contains no raw text, request body, response body, TextRef, source paths, image bytes or base64.
Current user turn remains text.
Recent tail remains text.
System/developer/tool schema remains unchanged.
Open tool state remains unchanged.
Protocol and endpoint match.
Unknown capability cannot become active.
Every candidate has a non-empty action and reason.
Secret-like and diff/patch content is skipped.
Missing request-local text fails closed.
Geometry is required only when a selected action needs it.
Only selected candidates contribute economics totals.
Provider state-machine ambiguity fails closed.
```

当前状态：PASS。Phase A/B shadow planner、lifecycle API 和 metadata-only events 已通过 Rust workspace 回归。

## 2. Capability Snapshot Lifecycle Gates

任何可被 Registry 接受的 active snapshot 必须满足：

```text
schema_version is supported.
snapshot_id is non-empty and unique.
revision > 0.
verified_at_ms > 0.
source is non-empty.
enabled = true.
lifecycle = active.
revoked_at_ms and revocation_reason are absent for active records.
provider/provider-id/model/endpoint identity matches embedded capability.
capability status is Supported.
image encoding and token formula pass policy validation.
```

撤销与取代规则：

```text
Revoked requires revoked_at_ms and non-empty revocation_reason.
Superseded requires non-empty supersedes_snapshot_id.
A valid revoked/superseded revision blocks all older active revisions for the same identity.
Only a newer active revision may restore the capability.
Duplicate snapshot IDs fail closed.
Unknown schema versions fail closed.
```

当前状态：基础生命周期 PASS；authority、signature、trust source、持久化撤销管理 API 尚未完成。

## 3. Stable Evidence Gates

计划证据必须满足：

```text
Fingerprint algorithm and version are explicit.
Raw request bytes use SHA-256 when available.
Canonical JSON fallback sorts object keys and preserves array order.
Debug/public output does not expose digest-adjacent raw content.
Before/after evidence uses the same fingerprint kind and version.
Canonical JSON v1 is not presented as RFC 8785/JCS unless formally implemented.
```

当前状态：RawRequestBytes 和 CanonicalJson SHA-256 v1 实现通过测试；HTTP Preview 目前仍使用 canonical fallback，原始 ingress bytes 尚未接入。

## 4. Provider Tool-State Gates

Anthropic 和 OpenAI 必须分别使用版本化状态机，不得共享未经验证的隐式假设。

```text
AnthropicMessagesV1 handles parallel tool_use and out-of-order matching results.
OpenAiChatV1 handles assistant tool_calls and tool-role results.
Missing result remains open.
Result before use fails closed.
Duplicate tool-use ID fails closed.
Duplicate result fails closed.
Recent tail remains protected.
Only closed historical tool results may become Optical candidates.
```

当前状态：Anthropic 与 OpenAI Chat inventory/graph fixture PASS。OpenAI 仍是独立 shadow analysis，不得进入 active transform。

## 5. Active Capability Gates

任何 active candidate 必须同时满足：

```text
Global policy enabled.
Global allow_active enabled.
Route policy enabled.
Route mode is active-compatible.
Route allows tool_result.
confirmed_experimental_lossy is true.
Capability is an exact Supported match from an active, non-revoked snapshot.
Snapshot revision is newer than any revocation/supersede barrier.
Endpoint supports the selected protocol patch contract.
Candidate is a closed historical tool_result.
Candidate is RenderSafe.
Candidate image count is within global and model limits.
Conservative net savings exceeds route threshold.
Cache stability gate passes.
```

当前状态：基础 validator 存在，但生产 active 仍 BLOCKED。

## 6. Metadata-only Event Gates

OpticalPreview 事件允许包含：

```text
final_status and mode
candidate/preserved/skipped counts
estimated image count
public economics aggregate
graph node/edge/dependency counts
closed/orphan/open tool counts
route-policy presence
configured/verified/rejected capability counts
snapshot revision presence
```

禁止包含：

```text
request body
candidate text
provider/route/model values
source paths
candidate reasons
TextRef
image bytes
base64
credentials or authorization headers
```

当前状态：PASS，集成泄漏测试通过。

## 7. Renderer Gates

```text
Renderer uses the same RenderGeometryProfile as estimator and UI.
Font metrics and CJK/wide glyph behavior are tested.
Paging preserves page order and head/tail rules.
Renderer output count equals plan image count.
Renderer never receives SecretNoDuplicate content.
Rendered bytes never enter logs, events or public summaries.
Renderer timeout and failure return original body.
Fidelity fixtures meet defined thresholds before active rollout.
```

当前状态：NOT IMPLEMENTED。

## 8. Protocol Patcher Gates

第一版只允许 Anthropic `/v1/messages` closed historical tool_result。

必须保留：

```text
tool_use_id
tool_result type
message role
message order
tool_use blocks
system
tools
current user
recent tail
```

必须验证：

```text
Patch path belongs to an approved active candidate.
Patch does not target AlwaysText nodes.
Patched JSON is protocol-valid.
Image count matches renderer output.
Exact sidecar contains no secret-like values.
Original body remains request-local for rollback.
```

当前状态：NOT IMPLEMENTED。

## 9. Post-Execution and Rollback Gates

```text
Post validator re-parses the final body.
Expected source path and node kind still match.
Before/after fingerprints match the approved operation.
Provider request is sent only after all post checks pass.
Any failure returns the untouched original body.
Failure events are metadata-only.
Rollback does not depend on persistent raw storage.
```

当前状态：NOT IMPLEMENTED。

## 10. Commit Record Gates

阶段提交记录必须满足：

```text
Exact stage, timestamps and files are recorded.
Branch is observational only; there is no target-branch field.
Shared files declare hunk-level or mixed staging.
Verification status reflects real command exit status.
Blockers are explicit.
Uncommitted records have submitted=false and null submitted_at/commit_hash.
Committed records have submitted=true, submitted_at and commit_hash.
Real Git operations remain in support/git-operations.json.
```

当前状态：schema v2 代码完成。标准库 unittest 无第三方依赖，但当前 CodexPro safe allowlist 禁止执行 `python3 -m unittest`，本机也没有 pytest，因此执行验证仍 BLOCKED。

## 11. Current Verification

```text
cargo test -p dtoken-compress                 76 unit + 4 integration PASS
cargo test -p dtoken-config                   4 PASS
cargo test -p dtoken-core-api                 35 unit + 6 integration PASS
cargo test -p dtoken-types                    3 PASS
cargo test --workspace                        PASS, exit 0
cargo clippy --workspace --all-targets -- -D warnings  PASS
cargo fmt --all -- --check                    NOT RUN: safe allowlist blocked
Python ledger unittest                        NOT RUN: safe allowlist blocked
```

## 12. 当前发布判断

```text
Shadow planner                         PASS
Capability lifecycle foundation        PASS
Stable fingerprint foundation          PASS
Provider-specific state fixtures       PASS
Metadata-only visibility APIs/events   PASS
Commit ledger execution verification   BLOCKED BY TOOL ENVIRONMENT
Desktop Optical Lab                    NOT IMPLEMENTED
Renderer                               BLOCKED
Protocol patcher                       BLOCKED
History active transform               BLOCKED
Production active transform            BLOCKED
```
