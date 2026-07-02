# d-token Living PRD

状态：Living Document  
唯一规则：本文件是 d-token 的唯一 PRD。后续任何产品目标、边界、技术路线、目录结构调整，都直接修改本文件，不再另起 v0.1/v0.2/v0.3 多版本 PRD。

## 0. 当前基线决策：Headroom-managed RTK + 桌面控制面

### 0.1 核心判断

`headroomlabs-ai/headroom` 已经包含并管理 RTK：Headroom wrap 链路会确保 RTK binary、注入 RTK instructions、启动 proxy 与 agent，并通过 `rtk gain --format json` 汇总 RTK 指标。因此 d-token 不再把 RTK 与 Headroom 视为两个默认平级、默认独立安装的外部系统。

新的产品基线是：

```text
Headroom = 上下文压缩核心 + proxy / SDK / MCP + agent wrap / unwrap + Headroom-managed RTK
RTK      = Headroom wrap 链路中的命令执行层能力；同时保留 standalone 兼容路径
d-token  = Headroom + RTK 工作流的 Windows/macOS 图形化控制面、安装事务、Provider/Route/cc-switch 编排、Doctor、Dashboard 与可观测层
```

### 0.2 不再重复造轮子

1. d-token 不重新实现 Headroom compression core。
2. d-token 不默认绕过 Headroom wrap 自己安装 RTK。
3. d-token 不默认直接调用 bare `rtk` 或 bare `headroom`。
4. d-token 不把 RTK 放进 proxy hot path 当作 tool-result compressor。
5. d-token 的默认路径必须尊重 Headroom 的 RTK wrap-side 架构。
6. Standalone RTK 只作为兼容/高级模式存在。

### 0.3 产品创新点

1. 把 Headroom 的命令行安装、wrap、proxy、dashboard、doctor、learn、RTK 状态、provider 配置，做成 Windows/macOS 可视化 App。
2. 用户可以图形化选择安装/接入哪些 agent：Claude Code、Codex、Cursor、Cline、OpenCode、Aider、Continue、Goose、OpenHands、Copilot CLI 等。
3. 用户可以图形化选择官方上游、自定义第三方 OpenAI-compatible / Anthropic-compatible 上游、cc-switch / chain bridge 路由。
4. 支持“先压缩，再转发”：Headroom 压缩后，再由 d-token route adapter 转发到官方 provider、第三方 provider、cc-switch 或本地 chain bridge。
5. 支持 Claude Code/Codex 等 agent 的协议差异、provider 差异、路由差异可视化，避免请求误走官方上游。
6. 支持中文界面和中文日志场景，Windows 上 UTF-8、盘符路径、项目名连字符、`.cmd` shim 等必须作为一等兼容目标。

## 1. 项目一句话

`d-token` 是一个面向 AI Coding 的本地优先图形化控制面：在 Windows/macOS 上把 Headroom、Headroom-managed RTK、agent wrap、provider route、cc-switch / chain bridge、project compression policy、Doctor、Dashboard、backup/rollback 与端到端 token savings attribution 合并成一个可安装、可观测、可回滚的桌面 App。

## 2. 用户目标

目标用户包括：

1. 使用 Claude Code、Codex、Cursor、Cline、OpenCode 等 AI Coding 工具的开发者。
2. 希望减少 token 浪费、压缩日志/文件/tool output/命令输出的重度 AI Coding 用户。
3. 同时使用官方 provider 与第三方中转 provider 的用户。
4. 需要 cc-switch 或多段路由的用户。
5. 不想手动配置 Headroom/RTK/hook/base URL/proxy/dashboard 的普通用户。
6. 在 Windows 上使用 Claude/Codex/Headroom/RTK 时遇到编码、路径、端口、shim 问题的用户。

用户最终应能完成：

```text
打开 d-token 桌面 App
  ↓
选择 agent
  ↓
选择压缩能力与项目策略
  ↓
选择 provider 或 cc-switch 路由
  ↓
预览 diff / 风险 / 回滚点
  ↓
一键安装或连接已有环境
  ↓
启动服务
  ↓
在 Dashboard 看到请求、压缩、RTK、路由、成本、错误和恢复建议
```

## 3. 开发期 no-touch 与成品 managed install

### 3.1 两条线必须分开

