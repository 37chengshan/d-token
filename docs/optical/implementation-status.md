# Optical / Hybrid Context Optimization 实现状态

更新时间：2026-07-12
事实来源：`code/crates/dtoken-compress/src/optical/`、`code/crates/dtoken-core-api/`、`code/crates/dtoken-types/`

## 1. 当前定位

Optical/Hybrid 后端执行链已经实现，但仍默认关闭。运行时可在同协议 Anthropic `/v1/messages` 路由上执行 passthrough、semantic、shadow、签名门控的 active Optical 和 pre-send fallback；没有可信签名 Capability、通过的绑定 eval、显式全局/路由授权或任一资源/校验门时均 fail closed。

```text
已实现：protected Context IR / semantic codec / deterministic PNG / sealed patch / post-transform validation
已接入：direct Anthropic JSON+SSE / raw-byte shadow+passthrough / accounting / Ledger / metrics / Doctor
默认行为：active off；未知模型、跨协议、无签名证据、资源超限或校验失败均保留原请求
仍禁止：未经真实模型评测的默认 active、history collapse、对外节省承诺
```

## 2. 已实现的规划基础

`dtoken-compress/src/optical/` 当前包含：

- applicability 与 protocol/endpoint/policy gate。
- request-local `TextArena`、文本引用和风险分类。
- semantic priority、volatility、secret/exact/diff 保护规则。
- Capability registry、版本化 snapshot 与 active/revoked/superseded 生命周期。
- Anthropic 与 OpenAI Chat 工具调用 inventory、ContextGraph 和状态机 lowering。
- immutable Plan IR、候选证据、render geometry 和 economics 估算。
- raw request bytes 与 canonical JSON 的 SHA-256 fingerprint 结构。
- route-resolved Protected Context IR；只批准已闭合、历史、低风险的 tool result。
- 纯 Rust 确定性 bitmap-font PNG renderer、分页、自解码与资源/取消/并发限制。
- sealed typed patch set 和 artifact-derived post-transform validator；approved path 之外必须完全不变。
- Ed25519 Capability attestation，绑定 provider、route、model、endpoint、renderer、font、eval 与有效期。
- multimodal signed accounting、send-state/fallback 状态机、typed Optical Ledger、metrics 与 Doctor/status evidence。

默认 Capability registry 为空并 fail closed。撤销或 supersede 的高 revision 不会静默回退到旧 active revision。

## 3. Provider 状态机

当前已覆盖：

- Anthropic `tool_use` / `tool_result` 的顺序、并行、闭合和 recent-tail 保护。
- orphan result、重复 tool-use id、重复 result 和未闭合 tool use 的 fail-closed 处理。
- OpenAI Chat `tool_calls` / `tool` role 的 inventory 与 graph lowering。

当前未完成：

- OpenAI Responses 的完整 provider-specific 状态机。
- 更大的 malformed、parallel、adversarial corpus。
- 跨 Provider 统一 execution state。
- summary、schema、cache zone 等更完整依赖边。

OpenAI lowering 目前只用于 shadow analysis，不授予图片或 active transform 能力。

## 4. Fingerprint 与 Plan 证据

当前结构支持：

- Raw request bytes SHA-256 v1。
- Canonical JSON SHA-256 v1。
- 稳定 object key 排序、保留 array 顺序。
- Capability snapshot id/revision 和 Provider graph 摘要。
- metadata-only public/debug summary。

生产 `/v1/messages` handler 直接接收 ingress bytes。passthrough、shadow 和 pre-send fallback 使用原始 bytes 发送，只有通过完整 post-transform validation 的 active 请求才重新编码；Preview API 仍以 canonical JSON 证据为主，不应把两者混为一谈。

Canonical JSON v1 是项目内部确定性编码，不宣称兼容 RFC 8785/JCS。

## 5. 当前 API

```text
POST /compression/optical/preview
GET  /compression/optical/status
GET  /compression/optical/capabilities
GET  /compression/optical/capabilities/lifecycle
GET  /compression/optical/events/recent
```

Preview 与 recent events 可以公开：

