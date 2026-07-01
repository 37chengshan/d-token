# d-token Living PRD

状态：Living Document  
唯一规则：本文件是 d-token 的唯一 PRD。后续任何产品目标、边界、技术路线、目录结构调整，都直接修改本文件，不再另起 v0.1/v0.2/v0.3 多版本 PRD。

## 1. 项目一句话

d-token 是一个面向 AI Coding 的本地分层压缩、路由与桌面观测系统：上游集成 `rtk-ai/rtk` 作为 CLI 命令输出预处理层，下游保留 Headroom-compatible API 接入层压缩能力，并通过 Windows/macOS 桌面 App 统一管理一键安装、provider、route、instance、project compression policy、chain bridge、Dashboard、Doctor 与端到端 token savings。

## 2. 产品定位

### 2.1 核心判断

RTK 与 Headroom-compatible core 工作在不同链路层级：

```text
RTK:      Command Execution Layer / Shell Output Layer
Headroom: API Gateway Layer / LLM Context Layer
```

二者不是竞争关系，而是上下游互补关系。

RTK 负责在命令输出进入 Agent 上下文前做第一层清洗；Headroom-compatible core 负责在完整上下文发往 LLM 前做第二层压缩。d-token 负责把两者统一安装、编排、配置、观测、统计、诊断和桌面化。

### 2.2 最终产品形态

```text
AI Coding Tool
  ↓
Shell Command Execution
  ↓
RTK CLI Proxy / Hook / Plugin
  ↓
Cleaned CLI Output enters Agent Context
  ↓
d-token Local Proxy
  ↓
Headroom-compatible Compression Core
  ↓
Route Decision
  ↓
Official Provider / Custom Provider / Chain Bridge
  ↓
Provider
```

### 2.3 产品目标

1. 保留 Headroom 的完整能力，不做删减版。
2. 完整迁移 Headroom Dashboard 到 Windows/macOS 桌面 App。
3. 将 `rtk-ai/rtk` 作为一等集成对象，而不是泛泛的“CLI 清洗脚本”。
4. 提供一键安装向导：用户可勾选安装 RTK、Headroom-compatible runtime、d-token core、Desktop App、agent hooks、provider configs。
5. 支持 RTK 外部二进制探测、版本探测、安装状态探测、agent hook 状态探测、analytics 数据导入。
6. 支持 Headroom 每项目压缩等级、推荐等级、输出 token shaping、memory/learn/MCP/CCR 开关。
7. 支持 Claude Code、Codex、Cline、OpenCode、Cursor、Windsurf、Gemini CLI、GitHub Copilot、Kilo Code、Antigravity 等 AI Coding 工具接入。
8. 支持官方 provider、第三方 compatible provider、chain bridge 三类路径。
9. 支持多个本地实例并行运行，并在 Dashboard 中按实例拆分数据。
10. 保证所有请求链路可观测：agent、project、instance、provider、route、chain mode、tokens、latency、error、transforms。
11. 保证两层压缩可观测：RTK command-output savings、Headroom context savings、end-to-end savings。
12. 保证 Windows/macOS 用户可以通过图形界面完成安装、配置、启动、停止、诊断、回滚、导出。

## 3. 外部项目学习结论

### 3.1 RTK 是什么

`rtk-ai/rtk` 是一个高性能 CLI proxy，用于在命令输出进入 LLM 上下文前过滤和压缩命令输出。其 README 定位为：减少常见开发命令的 LLM token 消耗，单 Rust binary，面向 AI Coding 场景。

### 3.2 RTK 的关键能力

d-token 必须理解并适配 RTK 的以下能力：

1. Single Rust binary。
2. CLI proxy pattern。
3. Bash hook / agent hook / plugin rewrite。
4. `rtk init` agent-specific setup。
5. `rtk gain` token savings analytics。
6. `rtk discover` missed savings opportunities。
7. `rtk session` adoption statistics。
8. command-specific filters。
9. raw-output tee recovery。
10. local config file。
11. local tracking database。
12. telemetry opt-in state。
13. Windows WSL full support / native Windows limited hook support。

