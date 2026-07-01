# d-token 总体架构

状态：Accepted  
关联：`docs/PRD.md`、`docs/TECH_STACK.md`

## 1. 架构目标

d-token 的架构目标是：

1. 保留 Headroom-compatible core。
2. 完整承接 Headroom Dashboard。
3. 新增 provider、route、instance、chain bridge。
4. 支持 Windows/macOS 原生 App。
5. 所有请求链路可观测。
6. 默认本地优先、默认安全。

## 2. 总体分层

```text
┌──────────────────────────────────────────────┐
│ d-token Desktop                              │
│ - Tauri 2                                    │
│ - React / TypeScript Dashboard               │
│ - Provider / Route / Instance UI             │
│ - Doctor / Logs / Export                     │
└──────────────────────────────────────────────┘
                    │
┌──────────────────────────────────────────────┐
│ Rust Orchestration Core                      │
│ - local API server                            │
│ - process manager                             │
│ - port manager                                │
│ - config loader                               │
│ - healthcheck                                 │
│ - request log writer                          │
└──────────────────────────────────────────────┘
                    │
┌──────────────────────────────────────────────┐
│ Headroom-compatible Core                     │
│ - compression pipeline                        │
│ - ContentRouter                               │
│ - SmartCrusher                                │
│ - CodeCompressor                              │
│ - CCR                                         │
│ - memory / learn / output shaper              │
└──────────────────────────────────────────────┘
                    │
┌──────────────────────────────────────────────┐
│ Routing Layer                                │
│ - ProviderProfile                             │
│ - RouteProfile                                │
│ - Instance                                    │
│ - Chain bridge                                │
└──────────────────────────────────────────────┘
                    │
       Official Provider / Custom Provider / Local Chain
```

## 3. 请求链路

```text
AI Coding Tool
  ↓
d-token Local Proxy
  ↓
Protocol Detector
  ↓
Compression Policy Executor
  ↓
Route Decision
  ↓
Provider / Chain Bridge
  ↓
Response Stream
  ↓
Metrics + Logs + Dashboard
```

每条请求必须记录：

1. request id。
2. timestamp。
3. agent。
4. instance id。
5. input protocol。
6. provider id。
7. route id。
8. chain mode。
9. model。
10. input tokens before。
11. input tokens after。
12. output tokens。
13. tokens saved。
14. transforms applied。
15. latency。
16. error if any。
17. config source。

## 4. 核心模块

### 4.1 Desktop

位置：`code/apps/desktop`

职责：

1. Dashboard。
2. Provider 管理。
3. Route 管理。
4. Instance 管理。
5. Doctor。
6. Logs。
7. Export。
8. Settings。
9. Sidecar lifecycle UI。

### 4.2 Rust core

位置：`code/crates/*`

职责：

1. `dtoken-proxy`：本地 HTTP proxy。
2. `dtoken-router`：provider/route/chain decision。
3. `dtoken-compress`：compression 接口与策略层。
4. `dtoken-core-api`：Desktop 调用的 local API。
5. `dtoken-process`：sidecar 和实例进程管理。

### 4.3 Python sidecar

位置：`code/sidecars/headroom-compatible`

职责：

1. Headroom-compatible compression。
2. CCR。
3. memory。
4. learn。
5. output shaper。
6. MCP compatibility。
7. evals/benchmarks。

### 4.4 Chain bridge

位置：`code/sidecars/chain-bridge`

职责：

1. 负责多段本地链路观测。
2. 记录 chain mode。
3. 做 healthcheck。
4. 不隐藏上游错误。
5. 不做静默 fallback。

## 5. Local API

Desktop 调用 Rust core 的本地 API：

```text
GET /health
GET /stats
GET /stats-history
GET /metrics
GET /providers
GET /routes
GET /instances
GET /chain/status
GET /route-decisions/recent
GET /logs/recent
POST /instances/start
POST /instances/stop
POST /instances/restart
POST /doctor/run
GET /export/json
GET /export/csv
```

## 6. 数据存储

```text
SQLite
  request index
  dashboard query cache
  provider status
  route status
  instance status

JSONL ledger
  savings events
  request events
  debug events

YAML / JSON config
  provider profiles
  route profiles
  instance profiles

OS keychain
  sensitive credentials
```

## 7. 配置模型

### 7.1 ProviderProfile

```ts
interface ProviderProfile {
  id: string;
  name: string;
  kind: 'official' | 'custom' | 'local_chain';
  protocol: 'openai' | 'anthropic';
  baseUrl: string;
  apiKeyRef?: string;
  models?: string[];
  timeoutMs: number;
  healthcheck?: string;
}
```

### 7.2 RouteProfile

```ts
interface RouteProfile {
  id: string;
  name: string;
  inputProtocol: 'openai' | 'anthropic';
  instanceId: string;
  targetProviderId: string;
  fallbackProviderId?: string;
  chainMode: 'direct' | 'chain_bridge';
  compressionPolicy: 'off' | 'safe' | 'balanced' | 'aggressive';
  bindHost: '127.0.0.1';
  localPort: number;
}
```

### 7.3 InstanceProfile

```ts
interface InstanceProfile {
  id: string;
  name: string;
  agent: string;
  routeId: string;
  localPort: number;
  processStatus: 'stopped' | 'starting' | 'running' | 'error';
  logPath: string;
}
```

## 8. Dashboard 原则

1. 不做简化版。
2. 所有 Headroom Dashboard 区块必须有位置。
3. 未接入真实数据时显示占位，不删除卡片。
4. 每个卡片都应尽量支持 instance/provider/route 过滤。
5. 所有错误都应该能从 request log 追溯到 route decision。

## 9. 运行原则

1. Desktop 可以关闭，proxy 可选择继续运行。
2. 每个实例独立端口、独立日志、独立 route。
3. sidecar 必须可观察、可停止、可重启。
4. 所有敏感字段默认脱敏。
5. 不做静默 provider fallback。
6. 默认只绑定 `127.0.0.1`。
