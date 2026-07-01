# d-token Living PRD

状态：Living Document  
唯一规则：本文件是 d-token 的唯一 PRD。后续任何产品目标、边界、技术路线、目录结构调整，都直接修改本文件，不再另起 v0.1/v0.2/v0.3 多版本 PRD。

## 1. 项目一句话

`d-token` 是一个面向 AI Coding 的本地分层压缩、路由与中转适配系统：先把 `rtk-ai/rtk` 与 Headroom-compatible 能力在本地跑通、迁移、修复和增强，再通过 Windows/macOS 桌面 App 提供一键安装、项目级压缩策略、Dashboard、Doctor、rollback 与端到端 token savings attribution。

## 2. 核心路线纠正

### 2.1 正确路线

本项目采用 **Ability-first Roadmap**。

正确顺序是：

```text
先克隆/引入 RTK 与 Headroom
  ↓
先把二者在本地跑通
  ↓
先迁移和修复我们发现的问题
  ↓
先完成中转适配、压缩链路、项目策略、事件账本
  ↓
能力稳定后再做桌面 App
  ↓
最后做安装器、签名、自动更新和正式发布
```

禁止出现：

```text
底层能力还不可用，但先做漂亮桌面 UI
```

桌面 App 是能力成熟后的产品化外壳，不是早期第一目标。

### 2.2 分层关系

RTK 与 Headroom-compatible core 工作在不同链路层级：

```text
RTK:      Command Execution Layer / Shell Output Layer
Headroom: API Gateway Layer / LLM Context Layer
d-token:  Local Orchestration / Route Adapter / Ability Integration Layer
Desktop:  Productization Layer, only after core ability is usable
```

RTK 负责在命令输出进入 Agent 上下文前做第一层清洗；Headroom-compatible core 负责在完整上下文发往 LLM 前做第二层压缩；d-token 负责把两者本地化、适配化、可观测化、可配置化、可回滚化。

## 3. 最终产品形态

```text
AI Coding Tool
  ↓
Shell Command Execution
  ↓
RTK CLI Proxy / Hook / Plugin
  ↓
Cleaned CLI Output enters Agent Context
  ↓
d-token Local Proxy / Route Adapter
  ↓
Headroom-compatible Compression Core
  ↓
Route Decision
  ↓
Official Provider / Custom Provider / Chain Bridge
  ↓
Provider
  ↓
Metrics / Event Ledger / Dashboard
```

桌面 App 最终用于：

1. 一键安装。
2. 工具选择。
3. Provider / Route / Instance 配置。
4. RTK / Headroom 状态查看。
5. ProjectCompressionPolicy 管理。
6. Dashboard。
7. Doctor。
8. Config diff / backup / rollback。

但桌面 App 不应早于核心能力完成。

## 4. 产品目标

1. 保留 Headroom 的完整能力，不做删减版。
2. 将 `rtk-ai/rtk` 作为一等集成对象，而不是泛泛的“CLI 清洗脚本”。
3. 先完成 RTK 与 Headroom 的本地克隆、能力分析、兼容性验证、bug 修复和适配增强。
4. 先完成中转适配链路：AI Coding Tool → d-token local proxy → Headroom-compatible compression → provider / chain bridge。
5. 支持 RTK command-output savings、Headroom context savings、end-to-end savings 分层统计。
6. 支持 Headroom 每项目压缩等级、推荐等级、输出 token shaping、memory、learn、MCP、CCR 开关。
7. 支持 Claude Code、Codex、Cline、OpenCode、Cursor、Windsurf、Gemini CLI、GitHub Copilot、Kilo Code、Antigravity 等 AI Coding 工具接入。
8. 支持官方 provider、第三方 compatible provider、chain bridge 三类路径。
9. 支持多个本地实例并行运行，并在数据层按 project / agent / instance / route / provider 拆分。
10. 能力稳定后，完整迁移 Headroom Dashboard 并新增 RTK / d-token 分层卡片。
11. 能力稳定后，提供 Windows/macOS 桌面 App 和一键安装向导。

## 5. 外部项目学习结论

### 5.1 RTK 是什么

`rtk-ai/rtk` 是一个高性能 CLI proxy，用于在命令输出进入 LLM 上下文前过滤和压缩命令输出。d-token 必须理解并适配它的关键能力：

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

### 5.2 RTK 的设计原则

