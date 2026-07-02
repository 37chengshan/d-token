# d-token Living PRD

状态：Living Document  
唯一规则：本文件是 d-token 的唯一 PRD。后续任何产品目标、边界、技术路线、目录结构调整，都直接修改本文件，不再另起 v0.1/v0.2/v0.3 多版本 PRD。

## 0. 当前基线决策：d-token App + Headroom-derived Runtime + Integrated RTK

### 0.1 核心判断

`headroomlabs-ai/headroom` 已经包含并管理 RTK：Headroom wrap 链路会确保 RTK binary、注入 RTK instructions、启动 proxy 与 agent，并通过 `rtk gain --format json` 汇总 RTK 指标。因此 d-token 不再把 RTK 与 Headroom 视为两个默认平级、默认独立安装的外部系统。

新的产品基线是：

```text
第一版：d-token = Headroom 的可视化增强版 + 路由增强版
第二版及以后：d-token 逐步吸收、改造、创新 Headroom 核心能力，形成 d-token 自有 runtime

Headroom-derived runtime = compression core + proxy / SDK / MCP + agent wrap / unwrap + integrated RTK
d-token App              = Windows/macOS 图形化安装器、控制面、Provider/Route/cc-switch 编排、Doctor、Dashboard、可观测与回滚层
RTK                      = runtime 内部命令执行层能力，不作为用户侧独立产品路径暴露
```

### 0.2 最终用户安装模型

最终用户不应被要求先手动安装 Headroom 或 RTK。目标体验是：

```text
下载 d-token App
  ↓
打开图形化安装向导
  ↓
选择 agent / provider / cc-switch / 压缩策略
  ↓
d-token 自动安装或启用内置 runtime
  ↓
启动服务并在 Dashboard 看到链路、压缩、路由、错误和成本
```

开发期仍允许连接外部 Headroom 做 no-touch 验证；但成品视角下，Headroom 能力应被打包、吸收、修改并作为 d-token runtime 的组成部分提供。

### 0.3 不再保留 Standalone RTK 产品路径

1. d-token 不提供用户侧 Standalone RTK 模式。
2. d-token 不默认单独安装 RTK。
3. d-token 不要求用户理解或选择 RTK。
4. RTK 只作为 Headroom-derived runtime 内部能力出现。
5. Doctor 可以检测 standalone RTK 冲突，但只是为了避免重复 hook / PATH 污染，不作为推荐路径。
6. d-token 不把 RTK 放进 proxy hot path 当作 tool-result compressor。

### 0.4 产品创新点

1. 把 Headroom 的命令行安装、wrap、proxy、dashboard、doctor、learn、RTK 状态、provider 配置，做成 Windows/macOS 可视化 App。
2. 用户可以图形化选择安装/接入哪些 agent：Claude Code、Codex、Cursor、Cline、OpenCode、Aider、Continue、Goose、OpenHands、Copilot CLI 等。
3. 用户可以图形化选择官方上游、自定义第三方 OpenAI-compatible / Anthropic-compatible 上游、cc-switch / chain bridge 路由。
4. 支持直接压缩后发到自定义 URL：官方 URL、第三方中转 URL、本地 provider URL。
5. 支持需要二次路由的场景：先压缩，再转发到 cc-switch，再由 cc-switch 路由到最终 provider。
6. 典型场景：Claude Code 中使用 OpenCode Go 订阅，需要 cc-switch 路由；d-token 负责先压缩、再转发一次到 cc-switch。
7. 支持 Claude Code/Codex 等 agent 的协议差异、provider 差异、路由差异可视化，避免请求误走官方上游。
8. 支持中文界面和中文日志场景，Windows 上 UTF-8、盘符路径、项目名连字符、`.cmd` shim 等必须作为一等兼容目标。
9. 第一版做可视化增强 + 路由增强；第二版开始逐步吸收和创新 compression/runtime 核心。

## 1. 项目一句话

`d-token` 是一个面向 AI Coding 的本地优先桌面控制面与运行时：在 Windows/macOS 上把 Headroom-derived compression runtime、integrated RTK、agent wrap、provider route、cc-switch / chain bridge、project compression policy、Doctor、Dashboard、backup/rollback 与端到端 token savings attribution 合并成一个用户下载即用、可安装、可观测、可回滚的图形化 App。

