# d-token Living PRD

状态：Living Document  
唯一规则：本文件是 d-token 的唯一 PRD。后续任何产品目标、边界、技术路线、目录结构调整，都直接修改本文件，不再另起 v0.1/v0.2/v0.3 多版本 PRD。

## 1. 项目一句话

d-token 是一个 Headroom-compatible 的本地 AI Coding 压缩路由桌面系统：保留 Headroom 全部核心能力，完整迁移 Headroom Dashboard，并新增 RTK/CLI 输出预处理层、provider、route、instance、chain bridge、Windows/macOS 原生 App 和中文优先体验。

## 2. 期望目标

### 2.1 产品目标

1. 保留 Headroom 的完整能力，不做删减版。
2. 把 Headroom Dashboard 完整迁移为 Windows/macOS 桌面 App。
3. 支持 Claude、Codex、Cline、OpenCode、Cursor、Aider、Continue、Goose、OpenHands 等 AI Coding 工具接入。
4. 支持官方 provider、第三方 compatible provider、chain bridge 三类路径。
5. 支持多个本地实例并行运行，并在 Dashboard 中按实例拆分数据。
6. 保证所有请求链路可观测：agent、instance、provider、route、chain mode、tokens、latency、error、transforms。
7. 保证 Windows/macOS 用户可以通过图形界面完成安装、配置、启动、停止、诊断、导出。
8. 支持分层压缩架构：RTK/CLI Output Preprocessor 负责命令执行层输出清洗，Headroom-compatible core 负责 API 接入层上下文压缩。
9. 支持“CLI 场景极致优化 + 全场景通用压缩”的组合收益，并在 Dashboard 中区分两层压缩贡献。

### 2.2 用户体验目标

1. 中文优先。
2. 桌面 App 优先，CLI 只作为调试和高级入口。
3. 默认本地运行，默认只监听 `127.0.0.1`。
4. 关键错误必须可解释，例如端口占用、provider 配置错误、route 未命中、sidecar 未启动、Dashboard 未收到请求。
5. 用户不需要理解 Headroom 内部实现，也能看懂自己节省了多少 token、请求走到哪里、哪里出了错。
6. 用户可以清楚看到 CLI 输出预处理层和 API 接入层压缩分别产生了多少收益，避免把两层效果混在一起。

## 3. 明确边界

### 3.1 必须做

1. Headroom-compatible core。
2. Full Dashboard migration。
3. RTK-compatible / CLI Output Preprocessor integration layer。
4. ProviderProfile。
5. RouteProfile。
6. Instance manager。
7. Chain bridge。
8. Local process manager。
9. Local logs and metrics。
10. Local config and secret handling。
11. Windows/macOS installer。
12. Doctor 页面。
13. JSON/CSV export。
14. Clean-room reimplementation first；若复用 Apache 2.0 代码，必须保留 license notice。

### 3.2 不做

1. 不做云端账号托管。
2. 不做账号共享、账号池、转售或代登录。
3. 不做违反平台条款的自动化用途。
4. 不偷偷读取或迁移用户已有 token。
5. 不把用户代码、日志、prompt 上传到 d-token 服务器。
6. 不做公网 relay 服务，默认只做本地 `127.0.0.1`。
7. 不做静默 provider fallback；任何 fallback 都必须可见、可配置、可审计。
8. 不做 Dashboard 简化版。
9. 不删除 Headroom 功能。
10. 不把 RTK/CLI 输出预处理层与 Headroom-compatible core 强耦合；两者必须可单独启用、单独关闭、单独观测。

## 4. Headroom 功能保留范围

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

## 5. RTK / CLI 输出预处理层

### 5.1 定位

RTK/CLI Output Preprocessor 是 d-token 的上游可插拔预处理层，工作在“命令执行层”，用于在 Shell 命令输出进入 Agent 上下文之前，提前清洗 CLI 输出噪音。

它与 Headroom-compatible core 是上下游互补关系，而不是竞争关系：