### 3.3 RTK 的过滤策略

d-token Dashboard 和 metrics schema 必须能表达 RTK 的过滤策略：Smart Filtering、Grouping、Truncation、Deduplication、Stats extraction、Error-only filtering、Structure-only filtering、Code filtering levels、Failure focus、Progress filtering、JSON/text dual mode、State-machine parsing、NDJSON streaming parsing。

### 3.4 RTK 的设计原则

d-token 适配 RTK 时必须保留以下工程原则：

1. Single responsibility：每类命令由独立模块处理。
2. Minimal overhead：预处理不能明显拖慢命令执行。
3. Exit code preservation：必须保留原命令退出码，避免破坏 CI/CD、测试、脚本语义。
4. Fail-safe：过滤失败时必须回退到原始输出或明确提示，而不是吞掉内容。
5. Transparent：用户必须能通过 verbose/raw-output/tee 机制看到原始输出。
6. Local-first：本地命令输出、本地统计、本地恢复文件优先。

### 3.5 Headroom 是什么

Headroom 是 context compression layer for AI agents，目标是在内容到达 LLM 前压缩 tool outputs、logs、RAG chunks、files、conversation history，并提供 library、proxy、agent wrap、MCP server、CCR、cross-agent memory、learn、output token reduction、dashboard、doctor、perf、metrics 等能力。

### 3.6 Headroom 的关键能力

d-token 必须保留：Library / SDK、Local proxy、Agent wrap / unwrap、MCP server、Content-aware compression pipeline、ContentRouter、CacheAligner、SmartCrusher、CodeCompressor、Kompress/text compressor、CCR reversible compression、local retrieval、Cross-agent memory、Per-project memory、SharedContext、headroom learn、output shaper、verbosity steering、effort routing、durable savings ledger、metrics endpoints、Historical stats、JSON/CSV export、Cost tracking、Per-model/per-agent/per-project breakdown、Doctor/perf/benchmark/troubleshooting。

## 4. 明确边界

### 4.1 必须做

1. Headroom-compatible core。
2. Full Dashboard migration。
3. RTK integration layer。
4. One-click installer / setup wizard。
5. Selective tool installation：用户可选择只安装 d-token、RTK、Headroom runtime 或任意组合。
6. RTK binary detection / version detection / hook status detection / analytics import。
7. Headroom binary/runtime detection / proxy status / wrap status / MCP status / dashboard metrics import。
8. ProjectProfile。
9. ProjectCompressionPolicy。
10. CompressionRecommendationEngine。
11. ProviderProfile。
12. RouteProfile。
13. InstanceProfile。
14. PreprocessorProfile。
15. LayeredCompressionStage。
16. Chain bridge。
17. Local process manager。
18. Local logs and metrics。
19. Config diff / backup / rollback。
20. Local config and secret handling。
21. Windows/macOS installer。
22. Doctor 页面。
23. JSON/CSV export。
24. Clean-room reimplementation first；若复用 Apache 2.0 代码，必须保留 license notice。

### 4.2 不做

1. 不做云端账号托管。
2. 不做账号共享、账号池、转售或代登录。
3. 不做违反平台条款的自动化用途。
4. 不偷偷读取或迁移用户已有 token。
5. 不把用户代码、日志、prompt 上传到 d-token 服务器。
6. 不做公网 relay 服务，默认只做本地 `127.0.0.1`。
7. 不做静默 provider fallback；任何 fallback 都必须可见、可配置、可审计。
8. 不做 Dashboard 简化版。
9. 不删除 Headroom 功能。
10. 不强制用户必须安装 RTK；RTK 是增强层，Headroom-compatible core 必须可单独工作。
11. 不强制用户必须安装 Headroom runtime；RTK-only 模式也必须能工作。
12. 不强制打开 RTK telemetry；d-token 必须尊重 RTK telemetry disabled-by-default 与 opt-in 状态。
13. 不把 RTK 与 Headroom-compatible core 强耦合；两者必须可单独启用、单独关闭、单独观测。
14. 不在默认日志中保存完整命令输出、完整文件内容、完整 prompt 或完整 API response。

