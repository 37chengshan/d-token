# d-token Living PRD

状态：Living Document  
唯一规则：本文件是 d-token 的唯一 PRD。后续任何产品目标、边界、技术路线、目录结构调整，都直接修改本文件，不再另起 v0.1/v0.2/v0.3 多版本 PRD。

## 1. 项目一句话

`d-token` 是一个面向 AI Coding 的本地分层压缩、路由、中转适配与图形化控制面：开发期采用 **No-touch Dev Mode**，避免破坏开发者本机已经跑通的 RTK、Headroom 与 agents；成品则提供可覆盖安装的一键安装器，把 RTK、Headroom、d-token proxy、provider route、project policy、Doctor、Dashboard、backup/rollback 与端到端 token savings attribution 合并成统一体验。

## 2. 关键路线澄清：开发期 no-touch，成品可覆盖安装

### 2.1 两条线必须分开

本项目有两条完全不同的线：

```text
开发测试线：No-touch Dev Mode
  目标：保护开发者本机已经部署好、正在使用的 agents / RTK / Headroom
  原则：不调用真实工具、不读写真实配置、不改 hook、不占真实端口

成品安装线：Production Managed Install
  目标：让最终用户一键覆盖安装 / 配置 / 升级 / 卸载
  原则：可以写配置、装工具、改 hook，但必须有事务、备份、校验、回滚、可解释日志
```

此前的“no-touch”只适用于 **开发期本地测试**，不代表成品不能覆盖安装。成品当然可以直接安装和覆盖配置，但必须是受控的安装事务。

### 2.2 开发期真实前提

开发者本机的 agents、RTK、Headroom 可能已经安装好，并且正在正常使用。

因此开发测试阶段禁止：

```text
重新安装 RTK
重新安装 Headroom
修改已有 agent hook
修改 shell profile
修改 RTK config
修改 Headroom config
占用真实 proxy 端口
启动/停止/重启真实服务
```

开发期必须通过 fixture、fake binary、临时 HOME、临时 PATH、临时端口、mock provider、sandbox config 来验证 d-token 自身逻辑。

### 2.3 成品安装目标

成品面向普通用户时，应支持：

1. 一键安装 RTK。
2. 一键安装 Headroom。
3. 一键安装 d-token core/proxy/desktop。
4. 一键配置 agent hooks。
5. 一键配置 provider routes。
6. 一键配置 project compression policy。
7. 覆盖旧版本安装。
8. 升级已有配置。
9. 卸载与清理。
10. rollback 到安装前状态。

成品覆盖安装必须遵守：

```text
scan
  ↓
plan
  ↓
show diff
  ↓
backup
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
stop new processes
  ↓
restore backups
  ↓
remove partial files
  ↓
report exact failure
```

## 3. 合并的含义

“合并”不是一开始把 RTK 和 Headroom 源码揉成一个项目，而是把它们的能力统一到 d-token 控制面。

```text
RTK owns:
  command rewrite
  command execution proxy
  command-specific filtering
  raw-output tee
  local RTK analytics

Headroom owns:
  context compression
  proxy / SDK / MCP
  CCR retrieve
  memory / learn
  savings ledger / stats

d-token owns:
  local control plane
  route / provider / chain adapter
  project policy
  unified dashboard
  event ledger
  layered attribution
  doctor
  config diff / backup / rollback
  desktop GUI
  production installer transaction
```

## 4. 最终产品形态

```text
AI Coding Tool
  ↓
Shell Command Execution
  ↓
RTK CLI Proxy / Hook / Plugin
  ↓
Cleaned CLI Output enters Agent Context
  ↓
Headroom or d-token Local Proxy / Route Adapter
  ↓
Headroom-compatible Compression Core
  ↓
Route Decision
  ↓
Official Provider / Custom Provider / Chain Bridge
  ↓
Provider
  ↓
d-token Event Ledger / Dashboard / Doctor / GUI
```

## 5. 产品目标

1. 开发期不破坏开发者本机已有 agents、RTK、Headroom。
2. 成品支持一键覆盖安装、升级、卸载与回滚。
3. 保留 Headroom 完整能力，不做删减版。
4. 将 `rtk-ai/rtk` 作为一等外部集成对象，而不是泛泛的“CLI 清洗脚本”。
5. 支持 RTK command-output savings、Headroom context savings、end-to-end savings 分层统计。
6. 支持 Headroom 每项目压缩等级、推荐等级、output shaping、memory、learn、MCP、CCR 开关。
7. 支持 Claude Code、Codex、Cline、OpenCode、Cursor、Windsurf、Gemini CLI、GitHub Copilot、Kilo Code、Antigravity 等 AI Coding 工具接入状态展示与最终安装配置。
8. 支持官方 provider、第三方 compatible provider、chain bridge 三类路径。
9. 支持多个本地实例并行运行，并按 project / agent / instance / route / provider 拆分统计。
10. 提供统一 GUI：Dashboard、Doctor、Project Policy、RTK/Headroom Status、Install Wizard、Config diff、Backup、Rollback、Logs、Export。

