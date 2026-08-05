# d-token Hybrid Context Optimization Engine 一轮完整迭代计划 V8

更新日期：2026-07-14

状态：当前后端迭代合同。产品边界仍以 `docs/PRD.md` 为准，执行清单同步到 `docs/PLAN.md`。

## 0. 目标

本轮不是单独调优 PNG renderer，而是完成 Hybrid Context Optimization Engine 的一次后端闭环迭代：

```text
route-resolved request
→ provider-specific inventory / state graph
→ content classification
→ secret / exact / structure / recency firewall
→ representation portfolio
→ cache and economics gate
→ immutable plan
→ safe text codec or signed Optical execution
→ post validation / pre-send fallback
→ provider usage reconciliation / Ledger / Doctor evidence
```

Optical 只是策略组合中的一个有损 codec。默认顺序固定为：

```text
PreserveText
→ LosslessStructuralCodec
→ SafeLogCodec
→ OpticalImage（仅通过全部门）
```

本轮不接前端，不默认启用 active，不注册仓库内置可信模型，不实现未经验证的 OpenAI active patcher。

## 1. 研究输入与证据边界

### 1.1 上游参照

- pxpipe 固定 commit：`7b7d6efa42926a73b1bd709b5f6d1af1ae679947`。
- 许可证：代码 MIT；Spleen 字体 BSD-2-Clause；现有 NOTICE 继续记录来源、字体和独立实现边界。
- 只复用公开思想、测试合同和已审计字体数据；生产实现仍由 d-token Rust runtime 所有。

上游值得保留的设计：

- 模型精确匹配，不让 sibling model 继承视觉资格。
- current/recent/open tool state 保留文本。
- closed history、reflow、IDS、factsheet、动态页高和 append-only cache chunk。
- 图片 token、文字反事实、缓存与完整账单分开核算。
- exact/hex 的无声编造被视为体系边界，不被平均分掩盖。

上游不能直接照搬的部分：

- pxpipe 默认允许的 Fable 路径不能授权 d-token 的任意 provider/route/model。
- 上游 history collapse、system/tool slab 和模型 allowlist 不能跳过 d-token 的 signed Capability、post-validator、Ledger 与 rollback/fallback 边界。
- 上游 Sol/Grok/Claude profile 不能跨模型继承。

### 1.2 本地真实评测

V1 极端密度压力测试：三模型约 `8.33%`，只证明 `1568×728 / 312列` 横向密集页不能直接套给 GPT‑5.6。

V2 自创多探针测试：最佳 profile 仍只有 `40.9%–50%`。主要问题是单图多题、重复填充和不真实语料，已降级为负向压力证据。

V3 pxpipe 上游等价轨：

- medium base：Luna `96/100`、Terra `90/100`、Sol `93/100` 算术；Gist 分别 `82/98`、`85/98`、`91/98`。
- medium state：`16/18`、`17/18`、`18/18`。
- never-stated 仍有 `1/16`、`1/16`、`2/16` 编造。
- 三模型 dense hex 均 `0/15`，且多数是无声错写。
- 18K coding history 的已有 12 个正向任务语义样本量实际只有四个模板；Luna/Sol ceiling，Terra 出现结构化 false-abstain。它能证明特定路径问题，不能证明通用模型排序。
- 多页高干扰 expanded Gist 明显下降；视觉 payload 中位预计节省约 `42%–44%`，但质量门未通过。
- max 在当前中转有严重长尾和 outcome-unknown，必须把 transport 与模型质量分离。

因此当前证据只允许以下结论：

1. 规则化短语义内容可被部分模型可靠读取。
2. exact/hash/path/secret 不能只存在图片中。
3. 多页高干扰、最终决策追踪和 never-stated 是主要质量风险。
4. reasoning effort 不具有稳定单调性，不能用 `max` 自动替代内容门。
5. Capability registry 必须继续为空；任何模型都未获得生产资格。

## 2. 本轮成功标准

### 2.1 功能 DoD

- 一个 route-resolved request 只生成一个统一 Hybrid Plan，不允许 semantic 与 Optical 隐式双重改写。
- 每个候选明确记录 content kind、risk tier、representation action、reason code、cache stability 和 economics evidence。
- JSON 只走语义等价的结构压缩；代码、diff、schema、tool id/arguments、exact、secret 和决策密集内容不进入图片。
- SafeLogCodec 不执行通用长文本摘要，不删除唯一中间决策。
- Optical engine 只消费 `OpticalImage` target；SafeText target 即使在 active route 上也不得被 renderer/patcher 接收。
- post-validator 重新证明 target action、node/path、precondition digest、图片 digest、协议结构和 approved path 外不变。
- OpenAI Responses 获得独立 inventory/state graph 和 shadow evidence，但 active patcher 仍不可达。
- renderer 支持与 estimator 一致的实际页尺寸；最后一页不为无内容像素付费，同时 profile digest 能使旧签名自动失效。
- accounting 区分 local estimate、provider actual sent、counterfactual、cache usage 和 paired evidence；不把估算写成 provider savings。