d-token 适配 RTK 时必须保留以下工程原则：

1. Single responsibility。
2. Minimal overhead。
3. Exit code preservation。
4. Fail-safe passthrough。
5. Transparent raw-output recovery。
6. Local-first tracking。
7. Integrity-aware hook status。
8. Permission-respecting，不绕过 host agent 权限模型。

### 5.3 Headroom 是什么

Headroom 是 context compression layer for AI agents，目标是在内容到达 LLM 前压缩 tool outputs、logs、RAG chunks、files、conversation history，并提供 library、proxy、agent wrap、MCP server、CCR、cross-agent memory、learn、output token reduction、dashboard、doctor、perf、metrics 等能力。

### 5.4 Headroom 必须保留的能力

必须保留：Library / SDK、Local proxy、Agent wrap / unwrap、MCP server、Content-aware compression pipeline、ContentRouter、CacheAligner、SmartCrusher、CodeCompressor、Kompress/text compressor、CCR reversible compression、local retrieval、Cross-agent memory、Per-project memory、SharedContext、headroom learn、output shaper、verbosity steering、effort routing、durable savings ledger、metrics endpoints、Historical stats、JSON/CSV export、Cost tracking、Per-model/per-agent/per-project breakdown、Doctor/perf/benchmark/troubleshooting。

### 5.5 Headroom 安全机制

d-token 不得破坏 Headroom 的安全设计：

1. live-zone-only compression：不能粗暴压缩全部历史上下文。
2. never remove human content。
3. parse failure no-op。
4. error preservation。
5. CCR reversible retrieval。
6. cache hot zone stability。
7. local-first store。
8. MCP compress / retrieve / stats 三类路径清晰可观测。

## 6. 明确边界

### 6.1 必须做

1. 克隆/引入 RTK 与 Headroom 到本地研发环境，并 pin upstream ref。
2. 建立 upstream baseline：版本、license、测试结果、可运行命令、已知问题。
3. 建立 clean-room / adapter-first 策略：优先适配，不直接复制外部实现；若复用 Apache 2.0 代码，必须保留 license notice。
4. Headroom-compatible core。
5. RTK integration layer。
6. Local proxy / route adapter。
7. ProviderProfile。
8. RouteProfile。
9. InstanceProfile。
10. ProjectProfile。
11. ProjectCompressionPolicy。
12. CompressionRecommendationEngine。
13. PreprocessorProfile。
14. LayeredCompressionStage。
15. DtokenEvent / event ledger。
16. Config diff / backup / rollback。
17. CompatibilityMatrix。
18. Doctor。
19. JSON/CSV export。
20. 桌面 App，但必须在核心能力可用后再进入。

### 6.2 不做

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
12. 不强制打开 RTK telemetry。
13. 不把 RTK 与 Headroom-compatible core 强耦合；两者必须可单独启用、单独关闭、单独观测。
14. 不在默认日志中保存完整命令输出、完整文件内容、完整 prompt 或完整 API response。
15. 不默认读取 RTK tracking DB、tee 文件或 Headroom CCR 原文。
16. 不在未形成统一 baseline 前把 RTK savings 与 Headroom savings 直接相加。
17. 不在核心能力未验证前投入复杂桌面 UI。

## 7. 本地能力优先的工程策略

### 7.1 上游源码处理

RTK 与 Headroom 的本地化策略：

```text
external upstream repo
  ↓
local clone / submodule / pinned source snapshot
  ↓
license review
  ↓
baseline test
  ↓
adapter integration
  ↓
bug reproduction
  ↓
patch or wrapper fix
  ↓
d-token compatibility layer
```

推荐目录策略：

```text
code/upstreams/rtk/          仅用于本地研究或 submodule，不直接改业务代码
code/upstreams/headroom/     仅用于本地研究或 submodule，不直接改业务代码
code/crates/*                d-token Rust core
code/runtimes/headroom/      Headroom-compatible runtime / adapter
support/upstream-notes/      上游差异、bug、patch、license 记录
```

如果不使用 submodule，而是仅本地 clone，不提交完整第三方源码，也必须在 `support/upstream-notes/` 记录 upstream ref、license、commit hash、测试结果。

### 7.2 能力完成标准

桌面开发前，至少需要满足：

