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
