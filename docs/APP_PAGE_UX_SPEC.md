# d-token App Page & UX Specification

状态：Living UX Spec  
归属：本文件是 d-token 桌面 App 页面、功能、体验逻辑与验收标准的专门文档。PRD 定义产品边界，PLAN 定义阶段任务，本文件定义 App 每个页面要解决什么问题、展示什么信息、用户如何操作、什么状态算体验闭环。

## 1. 设计原则

### 1.1 产品叙事

d-token App 不是把命令行搬到 GUI，而是把 AI Coding agent 的压缩、路由、端口、配置、诊断、回滚，从手改文件和手开终端，变成可视化、本地优先、可审计、可恢复的桌面工作流。

用户不应该需要理解 Headroom、RTK、proxy、hook、base URL、cc-switch、OpenAI-compatible、Anthropic-compatible 的全部底层细节。App 要把这些复杂概念翻译成三个用户能看懂的问题：

```text
当前 agent 的请求从哪里来？
请求经过了哪一层压缩、路由和转发？
最终会发到哪个 provider / cc-switch / 自定义端口？
```

### 1.2 体验红线

1. 不允许用户不知道请求最终发到哪里。
2. 不允许自定义 provider 被显示成 official provider。
3. 不允许 candidate 端口自动进入生产 route。
4. 不允许测试端口复用生产端口。
5. 不允许未验证进程身份就 kill。
6. 不允许默认保存完整 prompt、完整 response、完整代码文件或完整命令输出。
7. 不允许 GUI 只显示 “token error”；必须区分 key invalid、route mismatch、provider unreachable、cc-switch unreachable。
8. 不允许 fixture 数据伪装成真实数据。
9. 不允许完整 API key / Authorization / Cookie 进入 WebView 明文状态。
10. 不允许真实配置修改缺少 diff、backup、rollback。

### 1.3 页面设计方法

每个页面都必须回答四件事：

```text
Pain: 这个页面解决什么痛点？
Action: 用户可以点什么、改什么、运行什么？
Feedback: 操作后用户看到什么状态、证据、错误和修复建议？
Recovery: 失败后如何恢复、回滚、重新检测？
```

## 2. 全局信息架构

### 2.1 推荐导航

```text
Home
Setup Wizard
Agents
Runtime
Providers & Routes
Port Discovery
OpenCode Test Runner
Secrets & Credentials
Dashboard
Doctor
Config Ownership
Config Diff
Backup & Rollback
Maintenance
Compatibility Matrix
Settings
```

### 2.2 页面优先级

```text
P0:
  Home
  Setup Wizard
  Providers & Routes
  Port Discovery
  OpenCode Test Runner
  Dashboard
  Doctor
  Config Diff
  Backup & Rollback

P1:
  Agents
  Runtime
  Secrets & Credentials
  Config Ownership
  Maintenance
  Compatibility Matrix
  Settings

P2:
  Historical Analytics
  Advanced Logs
  Plugin / Extension Center
  Theme / Appearance polish
```

### 2.3 顶部全局状态条

所有页面顶部都应显示全局状态条：

```text
Runtime: running / stopped / degraded
Active Route: agent → compression → target
Listening Ports: d-token / OpenCode / cc-switch / custom provider / test ports
Safety Mode: fixture / shadow / managed
Doctor: green / warning / red
```

全局状态条必须点击可展开，展示：

1. 当前运行模式。
2. 当前 runtime 端口。
3. 当前 test route 端口。
4. 当前 active provider。
5. 当前 fallback 策略。
6. 最近一次 Doctor 结果。
7. 最近一次写配置/回滚记录。

## 3. 核心用户路径

### 3.1 第一次安装路径