1. RTK external mode 可以检测、运行、导入 gain/discover/session。
2. Headroom external mode 可以检测、启动 proxy、读取 stats、验证 MCP。
3. d-token local proxy 能接收请求并转发 provider。
4. d-token 能在至少一个真实 agent 场景跑通。
5. ProjectCompressionPolicy 能实际影响 Headroom compression behavior。
6. Event ledger 能记录 RTK stage、Headroom stage、provider stage。
7. Doctor 能定位 RTK/Headroom/proxy/provider/config 问题。
8. Config patcher 能对测试配置执行 diff/backup/rollback。
9. 至少一个端到端 demo 可以运行。

## 8. Headroom 项目级压缩策略

### 8.1 ProjectProfile

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

### 8.2 ProjectCompressionPolicy

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

### 8.3 CompressionRecommendationEngine

推荐等级必须基于：项目语言栈、最近请求 token 规模、命令输出噪音比例、Headroom 历史 savings、RTK 历史 savings、LLM 回答质量反馈、retrieve 调用频率、失败率与回退率、用户手动覆盖偏好、高风险任务标记。

```text
safe:       默认推荐，适合新项目和高风险任务
balanced:   常规编码任务推荐
aggressive: 大日志、大测试输出、大搜索结果推荐
max:        只对低风险探索/批量日志/RAG chunks 推荐，必须提示可能损失细节
```

## 9. RTK 集成设计

### 9.1 d-token 与 RTK 的职责边界

```text
RTK owns:
  command rewrite
  command execution proxy
  command-specific filtering
  raw-output tee
  local RTK analytics
  hook integrity verification
  project-local filter trust

d-token owns:
  status detection
  metrics ingestion
  layered savings attribution
  route/provider/instance/project management
  diff/backup/rollback UX
  desktop orchestration after core ability works
```

### 9.2 RTK 集成模式

1. External RTK：用户自己安装 RTK，d-token 只检测、展示、导入公开 analytics。这是优先模式。
2. Managed RTK Sidecar：d-token 下载或打包 RTK binary，并通过 Desktop 管理安装、升级、卸载、hook audit、analytics import。必须用户确认。
3. Built-in Preprocessor：RTK 不可用时提供基础 fallback，不声称等价于完整 RTK。

## 10. 分层压缩指标模型

### 10.1 三类 savings

