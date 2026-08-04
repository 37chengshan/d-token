# Optical / Hybrid Context Optimization 多维度审查报告（第 5 轮）

审查日期：2026-07-11

审查范围：

```text
N6  Pending commit ledger schema v2
N7  Capability lifecycle/revocation/supersede
N8  Stable raw-byte/canonical fingerprints
N9  Anthropic/OpenAI provider state machines
N10 Expanded metadata-only lifecycle/events API
N11 Top-level documentation and commit records
```

## 1. 总体结论

本轮把 Optical 从“版本化 shadow planner”推进到“具备能力撤销、稳定证据、Provider 状态机和可机器审计提交记录的 shadow control plane”。

当前判断：

```text
Rust code quality                         PASS
Full Rust workspace regression            PASS
Clippy -D warnings                        PASS
Capability revocation fail-closed         PASS
Canonical fingerprint determinism         PASS
Provider-specific graph fixtures          PASS
API/event privacy                         PASS
Pending ledger implementation             COMPLETE
Pending ledger execution verification     BLOCKED BY TOOL ENVIRONMENT
Production active transform               BLOCKED BY DESIGN
```

本轮没有执行 Git commit，因此所有阶段必须保持：

```text
submitted = false
submitted_at = null
commit_hash = null
```

## 2. 产品边界审查

Optical 继续保持正确定位：

> Optical 是 Hybrid Context Optimization Engine 中的一种有损 codec，而不是通用无损上下文压缩器。

本轮没有实现 renderer、request patch、active provider routing、history collapse 或持久化 raw recovery，符合安全实施顺序。

结论：通过。

## 3. Capability Snapshot 生命周期审查

### 3.1 已实现语义

Snapshot 增加：

```text
schema_version
Active / Revoked / Superseded lifecycle
revoked_at_ms
revocation_reason
supersedes_snapshot_id
```

Registry 当前要求：

```text
schema v1
enabled
unique snapshot id
revision > 0
verified timestamp
non-empty source
exact identity match
Supported capability
verified image/token policy
```

### 3.2 撤销屏障

本轮最重要的安全改进是 revision barrier：

```text
active revision 1
revoked revision 2
```

此时 revision 1 不会重新回退生效。只有：

```text
active revision 3
```

才可恢复能力。

这避免了“最新撤销记录被过滤后，旧 active 规则反而重新生效”的严重 fail-open 风险。

### 3.3 剩余设计问题

1. `source` 仍是自由字符串，没有 authority、签名或信任链。
2. Registry identity key 当前由 enum Debug 表示拼接，仅适合当前内部版本，不适合作为持久化跨版本合同；后续应改为显式稳定 ID。
3. `register_snapshot()` 对 revoked/superseded 记录返回错误；真正的 tombstone 语义目前由批量 `from_snapshots()` 建立。后续应拆分为：

```text
ingest_lifecycle_record
register_active_capability
```

4. `supersedes_snapshot_id` 在 Superseded 记录上的语义可能产生歧义。应明确是：

```text
superseded_by_snapshot_id
```

还是新 active 记录声明：

```text
supersedes_snapshot_id
```

5. 尚无撤销写 API、持久化审计流和配置签名。

结论：Shadow/control-plane 生命周期可提交；生产 active trust lifecycle 未完成。

## 4. 稳定请求指纹审查

### 4.1 已实现

```text
RawRequestBytes + SHA-256 + version 1
CanonicalJson + SHA-256 + version 1
object keys sorted
array order preserved
explicit kind/algorithm/version
metadata-only Debug
```

### 4.2 正确性

测试证明：

```text
相同对象不同键顺序 → canonical digest 相同
数组顺序不同 → digest 不同
相同 JSON 语义但原始空白不同 → raw-byte digest 不同
body mutation → digest 不同
```

### 4.3 剩余风险

