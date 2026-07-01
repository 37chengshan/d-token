# d-token 系统架构 v0.1

## 1. 总体架构

```text
Claude Code / Codex / Cline / OpenCode / Custom Agent
        │
        │ Anthropic-compatible / OpenAI-compatible request
        ▼
┌─────────────────────────────────────────────────────┐
│ d-token Local Proxy                                  │
│  - protocol detector                                 │
│  - route profile matcher                             │
│  - compression policy executor                       │
│  - local reversible cache                            │
│  - request log / metrics                             │
└─────────────────────────────────────────────────────┘
        │
        ├──────── direct ────────► Official Provider
        │                         - Anthropic
        │                         - OpenAI
        │
        ├──────── direct ────────► Custom Provider
        │                         - OpenAI-compatible
        │                         - Anthropic-compatible
        │
        └──── via_cc_switch ────► cc-switch
                                  └── final provider routing
```

核心原则：

- d-token 是 **前置压缩层 + 本地路由层 + 可视化控制层**。
- cc-switch 是可选二级路由目标，不替代 d-token 的压缩与可观测性。
- 桌面 App 是主入口，CLI 是开发/调试入口。

---

## 2. 模块划分

### 2.1 apps/desktop

桌面应用层。

职责：

- Provider 配置 UI。
- Instance 配置 UI。
- Proxy 启停。
- 日志与指标展示。
- 中文交互与首次启动向导。

推荐技术：

- Tauri
- React / TypeScript
- Vite
- Rust command bridge

### 2.2 crates/dtoken-proxy

本地代理服务。

职责：

- 监听本地端口。
- 接收 Anthropic/OpenAI-compatible 请求。
- 调用 router 决策上游。
- 调用 compress 模块压缩。
- 转发到 provider 或 cc-switch。
- 写入请求日志。

### 2.3 crates/dtoken-router

路由决策模块。

职责：

- 加载 provider profile。
- 加载 route profile。
- 判断 direct / via_cc_switch。
- 避免第三方 provider 静默回落官方上游。
- 返回 route decision。

### 2.4 crates/dtoken-compress

压缩模块。

职责：

- safe/balanced/aggressive policy。
- 文本、日志、工具输出、JSON、代码片段压缩。
- 原文缓存与 retrieval id。
- token before/after 估算。

### 2.5 sidecars/cc-switch-bridge

cc-switch 联动层。

职责：

- 管理 d-token 到 cc-switch 的转发配置。
- 健康检查 cc-switch endpoint。
- 显示二段链路状态。
- 记录 cc-switch route metadata。

### 2.6 packages/sdk

SDK 与共享类型。

职责：

- 暴露 JS/TS 类型。
- 供桌面 App、测试工具或外部集成使用。

---

## 3. 请求链路

### 3.1 Claude 官方链路

```text
Claude Code
  -> http://127.0.0.1:18011
  -> d-token proxy
  -> compress
  -> Anthropic official
```

### 3.2 Claude 第三方链路

```text
Claude Code
  -> http://127.0.0.1:18012
  -> d-token proxy
  -> compress
  -> custom_anthropic provider
```

### 3.3 Codex 官方链路

```text
Codex official instance
  -> http://127.0.0.1:18001
  -> d-token proxy A
  -> compress
  -> OpenAI official
```

### 3.4 Codex 中转/第三方链路

```text
Codex relay instance
  -> http://127.0.0.1:18002
  -> d-token proxy B
  -> compress
  -> custom_openai provider
```

### 3.5 cc-switch 链式链路

```text
Claude/Codex
  -> http://127.0.0.1:18003
  -> d-token proxy
  -> compress
  -> http://127.0.0.1:<cc-switch-port>
  -> cc-switch route
  -> final provider
```

---

## 4. 数据模型草案

### 4.1 ProviderProfile

```ts
export interface ProviderProfile {
  id: string;
  name: string;
  kind: 'anthropic_official' | 'openai_official' | 'custom_anthropic' | 'custom_openai' | 'cc_switch';
  protocol: 'anthropic' | 'openai';
  baseUrl: string;
  apiKeyEnv?: string;
  apiKeyStorageRef?: string;
  models?: string[];
  timeoutMs: number;
  healthcheckPath?: string;
  notes?: string;
}
```

### 4.2 RouteProfile

```ts
export interface RouteProfile {
  id: string;
  name: string;
  agent: 'claude-code' | 'codex' | 'cline' | 'opencode' | 'custom';
  inputProtocol: 'anthropic' | 'openai';
  compressionPolicy: 'off' | 'safe' | 'balanced' | 'aggressive';
  chain: 'direct' | 'via_cc_switch';
  targetProviderId: string;
  fallbackProviderId?: string;
  bindHost: '127.0.0.1';
  localPort: number;
}
```

### 4.3 RequestLog

```ts
export interface RequestLog {
  requestId: string;
  timestamp: string;
  agent: string;
  instanceId: string;
  model?: string;
  inputProtocol: 'anthropic' | 'openai';
  chain: 'direct' | 'via_cc_switch';
  targetProviderId: string;
  statusCode?: number;
  latencyMs?: number;
  tokensBefore?: number;
  tokensAfter?: number;
  savingsRatio?: number;
  errorCategory?: string;
  errorMessage?: string;
}
```

---

## 5. 关键工程约束

- 所有配置文件使用 UTF-8。
- 所有本地服务默认绑定 `127.0.0.1`。
- API key 与 Authorization header 默认脱敏。
- Windows 外部 CLI 调用要兼容 `.cmd` / `.exe` / `.bat`。
- 不靠目录名猜项目路径，优先读取 session 中的 cwd。
- 每个实例必须有唯一 `instance_id`、端口、日志路径。
