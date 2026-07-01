# 深度研究报告 01：RTK 与 Headroom 是什么、内部如何工作、用了哪些技术

日期：2026-07-01  
适用项目：d-token  
研究对象：`rtk-ai/rtk`、`headroomlabs-ai/headroom`

## 1. 执行摘要

RTK 和 Headroom 都是为 AI Coding 场景降低 token 消耗的工具，但它们工作在完全不同的层级。

```text
RTK
  位置：Command Execution Layer / Shell Output Layer
  目标：让命令输出在进入 Agent 上下文之前变短
  典型对象：git status、cargo test、pytest、docker logs、grep、tree、ls

Headroom
  位置：API Gateway Layer / LLM Context Layer
  目标：让完整上下文在发往 LLM 之前变短
  典型对象：tool outputs、logs、files、RAG chunks、conversation history、code blocks
```

最准确的组合关系是：

```text
RTK 是上游前置降噪器；Headroom 是下游上下文压缩器；d-token 是两者的桌面化编排层。
```

## 2. RTK 是什么

RTK，全称 Rust Token Killer，是一个高性能 CLI proxy。它的目标是在 AI agent 看到命令输出之前，把原始 CLI 输出压缩成更适合 LLM 消化的摘要。

RTK 的核心特征：

1. 单 Rust binary。
2. CLI proxy 模式。
3. 通过 hook/plugin 自动把命令改写成 `rtk ...`。
4. 对不同命令使用不同 filter。
5. 保留原命令 exit code。
6. 失败时 fail-safe 回退。
7. 本地 SQLite tracking database 统计 token savings。
8. raw-output tee recovery 让用户可找回完整输出。
9. Telemetry 默认关闭，需要显式 opt-in。
10. Native Windows hook 能力有限，WSL 是完整体验路径。

## 3. RTK 内部工作机制

### 3.1 总体链路

```text
AI Agent
  ↓
Agent Hook / Plugin
  ↓
Command rewrite: git status → rtk git status
  ↓
RTK CLI parser
  ↓
Command router
  ↓
Command-specific filter
  ↓
Filtered output
  ↓
Token tracking
  ↓
Agent context
```

### 3.2 Hook / Plugin 层

RTK 的最佳体验来自自动 rewrite：

```text
用户/Agent 想执行：git status
RTK hook 改写为：rtk git status
Agent 实际收到：过滤后的 compact output
```

RTK 支持多种 agent 接入方式：

1. Hook-based agents：通过 hook 读取 agent 的 tool call JSON，然后返回改写后的命令。
2. Plugin-based agents：通过插件 API 改写命令。
3. Instruction-based agents：无法自动 rewrite 时，通过规则文件提示 agent 显式调用 RTK。

关键限制：

1. hook 主要作用于 Bash/tool command calls。
2. Claude Code 内置 `Read`、`Grep`、`Glob` 不经过 Bash hook。
3. Native Windows 下自动 rewrite 能力有限，WSL 才是完整 hook 体验。

### 3.3 Rewrite pipeline

RTK 的 rewrite pipeline 可以抽象为：

```text
hook shell
  ↓
rewrite_cmd
  ↓
rewrite_command
  ↓
rewrite_compound
  ↓
rewrite_segment
  ↓
classify_command
  ↓
rtk command
```

它支持 compound command，例如：

```bash
cargo fmt --all && cargo test 2>&1 | tail -20
```

会变成：

```bash
rtk cargo fmt --all && rtk cargo test 2>&1 | tail -20
```

关键技术点：

1. lexer-based tokenization：识别 quotes、escapes、redirects、operators、pipes。
2. segment-level rewriting：按 `&&`、`||`、`;`、`|` 切分。
3. pipe semantics：通常只改写 pipe 左侧，右侧保持原样。
4. env prefix handling：支持 `ENV=xxx git status` 这类命令。
5. guard rules：避免 heredoc、arithmetic expansion、写重定向等危险误改写。
6. registry matching：通过 command registry 判断是否有 RTK equivalent。

### 3.4 CLI parsing and routing

RTK 进入 Rust binary 后：

1. Clap parser 解析命令。
2. Commands enum 匹配具体命令类型。
3. 路由到 `src/cmds` 下的具体 filter module。
4. 若没有命中 Rust filter，则进入 fallback path。
5. fallback path 尝试 TOML DSL filter。
6. 仍然没有 filter 时 passthrough。