```text
打开 d-token
  → Home 显示未安装 / 未配置
  → Setup Wizard
  → Scan local environment
  → 选择傻瓜模式或高级模式
  → 选择 OpenCode / Claude Code / Codex
  → 选择 provider 或 cc-switch
  → Port Discovery 识别现有端口
  → Install Plan
  → Config Diff
  → Apply
  → Verify
  → Doctor green / warning / red
  → 生成 Rollback Point
  → Dashboard 显示 active route
```

用户体验目标：

1. 新用户知道 App 会做什么。
2. 新用户知道 App 会改什么。
3. 新用户知道失败后能不能恢复。
4. 新用户知道请求最终发到哪里。

### 3.2 日常使用路径

```text
打开 d-token
  → Home
  → 查看 runtime / route / port / Doctor 总状态
  → Start / Stop runtime
  → View ports
  → Run OpenCode test
  → Add provider
  → Dashboard 看请求和 savings
```

用户体验目标：Home 是日常控制台，Setup Wizard 只是首次安装或重新配置入口。

### 3.3 自定义 provider 路径

```text
Providers & Routes
  → Add custom provider
  → 填 base URL
  → 选择 OpenAI-compatible / Anthropic-compatible / auto
  → Health check
  → Port Discovery 标记为 custom provider
  → Route Simulator 显示请求会去哪
  → OpenCode Test Runner 新端口测试
  → Promote test route to production route
```

### 3.4 cc-switch 路径

```text
Port Discovery
  → Auto Detect
  → 发现 cc-switch candidate
  → 查看 evidence
  → Confirm as cc-switch
  → Route Builder 创建 compress_then_cc_switch
  → Route Simulator 显示最终路径
  → OpenCode Test Runner 新端口测试
  → Dashboard 显示 final provider
```

### 3.5 失败恢复路径

```text
Doctor red
  → 显示失败阶段：runtime / port / route / provider / cc-switch / credential
  → 展示证据和修复建议
  → 一键打开 Config Diff、Port Discovery 或 Secrets
  → 修复后 Run Verify
  → 仍失败则 Rollback
```

## 4. 页面规格

### 4.1 Home / Overview

Pain：用户每天打开 App 后，需要一个总控台知道当前系统是否可用。Setup Wizard 不能承担日常控制台职责。

核心 UI：

```text
Home

Status cards:
  Runtime           running / stopped / degraded
  Active route      OpenCode → d-token → compression → cc-switch → provider
  Ports             5 listening, 1 warning
  Doctor            green / warning / red
  Savings today     RTK / runtime / route overhead
  Errors today      provider 401 / cc-switch unreachable / route mismatch

Quick actions:
  Start runtime
  Stop runtime
  Add provider
  View ports
  Run OpenCode test
  Open Doctor
```

必须展示：

1. 当前安全模式：Fixture / Shadow / Managed。
2. 当前 runtime 状态。
3. 当前 active route。
4. 当前监听端口摘要。
5. 今日请求数、savings、错误。
6. Doctor 总状态。
7. 快捷操作。

Recovery：Home 中所有 red/warning card 必须能点击进入对应页面，例如 Doctor、Port Discovery、Secrets、Config Diff。

DoD：用户打开 App 10 秒内能知道“能不能用、请求去哪、哪里有风险”。

### 4.2 Setup Wizard

Pain：新用户不想手动装 Headroom/RTK，不想手改 agent/provider/cc-switch 配置。

Setup Wizard 必须是完整 stepper，而不是普通设置页。

```text
1. Welcome
2. Scan Environment
3. Choose Mode
4. Choose Agent
5. Choose Runtime
6. Choose Provider / cc-switch
7. Port Discovery
8. Install Plan
9. Config Diff
10. Apply
11. Verify
12. Doctor Result
13. Rollback Point
```

每一步必须遵循：

```text
输入 → 检测 → 风险提示 → 用户确认 → 结果 → 下一步
```

关键状态：

1. Fixture Demo：不碰真实环境。
2. Shadow Detect：只读真实环境。
3. Managed Install：允许写配置，但必须 diff / backup / rollback。