## 2. 用户目标

目标用户包括：

1. 使用 Claude Code、Codex、Cursor、Cline、OpenCode 等 AI Coding 工具的开发者。
2. 希望减少 token 浪费、压缩日志/文件/tool output/命令输出的重度 AI Coding 用户。
3. 同时使用官方 provider 与第三方中转 provider 的用户。
4. 需要 cc-switch 或多段路由的用户。
5. 想使用 OpenCode Go、第三方中转、自定义 URL，但又需要 Claude/Codex 等 agent 兼容的人。
6. 不想手动配置 Headroom/RTK/hook/base URL/proxy/dashboard 的普通用户。
7. 在 Windows 上使用 Claude/Codex/Headroom/RTK 时遇到编码、路径、端口、shim 问题的用户。

用户最终应能完成：

```text
打开 d-token 桌面 App
  ↓
选择傻瓜模式或高级模式
  ↓
选择 agent
  ↓
选择压缩能力与项目策略
  ↓
选择 provider：官方 / 第三方自定义 URL / cc-switch / chain bridge
  ↓
预览 diff / 风险 / 回滚点
  ↓
一键安装或启用内置 runtime
  ↓
启动服务
  ↓
在 Dashboard 看到请求、压缩、RTK、路由、成本、错误和恢复建议
```

## 3. 开发期 no-touch 与成品 managed install

### 3.1 两条线必须分开

```text
开发测试线：No-touch Dev Mode
  目标：保护开发者本机已经部署好、正在使用的 agents / Headroom / RTK / cc-switch
  原则：不调用真实工具、不读写真实配置、不改 hook、不占真实端口、不启动/停止真实服务

成品安装线：Production Managed Install
  目标：让最终用户一键安装 / 接入 / 配置 / 升级 / 卸载 d-token App 与内置 runtime
  原则：可以写配置、装工具、改 hook、启动服务，但必须有事务、备份、校验、回滚、可解释日志
```

### 3.2 开发期禁止行为

开发期默认禁止：

1. 重新安装 Headroom。
2. 重新安装 RTK。
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

## 4. 产品分层与 Ownership

### 4.1 Headroom-derived runtime owns

d-token 将直接吸收和修改 Headroom 能力，形成 Headroom-derived runtime。该 runtime 负责：

1. Context compression pipeline。
2. ContentRouter / CacheAligner / SmartCrusher / CodeCompressor / Kompress text compressor。
3. CCR reversible compression 与 local retrieval。
4. Local proxy / SDK / MCP server。
5. Agent wrap / unwrap。
6. Cross-agent memory、per-project memory、SharedContext。
7. learn / verbosity / output shaper。
8. Metrics、stats、history、dashboard primitives。
9. Integrated RTK install / hook / wrap-side metrics bridge。
10. Windows/macOS 兼容性补丁：UTF-8、路径、shim、端口、进程身份。

### 4.2 Integrated RTK owns

RTK 作为 runtime 内部能力负责：

1. command rewrite。
2. command execution shim / hook。
3. command-specific filtering。
4. raw-output tee recovery。
5. command-output savings analytics。
6. missed savings opportunities。
7. session adoption statistics。

RTK 不作为用户可选的独立产品路径出现。

### 4.3 d-token App owns

1. Windows/macOS desktop app。
2. 图形化安装向导与工具选择。
3. 傻瓜模式与高级模式。
4. 内置 runtime 的安装、启用、升级、停止、卸载、回滚。
5. ProviderProfile / RouteProfile / InstanceProfile / ProjectProfile。
6. 官方 provider、第三方 provider、自定义 URL、cc-switch、chain bridge 路由。
7. Anthropic-compatible 与 OpenAI-compatible 协议注入、转换与审计。
8. ProjectCompressionPolicy 与推荐理由。
9. Dashboard、Doctor、Logs、Export、Backup、Rollback。
10. Event Ledger 与 layered savings attribution normalization。
11. Windows/macOS 兼容性诊断与修复提示。
12. 中文 / 英文 i18n。
13. 安全边界：本地优先、默认 127.0.0.1、密钥脱敏、无静默 fallback。