```text
AI Agent 发起命令
  ↓
RTK / CLI Output Preprocessor
  ↓
第一层压缩：清洗 CLI 输出噪音，例如 ANSI 码、空行、重复日志、低价值终端噪声
  ↓
压缩后的命令结果进入 Agent 上下文
  ↓
d-token Local Proxy / Headroom-compatible Core
  ↓
第二层压缩：对全部上下文进行统一压缩，例如命令结果、文件、对话历史、工具结果
  ↓
Route Decision
  ↓
Official Provider / Custom Provider / Chain Bridge
  ↓
Provider
```

### 5.2 与 Headroom 的分工

RTK/CLI Output Preprocessor 负责：

1. 命令执行层输出清洗。
2. Shell stdout/stderr 噪音裁剪。
3. ANSI escape code 清理。
4. 空行、重复行、低价值日志裁剪。
5. CLI 场景下的前置 token 降噪。

Headroom-compatible core 负责：

1. API 接入层统一压缩。
2. 全部上下文压缩。
3. 语义压缩。
4. 可逆缓存。
5. CCR。
6. memory / learn / output shaper。
7. 历史统计和 savings ledger。

### 5.3 兼容性原则

1. RTK/CLI Output Preprocessor 不触碰 LLM API 流量。
2. Headroom-compatible core 不接管 Shell hook 实现。
3. 两层压缩必须可独立启停。
4. 两层压缩的 savings 必须分别统计。
5. 如果用户只使用 Headroom-compatible core，d-token 必须正常工作。
6. 如果用户同时使用 RTK 和 Headroom-compatible core，d-token 必须能展示两层流水线。
7. 如果具体 RTK GitHub 仓库后续确认，PRD 只记录其兼容适配要求，不把 d-token 架构绑定到单一外部实现。

### 5.4 Dashboard 新增观测维度

Dashboard 必须新增：

1. Preprocessor enabled。
2. Preprocessor implementation：RTK / builtin / custom。
3. CLI output tokens before。
4. CLI output tokens after。
5. CLI output tokens saved。
6. CLI output savings percent。
7. Preprocessor transforms applied。
8. Headroom input tokens before。
9. Headroom input tokens after。
10. End-to-end tokens saved。
11. Layer contribution：preprocessor vs Headroom-compatible core。
12. Per-agent CLI savings。
13. Per-command savings。
14. Per-instance layered compression view。

### 5.5 技术边界

RTK/CLI Output Preprocessor 可以是：

1. 外部 GitHub 仓库工具。
2. d-token 内置 runtime。
3. 用户自定义命令输出过滤器。
4. Agent hook 适配层。

d-token 不强制指定唯一实现，但必须提供标准接口：

```text
raw_cli_output
  ↓
preprocessor transform
  ↓
cleaned_cli_output
  ↓
preprocessor_metrics
```

## 6. Dashboard 全量迁移范围

必须迁移：

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
44. Layered compression contribution。
45. CLI preprocessor savings。
46. End-to-end savings。

新增 d-token 维度：ProviderProfile、RouteProfile、Instance、Chain mode、OS process status、Config source、PreprocessorProfile、LayeredCompressionStage。

## 7. 技术选型

### 7.1 桌面 App

采用：

```text
Tauri 2 + React + TypeScript + Vite
```

理由：

1. Tauri 支持 Windows/macOS/Linux 桌面应用。
2. Tauri 适合轻量桌面壳，不需要打包完整 Chromium。
3. Tauri 支持 external binaries / sidecar，适合管理 proxy、compress、router、doctor、preprocessor 等本地进程。
4. Tauri 支持 updater，适合后续 Windows/macOS 自动更新。
5. React/TypeScript 适合把 Headroom Dashboard 组件化迁移。

Electron 仅作为备选，不作为主路线。只有在极端追求前端开发速度且完全不想碰 Rust 时才考虑。

### 7.2 本地核心

