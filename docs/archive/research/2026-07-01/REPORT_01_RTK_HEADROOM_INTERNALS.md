# 深度研究报告 01：RTK 与 Headroom 内部机制二次优化版

日期：2026-07-01  
适用项目：d-token  
研究对象：`rtk-ai/rtk`、`headroomlabs-ai/headroom`  
状态：Second-pass optimized

## 1. 执行摘要

RTK 与 Headroom 都服务于 AI Coding 场景的 token 优化，但它们不在同一层工作。

```text
RTK
  层级：Command Execution Layer / Shell Output Layer
  目标：命令输出进入 Agent 上下文前先降噪
  技术：Rust CLI proxy、hook/plugin rewrite、command filters、SQLite tracking、tee recovery

Headroom
  层级：API Gateway Layer / LLM Context Layer
  目标：完整上下文发往 LLM 前统一压缩
  技术：library、proxy、MCP、wrap、ContentRouter、CacheAligner、SmartCrusher、CodeCompressor、CCR、memory、metrics

d-token
  层级：Desktop Control Plane / Local Orchestration Layer
  目标：统一安装、配置、观测、策略、回滚和端到端 savings attribution
```

核心结论：RTK 不是 Headroom 的替代品，Headroom 也不是 RTK 的替代品。d-token 的定位不是重写二者，而是做本地控制面：安全安装、状态探测、指标导入、策略管理、路由编排、跨层归因、Doctor 和 rollback。

## 2. RTK 内部机制

### 2.1 RTK 是什么

RTK，全称 Rust Token Killer，是一个高性能 CLI proxy。它通过把原命令改写为 `rtk ...`，在命令执行后对 stdout/stderr 进行命令特定过滤，把冗长输出变成 LLM 更容易处理的 compact output。

核心特征：

1. 单 Rust binary。
2. 使用 command registry 判断能否 rewrite。
3. 通过 hook/plugin 把 `git status`、`cargo test` 等命令改成 `rtk git status`、`rtk cargo test`。
4. 使用 Rust filters 与 TOML DSL filters 两套过滤系统。
5. 保留原命令 exit code。
6. filter 失败时 fail-safe passthrough。
7. 本地 tracking database 记录 token savings。
8. `rtk gain`、`rtk discover`、`rtk session` 提供 analytics。
9. raw-output tee recovery 用于恢复完整输出。
10. telemetry disabled-by-default，必须用户 opt-in。
11. Native Windows 支持 filters，但完整 auto-rewrite 更适合 WSL。

### 2.2 总体链路

```text
AI Agent
  ↓
Agent hook / plugin / instruction
  ↓
rtk rewrite
  ↓
Command registry classification
  ↓
Rewritten command: rtk <original command>
  ↓
RTK CLI parser
  ↓
Command router
  ↓
Rust filter or TOML DSL filter
  ↓
Filtered output printed to agent
  ↓
Token tracking written locally
```

### 2.3 Hook lifecycle management

RTK 的 hook system 不是简单 shell alias，而是 lifecycle management layer。

它负责：

1. `rtk init` 安装流程。
2. agent-specific hook 文件生成。
3. agent settings patch。
4. SHA-256 integrity baseline。
5. hook version checking。
6. hook audit。
7. `rtk rewrite` CLI bridge。
8. project-local TOML filter trust。

边界划分：

```text
hooks owns: install / verify / audit / trust
rewrite registry owns: command classification and rewrite
cmds owns: command-specific filtering
core owns: tracking, config, tee, utilities
analytics owns: read-only dashboards
```

d-token 的采纳策略：不要伪造 RTK hook 逻辑。优先调用 RTK 自身安装/验证能力；d-token 只展示 hook 状态、hash 状态、agent support、fallback mode 和 rollback path。

### 2.4 Integrity verification

RTK hook integrity 的关键机制：

1. 安装时计算 hook 文件 SHA-256。
2. 将 hash baseline 存到本地。
3. 运行时重新计算 hash 并比较。
4. verify/audit 能输出状态。

d-token 应映射为：

```ts
interface RtkHookIntegrity {
  state: 'verified' | 'tampered' | 'no_baseline' | 'not_installed' | 'orphaned_hash' | 'unknown';
  hook_path?: string;
  hash_path?: string;
  message: string;
  blocking: boolean;
}
```

UI 要求：

1. `tampered` 必须红色高危提示。
2. `no_baseline` 应提示用户重新 verify 或 reinstall。
3. `orphaned_hash` 应提示清理或修复。
4. 不得自动覆盖用户 hook，除非用户确认。

### 2.5 Permission model

RTK 对 Claude/Copilot 等工具的权限决策遵守：

```text
Deny > Ask > Allow > Default
```

这意味着 d-token 必须尊重 host agent 的安全模型：

1. 不把 Deny 改成 Allow。
2. 不绕过 Ask。
3. 不自动扩大 Bash 权限。
4. 不静默修改 permissions。
5. 如果 agent 不支持 Ask，应在 UI 中标明限制。