Dashboard 必须区分：

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
Rust core + Python sidecar transitional architecture + RTK external binary integration
```

短期：

- Rust 负责 process manager、local API、proxy/router skeleton、logs、config、port manager、doctor、RTK adapter interface、project policy manager、event ledger、config patcher。
- Python sidecar 保留 Headroom-compatible compression、CCR、memory、learn、MCP 等能力。
- RTK 优先以外部 binary 接入，不在 M1/M2 复制其 filters。

长期：

- 稳定的 proxy、router、metrics、ledger、config、process manager、RTK metrics ingestion、project policy manager 逐步 Rust 化。
- 高级压缩、ML、learn、evals 可继续保留 Python sidecar，直到 Rust parity 足够可靠。
- 若将 RTK 或 Headroom 功能内置，必须另做 license review、feature parity matrix 和 compatibility test suite。

### 11.2 桌面 App

桌面 App 技术路线仍采用：

```text
Tauri 2 + React + TypeScript + Vite
```

但桌面开发必须后置到 M4。桌面只负责产品化，不负责在能力未完成时“假装可用”。

## 12. Doctor 要求

### 12.1 d-token Core

Doctor 必须检查：Local proxy 是否运行、端口是否占用、route 是否匹配、provider 是否健康、chain bridge 是否健康、Headroom-compatible runtime 是否可用、project policy 是否有效、config backup/rollback 是否可用、event ledger 是否可写、installation transaction 是否存在半完成状态。

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
20. 所有 managed binary 必须记录来源与校验信息。

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

## 16. 里程碑：Ability-first Roadmap

### M0：文档、研究与路线收敛

1. 单一 living PRD。
2. 技术选型文档。
3. 架构文档。
4. 安全边界文档。
5. RTK/Headroom 深度研究报告。
6. PRD 多维审查报告。
7. 仓库目录收敛。

### M1：Upstream Clone + Core Skeleton + Baseline Verification

目标：先把 RTK 与 Headroom 的上游能力“拉到本地、跑起来、看清楚”。

1. 克隆或引入 `rtk-ai/rtk`。
2. 克隆或引入 Headroom。
3. 记录 upstream commit hash、license、安装方式、测试方式。
4. 跑通 RTK 基本命令：version、init/show、gain、discover、session。
5. 跑通 Headroom 基本命令：version、doctor、proxy/stats、MCP status。
6. Rust workspace。
7. local API skeleton。
8. proxy skeleton。
9. config loader。
10. process manager skeleton。
11. doctor skeleton。
12. RTK adapter skeleton。
13. Headroom adapter skeleton。
14. ProjectPolicy skeleton。
15. ConfigPatcher skeleton。
16. EventLedger skeleton。
17. 最小 CI：structure check、cargo check、cargo test。

M1 验收：上游项目能在本地跑通，d-token skeleton 能编译，能通过 API 返回 RTK/Headroom status mock 或真实 detection。

### M2：Ability Migration + Bug Fix + Adapter Hardening

目标：不做桌面，先让能力真实可用。

1. 真实解析 `rtk gain --all --format json`。
2. 真实解析 `rtk discover`。
3. 真实解析 `rtk session`。
4. RTK hook status / integrity detection。
5. RTK Windows native / WSL limitation detection。
6. Headroom CLI / proxy / stats / MCP / CCR summary detection。
7. Headroom project compression policy 映射到真实 runtime knobs。
8. 复现并记录我们发现的 RTK / Headroom / d-token 兼容问题。
9. 对问题分类：upstream bug、adapter bug、usage limitation、Windows/macOS limitation。
10. 实现 adapter-level fixes 或 upstream patch proposal。
11. 完成 ConfigPatcher 的真实 diff / backup / rollback skeleton。
12. 完成 EventLedger JSONL 持久化。
13. 完成第一版 CompatibilityMatrix。

M2 验收：命令行和 API 层已经能真实读取 RTK/Headroom 状态与统计，能定位主要问题，能安全 patch/rollback 测试配置。

### M3：End-to-End Functional Alpha

目标：让 d-token 在没有桌面 UI 的情况下，已经能真正工作。

1. d-token local proxy 能接收真实 AI Coding 工具请求。
2. d-token 能把请求转给 Headroom-compatible compression。
3. d-token 能转发到 official provider / custom provider / chain bridge。
4. RTK → Agent context → d-token proxy → Headroom → provider 路径跑通。
5. ProjectCompressionPolicy 能实际影响压缩策略。
6. EventLedger 记录 RTK stage、Headroom stage、provider stage。
7. 防重复 savings attribution 生效。
8. Doctor 能诊断 RTK、Headroom、provider、route、config、port、platform 问题。
9. CLI-first demo 能跑通至少一个真实项目。
10. 完成端到端 benchmark：tokens、latency、error、quality notes。

M3 验收：即使没有桌面 App，d-token 也已经是一个可用的本地 CLI/proxy 工具。

### M4：Desktop Productization

目标：在能力可用后，把它产品化为 Windows/macOS 图形 App。

1. Tauri shell。
2. React Dashboard。
3. 一键安装向导。
4. Tool selection cards。
5. Headroom Dashboard 全卡片迁移。
6. RTK Dashboard cards。
7. Project compression policy cards。
8. Layered compression cards。
9. InstallationTransaction cards。
10. Compatibility warning cards。
11. Config diff / backup / rollback UI。
12. Doctor UI。
13. Logs / Export UI。
14. mock 数据替换为 M3 真实 API。

M4 验收：桌面 App 是真实能力的 UI，而不是 mock 壳。

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
5. Headroom 功能迁移不能删项。
6. Dashboard 迁移不能缩水。
7. Windows/macOS App 技术路线明确，但不得早于核心能力完成。
8. RTK 与 Headroom-compatible core 必须可组合、可单独启停、可单独观测。
9. Dashboard 必须能区分 RTK 命令输出预处理收益、Headroom 上下文压缩收益、端到端总收益。
10. 用户必须能按项目自定义 Headroom 压缩等级，并查看推荐等级与推荐理由。
11. 用户必须能通过安装向导自定义安装 d-token / RTK / Headroom / MCP / hooks 的任意组合。
12. 所有 agent/RTK/Headroom/shell 配置修改必须支持 diff、backup、rollback。
13. CI 必须至少覆盖 structure check、cargo check、cargo test。
14. M3 前不得把桌面 UI 作为主交付物。
15. M4 桌面 App 必须连接 M3 真实能力，不接受纯 mock 产品化。