```text
开发测试线：No-touch Dev Mode
  目标：保护开发者本机已经部署好、正在使用的 agents / RTK / Headroom / cc-switch
  原则：不调用真实工具、不读写真实配置、不改 hook、不占真实端口、不启动/停止真实服务

成品安装线：Production Managed Install
  目标：让最终用户一键安装 / 接入 / 配置 / 升级 / 卸载
  原则：可以写配置、装工具、改 hook、启动服务，但必须有事务、备份、校验、回滚、可解释日志
```

### 3.2 开发期禁止行为

开发期默认禁止：

1. 重新安装 RTK。
2. 重新安装 Headroom。
3. 修改已有 agent hook。
4. 修改 shell profile。
5. 修改 RTK config。
6. 修改 Headroom config。
7. 修改 cc-switch config。
8. 占用真实 proxy 端口。
9. 启动、停止、重启真实服务。
10. 读取真实 prompt、代码、CCR 原文、tee 文件、session 原文。

开发期必须通过 fixture、fake binary、临时 HOME、临时 PATH、临时端口、mock provider、sandbox config 来验证 d-token 自身逻辑。

### 3.3 成品安装事务

所有真实写操作必须进入 InstallationTransaction：

```text
scan
  ↓
plan
  ↓
show diff
  ↓
backup
  ↓
confirm
  ↓
apply
  ↓
verify
  ↓
doctor
  ↓
commit rollback point
```

失败时必须：

```text
stop only d-token-owned new processes
  ↓
verify process identity before killing anything
  ↓
restore backups
  ↓
remove partial files
  ↓
report exact failure and recovery action
```

## 4. Ownership 边界

### 4.1 Headroom owns

1. Context compression pipeline。
2. ContentRouter / CacheAligner / SmartCrusher / CodeCompressor / Kompress text compressor。
3. CCR reversible compression 与 local retrieval。
4. Local proxy / SDK / MCP server。
5. Agent wrap / unwrap。
6. Cross-agent memory、per-project memory、SharedContext。
7. `headroom learn` 与 output shaper。
8. Metrics、stats、history、dashboard primitives。
9. Headroom-managed RTK install / hook / wrap-side metrics bridge。

### 4.2 RTK owns

RTK 作为能力来源负责：

1. command rewrite。
2. command execution shim / hook。
3. command-specific filtering。
4. raw-output tee recovery。
5. `rtk gain` token savings analytics。
6. `rtk discover` missed savings opportunities。
7. `rtk session` adoption statistics。
8. standalone RTK mode 下的 local config / tracking database / telemetry state。

### 4.3 d-token owns

1. Windows/macOS desktop app。
2. 图形化安装向导与工具选择。
3. Headroom-managed RTK 默认路径的状态展示与配置编排。
4. Standalone RTK 兼容路径的只读检测与显式高级管理。
5. ProviderProfile / RouteProfile / InstanceProfile / ProjectProfile。
6. 官方 provider、第三方 provider、cc-switch、chain bridge 路由。
7. Anthropic-compatible 与 OpenAI-compatible 协议注入、转换与审计。
8. ProjectCompressionPolicy 与推荐理由。
9. Dashboard、Doctor、Logs、Export、Backup、Rollback。
10. Event Ledger 与 layered savings attribution normalization。
11. Windows/macOS 兼容性诊断与修复提示。
12. 中文 / 英文 i18n。
13. 安全边界：本地优先、默认 127.0.0.1、密钥脱敏、无静默 fallback。

## 5. 最终产品形态

### 5.1 默认路径：Headroom-managed RTK

```text
AI Coding Tool
  ↓
headroom wrap <agent> managed by d-token GUI
  ↓
Headroom-managed RTK command rewrite / shell output reduction
  ↓
Cleaned CLI output enters agent context
  ↓
Headroom proxy / compression core
  ↓
d-token route adapter
  ↓
Official Provider / Custom Provider / cc-switch / Chain Bridge
  ↓
Provider
  ↓
d-token Event Ledger / Dashboard / Doctor / GUI
```

### 5.2 Standalone RTK 兼容路径

```text
AI Coding Tool
  ↓
Existing standalone RTK hook / shim
  ↓
Headroom or d-token route/proxy entry
  ↓
Compression + route decision
  ↓
Provider / cc-switch / Chain Bridge
```

Standalone RTK 只在以下条件下启用：

1. 用户已有独立 RTK 并主动选择 External Standalone RTK。
2. 用户在 Managed Mode 中显式选择 Standalone RTK。
3. Doctor 已确认不会与 Headroom-managed RTK 重复 hook、重复 PATH、重复 tracking。

