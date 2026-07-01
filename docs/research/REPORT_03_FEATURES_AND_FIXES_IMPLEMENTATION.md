# 深度研究报告 03：新增功能与问题修复如何实现

日期：2026-07-01  
适用项目：d-token  
目标：把 PRD 中新增功能、此前发现的问题、M1 遗留问题转成可执行工程方案。

## 1. 执行摘要

当前 d-token 已经完成了文档收敛和 M1 Rust skeleton，但距离“用户可用产品”还有几个关键工程缺口：

1. M1 workspace 还未经过本地 `cargo check --workspace` 与 `cargo test --workspace` 验证。
2. PRD 已要求 `dtoken-preprocessor`，但代码中尚未创建。
3. PRD 已要求 `dtoken-project-policy`，但代码中尚未创建。
4. 还没有 RTK/Headroom detection endpoints。
5. 还没有 config diff / backup / rollback。
6. 还没有一键安装向导。
7. 还没有 project-level Headroom compression policy。
8. 还没有防重复 savings attribution。
9. 旧 GitHub Actions workflow 曾有结构检查失败问题，CI 需要重新按新目录规范设计。

本报告把这些问题拆成 M1A-M5 的可执行实现路线。

## 2. 问题汇总

### 2.1 仓库结构类问题

已解决：

1. 只保留一个 PRD：`docs/PRD.md`。
2. 文档进入 `docs/`。
3. 代码进入 `code/`。
4. 测试、脚本、CI 模板和杂项进入 `support/`。
5. 根目录 AI 入口文件已更新。

仍需处理：

1. `.github/workflows` 与根目录极简规则存在现实冲突：GitHub Actions 真正启用时必须放回 `.github/workflows`。
2. `support/scripts/check_structure.py` 需要随 M1 新 crate 更新。
3. CI gate 需要恢复，但不能破坏根目录规则。

建议：

```text
support/ci/            存模板和说明
.github/workflows/     只放真正启用的 GitHub Actions workflow
```

PRD 可接受 `.github/workflows` 作为平台必要目录例外，但必须保持内容极小。

### 2.2 M1 编译验证问题

当前问题：

1. 已创建 Rust workspace。
2. 已创建多个 crate。
3. 但未实际运行 `cargo check --workspace`。
4. 未实际运行 `cargo test --workspace`。

解决方案：

1. 在本地执行：

```bash
cd code
cargo check --workspace
cargo test --workspace
```

2. 修正所有编译错误。
3. 添加 CI：

```bash
python support/scripts/check_structure.py
cd code && cargo check --workspace
cd code && cargo test --workspace
```

4. M1 只有在上述三项通过后才能标记为 Done。

### 2.3 RTK 集成缺口

PRD 已要求：

1. RTK binary detection。
2. RTK version detection。
3. RTK hook status detection。
4. RTK analytics import。
5. RTK telemetry status detection。
6. RTK tee visibility。

当前代码缺口：

1. 没有 `dtoken-preprocessor` crate。
2. 没有 `/rtk/status` endpoint。
3. 没有 `/rtk/gain` endpoint。
4. 没有 RTK command runner。
5. 没有 RTK JSON output parser。

### 2.4 Headroom 集成缺口

PRD 已要求：

1. Headroom runtime/proxy detection。
2. Headroom per-project compression policy。
3. Headroom Dashboard full migration。
4. CCR / memory / learn / MCP / output shaper 状态展示。

当前代码缺口：

1. 没有 `dtoken-headroom-adapter` crate。
2. 没有 `/headroom/status` endpoint。
3. 没有 `/headroom/policies` endpoint。
4. 没有 project policy manager。
5. 没有 compression recommendation engine。

### 2.5 配置安全问题

PRD 已要求所有配置修改支持：

1. diff。
2. backup。
3. rollback。
4. no silent mutation。
5. conflict detection。

当前代码缺口：

1. 没有 `dtoken-config-patcher` crate。
2. 没有 patch plan 数据模型。
3. 没有 backup directory 设计。
4. 没有 rollback endpoint。

## 3. M1A：修复与验证当前 Rust skeleton

### 3.1 目标

让当前 M1 skeleton 从“文件已落库”变成“可编译、可测试”。

### 3.2 任务