必须展示：

1. 是否会写入真实配置。
2. 会修改哪些文件。
3. 会启动哪些本地端口。
4. 会创建哪些备份。
5. 会生成什么 rollback point。
6. 请求最终默认 route。

DoD：任何真实写操作前，用户都能看到 install plan、config diff、风险等级和 rollback 方案。

### 4.3 Agents

Pain：用户不知道自己的 Claude Code、Codex、OpenCode、Cline 是否已安装、是否能被 d-token 接入。

核心 UI：

```text
Agent Card:
  OpenCode         supported / P0 test agent / route ready
  Claude Code      supported / needs config / route missing
  Codex CLI        supported / not installed
  Cline            planned / manual proxy only
```

每个 agent 必须展示：

1. 安装状态。
2. binary / shim 检测结果。
3. wrap/config 支持方式。
4. provider route 状态。
5. Windows native / WSL / macOS 支持等级。
6. 是否支持 test route。
7. 最近一次请求是否经过 d-token。

OpenCode 必须标记为 P0 E2E 测试 agent。

Recovery：agent 失败时，应提供“查看配置”“创建 test route”“打开 Doctor”“回滚 patch”。

### 4.4 Runtime

Pain：用户需要知道 d-token runtime、compression layer、integrated RTK 是否真的可用。

核心 UI：

```text
Runtime status:
  d-token runtime: running
  compression layer: available
  integrated RTK: available
  local proxy: 127.0.0.1:xxxxx
  version provenance: available
```

必须展示：

1. runtime 版本。
2. upstream base commit / provenance。
3. integrated RTK 状态。
4. proxy/listening 状态。
5. 最近 health check。
6. 是否为 packaged runtime / external migration / fixture。

DoD：用户能区分 runtime 故障、RTK 故障、route 故障、provider 故障。

### 4.5 Providers & Routes

Pain：用户需要配置官方 provider、第三方自定义 URL、cc-switch、chain bridge，但不能搞混 official/custom。

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

1. Provider 类型：official / custom / cc_switch / local_chain。
2. 协议：OpenAI / Anthropic / auto。
3. base URL。
4. key 状态：configured / missing / invalid / redacted。
5. health check。
6. fallback 策略，默认 disabled。
7. route chain mode：direct / compress_then_provider / compress_then_cc_switch / chain_bridge。

体验要求：

1. 自定义 URL 不得被伪装成 official provider。
2. 任何 fallback 必须显式确认。
3. 401/403/429/5xx 必须映射到可解释错误。
4. Provider route 保存前必须支持 Simulate request path。

### 4.6 Route Simulator：Where will my request go?

Pain：用户最怕“以为走第三方，结果走官方；以为走 cc-switch，结果没走 d-token”。

Route Simulator 可以作为 Providers & Routes 或 Port Discovery 的内嵌功能，也可以是右侧 drawer。

核心 UI：

```text
Simulate request path

Input:
  Agent: OpenCode
  Project: current project
  Route mode: custom provider / cc-switch
  Safety mode: test / production

Output:
  OpenCode → d-token :18180 → compression → cc-switch :xxxx → final provider
  Fallback: disabled
  Credential source: d-token test credential
  Risk: low / medium / high
```

必须展示：

1. input protocol。
2. output protocol。
3. local d-token port。
4. cc-switch/custom provider endpoint。
5. final provider。
6. credential source。
7. fallback policy。
8. risk reason。

DoD：用户保存 route 前必须可以预览请求最终流向。

### 4.7 Port Discovery

Pain：用户不知道本地哪个端口是 d-token，哪个是 OpenCode，哪个可能是 cc-switch，哪个是自定义 provider。

这是一个必须独立存在的页面或 Providers & Routes 下的一级 Tab，不应隐藏在日志里。

核心 UI：

