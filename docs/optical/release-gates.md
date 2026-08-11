# Optical / Hybrid Context Optimization 发布门槛

更新日期：2026-07-15

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

当前状态：加载路径已实现 Ed25519 authority/signature、有效期、精确 provider/route/model/endpoint 绑定，以及 renderer/font/eval digest 校验；无签名、篡改或 eval 不合格均 fail closed。持久化 lifecycle 写 API 仍未实现。

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

当前状态：RawRequestBytes 和 CanonicalJson SHA-256 v1 已实现；生产 `/v1/messages` ingress 保留 raw bytes，shadow/passthrough/pre-send fallback 直接发送原始 bytes。Preview API 仍以 canonical fallback 为主。

## 4. Provider Tool-State Gates

Anthropic 和 OpenAI 必须分别使用版本化状态机，不得共享未经验证的隐式假设。

```text
AnthropicMessagesV1 handles parallel tool_use and out-of-order matching results.
OpenAiChatV1 handles assistant tool_calls and tool-role results.
OpenAiResponsesV1 handles message/function_call/function_call_output items by call_id in shadow only.
Missing result remains open.
Result before use fails closed.
Duplicate tool-use ID fails closed.
Duplicate result fails closed.
Recent tail remains protected.
The most recent six Responses completed call pairs remain protected.
Only closed historical tool results may become Optical candidates.
```

当前状态：Anthropic、OpenAI Chat 与 OpenAI Responses inventory/graph fixture PASS。OpenAI 仍是独立 shadow analysis，不得进入 active transform。

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

当前状态：active executor 已实现并保持默认关闭。只有受信 runtime config 加载的签名 Capability、通过的绑定 eval 和全部运行时门同时满足时才可达。C5 切片（PR-C5C/C5D）已交付 d-token 自有的 Ed25519 capability/eval 权威生命周期工具（`examples/optical_capability_signer.rs`）、带签名的精确 Anthropic 路由演示 snapshot/eval/权威 fixture，以及通过生产 `from_runtime_config` 加载路径的测试自有 signed route 自动 active 正例与 mutation/secret/quality/economics/cache/resource PreserveText 负例（`tests/optical_active_contract.rs`）。仓库默认配置仍然 fail-closed（`allow_active=false`、空 registry）；真实 Provider 付费 eval 与双平台 N10 U7 证据未完成前，production default 保持 BLOCKED，发布结论不得使用 demo fixture 冒充真实 Provider 证据。

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
Renderer output is deterministic and self-decodes as grayscale PNG with exact geometry.
Deadline, cancellation, queue, page, pixel, PNG, encoded-image, body and memory budgets fail closed.
```

当前状态：renderer 与资源治理已实现，确定性/自解码/取消/预算回归通过；真实模型 fidelity gate 尚未完成。

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

当前状态：Anthropic typed sealed patcher 已实现；只接受 Protected Context IR 生成的 approved target，并保留 tool_use_id、角色、顺序、system、tools、当前轮和近期尾部。OpenAI active patcher 未实现。

## 9. Post-Transform Validation and Fallback Gates

```text
Post validator re-parses the final body.
Expected source path and node kind still match.
Before/after fingerprints match the approved operation.
Provider request is sent only after all post checks pass.
Any failure returns the untouched original body.
Failure events are metadata-only.
Pre-send fallback does not depend on persistent raw storage.
After upstream send commit, original-body automatic retry is forbidden.
```

当前状态：artifact-derived validator 和 send-state/fallback 状态机已实现；validator 验证 image envelope、base64、PNG、digest，并证明 approved path 外 JSON 完全不变。pre-send 回退原文，post-commit 不自动重试。

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

当前状态：schema v2 代码存在；本轮结构检查已通过。该账本不替代 Hybrid runtime、模型质量或真实 routed evidence。

## 11. Current Verification

```text
python3 support/scripts/check_structure.py                 PASS
python3 -m unittest discover -s support/tests              PASS, 7 tests
cargo fmt --all -- --check                                 PASS
cargo clippy --workspace --all-targets -- -D warnings     PASS
cargo test --workspace                                     PASS, 467 tests / 50 suites
npm run test:run                                           PASS, 35 files / 175 tests
npx tsc --noEmit                                           PASS
npm run build                                              PASS
Playwright desktop-chromium                                PASS, 10/10
macOS sidecar preparation dry-run                          PASS
git diff --check                                           PASS
Playwright WebKit                                          NOT RUN: local binary absent; no dependency install
real Provider / signed Capability candidate                BLOCKED: no safe credential reference/eval artifact
current-run OpenCode -> d-token -> upstream                NOT RUN: no safe OS credential reference
```

## 12. 当前发布判断

V2 Codex视觉评测的实验门独立于生产Capability门：

```text
Stage 0 provenance/license/font/golden/profile/secret/hybrid/path/budget/isolation PASS
Stage 1 each model selects highest-density passing R/B/P; H is research-only
Stage 2 six independent scenes satisfy LAB_PASS
Stage 3 top two combinations satisfy sentinel pass^3
Relay cost uses original/standard amount only; multiplier-adjusted amount excluded
Private model slug identity and usage evidence complete
```

即使全部通过也只能标记 `LAB_CANDIDATE`；不得自动注册Capability或启用active。当前V2没有执行付费调用，因此该门仍为 `NOT RUN`。

pxpipe 评测配方参考 pin：v0.12.0 `0ceabbed941cf7e95d395153825e944ea9f82e2c`（2026-08-06 release；上游 LICENSE 仍为 MIT、无 NOTICE 文件；取代 v0.8.0 `7b7d6efa`）。1928×1928 仅为上游历史 audit 页（`LEGIBILITY-AUDIT-2026-07-01.md` 中 "old page" CLAMPED），不构成生产 profile。

```text
Shadow planner                         PASS
Capability lifecycle foundation        PASS
Signed capability/eval load gate       PASS
Stable fingerprint foundation          PASS
Provider-specific state fixtures       PASS
Metadata-only visibility APIs/events   PASS
Protected Context IR / semantic codec  PASS
Deterministic bounded PNG renderer     PASS
Anthropic sealed patch/validator       PASS
Send-state and pre-send fallback       PASS
Signed multimodal accounting           PASS
Desktop Visual Compression Beta UI     IMPLEMENTED; REAL WINDOWS/AGENT ACCEPTANCE PENDING
Hybrid Agent integration transaction   IMPLEMENTED; ISOLATED CONFIG EVIDENCE ONLY
Agent bridge Hook/MCP metadata path     IMPLEMENTED; SIGNATURE-BEFORE-EXECUTION CONTRACT PASS; REAL AGENT TRUST/ROUTE EVIDENCE PENDING
OpenAI Optical active patcher          NOT IMPLEMENTED (shadow-only contract)
History collapse/cache-zone            IMPLEMENTED; WIRED INTO ACTIVE PLAN PATH; N10 E2E EVIDENCE PENDING
Signed capability/eval lifecycle tool  IMPLEMENTED (examples/optical_capability_signer + signed demo fixtures);
                                       N0 后真实 Provider 付费 eval 证据 PENDING
Production active default              BLOCKED UNTIL REAL-PROVIDER GATE (demo fixtures are not provider evidence)
```