## 5. 一键安装与配置向导

### 5.1 安装向导目标

用户打开 d-token Desktop 后，应能通过图形界面完成：

1. 检测系统：Windows/macOS、CPU 架构、shell、PATH、Python、Rust 可选、Node 可选、WebView2、Xcode CLT、WSL。
2. 选择安装组件：d-token core、RTK、Headroom-compatible runtime、Desktop App、agent hooks、MCP、provider configs。
3. 选择安装模式：External / Managed / Built-in fallback。
4. 选择目标 agent：Claude Code、Codex、Cline、OpenCode、Cursor、Windsurf、Gemini CLI、GitHub Copilot、Kilo Code、Antigravity。
5. 展示即将修改的配置文件 diff。
6. 创建配置备份。
7. 用户确认后执行安装。
8. 安装后运行 Doctor。
9. 生成回滚点。
10. 进入 Dashboard。

### 5.2 工具选择矩阵

```text
Only d-token core:        route/provider/instance + dashboard skeleton
RTK only:                 CLI output savings + RTK dashboard cards
Headroom only:            API context compression + Headroom dashboard cards
RTK + Headroom:           layered compression + end-to-end attribution
All managed:              d-token controls install/update/uninstall with confirmation
External tools:           d-token only detects/imports metrics, does not mutate config silently
```

### 5.3 安装安全要求

1. 任何 hook/config 修改前必须显示 diff。
2. 任何 hook/config 修改前必须创建 backup。
3. 支持一键 rollback。
4. 不静默打开 telemetry。
5. 不静默启用 full raw output 保存。
6. 不覆盖用户已有配置；必须 merge 或提示冲突。
7. 安装完成后必须运行 Doctor。
8. 安装失败必须能恢复到安装前状态。

## 6. Headroom 项目级压缩策略

### 6.1 ProjectProfile

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
  created_at: string;
  updated_at: string;
}
```

### 6.2 ProjectCompressionPolicy

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
  output_verbosity: 'auto' | 'terse' | 'normal' | 'detailed';
  holdout_ratio?: number;
  notes?: string;
}
```

### 6.3 推荐等级引擎

CompressionRecommendationEngine 必须基于以下信号给出推荐等级：

1. 项目语言栈：Rust/TS/Python/Go/Java 等。
2. 最近请求 token 规模。
3. 命令输出噪音比例。
4. Headroom 历史 savings。
5. RTK 历史 savings。
6. LLM 回答质量反馈。
7. retrieve 调用频率。
8. 失败率与回退率。
9. 用户手动覆盖偏好。
10. 项目是否为高风险任务，例如生产部署、数据库迁移、法务/合规文档。

推荐原则：

```text
safe:       默认推荐，适合新项目和高风险任务
balanced:   常规编码任务推荐
aggressive: 大日志、大测试输出、大搜索结果推荐
max:        只对低风险探索/批量日志/RAG chunks 推荐，必须提示可能损失细节
```

### 6.4 UI 要求

每个项目页面必须展示：

1. 当前压缩等级。
2. 推荐压缩等级。
3. 推荐理由。
4. 预计 savings。
5. 质量风险提示。
6. 可一键应用推荐。
7. 可手动覆盖。
8. 可回滚上一版策略。
9. RTK 与 Headroom 的分层贡献。
10. 当前 agent/route/provider 绑定关系。

## 7. RTK 集成设计

### 7.1 RTK 在 d-token 中的定位

RTK 是 d-token 的 Command Execution Layer 优先集成对象。

d-token 不直接替代 RTK 的 command filters，而是优先把 RTK 当作外部能力接入：

```text
RTK owns:
  command rewrite
  command execution proxy
  command-specific filtering
  raw-output tee
  local RTK analytics

d-token owns:
  desktop orchestration
  status detection
  metrics ingestion
  layered savings attribution
  cross-layer dashboard
  route/provider/instance/project management
```

### 7.2 RTK 集成模式

d-token 必须支持三种模式：

#### Mode A：External RTK

