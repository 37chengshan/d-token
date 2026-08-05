# 核心算法与机制研究

## 1. 两类应用的算法重心不同

AiMaMi 的算法主要服务于“管理与调度”：账号/额度归一化、Provider 选择、代理状态、会话索引、配置合并和进程控制。

d-token 的算法主要服务于“请求执行与安全”：显式路由、协议适配、上下文压缩、Optical/Hybrid 规划、风险分类、经济门、原子事务和证据归因。

因此，不能只比较谁的算法数量多，而应比较算法是否与产品核心问题匹配。

## 2. AiMaMi 类应用的重要算法

### 2.1 账号与配额归一化

不同账号或 Provider 可能返回不同格式的限制：

- 剩余请求次数。
- token 额度。
- 5 小时窗口、日窗口、周窗口、月窗口。
- 重置时间。
- 不同模型成本倍率。

应先转换为统一模型：

```text
QuotaWindow {
  kind,
  used,
  limit,
  remaining,
  reset_at,
  source,
  observed_at,
  confidence
}
```

注意：

- `remaining = limit - used` 可能为负，不能盲目 clamp 后隐藏超限。
- 重置时间必须携带时区。
- 本地推算与远端实际返回必须区分。
- 多窗口不能简单相加。
- 额度状态过期后应显示 stale，而不是继续显示绿色。

### 2.2 账号/Provider 选择

常见策略：

- 固定选择。
- 优先级。
- 轮询。
- 按剩余额度。
- 按错误率和延迟。

安全实现应返回可解释 decision：

```text
candidate set
→ eligibility filter
→ policy score
→ tie detection
→ explicit decision or ambiguity
```

若两个候选得分相同或关键信息缺失，应要求用户选择，而不是静默猜测。

### 2.3 会话索引

本地会话多时需要：

- 增量扫描。
- 文件 mtime/size 快速判断。
- 内容 hash 去重。
- 标题/时间/项目路径索引。
- 异常文件隔离。

不要每次启动全文解析所有会话。可以使用 SQLite 索引，但原始会话文件仍由上游工具拥有。

### 2.4 配置合并

用户配置、应用托管块和上游默认值应区分：

```text
upstream defaults
+ user-owned fields
+ app-managed scoped block
= effective config
```

应用只能修改自己的 scoped block 或明确字段，不能重新序列化并覆盖所有未知字段。

## 3. d-token 路由算法

### 3.1 输入

- ingress protocol。
- endpoint。
- requested model。
- route id / instance id。
- Provider protocol。
- credential reference 状态。
- route policy。

### 3.2 决策原则

1. 先过滤协议不兼容候选。
2. 明确 route/instance 优先。
3. 检查 requested model 是否存在。
4. 检查 Provider 与 decision 一致。
5. 检查上游不能指向同一 d-token listener。
6. 候选不唯一时返回 ambiguity。
7. 不允许静默 fallback 到官方 Provider。

### 3.3 输出

```text
RouteDecision {
  route_id,
  instance_id,
  provider_id,
  input_protocol,
  output_protocol,
  requested_model,
  adapted_model,
  reason_code,
  evidence
}
```

决策必须写入脱敏 Ledger，才能证明请求实际走向。

## 4. 协议适配算法

支持 OpenAI Chat、Responses 和 Anthropic Messages 时，需要区分：

- system/developer 指令。
- 普通 text content。
- tool definitions。
- tool call / tool use。
- tool result。
- streaming event。
- generation controls。

原则：

- 未知字段 fail safe 保留或拒绝，不能悄悄丢弃。
- tool id、arguments、result 关联必须精确。
- 跨协议转换记录 adapted model 与 output protocol。
- 流式事件必须保持顺序、终止语义和错误状态。
- 无文本但有 tool call 的成功响应不能误判为失败。

## 5. Hybrid Context Optimization

### 5.1 目标

在不破坏当前用户指令、工具状态和精确文本要求的前提下，减少历史上下文成本。

### 5.2 处理链

```text
resolve route and model capability
→ lower request to protected Context IR
→ classify nodes
→ build dependency graph
→ find eligible historical closed tool results
→ risk classification
→ estimate economics
→ shadow plan
→ active gate
→ deterministic render
→ typed patch
→ post-validation
→ durable pre-send evidence
→ upstream send
```

### 5.3 Protected Context IR

节点至少分为：

- current user instruction。
- system/developer instruction。
- tool schema。
- open tool state。
- recent tail。
- historical closed tool result。
- exact-text-required block。
- secret/credential block。
- unknown opaque block。

只有满足规则的历史 closed tool result 可以成为候选。

### 5.4 Dependency Graph