## 6. 模式设计

### 6.1 Fixture Mode：开发默认

完全读取 `support/fixtures`，不调用任何真实命令。

用于测试：parser、API response、Dashboard、EventLedger、LayeredSavings、ProjectPolicy。

必须保证：

1. 不调用真实 `rtk`。
2. 不调用真实 `headroom`。
3. 不读真实 agent config。
4. 不改任何真实配置。
5. 只使用测试端口。

### 6.2 Fake-binary Mode：开发命令调用测试

通过 `support/fake-bin` 提供假的 `rtk` / `headroom`，测试 command runner、stdout/stderr、exit code、error boundary。

必须使用临时 `HOME`、临时 `PATH`、临时端口。

### 6.3 Shadow Mode：开发者主动只读检测

只读检测真实环境，例如 `rtk --version` / `headroom --version` / local health endpoint。

Shadow Mode 必须由开发者主动开启，不能作为默认行为。

### 6.4 External Mode：成品连接已有安装

适合已经手动装好 RTK/Headroom 的用户。d-token 只做检测、状态展示、analytics import、Dashboard、Doctor 和 route/project 管理。

External Mode 不做安装、升级、卸载和 hook rewrite，除非用户主动进入配置修改流程。

### 6.5 Managed Mode：成品一键覆盖安装

适合新用户或希望 d-token 托管工具生命周期的用户。

可做：install、verify、configure、start/stop、update、uninstall、rollback、hook setup、shell/profile patch、provider route patch。

必须走 InstallationTransaction：diff、backup、confirm、apply、doctor、rollback point。

### 6.6 Built-in Fallback Mode

当 RTK 不可用时，d-token 可提供基础 CLI output preprocessor：ANSI 清理、空行压缩、重复行折叠、超长输出截断、error-only mode。

该模式只能作为 fallback，不声称等价于完整 RTK。

## 7. RTK 适配范围

d-token 必须适配 RTK：

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
14. hook integrity verification。
15. project-local filter trust。
16. permission precedence：Deny > Ask > Allow > Default。

开发期适配原则：fixture/fake/shadow 优先，不碰真实环境。  
成品适配原则：Managed Mode 可安装和覆盖，但必须事务化。

## 8. Headroom 适配范围

必须保留并适配：Library / SDK、Local proxy、Agent wrap / unwrap、MCP server、Content-aware compression pipeline、ContentRouter、CacheAligner、SmartCrusher、CodeCompressor、Kompress/text compressor、CCR reversible compression、local retrieval、Cross-agent memory、Per-project memory、SharedContext、headroom learn、output shaper、verbosity steering、effort routing、durable savings ledger、metrics endpoints、Historical stats、JSON/CSV export、Cost tracking、Per-model/per-agent/per-project breakdown、Doctor/perf/benchmark/troubleshooting。

开发期适配原则：不读真实 CCR 原文、不改真实 Headroom config、不占真实端口。  
成品适配原则：可安装、启动、配置、更新和卸载，但必须可回滚。

## 8.1 Agent 覆盖要求

d-token 必须覆盖 RTK 与 Headroom 两个上游当前支持或明确列出的 agent 并集：

```text
Claude Code
VS Code Copilot Chat
GitHub Copilot CLI
Cursor
Gemini CLI
OpenCode
OpenClaw
Pi
Hermes
Cline / Roo Code
Windsurf
Codex CLI
Kilo Code
Google Antigravity
Mistral Vibe
Aider
Continue
Goose
OpenHands
Cortex Code
```

每个 agent 在 GUI、Doctor 与 CompatibilityMatrix 中至少展示：

1. RTK 是否支持。
2. RTK 集成方式：hook、plugin、rules、instructions、planned。
3. Headroom 是否支持。
4. Headroom 集成方式：wrap、manual proxy、library only、memory/learn only。
5. macOS 支持等级。
6. Windows native 支持等级。
7. Windows WSL 支持等级。
8. 限制说明。

## 8.2 OpenAI / Anthropic 协议自动注入要求

Headroom 能通过 proxy / SDK / middleware 接入 OpenAI-compatible 与 Anthropic-compatible 流量，d-token 必须把这类“协议级接入”作为一等能力，而不是只按具体 agent 列表处理。

开发期：

1. 默认只用 fixture 表达 OpenAI / Anthropic injection plan。
2. 不探测真实 Headroom 端口。
3. 不修改真实 provider/base URL 配置。
4. 不发送真实请求。

成品：