### 2.6 Rewrite pipeline

RTK rewrite 的关键流程：

```text
hook shell
  ↓
rewrite_command()
  ↓
rewrite_compound()
  ↓
rewrite_segment()
  ↓
classify_command()
  ↓
rule rewrite
```

它能处理 compound commands，例如：

```bash
cargo fmt --all && cargo test 2>&1 | tail -20
```

目标改写为：

```bash
rtk cargo fmt --all && rtk cargo test 2>&1 | tail -20
```

关键技术边界：

1. lexer-based tokenization。
2. shell operators 拆分。
3. pipe 左侧优先 rewrite。
4. trailing redirects 保留。
5. env prefix 双阶段处理。
6. `RTK_DISABLED=1` guard。
7. unsupported shellism passthrough。

### 2.7 TOML DSL filter pipeline

RTK 的 TOML DSL filter pipeline 可抽象为 8 个阶段：

1. strip_ansi。
2. replace。
3. match_output。
4. strip / keep_lines。
5. truncate_lines_at。
6. head / tail_lines。
7. max_lines。
8. on_empty。

filter lookup 顺序：

```text
project-local .rtk/filters.toml   需要 trust
user-global ~/.config/rtk/filters.toml
built-in filters                  build-time embedded
```

对 d-token 的意义：如果 Desktop 支持编辑 RTK filters，必须区分 project-local、user-global、built-in，并尊重 `rtk trust`。不应把 project-local filter 自动启用。

### 2.8 Tracking database and analytics

RTK tracking DB 记录命令级数据：

1. original_cmd。
2. rtk_cmd。
3. project_path。
4. input_tokens。
5. output_tokens。
6. saved_tokens。
7. savings_pct。
8. exec_time_ms。
9. parse_failures。

对 d-token 的要求：

1. 只读导入 RTK tracking DB。
2. 优先使用 `rtk gain --all --format json` 这类公开命令。
3. 不直接修改 RTK DB。
4. 不上传 RTK DB。
5. 不默认展示完整命令参数。
6. import failure 不应阻塞 provider routing，只应降级为 warning。

### 2.9 Tee recovery

Tee recovery 是 RTK 的安全阀：输出被截断或过滤后，用户仍能按需查看原始输出。

d-token Dashboard 默认只展示：

1. path。
2. size。
3. command category。
4. timestamp。
5. truncation reason。
6. content hash。

不默认打开完整内容。

## 3. Headroom 内部机制

### 3.1 Headroom 是什么

Headroom 是 AI agents 的 context compression layer。它压缩 tool outputs、logs、DB reads、RAG results、file reads、API responses、conversation history 等，工作方式包括 library、proxy、agent wrap、MCP server 和 framework integrations。

### 3.2 总体链路

```text
Agent / App
  ↓
Headroom proxy / SDK / wrap / MCP
  ↓
CacheAligner
  ↓
ContentRouter
  ↓
SmartCrusher / CodeCompressor / SearchCompressor / LogCompressor / Kompress
  ↓
CCR local store
  ↓
compressed content + retrieval reference
  ↓
LLM provider
```

### 3.3 Live-zone-only compression

Headroom 的现代策略不是“随便压整个历史上下文”，而是 live-zone-only compression：

1. 不删除 conversation history。
2. 不做旧式 position-based context dropping。
3. 不做任意 score-based context management。
4. 只压最新 user message / latest tool result 等 live zone。
5. 保持 system prompt、tools、older turns 的 cache hot zone 稳定。
6. 通过 CCR 保持可逆 retrieval。

对 d-token 的意义：项目级压缩策略不应简单实现“压缩所有历史消息”。必须保持 Headroom 的 cache/hot-zone 设计，否则会破坏 provider prompt caching 和答案稳定性。

### 3.4 Transform safety guarantees

Headroom transforms 的安全原则：

1. Never remove human content。
2. Never break tool ordering。
3. Parse failures are no-ops。
4. Preserve recency。
5. Preserve errors。

对 d-token 的要求：

1. Dashboard 必须展示 transform failure/pass-through。
2. compression policy 不能覆盖 error preservation。
3. aggressive/max 也不能移除用户内容。
4. parse failure 应显示为 no-op，而不是 error panic。

### 3.5 ContentRouter

ContentRouter 负责检测内容类型和选择压缩器。

检测信号：

1. JSON structure。
2. source code patterns。
3. search result `file:line:` patterns。
4. log timestamp / log level。
5. file paths / tool names。
6. fallback plain text。

路由目标：CodeCompressor、SmartCrusher、SearchCompressor、LogCompressor、TextCompressor、Passthrough。

对 d-token 的意义：项目级压缩策略不能只是 `safe/aggressive` 一个枚举，还要映射到真实 Headroom knobs：启用哪些 compressor、阈值多少、最小 token 数、fallback 行为如何。

