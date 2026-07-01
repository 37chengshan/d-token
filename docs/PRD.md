# d-token Living PRD

状态：Living Document  
唯一规则：本文件是 d-token 的唯一 PRD。后续任何产品目标、边界、技术路线、目录结构调整，都直接修改本文件，不再另起 v0.1/v0.2/v0.3 多版本 PRD。

## 1. 项目一句话

`d-token` 是一个面向 AI Coding 的本地分层压缩、路由、中转适配与图形化控制面：默认 **External-first / No-touch** 接入用户本地已经安装并跑通的 `rtk-ai/rtk` 与 Headroom，本项目不重新部署、不覆盖配置、不静默修改 hook，而是把两者能力合并为统一的本地控制层、Dashboard、项目级策略、Doctor、配置备份/回滚与端到端 token savings attribution。

## 2. 核心路线纠正：No-touch External-first

### 2.1 当前真实前提

用户本地的 agents、RTK、Headroom 已经安装好，并且已经能跑通。

因此 d-token 当前阶段不应再把重点放在：

```text
重新克隆上游
重新安装 RTK
重新安装 Headroom
重新改用户已有 hook
重新覆盖用户配置
```

当前重点应该是：

```text
发现已有安装
  ↓
只读检测状态
  ↓
导入公开 metrics / stats / analytics
  ↓
合并 RTK + Headroom 能力为统一控制面
  ↓
补中转适配、项目策略、事件账本、分层统计
  ↓
图形化展示和配置管理
  ↓
只有用户主动选择 managed mode 时才执行安装/修改/卸载
```

### 2.2 不动用户本地环境的原则

默认模式是 External Mode：

1. 不重新安装 RTK。
2. 不重新安装 Headroom。
3. 不改用户已有 agent hook。
4. 不改 shell profile。
5. 不改 RTK config。
6. 不改 Headroom config。
7. 不读完整 tee 文件。
8. 不读 Headroom CCR 原文缓存。
9. 不上传任何本地数据。
10. 只通过官方 CLI、HTTP endpoint、stats、metrics 或只读本地摘要读取状态。

任何写操作必须满足：

```text
用户主动点击
  ↓
生成 patch plan
  ↓
展示 diff
  ↓
创建 backup
  ↓
用户二次确认
  ↓
apply
  ↓
doctor
  ↓
可 rollback
```

### 2.3 合并的含义

这里的“合并”不是一开始把 RTK 和 Headroom 源码揉成一个大项目，也不是替代它们的本体。

正确含义是：

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
```

## 3. 最终产品形态

```text
AI Coding Tool
  ↓
Shell Command Execution
  ↓
Existing RTK CLI Proxy / Hook / Plugin
  ↓
Cleaned CLI Output enters Agent Context
  ↓
Existing Headroom or d-token Local Proxy / Route Adapter
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

## 4. 分层关系

```text
RTK:      Command Execution Layer / Shell Output Layer
Headroom: API Gateway Layer / LLM Context Layer
d-token:  Local Merge / Route Adapter / Control Plane / GUI Layer
```

RTK 负责命令输出进入 Agent 上下文前的第一层清洗。  
Headroom 负责完整上下文发往 LLM 前的第二层压缩。  
d-token 负责把已有 RTK/Headroom 能力合并为统一的本地控制层和图形化体验。

## 5. 产品目标

1. 默认接入用户已安装、已跑通的 RTK 与 Headroom。
2. 默认不重装、不覆盖、不静默修改用户环境。
3. 保留 Headroom 完整能力，不做删减版。
4. 将 `rtk-ai/rtk` 作为一等外部集成对象，而不是泛泛的“CLI 清洗脚本”。
5. 支持 RTK command-output savings、Headroom context savings、end-to-end savings 分层统计。
6. 支持 Headroom 每项目压缩等级、推荐等级、output shaping、memory、learn、MCP、CCR 开关。
7. 支持 Claude Code、Codex、Cline、OpenCode、Cursor、Windsurf、Gemini CLI、GitHub Copilot、Kilo Code、Antigravity 等 AI Coding 工具接入状态展示。
8. 支持官方 provider、第三方 compatible provider、chain bridge 三类路径。
9. 支持多个本地实例并行运行，并按 project / agent / instance / route / provider 拆分统计。
10. 提供统一 GUI：Dashboard、Doctor、Project Policy、RTK/Headroom Status、Config diff、Backup、Rollback、Logs、Export。
11. 只有在用户主动选择 Managed Mode 时，才提供安装、升级、卸载、hook setup 等能力。

## 6. 外部项目学习结论

### 6.1 RTK 是什么