```text
[Auto Detect Ports]

Left: Detected Endpoints
  d-token runtime       127.0.0.1:18180   running   owned by d-token
  OpenCode server       127.0.0.1:4096    running   external
  cc-switch candidate   127.0.0.1:xxxx    candidate needs confirm
  custom provider       127.0.0.1:1337    healthy   OpenAI-compatible

Middle: Route Graph
  OpenCode server :4096
    ↓
  d-token test route :18180
    ↓
  compression runtime
    ↓
  cc-switch candidate :xxxx
    ↓
  final provider

Right: Evidence / Actions
  Evidence:
    - found in OpenCode config baseURL
    - /v1/models responded 200
    - process name matched cc-switch
    - seen in Event Ledger 12 minutes ago

  Actions:
    Rescan
    Confirm as cc-switch
    Confirm as custom provider
    Simulate request path
    Create test route
    Ignore
```

自动识别来源：

1. d-token route registry。
2. Event Ledger 历史 route。
3. ProviderProfile / RouteProfile。
4. OpenCode config / provider baseURL。
5. cc-switch 配置文件或进程参数。
6. 127.0.0.1 端口健康探测。
7. `/v1/models`、`/health`、`/global/health` 等非侵入式 endpoint。

识别规则：

1. 只能默认扫描 loopback 地址。
2. 不默认扫描公网或局域网地址。
3. 不读取完整 prompt / response。
4. 不打印 API key。
5. 识别结果必须分为 confirmed / candidate / unknown。
6. candidate 不得自动接入生产 route。

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
  evidence?: string[];
  last_seen_at?: string;
  notes?: string;
}
```

DoD：用户能一眼看出：哪个端口是谁开的、为什么被识别、是否已确认、请求会不会经过它。

### 4.8 OpenCode Test Runner

Pain：需要用真实 agent 验证 d-token 压缩和路由，而不是只看 fixture UI。

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

Step 5: Export report
  [Export redacted test report]
```

必须展示：

1. d-token test port。
2. OpenCode server port。
3. target provider / cc-switch endpoint。
4. route chain。
5. request 是否进入 d-token。
6. compression 是否执行。
7. provider/cc-switch 是否收到请求。
8. 测试结束是否释放端口。
9. NO_PROXY 检查结果。
10. test report。

体验要求：

1. 本地测试不得复用生产端口。
2. 如果端口占用，自动换端口，不盲目 kill。
3. OpenCode config patch 必须可预览、可回滚。
4. 默认使用临时 HOME / 临时 project config。
5. 检查 `NO_PROXY=localhost,127.0.0.1`，避免本地代理循环。
6. 测试成功后可以选择 Promote test route to production route，但必须二次确认。

测试报告字段：

```ts
interface OpenCodeTestReport {
  report_id: string;
  timestamp: string;
  agent: 'opencode';
  route_kind: 'custom_provider' | 'cc_switch';
  dtoken_test_port: number;
  opencode_server_port?: number;
  target_provider_id?: string;
  cc_switch_endpoint?: string;
  final_provider?: string;
  request_seen_by_dtoken: boolean;
  compression_applied: boolean;
  provider_seen: boolean;
  fallback_occurred: boolean;
  no_proxy_ok: boolean;
  port_released: boolean;
  redacted_logs_path?: string;
}
```

### 4.9 Secrets & Credentials

Pain：provider key、cc-switch key、自定义 header 是高风险信息，不应只藏在 Settings。

核心 UI：

```text
Secrets & Credentials

Provider key status:
  Official Anthropic       configured / redacted / last used 2h ago
  Custom OpenAI endpoint   missing
  cc-switch                configured / health ok

Actions:
  Add key
  Rotate key
  Remove key
  Test connectivity
  View redaction preview
```

必须展示：

1. provider key status。
2. storage location：keychain / credential manager / local encrypted store。
3. last used route。
4. redaction preview。
5. rotate / remove / test connectivity。

红线：永远不在 WebView 明文状态里显示完整 key。