采用：

```text
Rust core + Python sidecar transitional architecture
```

短期：

- Rust 负责 process manager、local API、proxy/router skeleton、logs、config、port manager、doctor、preprocessor interface。
- Python sidecar 保留 Headroom-compatible compression、CCR、memory、learn、MCP 等能力。

长期：

- 稳定的 proxy、router、metrics、ledger、config、process manager、preprocessor metrics 逐步 Rust 化。
- 高级压缩、ML、learn、evals 可继续保留 Python sidecar，直到 Rust parity 足够可靠。

## 8. 运行时架构

### 8.1 标准 API 接入链路

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

### 8.2 CLI 分层压缩链路

```text
AI Coding Tool
  ↓
Shell Command Execution
  ↓
RTK / CLI Output Preprocessor
  ↓
Agent Context
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

### 8.3 桌面端职责

桌面端负责：

1. 配置。
2. 启停。
3. 状态展示。
4. 日志查看。
5. 诊断。
6. 导出。
7. PreprocessorProfile 管理。
8. 分层压缩贡献展示。

请求链路由本地 proxy/core 处理，Desktop 关闭后可以选择继续保留本地代理运行。

## 9. 目录结构

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

## 10. Windows/macOS 重点要求

### 10.1 Windows

1. WebView2 检测。
2. MSVC toolchain 检测。
3. 端口占用检测。
4. 旧进程残留检测。
5. `.cmd` / `.exe` / `.bat` shim 兼容。
6. 盘符路径兼容。
7. UTF-8 日志。
8. 防火墙提示。
9. installer 签名预留。
10. Shell hook / preprocessor 启停状态检测。

### 10.2 macOS

1. Intel / Apple Silicon 双架构。
2. Xcode Command Line Tools 检测。
3. quarantine / xattr 提示。
4. Keychain 预留。
5. LaunchAgent 预留。
6. 菜单栏托盘预留。
7. notarization 预留。
8. Shell hook / preprocessor 启停状态检测。

## 11. 安全原则

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
11. CLI 输出预处理层不得默认记录完整命令输出。
12. 如果开启完整 CLI 输出留存，必须显式确认、仅本地保存、可一键删除、导出时脱敏。

## 12. 里程碑

### M0：文档与仓库结构收敛

1. 单一 living PRD。
2. 技术选型文档。
3. 架构文档。
4. 安全边界文档。
5. 最终多维审查报告。
6. 仓库目录重构为 `docs/`、`code/`、`support/`。

### M1：Core Skeleton

1. Rust workspace。
2. local API skeleton。
3. proxy skeleton。
4. config loader。
5. process manager skeleton。
6. doctor skeleton。
7. preprocessor interface skeleton。

### M2：Dashboard Skeleton

1. Tauri shell。
2. React Dashboard。
3. Headroom Dashboard 全卡片占位。
4. Layered compression cards 占位。
5. mock `/stats` / `/stats-history`。
6. export 占位。

### M3：Provider / Route / Instance / Preprocessor

1. ProviderProfile。
2. RouteProfile。
3. Instance manager。
4. Chain bridge。
5. PreprocessorProfile。
6. LayeredCompressionStage。
7. request log schema。

### M4：Headroom-compatible Parity + Layered Compression

1. Compression pipeline。
2. CCR。
3. memory。
4. learn。
5. output shaper。
6. historical ledger。
7. RTK/custom preprocessor metrics ingestion。
8. end-to-end savings attribution。

### M5：Production Hardening

1. Windows installer。
2. macOS bundle。
3. updater。
4. signing/notarization 预留。
5. benchmark/evals。
6. crash recovery。
7. shell hook compatibility matrix。

## 13. 验收标准

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
11. RTK/CLI Output Preprocessor 与 Headroom-compatible core 必须可组合、可单独启停、可单独观测。
12. Dashboard 必须能区分 CLI 预处理收益、Headroom 压缩收益、端到端总收益。