`rtk-ai/rtk` 是一个高性能 CLI proxy，用于在命令输出进入 LLM 上下文前过滤和压缩命令输出。d-token 必须适配它的关键能力：

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

### 6.2 RTK 适配原则

1. External-first。
2. 只读检测优先。
3. 不修改 RTK DB。
4. 不自动开启 telemetry。
5. 不绕过 agent permission model。
6. 不静默安装 hook。
7. 不默认读取 tee 完整内容。
8. 不复制 RTK filters，除非后续单独做 license review 和 parity test。

### 6.3 Headroom 是什么

Headroom 是 context compression layer for AI agents，目标是在内容到达 LLM 前压缩 tool outputs、logs、RAG chunks、files、conversation history，并提供 library、proxy、agent wrap、MCP server、CCR、cross-agent memory、learn、output token reduction、dashboard、doctor、perf、metrics 等能力。

### 6.4 Headroom 必须保留的能力

必须保留：Library / SDK、Local proxy、Agent wrap / unwrap、MCP server、Content-aware compression pipeline、ContentRouter、CacheAligner、SmartCrusher、CodeCompressor、Kompress/text compressor、CCR reversible compression、local retrieval、Cross-agent memory、Per-project memory、SharedContext、headroom learn、output shaper、verbosity steering、effort routing、durable savings ledger、metrics endpoints、Historical stats、JSON/CSV export、Cost tracking、Per-model/per-agent/per-project breakdown、Doctor/perf/benchmark/troubleshooting。

### 6.5 Headroom 适配原则

1. External-first。
2. proxy/MCP/wrap/status 都应可观测。
3. 不默认读取 CCR 原文。
4. 不破坏 live-zone-only compression。
5. 不移除 human content。
6. parse failure no-op。
7. preserve errors。
8. 不与 Headroom provider fallback 产生不可解释重叠。
9. 项目级策略必须映射到真实 Headroom knobs，而不是只做假 UI 枚举。

## 7. 明确边界

### 7.1 必须做

1. External RTK detection：binary、version、hook status、integrity、telemetry、gain/discover/session。
2. External Headroom detection：CLI、proxy、stats、MCP、CCR summary、doctor、runtime status。
3. Local control plane。
4. Local proxy / route adapter。
5. ProviderProfile。
6. RouteProfile。
7. InstanceProfile。
8. ProjectProfile。
9. ProjectCompressionPolicy。
10. CompressionRecommendationEngine。
11. PreprocessorProfile。
12. LayeredCompressionStage。
13. DtokenEvent / event ledger。
14. Config diff / backup / rollback。
15. CompatibilityMatrix。
16. Doctor。
17. JSON/CSV export。
18. GUI Dashboard。
19. GUI Project Policy。
20. GUI RTK/Headroom status。
21. GUI config diff / backup / rollback。
22. Optional Managed Mode：仅用户主动选择时才安装/升级/卸载。

### 7.2 不做

1. 不默认重新安装 RTK。
2. 不默认重新安装 Headroom。
3. 不默认克隆上游源码到仓库。
4. 不默认修改用户本地已经跑通的 agent hook。
5. 不默认修改 shell profile。
6. 不默认修改 RTK config。
7. 不默认修改 Headroom config。
8. 不做云端账号托管。
9. 不做账号共享、账号池、转售或代登录。
10. 不做违反平台条款的自动化用途。
11. 不偷偷读取或迁移用户已有 token。
12. 不把用户代码、日志、prompt 上传到 d-token 服务器。
13. 不做公网 relay 服务，默认只做本地 `127.0.0.1`。
14. 不做静默 provider fallback；任何 fallback 都必须可见、可配置、可审计。
15. 不做 Dashboard 简化版。
16. 不删除 Headroom 功能。
17. 不强制用户必须安装 RTK；RTK 是增强层，Headroom-compatible core 必须可单独工作。
18. 不强制用户必须安装 Headroom runtime；RTK-only 模式也必须能工作。
19. 不强制打开 RTK telemetry。
20. 不把 RTK 与 Headroom-compatible core 强耦合；两者必须可单独启用、单独关闭、单独观测。
21. 不在默认日志中保存完整命令输出、完整文件内容、完整 prompt 或完整 API response。
22. 不默认读取 RTK tracking DB、tee 文件或 Headroom CCR 原文。
23. 不把 RTK savings 与 Headroom savings 直接相加。

## 8. 模式设计

### 8.1 External Mode 默认模式

适合当前用户：本地已经装好并跑通 RTK 与 Headroom。

d-token 只做：

1. detect。
2. status。
3. read-only analytics import。
4. dashboard。
5. doctor。
6. layered attribution。
7. project policy。
8. route/provider/instance management。

不做：