1. OpenAI-compatible client 可通过 base URL / proxy route / SDK wrapper 接入。
2. Anthropic-compatible client 可通过 messages proxy / route adapter 接入。
3. 所有注入必须经过 diff、backup、confirm、doctor、rollback point。
4. route decision 和 header stripping 必须可审计。
5. CCR 原文默认不读，只展示摘要。

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
  output_verbosity: 'auto' | 'terse' | 'normal' | 'detailed';
  holdout_ratio?: number;
  notes?: string;
}
```

### 9.3 CompressionRecommendationEngine

推荐等级必须基于：项目语言栈、最近请求 token 规模、命令输出噪音比例、Headroom 历史 savings、RTK 历史 savings、LLM 回答质量反馈、retrieve 调用频率、失败率与回退率、用户手动覆盖偏好、高风险任务标记。

```text
safe:       默认推荐，适合新项目和高风险任务
balanced:   常规编码任务推荐
aggressive: 大日志、大测试输出、大搜索结果推荐
max:        只对低风险探索/批量日志/RAG chunks 推荐，必须提示可能损失细节
```

## 10. 分层压缩指标模型

```text
raw_cli_tokens
  → rtk_cleaned_tokens
  → headroom_input_tokens
  → headroom_final_tokens

rtk_savings = raw_cli_tokens - rtk_cleaned_tokens
headroom_savings = headroom_input_tokens - headroom_final_tokens
end_to_end_savings = raw_total_context_tokens - final_llm_input_tokens
```

禁止把 `rtk_savings + headroom_savings` 直接相加当作总 savings，除非 denominator 和 baseline 已经统一。

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
  stage: 'rtk_rewrite' | 'rtk_filter' | 'headroom_route' | 'headroom_compress' | 'headroom_retrieve' | 'provider_forward' | 'install_step' | 'config_patch';
  source: 'rtk' | 'headroom' | 'dtoken';
  status: 'ok' | 'passthrough' | 'failed' | 'disabled' | 'skipped';
  input_tokens?: number;
  output_tokens?: number;
  saved_tokens?: number;
  baseline_kind?: 'raw_cli' | 'rtk_cleaned' | 'headroom_input' | 'final_llm_input';
  error_code?: string;
  metadata?: Record<string, unknown>;
}
```

开发期 Event Ledger 不保存 full prompt、full response、full file content、full command output。成品默认也不保存，除非用户显式开启本地调试模式。

## 12. 技术选型

### 12.1 本地核心

采用：

```text
Rust core + external RTK integration + external Headroom integration + optional Python sidecar transitional architecture
```

Rust 负责 process manager、local API、proxy/router、logs、config、port manager、doctor、RTK adapter、Headroom adapter、project policy manager、event ledger、config patcher、installer transaction。

### 12.2 桌面 App

桌面 App 技术路线：

```text
Tauri 2 + React + TypeScript + Vite
```

开发期 GUI 默认连接 fixture/fake 数据。成品 GUI 默认进入安装向导或现有安装连接向导。

## 13. Doctor 要求

### 13.1 开发期 Doctor

开发期 Doctor 必须支持 no-touch 检查：

1. 当前 profile 是否为 fixture/fake/shadow。
2. 是否误调用真实 binary。
3. 是否使用测试端口。
4. 是否使用临时 HOME。
5. 是否存在外部配置写入风险。

### 13.2 成品 Doctor

成品 Doctor 必须检查：Local proxy、端口、route、provider、chain bridge、project policy、config backup/rollback、event ledger、installation transaction、RTK binary/version/hook/integrity、Headroom CLI/proxy/MCP/stats/CCR summary。

## 14. 安全与隐私原则

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

## 15. CompatibilityMatrix

必须维护 OS / arch / shell / agent / RTK mode / Headroom mode 的兼容矩阵：

```ts
interface CompatibilityCase {
  id: string;
  os: 'macos' | 'windows' | 'linux';
  arch: 'x64' | 'arm64';
  shell: 'bash' | 'zsh' | 'powershell' | 'cmd' | 'wsl-bash';
  agent: string;
  rtk_mode: 'external' | 'managed' | 'builtin' | 'disabled';
  headroom_mode: 'external' | 'managed' | 'disabled';
  expected_hook_support: 'full' | 'partial' | 'none';
  expected_proxy_support: 'full' | 'partial' | 'none';
  status: 'untested' | 'pass' | 'fail' | 'blocked';
  notes?: string;
}
```

Mac 与 Windows 都是首要目标平台。Windows 必须区分 native 与 WSL：native 模式通常只能获得部分 hook/规则能力，WSL 才能获得完整 shell hook 体验。

## 16. 目录结构

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

## 17. 里程碑：Dev No-touch + Production Managed Install Roadmap