## 5. 最终产品形态

### 5.1 直接压缩到官方或第三方自定义 URL

```text
AI Coding Tool
  ↓
d-token managed agent wrap
  ↓
Integrated RTK command rewrite / shell output reduction
  ↓
d-token Headroom-derived compression runtime
  ↓
d-token route adapter
  ↓
Official Provider / Custom OpenAI-compatible URL / Custom Anthropic-compatible URL
  ↓
Final upstream provider
  ↓
d-token Event Ledger / Dashboard / Doctor / GUI
```

适用场景：

1. 官方 OpenAI / Anthropic。
2. 第三方 OpenAI-compatible 中转。
3. 第三方 Anthropic-compatible 中转。
4. 本地模型服务。
5. 用户自己提供的 base URL。

### 5.2 先压缩，再转发到 cc-switch

当用户在 Claude Code 等 Anthropic-shaped agent 中使用 OpenAI-format URL、OpenCode Go 订阅、cc-switch 或多上游中转时，d-token 必须支持二次转发：

```text
Claude Code / Anthropic-compatible client
  ↓
d-token local entry
  ↓
d-token Headroom-derived compression runtime
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
7. 用户不需要理解底层两次转发，只需要在图形界面选择“使用 cc-switch 路由”。

### 5.3 未来自研替代路径

第二版开始，d-token 可以逐步将 Headroom-derived runtime 中的关键能力改造为 d-token-native runtime：

```text
Phase 1: package and modify Headroom-derived runtime
Phase 2: replace fragile CLI/config/dashboard pieces with d-token-native Rust/TS modules
Phase 3: replace selected compression/router/observability core with d-token-native implementation
Phase 4: keep compatibility with Headroom concepts, but user-facing runtime becomes d-token-owned
```

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

通过 `support/fake-bin` 提供假的 `headroom` / `rtk` / `cc-switch`，测试 command runner、stdout/stderr、exit code、Windows `.cmd` shim、error boundary。

必须使用临时 HOME、临时 PATH、临时端口。

### 6.3 Shadow Mode：开发者主动只读检测

只读检测真实环境，例如 existing Headroom、RTK binary provenance、cc-switch health endpoint、provider health。

Shadow Mode 必须由开发者主动开启，不能作为默认行为。

### 6.4 External Mode：连接已有安装，仅用于开发/迁移

适合已经手动装好 Headroom / RTK / cc-switch 的开发者或早期用户。d-token 只做检测、状态展示、analytics import、Dashboard、Doctor 和 route/project 管理。

External Mode 是迁移辅助模式，不是最终用户主路径。

### 6.5 Managed Mode：成品默认路径

适合新用户或希望 d-token 托管工具生命周期的用户。

默认 Managed Mode 安装顺序：

1. 安装/启用 d-token desktop。
2. 安装/启用 d-token Headroom-derived runtime。
3. 通过 runtime 内部路径启用 integrated RTK。
4. 可选安装/连接 cc-switch。
5. 图形化配置 agent wrap。
6. 图形化配置 provider routes。
7. 图形化配置 project compression policy。
8. 生成 rollback point。

### 6.6 Built-in Fallback Mode

当完整 runtime 不可用时，d-token 可提供基础 CLI output preprocessor：ANSI 清理、空行压缩、重复行折叠、超长输出截断、error-only mode。

该模式只能作为 emergency fallback，不声称等价于完整 runtime。

## 7. Integrated RTK 适配范围

Standalone RTK 不作为用户路径保留。d-token 必须适配 integrated RTK：

1. runtime 内部 RTK binary 是否存在。
2. RTK binary 来源、版本、校验状态。
3. agent wrap 是否启用了 RTK instructions。
4. runtime metrics 中 RTK invocation / savings 是否可见。
5. wrap session 结束后的 RTK savings summary。
6. Agent instruction 文件中 d-token/runtime/RTK block 的 diff、backup、rollback。
7. Windows native / WSL / macOS 下 hook 支持等级。
8. 是否存在外部 standalone RTK 冲突、重复 PATH、重复 hook。
9. 冲突只做提示和修复建议，不把 standalone RTK 暴露成推荐选项。

## 8. Headroom-derived runtime 适配范围

必须保留、吸收并逐步改造：Library / SDK、Local proxy、Agent wrap / unwrap、MCP server、Content-aware compression pipeline、ContentRouter、CacheAligner、SmartCrusher、CodeCompressor、Kompress/text compressor、CCR reversible compression、local retrieval、Cross-agent memory、Per-project memory、SharedContext、learn、output shaper、verbosity steering、effort routing、durable savings ledger、metrics endpoints、Historical stats、JSON/CSV export、Cost tracking、Per-model/per-agent/per-project breakdown、Doctor/perf/benchmark/troubleshooting、integrated RTK。

开发期适配原则：不读真实 CCR 原文、不改真实 Headroom config、不占真实端口。  
成品适配原则：通过 d-token App 安装、启动、配置、更新和卸载，但必须可回滚。

## 8.1 Agent 覆盖要求

Agent matrix 以 d-token managed agent wrap 支持为主。

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

1. d-token managed wrap 是否支持。
2. integrated RTK 是否生效。
3. Agent 配置方式：wrap、manual proxy、plugin、rules、instructions、library only、memory/learn only。
4. 官方 provider 配置状态。
5. 第三方 provider 配置状态。
6. 自定义 URL 配置状态。
7. cc-switch route 配置状态。
8. macOS 支持等级。
9. Windows native 支持等级。
10. Windows WSL 支持等级。
11. 限制说明与修复建议。

## 8.2 Provider / Route / Protocol 要求

Headroom-derived runtime 能通过 proxy / SDK / middleware 接入 OpenAI-compatible 与 Anthropic-compatible 流量，d-token 必须把协议级接入作为一等能力，而不是只按具体 agent 列表处理。

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
  runtime_instance_id?: string;
  target_provider_id: string;
  cc_switch_provider_id?: string;
  chain_mode: 'direct' | 'compress_then_provider' | 'compress_then_cc_switch' | 'chain_bridge';
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
8. 用户可以选择“压缩后直连自定义 URL”或“压缩后转发到 cc-switch”。

## 9. Headroom-derived 项目级压缩策略

### 9.1 ProjectProfile

```ts
interface ProjectProfile {
  id: string;
  name: string;
  root_path: string;
  language_stack: string[];
  agents: string[];
  default_instance_id?: string;
  compression_policy_id?: string;
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
  enable_integrated_rtk: boolean;
  output_verbosity: 'auto' | 'terse' | 'normal' | 'detailed';
  holdout_ratio?: number;
  notes?: string;
}
```

### 9.3 CompressionRecommendationEngine

推荐等级必须基于：项目语言栈、最近请求 token 规模、命令输出噪音比例、runtime 历史 savings、integrated RTK 历史 savings、LLM 回答质量反馈、retrieve 调用频率、失败率与回退率、用户手动覆盖偏好、高风险任务标记、provider/cc-switch 延迟与错误率。

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
  → runtime_input_tokens
  → runtime_final_tokens
  → provider_input_tokens

rtk_savings = raw_cli_tokens - rtk_cleaned_tokens
runtime_savings = runtime_input_tokens - runtime_final_tokens
route_overhead = provider_input_tokens - runtime_final_tokens
end_to_end_savings = raw_total_context_tokens - final_llm_input_tokens
```