用户自己安装 RTK，d-token 只检测和读取：`rtk --version`、`rtk init --show`、`rtk gain --all --format json`、`rtk discover --all`、`rtk session`、RTK config path、RTK tracking database path、RTK tee output path。

这是优先模式。

#### Mode B：Managed RTK Sidecar

d-token 下载或打包 RTK binary，并通过 Desktop 管理安装、升级、卸载、agent hook setup、hook audit、local analytics import。该模式必须保留用户确认，不得静默改写 agent 配置。

#### Mode C：Built-in Preprocessor

在 RTK 不可用或用户不想安装 RTK 时，d-token 提供基础内置预处理器：ANSI 清理、空行压缩、重复行折叠、超长输出截断、error-only mode、raw output local recovery。该模式只能作为 fallback，不应声称等价于完整 RTK。

### 7.3 Agent Hook 适配要求

1. Hook-based agents：通过 hook 在命令执行前 rewrite。
2. Plugin-based agents：通过 plugin API rewrite。
3. Instruction-based agents：通过 AGENTS/CLAUDE/规则文件提示用户显式调用 RTK。
4. Native Windows：允许 RTK filters 工作，但自动 rewrite 能力受限。
5. WSL：作为 Windows 上获得完整 hook 体验的推荐路径。

### 7.4 RTK 限制必须在 UI 中明确提示

1. RTK hook 主要作用于 Bash/tool command calls。
2. Claude Code 内置 `Read`、`Grep`、`Glob` 等工具不经过 Bash hook，不能被 RTK 自动 rewrite。
3. Native Windows 下 hook 自动改写能力有限，用户可能需要显式调用 `rtk ...`。
4. RTK 过滤失败或用户需要完整上下文时，应通过 verbose/raw-output/tee 查看原始输出。
5. RTK telemetry 默认关闭，且必须用户显式同意才可启用。

## 8. 分层压缩指标模型

### 8.1 三类 savings

Dashboard 必须区分三类 savings：

```text
rtk_savings
  命令输出从 raw_cli_output 到 cleaned_cli_output 的节省

headroom_savings
  Agent 上下文进入 d-token Local Proxy 后到最终 LLM input 的节省

end_to_end_savings
  raw_cli_output + files + conversation + tool results 到最终 LLM input 的总节省
```

### 8.2 防重复统计

RTK 已经减少过的 CLI 输出进入 Headroom 后，Headroom 的 savings 只能按“RTK 后输入”计算；端到端 savings 必须保留 raw baseline。

```text
raw_cli_tokens
  → rtk_cleaned_tokens
  → headroom_final_tokens

rtk_savings = raw_cli_tokens - rtk_cleaned_tokens
headroom_savings = headroom_input_tokens - headroom_final_tokens
end_to_end_savings = raw_cli_tokens + non_cli_raw_context_tokens - final_llm_input_tokens
```

禁止把 `rtk_savings + headroom_savings` 直接相加当作总 savings，除非 denominator 和 baseline 已经统一。

### 8.3 LayeredCompressionStage

```ts
interface LayeredCompressionStage {
  request_id?: string;
  command_id?: string;
  project_id?: string;
  stage: 'rtk_preprocess' | 'headroom_context' | 'route_forward';
  source: 'rtk' | 'headroom' | 'dtoken_builtin' | 'custom';
  input_tokens?: number;
  output_tokens?: number;
  tokens_saved?: number;
  savings_percent?: number;
  transforms_applied: string[];
  raw_recovery_ref?: string;
  status: 'ok' | 'passthrough' | 'failed' | 'disabled';
}
```

### 8.4 PreprocessorProfile

```ts
interface PreprocessorProfile {
  id: string;
  name: string;
  implementation: 'rtk_external' | 'rtk_managed' | 'dtoken_builtin' | 'custom';
  binary_path?: string;
  version?: string;
  agent_hook_status: 'installed' | 'missing' | 'partial' | 'unsupported' | 'unknown';
  telemetry_status: 'disabled' | 'enabled' | 'unknown';
  config_path?: string;
  tracking_db_path?: string;
  tee_dir?: string;
  supported_agents: string[];
}
```

