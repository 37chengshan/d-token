# Optical / Hybrid Context Optimization 测试矩阵

更新日期：2026-07-11

## 1. Applicability 与安全默认值

```text
POST /v1/messages Anthropic can enter shadow analysis
GET /v1/messages ineligible
empty body ineligible
unsupported path ineligible
protocol/endpoint mismatch ineligible
unknown model shadow-only
default registry grants no active model
active requires global allow_active
active requires route mode and allow_tool_result
active requires confirmed_experimental_lossy
over-budget request is never active
```

## 2. Capability Snapshot Lifecycle

```text
legacy snapshots default to schema v1 + active lifecycle
duplicate snapshot ID is rejected
unsupported schema version is rejected
active snapshot cannot contain revocation metadata
revoked snapshot requires timestamp and reason
superseded snapshot requires replacement metadata
higher revoked revision blocks older active revision
higher superseded revision blocks older active revision
newer active revision may restore after revocation
provider/provider-id/model/endpoint identity mismatch is rejected
highest valid active revision wins
registry counts configured/verified/rejected/revoked/superseded records
```

已覆盖核心 duplicate/revocation/recovery 测试；superseded revision barrier 与 schema migration 仍建议增加独立 fixture。

## 3. Provider-specific ContextGraph

```text
Anthropic matching tool_use/tool_result creates dependency edge
Anthropic parallel tool uses may close out of order
orphan tool_result is not closed
recent closed result is not historical
duplicate tool-use ID makes result ambiguous
duplicate results for one tool_use fail closed
sequence edges preserve block order
open tool use remains tracked
OpenAI assistant tool_calls are lowered to ToolUse
OpenAI tool-role messages are lowered to ToolResult
OpenAI parallel tool calls use OpenAiChatV1 state machine
state-machine name and version are metadata-only
```

仍需新增：

```text
result before use fixture
missing one of multiple parallel results
malformed tool_call_id
OpenAI Responses lowering
mixed provider/protocol malformed input
very large parallel tool fan-out
```

## 4. Stable Fingerprints 与 Plan IR

```text
body mutation changes fingerprint
canonical object key order does not change fingerprint
array order changes fingerprint
raw byte encoding differences change raw-byte fingerprint
fingerprint kind/algorithm/version are explicit
Plan Debug excludes raw request content
Plan Debug excludes concrete snapshot ID
capability revision and graph summary remain internally available
```

仍需新增：

```text
HTTP ingress raw-byte wiring
number canonicalization edge cases
Unicode normalization policy
negative zero/exponent/large integer fixtures
cross-language canonical v1 fixture corpus
```

## 5. Raw Text、Risk、Geometry、Economics

```text
TextRef Debug excludes raw text
TextArena Debug excludes raw text
candidate text remains request-local
ConversationInventory Debug excludes raw text
embedded Authorization/Bearer is SecretNoDuplicate
embedded Cookie/API-key markers are SecretNoDuplicate
UUID/hash/path is ExactTextRequired
embedded diff is StructuralUnsafe
dense exact anchors skip block
ordinary log remains RenderSafe
geometry capacity derives from padding/cell/rows
capability limits constrain dimensions
token estimate applies formula and safety margin
selected dense log produces positive economics
secret/orphan/recent candidates contribute zero selected savings
```

## 6. Structured Validation

```text
shadow compares actual original/planned body
shadow rejects body change
shadow rejects forbidden public markers
all candidates require action and reason
current user remains text
recent tail remains text
active requires valid shadow plan
active requires global/route/confirmation gates
active requires Supported non-revoked capability
active enforces cache stability, patch membership, image budget and net savings
```

## 7. API 与 Event Ledger

```text
GET /compression/optical/status reflects configured policy
GET /compression/optical/capabilities defaults fail closed
GET /compression/optical/capabilities/lifecycle returns counts only
POST /compression/optical/preview returns metadata-only summary
unsupported path becomes Ineligible
preview uses configured route policy and snapshot registry
preview writes LayeredStage::OpticalPreview event
GET /compression/optical/events/recent filters OpticalPreview events
event includes status/mode/candidate/image/economics/graph/capability aggregates
event does not leak request marker/provider/route/model/body/source path/base64
```

## 8. 提交账本 schema v2

测试代码已改为 Python 标准库 `unittest`，覆盖：

```text
plan creates exact staged record
mark-plan committed requires hash and writes submitted_at
operation/pending logs remain separate
duplicate ID does not overwrite valid JSON
ready plan cannot retain blockers
unknown dependency is rejected
shared files require hunk-level or mixed staging
committed records require submitted=true and commit evidence
workspace branch is observational only
atomic JSON replacement preserves complete documents
```

当前执行状态：

```text
python3 -m unittest support.scripts.test_git_operation_audit
→ CodexPro safe allowlist blocked command

pytest support/scripts/test_git_operation_audit.py
→ pytest executable not installed, exit 127
```

因此该组测试是“测试代码已完成、执行环境阻塞”，不能标记为 passed。

## 9. 当前实际结果

```text
cargo test -p dtoken-compress
76 unit passed
4 integration passed
0 failed

cargo test -p dtoken-config
4 passed
0 failed

cargo test -p dtoken-core-api
35 unit passed
6 Optical API integration passed
0 failed

cargo test -p dtoken-types
3 passed
0 failed

cargo test --workspace
all tests passed
exit code 0

cargo clippy --workspace --all-targets -- -D warnings
passed
0 warnings
```

未执行：

```text
cargo fmt --all -- --check
CodexPro safe allowlist rejected command

Python standard-library ledger tests
CodexPro safe allowlist rejected python3 command
```

## 10. 下一阶段必须新增

```text
snapshot authority/signature fixtures
superseded lifecycle and schema migration fixtures
HTTP raw-byte evidence integration test
JCS/canonical-number fixture corpus
provider-specific malformed and adversarial state fixtures
cache hit/miss/invalidation economics ranges
CJK and wide glyph fidelity
renderer page-order and image-budget tests
post-patch Anthropic protocol validation
rollback after render/patch failure
Desktop Optical Lab API contract tests
```