禁止把 `rtk_savings + runtime_savings` 直接相加当作总 savings，除非 denominator 和 baseline 已经统一。

### 10.1 Runtime 数据来源

1. RTK savings 来自 d-token runtime metrics 或 integrated RTK metrics。
2. Context savings 来自 d-token runtime stats/history/metrics。
3. d-token 按 project/agent/route/provider 拆分展示。
4. d-token 不直接轮询 standalone `rtk gain` 作为产品路径。

### 10.2 Provider / cc-switch 指标

Dashboard 必须展示：

1. route decision count。
2. provider request count。
3. provider 401/403/429/5xx。
4. cc-switch double-hop latency。
5. compression latency。
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
  stage: 'rtk_rewrite' | 'rtk_filter' | 'agent_wrap' | 'runtime_route' | 'runtime_compress' | 'runtime_retrieve' | 'dtoken_route_decision' | 'protocol_adapt' | 'cc_switch_forward' | 'provider_forward' | 'install_step' | 'config_patch';
  source: 'rtk' | 'runtime' | 'dtoken' | 'cc_switch' | 'provider';
  status: 'ok' | 'passthrough' | 'failed' | 'disabled' | 'skipped';
  input_tokens?: number;
  output_tokens?: number;
  saved_tokens?: number;
  baseline_kind?: 'raw_cli' | 'rtk_cleaned' | 'runtime_input' | 'runtime_final' | 'provider_input' | 'final_llm_input';
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
Headroom-derived packaged runtime
Integrated RTK default runtime ability
Optional cc-switch adapter
SQLite + JSONL event ledger + local JSON/YAML config
```

### 12.2 GUI 必须取代的命令行/网页体验

GUI 必须覆盖并逐步取代普通用户对以下命令或页面的依赖：

1. agent wrap / unwrap。
2. local proxy start / stop / restart。
3. doctor。
4. dashboard。
5. learn / verbosity。
6. RTK savings 聚合查看。
7. provider/base URL 配置。
8. cc-switch route 配置。
9. backup / rollback / diff。

高级用户仍可使用 CLI，但 GUI 是默认主入口。

### 12.3 GUI 页面

必须包含：

1. Welcome / Setup Wizard。
2. 傻瓜模式 / 高级模式选择。
3. Agent Selection。
4. Provider Setup：官方 / 第三方自定义 URL / cc-switch。
5. Route Builder：agent → compression → provider / cc-switch。
6. Runtime Status。
7. Integrated RTK Status。
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
4. d-token runtime/proxy/MCP/stats/CCR summary。
5. Integrated RTK binary/version/hook/metrics。
6. 外部 standalone RTK 冲突检测。
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
3. Dashboard 看不到请求：必须提供 route chain probe，确认 agent 请求是否真的经过 d-token runtime。
4. Output token savings 提示误导：可选功能说明必须和错误提示视觉区分。
5. learn/verbosity 项目上下文不明确：GUI 必须告诉用户需要选择项目、当前目录或 all projects。
6. 仪表盘无中文：d-token GUI 必须支持中文。

### 14.2 Windows 功能性兼容

1. UTF-8 编码：读取 session、JSONL、配置、日志时必须显式 UTF-8，不能依赖 Windows 默认 GBK。
2. 盘符路径：必须正确处理 `D:\work\project`、`D--work-project` 等编码/解码格式。
3. 项目名连字符：不能仅靠 `-` 反推路径；应优先从 session `cwd` 或显式 project root 反查真实路径。
4. npm 安装 CLI shim：Windows CreateProcess 失败后必须尝试 `.cmd` / `.exe` / `.bat` shim，但要记录实际执行路径。

## 15. 安全、隐私与开源合规原则

1. 默认只监听 `127.0.0.1`。
2. API key 只存在本地。
3. 日志默认脱敏。
4. Authorization / Cookie / API key 永不明文展示。
5. 导出日志二次脱敏。
6. RTK/CLI 输出预处理层不得默认记录完整命令输出。
7. d-token 不自动开启 RTK telemetry。
8. d-token 导入 RTK analytics 时默认只读取聚合统计。
9. d-token 不上传 RTK tracking database、tee files、prompt、代码、命令输出到远端。
10. d-token 不默认读取 CCR 原文缓存。
11. 所有 managed binary 必须记录来源与校验信息。
12. 成品覆盖安装必须有 backup / rollback。
13. 开发期测试必须默认 no-touch。
14. 所有非 loopback 绑定必须拒绝或要求强认证与显式确认。
15. 所有 provider route 与 fallback 必须可见、可解释、可审计。
16. 不提供账号托管、账号共享、账号池、转售或代登录能力。
17. 不帮助隐藏真实使用来源，不绕过第三方平台条款。
18. 吸收 Headroom/RTK 代码必须遵守上游开源 license、保留必要版权声明、NOTICE、变更说明与依赖清单。
19. d-token 修改 Headroom-derived runtime 时必须记录 upstream base commit，方便安全审计、升级和回滚。

## 16. CompatibilityMatrix

必须维护 OS / arch / shell / agent / runtime mode / provider route / cc-switch mode 的兼容矩阵：

```ts
interface CompatibilityCase {
  id: string;
  os: 'macos' | 'windows' | 'linux';
  arch: 'x64' | 'arm64';
  shell: 'bash' | 'zsh' | 'powershell' | 'cmd' | 'wsl-bash';
  agent: string;
  runtime_mode: 'fixture' | 'external_headroom_migration' | 'managed_dtoken_runtime' | 'disabled';
  rtk_source: 'integrated' | 'conflicting_external_detected' | 'disabled';
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
code/      所有应用代码、crate、package、runtime、配置样例
support/   测试、脚本、开发辅助、CI 模板、fixture、fake-bin、临时杂项
```

## 18. 里程碑路线图

### M0：文档、研究与路线收敛

1. 单一 living PRD。
2. 技术选型文档。
3. 架构文档。
4. 安全边界文档。
5. Headroom-derived runtime 吸收路线研究。
6. Provider / cc-switch / chain bridge 路由研究。
7. PRD 多维审查报告。
8. 仓库目录收敛。

### M1：No-touch Dev Foundation

目标：开发期完全不破坏本机已有环境。

1. runtime/RTK/cc-switch fixtures。
2. fake runtime / fake rtk / fake cc-switch binary。
3. fixture mode。
4. fake-binary mode。
5. Runtime adapter skeleton。
6. Integrated RTK status skeleton。
7. ProviderRoute skeleton。
8. ProjectPolicy skeleton。
9. ConfigPatcher skeleton，但默认只 plan 不 apply。
10. EventLedger skeleton。
11. local API skeleton。
12. 最小 CI：structure check、cargo check、cargo test、no-touch smoke。

M1 验收：d-token 能在不调用真实 Headroom/RTK/cc-switch、不读取真实 agent 配置、不修改真实环境的前提下完成基础 API 与测试。

### M2：Headroom-derived Runtime Import + External Read-only Integration

目标：把 Headroom 能力作为 d-token runtime 的来源纳入仓库/构建/打包计划，同时保留只读外部连接用于迁移验证。

1. 引入 Headroom-derived runtime 代码或子模块/源码镜像策略。
2. 记录 upstream base commit、license、NOTICE、修改边界。
3. 真实读取 runtime health/stats/history/metrics。
4. 真实检测 integrated RTK binary/version/metrics。
5. 只读解析 existing Headroom wrap / agent 状态。
6. 真实检测 provider route 与 cc-switch health。
7. EventLedger JSONL 持久化。
8. layered savings attribution v0。
9. Doctor 能诊断 runtime/RTK/provider/cc-switch/route/config/platform。

M2 验收：d-token 可以只读连接真实现有安装，也可以在开发环境中运行 Headroom-derived runtime，但不会默认写真实配置或控制真实服务。

### M3：GUI Control Plane MVP

目标：开始图形化，但 GUI 开发默认 fixture，不破坏开发者本机环境。

1. Tauri shell。
2. React Dashboard。
3. Fixture Demo Mode 默认入口。
4. 傻瓜模式 / 高级模式 UI。
5. Agent selection UI。
6. Provider setup UI。
7. 自定义 URL UI。
8. cc-switch route UI。
9. Runtime Status cards。
10. Integrated RTK Status cards。
11. Layered savings cards。
12. Project compression policy cards。
13. Doctor UI。
14. Compatibility warning cards。
15. Config diff / backup / rollback UI。
16. 中文 / English i18n。

M3 验收：开发期 GUI 不碰真实环境；用户主动 shadow/external 时只读连接。

### M4：Production Managed Installer

目标：成品支持一键安装、图形化配置和可回滚。用户只需要下载 d-token App，不需要先安装 Headroom 或 RTK。

1. InstallationTransaction。
2. Tool selection wizard。
3. Managed d-token runtime install/update/uninstall。
4. Integrated RTK install/update/uninstall through runtime path。
5. d-token desktop/proxy/route adapter install/update/uninstall。
6. Optional cc-switch install/connect/configure。
7. Agent wrap setup。
8. Shell profile / agent config patch。
9. Provider route patch。
10. Config diff / backup / apply / rollback。
11. Install Doctor。
12. rollback point。
13. version provenance。
14. Windows UTF-8/path/shim compatibility checks。

M4 验收：成品可以覆盖安装和配置，但所有写操作都可追踪、可备份、可回滚；用户不需要独立安装 Headroom/RTK。

### M5：Production Hardening + Release

目标：正式发布。

1. Windows installer。
2. macOS bundle。
3. updater。
4. signing / notarization。
5. benchmark / evals。
6. crash recovery。
7. Integrated RTK compatibility matrix。
8. Provider / cc-switch E2E compatibility suite。
9. runtime install/uninstall rollback。
10. privacy/export/redaction audit。
11. upstream license/NOTICE audit。
12. CI gate：structure check、cargo check、cargo test、no-touch smoke、desktop build。
13. E2E compatibility test suite。
14. User guide / developer guide / troubleshooting guide。

M5 验收：可以放心让外部用户下载、覆盖安装、回滚、升级和排障。

### M6：d-token-native Runtime Innovation

目标：在保持用户体验稳定的前提下，逐步替代 Headroom-derived runtime 中的关键模块。

1. 替换脆弱 CLI/config/dashboard glue。
2. Rust-native route adapter。
3. d-token-native observability ledger。
4. d-token-native provider protocol adapter。
5. d-token-native compression policy engine。
6. 保留必要的 Headroom-compatible API 和迁移层。

M6 验收：用户仍然只下载 d-token App；内部 runtime 可以逐步从 Headroom-derived 迁移为 d-token-native。

## 19. 验收标准

1. 仓库只有一个 PRD：`docs/PRD.md`。
2. 所有正式文档都在 `docs/`。
3. 所有正式代码都在 `code/`。
4. 所有测试、脚本、fixture、fake-bin 都在 `support/`。
5. 开发期默认 no-touch，不破坏开发者本机已有 agents/RTK/Headroom/cc-switch。
6. 成品支持覆盖安装，但必须有事务、备份、校验和回滚。
7. 用户只需下载安装 d-token App，不应被要求先安装 Headroom 或 RTK。
8. Headroom 功能吸收不能删项，第一版可以增强封装，后续必须支持创新替代。
9. Dashboard 迁移不能缩水，GUI 必须逐步取代普通用户对网页 dashboard 与命令行安装的依赖。
10. 默认 RTK 路径是 integrated RTK，不保留 standalone RTK 用户路径。
11. Dashboard 必须能区分 RTK 命令输出预处理收益、runtime 上下文压缩收益、端到端总收益、provider/cc-switch route overhead。
12. 用户必须能按项目自定义压缩等级，并查看推荐等级与推荐理由。
13. 用户必须能选择官方 provider、第三方自定义 URL、cc-switch 或 chain bridge。
14. 用户必须能图形化选择 agent 并安装/接入对应 wrap/config。
15. 所有 agent/runtime/provider/cc-switch/shell 配置修改必须支持 diff、backup、rollback。
16. CI 必须至少覆盖 structure check、cargo check、cargo test、no-touch smoke、desktop build。
17. M3 GUI 开发默认 fixture，不允许默认扫描或修改真实环境。
18. M4/M5 成品安装器允许覆盖安装，但必须可追踪、可解释、可恢复。
19. Windows UTF-8、盘符路径、项目名连字符、`.cmd` shim 必须进入兼容性测试。
20. 不允许请求因为配置缺失静默走官方 provider；route decision 必须可见、可解释、可审计。
21. d-token 吸收/修改 Headroom/RTK 代码必须保留开源合规记录、upstream base commit、NOTICE 与变更说明。
