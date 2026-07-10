# Optical / Hybrid Context Optimization 多维度审查报告（第 3 轮）

审查日期：2026-07-10

审查对象：V7 架构迭代、Phase A shadow planner、Phase B visibility API。

审查维度：

```text
产品定位
架构边界
安全性
算法正确性
经济性
协议兼容性
可测试性
可观测性
扩展性
发布准备度
```

## 1. 总结结论

本轮发现并修复了上一版本中最关键的五类问题：

```text
真实文本未进入 risk pipeline
经济收益汇总包含未选中候选
validator 依赖调用方布尔声明
capability 默认硬编码 active 模型
geometry 与 policy gate 过于简化
```

修复后：

```text
Phase A shadow planner：达到可合并质量
Phase B metadata-only API：达到基础可用质量
Active renderer/patcher：仍不具备发布条件
```

## 2. 产品定位审查

### 发现

旧架构以 Optical 为中心，容易把“图片化”误认为主要优化手段。

### 优化

V7 将 Optical 降级为 Hybrid Context Optimization Engine 的一个 codec。

推荐策略顺序：

```text
Preserve/Cache
→ Native Context Management
→ Lossless Structural Compression
→ Semantic Compression
→ Optical Capsule
→ External Retrieval
```

### 结论

通过。产品定位不再诱导过早实现 renderer。

## 3. 架构与数据流审查

### 旧问题

`decision.rs` 对合成字符串进行风险分析：

```text
block-{id}-chars-{chars}
```

这无法识别真实 secret、diff、UUID、hash 和 path。

### 修复

```text
Inventory extracts candidate content
→ TextArena stores raw text request-locally
→ BlockRef stores TextRef
→ Decision resolves TextRef
→ Risk classifies actual content
```

缺失 TextRef：

```text
SkipStructuralRisk
reason = missing_request_local_text
```

### 结论

通过。真实数据链已经闭环，同时保持 raw text containment。

## 4. 安全性审查

### 已验证

```text
TextArena/TextRef 不可序列化
TextArena/TextRef Debug 不打印原文
Public summary 不含 raw body/base64
Secret-like 内容整块跳过
Secret skipped candidate 不计入 image/economics totals
Unknown model 默认不能 active
Protocol/endpoint mismatch fail closed
```

Risk detector 已从“检查开头”升级为“扫描整个 block”，覆盖：

```text
Authorization/Bearer
Cookie/Set-Cookie
x-api-key/api_key/api-key/apikey
sk-* credential-like token
UUID/hash/path
diff headers/hunks
```

### 仍需注意

当前 detector 是保守启发式，不应被视为完整 DLP/secret scanner。进入 active 前应增加专用 scanner、误报/漏报 fixtures 和版本化规则。

### 结论

Shadow 安全性通过；Active 安全性仍需后续组件。

## 5. Capability 审查

### 旧问题

生产 resolver 通过硬编码模型名授予 `Supported`。

### 修复

```text
Default OpticalCapabilityRegistry = empty
resolve_model_capability() = no active rule by default
Supported = exact verified registry match only
```

显式实验 rule 只能由调用方注册，不会自动生效。

### 剩余

```text
registry config persistence
capability revision
verified timestamp
source attribution
revocation
provider-specific fixture verification
```

### 结论

Fail-closed 设计通过；生产 capability lifecycle 未完成。

## 6. Applicability Gate 审查

当前已检查：

```text
method
path
body non-empty
protocol/endpoint match
global enabled
route enabled
request budget
allow_active
route mode
allow_tool_result
confirmed_experimental_lossy
capability status
```

Shadow 与 Active 分离：active 禁用不会阻止 metadata-only preview。

当前 budget 使用 request body bytes 作为保守上界，后续应替换为 inventory 的 exact candidate chars。

结论：Phase A/B 通过；Active 前需精确 candidate budget。

## 7. Geometry 审查

### 修复

Geometry profile 现在包含：

```text
width/height
padding
cell dimensions
header rows
cols/rows/capacity
pixels per token
safety margin
```

