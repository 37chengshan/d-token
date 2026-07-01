# d-token Living PRD

状态：Living Document  
唯一规则：本文件是 d-token 的唯一 PRD。后续任何产品目标、边界、技术路线、目录结构调整，都直接修改本文件，不再另起 v0.1/v0.2/v0.3 多版本 PRD。

## 1. 项目一句话

d-token 是一个面向 AI Coding 的本地分层压缩、路由与桌面观测系统：上游集成 `rtk-ai/rtk` 作为 CLI 命令输出预处理层，下游保留 Headroom-compatible API 接入层压缩能力，并通过 Windows/macOS 桌面 App 统一管理 provider、route、instance、chain bridge、Dashboard、Doctor 与端到端 token savings。

## 2. 产品定位

### 2.1 核心判断

RTK 与 Headroom-compatible core 工作在不同链路层级：

```text
RTK:      Command Execution Layer / Shell Output Layer
Headroom: API Gateway Layer / LLM Context Layer
```

二者不是竞争关系，而是上下游互补关系。

RTK 负责在命令输出进入 Agent 上下文前做第一层清洗；Headroom-compatible core 负责在完整上下文发往 LLM 前做第二层压缩。d-token 负责把两者统一编排、观测、统计、诊断和桌面化。

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
4. 支持 RTK 外部二进制探测、版本探测、安装状态探测、agent hook 状态探测、analytics 数据导入。
5. 支持 Claude Code、Codex、Cline、OpenCode、Cursor、Windsurf、Gemini CLI、GitHub Copilot、Kilo Code、Antigravity 等 AI Coding 工具接入。
6. 支持官方 provider、第三方 compatible provider、chain bridge 三类路径。
7. 支持多个本地实例并行运行，并在 Dashboard 中按实例拆分数据。
8. 保证所有请求链路可观测：agent、instance、provider、route、chain mode、tokens、latency、error、transforms。
9. 保证两层压缩可观测：RTK command-output savings、Headroom context savings、end-to-end savings。
10. 保证 Windows/macOS 用户可以通过图形界面完成安装、配置、启动、停止、诊断、导出。

## 3. 外部项目学习结论：rtk-ai/rtk

### 3.1 RTK 是什么

`rtk-ai/rtk` 是一个高性能 CLI proxy，用于在命令输出进入 LLM 上下文前过滤和压缩命令输出。其 README 定位为：减少常见开发命令的 LLM token 消耗，单 Rust binary，面向 AI Coding 场景。

### 3.2 RTK 的核心能力

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

### 3.3 RTK 支持的命令生态

RTK 的适配范围覆盖：

1. Files：`ls`、`tree`、`read`、`find`、`grep`、`diff`。
2. Git：`git status`、`git log`、`git diff`、`git add`、`git commit`、`git push`、`git pull`。
3. GitHub CLI：`gh pr`、`gh issue`、`gh run`。
4. Test runners：`jest`、`vitest`、`playwright`、`pytest`、`go test`、`cargo test`、`rake test`、`rspec`。
5. Build and lint：`eslint`、`tsc`、`next build`、`prettier`、`cargo build`、`cargo clippy`、`ruff`、`golangci-lint`、`rubocop`。
6. Package managers：`pnpm`、`uv`、`pip`、`bundle`、`prisma`。
7. Cloud and containers：`aws`、`docker`、`kubectl`、`oc`。
8. Infrastructure as Code：`pulumi`。
9. Data and analytics：`json`、`deps`、`env`、`log`、`curl`、`wget`、`summary`、`proxy`。

### 3.4 RTK 的过滤策略

d-token Dashboard 和 metrics schema 必须能表达 RTK 的过滤策略，包括：

1. Smart Filtering。
2. Grouping。
3. Truncation。
4. Deduplication。
5. Stats extraction。
6. Error-only filtering。
7. Structure-only filtering。
8. Code filtering levels。
9. Failure focus。
10. Progress filtering。
11. JSON/text dual mode。
12. State-machine parsing。
13. NDJSON streaming parsing。

### 3.5 RTK 的设计原则

d-token 适配 RTK 时必须保留以下工程原则：

1. Single responsibility：每类命令由独立模块处理。
2. Minimal overhead：预处理不能明显拖慢命令执行。
3. Exit code preservation：必须保留原命令退出码，避免破坏 CI/CD、测试、脚本语义。
4. Fail-safe：过滤失败时必须回退到原始输出或明确提示，而不是吞掉内容。
5. Transparent：用户必须能通过 verbose/raw-output/tee 机制看到原始输出。
6. Local-first：本地命令输出、本地统计、本地恢复文件优先。

## 4. 明确边界

### 4.1 必须做