### 4.10 Dashboard

Pain：用户需要看到真实请求、压缩、路由、成本、错误与收益，而不是静态 demo。

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

1. 当前请求链路。
2. active route。
3. token savings 分层。
4. provider latency。
5. cc-switch double-hop latency。
6. fallback 是否发生。
7. OpenCode test route 独立标记。
8. 数据来源：fixture / shadow / managed / test route。

DoD：Dashboard 不得把 fixture 数据伪装成真实数据；必须显示数据来源。

### 4.11 Doctor

Pain：失败时用户需要知道具体错在哪一层，而不是看到泛化报错。

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

1. runtime / integrated RTK。
2. d-token 端口。
3. OpenCode server 端口。
4. cc-switch candidate 端口。
5. custom provider URL。
6. route decision。
7. provider auth。
8. no silent fallback。
9. Windows shim / macOS quarantine。
10. Tauri capability / IPC 安全边界。
11. config ownership。
12. rollback availability。

Recovery：每个 red/warning 项必须给出修复入口，例如 Port Discovery、Secrets、Config Diff、Backup & Rollback。

### 4.12 Config Ownership

Pain：用户需要知道 d-token 到底改了哪些文件、哪些目录、哪些配置区块。

核心 UI：

```text
Config Ownership

Native configs:
  OpenCode config        read-only / temporary patch / managed block
  Claude Code config     managed block
  Codex config           read-only
  cc-switch config       external / candidate

D-token data:
  app data path
  runtime data path
  route registry path
  event ledger path
  backup path

Managed blocks:
  file path
  block name
  last patch
  rollback available
```

必须区分：

```text
Agent native config       原生配置，d-token 只读或受控修改
D-token app data          d-token 自己的数据目录
D-token managed blocks    d-token 受控区块，可 diff / rollback
External configs          cc-switch / OpenCode / Claude / Codex 外部配置
```

DoD：用户能明确知道哪些文件属于 d-token，哪些文件只是被 d-token 读取或受控 patch。

### 4.13 Config Diff

Pain：任何真实写配置前，用户必须知道会改什么。

必须展示：

1. 文件路径。
2. 修改原因。
3. before / after。
4. 风险等级。
5. rollback point。
6. managed block 标记。

OpenCode 测试模式中，优先使用临时配置；如必须 patch 用户配置，必须二次确认。

### 4.14 Backup & Rollback

Pain：用户敢点安装，因为知道能恢复。

必须展示：

1. 最近 rollback points。
2. 影响文件。
3. 影响端口 / 进程。
4. 是否可自动恢复。
5. 恢复后 Doctor 结果。
6. rollback diff。

DoD：任何 Managed Install 都必须产生 rollback point 或明确说明为何不需要。

### 4.15 Maintenance

Pain：用户需要修复残留端口、旧 route、坏 registry、半失败安装，但不能误杀进程或破坏环境。

核心 UI：

```text
Maintenance

Actions:
  Restart d-token-owned runtime
  Release d-token-owned test ports
  Rebuild route registry
  Clean stale test routes
  Repair managed config blocks
  Export redacted diagnostic package
```

安全规则：

1. 只能默认处理 d-token-owned process / port / file。
2. 外部进程必须先显示身份和风险，再由用户确认。
3. 不允许默认 force kill 未验证进程。
4. 所有维修操作进入 Event Ledger。

### 4.16 Logs

Pain：用户需要可解释日志，而不是泄露隐私的原始日志。

必须支持：

1. stage filter：install / route / compression / provider / cc-switch / OpenCode test。
2. request id filter。
3. redacted view 默认开启。
4. export redacted report。

禁止默认展示：

1. 完整 prompt。
2. 完整 response。
3. 完整代码文件。
4. API key / Authorization。

### 4.17 Compatibility Matrix

Pain：用户需要知道自己的 OS、shell、agent、provider route 是否被支持。

