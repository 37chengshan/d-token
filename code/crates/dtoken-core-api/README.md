# dtoken-core-api

更新时间：2026-07-11 晚间

`dtoken-core-api` 是 d-token-owned Rust runtime 的 loopback HTTP API。默认只绑定 `127.0.0.1`，服务桌面 App、本地 Agent 路由和开发验证，不是公网 API。

## 当前端点类别

- 协议入口：OpenAI Chat Completions、OpenAI Responses、Anthropic Messages。
- 运行状态：health、Dashboard payload、stats、history、metrics。
- 配置与路由：providers、routes、instances、active config、chain status、recent decisions。
- 可观测性：recent logs、Event Ledger、JSON/CSV export。
- 诊断：Doctor。
- Headroom-derived/integrated RTK：metadata-only status、stats、cache、preprocessor 信息。
- 项目能力：project policy、CCR、memory/learn、output shaping、Agent compatibility、protocol injection。
- Optical：shadow preview、status、capabilities、lifecycle、recent events。

## 安全边界

1. 不在端点中返回明文 credential、Authorization、cookie 或 auth 文件。
2. 日志、事件和导出默认只保留脱敏元数据，不存完整 prompt/response。
3. Provider/Route 决策必须显式并记录；禁止静默 fallback。
4. Optical API 只返回规划和脱敏元数据，不改写请求体。
5. 外部 Headroom dashboard 不是该 API 的产品数据源。

完整端点以 `src/lib.rs` 的 Router 注册为准，当前状态见 `docs/product/IMPLEMENTATION_STATUS.md`。

## 后端对接说明（2026-09-06）

- `/dashboard/summary` 是轻量汇总；核对单次模型、路由、压缩与结果请使用 `/dashboard/payload` 或脱敏导出中的 `request_id`，不能以 summary 缺少明细字段判定未执行。
- 压缩跳过/拒绝原因写入 Event Ledger 的 `metadata.compression_fallback_reason`，仅含固定原因码。无压缩候选仍可正常完成模型请求；上游失败不能显示成节省成功。
- 实际压缩验收需同时满足上游完成、前后输入减少、已应用变换、同请求 accounting 与四类账本关联。单次没有历史工具输出的转发成功不证明压缩生效。
- Runtime capability learning only records bounded metadata observed from successful upstream responses: completed stream terminals and structurally valid provider tool/function-call responses. Declared request tool schemas, generic HTTP success, failed/cancelled/incomplete streams, payload content, and secrets are not learning evidence. The bridge passes the actual upstream protocol observed by the route, but the current registry schema does not persist protocol as an observed field; capability summaries continue to show the protocol supplied by the resolve caller.