1. Core API 使用 Axum `Json` extractor，Preview 目前仍走 CanonicalJson fallback；RawRequestBytes 入口尚未连接 HTTP ingress。
2. Canonical JSON v1 是项目内部规则，不是 RFC 8785/JCS 实现。
3. 数字表示仍依赖 `serde_json::Number::to_string()`，需要补：

```text
negative zero
exponent form
large integer
fraction normalization
```

4. Unicode normalization 当前保持 JSON 字符串语义，没有 NFC/NFD 归一化合同。
5. Plan Debug 不输出 digest 和具体 snapshot ID，隐私边界较保守，但排障时只能依赖 metadata。

结论：内部 shadow evidence 通过；跨语言、跨实现、active patch evidence 尚未完成。

## 5. Provider-specific Tool State 审查

### 5.1 AnthropicMessagesV1

覆盖：

```text
parallel tool_use
out-of-order matching tool_result
orphan result
duplicate tool ID
duplicate result
recent tail
closed historical boundary
```

### 5.2 OpenAiChatV1

新增独立 lowering：

```text
assistant.tool_calls → ToolUse
tool role + tool_call_id → ToolResult
parallel call/result matching
provider state-machine metadata
```

### 5.3 重要边界

OpenAI 当前没有接入 Optical decision builder、TextArena risk pipeline 或 active capability。它只是独立 inventory/graph fixture，属于正确的 fail-closed 增量实现。

### 5.4 剩余风险

需要继续覆盖：

```text
result before use
missing one parallel result
malformed tool_call_id
duplicate OpenAI tool_call IDs
OpenAI Responses events
mixed malformed provider payload
very large parallel fan-out
```

当前图只表达 Sequence 和 ToolUseToResult，还没有 cache/schema/summary dependencies。

结论：Provider-specific foundation 通过；不得宣称 OpenAI Optical transform 可用。

## 6. API 与事件隐私审查

新增只读端点：

```text
GET /compression/optical/capabilities/lifecycle
```

只返回：

```text
configured/active/revoked/superseded counts
verified/rejected counts
schema version
```

不返回 provider/model/snapshot ID。

Preview Event 已扩展：

```text
status/mode
candidate/preserved/skipped
estimated images
public economics aggregate
graph counts
closed/orphan/open tool counts
configured/verified/rejected rule counts
snapshot revision presence
```

仍禁止：

```text
raw request
candidate text
provider/route/model identity
source path
candidate reason
TextRef
image/base64
```

集成测试使用私有 marker、provider、route、model 值验证未泄漏。

结论：通过。

## 7. 提交记录系统审查

### 7.1 schema v2 目标

`support/pending-commits.json` 不再设置目标分支。每个记录写清：

```text
阶段
时间
记录时所在 branch
精确文件
shared files / staging mode
验证状态和命令
阻塞原因
是否已提交
submitted_at
commit_hash
```

Branch 只是观察信息，不影响计划有效性。

### 7.2 审计分离

```text
support/pending-commits.json
```

记录计划和提交状态。

```text
support/git-operations.json
```

只记录真实发生的 Git 操作。

二者不可互相替代。

### 7.3 状态一致性规则

```text
ready + blockers                 invalid
committed without hash/time      invalid
uncommitted with hash/time       invalid
shared file + whole-file staging invalid
unknown dependency               invalid
duplicate ID                     invalid
```

### 7.4 验证阻塞

测试已改为标准库 `unittest`，但：

```text
python3 -m unittest ...
```

被 CodexPro safe allowlist 阻止；本机又没有 pytest。

因此 N6 正确标记为 blocked，不得虚报 passed。

结论：规则与实现合理；执行验证环境未满足。

## 8. 性能与复杂度审查