1. install。
2. upgrade。
3. uninstall。
4. silent patch。
5. telemetry enable。
6. hook rewrite。

### 8.2 Managed Mode 可选模式

适合新用户或希望 d-token 托管工具生命周期的用户。

d-token 可以做：

1. install。
2. verify。
3. configure。
4. start/stop。
5. update。
6. uninstall。
7. rollback。

前提：用户主动选择，并经过 diff / backup / confirmation / doctor / rollback point。

### 8.3 Built-in Fallback Mode

当 RTK 不可用时，d-token 可提供基础 CLI output preprocessor：ANSI 清理、空行压缩、重复行折叠、超长输出截断、error-only mode。

该模式只能作为 fallback，不声称等价于完整 RTK。

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

### 10.1 三类 savings

```text
rtk_savings
  命令输出从 raw_cli_output 到 cleaned_cli_output 的节省

headroom_savings
  Agent 上下文进入 d-token Local Proxy 后到最终 LLM input 的节省

end_to_end_savings
  raw_cli_output + files + conversation + tool results 到最终 LLM input 的总节省
```

### 10.2 防重复统计

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

### 10.3 DtokenEvent / Event Ledger

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

## 11. 技术选型

### 11.1 本地核心

采用：

```text
Rust core + external RTK integration + external Headroom integration + optional Python sidecar transitional architecture
```

短期：

- Rust 负责 process manager、local API、proxy/router skeleton、logs、config、port manager、doctor、RTK adapter interface、Headroom adapter interface、project policy manager、event ledger、config patcher。
- RTK 优先以 external binary 接入。
- Headroom 优先以 external CLI/proxy/MCP 接入。
- 不复制 RTK filters。
- 不复制 Headroom core。

长期：

- 稳定的 proxy、router、metrics、ledger、config、process manager、RTK metrics ingestion、project policy manager 逐步 Rust 化。
- 若将 RTK 或 Headroom 功能内置，必须另做 license review、feature parity matrix 和 compatibility test suite。

### 11.2 桌面 App

桌面 App 技术路线采用：

```text
Tauri 2 + React + TypeScript + Vite
```

但本轮目标是图形化合并现有本地能力，不是重新部署本体。GUI 应优先连接 External Mode 的真实 RTK/Headroom 状态和统计。

## 12. Doctor 要求

### 12.1 d-token Core

Doctor 必须检查：Local proxy 是否运行、端口是否占用、route 是否匹配、provider 是否健康、chain bridge 是否健康、project policy 是否有效、config backup/rollback 是否可用、event ledger 是否可写、是否存在半完成 transaction。

### 12.2 RTK

Doctor 必须检查：RTK binary 是否存在、RTK version、是否存在同名错误包或错误 binary、RTK hook 是否安装、hook integrity、当前 agent 是否支持 auto-rewrite、当前平台是否支持完整 hook、Windows 是否处于 WSL 环境、Native Windows 是否需要显式 `rtk ...` 调用、RTK config 是否存在、RTK tracking database 是否存在、RTK tee directory 是否存在、RTK telemetry 状态、`rtk gain --all --format json` 是否可执行。

### 12.3 Headroom

Doctor 必须检查：Headroom CLI 是否存在、Python runtime 是否满足、proxy 是否运行、wrap 状态、MCP server 状态、dashboard 数据端点、metrics/savings ledger、CCR retrieve 是否可用、CCR cache summary 是否可展示、project compression policy 是否生效、output shaper 是否生效、memory/learn 是否生效。

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
17. d-token 修改 agent/RTK/Headroom/shell 配置前必须显示 diff、创建 backup、支持 rollback。
18. d-token 不默认读取 Headroom CCR 原文缓存；仅显示摘要、大小、数量和检索状态。
19. d-token 的 event ledger 默认不保存 full prompt、full response、full file content、full command output。
20. Managed binary 必须记录来源与校验信息。

## 14. CompatibilityMatrix

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

Dashboard 与 Doctor 必须把 compatibility warning 显示给用户，而不是只在日志里写。

## 15. 目录结构

仓库根目录允许：

```text
README.md
AGENTS.md
CLAUDE.md
docs/
code/
support/
.github/workflows/   平台必要例外，仅用于 CI
.gitignore
```

目录含义：

```text
README.md  面向 GitHub 访问者的项目入口介绍
docs/      所有正式文档，包含唯一 PRD、架构、技术决策、安全边界、审查报告、研究报告
code/      所有应用代码、crate、package、sidecar、runtime、配置样例
support/   测试、脚本、开发辅助、CI 模板、临时杂项
```

## 16. 里程碑：No-touch Merge + GUI Roadmap