容量由字段推导；token estimate 使用向上取整；尺寸受 capability 上限裁剪；非法公式返回不可用。

### 剩余

```text
CJK/wide glyph capacity
font metrics
actual renderer parity
different provider token formulas
image encoding size budget
```

### 结论

Shadow estimator 通过；Renderer parity 未验证。

## 8. Economics 审查

### 旧问题

所有候选都可能进入预计收益，即使被 secret 或 structural gate 跳过。

### 修复

只有：

```text
CandidateAction::ShadowEstimateOnly
```

进入：

```text
estimated_images
text_tokens_before
image_tokens_after
sidecar_tokens
cache_burn_tokens
net_saved_tokens
```

Secret test 已证明 totals 为 0。

### 剩余

```text
scalar estimate → range estimate
bucket-specific chars/token
real cache rates
preprocessing latency
provider actual usage calibration
quality risk cost
```

### 结论

汇总正确性通过；经济模型仍是实验估算，不可直接驱动生产 active。

## 9. Validator 审查

### 旧问题

Validator 接收调用方提供的裸 bool，存在错误 evidence 被信任的风险。

### 修复

Structured Shadow Input 读取：

```text
original body
planned body
public summary
candidate action/reason
current user protection
recent tail protection
geometry sufficiency
```

Structured Active Input 读取：

```text
global/route policies
capability
cache stability
secret sidecar state
patch membership
image count
net savings
```

旧 bool API 暂时保留兼容，后续应标记 deprecated 并移除生产调用。

### 结论

结构化验证方向通过；完整 immutable Plan IR 和 post-patch validator 尚未完成。

## 10. Protocol/API 审查

已完成：

```text
POST /compression/optical/preview
GET /compression/optical/status
GET /compression/optical/capabilities
```

API 测试验证：

```text
default off
shadow-only
fail-closed
unsupported path ineligible
preview summary redacted
private provider/route/model/raw body not leaked
```

未完成：

```text
route policy persistence
capability snapshot API
preview event ledger
config mutation endpoints
active protocol patch
```

结论：Visibility API 通过；Control/Active API 未完成。

## 11. 测试证据

本轮实际运行：

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
all tests passed
0 failed
```

完整 workspace 仍因缺少：

```text
code/apps/desktop/src-tauri/binaries/dtoken-proxy-aarch64-apple-darwin
```

而在 Tauri build script 阶段停止。该问题与 Optical 算法无关，但阻止声明“完整 workspace 全绿”。

## 12. 评分

| 维度 | 评分 | 说明 |
|---|---:|---|
| 产品定位 | 9/10 | 已升级为 hybrid optimizer |
| 安全默认值 | 9/10 | 默认 registry 无 active rule |
| Raw containment | 9/10 | request-local + metadata-only |
| Shadow 算法闭环 | 8.5/10 | risk/economics/validator 已串联 |
| Capability lifecycle | 6.5/10 | 缺持久化、版本和来源 |
| Economics 精度 | 6/10 | 仍是启发式 scalar |
| Renderer readiness | 2/10 | 尚未实施 |
| Active readiness | 3/10 | 缺 patch/post validation/rollback |
| 测试覆盖 | 8.5/10 | 后端和 API 回归良好 |
| 文档一致性 | 8.5/10 | V7 和状态矩阵已更新 |

## 13. 发布判断

```text
Phase A shadow planner：可以合并
Phase B visibility endpoints：可以合并
Desktop Optical Lab：可以开始
Renderer：暂不开始 active rollout
Active request mutation：禁止发布
```

## 14. 下一轮最高优先级

```text
P0 Capability snapshot config/revisioning
P0 ContextGraph + closed tool dependency
P0 Immutable internal plan
P1 Preview metadata event ledger
P1 Economics estimate ranges
P1 Fidelity/adversarial fixtures
P2 Desktop Optical Lab
P2 Renderer feature flag
P3 Anthropic patcher + post validator + rollback
```