### 5.3 cc-switch / 双跳路由路径

当用户在 Claude Code 等 Anthropic-shaped agent 中使用 OpenAI-format URL、cc-switch 或多上游中转时，d-token 必须支持双跳：

```text
Claude Code / Anthropic-compatible client
  ↓
d-token local entry / Headroom proxy entry
  ↓
Headroom live-zone compression
  ↓
d-token protocol/route adapter
  ↓
cc-switch OpenAI-compatible or Anthropic-compatible route
  ↓
Final upstream provider
```

要求：

1. 先压缩，再转发。
2. route decision 必须记录 input protocol、output protocol、target provider、cc-switch endpoint。
3. 不允许静默从第三方 provider fallback 到官方 Anthropic/OpenAI。
4. Provider 401 必须显示为 route/provider mismatch，而不是泛化为 token 错误。
5. Header stripping、Authorization 选择、base URL 选择必须可审计。
6. GUI 必须展示“当前请求实际会去哪里”。

## 6. 模式设计

### 6.1 Fixture Mode：开发默认

完全读取 `support/fixtures`，不调用任何真实命令。

用于测试：parser、API response、Dashboard、EventLedger、LayeredSavings、ProjectPolicy、ProviderRoute、cc-switch route plan。

必须保证：

1. 不调用真实 `rtk`。
2. 不调用真实 `headroom`。
3. 不调用真实 cc-switch。
4. 不读真实 agent config。
5. 不改任何真实配置。
6. 只使用测试端口。

### 6.2 Fake-binary Mode：开发命令调用测试

通过 `support/fake-bin` 提供假的 `rtk` / `headroom` / `cc-switch`，测试 command runner、stdout/stderr、exit code、Windows `.cmd` shim、error boundary。

必须使用临时 HOME、临时 PATH、临时端口。

### 6.3 Shadow Mode：开发者主动只读检测

只读检测真实环境，例如 `headroom --version`、Headroom health endpoint、Headroom metrics、RTK binary provenance、cc-switch health endpoint。

Shadow Mode 必须由开发者主动开启，不能作为默认行为。

### 6.4 External Mode：连接已有安装

适合已经手动装好 Headroom / RTK / cc-switch 的用户。d-token 只做检测、状态展示、analytics import、Dashboard、Doctor 和 route/project 管理。

External Mode 不做安装、升级、卸载和 hook rewrite，除非用户主动进入配置修改流程。

### 6.5 Managed Mode：成品一键安装

适合新用户或希望 d-token 托管工具生命周期的用户。

默认 Managed Mode 安装顺序：

1. 安装/校验 Headroom。
2. 通过 Headroom-managed 路径安装/校验 RTK。
3. 安装/校验 d-token desktop / local API / route adapter。
4. 可选安装/连接 cc-switch。
5. 图形化配置 agent wrap。
6. 图形化配置 provider routes。
7. 图形化配置 project compression policy。
8. 生成 rollback point。

Standalone RTK Managed Mode 不是默认项，必须由用户显式选择。

### 6.6 Built-in Fallback Mode

当 Headroom-managed RTK 与 standalone RTK 都不可用时，d-token 可提供基础 CLI output preprocessor：ANSI 清理、空行压缩、重复行折叠、超长输出截断、error-only mode。

该模式只能作为 fallback，不声称等价于完整 RTK 或 Headroom-managed RTK。

## 7. RTK 双路径适配范围

### 7.1 Headroom-managed RTK：默认主路径

d-token 必须优先适配 Headroom-managed RTK：

1. Headroom 是否已安装。
2. Headroom-managed RTK binary 是否存在。
3. RTK binary 来源、版本、校验状态。
4. `headroom wrap <agent>` 是否启用了 RTK instructions。
5. Headroom metrics 中 RTK invocation / savings 是否可见。
6. wrap session 结束后的 RTK savings summary。
7. Agent instruction 文件中 Headroom/RTK block 的 diff、backup、rollback。
8. Windows native / WSL / macOS 下 hook 支持等级。
9. 是否存在重复 RTK binary、重复 PATH、重复 hook。

### 7.2 Standalone RTK：兼容路径

Standalone RTK 适配范围：

1. `rtk --version`。
2. `rtk gain --all --format json`。
3. `rtk discover`。
4. `rtk session`。
5. local config file。
6. local tracking database。
7. raw-output tee recovery。
8. telemetry opt-in state。
9. hook integrity verification。
10. permission precedence：Deny > Ask > Allow > Default。