### M0：文档、研究与路线收敛

1. 单一 living PRD。
2. 技术选型文档。
3. 架构文档。
4. 安全边界文档。
5. RTK/Headroom 深度研究报告。
6. PRD 多维审查报告。
7. 仓库目录收敛。

### M1：No-touch Dev Foundation

目标：开发期完全不破坏本机已有环境。

1. RTK/Headroom fixtures。
2. fake RTK / fake Headroom binary。
3. fixture mode。
4. fake-binary mode。
5. RTK adapter skeleton。
6. Headroom adapter skeleton。
7. ProjectPolicy skeleton。
8. ConfigPatcher skeleton，但默认只 plan 不 apply。
9. EventLedger skeleton。
10. local API skeleton。
11. 最小 CI：structure check、cargo check、cargo test。

M1 验收：d-token 能在不调用真实 RTK/Headroom、不读取真实 agent 配置、不修改真实环境的前提下完成基础 API 与测试。

### M2：Real External Read-only Integration

目标：在用户主动开启 shadow/external 时，只读接入真实本体。

1. 真实解析 `rtk gain --all --format json`。
2. 真实解析 `rtk discover`。
3. 真实解析 `rtk session`。
4. 真实检测 RTK hook/integrity/telemetry。
5. 真实读取 Headroom health/stats/history/metrics。
6. 真实检测 Headroom proxy/MCP/CCR summary。
7. EventLedger JSONL 持久化。
8. layered savings attribution v0。
9. Doctor 能诊断 RTK/Headroom/provider/route/config/platform。

M2 验收：d-token 可以只读连接真实现有安装，但不会写配置或控制服务。

### M3：GUI Control Plane MVP

目标：开始图形化，但 GUI 开发默认 fixture，不破坏开发者本机环境。

1. Tauri shell。
2. React Dashboard。
3. Fixture Demo Mode 默认入口。
4. Fake Binary Test Mode。
5. Shadow Detect Mode，用户主动。
6. RTK Status cards。
7. Headroom Status cards。
8. Layered savings cards。
9. Project compression policy cards。
10. Doctor UI。
11. Compatibility warning cards。
12. Config diff / backup / rollback UI。

M3 验收：开发期 GUI 不碰真实环境；用户主动 shadow/external 时只读连接。

### M4：Production Managed Installer

目标：成品支持一键覆盖安装。

1. InstallationTransaction。
2. Tool selection wizard。
3. Managed RTK install/update/uninstall。
4. Managed Headroom install/update/uninstall。
5. d-token proxy install/update/uninstall。
6. Agent hook setup。
7. Shell profile patch。
8. Provider route patch。
9. Config diff / backup / apply / rollback。
10. Install Doctor。
11. rollback point。
12. version provenance。

M4 验收：成品可以覆盖安装和配置，但所有写操作都可追踪、可备份、可回滚。

### M5：Production Hardening + Release

目标：正式发布。

1. Windows installer。
2. macOS bundle。
3. updater。
4. signing / notarization。
5. benchmark / evals。
6. crash recovery。
7. RTK hook compatibility matrix。
8. RTK install/uninstall rollback。
9. Headroom install/uninstall rollback。
10. privacy/export/redaction audit。
11. CI gate：structure check、cargo check、cargo test、no-touch smoke。
12. E2E compatibility test suite。
13. User guide / developer guide / troubleshooting guide。

M5 验收：可以放心让外部用户下载、覆盖安装、回滚、升级和排障。

## 18. 验收标准

1. 仓库只有一个 PRD：`docs/PRD.md`。
2. 所有正式文档都在 `docs/`。
3. 所有正式代码都在 `code/`。
4. 所有测试、脚本、fixture、fake-bin 都在 `support/`。
5. 开发期默认 no-touch，不破坏开发者本机已有 agents/RTK/Headroom。
6. 成品支持覆盖安装，但必须有事务、备份、校验和回滚。
7. Headroom 功能迁移不能删项。
8. Dashboard 迁移不能缩水。
9. RTK 与 Headroom-compatible core 必须可组合、可单独启停、可单独观测。
10. Dashboard 必须能区分 RTK 命令输出预处理收益、Headroom 上下文压缩收益、端到端总收益。
11. 用户必须能按项目自定义 Headroom 压缩等级，并查看推荐等级与推荐理由。
12. 用户必须能选择 External Mode 或 Managed Mode。
13. 所有 agent/RTK/Headroom/shell 配置修改必须支持 diff、backup、rollback。
14. CI 必须至少覆盖 structure check、cargo check、cargo test、no-touch smoke。
15. M3 GUI 开发默认 fixture，不允许默认扫描或修改真实环境。
16. M4/M5 成品安装器允许覆盖安装，但必须可追踪、可解释、可恢复。