### 2.2 安全 DoD

- Secret 扫描发生在 factsheet、codec、renderer 和持久化之前。
- current user、recent tail、最近六个闭合调用对、open/orphan/duplicate/malformed tool state 永远保留原生文本/结构。
- exact 分类覆盖 hash、UUID、路径、URL、版本、数字、flag、LABEL=value、camelCase/PascalCase、ticket/advisory code。
- 任一 exact 密度过高、未知内容、未知模型、缺签名、缺 eval、缓存不稳定、经济性不足、资源超限或校验失败均 fail closed。
- post-send 不自动重放原文；pre-send 才能回退 untouched original bytes。
- 普通日志、事件、Doctor、导出不出现 prompt、response、源码、TextRef、PNG/base64 或凭据。

### 2.3 验证 DoD

- `cargo test -p dtoken-compress`、`dtoken-types`、`dtoken-config`、`dtoken-core-api` 通过。
- `cargo clippy --workspace --all-targets -- -D warnings` 通过。
- 本轮触及的 Rust 文件格式通过；若全仓 fmt 被用户无关改动阻塞，必须给出具体文件证据。
- `python3 support/scripts/check_structure.py` 通过。
- 新增 fixture 覆盖正常、原失败、边界、非法输入、orphan/duplicate、资源错误和 fallback。
- V3 图片评测继续由独立 Harness 运行；结果只影响 Capability gate，不自动写生产配置。

## 3. 目标架构

### 3.1 Unified Hybrid Plan

新增/统一内部表示：

```text
HybridPlan
  request_fingerprint
  resolved provider / route / model / endpoint digest
  inventory + state graph summary
  protected nodes
  representation targets[]
  cache evidence
  economics range
  capability snapshot binding
  validation contract
```

每个 target：

```text
node_id
source_path
precondition_digest
content_kind
risk_tier
action = PreserveText | LosslessJson | SafeLogFold | OpticalImage
reason_code
estimated before/after
profile/capability binding when Optical
```

公共 summary 只暴露枚举、计数和区间，不暴露原文与 digest-adjacent 信息。

### 3.2 Content Router

第一轮只启用能确定验证的 codec：

| Content kind | 默认动作 | 验证 |
|---|---|---|
| JSON object/array | LosslessJson | parse before/after deep equality |
| 重复结构化日志 | SafeLogFold | 保留顺序与所有唯一行，只折叠相邻重复 |
| search results | PreserveText | 等专用 codec |
| code / diff / patch | PreserveText | 不做通用摘要 |
| prose / unknown | PreserveText | 等质量 gate |

`Aggressive/Max` 不能让 SafeLogCodec 继承通用 `collapse_long_text`。

### 3.3 Risk Firewall

优先级：

```text
SecretNoDuplicate
→ StructuralUnsafe
→ BlockUnsafe
→ ExactTextRequired
→ DecisionSensitive
→ RenderSafe
```

在安全 sidecar 尚未与 active patcher、post-validator 和预算形成一体化合同前，`ExactTextRequired` 整块保留文本；不以“factsheet 存在”降低保护级别。

### 3.4 Provider State

- Anthropic Messages：保留现有 parallel/out-of-order/duplicate/orphan fail-closed。
- OpenAI Chat：保留独立 `tool_calls` / tool result 状态机，仍仅 shadow/semantic。
- OpenAI Responses：新增 `message`、`function_call`、`function_call_output` inventory；相邻完成对可进入旧历史分析，最近六对和所有异常状态保留原生；本轮不写 active patcher。

### 3.5 Cache Zones

本轮先实现 plan evidence，不直接启用 history collapse：

```text
stable prefix
frozen closed-history chunks
volatile tail
current/open state
```

稳定 digest 必须使用 SHA-256，不使用进程随机化的 `DefaultHasher`。任何 volatile relocation/drop 仍只生成 shadow 建议；没有 protocol patch 和因果缓存证据前不得 active。

### 3.6 Renderer 与 Profile

- Geometry、renderer、estimator、UI/summary 使用同一 profile。
- profile 精确绑定 provider/route/model/endpoint、字体、布局、分页和 eval digest。
- profile ladder 只用于 eval：readable、balanced、dense；模型不得继承 sibling 的 profile。
- 生产 active 只消费签名 snapshot 中绑定的 profile。
- renderer 页面按实际内容高度输出；完整页保持 max height，最后一页按 header+内容行+padding 对齐收缩。
- 每页实际 width/height 参与 token 估算和 artifact evidence。

### 3.7 Economics 与 Accounting

从单一标量升级为：

```text
conservative / expected / optimistic
```

