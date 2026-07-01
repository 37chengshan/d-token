# 深度研究报告 03：新增功能与问题修复实现方案二次优化版

日期：2026-07-01  
适用项目：d-token  
状态：Second-pass optimized  
目标：将 PRD 新增能力、研究报告缺口、M1 遗留问题转成可执行工程蓝图。

## 1. 执行摘要

当前 d-token 已完成：

1. 单一 PRD。
2. docs/code/support 目录收敛。
3. 根目录 README 项目入口。
4. Rust workspace 初始 skeleton。
5. local API skeleton。
6. proxy skeleton。
7. router/config/compress/process/doctor skeleton。

仍需补齐：

1. `cargo check --workspace` 与 `cargo test --workspace` 验证。
2. `dtoken-preprocessor`。
3. `dtoken-headroom-adapter`。
4. `dtoken-project-policy`。
5. `dtoken-config-patcher`。
6. `dtoken-event-ledger`。
7. `dtoken-compatibility`。
8. RTK/Headroom detection endpoints。
9. config diff/backup/rollback。
10. installation transaction。
11. layered savings attribution。
12. compatibility matrix。
13. CI gate。

实施原则：

```text
先 mock schema，再接外部工具；
先只读 detection，再允许 patch；
先 diff/backup/rollback，再安装 hook；
先 event ledger，再做 dashboard；
先本地 Doctor，再做自动修复；
先 compatibility warning，再做 managed install。
```

## 2. 新 crate 规划

### 2.1 dtoken-preprocessor

职责：RTK adapter 与 built-in fallback preprocessor。

目录：

```text
code/crates/dtoken-preprocessor/
  Cargo.toml
  src/lib.rs
  src/model.rs
  src/rtk.rs
  src/rtk_status.rs
  src/rtk_integrity.rs
  src/rtk_analytics.rs
  src/builtin.rs
  tests/rtk_missing.rs
  tests/rtk_parse_gain.rs
  tests/rtk_integrity.rs
```

核心 API：

```rust
pub struct RtkAdapter {
    pub binary_path: Option<PathBuf>,
}

impl RtkAdapter {
    pub async fn detect() -> RtkStatus;
    pub async fn version(&self) -> anyhow::Result<Option<String>>;
    pub async fn hook_status(&self) -> anyhow::Result<RtkHookStatus>;
    pub async fn hook_integrity(&self) -> anyhow::Result<RtkHookIntegrity>;
    pub async fn telemetry_status(&self) -> anyhow::Result<TelemetryStatus>;
    pub async fn gain_json(&self) -> anyhow::Result<RtkGainReport>;
    pub async fn discover(&self) -> anyhow::Result<RtkDiscoverReport>;
    pub async fn session(&self) -> anyhow::Result<RtkSessionReport>;
}
```

状态模型：

```ts
interface RtkStatus {
  found: boolean;
  binary_path?: string;
  version?: string;
  install_method?: 'homebrew' | 'cargo_git' | 'release_zip' | 'managed' | 'unknown';
  package_identity: 'rtk_ai' | 'wrong_package' | 'unknown';
  hook_status: 'installed' | 'missing' | 'partial' | 'unsupported' | 'unknown';
  integrity_state?: 'verified' | 'tampered' | 'no_baseline' | 'not_installed' | 'orphaned_hash' | 'unknown';
  telemetry_status: 'disabled' | 'enabled' | 'unknown';
  warnings: string[];
}
```

M1 范围：只读 detection + mock analytics，不做 hook mutation。

### 2.2 dtoken-headroom-adapter

职责：Headroom CLI/proxy/MCP/status/stats adapter。

目录：

```text
code/crates/dtoken-headroom-adapter/
  Cargo.toml
  src/lib.rs
  src/status.rs
  src/proxy.rs
  src/mcp.rs
  src/stats.rs
  src/cache_summary.rs
  tests/missing.rs
  tests/status_parse.rs
  tests/cache_summary.rs
```

核心 API：

```rust
pub struct HeadroomAdapter {
    pub binary_path: Option<PathBuf>,
    pub proxy_url: Option<String>,
}

impl HeadroomAdapter {
    pub async fn detect() -> HeadroomStatus;
    pub async fn cli_version(&self) -> anyhow::Result<Option<String>>;
    pub async fn doctor(&self) -> anyhow::Result<DoctorSummary>;
    pub async fn proxy_status(&self) -> anyhow::Result<ProxyStatus>;
    pub async fn mcp_status(&self) -> anyhow::Result<McpStatus>;
    pub async fn stats(&self) -> anyhow::Result<HeadroomStats>;
    pub async fn cache_summary(&self) -> anyhow::Result<CcrCacheSummary>;
}
```

