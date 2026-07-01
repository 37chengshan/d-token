# Headroom 功能全量迁移矩阵

状态：Draft  
日期：2026-07-01  
原则：Headroom 的核心功能在 d-token 中必须保留。迁移可以重写实现和 UI，但不能删功能。

## 1. 迁移状态定义

| 状态 | 含义 |
|---|---|
| `must` | P0 必须迁移 |
| `should` | P1 应迁移 |
| `deferred` | 阶段性延后，但不得删除 |
| `extended` | d-token 在 Headroom 基础上增强 |

## 2. 功能矩阵

| 功能族 | Headroom 能力 | d-token 要求 | 状态 |
|---|---|---|---|
| Entry | Python library / SDK | 保留 SDK 能力，后续可提供 Rust/TS binding | must |
| Entry | TypeScript SDK | 保留 TS SDK 或兼容 API | must |
| Entry | Local proxy | 保留，并增强 provider/route/instance 维度 | must |
| Entry | MCP server | 保留 compress / retrieve / stats | must |
| Entry | Agent wrap / unwrap | 保留，并通过桌面 App 图形化 | must |
| Compression | ContentRouter | 保留内容类型识别与压缩策略选择 | must |
| Compression | SmartCrusher | 保留 JSON/数组/日志等结构化压缩 | must |
| Compression | CodeCompressor | 保留 AST-aware code compression | must |
| Compression | Text/log/diff compression | 保留 | must |
| Compression | Optional ML compressor | 保留入口，可阶段性延后模型集成 | should |
| Cache | CacheAligner | 保留 provider prefix cache 优化 | must |
| CCR | Compress-Cache-Retrieve | 保留可逆压缩 | must |
| CCR | Local original store | 保留本地原文缓存与 TTL | must |
| CCR | Retrieve by hash/query | 保留 | must |
| Memory | Cross-agent memory | 保留，并纳入多实例维度 | must |
| Memory | Per-project memory | 保留，避免跨项目污染 | must |
| Memory | SharedContext | 保留 inter-agent handoff | should |
| Learn | Failure learning | 保留写入本地规则文件能力 | must |
| Learn | Verbosity learning | 保留，并与 output shaper 联动 | must |
| Output | Output token reduction | 保留 | must |
| Output | Verbosity steering | 保留 | must |
| Output | Effort routing | 保留 | must |
| Output | Output savings estimate | 保留估算与置信区间 | must |
| Output | Holdout measurement | 保留 measured/estimated 标识 | should |
| Metrics | `/stats` | 保留兼容语义 | must |
| Metrics | `/stats-history` | 保留历史数据 | must |
| Metrics | `/metrics` Prometheus | 保留 | should |
| Metrics | Durable savings ledger | 保留跨进程、跨重启统计 | must |
| Metrics | JSON/CSV export | 保留 | must |
| Metrics | Cost tracking | 保留 | must |
| Breakdown | Per-model | 保留 | must |
| Breakdown | Per-agent/client | 保留 | must |
| Breakdown | Per-project | 保留 | must |
| Ops | doctor | 保留并桌面化 | must |
| Ops | perf | 保留 | should |
| Ops | benchmarks/evals | 保留为开发验收工具 | should |
| Integration | OpenAI SDK | 保留 | should |
| Integration | Anthropic SDK | 保留 | should |
| Integration | Vercel AI SDK | 保留 | should |
| Integration | LangChain | 保留 | should |
| Integration | Agno | 保留 | should |
| Integration | Strands | 保留 | should |
| Integration | LiteLLM | 保留 cost/model pricing 能力 | should |
| Agent | Claude Code | 保留并增强 route/profile 管理 | must |
| Agent | Codex | 保留并增强多实例管理 | must |
| Agent | Cursor | 保留手动配置引导 | should |
| Agent | Aider | 保留 | should |
| Agent | OpenCode | 保留 | should |
| Agent | Cline | 保留 | should |
| Agent | Continue | 保留 | should |
| Agent | Goose | 保留 | should |
| Agent | OpenHands | 保留 | should |
| Agent | OpenClaw | 保留 | should |
| Agent | Vibe | 保留 | should |
| d-token Extension | Provider profiles | 新增 | extended |
| d-token Extension | Route profiles | 新增 | extended |
| d-token Extension | Chain bridge | 新增 | extended |
| d-token Extension | Desktop App | 新增 | extended |
| d-token Extension | Windows/macOS process manager | 新增 | extended |

## 3. 验收规则

- `must` 项不得删除。
- `should` 项若延期，必须写入里程碑。
- `deferred` 项必须有版本、原因和替代方案。
- Dashboard 相关指标必须在 `docs/prd/headroom-dashboard-migration-matrix.md` 单独追踪。