Standalone RTK 真实调用必须满足：

1. 用户显式开启 standalone mode。
2. 不使用 bare `rtk`；必须使用已解析、已校验、可展示来源的绝对路径。
3. 不与 Headroom-managed RTK 重复安装或重复 hook。
4. Doctor 必须展示冲突风险。

## 8. Headroom 适配范围

必须保留并适配：Library / SDK、Local proxy、Agent wrap / unwrap、MCP server、Content-aware compression pipeline、ContentRouter、CacheAligner、SmartCrusher、CodeCompressor、Kompress/text compressor、CCR reversible compression、local retrieval、Cross-agent memory、Per-project memory、SharedContext、headroom learn、output shaper、verbosity steering、effort routing、durable savings ledger、metrics endpoints、Historical stats、JSON/CSV export、Cost tracking、Per-model/per-agent/per-project breakdown、Doctor/perf/benchmark/troubleshooting、Headroom-managed RTK。

开发期适配原则：不读真实 CCR 原文、不改真实 Headroom config、不占真实端口。  
成品适配原则：可安装、启动、配置、更新和卸载，但必须可回滚。

## 8.1 Agent 覆盖要求

Agent matrix 以 Headroom wrap 支持为主，叠加 standalone RTK 支持和 d-token GUI 支持。

必须覆盖：

```text
Claude Code
Codex CLI
Cursor
Cline / Roo Code
OpenCode
OpenClaw
Aider
Continue
Goose
OpenHands
GitHub Copilot CLI
VS Code Copilot Chat
Gemini CLI
Windsurf
Kilo Code
Google Antigravity
Mistral Vibe
Pi
Hermes
Cortex Code
```

每个 agent 在 GUI、Doctor 与 CompatibilityMatrix 中至少展示：

1. Headroom wrap 是否支持。
2. Headroom-managed RTK 是否支持。
3. Standalone RTK 是否支持。
4. Agent 配置方式：wrap、manual proxy、plugin、rules、instructions、library only、memory/learn only。
5. 官方 provider 配置状态。
6. 第三方 provider 配置状态。
7. cc-switch route 配置状态。
8. macOS 支持等级。
9. Windows native 支持等级。
10. Windows WSL 支持等级。
11. 限制说明与修复建议。

## 8.2 Provider / Route / Protocol 要求

Headroom 能通过 proxy / SDK / middleware 接入 OpenAI-compatible 与 Anthropic-compatible 流量，d-token 必须把协议级接入作为一等能力，而不是只按具体 agent 列表处理。

### Provider 类型

```ts
interface ProviderProfile {
  id: string;
  name: string;
  kind: 'official' | 'custom' | 'cc_switch' | 'local_chain';
  protocol: 'openai' | 'anthropic' | 'auto';
  base_url: string;
  api_key_ref?: string;
  models?: string[];
  timeout_ms: number;
  healthcheck?: string;
  notes?: string;
}
```

### Route 类型

```ts
interface RouteProfile {
  id: string;
  name: string;
  input_protocol: 'openai' | 'anthropic';
  output_protocol: 'openai' | 'anthropic' | 'same_as_input';
  agent_id?: string;
  project_id?: string;
  headroom_instance_id?: string;
  target_provider_id: string;
  cc_switch_provider_id?: string;
  chain_mode: 'direct' | 'headroom_then_provider' | 'headroom_then_cc_switch' | 'chain_bridge';
  compression_policy_id?: string;
  bind_host: '127.0.0.1';
  local_port: number;
  fallback_policy: 'disabled' | 'explicit_only';
}
```

### 路由要求

1. 官方 provider 与第三方 provider 必须显式区分。
2. Claude/Anthropic traffic 不得因为配置缺失自动发往 `api.anthropic.com`。
3. OpenAI-compatible provider 不得伪装成 official OpenAI，必须展示 base URL。
4. cc-switch 必须作为 ProviderProfile 或 ChainBridgeProfile 显式配置。
5. route decision 必须进入 Event Ledger。
6. Dashboard 必须显示当前 agent → compression → route → provider 的完整链路。
7. 所有 fallback 默认禁用；如支持 fallback，必须显式展示并要求用户确认。

## 9. Headroom 项目级压缩策略

### 9.1 ProjectProfile