关键边界：

1. npm `headroom-ai` 只是 SDK，不等价于 CLI/runtime。
2. 默认不读取 CCR 原文。
3. 默认不读取 request/response full logs。
4. proxy 和 MCP 可独立存在。
5. runtime isolation 由 installer/doctor 检查。

### 2.3 dtoken-project-policy

职责：项目级 Headroom 压缩策略。

目录：

```text
code/crates/dtoken-project-policy/
  Cargo.toml
  src/lib.rs
  src/project.rs
  src/policy.rs
  src/recommend.rs
  src/history.rs
  tests/recommendation.rs
  tests/manual_override.rs
```

推荐 API：

```rust
pub trait RecommendationEngine {
    fn recommend(&self, input: RecommendationInput) -> CompressionRecommendation;
}
```

v0 规则：

```text
new project → safe
high risk task → safe
retrieve_rate high → lower level
failure_rate high → lower level
large logs/tests and low failure → aggressive
normal code project → balanced
manual override → do not auto-change
```

### 2.4 dtoken-config-patcher

职责：所有配置修改唯一入口。

目录：

```text
code/crates/dtoken-config-patcher/
  Cargo.toml
  src/lib.rs
  src/diff.rs
  src/backup.rs
  src/rollback.rs
  src/patch.rs
  src/audit.rs
  src/redact.rs
  tests/patch_json.rs
  tests/patch_toml.rs
  tests/rollback.rs
  tests/redact_secrets.rs
```

核心 contract：

```text
No patch without diff.
No apply without backup.
No silent mutation.
No overwrite on hash mismatch.
No raw secret in diff.
No telemetry enable without explicit confirmation.
```

### 2.5 dtoken-event-ledger

职责：统一记录 RTK、Headroom、d-token 事件，用于分层 savings、Dashboard、审计和 debug。

目录：

```text
code/crates/dtoken-event-ledger/
  Cargo.toml
  src/lib.rs
  src/event.rs
  src/store.rs
  src/query.rs
  src/attribution.rs
  tests/ledger.rs
  tests/attribution.rs
```

事件模型：

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

存储策略：

1. M1/M2：JSONL。
2. M3：SQLite index + JSONL audit log。
3. M4：aggregation tables。
4. M5：export redaction audit。

### 2.6 dtoken-compatibility

职责：维护 OS / arch / shell / agent / RTK mode / Headroom mode 的兼容矩阵。

目录：

```text
code/crates/dtoken-compatibility/
  Cargo.toml
  src/lib.rs
  src/matrix.rs
  src/evaluate.rs
  src/warnings.rs
  tests/windows_native.rs
  tests/macos_zsh.rs
  tests/wsl.rs
```

模型：

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

## 3. Endpoint contract

### 3.1 RTK endpoints

```text
GET /rtk/status
GET /rtk/gain
GET /rtk/discover
GET /rtk/session
GET /rtk/hook-integrity
```

### 3.2 Headroom endpoints

```text
GET /headroom/status
GET /headroom/proxy
GET /headroom/mcp
GET /headroom/stats
GET /headroom/cache-summary
GET /headroom/policies
```

### 3.3 Project endpoints

```text
GET  /projects
POST /projects
GET  /projects/:id/policy
PUT  /projects/:id/policy
POST /projects/:id/recommend-compression
POST /projects/:id/policy-rollback
```

### 3.4 Install endpoints

```text
GET  /install/scan
POST /install/plan
POST /install/apply
POST /install/rollback
GET  /install/transactions
GET  /install/transactions/:id
```

### 3.5 Config endpoints

```text
POST /config/patches/plan
POST /config/patches/apply
GET  /config/rollback-points
POST /config/rollback
```

### 3.6 Event and compatibility endpoints

```text
GET /events
GET /events/:id
GET /layered-savings
GET /compatibility
GET /feature-flags
```

### 3.7 统一 response

```ts
interface ApiResponse<T> {
  ok: boolean;
  data?: T;
  error?: {
    code: string;
    message: string;
    recoverable: boolean;
    suggested_action?: string;
  };
  warnings: string[];
}
```