1. 本地运行 `cargo check --workspace`。
2. 修复 compile errors。
3. 本地运行 `cargo test --workspace`。
4. 修复 test errors。
5. 运行 `python support/scripts/check_structure.py`。
6. 更新 `docs/M1_STATUS.md`，从 `Started / Core Skeleton Created` 改成 `M1A Verified`。

### 3.3 可能错误点

1. `tracing` dependency 缺失。
2. axum 0.8 API 细节。
3. tokio process feature。
4. serde enum rename 与 JSON 示例不匹配。
5. integration test 缺依赖。
6. workspace path 错误。

### 3.4 验收标准

```bash
cd code
cargo check --workspace
cargo test --workspace
python ../support/scripts/check_structure.py
```

三者全部通过。

## 4. M1B：实现 dtoken-preprocessor crate

### 4.1 crate 职责

`dtoken-preprocessor` 不复制 RTK filters，而是做适配层。

职责：

1. 检测 RTK binary。
2. 检测 RTK version。
3. 检测错误同名包。
4. 检测 RTK hook 状态。
5. 读取 RTK telemetry status。
6. 调用 `rtk gain --all --format json`。
7. 调用 `rtk discover --all`。
8. 调用 `rtk session`。
9. 解析 RTK analytics。
10. 暴露 PreprocessorProfile。

### 4.2 模块设计

```text
code/crates/dtoken-preprocessor/
  Cargo.toml
  src/lib.rs
  src/rtk.rs
  src/status.rs
  src/analytics.rs
  src/model.rs
  tests/rtk_status.rs
```

### 4.3 Rust API

```rust
pub struct RtkAdapter {
    binary_path: Option<PathBuf>,
}

impl RtkAdapter {
    pub async fn detect() -> RtkStatus;
    pub async fn version(&self) -> Result<Option<String>>;
    pub async fn hook_status(&self) -> Result<HookStatus>;
    pub async fn telemetry_status(&self) -> Result<TelemetryStatus>;
    pub async fn gain_json(&self) -> Result<RtkGainReport>;
    pub async fn discover(&self) -> Result<RtkDiscoverReport>;
    pub async fn session(&self) -> Result<RtkSessionReport>;
}
```

### 4.4 Endpoint

新增：

```text
GET /preprocessors
GET /rtk/status
GET /rtk/gain
GET /rtk/discover
GET /rtk/session
```

### 4.5 风险

1. RTK JSON output schema 可能变化。
2. `rtk init --show` 输出可能不是稳定机器格式。
3. Windows native 与 WSL 行为不同。
4. 错误包检测需要谨慎，不能误报。

### 4.6 验收标准

1. 没装 RTK 时返回 `missing`。
2. 装错 RTK 时返回 `wrong_package`。
3. 装对 RTK 时返回 version。
4. `rtk gain --all --format json` 可解析。
5. Native Windows 提示 hook limitation。

## 5. M1C：实现 dtoken-project-policy crate

### 5.1 crate 职责

管理项目级 Headroom 压缩策略和推荐等级。

职责：

1. ProjectProfile CRUD。
2. ProjectCompressionPolicy CRUD。
3. 推荐等级计算。
4. 策略 diff。
5. 策略 rollback。
6. 与 RouteProfile / InstanceProfile 绑定。

### 5.2 模块设计

```text
code/crates/dtoken-project-policy/
  Cargo.toml
  src/lib.rs
  src/project.rs
  src/policy.rs
  src/recommend.rs
  src/history.rs
  tests/recommendation.rs
```

### 5.3 推荐算法 v0

先做规则引擎，不做 ML。

输入：

1. language_stack。
2. avg_request_tokens_7d。
3. avg_cli_noise_ratio_7d。
4. rtk_savings_7d。
5. headroom_savings_7d。
6. retrieve_rate_7d。
7. failure_rate_7d。
8. user_overrides。
9. risk_level。

规则：

```text
risk_level = high → safe
retrieve_rate > 15% → lower one level
failure_rate > 5% → lower one level
avg_request_tokens > 50k and failure_rate low → aggressive
large log/test project and RTK savings high → balanced/aggressive
new project with no data → safe
normal code project → balanced
```

### 5.4 Endpoint

新增：

```text
GET  /projects
POST /projects
GET  /projects/:id
PUT  /projects/:id
GET  /projects/:id/policy
PUT  /projects/:id/policy
POST /projects/:id/recommend-compression
GET  /projects/:id/policy-history
POST /projects/:id/policy-rollback
```

### 5.5 验收标准