```ts
interface ProjectProfile {
  id: string;
  name: string;
  root_path: string;
  language_stack: string[];
  agents: string[];
  default_instance_id?: string;
  headroom_policy_id?: string;
  rtk_profile_id?: string;
  provider_route_id?: string;
  cc_switch_route_id?: string;
  created_at: string;
  updated_at: string;
}
```

### 9.2 ProjectCompressionPolicy

```ts
interface ProjectCompressionPolicy {
  id: string;
  project_id: string;
  level: 'off' | 'safe' | 'balanced' | 'aggressive' | 'max';
  recommended_level: 'safe' | 'balanced' | 'aggressive' | 'max';
  recommendation_confidence: 'low' | 'medium' | 'high';
  enable_ccr: boolean;
  enable_memory: boolean;
  enable_learn: boolean;
  enable_mcp: boolean;
  enable_output_shaper: boolean;
  enable_cache_aligner: boolean;
  enable_code_compressor: boolean;
  enable_json_smartcrusher: boolean;
  enable_text_kompress: boolean;
  enable_headroom_managed_rtk: boolean;
  output_verbosity: 'auto' | 'terse' | 'normal' | 'detailed';
  holdout_ratio?: number;
  notes?: string;
}
```

### 9.3 CompressionRecommendationEngine

推荐等级必须基于：项目语言栈、最近请求 token 规模、命令输出噪音比例、Headroom 历史 savings、Headroom-managed RTK 历史 savings、standalone RTK 历史 savings、LLM 回答质量反馈、retrieve 调用频率、失败率与回退率、用户手动覆盖偏好、高风险任务标记、provider/cc-switch 延迟与错误率。

```text
safe:       默认推荐，适合新项目、高风险任务、未知 provider route
balanced:   常规编码任务推荐
aggressive: 大日志、大测试输出、大搜索结果推荐
max:        只对低风险探索/批量日志/RAG chunks 推荐，必须提示可能损失细节
```

## 10. 分层压缩与路由指标模型

```text
raw_cli_tokens
  → rtk_cleaned_tokens
  → headroom_input_tokens
  → headroom_final_tokens
  → provider_input_tokens

rtk_savings = raw_cli_tokens - rtk_cleaned_tokens
headroom_savings = headroom_input_tokens - headroom_final_tokens
route_overhead = provider_input_tokens - headroom_final_tokens
end_to_end_savings = raw_total_context_tokens - final_llm_input_tokens
```

禁止把 `rtk_savings + headroom_savings` 直接相加当作总 savings，除非 denominator 和 baseline 已经统一。

### 10.1 Headroom-managed mode 数据来源

1. RTK savings 优先来自 Headroom wrap metrics 或 Headroom 暴露的 RTK metrics。
2. Headroom context savings 来自 Headroom stats/history/metrics。
3. d-token 只做归一化、展示、按 project/agent/route/provider 拆分。
4. d-token 不默认直接轮询 standalone `rtk gain`。

### 10.2 Standalone mode 数据来源

1. 用户显式选择 standalone RTK 后，d-token 可读取 `rtk gain/discover/session`。
2. 所有命令必须使用已解析绝对路径。
3. 所有读取默认只取聚合统计，不读取完整 tee 原文。

### 10.3 Provider / cc-switch 指标

Dashboard 必须展示：

1. route decision count。
2. provider request count。
3. provider 401/403/429/5xx。
4. cc-switch double-hop latency。
5. Headroom compression latency。
6. route adapter latency。
7. provider final latency。
8. compressed tokens before forwarding。
9. final provider input tokens。
10. fallback 是否发生；默认不允许静默 fallback。

## 11. Event Ledger

```ts
interface DtokenEvent {
  event_id: string;
  timestamp_ms: number;
  project_id?: string;
  agent_id?: string;
  instance_id?: string;
  command_id?: string;
  request_id?: string;
  stage: 'rtk_rewrite' | 'rtk_filter' | 'headroom_wrap' | 'headroom_route' | 'headroom_compress' | 'headroom_retrieve' | 'dtoken_route_decision' | 'protocol_adapt' | 'cc_switch_forward' | 'provider_forward' | 'install_step' | 'config_patch';
  source: 'rtk' | 'headroom' | 'dtoken' | 'cc_switch' | 'provider';
  status: 'ok' | 'passthrough' | 'failed' | 'disabled' | 'skipped';
  input_tokens?: number;
  output_tokens?: number;
  saved_tokens?: number;
  baseline_kind?: 'raw_cli' | 'rtk_cleaned' | 'headroom_input' | 'headroom_final' | 'provider_input' | 'final_llm_input';
  input_protocol?: 'openai' | 'anthropic';
  output_protocol?: 'openai' | 'anthropic';
  provider_id?: string;
  route_id?: string;
  error_code?: string;
  metadata?: Record<string, unknown>;
}
```

