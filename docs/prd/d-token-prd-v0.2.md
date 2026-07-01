# d-token PRD v0.2

状态：Draft / Supersedes v0.1  
创建日期：2026-07-01

## 1. 核心结论

d-token 的产品定位从“借鉴 Headroom 的压缩路由工具”升级为：

> Headroom-compatible 的本地 AI Coding 压缩路由桌面 App。

这意味着：

1. Headroom 的核心能力必须完整保留。
2. Headroom Dashboard 必须完整迁移到 d-token Desktop。
3. d-token 在此基础上新增多 provider、多 route、多实例、cc-switch bridge、Windows/macOS 原生 App 和中文体验。

最终产品形态：

```text
Headroom-compatible Core
+ Headroom Dashboard Full Migration
+ d-token Provider / Route / Instance Layer
+ d-token Desktop App
= d-token
```

## 2. 对 v0.1 PRD 的审查结论

v0.1 的方向是对的，但执行约束不够硬。

主要问题：

1. Headroom 被写成“借鉴对象”，而不是必须保留的功能底座。
2. Dashboard 被写成普通监控页，没有要求迁移 Headroom Dashboard 的完整信息架构。
3. 没有 Headroom feature parity matrix，后续开发容易漏功能。
4. 没有 Dashboard migration matrix，后续 UI 容易缩水。
5. d-token 自有创新层与 Headroom-compatible core 的边界不够清晰。

v0.2 修正：

1. Headroom 功能全量保留变成 P0。
2. Headroom Dashboard 全量迁移变成 P0。
3. 所有新增 provider、route、instance 能力都必须进入 Dashboard 可观测链路。
4. CLI 不再是主入口，Desktop App 是主入口。

## 3. P0：Headroom 功能全量保留

以下功能族必须保留，不得删除：

1. Library / SDK 模式。
2. Local proxy 模式。
3. Agent wrap / unwrap 模式。
4. MCP server 模式。
5. Content-aware compression pipeline。
6. ContentRouter。
7. CacheAligner / prefix cache optimization。
8. SmartCrusher。
9. CodeCompressor / AST-aware compression。
10. Text / log / diff / JSON / code compression。
11. Optional ML compressor 入口。
12. CCR reversible compression。
13. 本地原文缓存与 retrieve。
14. Cross-agent memory。
15. Per-project memory。
16. SharedContext / inter-agent context handoff。
17. Failure learning。
18. Verbosity learning。
19. Output token reduction。
20. Verbosity steering。
21. Effort routing。
22. Output savings estimation。
23. Measured / estimated savings 标识。
24. Durable savings ledger。
25. Metrics endpoints。
26. Prometheus metrics。
27. Historical stats。
28. JSON / CSV export。
29. Cost tracking。
30. Per-model breakdown。
31. Per-client / per-agent breakdown。
32. Per-project breakdown。
33. Agent compatibility matrix。
34. Framework integrations。
35. Optional extras：ml、code、memory、vector、relevance、image、evals、pytorch-mps 等能力入口。
36. Doctor / perf / benchmark / troubleshooting 能力。

## 4. P0：Headroom Dashboard 全量迁移

d-token Desktop 必须迁移 Headroom Dashboard 的完整信息架构，包括：

1. Session / Historical 双视图。
2. 健康状态、版本、更新时间。
3. Light / Dark 主题切换。
4. Live Feed。
5. Telemetry 状态提示。
6. Savings profile / target savings 展示。
7. Money saved。
8. Token savings。
9. Output tokens saved。
10. Overhead。
11. Throughput：input、compression、forward、generation。
12. Agent usage。
13. Window contribution / efficiency。
14. Anomalies / discrepancies。
15. External service status visibility。
16. Prefix cache / provider cache。
17. Waste signal breakdown。
18. Savings over time。
19. Token usage。
20. Provider breakdown。
21. Performance / latency / TTFB / failed requests。
22. Pipeline timing breakdown。
23. Per-model token savings。
24. Per-project savings。
25. Recent requests expandable table。
26. Request transform details。
27. Waste signals per request。
28. Historical proxy compression。
29. Lifetime compression savings。
30. Lifetime tokens saved。
31. Active days。
32. Average saved per day。
33. Hourly / daily / weekly / monthly rollups。
34. JSON / CSV export。

