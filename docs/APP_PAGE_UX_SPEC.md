# d-token App Page & UX Specification

状态：Living UX Spec  
归属：本文件是 d-token 桌面 App 页面、功能、体验逻辑与验收标准的专门文档。PRD 定义产品边界，PLAN 定义阶段任务，本文件定义 App 每个页面要解决什么问题、展示什么信息、用户如何操作、什么状态算体验闭环。

## 1. 设计目标

App 的核心目标不是把命令行搬到 GUI，而是把复杂的 Headroom-derived runtime、Integrated RTK、Provider、cc-switch、自定义转发端口、OpenCode 测试链路变成用户能理解的可视化流程。

用户应始终知道三件事：

```text
当前 agent 的请求从哪里来
当前请求经过了哪一层压缩和路由
最终会发到哪个 provider / cc-switch / 自定义端口
```

## 2. 全局信息架构

建议左侧导航：

```text
Setup
Agents
Runtime
Providers & Routes
Port Discovery
OpenCode Test Runner
Dashboard
Doctor
Logs
Config Diff
Backup & Rollback
Compatibility Matrix
Settings
```

顶部全局状态条：

```text
Runtime: running / stopped / degraded
Active Route: agent → compression → target
Listening Ports: d-token / cc-switch / custom provider / test ports
Safety Mode: fixture / shadow / managed
Doctor: green / warning / red
```

## 3. 页面清单与体验逻辑

### 3.1 Welcome / Setup Wizard

目标：让新用户不用理解 Headroom、RTK、proxy、cc-switch，也能完成第一轮配置。

核心 UI：

```text
[欢迎使用 d-token]
选择模式：
  ○ 傻瓜模式：推荐，一键安装和配置
  ○ 高级模式：自定义 provider、端口、cc-switch、协议

下一步：选择 agent
```

必须展示：

- 当前运行模式：Fixture / Shadow / Managed。
- 是否会写入真实配置。
- 是否会创建备份和回滚点。
- 是否会启动本地端口。

DoD：用户在任何真实写操作前都能看到 diff、风险和回滚说明。

### 3.2 Agent Selection

目标：选择要接入的 AI Coding 工具。

核心 UI：

```text
Agent Card:
  Claude Code      supported / needs config / route missing
  Codex CLI        supported / not installed
  OpenCode         supported / P0 test agent
  Cline            planned
```

每个 agent 必须展示：

- 安装状态。
- wrap/config 支持方式。
- provider route 状态。
- Windows native / WSL / macOS 支持等级。
- 是否支持 test route。

OpenCode 必须标记为 P0 E2E 测试 agent。

### 3.3 Runtime Status

目标：展示 d-token 内置 runtime 是否可用。

核心 UI：

```text
Runtime status card:
  d-token runtime: running
  compression layer: available
  integrated RTK: available
  local proxy: 127.0.0.1:xxxxx
  version provenance: available
```

必须展示：

- runtime 版本。
- upstream base commit / provenance。
- integrated RTK 状态。
- proxy/listening 状态。
- 最近健康检查结果。

DoD：用户能区分 runtime 故障、RTK 故障、route 故障、provider 故障。

### 3.4 Providers & Routes

目标：配置官方 provider、第三方自定义 URL、cc-switch、chain bridge。

核心 UI：

```text
Provider list:
  Official Anthropic
  Official OpenAI
  Custom OpenAI-compatible
  Custom Anthropic-compatible
  cc-switch
  Local provider

Route Builder:
  Agent → Compression → Route Adapter → Target
```

必须展示：

- Provider 类型：official / custom / cc_switch / local_chain。
- 协议：OpenAI / Anthropic / auto。
- base URL。
- 是否需要 API key。
- health check 结果。
- fallback 策略，默认 disabled。

体验要求：

- 自定义 URL 不得被伪装成 official provider。
- 任何 fallback 必须显式确认。
- 401/403/429/5xx 必须映射到可解释错误。

### 3.5 Port Discovery

目标：自动识别并展示 cc-switch 转发端口、自定义 provider 转发端口、d-token 测试端口和生产端口。

这是一个必须独立存在的页面或 Providers & Routes 下的一级 Tab，不应隐藏在日志里。

核心 UI：

```text
[Auto Detect Ports]

Detected endpoints:
  d-token runtime       127.0.0.1:18180   running   owned by d-token
  OpenCode server       127.0.0.1:4096    running   external
  cc-switch candidate   127.0.0.1:xxxx    unknown   needs confirm
  custom provider       127.0.0.1:1337    healthy   OpenAI-compatible

Actions:
  Rescan
  Confirm as cc-switch
  Confirm as custom provider
  Create test route
  Ignore
```