开发期 Event Ledger 不保存 full prompt、full response、full file content、full command output。成品默认也不保存，除非用户显式开启本地调试模式。

## 12. Windows/macOS 图形化 App 要求

### 12.1 App 技术路线

```text
Tauri 2 + React + TypeScript + Vite
Rust orchestration core
Headroom external/managed integration
Headroom-managed RTK default integration
Optional standalone RTK adapter
Optional cc-switch adapter
SQLite + JSONL event ledger + local JSON/YAML config
```

### 12.2 GUI 必须取代的命令行/网页体验

GUI 必须覆盖并逐步取代普通用户对以下命令或页面的依赖：

1. `headroom wrap <agent>`。
2. `headroom unwrap <agent>`。
3. `headroom proxy`。
4. `headroom doctor`。
5. `headroom dashboard`。
6. `headroom learn --verbosity`。
7. `rtk gain/discover/session` 聚合查看。
8. provider/base URL 配置。
9. cc-switch route 配置。
10. backup / rollback / diff。

高级用户仍可使用 CLI，但 GUI 是默认主入口。

### 12.3 GUI 页面

必须包含：

1. Welcome / Setup Wizard。
2. Agent Selection。
3. Provider Setup：官方 / 第三方 / cc-switch。
4. Route Builder：agent → compression → provider / cc-switch。
5. Headroom Status。
6. Headroom-managed RTK Status。
7. Standalone RTK Status。
8. Project Policy。
9. Dashboard。
10. Doctor。
11. Logs。
12. Config Diff。
13. Backup / Rollback。
14. Compatibility Matrix。
15. Settings。
16. 中文 / English 切换。

## 13. Doctor 要求

### 13.1 开发期 Doctor

开发期 Doctor 必须支持 no-touch 检查：

1. 当前 profile 是否为 fixture/fake/shadow。
2. 是否误调用真实 binary。
3. 是否使用测试端口。
4. 是否使用临时 HOME。
5. 是否存在外部配置写入风险。
6. 是否误连接真实 Headroom / RTK / cc-switch。

### 13.2 成品 Doctor

成品 Doctor 必须检查：

1. Local proxy。
2. 端口占用与旧进程残留。
3. 进程身份校验：kill 前必须确认是 d-token-owned 或用户确认。
4. Headroom CLI/proxy/MCP/stats/CCR summary。
5. Headroom-managed RTK binary/version/hook/metrics。
6. Standalone RTK binary/version/hook/integrity/conflict。
7. Provider route。
8. cc-switch route。
9. Project policy。
10. Config backup/rollback。
11. Event ledger。
12. Installation transaction。
13. Windows UTF-8、盘符路径、`.cmd` shim。
14. macOS xattr/quarantine/keychain/launch agent。

## 14. 已知问题沉淀为产品要求

以下问题必须转化为 d-token 的 Doctor、GUI、路由和安装验收项。

### 14.1 Provider / Route / Dashboard UX

1. 第三方 provider 401：不得因路由缺失把请求静默发到官方 Anthropic/OpenAI；GUI 必须展示真实 target provider。
2. 重启后端口占用：必须检测旧进程、端口占用、PID 身份，不能盲目启动新服务。
3. Dashboard 看不到请求：必须提供 route chain probe，确认 agent 请求是否真的经过 Headroom/d-token。
4. Output token savings 提示误导：可选功能说明必须和错误提示视觉区分。
5. `headroom learn --verbosity --apply` 项目上下文不明确：GUI 必须告诉用户需要选择项目、当前目录或 `--all`。
6. 仪表盘无中文：d-token GUI 必须支持中文。

### 14.2 Windows 功能性兼容

1. UTF-8 编码：读取 session、JSONL、配置、日志时必须显式 UTF-8，不能依赖 Windows 默认 GBK。
2. 盘符路径：必须正确处理 `D:\work\project`、`D--work-project` 等编码/解码格式。
3. 项目名连字符：不能仅靠 `-` 反推路径；应优先从 session `cwd` 或显式 project root 反查真实路径。
4. npm 安装 CLI shim：Windows CreateProcess 失败后必须尝试 `.cmd` / `.exe` / `.bat` shim，但要记录实际执行路径。