1. ContextGraph 当前构建复杂度近似 O(n)，HashMap/HashSet 可接受。
2. Canonical JSON 会重新构造完整规范化字符串，时间和额外内存为 O(body size)。Preview 有 `max_candidate_chars_per_request`，但后续应增加独立 fingerprint byte budget。
3. Snapshot Registry 在 resolve 时线性扫描 active rules。当前规则数很少可接受；生产配置增长后可按 identity 建立索引。
4. Event Ledger 只写聚合 metadata，事件大小受控。
5. OpenAI inventory 当前复制部分 tool result 内容长度但不保存 TextArena raw；后续接入真实 risk 时需避免重复字符串复制。

结论：当前 shadow 规模可接受；active/control-plane 扩大前需做基准测试。

## 9. 测试与工具链证据

```text
cargo test -p dtoken-compress
76 unit + 4 integration passed

cargo test -p dtoken-config
4 passed

cargo test -p dtoken-core-api
35 unit + 6 Optical API integration passed

cargo test -p dtoken-types
3 passed

cargo test --workspace
all passed, exit 0

cargo clippy --workspace --all-targets -- -D warnings
passed, 0 warnings
```

未完成：

```text
cargo fmt --all -- --check
→ safe allowlist blocked

python3 -m unittest support.scripts.test_git_operation_audit
→ safe allowlist blocked

pytest support/scripts/test_git_operation_audit.py
→ executable missing, exit 127
```

桌面测试仍输出：

```text
kill: 999999: No such process
```

但 workspace 总退出码为 0。

## 10. 代码质量评分

| 维度 | 评分 | 说明 |
|---|---:|---|
| 产品边界 | 9.4/10 | 保持 shadow/control-plane，不越界 active |
| 安全默认值 | 9.5/10 | empty registry、policy off、revocation barrier |
| Capability lifecycle | 8.5/10 | 撤销语义完成，缺 trust/signature/API |
| Fingerprint evidence | 8.5/10 | 双证据完成，raw ingress/JCS 未完成 |
| Provider dependency model | 8.7/10 | Anthropic/OpenAI 分离，fixture 仍需扩展 |
| Raw-content containment | 9.4/10 | API/event/debug 均 metadata-only |
| API observability | 8.5/10 | 聚合信息完整，无身份泄漏 |
| 性能可控性 | 7.8/10 | 当前可接受，缺 benchmark/index |
| Rust test coverage | 9.2/10 | workspace + Clippy 全绿 |
| Commit auditability | 8.3/10 | schema v2 完成，Python 执行环境阻塞 |
| Production active readiness | 3.8/10 | renderer/patch/post-validator/rollback 未实现 |

## 11. 可提交与不可发布边界

可以按 `support/pending-commits.json` 阶段精确提交：

```text
N7 Capability lifecycle
N8 Stable fingerprints
N9 Provider state machines
N10 Lifecycle/events APIs
N11 Docs/review
```

N6 代码已完成，但建议在能运行标准库 unittest 的环境验证后再标 ready。

仍禁止发布：

```text
PNG renderer
request mutation
active provider routing
history transform
post-execution validator
rollback executor
unsigned/untrusted capability activation
```

## 12. 下一轮优先级

```text
P0 Run commit-ledger unittest in permitted Python environment
P0 Snapshot authority/signature/trust source
P0 Separate lifecycle ingest API from active-rule registration
P0 Wire raw request bytes at HTTP ingress
P1 Canonical number/Unicode/cross-language fixture corpus
P1 Provider-specific malformed/adversarial fixtures
P1 Economics P10/Expected/P90 ranges
P2 Desktop Optical Lab
P2 Fidelity corpus and renderer feature flag
P3 Anthropic patcher + post validator + rollback
```

## 13. 最终审查结语

本轮的核心价值不是增加图片能力，而是完善了未来任何有损变换都需要的控制面证据：

```text
能力可以被撤销
旧能力不会错误回退
请求证据可确定比较
不同 Provider 使用不同状态机
可观测事件不含原文
阶段改动和提交状态可机器审计
```

在 trust source、raw ingress evidence、renderer、patcher、post-validator 和 rollback 完成前，Production Active Transform 必须继续保持关闭。
