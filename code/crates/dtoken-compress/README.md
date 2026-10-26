# dtoken-compress

更新时间：2026-09-06

`dtoken-compress` 负责 d-token 的压缩策略、Headroom-derived pipeline 边界和 Optical / Hybrid Context Optimization 的 shadow planner。

## 当前职责

- compression policy：off / safe / balanced / aggressive / max 的解析与策略边界。
- 文本、代码、JSON、cache、CCR/memory/output-shaping 等 Headroom-derived 能力的 Rust-owned 组合边界。
- transforms、tokens before/after、公开 savings metadata 的标准化。
- Optical applicability、risk、ContextGraph、Capability Snapshot、geometry、economics 和 immutable Plan IR。
- 供 `dtoken-core-api` 使用的 metadata-only Optical preview/status/capability/event 数据。

## 压缩结果

`CompressionResult` 返回实际前后估算、变换名称及可选 `fallback_reason_code`。没有安全变换时保留原内容，并记录固定原因：`compression_disabled`、`compression_context_unresolved`、`compression_unsupported_context`、`compression_no_safe_candidate`、`compression_non_beneficial`、`compression_cache_evidence_invalid`、`compression_cache_evidence_mismatch` 或 `compression_validation_failed`。原因不包含请求文本或原始异常。成功压缩时该字段为空；工具定义排序与内容压缩分别记录，不能把排序算成内容节省。

runtime 将原因投影到同一请求的 Event Ledger `metadata.compression_fallback_reason`。这与上游失败不同：跳过压缩仍可正常转发，上游是否完成由请求终态与 accounting 记录决定。

## Optical 边界

源码包含确定性 PNG renderer、typed patcher、post-transform validator 和发送前恢复路径。正式桌面保持 shadow preview，默认不授予 active 能力；这些代码的存在不能替代真实模型的质量、费用和延迟验收，也不能证明用户请求已主动省 token。完整状态见 `docs/optical/implementation-status.md`。

## 安全原则

Secret、精确字符串、diff-sensitive、开放工具调用和不受支持的 Provider 状态必须 fail closed。默认 Capability registry 不授予 active 能力。