## 15. 安全与隐私原则

1. 默认只监听 `127.0.0.1`。
2. API key 只存在本地。
3. 日志默认脱敏。
4. Authorization / Cookie / API key 永不明文展示。
5. 导出日志二次脱敏。
6. RTK/CLI 输出预处理层不得默认记录完整命令输出。
7. d-token 不自动开启 RTK telemetry。
8. d-token 导入 RTK analytics 时默认只读取聚合统计。
9. d-token 不上传 RTK tracking database、tee files、prompt、代码、命令输出到远端。
10. d-token 不默认读取 Headroom CCR 原文缓存。
11. 所有 managed binary 必须记录来源与校验信息。
12. 成品覆盖安装必须有 backup / rollback。
13. 开发期测试必须默认 no-touch。
14. 所有非 loopback 绑定必须拒绝或要求强认证与显式确认。
15. 所有 provider route 与 fallback 必须可见、可解释、可审计。
16. 不提供账号托管、账号共享、账号池、转售或代登录能力。
17. 不帮助隐藏真实使用来源，不绕过第三方平台条款。

## 16. CompatibilityMatrix

必须维护 OS / arch / shell / agent / Headroom mode / RTK source / provider route / cc-switch mode 的兼容矩阵：

```ts
interface CompatibilityCase {
  id: string;
  os: 'macos' | 'windows' | 'linux';
  arch: 'x64' | 'arm64';
  shell: 'bash' | 'zsh' | 'powershell' | 'cmd' | 'wsl-bash';
  agent: string;
  headroom_mode: 'external' | 'managed' | 'disabled';
  rtk_source: 'headroom_managed' | 'standalone_external' | 'standalone_managed' | 'builtin_fallback' | 'disabled';
  provider_route_mode: 'official' | 'custom_openai' | 'custom_anthropic' | 'cc_switch' | 'chain_bridge';
  expected_hook_support: 'full' | 'partial' | 'none';
  expected_proxy_support: 'full' | 'partial' | 'none';
  status: 'untested' | 'pass' | 'fail' | 'blocked';
  notes?: string;
}
```

Mac 与 Windows 都是首要目标平台。Windows 必须区分 native 与 WSL：native 模式通常只能获得部分 hook/规则能力，WSL 才能获得完整 shell hook 体验。

## 17. 目录结构

仓库根目录允许：

```text
README.md
AGENTS.md
CLAUDE.md
docs/
code/
support/
.github/workflows/
.gitignore
```

目录含义：

```text
README.md  面向 GitHub 访问者的项目入口介绍
docs/      所有正式文档，包含唯一 PRD、架构、技术决策、安全边界、审查报告、研究报告
code/      所有应用代码、crate、package、sidecar、runtime、配置样例
support/   测试、脚本、开发辅助、CI 模板、fixture、fake-bin、临时杂项
```

## 18. 里程碑路线图

### M0：文档、研究与路线收敛

1. 单一 living PRD。
2. 技术选型文档。
3. 架构文档。
4. 安全边界文档。
5. Headroom-managed RTK 架构研究。
6. Provider / cc-switch / chain bridge 路由研究。
7. PRD 多维审查报告。
8. 仓库目录收敛。

### M1：No-touch Dev Foundation

目标：开发期完全不破坏本机已有环境。

1. Headroom/RTK/cc-switch fixtures。
2. fake headroom / fake rtk / fake cc-switch binary。
3. fixture mode。
4. fake-binary mode。
5. Headroom adapter skeleton。
6. Headroom-managed RTK status skeleton。
7. Standalone RTK adapter skeleton。
8. ProviderRoute skeleton。
9. ProjectPolicy skeleton。
10. ConfigPatcher skeleton，但默认只 plan 不 apply。
11. EventLedger skeleton。
12. local API skeleton。
13. 最小 CI：structure check、cargo check、cargo test、no-touch smoke。

M1 验收：d-token 能在不调用真实 Headroom/RTK/cc-switch、不读取真实 agent 配置、不修改真实环境的前提下完成基础 API 与测试。

### M2：External Read-only Integration

目标：在用户主动开启 shadow/external 时，只读接入真实本体。

1. 真实读取 Headroom health/stats/history/metrics。
2. 真实检测 Headroom-managed RTK binary/version/metrics。
3. 只读解析 Headroom wrap / agent 状态。
4. 可选只读解析 standalone `rtk gain/discover/session`。
5. 真实检测 provider route 与 cc-switch health。
6. EventLedger JSONL 持久化。
7. layered savings attribution v0。
8. Doctor 能诊断 Headroom/RTK/provider/cc-switch/route/config/platform。