## 4. Failure taxonomy

```ts
type DtokenErrorCode =
  | 'tool_missing'
  | 'wrong_tool_package'
  | 'unsupported_platform'
  | 'native_windows_hook_limited'
  | 'wsl_missing'
  | 'python_missing'
  | 'python_version_too_low'
  | 'headroom_cli_missing'
  | 'headroom_proxy_down'
  | 'rtk_gain_failed'
  | 'rtk_hook_tampered'
  | 'config_parse_failed'
  | 'config_hash_mismatch'
  | 'backup_failed'
  | 'rollback_failed'
  | 'port_in_use'
  | 'provider_unhealthy'
  | 'route_not_found'
  | 'compression_failed'
  | 'ledger_write_failed'
  | 'compatibility_blocked'
  | 'managed_binary_unverified';
```

每个错误必须有：

1. 用户可读 message。
2. recoverable 标记。
3. suggested action。
4. log correlation id。
5. 是否阻塞继续安装。
6. 对应 Dashboard 状态。

## 5. Installation transaction implementation

状态机：

```text
idle
  → scanning
  → planning
  → awaiting_confirmation
  → backing_up
  → applying
  → validating
  → doctor_running
  → committed
  → failed
  → rolling_back
  → rolled_back
```

记录模型：

```ts
interface InstallTransactionRecord {
  id: string;
  status: string;
  selected_tools: string[];
  selected_agents: string[];
  patch_plan_ids: string[];
  rollback_point_id?: string;
  started_at: string;
  finished_at?: string;
  error_code?: string;
}
```

实现要求：

1. `plan` 不写文件。
2. `apply` 前必须检查 hash。
3. backup 必须包含原文件路径、hash、backup path、timestamp。
4. rollback 不依赖文件可解析性。
5. managed install 要记录 provenance。
6. Doctor failed 时 transaction 可以进入 `failed` 或 `committed_with_warnings`，具体取决于 blocker。

## 6. 双层 savings attribution

### 6.1 问题

如果直接将 RTK savings 和 Headroom savings 相加，会重复计算，因为 Headroom 输入可能已经是 RTK 清洗后的内容。

### 6.2 解决方案

必须记录 baseline graph：

```text
raw_cli_tokens
  ↓
rtk_cleaned_tokens
  ↓
headroom_input_tokens
  ↓
headroom_final_tokens
```

公式：

```text
rtk_savings = raw_cli_tokens - rtk_cleaned_tokens
headroom_savings = headroom_input_tokens - headroom_final_tokens
end_to_end_savings = raw_total_context_tokens - final_llm_input_tokens
```

### 6.3 工程实现

1. RTK events 用 `command_id`。
2. Headroom events 用 `request_id`。
3. d-token 通过 `project_id + agent_id + time_window + command hash` 做弱关联。
4. 未来如果 agent 能传递 correlation id，则使用强关联。
5. UI 必须标记 attribution confidence。

```ts
interface SavingsAttribution {
  project_id?: string;
  window_start_ms: number;
  window_end_ms: number;
  rtk_savings_tokens: number;
  headroom_savings_tokens: number;
  end_to_end_savings_tokens: number;
  attribution_confidence: 'exact' | 'estimated' | 'low';
  notes: string[];
}
```

## 7. Feature flags

新增功能必须用 feature flag：

```text
feature.rtk_adapter
feature.headroom_adapter
feature.project_policy
feature.config_patcher
feature.install_wizard
feature.event_ledger
feature.compatibility_matrix
feature.managed_rtk
feature.managed_headroom
feature.savings_attribution
```

默认：

```text
rtk_adapter = on
headroom_adapter = on
project_policy = on
config_patcher = on
event_ledger = on
compatibility_matrix = on
install_wizard = off until M2
managed_rtk = off until hardening
managed_headroom = off until hardening
```

## 8. CI gate

### 8.1 `.github/workflows` 例外

虽然根目录保持极简，但 GitHub Actions 必须放 `.github/workflows`。该目录作为平台必要例外。

### 8.2 最小 CI

```yaml
name: ci
on: [push, pull_request]
jobs:
  check:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions-rs/toolchain@v1
        with:
          toolchain: stable
      - run: python support/scripts/check_structure.py
      - run: cargo check --workspace
        working-directory: code
      - run: cargo test --workspace
        working-directory: code
```