- final status、mode。
- candidate/preserved/skipped 数量。
- 估计图片数和公开 economics 摘要。
- graph node/edge/tool dependency 数量。
- closed/orphan/open tool 数量。
- route-policy 与 capability 规则计数。
- snapshot revision 是否存在。

不得公开：

- request body 或候选原文。
- provider、route、model 的敏感原值。
- source path、TextRef、图片字节或 base64。
- credential、token、cookie、Authorization 或完整 prompt/response。

## 6. 尚未闭环

1. 真实 Provider 上的 fidelity/cost/latency corpus 与签名 eval artifact；仓库不内置任何默认可信模型。
2. Capability/lifecycle 的用户操作写 API 与 authority 运维流程；当前只从受校验配置加载。
3. OpenAI Chat/Responses Optical active patcher；跨协议 bridge 明确只能 passthrough/shadow。
4. history collapse、cache-zone 持久恢复与独立 summary/drop 策略。
5. post-commit rollback 不可能安全自动重试；当前保证 pre-send 原文回退，commit 后只记录失败并禁止重复发送。
6. 面向用户的安全预览 UI；本任务按范围没有接入前端，也不得新增工程化“Optical Lab”一级导航。

## 7. 发布判断

| 能力 | 判断 |
|---|---|
| Shadow planner | 可继续开发和测试 |
| Metadata-only Preview API | 可作为实验可观测面 |
| Capability lifecycle visibility | 可作为实验状态面 |
| Provider dependency analysis | 可用于 fail-closed 规划 |
| Active execution code | 已实现，默认关闭且只限同协议 Anthropic closed historical tool result |
| Production active default | 禁止，缺少本地真实模型签名 eval 与候选版路由证据 |
| 对外成本节省承诺 | 禁止；本地 estimator 与第三方数据都不能替代 provider actual/paired evidence |

## 8. 验证

当前验证应重新执行，不沿用旧报告中的测试数量或“未提交/环境阻塞”结论：

```bash
cd code
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p dtoken-types
cargo test -p dtoken-config
cargo test -p dtoken-compress
cargo test -p dtoken-core-api
cargo test --workspace
```

阶段文件和历史提交记录可以查看 `support/pending-commits.json`，真实 Git 操作查看 `support/git-operations.json`。两份账本只用于追溯，不替代当前 Git 状态和测试结果。

## 9. 下一顺序

1. 用安全的 OS credential reference 在真实 Anthropic-compatible route 上生成 fidelity/cost/latency 证据。
2. 对同一 renderer/font/corpus 生成 eval artifact，并由配置的 Ed25519 authority 签署精确路由 Capability。
3. 运行 OpenCode → d-token → upstream 候选版验收并关联 route decision、Ledger 与 provider usage。
4. 扩充 malformed/adversarial/fault-injection corpus；OpenAI active 继续保持不可达。
5. 后续单独设计 history collapse/cache-zone；不得与首批 Optical active 混合启用。

## 10. Codex CLI pxpipe / Hybrid 视觉评测 V2

仓库已加入独立、默认不联网的 V2 实验harness：

- Rust eval-only H/R/B/P profile、Spleen 5×8 atlas、缩放、reflow、newline marker、banner/IDS和secret-first factsheet。
- exact model matcher只接受 `gpt-5.6-luna`、`gpt-5.6-terra`、`gpt-5.6-sol`；每个模型生成独立profile revision，不继承Sol证据。
- Python Stage 0/1/2/3状态机最多42次调用；标准费软停 `$4.25`，中转硬限额证据要求 `$5.00`，原子attempt和orphan禁止重试。
- Codex 0.144.1隔离适配使用全新最小`CODEX_HOME`+`--strict-config`；不使用会连隔离relay配置一起忽略的`--ignore-user-config`。provider URL只存在于临时配置，不进入argv或持久产物。
- 图片调用、provider usage、文字离线反事实、中转原始金额和缓存趋势在产物中分别标注。

当前状态：Rust 107项测试、Python新旧harness 35项测试、Rust harness clippy、结构检查以及12张Stage 1真实离线PNG/Stage 0均已通过；付费授权绑定已生成，但尚未执行V2模型调用，因此不能标记 `LAB_CANDIDATE`。默认 Capability registry仍为空，OpenAI active路径仍不可达。