自动识别来源：

1. d-token 自己的 route registry。
2. Event Ledger 中的历史 route。
3. 用户配置的 ProviderProfile / RouteProfile。
4. OpenCode config / provider baseURL。
5. cc-switch 配置文件或进程参数。
6. 127.0.0.1 端口健康探测。
7. `/v1/models`、`/health`、`/global/health` 等非侵入式 endpoint。

识别规则：

- 只能默认扫描 loopback 地址。
- 不默认扫描公网或局域网地址。
- 不读取完整 prompt / response。
- 不打印 API key。
- 识别结果必须分为 confirmed / candidate / unknown。
- candidate 不得自动接入生产 route。

端口类型：

```ts
type PortKind =
  | 'dtoken_runtime'
  | 'dtoken_test_route'
  | 'opencode_server'
  | 'cc_switch'
  | 'custom_openai_provider'
  | 'custom_anthropic_provider'
  | 'unknown_local_service';
```

端口卡片字段：

```ts
interface DetectedPortEndpoint {
  id: string;
  host: '127.0.0.1' | 'localhost';
  port: number;
  kind: PortKind;
  confidence: 'confirmed' | 'candidate' | 'unknown';
  owner: 'dtoken' | 'external' | 'unknown';
  protocol_guess?: 'openai' | 'anthropic' | 'opencode_server' | 'unknown';
  health_status: 'healthy' | 'unhealthy' | 'not_checked';
  source: 'registry' | 'config' | 'ledger' | 'process' | 'probe' | 'manual';
  last_seen_at?: string;
  notes?: string;
}
```

DoD：用户能一眼看出：哪个端口是 d-token 自己开的，哪个是 OpenCode，哪个可能是 cc-switch，哪个是自定义 provider，哪个还没确认。

### 3.6 OpenCode Test Runner

目标：用 OpenCode 真实验证本地新端口链路。

核心 UI：

```text
OpenCode Test Runner

Step 1: Choose route
  ○ OpenCode → d-token test port → custom provider URL
  ○ OpenCode → d-token test port → cc-switch → final provider

Step 2: Allocate new test port
  Suggested: 127.0.0.1:18180
  [Allocate new port]

Step 3: Generate temporary OpenCode config
  [Preview diff]

Step 4: Run test
  [Start test route]
  [Open OpenCode]
  [Stop and release port]
```

必须展示：

- d-token test port。
- OpenCode server port。
- target provider / cc-switch endpoint。
- route chain。
- request 是否进入 d-token。
- compression 是否执行。
- provider/cc-switch 是否收到请求。
- 测试结束是否释放端口。

体验要求：

- 本地测试不得复用生产端口。
- 如果端口占用，自动换端口，不盲目 kill。
- OpenCode config patch 必须可预览、可回滚。
- 默认使用临时 HOME / 临时 project config。
- 检查 `NO_PROXY=localhost,127.0.0.1`，避免本地代理循环。

### 3.7 Dashboard

目标：展示请求、压缩、路由、成本、错误与收益。

核心 UI：

```text
Request flow:
Agent → d-token runtime → compression → route adapter → cc-switch/custom provider → final provider

Savings:
RTK command-output savings
runtime context savings
route overhead
total end-to-end savings
```

必须展示：

- 当前请求链路。
- active route。
- token savings 分层。
- provider latency。
- cc-switch double-hop latency。
- fallback 是否发生。
- OpenCode test route 独立标记。

DoD：Dashboard 不得把 fixture 数据伪装成真实数据；必须显示数据来源。

### 3.8 Doctor

目标：给用户解释为什么失败，以及怎么修。

核心 UI：

```text
Doctor results:
  Runtime        green
  Port           warning: candidate cc-switch not confirmed
  Route          red: OpenCode not pointing to d-token test port
  Provider       green
  Security       green
```

必须检查：

- runtime / integrated RTK。
- d-token 端口。
- OpenCode server 端口。
- cc-switch candidate 端口。
- custom provider URL。
- route decision。
- provider auth。
- no silent fallback。
- Windows shim / macOS quarantine。
- Tauri capability / IPC 安全边界。

### 3.9 Logs

目标：展示可解释日志，而不是泄露隐私的原始日志。

必须支持：