## 9. Dashboard 全量迁移范围

### 9.1 必须迁移 Headroom Dashboard

必须迁移：Version、Session/Historical、Health status、Updated time、Light/Dark theme、Live Feed、Telemetry badge、Savings profile、Money saved、Token savings、Output tokens saved、Overhead、Throughput、Agent Usage、Before/After/Saved/Savings、Agent rows、Window contribution/efficiency、Anomalies/discrepancies、Prefix cache by provider、Cache reads/writes/busts、Removed content breakdown、Savings over time、Token usage、Provider breakdown、Performance/latency/TTFB/failed requests、Pipeline timing breakdown、Per-model token savings、Per-project savings、Recent requests expandable table、Original tokens、Compressed tokens、Tokens removed、Optimization time、Transforms applied、Waste signals per request、Historical proxy compression、JSON export、CSV export、Lifetime money saved、Lifetime tokens saved、Active days、Average saved per day、Hourly/daily/weekly/monthly rollups。

### 9.2 必须新增 d-token / RTK / Project 卡片

1. One-click installer status。
2. Tool selection：d-token / RTK / Headroom / MCP / hooks。
3. Project compression policy。
4. Recommended compression level。
5. Recommendation reason。
6. Policy diff / rollback。
7. RTK Status。
8. RTK Version。
9. RTK Hook Status。
10. RTK Supported Agent Mode。
11. RTK Telemetry Status。
12. RTK Gain Summary。
13. RTK Daily Savings。
14. RTK Recent Commands。
15. RTK Missed Opportunities。
16. RTK Session Adoption。
17. RTK Passthrough Commands。
18. RTK Low-savings Commands。
19. RTK Tee Recovery Files。
20. RTK Windows Compatibility Notice。
21. Layered Contribution：RTK vs Headroom。
22. Headroom-after-RTK savings。
23. Per-agent layered savings。
24. Per-instance layered savings。
25. Per-command layered savings。

新增 d-token 维度：ProviderProfile、RouteProfile、InstanceProfile、ProjectProfile、ProjectCompressionPolicy、CompressionRecommendation、Chain mode、OS process status、Config source、PreprocessorProfile、LayeredCompressionStage、RTKCommandCategory、RTKHookMode。

## 10. 技术选型

### 10.1 桌面 App

采用：

```text
Tauri 2 + React + TypeScript + Vite
```

理由：Tauri 适合轻量桌面壳，支持 Windows/macOS/Linux，支持 external binaries / sidecar，适合管理 proxy、compress、router、doctor、RTK/preprocessor、Headroom runtime 等本地进程；React/TypeScript 适合迁移 Headroom Dashboard 和 RTK analytics。

Electron 仅作为备选，不作为主路线。

### 10.2 本地核心

采用：

```text
Rust core + Python sidecar transitional architecture + RTK external binary integration
```

短期：

- Rust 负责 process manager、local API、proxy/router skeleton、logs、config、port manager、doctor、installer orchestrator、RTK adapter interface、project policy manager。
- Python sidecar 保留 Headroom-compatible compression、CCR、memory、learn、MCP 等能力。
- RTK 优先以外部 binary 接入，不在 M1/M2 复制其 filters。

长期：

- 稳定的 proxy、router、metrics、ledger、config、process manager、RTK metrics ingestion、project policy manager 逐步 Rust 化。
- 高级压缩、ML、learn、evals 可继续保留 Python sidecar，直到 Rust parity 足够可靠。
- 若将 RTK 或 Headroom 功能内置，必须另做 license review、feature parity matrix 和 compatibility test suite。

## 11. Doctor 要求

### 11.1 d-token Core

Doctor 页面必须检查：Local proxy 是否运行、端口是否占用、route 是否匹配、provider 是否健康、chain bridge 是否健康、Headroom-compatible runtime 是否可用、project policy 是否有效、config backup/rollback 是否可用。

### 11.2 RTK