1. Headroom-compatible core。
2. Full Dashboard migration。
3. RTK integration layer。
4. RTK binary detection。
5. RTK version detection。
6. RTK agent hook status detection。
7. RTK analytics import：`gain` / `discover` / `session`。
8. RTK raw-output tee visibility。
9. ProviderProfile。
10. RouteProfile。
11. Instance manager。
12. PreprocessorProfile。
13. LayeredCompressionStage。
14. Chain bridge。
15. Local process manager。
16. Local logs and metrics。
17. Local config and secret handling。
18. Windows/macOS installer。
19. Doctor 页面。
20. JSON/CSV export。
21. Clean-room reimplementation first；若复用 Apache 2.0 代码，必须保留 license notice。

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
11. 不强制打开 RTK telemetry；d-token 必须尊重 RTK telemetry disabled-by-default 与 opt-in 状态。
12. 不把 RTK 与 Headroom-compatible core 强耦合；两者必须可单独启用、单独关闭、单独观测。
13. 不在默认日志中保存完整命令输出、完整文件内容、完整 prompt 或完整 API response。

## 5. Headroom 功能保留范围

必须保留：

1. Library / SDK。
2. Local proxy。
3. Agent wrap / unwrap。
4. MCP server。
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
16. SharedContext / inter-agent handoff。
17. Failure learning。
18. Verbosity learning。
19. Output token reduction。
20. Verbosity steering。
21. Effort routing。
22. Output savings estimation。
23. Measured / estimated savings 标识。
24. Durable savings ledger。
25. Metrics endpoints。
26. Historical stats。
27. JSON / CSV export。
28. Cost tracking。
29. Per-model breakdown。
30. Per-client / per-agent breakdown。
31. Per-project breakdown。
32. Doctor / perf / benchmark / troubleshooting。
33. Framework integrations：OpenAI SDK、Anthropic SDK、Vercel AI SDK、LangChain、Agno、Strands、LiteLLM。

## 6. RTK 集成设计

### 6.1 RTK 在 d-token 中的定位

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
  route/provider/instance management
```

### 6.2 RTK 集成模式

d-token 必须支持三种模式：

#### Mode A：External RTK

用户自己安装 RTK，d-token 只检测和读取：

1. `rtk --version`。
2. `rtk init --show`。
3. `rtk gain --all --format json`。
4. `rtk discover --all`。
5. `rtk session`。
6. RTK config path。
7. RTK tracking database path。
8. RTK tee output path。

这是优先模式。

#### Mode B：Managed RTK Sidecar

d-token 下载或打包 RTK binary，并通过 Desktop 管理：

1. 安装。
2. 升级。
3. 卸载。
4. agent hook setup。
5. hook audit。
6. local analytics import。

该模式必须保留用户确认，不得静默改写 agent 配置。

#### Mode C：Built-in Preprocessor

在 RTK 不可用或用户不想安装 RTK 时，d-token 提供基础内置预处理器：

1. ANSI 清理。
2. 空行压缩。
3. 重复行折叠。
4. 超长输出截断。
5. error-only mode。
6. raw output local recovery。

该模式只能作为 fallback，不应声称等价于完整 RTK。

### 6.3 Agent Hook 适配要求

d-token 必须识别 RTK 对不同 agent 的接入方式：

1. Hook-based agents：通过 hook 在命令执行前 rewrite。
2. Plugin-based agents：通过 plugin API rewrite。
3. Instruction-based agents：通过 AGENTS/CLAUDE/规则文件提示用户显式调用 RTK。
4. Native Windows：允许 RTK filters 工作，但自动 rewrite 能力受限。
5. WSL：作为 Windows 上获得完整 hook 体验的推荐路径。

### 6.4 RTK 限制必须在 UI 中明确提示

1. RTK hook 主要作用于 Bash/tool command calls。
2. Claude Code 内置 `Read`、`Grep`、`Glob` 等工具不经过 Bash hook，不能被 RTK 自动 rewrite。
3. Native Windows 下 hook 自动改写能力有限，用户可能需要显式调用 `rtk ...`。
4. RTK 过滤失败或用户需要完整上下文时，应通过 verbose/raw-output/tee 查看原始输出。
5. RTK telemetry 默认关闭，且必须用户显式同意才可启用。

### 6.5 RTK 数据导入

d-token Dashboard 必须优先通过 RTK 官方命令和本地数据导入统计：

1. `rtk gain`：总 savings。
2. `rtk gain --history`：最近命令历史。
3. `rtk gain --daily`：按日统计。
4. `rtk gain --all --format json`：Dashboard 导入。
5. `rtk discover`：漏掉的优化机会。
6. `rtk session`：不同会话/agent 的 adoption 情况。
7. local SQLite tracking database：只读导入，不破坏 RTK 原始数据。
8. tee files：只显示路径和摘要，默认不读取完整内容。

### 6.6 RTK 配置管理

d-token Desktop 可提供 RTK 配置 UI，但必须遵守：

1. 不自动开启 telemetry。
2. 不默认启用 full raw output 保存。
3. 不静默修改用户现有 RTK config。
4. 修改前展示 diff。
5. 支持 rollback。
6. 支持 exclude_commands。
7. 支持 tee mode：failures / always / never。
8. 支持 per-project filter 状态展示。

## 7. 分层压缩指标模型

### 7.1 三类 savings

Dashboard 必须区分三类 savings：

```text
rtk_savings
  命令输出从 raw_cli_output 到 cleaned_cli_output 的节省