- stage filter：install / route / compression / provider / cc-switch / OpenCode test。
- request id filter。
- redacted view 默认开启。
- export redacted report。

禁止默认展示：

- 完整 prompt。
- 完整 response。
- 完整代码文件。
- API key / Authorization。

### 3.10 Config Diff

目标：任何真实写配置前都能看到 diff。

必须展示：

- 文件路径。
- 修改原因。
- before / after。
- 风险等级。
- rollback point。

OpenCode 测试模式中，优先使用临时配置；如必须 patch 用户配置，必须二次确认。

### 3.11 Backup & Rollback

目标：用户敢点安装，因为知道能恢复。

必须展示：

- 最近 rollback points。
- 影响文件。
- 影响端口 / 进程。
- 是否可自动恢复。
- 恢复后 Doctor 结果。

### 3.12 Compatibility Matrix

目标：展示不同系统、shell、agent、route 的支持等级。

必须包含：

- macOS。
- Windows native。
- Windows WSL。
- Claude Code。
- Codex CLI。
- OpenCode。
- custom provider route。
- cc-switch route。
- OpenCode test port support。

### 3.13 Settings

目标：管理全局偏好和安全边界。

必须包含：

- 语言：中文 / English。
- 默认模式：Fixture / Shadow / Managed。
- 默认端口范围。
- 是否启用端口自动识别。
- 是否允许非 loopback：默认 false。
- 日志脱敏级别。
- 密钥存储方式。
- 更新通道。

## 4. 关键用户体验路径

### 4.1 新用户傻瓜路径

```text
打开 App
  → 选择傻瓜模式
  → 选择 OpenCode 或 Claude Code
  → 选择 provider
  → App 自动分配 d-token 端口
  → App 生成 diff 和 rollback point
  → 启动
  → Dashboard 展示请求链路
```

### 4.2 高级用户自定义 URL 路径

```text
打开 Providers & Routes
  → Add custom provider
  → 输入 base URL
  → Health check
  → 选择 agent
  → 生成 route
  → Dashboard 验证请求经过 d-token
```

### 4.3 cc-switch 路径

```text
打开 Port Discovery
  → Auto Detect
  → 发现 cc-switch candidate
  → 用户确认 endpoint
  → Route Builder 创建 compress_then_cc_switch
  → OpenCode Test Runner 新开端口验证
  → Dashboard 显示 final provider
```

### 4.4 OpenCode 本地测试路径

```text
打开 OpenCode Test Runner
  → 选择 custom provider 或 cc-switch
  → Allocate new test port
  → Preview temporary OpenCode config
  → Start test route
  → Run OpenCode
  → Dashboard 看到请求
  → Stop and release port
  → Export report
```

## 5. UX 红线

1. 不允许用户不知道请求最终发到哪里。
2. 不允许自定义 provider 被显示成 official provider。
3. 不允许 candidate 端口自动进入生产 route。
4. 不允许测试端口复用生产端口。
5. 不允许未验证进程身份就 kill。
6. 不允许默认保存完整 prompt / response / 代码。
7. 不允许 GUI 只显示 “token error”，必须区分 key invalid、route mismatch、provider unreachable、cc-switch unreachable。
8. 不允许 fixture 数据伪装成真实数据。

## 6. 页面实现优先级

```text
P0:
  Setup Wizard
  Providers & Routes
  Port Discovery
  OpenCode Test Runner
  Dashboard route chain
  Doctor route/port diagnosis

P1:
  Config Diff
  Backup & Rollback
  Compatibility Matrix
  Logs export

P2:
  Advanced settings
  Historical analytics
  Theme/polish
```

## 7. 验收标准

1. 用户能在 GUI 中看到 d-token、OpenCode、cc-switch、自定义 provider 的端口关系。
2. 用户能自动识别 cc-switch candidate，但必须手动确认后才能用于生产 route。
3. 用户能自动识别自定义 provider port，并看到协议猜测与健康检查。
4. 用户能通过 OpenCode Test Runner 新开测试端口完成 custom provider route smoke。
5. 用户能通过 OpenCode Test Runner 新开测试端口完成 cc-switch route smoke。
6. Dashboard 能展示 agent → runtime → compression → route → target 的完整链路。
7. Doctor 能解释端口占用、route mismatch、provider 401、cc-switch unreachable、OpenCode 未指向 test port。
8. 所有真实配置修改都有 diff、backup、rollback。
9. 所有敏感信息默认脱敏。
10. 所有页面都支持中文优先显示。
