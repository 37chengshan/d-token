# d-token SDK

更新时间：2026-07-30

当前目录提供一个最小可用的 TypeScript HTTP binding，面向 d-token 本地 loopback runtime 的稳定压缩决策合同：

- 入口：`src/index.ts`
- 当前能力：`compressionDecision()` 与 body-free typed reads：`runtimeStats()`、`runtimeMetricsSummary()`、`ccrSummary()`、`ccrMetadata()`、`memorySummary()`、`opticalStatus()`、`opticalCapabilities()`、`mcpStatus()`
- 合同来源：`POST /v1/compression/decision`
- 对齐目标：与 `dtoken-core-api` 的公共 Rust API、HTTP surface、MCP `headroom_compress` 返回同一 `CompressionDecision` 合同

边界：

1. 这是薄 SDK，不复制压缩业务逻辑，只调用 d-token-owned HTTP surface。
2. `ccrMetadata()` 只读取未授权 loopback metadata surface；SDK 不提供 CCR original retrieve/delete，也不接收 managed capability。
3. `memorySummary()` 只报告 HTTP surface 的 `scope_required`/unavailable 摘要；SDK 不提供 SharedContext export/write/delete，精确 scope 仍只由 managed MCP capability 建立。
4. Optical 与 metrics 没有对应 MCP tool 时保持 unavailable；SDK 不伪造 MCP 能力，只读取真实 HTTP 摘要和 `/headroom/mcp` 的工具边界。
5. 当前不包含 Provider/Route/Instance/Event 的全量管理客户端，也不处理桌面认证、远程发布或浏览器分发，仅用于本地 runtime 集成。

本地验证：

```bash
npm run build
npm test
```