headroom_savings
  Agent 上下文进入 d-token Local Proxy 后到最终 LLM input 的节省

end_to_end_savings
  raw_cli_output + files + conversation + tool results 到最终 LLM input 的总节省
```

### 7.2 LayeredCompressionStage

新增领域模型：

```ts
interface LayeredCompressionStage {
  request_id?: string;
  command_id?: string;
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

### 7.3 PreprocessorProfile

新增领域模型：

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

### 7.4 Dashboard 必须新增的 RTK 卡片

1. RTK Status。
2. RTK Version。
3. RTK Hook Status。
4. RTK Supported Agent Mode。
5. RTK Telemetry Status。
6. RTK Gain Summary。
7. RTK Daily Savings。
8. RTK Recent Commands。
9. RTK Missed Opportunities。
10. RTK Session Adoption。
11. RTK Passthrough Commands。
12. RTK Low-savings Commands。
13. RTK Tee Recovery Files。
14. RTK Windows Compatibility Notice。
15. Layered Contribution：RTK vs Headroom。

## 8. Dashboard 全量迁移范围

必须迁移 Headroom Dashboard：

1. Version。
2. Session / Historical 双视图。
3. Health status。
4. Updated time。
5. Light / Dark theme。
6. Live Feed。
7. Telemetry badge。
8. Savings profile。
9. Money saved。
10. Token savings。
11. Output tokens saved。
12. Overhead。
13. Throughput：input、compression、forward、generation。
14. Agent Usage。
15. Before / after / saved / savings。
16. Agent rows。
17. Window contribution / efficiency。
18. Anomalies / discrepancies。
19. Prefix cache by provider。
20. Cache reads / writes / busts。
21. Removed content breakdown。
22. Savings over time。
23. Token usage。
24. Provider breakdown。
25. Performance / latency / TTFB / failed requests。
26. Pipeline timing breakdown。
27. Per-model token savings。
28. Per-project savings。
29. Recent requests expandable table。
30. Original tokens。
31. Compressed tokens。
32. Tokens removed。
33. Optimization time。
34. Transforms applied。
35. Waste signals per request。
36. Historical proxy compression。
37. JSON export。
38. CSV export。
39. Lifetime money saved。
40. Lifetime tokens saved。
41. Active days。
42. Average saved per day。
43. Hourly / daily / weekly / monthly rollups。

必须新增 d-token / RTK 分层卡片：

44. Layered compression contribution。
45. CLI preprocessor savings。
46. End-to-end savings。
47. RTK command ecosystem breakdown。
48. RTK hook adoption。
49. RTK missed savings opportunities。
50. RTK raw-output recovery visibility。
51. Headroom-after-RTK savings。
52. Per-agent layered savings。
53. Per-instance layered savings。
54. Per-command layered savings。

新增 d-token 维度：ProviderProfile、RouteProfile、Instance、Chain mode、OS process status、Config source、PreprocessorProfile、LayeredCompressionStage、RTKCommandCategory、RTKHookMode。

## 9. 技术选型

### 9.1 桌面 App

采用：

```text
Tauri 2 + React + TypeScript + Vite
```

理由：

1. Tauri 支持 Windows/macOS/Linux 桌面应用。
2. Tauri 适合轻量桌面壳，不需要打包完整 Chromium。
3. Tauri 支持 external binaries / sidecar，适合管理 proxy、compress、router、doctor、RTK/preprocessor 等本地进程。
4. Tauri 支持 updater，适合后续 Windows/macOS 自动更新。
5. React/TypeScript 适合把 Headroom Dashboard 和 RTK analytics 组件化迁移。

Electron 仅作为备选，不作为主路线。只有在极端追求前端开发速度且完全不想碰 Rust 时才考虑。

### 9.2 本地核心

采用：

```text
Rust core + Python sidecar transitional architecture + RTK external binary integration
```

短期：

- Rust 负责 process manager、local API、proxy/router skeleton、logs、config、port manager、doctor、RTK adapter interface。
- Python sidecar 保留 Headroom-compatible compression、CCR、memory、learn、MCP 等能力。
- RTK 优先以外部 binary 接入，不在 M1/M2 复制其 filters。

长期：

- 稳定的 proxy、router、metrics、ledger、config、process manager、RTK metrics ingestion 逐步 Rust 化。
- 高级压缩、ML、learn、evals 可继续保留 Python sidecar，直到 Rust parity 足够可靠。
- 若将 RTK 功能内置，必须另做 license review、feature parity matrix 和 compatibility test suite。

## 10. 运行时架构

### 10.1 标准 API 接入链路

```text
AI Coding Tool
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

### 10.2 RTK + Headroom 分层压缩链路

```text
AI Coding Tool
  ↓
Bash / Terminal Command Call
  ↓
RTK Hook / Plugin / Explicit rtk Command
  ↓
RTK CLI Proxy Executes Original Command
  ↓
RTK Command-specific Filter
  ↓
Cleaned CLI Output Enters Agent Context
  ↓
d-token Local Proxy
  ↓
Headroom-compatible Compression Core
  ↓
Route Decision
  ↓
Provider
```

### 10.3 桌面端职责

桌面端负责：

1. 配置。
2. 启停。
3. 状态展示。
4. 日志查看。
5. 诊断。
6. 导出。
7. PreprocessorProfile 管理。
8. RTK 安装/版本/hook/telemetry 状态展示。
9. RTK analytics 导入。
10. 分层压缩贡献展示。

请求链路由本地 proxy/core 处理，Desktop 关闭后可以选择继续保留本地代理运行。

## 11. Doctor 要求

Doctor 页面必须检查：

### 11.1 d-token Core

1. Local proxy 是否运行。
2. 端口是否占用。
3. route 是否匹配。
4. provider 是否健康。
5. chain bridge 是否健康。
6. Headroom-compatible runtime 是否可用。

### 11.2 RTK

1. RTK binary 是否存在。
2. RTK version。
3. 是否存在同名错误包或错误 binary。
4. RTK hook 是否安装。
5. 当前 agent 是否支持 auto-rewrite。
6. 当前平台是否支持完整 hook。
7. Windows 是否处于 WSL 环境。
8. Native Windows 是否需要显式 `rtk ...` 调用。
9. RTK config 是否存在。
10. RTK tracking database 是否存在。
11. RTK tee directory 是否存在。
12. RTK telemetry 状态。
13. `rtk gain --all --format json` 是否可执行。

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
docs/      所有正式文档，包含唯一 PRD、架构、技术决策、安全边界、审查报告
code/      所有应用代码、crate、package、sidecar、runtime、配置样例
support/   测试、脚本、开发辅助、CI 模板、临时杂项
```

旧的多版本 PRD、分散 README、分散 architecture/review/research 文档必须收敛到 `docs/` 中，不再在根目录或代码目录中扩散。

## 15. 里程碑

### M0：文档与仓库结构收敛

1. 单一 living PRD。
2. 技术选型文档。
3. 架构文档。
4. 安全边界文档。
5. 最终多维审查报告。
6. 仓库目录重构为 `docs/`、`code/`、`support/`。

### M1：Core Skeleton + RTK Adapter Skeleton

1. Rust workspace。
2. local API skeleton。
3. proxy skeleton。
4. config loader。
5. process manager skeleton。
6. doctor skeleton。
7. `dtoken-preprocessor` crate。
8. RTK binary/version detection。
9. PreprocessorProfile。
10. LayeredCompressionStage。
11. M1 mock RTK metrics schema。

### M2：Dashboard Skeleton

1. Tauri shell。
2. React Dashboard。
3. Headroom Dashboard 全卡片占位。
4. RTK Dashboard cards 占位。
5. Layered compression cards 占位。
6. mock `/stats` / `/stats-history`。
7. mock `/preprocessors` / `/rtk/status` / `/rtk/gain`。
8. export 占位。

### M3：Provider / Route / Instance / RTK Integration

1. ProviderProfile。
2. RouteProfile。
3. Instance manager。
4. Chain bridge。
5. PreprocessorProfile。
6. LayeredCompressionStage。
7. request log schema。
8. RTK status detection。
9. RTK analytics import。
10. RTK hook status detection。
11. Windows WSL/native mode detection。

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

### M5：Production Hardening

1. Windows installer。
2. macOS bundle。
3. updater。
4. signing/notarization 预留。
5. benchmark/evals。
6. crash recovery。
7. RTK hook compatibility matrix。
8. RTK install/uninstall rollback。
9. privacy/export/redaction audit。

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