Anthropic 与 OpenAI 工具调用状态机不同。算法需处理：

- 并行 tool calls。
- 结果乱序返回。
- duplicate tool ids。
- duplicate results。
- orphan result。
- open/unclosed tool state。
- recent result 保护。

任何 ambiguity 都应 fail closed。

### 5.5 风险分类

高风险内容：

- secret 标记。
- UUID、hash、路径、精确数值密集内容。
- diff/patch。
- schema/代码片段。
- 用户要求逐字保留的文本。

这些内容应保留原文或完全跳过，不做有损渲染。

### 5.6 经济门

需要比较：

```text
estimated text tokens removed
vs
image tokens + sidecar text + base64/request overhead + cache effects
```

不应只看字符数。结果应支持 signed savings，允许负值，避免把亏损 clamp 成零后看起来“没有损失”。

### 5.7 Active Gate

active 需要同时满足：

- global enabled。
- route enabled。
- explicit allow_active。
- experimental confirmation。
- 同协议安全表面。
- 精确 model capability。
- route-bound signature/attestation。
- eval corpus 通过。
- renderer/font/profile digest 匹配。
- 经济门通过。
- 资源预算通过。
- patch/post-validation 可用。
- pre-send fallback 保留原文。

缺任一项返回 shadow 或 passthrough。

## 6. 确定性渲染

渲染算法必须：

- 同输入、同配置产生 byte-identical PNG。
- 固定字体、布局、分页、DPI 和编码。
- 限制像素、内存、页数、并发和超时。
- 支持取消。
- 生成 artifact digest。
- 能由 post-validator 自解码或验证结构。

确定性是审计和 eval 的基础，不能依赖系统字体漂移。

## 7. Typed Patcher 与 Post-Validator

Patcher 只能修改 sealed path：

- 不允许动态 JSONPath 越界。
- 不允许改 current instruction、tools、model 或 generation controls。
- 记录每个 patch operation。

Post-validator 重新比较：

- 保护字段是否字节/语义一致。
- mutation 是否只发生在批准路径。
- artifact 是否匹配 digest。
- 图片数量和资源是否符合能力。
- secret 是否进入 sidecar/metadata。

验证失败在发送前回退原请求。

## 8. 配置事务算法

### 8.1 Snapshot

```text
exists
sha256
byte_len
permissions
```

### 8.2 Plan

- 严格解析原配置。
- 最小字段修改。
- 验证 next content。
- 生成脱敏 unified diff。
- 生成 before/after hash。

### 8.3 Review Binding

```text
revision = hash(agent + target + request + before_hash + after_hash)
```

apply 必须携带 revision、before 和 after hash。

### 8.4 Atomic Apply

- 创建同目录临时文件。
- 写入并 fsync。
- 保留权限。
- 原子 rename/MoveFileEx。
- sync parent directory。

### 8.5 Restore

只在当前文件仍等于 apply 后 hash 时恢复，防止覆盖用户之后的外部修改。

## 9. Event Ledger 与归因算法

事件应包含：

- event id。
- timestamp。
- stage。
- route/provider/model 的安全标识。
- request state。
- transform disposition。
- sent payload variant。
- usage evidence kind。
- error classification。

节省归因规则：

- shadow 不计 realized savings。
- pre-send fallback 不计 realized savings。
- upstream 未成功处理不能声明实际节省。
- 多 transform 不重复计数。
- provider actual 与 counterfactual 证据必须明确。

## 10. Doctor 规则引擎

Doctor 不应是硬编码字符串列表。每项检查包含：

```text
id
category
severity
state
source
observed_at
evidence
remediation
safe_auto_fix
```

检查按依赖执行：

```text
filesystem
→ config
→ credential
→ binary
→ process identity
→ port
→ health
→ route
→ upstream
→ routed Agent test
```

上游检查失败时，后续依赖项应标记 blocked，而不是都显示失败。

## 11. 算法测试

- property test：hash、canonical JSON、transaction ownership。
- adversarial fixture：duplicate tool ids、orphan results、secret、diff、malformed JSON。
- golden test：协议转换、deterministic PNG、diff。
- concurrency test：多 writer、多 refresh、external modification。
- fuzz：配置解析、协议输入、Ledger 尾部损坏。
- benchmark：plan latency、render memory、large history。
- real model eval：fidelity、cost、latency、cache effect。

## 12. 算法层结论

AiMaMi 的调度与管理算法适合提升 Codex 日常体验；d-token 的路由、事务和上下文优化算法构成更深的技术壁垒。d-token 应借鉴 AiMaMi 的管理任务表达，但不能为了快速做成“账号/配置工具”而削弱已有的安全算法。