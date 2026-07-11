# Optical / Hybrid Context Optimization 实现状态

更新时间：2026-07-11 晚间
事实来源：`code/crates/dtoken-compress/src/optical/`、`code/crates/dtoken-core-api/`、`code/crates/dtoken-types/`

## 1. 当前定位

Optical 当前是默认关闭的 shadow planner 与 metadata-only control plane。它可以分析候选上下文、风险、依赖关系、Capability 和经济性，但不能生成生产图片或改写真实请求。

```text
允许：shadow planning / preview / status / lifecycle / redacted events
禁止：renderer / request mutation / active routing / history collapse / production claim
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
- shadow/active 输入验证器；当前 active 仍被能力和发布门槛阻断。

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

当前 HTTP Preview 经过 JSON extractor；真正的 ingress raw bytes 尚未贯穿到 Preview Plan。因此 raw-byte 结构存在，但不能宣称 API 已证明原始网络请求字节身份。

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

## 6. 尚未实现

1. Capability authority、签名、信任来源和正式 lifecycle 写 API。
2. HTTP ingress 原始字节保留与 fingerprint wiring。
3. PNG renderer、分页/reflow 和稳定 geometry 实现。
4. Anthropic/OpenAI protocol patcher。
5. post-execution validator、rollback journal 和 rollback executor。
6. history collapse、cache zone、持久化 recovery cache。
7. 真实模型 fidelity corpus、质量 gate、延迟和成本校准。
8. 面向用户的安全预览 UI；不得新增工程化“Optical Lab”一级导航。

## 7. 发布判断

| 能力 | 判断 |
|---|---|
| Shadow planner | 可继续开发和测试 |
| Metadata-only Preview API | 可作为实验可观测面 |
| Capability lifecycle visibility | 可作为实验状态面 |
| Provider dependency analysis | 可用于 fail-closed 规划 |
| Active request transform | 禁止发布 |
| 对外成本节省承诺 | 禁止，缺少 renderer 与真实模型校准证据 |

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

1. Snapshot authority/signature/trust source 与只读 revocation audit。
2. 在 HTTP ingress 保留真实 raw bytes 并接入 fingerprint。
3. 扩充 Provider-specific malformed/adversarial fixtures。
4. Economics 升级为 P10 / Expected / P90 区间。
5. 在现有高级设置或分析信息架构中增加安全预览，不创建顶级实验室页面。
6. 完成 fidelity corpus 后，再按 feature flag 推进 renderer、patcher、validator 和 rollback。