### 3.6 CacheAligner

CacheAligner 目标是稳定 prompt prefix，提升 provider cache hit rate。

d-token Dashboard 必须把“token savings”和“cache hit improvement”分开显示。CacheAligner 可能不直接减少 tokens，但能降低成本/延迟。

### 3.7 CCR reversible compression

CCR 通过本地保存原始内容，并给 LLM 一个 retrieval reference，使压缩可逆。

关键 UI 边界：

1. 显示 cache size。
2. 显示 entries count。
3. 显示 retrieval count。
4. 显示 TTL / project scope。
5. 不默认读取原文。
6. 支持清理项目缓存。

### 3.8 MCP server

Headroom MCP 提供三类工具：

1. `headroom_compress`：按需压缩内容。
2. `headroom_retrieve`：按 hash 恢复原文或搜索原文。
3. `headroom_stats`：查看 session/sub-agent/proxy stats。

安装向导必须区分：

```text
MCP only: on-demand compression/retrieve/stats
Proxy only: automatic HTTP-level compression
MCP + Proxy: 自动压缩 + 按需 retrieve + stats aggregation
```

### 3.9 Proxy production features

Headroom proxy 不只是压缩器，还可能包含：

1. local server。
2. context optimization。
3. semantic caching。
4. rate limiting。
5. retry with exponential backoff。
6. cost tracking and budgets。
7. provider fallback。
8. metrics。
9. request/response logging。
10. provider route registration。
11. project context classification。

对 d-token 的要求：如果 d-token 自己也做 provider routing，要明确边界，避免和 Headroom provider fallback 重叠导致不可解释。

## 4. RTK 与 Headroom 的组合数据模型

### 4.1 Event correlation

必须有统一 event ledger：

```ts
interface DtokenEvent {
  event_id: string;
  timestamp_ms: number;
  project_id?: string;
  agent_id?: string;
  instance_id?: string;
  command_id?: string;
  request_id?: string;
  stage:
    | 'rtk_rewrite'
    | 'rtk_filter'
    | 'headroom_route'
    | 'headroom_compress'
    | 'headroom_retrieve'
    | 'provider_forward'
    | 'install_step'
    | 'config_patch';
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

### 4.2 防重复统计

不能直接把 RTK savings 与 Headroom savings 相加。

正确方法：

```text
raw_cli_tokens
  → rtk_cleaned_tokens
  → headroom_input_tokens
  → headroom_final_tokens

rtk_savings = raw_cli_tokens - rtk_cleaned_tokens
headroom_savings = headroom_input_tokens - headroom_final_tokens
end_to_end_savings = raw_total_context_tokens - final_llm_input_tokens
```

### 4.3 Attribution confidence

由于 RTK command event 与 Headroom request event 不一定共享强 ID，d-token 必须给归因结果打置信标签：

```text
exact      command_id/request_id 可强关联
estimated  project_id + agent_id + time window + command hash 弱关联
low        只能按窗口聚合，不能下钻到具体命令
```

## 5. d-token 采纳策略

### 5.1 对 RTK

1. External-first。
2. Managed mode 可选。
3. 不复制 RTK filters。
4. 不修改 RTK DB。
5. 不自动开启 telemetry。
6. 不绕过 permission model。
7. 不静默安装 hook。
8. 只读 analytics。
9. hook integrity 状态必须进入 Doctor。
10. project-local filters 必须尊重 trust。

### 5.2 对 Headroom

1. Full parity first。
2. Proxy/MCP/wrap/status 全部可观测。
3. 不读取 CCR 原文。
4. 不破坏 live-zone-only compression。
5. 不与 Headroom provider fallback 产生不可解释重叠。
6. 项目级策略映射到真实 Headroom knobs，而不是虚假的 UI 枚举。
7. 输出 token shaping 必须单独统计，不混入 input compression savings。

### 5.3 对 d-token 自身

1. Desktop 是控制面，不是黑盒代理。
2. 所有配置变更必须 diff → backup → apply → validate → rollback。
3. 所有指标必须能追溯到 event ledger。
4. 所有 error boundary 必须可解释。
5. 所有 managed binary 必须记录 provenance。

## 6. PRD 二次审查结论

本报告反推 PRD 需要补齐的条目：

1. `DtokenEvent` / event ledger。
2. `InstallationTransaction`。
3. `CompatibilityMatrix`。
4. root `README.md` 合法化。
5. RTK hook integrity 与 permission model。
6. Headroom live-zone-only compression 与 transform safety。
7. managed binary provenance。
8. attribution confidence。

上述内容已同步进入 `docs/PRD.md`。

## 7. 结论

报告 01 二次优化后的核心判断：

```text
RTK owns command output filtering.
Headroom owns context compression.
d-token owns installation, orchestration, policy, observability, attribution, compatibility and rollback.
```

d-token 要成为本地 AI Coding 压缩控制面，而不是重新发明 RTK 和 Headroom。