### M0：文档、研究与路线收敛

1. 单一 living PRD。
2. 技术选型文档。
3. 架构文档。
4. 安全边界文档。
5. RTK/Headroom 深度研究报告。
6. PRD 多维审查报告。
7. 仓库目录收敛。

### M1：External Detection + Read-only Adapter Skeleton

目标：不动用户本地已跑通环境，只做发现和只读接入。

1. RTK binary/version detection。
2. RTK missing/found/wrong-package status。
3. RTK telemetry/hook/integrity status skeleton。
4. RTK gain/discover/session mock 或只读命令入口。
5. Headroom CLI/version detection。
6. Headroom proxy/MCP/stats/cache summary skeleton。
7. ProjectPolicy skeleton。
8. ConfigPatcher skeleton，但默认不 apply。
9. EventLedger skeleton。
10. local API skeleton。
11. 最小 CI：structure check、cargo check、cargo test。

M1 验收：d-token 能在不改用户本地环境的前提下识别 RTK/Headroom，并暴露状态 API。

### M2：Unified Merge Layer + Real External Integration

目标：把已有 RTK/Headroom 能力合并到 d-token 控制层。

1. 真实解析 `rtk gain --all --format json`。
2. 真实解析 `rtk discover`。
3. 真实解析 `rtk session`。
4. 真实检测 RTK hook/integrity/telemetry。
5. 真实读取 Headroom health/stats/history/metrics。
6. 真实检测 Headroom proxy/MCP/CCR summary。
7. EventLedger JSONL 持久化。
8. layered savings attribution v0。
9. ProjectCompressionPolicy 连接真实 Headroom knobs。
10. Doctor 能诊断 RTK/Headroom/provider/route/config/platform。
11. ConfigPatcher 能生成 diff/backup/rollback plan，但仍需用户确认才 apply。

M2 验收：d-token 已经成为 RTK + Headroom 的统一 CLI/API 控制层，仍不强制修改本地环境。

### M3：GUI Control Plane MVP

目标：开始图形化，但 GUI 连接 M2 真实能力，不做纯 mock 壳。

1. Tauri shell。
2. React Dashboard。
3. RTK Status cards。
4. Headroom Status cards。
5. Layered savings cards。
6. Project compression policy cards。
7. Doctor UI。
8. Compatibility warning cards。
9. Logs / Export UI。
10. Config diff / backup / rollback UI。
11. Tool selection：External Mode 默认。
12. Managed Mode 显示但默认关闭。

M3 验收：桌面 App 可以读取真实 RTK/Headroom 状态与统计，能展示分层 savings 和项目策略，但不会默认修改用户环境。

### M4：Optional Managed Mode + One-click Setup

目标：给新用户或希望托管工具的用户提供一键安装配置，但不影响 External Mode 用户。

1. InstallationTransaction。
2. Tool selection wizard。
3. Managed RTK install/update/uninstall。
4. Managed Headroom install/update/uninstall。
5. Agent hook setup。
6. Shell profile patch。
7. Config diff / backup / apply / rollback。
8. Install Doctor。
9. rollback point。
10. version provenance。

M4 验收：用户可以选择一键安装，但默认不对已有环境做任何修改。

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
11. CI gate：structure check、cargo check、cargo test。
12. E2E compatibility test suite。
13. User guide / developer guide / troubleshooting guide。

M5 验收：可以放心让外部用户下载、安装、回滚、升级和排障。

## 17. 验收标准

1. 仓库只有一个 PRD：`docs/PRD.md`。
2. 所有正式文档都在 `docs/`。
3. 所有正式代码都在 `code/`。
4. 所有测试、脚本、杂项都在 `support/`。
5. 默认 External Mode，不动用户本地已经跑通的 RTK/Headroom。
6. Headroom 功能迁移不能删项。
7. Dashboard 迁移不能缩水。
8. Windows/macOS App 技术路线明确，但必须优先连接真实 external 能力。
9. RTK 与 Headroom-compatible core 必须可组合、可单独启停、可单独观测。
10. Dashboard 必须能区分 RTK 命令输出预处理收益、Headroom 上下文压缩收益、端到端总收益。
11. 用户必须能按项目自定义 Headroom 压缩等级，并查看推荐等级与推荐理由。
12. 用户必须能选择 External Mode 或 Managed Mode。
13. 所有 agent/RTK/Headroom/shell 配置修改必须支持 diff、backup、rollback。
14. CI 必须至少覆盖 structure check、cargo check、cargo test。
15. M3 GUI 必须连接 M2 真实 external integration，不接受纯 mock 产品化。
16. Managed install/setup 只能在 M4 作为可选模式出现，不得影响已跑通的本地环境。