必须包含：

1. macOS。
2. Windows native。
3. Windows WSL。
4. Claude Code。
5. Codex CLI。
6. OpenCode。
7. custom provider route。
8. cc-switch route。
9. OpenCode test port support。
10. Windows `.cmd` shim。
11. macOS xattr/quarantine。

### 4.18 Updates & Runtime Version

Pain：用户需要知道 App、runtime、Headroom-derived base、integrated RTK 的版本和可回滚状态。

核心 UI：

```text
Updates & Runtime Version

App version
Runtime version
Headroom-derived base commit
Integrated RTK version
Update channel
Release notes
Rollback previous version
```

必须展示：

1. app version。
2. runtime version。
3. Headroom-derived base commit。
4. integrated RTK version。
5. update channel。
6. release notes。
7. previous rollback point。

### 4.19 Settings

Pain：全局偏好和安全边界集中管理。

必须包含：

1. 语言：中文 / English。
2. 默认模式：Fixture / Shadow / Managed。
3. 默认端口范围。
4. 是否启用端口自动识别。
5. 是否允许非 loopback：默认 false。
6. 日志脱敏级别。
7. 密钥存储方式。
8. 更新通道。
9. telemetry：默认 off，且只允许本地聚合状态。

## 5. 状态设计

每个页面至少必须有：

```text
Empty State
Loading State
Success State
Warning State
Error State
Recovery State
```

示例：Port Discovery

```text
Empty: 尚未扫描端口，点击 Rescan。
Loading: 正在扫描 loopback 端口，不扫描公网或局域网。
Success: 发现 5 个端口，2 个 confirmed，2 个 candidate，1 个 unknown。
Warning: 发现 cc-switch candidate，但尚未确认，不能进入生产 route。
Error: 扫描失败，权限不足或端口探测超时。
Recovery: 打开 Doctor 或手动添加 endpoint。
```

## 6. 页面实现优先级

### P0 实现顺序

```text
1. Home
2. Setup Wizard stepper
3. Providers & Routes
4. Port Discovery
5. OpenCode Test Runner
6. Dashboard route chain
7. Doctor route/port diagnosis
8. Config Diff
9. Backup & Rollback
```

### P1 实现顺序

```text
1. Agents
2. Runtime
3. Secrets & Credentials
4. Config Ownership
5. Maintenance
6. Compatibility Matrix
7. Settings
```

### P2 实现顺序

```text
1. Historical Analytics
2. Advanced Logs
3. Updates polish
4. Theme / layout polish
```

## 7. 页面验收标准

1. 用户能在 Home 看到 runtime、active route、端口、Doctor、今日请求和错误总览。
2. 用户能通过 Setup Wizard 完成 scan → plan → diff → apply → verify → rollback point。
3. 用户能在 GUI 中看到 d-token、OpenCode、cc-switch、自定义 provider 的端口关系。
4. 用户能自动识别 cc-switch candidate，但必须手动确认后才能用于生产 route。
5. 用户能自动识别自定义 provider port，并看到协议猜测与健康检查。
6. 用户能用 Route Simulator 预览请求最终去向。
7. 用户能通过 OpenCode Test Runner 新开测试端口完成 custom provider route smoke。
8. 用户能通过 OpenCode Test Runner 新开测试端口完成 cc-switch route smoke。
9. Dashboard 能展示 agent → runtime → compression → route → target 的完整链路。
10. Doctor 能解释端口占用、route mismatch、provider 401、cc-switch unreachable、OpenCode 未指向 test port。
11. 用户能在 Config Ownership 中看到 d-token 自有数据目录、agent 原生配置、managed blocks、backup path。
12. 所有真实配置修改都有 diff、backup、rollback。
13. 所有敏感信息默认脱敏。
14. candidate 端口不自动进入生产 route。
15. test route 不污染 production route。
16. 所有页面都支持中文优先显示。