新增 d-token 维度：

1. ProviderProfile。
2. RouteProfile。
3. Instance。
4. Chain mode。
5. OS process status。
6. Config source。

## 5. 分层架构

```text
┌──────────────────────────────────────────────┐
│ d-token Desktop                              │
│ - Headroom Dashboard full migration           │
│ - Provider / Route / Instance management      │
│ - Chinese-first UX                            │
└──────────────────────────────────────────────┘
                    │
┌──────────────────────────────────────────────┐
│ d-token Orchestration Layer                  │
│ - multi-instance manager                      │
│ - port manager                                │
│ - healthcheck                                 │
│ - config generator                            │
└──────────────────────────────────────────────┘
                    │
┌──────────────────────────────────────────────┐
│ Headroom-compatible Core                     │
│ - SDK / Proxy / MCP / Wrap                    │
│ - compression pipeline                        │
│ - CCR / memory / learn                        │
│ - metrics / savings ledger                    │
└──────────────────────────────────────────────┘
                    │
┌──────────────────────────────────────────────┐
│ d-token Routing Layer                        │
│ - provider profiles                           │
│ - route profiles                              │
│ - chain bridge                                │
│ - route decision                              │
└──────────────────────────────────────────────┘
```

## 6. Dashboard 迁移方式

推荐策略：

```text
Headroom Dashboard 信息架构
        ↓
React/Tauri 组件化重写
        ↓
保留统计语义和卡片结构
        ↓
增加 d-token provider / route / instance 维度
```

保留或兼容以下数据入口：

1. `GET /health`
2. `GET /stats`
3. `GET /stats-history`
4. `GET /metrics`
5. JSON / CSV export
6. live events feed

新增 d-token 数据入口：

1. `GET /providers`
2. `GET /routes`
3. `GET /instances`
4. `GET /chain/status`
5. `GET /route-decisions/recent`

## 7. 多实例方案

必须支持多个独立实例并行运行：

```text
instance-a
  -> d-token proxy A
  -> Headroom-compatible compression
  -> provider profile A

instance-b
  -> d-token proxy B
  -> Headroom-compatible compression
  -> provider profile B or chain bridge
```

Dashboard 中必须按 instance 拆分：

1. 请求数。
2. token saved。
3. provider。
4. route。
5. latency。
6. error。
7. process status。

## 8. 新里程碑

### M0：范围锁定

- v0.2 PRD。
- Headroom 全功能迁移矩阵。
- Dashboard 全量迁移矩阵。
- PRD 多维审查报告。

### M1：Headroom-compatible Core Skeleton

- SDK / Proxy / MCP / Wrap 模块边界。
- CCR store 接口。
- Savings ledger 接口。
- Metrics endpoint 接口。

### M2：Dashboard Full Migration Skeleton

- Tauri Desktop shell。
- React Dashboard 页面。
- Headroom Dashboard 卡片全量占位。
- `/stats` / `/stats-history` mock data。
- JSON / CSV export 占位。

### M3：Routing + Provider Layer

- Provider profile。
- Route profile。
- Chain bridge。
- 多实例管理。

### M4：Compression Engine Parity

- ContentRouter。
- SmartCrusher。
- CodeCompressor。
- CacheAligner。
- CCR retrieve。
- Memory / learn / output shaper。

### M5：Production Hardening

- Windows/macOS installer。
- 配置迁移。
- 端口与进程管理。
- 日志脱敏。
- benchmark/evals。

## 9. 成功指标

1. Headroom 功能迁移矩阵中 P0/P1 项覆盖率达到 100%。
2. Dashboard 卡片/表格/视图迁移覆盖率达到 100%。
3. provider / route / instance 链路不串线。
4. chain bridge 链路可观测。
5. 多实例可并行运行。
6. Windows/macOS 上无需命令行也能完成配置、启动、监控、导出。
7. token savings、output savings、per-model、per-project、per-agent、historical rollups 均可查看。