### 8.3 Gate 标准

1. structure check must pass。
2. cargo check must pass。
3. cargo test must pass。
4. no real secrets in config examples。
5. docs index must include new reports。
6. root README must exist。
7. no duplicate PRD versions。

## 9. 分阶段实施路线

### M1A：验证当前 workspace

1. 跑 `python support/scripts/check_structure.py`。
2. 跑 `cargo check --workspace`。
3. 跑 `cargo test --workspace`。
4. 修复所有错误。

### M1B：RTK adapter

1. 新增 `dtoken-preprocessor`。
2. 实现 `rtk --version` detection。
3. 实现 missing/wrong-package 状态。
4. 实现 hook integrity mock。
5. mock `rtk gain` parser。
6. 增加 `/rtk/status`。
7. 增加 `/rtk/hook-integrity`。

### M1C：Headroom adapter

1. 新增 `dtoken-headroom-adapter`。
2. 实现 `headroom --version` detection。
3. 检测 npm SDK vs CLI。
4. mock `/headroom/status`。
5. mock `/headroom/cache-summary`。

### M1D：Project policy

1. 新增 `dtoken-project-policy`。
2. ProjectProfile model。
3. ProjectCompressionPolicy model。
4. recommendation v0。
5. manual override no-auto-change rule。

### M1E：Config patcher + event ledger

1. 新增 `dtoken-config-patcher`。
2. 新增 `dtoken-event-ledger`。
3. patch plan/diff/backup/rollback skeleton。
4. event JSONL。
5. secret redaction tests。

### M1F：Compatibility matrix

1. 新增 `dtoken-compatibility`。
2. 建立 OS / shell / agent / mode matrix。
3. 输出 compatibility warnings。
4. Doctor 接入 blocked/partial/full 状态。

### M2：Desktop skeleton

1. Tauri shell。
2. Installer wizard mock。
3. Dashboard cards mock。
4. Project policy UI mock。
5. Doctor UI mock。
6. Compatibility UI mock。
7. Event ledger health card。

### M3：真实集成

1. RTK gain/discover/session import。
2. Headroom stats/proxy/MCP import。
3. Read-only DB summary。
4. config patch real implementation。
5. real Doctor。
6. compatibility matrix first pass validation。

### M4：策略与归因

1. layered attribution。
2. recommendation engine improvement。
3. quality feedback loop。
4. measured/estimated labels。
5. attribution confidence。

### M5：生产硬化

1. installers。
2. signing/notarization。
3. updater。
4. rollback hardening。
5. compatibility test suite。
6. managed binary provenance audit。
7. privacy/export/redaction audit。

## 10. 验收总表

| 阶段 | Done 标准 |
|---|---|
| M1A | structure check / cargo check / cargo test 全过 |
| M1B | RTK missing/found/wrong-package/status/integrity endpoints 可用 |
| M1C | Headroom missing/found/proxy/mcp/cache summary 可用 |
| M1D | 项目策略 CRUD + 推荐 v0 |
| M1E | config diff/backup/rollback skeleton + event ledger |
| M1F | compatibility matrix + Doctor warnings |
| M2 | 桌面 UI 可跑，所有核心卡片有 mock 数据 |
| M3 | 真实 RTK/Headroom 数据导入，不修改外部 DB |
| M4 | 分层 savings 不重复统计，推荐等级可解释 |
| M5 | 安装器、CI、兼容矩阵、回滚、审计全部可用 |

## 11. PRD 二次审查结论

本报告反推 PRD 需要补齐：

1. `dtoken-event-ledger` crate。
2. `dtoken-compatibility` crate。
3. InstallationTransaction 模型。
4. CompatibilityCase 模型。
5. Hook integrity endpoint。
6. Headroom cache summary endpoint。
7. root README 与 structure check 的一致性。
8. CI gate 中 root README 与 duplicate PRD 检查。

上述内容已同步进入 `docs/PRD.md` 与 `support/scripts/check_structure.py`。

## 12. 结论

下一步不应该直接做复杂 UI 或真实压缩，而是先把 M1A-M1F 补齐。

正确顺序：

```text
验证现有 Rust workspace
  → RTK adapter
  → Headroom adapter
  → Project policy
  → Config patcher + event ledger
  → Compatibility matrix
  → Desktop installer wizard
```

这样能保证后续 UI 和真实集成都有可靠底座。