### 3.5 Filter systems

RTK 有两类 filter：

#### Rust filters

适合复杂命令：

1. git。
2. cargo。
3. pytest。
4. go test。
5. docker logs。
6. kubectl logs。
7. tsc。
8. ruff。
9. eslint。

这些 filter 可以使用 regex、JSON parsing、state machine、NDJSON streaming parser、结构化聚合等技术。

#### TOML DSL filters

适合通用命令或用户自定义过滤：

1. 正则行过滤。
2. section extraction。
3. truncation。
4. simple rule-based compression。

### 3.6 Filtering strategy taxonomy

RTK 常见压缩策略：

| Strategy | 技术 | 典型命令 | 价值 |
|---|---|---|---|
| Stats extraction | 统计聚合 | git status / git log | 从大量行变成摘要 |
| Error only | 只保留错误 | test / build | 隐藏成功项 |
| Grouping | 按 rule/file/type 聚合 | eslint / tsc / grep | 减少重复错误 |
| Deduplication | 重复行折叠 | logs | 消除刷屏日志 |
| Structure only | 只保留结构 | json | 去掉大值保留 schema |
| Code filtering | 代码级精简 | read | 去注释/去函数体 |
| Failure focus | 只看失败 | pytest / go test | 测试输出极大压缩 |
| Progress filtering | 去进度条 | wget / pnpm | 去 ANSI 和进度噪声 |
| State machine | 状态机解析 | pytest | 抽取测试状态 |
| NDJSON streaming | 流式事件聚合 | go test | 汇总事件流 |

### 3.7 Token tracking

RTK 每次命令执行后写入本地 SQLite tracking database。

记录字段包括：

1. raw input token estimate。
2. filtered output token estimate。
3. savings percent。
4. execution time。
5. project path。
6. timestamp。
7. command category。

`rtk gain`、`rtk discover`、`rtk session` 都基于这些本地数据生成统计。

### 3.8 Tee recovery

对于失败命令或需要完整上下文的场景，RTK 可以保留 raw output tee file。

d-token 适配时必须只默认展示：

1. tee 文件路径。
2. 文件大小。
3. 摘要。
4. 创建时间。

不应默认读取完整内容。

### 3.9 RTK telemetry

RTK telemetry 默认关闭，需要用户显式 opt-in。

d-token 必须做到：

1. 只展示 telemetry 状态。
2. 不替用户开启 telemetry。
3. 不把 RTK tracking database 上传到任何远端。
4. 不读取源码、文件内容、完整命令参数、secrets、环境变量值。

## 4. Headroom 是什么

Headroom 是 AI agents 的 context compression layer。它不是命令输出前置过滤器，而是 LLM API 接入层压缩系统。

它压缩的对象包括：

1. tool outputs。
2. logs。
3. files。
4. RAG chunks。
5. conversation history。
6. code blocks。
7. structured JSON。

Headroom 的目标是：在不改变答案质量的前提下，用更少 tokens 发送给 LLM。

## 5. Headroom 内部工作机制

### 5.1 总体链路

```text
Agent / App
  ↓
Headroom proxy / library / wrap / MCP
  ↓
CacheAligner
  ↓
ContentRouter
  ↓
Specialized compressors
  ↓
CCR local cache
  ↓
Compressed prompt + retrieve ability
  ↓
LLM provider
```

### 5.2 接入模式

Headroom 提供多种接入方式：

1. Library：应用内直接 `compress(messages)`。
2. Proxy：本地代理，OpenAI/Anthropic-compatible 流量经过代理。
3. Agent wrap：一条命令包装 Claude、Codex、Cline、OpenCode 等 agent。
4. MCP server：提供 `headroom_compress`、`headroom_retrieve`、`headroom_stats` 等工具。
5. Dashboard：观测 savings、latency、providers、per-agent/per-project 指标。

### 5.3 CacheAligner

CacheAligner 的目的：稳定 prompt prefix，让 provider 的 KV cache 更容易命中。

价值：

1. 减少重复上下文变动。
2. 提高 cache hit rate。
3. 降低 latency 和 cost。
4. 避免压缩层破坏 provider cache。