1. 新项目默认 safe。
2. 常规项目推荐 balanced。
3. 大日志/大测试项目推荐 aggressive。
4. 高风险项目永远不自动推荐 max。
5. 用户手动 override 不被自动覆盖。
6. 每次 policy 变更都写 history。

## 6. M1D：实现 Headroom adapter skeleton

### 6.1 crate 职责

`dtoken-headroom-adapter` 负责检测和读取 Headroom 状态，不复制 Headroom core。

职责：

1. `headroom --version`。
2. `headroom doctor`。
3. `headroom perf`。
4. proxy status。
5. wrap status。
6. MCP status。
7. dashboard/stats endpoint。
8. savings ledger path。
9. CCR cache summary。
10. output shaper status。

### 6.2 模块设计

```text
code/crates/dtoken-headroom-adapter/
  Cargo.toml
  src/lib.rs
  src/status.rs
  src/proxy.rs
  src/policies.rs
  src/metrics.rs
  tests/status.rs
```

### 6.3 Endpoint

新增：

```text
GET /headroom/status
GET /headroom/proxy
GET /headroom/mcp
GET /headroom/stats
GET /headroom/policies
GET /headroom/cache-summary
GET /headroom/output-shaper
```

### 6.4 验收标准

1. 没装 Headroom 返回 `missing`。
2. 只装 npm SDK 不误判为 CLI 可用。
3. Python 版本不足时给出明确错误。
4. proxy 未启动时给出启动建议。
5. CCR cache 只展示 summary，不读取原文。

## 7. M1E：实现 config diff / backup / rollback skeleton

### 7.1 crate 职责

`dtoken-config-patcher` 是安全修改配置的唯一入口。

它必须覆盖：

1. agent config。
2. RTK config。
3. Headroom config。
4. d-token config。
5. shell profile / PATH 修改。

### 7.2 数据模型

```rust
pub struct ConfigPatchPlan {
    pub id: String,
    pub target_file: PathBuf,
    pub tool: ToolKind,
    pub agent: Option<String>,
    pub before_hash: String,
    pub after_hash: String,
    pub diff: String,
    pub risk_level: RiskLevel,
    pub requires_restart: bool,
}

pub struct RollbackPoint {
    pub id: String,
    pub created_at: String,
    pub backup_dir: PathBuf,
    pub target_files: Vec<PathBuf>,
    pub patch_ids: Vec<String>,
}
```

### 7.3 Endpoint

```text
POST /config/patches/plan
POST /config/patches/apply
GET  /config/patches
GET  /config/rollback-points
POST /config/rollback
```

### 7.4 验收标准

1. 每次修改前生成 diff。
2. 每次修改前创建 backup。
3. 修改失败自动 rollback。
4. 用户可手动 rollback。
5. 不支持解析的文件只读，不强行修改。
6. 所有敏感字段脱敏。

## 8. M2：一键安装向导实现

### 8.1 UI 页面

1. Welcome。
2. System Scan。
3. Tool Selection。
4. Agent Selection。
5. Provider Setup。
6. Project Setup。
7. Diff Review。
8. Apply Install。
9. Doctor Result。
10. Dashboard。

### 8.2 API

```text
GET  /install/scan
POST /install/plan
POST /install/apply
POST /install/rollback
```

### 8.3 状态机

```text
idle
  → scanning
  → planning
  → waiting_user_confirm
  → backing_up
  → applying
  → validating
  → doctor_running
  → done
  → failed
  → rolling_back
```

### 8.4 验收标准

1. 用户能只安装 RTK。
2. 用户能只安装 Headroom。
3. 用户能同时安装两者。
4. 用户能只接入已有 external tool。
5. 每次配置修改都有 diff。
6. 安装失败可 rollback。
7. Doctor 失败时不进入“已完成”状态。

## 9. M3：真实数据导入与分层归因

### 9.1 RTK 数据导入

1. 读取 `rtk gain --all --format json`。
2. 读取 `rtk discover --all`。
3. 读取 `rtk session`。
4. 可选只读 RTK tracking DB。
5. 不读取完整 tee 内容。

### 9.2 Headroom 数据导入

1. `/stats`。
2. `/stats-history`。
3. `/metrics`。
4. savings ledger。
5. per-agent/per-project breakdown。
6. output savings measured/estimated。

### 9.3 防重复统计

关键：不能把两层 savings 机械相加。

