# dtoken-compress

更新时间：2026-07-11 晚间

`dtoken-compress` 负责 d-token 的压缩策略、Headroom-derived pipeline 边界和 Optical / Hybrid Context Optimization 的 shadow planner。

## 当前职责

- compression policy：off / safe / balanced / aggressive / max 的解析与策略边界。
- 文本、代码、JSON、cache、CCR/memory/output-shaping 等 Headroom-derived 能力的 Rust-owned 组合边界。
- transforms、tokens before/after、公开 savings metadata 的标准化。
- Optical applicability、risk、ContextGraph、Capability Snapshot、geometry、economics 和 immutable Plan IR。
- 供 `dtoken-core-api` 使用的 metadata-only Optical preview/status/capability/event 数据。

## Optical 边界

当前已经实现 shadow planning，但未实现：

- PNG renderer。
- request patcher 或 active mutation。
- post-execution validator。
- rollback executor。
- history collapse 和生产 recovery cache。

因此任何调用方都不得把 Optical 描述为已在真实请求中主动省 token。完整状态见 `docs/optical/implementation-status.md`。

## 安全原则

Secret、精确字符串、diff-sensitive、开放工具调用和不受支持的 Provider 状态必须 fail closed。默认 Capability registry 不授予 active 能力。