### 5.4 ContentRouter

ContentRouter 判断输入内容类型，并选择压缩器。

典型类型：

1. JSON。
2. code。
3. prose/text。
4. logs。
5. diff。
6. RAG chunks。
7. conversation history。

### 5.5 Specialized compressors

Headroom 中的压缩器包括：

1. SmartCrusher：面向 JSON / structured content。
2. CodeCompressor：面向代码，AST-aware。
3. Kompress / text compressor：面向 prose / text。
4. log/diff compressors：面向日志与变更。
5. optional ML compressor：复杂文本压缩。

### 5.6 CCR reversible compression

CCR 是 Headroom 的关键差异化能力。

基本思想：

```text
原始内容本地缓存
  ↓
发送给 LLM 的是压缩摘要 + retrieve reference
  ↓
LLM 需要细节时调用 retrieve
```

这能解决“压缩太狠导致细节丢失”的问题，但对 d-token 提出了隐私要求：

1. 默认不展示原文缓存内容。
2. Dashboard 只展示数量、大小、命中率、retrieve 次数。
3. 导出时必须脱敏。
4. 用户可清理项目缓存。

### 5.7 Memory / learn / output shaper

Headroom 不只压输入，也关注输出和行为学习：

1. Cross-agent memory：不同 agent 共享记忆。
2. Per-project memory：项目级记忆。
3. `headroom learn`：从失败会话中提炼修正规则。
4. Verbosity steering：让模型少写废话。
5. Effort routing：对简单工具结果降低推理强度。
6. Output savings：估计或测量输出 token savings。

### 5.8 Metrics and dashboard

Headroom Dashboard 需要迁移的指标包括：

1. input compression savings。
2. output tokens saved。
3. overhead。
4. throughput。
5. provider breakdown。
6. per-model savings。
7. per-agent savings。
8. per-project savings。
9. recent requests。
10. historical stats。
11. export。

## 6. RTK 与 Headroom 的本质差异

| 维度 | RTK | Headroom |
|---|---|---|
| 所在层级 | 命令执行层 | LLM API 接入层 |
| 输入对象 | CLI stdout/stderr | 全部 prompt/context |
| 核心机制 | command rewrite + filter | content routing + compression + CCR |
| 运行形态 | Rust binary | Python CLI/proxy/library/MCP + TS SDK |
| 数据库 | 本地 tracking SQLite | savings ledger / cache / memory |
| 风险 | hook 覆盖不全、误过滤 | 过度压缩、retrieve 依赖 |
| 最强场景 | 测试、日志、git、构建输出 | 文件、RAG、对话历史、复杂上下文 |
| d-token 角色 | 适配、检测、导入 analytics | 管理、配置、迁移 dashboard、压缩策略 |

## 7. d-token 的集成原则

1. RTK 优先 external integration，不复制 filters。
2. Headroom 优先 full parity，不做删减版。
3. 两者都可独立启停。
4. 两者都必须可观测。
5. 两者 savings 必须分层统计。
6. 不能把两层 savings 直接相加。
7. Desktop 必须展示每层输入、输出、节省、状态、错误。
8. 配置修改必须 diff、backup、rollback。
9. 默认不读取原始命令输出和 CCR 原文缓存。
10. 默认不上传任何用户数据。

## 8. 对 d-token 的工程启示

d-token 不应该变成“再造 RTK + 再造 Headroom”的大杂烩，而应该是：

```text
1. Desktop shell
2. local orchestration core
3. external binary/runtime manager
4. metrics ingestion layer
5. project policy manager
6. layered compression attribution engine
7. safety-first config patcher
8. full dashboard migration
```

最小正确实现顺序：

```text
M1: types + detection + mock metrics
M2: desktop installer + dashboard cards
M3: real RTK analytics import + Headroom status import
M4: Headroom compression parity + savings attribution
M5: installer hardening + rollback + compatibility matrix
```

## 9. 研究结论

RTK 与 Headroom 都很强，但它们强在不同地方。

RTK 的价值是：用极低成本提前清掉 CLI 噪音。  
Headroom 的价值是：对最终发往 LLM 的完整上下文做统一压缩、可逆缓存和长期观测。  
d-token 的价值是：让这两层能力一键安装、统一配置、按项目调优、可观测、可回滚、可解释。