必须以 baseline graph 记录：

```text
raw_cli_tokens
rtk_cleaned_tokens
headroom_input_tokens
headroom_final_tokens
non_cli_context_tokens
final_llm_input_tokens
```

最终计算：

```text
rtk_savings = raw_cli_tokens - rtk_cleaned_tokens
headroom_savings = headroom_input_tokens - headroom_final_tokens
end_to_end_savings = raw_total_context_tokens - final_llm_input_tokens
```

## 10. M4：质量与高级功能

### 10.1 压缩质量控制

1. Holdout：部分请求不压缩，用作对照组。
2. Retrieve rate：CCR retrieve 频率太高说明压缩太狠。
3. Failure rate：压缩后任务失败率。
4. User rollback：用户回滚策略说明推荐有问题。
5. Agent feedback：用户手动标记回答质量。

### 10.2 推荐等级优化

推荐等级从 v0 规则引擎升级为：

1. Heuristic baseline。
2. Project-specific calibration。
3. User preference memory。
4. Risk-aware recommendation。
5. Confidence scoring。

### 10.3 Headroom 高级开关

项目级支持：

1. CCR。
2. memory。
3. learn。
4. MCP。
5. output shaper。
6. CacheAligner。
7. CodeCompressor。
8. SmartCrusher。
9. Kompress/text。
10. verbosity setting。
11. holdout ratio。

## 11. M5：生产硬化

### 11.1 Installer hardening

1. macOS signing / notarization。
2. Windows code signing。
3. updater。
4. uninstall cleanup。
5. rollback after crash。
6. path/shim compatibility。

### 11.2 CI hardening

需要 GitHub Actions：

```yaml
steps:
  - python support/scripts/check_structure.py
  - cd code && cargo check --workspace
  - cd code && cargo test --workspace
```

说明：虽然根目录极简，但 `.github/workflows` 是 GitHub Actions 的平台必要路径，可作为例外保留。

### 11.3 Compatibility matrix

必须覆盖：

1. macOS Intel。
2. macOS Apple Silicon。
3. Windows native。
4. Windows WSL。
5. Claude Code。
6. Codex。
7. Cline。
8. OpenCode。
9. Cursor。
10. Gemini CLI。
11. RTK external / managed / builtin。
12. Headroom external / managed / disabled。

## 12. 对之前问题的逐项修复映射

| 问题 | 修复方案 | 阶段 |
|---|---|---|
| 多版本 PRD 混乱 | 只保留 `docs/PRD.md` | 已完成 |
| 文档/代码/杂项混放 | `docs/` / `code/` / `support/` | 已完成 |
| PRD 未写一键安装 | 新增 installer wizard | 已补 PRD，M2 实现 |
| 未写 Headroom 项目级压缩 | 新增 ProjectCompressionPolicy | 已补 PRD，M1C/M3 实现 |
| 未写推荐等级 | 新增 CompressionRecommendationEngine | 已补 PRD，M1C/M4 实现 |
| RTK 只是抽象层 | 明确 `rtk-ai/rtk` 一等集成 | 已补 PRD，M1B/M3 实现 |
| 双层 savings 可能重复统计 | 新增 baseline graph 和计算公式 | 已补 PRD，M3 实现 |
| 配置修改不安全 | 新增 diff/backup/rollback | 已补 PRD，M1E/M2 实现 |
| M1 未验证编译 | cargo check/test gate | M1A |
| CI 结构失败历史 | 新 CI gate + `.github/workflows` 例外 | M5 |
| Windows/macOS 差异 | WSL/native/macOS CLT/Keychain 等检测 | M2/M5 |

## 13. 最终路线图

```text
M1A  Verify current workspace
M1B  Add dtoken-preprocessor
M1C  Add dtoken-project-policy
M1D  Add dtoken-headroom-adapter
M1E  Add dtoken-config-patcher
M2   Desktop installer wizard + dashboard skeleton
M3   Real RTK/Headroom integration + project policy
M4   Quality metrics + recommendation engine + double-count prevention
M5   Production installer + CI + compatibility matrix
```

## 14. 结论

下一步不应该直接冲 M2 UI，而应该先补完 M1B-M1E。

正确顺序是：

```text
先让 core 能检测 RTK / Headroom / project policy / config rollback
再做 Desktop UI
最后做真实压缩和复杂推荐
```

这能避免做出一个漂亮但不可用的图形壳。