至少计入：文本 token、实际每页视觉 token、sidecar、cache write/read、prefix churn、输出风险和渲染延迟。Active 只使用 conservative 下界。

provider actual 只来自响应 usage 或中转原始金额对账；离线文字 token 和 family 公式保持 counterfactual/estimate 标签。

## 4. 实施阶段

### Phase 1 — Strategy Isolation

- 拆分 `SafeTextCodec` 与 `OpticalImage` target。
- JSON/decision log 不得进入 renderer。
- post-validator 再验证 target action。
- SafeLogCodec 固定使用保守折叠。

### Phase 2 — Risk and Exactness

- 扩充 exact/secret/decision detection。
- 为每类风险增加 false-positive/false-negative fixture。
- 密集 exact、minified blob、Unicode/CJK escape、code/diff 全部 fail closed。

### Phase 3 — Unified Plan and Cache Evidence

- 将 content/risk/action/cache/economics 绑定到 immutable plan target。
- volatility stable digest 改为 SHA-256。
- 引入 stable/frozen/volatile/current zone summary；本轮只 shadow。

### Phase 4 — OpenAI Responses Shadow State Machine

- 独立 inventory、graph enum、closed/recent6/open/orphan/duplicate fixtures。
- route/runtime public summary 展示 Responses shadow 分析能力。
- active execution 明确返回 unsupported，不静默桥接。

### Phase 5 — Renderer and Economics Accuracy

- 动态最后页高度。
- per-page token estimate 与 actual artifact dimensions 对齐。
- profile digest revision 升级，使旧 attestation fail closed。
- economics 输出三点区间，active 使用 conservative。

### Phase 6 — Runtime, Accounting, Observability

- direct Anthropic 热路径消费统一 plan；semantic/Optical 不重复改写。
- Ledger 记录 action 计数、estimate basis、actual usage presence、fallback 和 post-send state。
- Doctor 检查 stale profile/eval/signature、unsupported endpoint、cache instability 和 missing usage。
- 保持 metadata-only。

### Phase 7 — Verification and Documentation

- crate、workspace、structure checks。
- 同步 `docs/PLAN.md`、implementation status、Optical README、release gates 与 test matrix。
- V3/V4 eval artifact 只能生成 `LAB_CANDIDATE`；生产仍需同候选版 routed evidence 与签名。

## 5. 本轮不做

- 不接前端。
- 不写入或修改用户 Codex/OpenCode/Claude 配置。
- 不启动生产 d-token Optical active。
- 不自动注册 Luna/Terra/Sol/Fable/Opus 等任何模型。
- 不发纯文字真实对照请求。
- 不实现 OpenAI Responses active patcher。
- 不实现生产 history collapse 或自动删除旧对话。
- 不以模型排序代替安全门。

## 6. 结论规则

本轮代码完成只表示“引擎后端闭环增强”，不表示模型或生产路径可用。

只有同时具备以下证据才允许单条 route 的实验 active：

1. 精确 provider/route/model/endpoint/profile 的通过 eval artifact。
2. 可信 authority 的有效签名与未撤销 revision。
3. 同一候选 commit 的真实 routed fidelity/cost/latency evidence。
4. 安全门零失败、exact 保留文本、post-validator 通过。
5. 用户显式 route 授权和可回退配置事务。

否则 Capability registry 保持为空，运行时保持 semantic/shadow/passthrough。

## 7. 2026-07-14 实施结果

Phase 1–7 已按本合同落地并完成本轮定向验证：

- `SafeTextCodec` 与 `OpticalImage` target 已隔离，active renderer 只消费后者且要求 cache-stable。
- exact/secret/decision/structure 分类已扩充；稳定分区 digest 使用 SHA-256。
- immutable plan evidence 已包含 content kind、risk tier、action、reason、cache-stable 与三点 economics，不公开原文或具体 digest。
- OpenAI Responses 已实现 `OpenAiResponsesV1` shadow inventory/state graph、recent-six/open/orphan/duplicate 保护；active target 始终为空。
- `anthropic_dense_v2` 仅收缩最后一页，完整页维持最大高度；active token/像素核算读取每个 artifact 的实际尺寸。
- Event Ledger 增加 representation/cache/exact/decision 聚合计数；`/compression/optical/status` schema v2 报告统一 plan、Responses shadow 和动态页核算 readiness，默认 active 仍关闭。
- 实际验证：`dtoken-compress` 124 项测试、`dtoken-core-api` 58 项测试、`dtoken-types` 15 项测试、`dtoken-config` 4 项测试全部通过；`dtoken-compress` 与 `dtoken-core-api` 严格 Clippy、workspace fmt 和结构检查通过。

模型评测独立结论：Max Prompted 当前 `130/143` 合同有效，剩余 13 个为 Luna outcome-unknown 与 Terra HTTP 200 但缺 output/usage 的 transport failure；不得把这些缺失样本计作模型错误，也不得因此写入 Capability registry。