Doctor 页面必须检查：RTK binary 是否存在、RTK version、是否存在同名错误包或错误 binary、RTK hook 是否安装、当前 agent 是否支持 auto-rewrite、当前平台是否支持完整 hook、Windows 是否处于 WSL 环境、Native Windows 是否需要显式 `rtk ...` 调用、RTK config 是否存在、RTK tracking database 是否存在、RTK tee directory 是否存在、RTK telemetry 状态、`rtk gain --all --format json` 是否可执行。

### 11.3 Headroom

Doctor 页面必须检查：Headroom CLI 是否存在、Python runtime 是否满足、proxy 是否运行、wrap 状态、MCP server 状态、dashboard 数据端点、metrics/savings ledger、CCR retrieve 是否可用、project compression policy 是否生效、output shaper 是否生效、memory/learn 是否生效。

## 12. Windows/macOS 重点要求

### 12.1 Windows

1. WebView2 检测。
2. MSVC toolchain 检测。
3. 端口占用检测。
4. 旧进程残留检测。
5. `.cmd` / `.exe` / `.bat` shim 兼容。
6. 盘符路径兼容。
7. UTF-8 日志。
8. 防火墙提示。
9. installer 签名预留。
10. RTK native Windows mode 检测。
11. WSL 检测。
12. RTK hook limitation 明示。
13. PowerShell / cmd / Windows Terminal 指引。
14. Python/Headroom runtime 检测。
15. 配置 backup/rollback 检测。

### 12.2 macOS

1. Intel / Apple Silicon 双架构。
2. Xcode Command Line Tools 检测。
3. quarantine / xattr 提示。
4. Keychain 预留。
5. LaunchAgent 预留。
6. 菜单栏托盘预留。
7. notarization 预留。
8. RTK Homebrew 安装状态检测。
9. RTK hook / plugin 状态检测。
10. Python/Headroom runtime 检测。
11. 配置 backup/rollback 检测。

## 13. 安全与隐私原则

1. 默认只监听 `127.0.0.1`。
2. 不默认开放公网端口。
3. API key 只存在本地。
4. 优先 OS keychain，开发期可用本地配置文件占位。
5. 日志默认脱敏。
6. Authorization / Cookie / API key 永不明文展示。
7. 导出日志二次脱敏。
8. 所有 sidecar 可停止、可查看、可重启。
9. 所有 route decision 必须可记录、可解释。
10. 不做静默 fallback。
11. RTK/CLI 输出预处理层不得默认记录完整命令输出。
12. 如果开启完整 CLI 输出留存，必须显式确认、仅本地保存、可一键删除、导出时脱敏。
13. d-token 不自动开启 RTK telemetry。
14. d-token 读取 RTK telemetry 状态时只能展示状态，不替用户同意。
15. d-token 导入 RTK analytics 时默认只读取聚合统计，不读取源码、文件内容、完整命令参数或敏感环境变量。
16. d-token 不上传 RTK tracking database、tee files、prompt、代码、命令输出到远端。
17. d-token 修改 agent/RTK/Headroom 配置前必须显示 diff、创建 backup、支持 rollback。
18. d-token 不默认读取 Headroom CCR 原文缓存；仅显示摘要、大小、数量和检索状态。

## 14. 目录结构

仓库根目录只保留：

```text
AGENTS.md
CLAUDE.md
docs/
code/
support/
.gitignore
```

目录含义：

```text
docs/      所有正式文档，包含唯一 PRD、架构、技术决策、安全边界、审查报告、研究报告
code/      所有应用代码、crate、package、sidecar、runtime、配置样例
support/   测试、脚本、开发辅助、CI 模板、临时杂项
```

## 15. 里程碑

### M0：文档与仓库结构收敛

1. 单一 living PRD。
2. 技术选型文档。
3. 架构文档。
4. 安全边界文档。
5. 最终多维审查报告。
6. 仓库目录重构为 `docs/`、`code/`、`support/`。

### M1：Core Skeleton + RTK/Headroom Adapter Skeleton