M2 验收：d-token 可以只读连接真实现有安装，但不会写配置或控制服务。

### M3：GUI Control Plane MVP

目标：开始图形化，但 GUI 开发默认 fixture，不破坏开发者本机环境。

1. Tauri shell。
2. React Dashboard。
3. Fixture Demo Mode 默认入口。
4. Agent selection UI。
5. Provider setup UI。
6. cc-switch route UI。
7. Headroom Status cards。
8. Headroom-managed RTK Status cards。
9. Standalone RTK Status cards。
10. Layered savings cards。
11. Project compression policy cards。
12. Doctor UI。
13. Compatibility warning cards。
14. Config diff / backup / rollback UI。
15. 中文 / English i18n。

M3 验收：开发期 GUI 不碰真实环境；用户主动 shadow/external 时只读连接。

### M4：Production Managed Installer

目标：成品支持一键安装、图形化配置和可回滚。

1. InstallationTransaction。
2. Tool selection wizard。
3. Managed Headroom install/update/uninstall。
4. Headroom-managed RTK install/update/uninstall through Headroom path。
5. Optional standalone RTK install/update/uninstall only when explicitly selected。
6. d-token desktop/proxy/route adapter install/update/uninstall。
7. Optional cc-switch install/connect/configure。
8. Agent wrap setup。
9. Shell profile / agent config patch。
10. Provider route patch。
11. Config diff / backup / apply / rollback。
12. Install Doctor。
13. rollback point。
14. version provenance。
15. Windows UTF-8/path/shim compatibility checks。

M4 验收：成品可以覆盖安装和配置，但所有写操作都可追踪、可备份、可回滚；默认不重复安装 standalone RTK。

### M5：Production Hardening + Release

目标：正式发布。

1. Windows installer。
2. macOS bundle。
3. updater。
4. signing / notarization。
5. benchmark / evals。
6. crash recovery。
7. Headroom-managed RTK compatibility matrix。
8. Standalone RTK compatibility matrix。
9. Provider / cc-switch E2E compatibility suite。
10. Headroom install/uninstall rollback。
11. privacy/export/redaction audit。
12. CI gate：structure check、cargo check、cargo test、no-touch smoke、desktop build。
13. E2E compatibility test suite。
14. User guide / developer guide / troubleshooting guide。

M5 验收：可以放心让外部用户下载、覆盖安装、回滚、升级和排障。

## 19. 验收标准

1. 仓库只有一个 PRD：`docs/PRD.md`。
2. 所有正式文档都在 `docs/`。
3. 所有正式代码都在 `code/`。
4. 所有测试、脚本、fixture、fake-bin 都在 `support/`。
5. 开发期默认 no-touch，不破坏开发者本机已有 agents/RTK/Headroom/cc-switch。
6. 成品支持覆盖安装，但必须有事务、备份、校验和回滚。
7. Headroom 功能迁移不能删项。
8. Dashboard 迁移不能缩水，GUI 必须逐步取代普通用户对网页 dashboard 与命令行安装的依赖。
9. 默认 RTK 路径是 Headroom-managed RTK。
10. Standalone RTK 必须是显式兼容/高级路径。
11. Dashboard 必须能区分 RTK 命令输出预处理收益、Headroom 上下文压缩收益、端到端总收益、provider/cc-switch route overhead。
12. 用户必须能按项目自定义 Headroom 压缩等级，并查看推荐等级与推荐理由。
13. 用户必须能选择官方 provider、第三方 provider、cc-switch 或 chain bridge。
14. 用户必须能图形化选择 agent 并安装/接入对应 wrap/config。
15. 所有 agent/RTK/Headroom/provider/cc-switch/shell 配置修改必须支持 diff、backup、rollback。
16. CI 必须至少覆盖 structure check、cargo check、cargo test、no-touch smoke、desktop build。
17. M3 GUI 开发默认 fixture，不允许默认扫描或修改真实环境。
18. M4/M5 成品安装器允许覆盖安装，但必须可追踪、可解释、可恢复。
19. Windows UTF-8、盘符路径、项目名连字符、`.cmd` shim 必须进入兼容性测试。
20. 不允许请求因为配置缺失静默走官方 provider；route decision 必须可见、可解释、可审计。