1. Rust workspace。
2. local API skeleton。
3. proxy skeleton。
4. config loader。
5. process manager skeleton。
6. doctor skeleton。
7. `dtoken-preprocessor` crate。
8. `dtoken-project-policy` crate。
9. RTK binary/version detection。
10. Headroom runtime/proxy detection。
11. PreprocessorProfile。
12. ProjectProfile。
13. ProjectCompressionPolicy。
14. LayeredCompressionStage。
15. M1 mock RTK/Headroom metrics schema。
16. config diff/backup/rollback skeleton。

### M2：Dashboard Skeleton + Installer Wizard

1. Tauri shell。
2. React Dashboard。
3. One-click installer wizard。
4. Tool selection cards。
5. Headroom Dashboard 全卡片占位。
6. RTK Dashboard cards 占位。
7. Project compression policy cards 占位。
8. Layered compression cards 占位。
9. mock `/stats` / `/stats-history`。
10. mock `/preprocessors` / `/rtk/status` / `/rtk/gain`。
11. mock `/headroom/status` / `/headroom/policies` / `/projects`。
12. export 占位。

### M3：Provider / Route / Instance / RTK / Headroom Integration

1. ProviderProfile。
2. RouteProfile。
3. Instance manager。
4. Chain bridge。
5. PreprocessorProfile。
6. ProjectProfile。
7. ProjectCompressionPolicy。
8. CompressionRecommendationEngine。
9. LayeredCompressionStage。
10. request log schema。
11. RTK status detection。
12. RTK analytics import。
13. RTK hook status detection。
14. Headroom proxy status detection。
15. Headroom project policy application。
16. Windows WSL/native mode detection。
17. config backup/rollback。

### M4：Headroom-compatible Parity + Layered Compression

1. Compression pipeline。
2. CCR。
3. memory。
4. learn。
5. output shaper。
6. historical ledger。
7. RTK/custom preprocessor metrics ingestion。
8. end-to-end savings attribution。
9. RTK + Headroom double-count prevention。
10. raw-output recovery reference model。
11. project-level compression recommendation。
12. quality holdout / measured vs estimated output savings。

### M5：Production Hardening

1. Windows installer。
2. macOS bundle。
3. updater。
4. signing/notarization 预留。
5. benchmark/evals。
6. crash recovery。
7. RTK hook compatibility matrix。
8. RTK install/uninstall rollback。
9. Headroom install/uninstall rollback。
10. privacy/export/redaction audit。
11. CI gate：cargo check、cargo test、structure check。

## 16. 验收标准

1. 仓库只有一个 PRD：`docs/PRD.md`。
2. 所有正式文档都在 `docs/`。
3. 所有代码都在 `code/`。
4. 所有测试、脚本、杂项都在 `support/`。
5. 根目录只保留 AI 入口说明文件和必要目录。
6. Headroom 功能迁移不能删项。
7. Dashboard 迁移不能缩水。
8. Windows/macOS App 技术路线明确。
9. 安全边界明确。
10. 每轮大改后必须更新本 PRD，而不是新建版本 PRD。
11. RTK 与 Headroom-compatible core 必须可组合、可单独启停、可单独观测。
12. Dashboard 必须能区分 RTK 命令输出预处理收益、Headroom 上下文压缩收益、端到端总收益。
13. d-token 不得替用户静默修改 RTK hook/config。
14. d-token 不得自动开启 RTK telemetry。
15. Native Windows RTK hook limitation 必须在 UI 和 Doctor 中明确提示。
16. Claude Code 内置工具绕过 Bash hook 的限制必须在 UI 和 Doctor 中明确提示。
17. RTK raw-output tee 默认只展示摘要和路径，不默认读取完整内容。
18. 真实 provider 转发、RTK 导入、Headroom 压缩三条链路必须有独立错误边界。
19. 用户必须能按项目自定义 Headroom 压缩等级，并查看推荐等级与推荐理由。
20. 用户必须能通过安装向导自定义安装 d-token / RTK / Headroom / MCP / hooks 的任意组合。
21. 所有 agent/RTK/Headroom 配置修改必须支持 diff、backup、rollback。
22. CI 必须至少覆盖 structure check、cargo check、cargo test。
