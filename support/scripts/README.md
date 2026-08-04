# scripts

本目录存放本地开发脚本和辅助检查脚本。

正式产品代码不要放在这里；正式代码统一放入 `code/`。

## Git 操作审计

执行 commit、分支操作、merge、rebase 或 push 前先记录：

    python3 support/scripts/git_operation_audit.py record \
      --operation commit \
      --summary "完成 Optical Phase B 与 macOS 基线验证"

随时查询最近记录：

    python3 support/scripts/git_operation_audit.py query --limit 20
    python3 support/scripts/git_operation_audit.py query --branch optical --json
    python3 support/scripts/git_operation_audit.py validate --json

审计文件为 support/git-operations.json。commit 记录保存操作前 HEAD 和一句话简介；结果 commit 可通过相邻 Git 历史确定，避免让被提交文件引用自身尚未生成的哈希。

### 分阶段待提交计划（schema v2）

尚未执行的提交不得伪装成真实 Git 操作。`support/pending-commits.json` 只记录阶段计划和提交状态；`support/git-operations.json` 只记录真实发生的 branch/commit/merge/rebase/push 操作。

每个阶段必须写清楚：

```text
phase / title / summary
created_at / updated_at
记录时所在 branch（仅观察信息，不是目标分支约束）
精确 files 列表
staging_mode：whole-file / hunk-level / mixed
shared_files：必须按 hunk 暂存的共享文件
verification.status 与实际 checks
blockers
submitted / submitted_at / commit_hash
```

新增计划示例：

```bash
python3 support/scripts/git_operation_audit.py plan \
  --id optical-n8-stable-fingerprint \
  --phase N8 \
  --title "稳定请求指纹" \
  --summary "增加 raw-byte 与 canonical JSON SHA-256 证据" \
  --commit-message "feat(optical): add stable request fingerprints" \
  --file-path code/crates/dtoken-compress/src/optical/plan_ir.rs \
  --test "cargo test -p dtoken-compress" \
  --verification-status passed \
  --status ready
```

查询、验证和更新状态：

```bash
python3 support/scripts/git_operation_audit.py query-plans --json
python3 support/scripts/git_operation_audit.py validate-plans --json
python3 support/scripts/git_operation_audit.py mark-plan \
  --id optical-n8-stable-fingerprint \
  --status ready \
  --verification-status passed
```

状态语义：

```text
pending     仍在实现或验证
ready       已实现并验证，但 submitted=false
blocked     存在明确阻塞，必须写 blockers
committed   已完成真实 commit，submitted=true，且填写 submitted_at/commit_hash
superseded  已被新的阶段计划替代
```

记录规则：

1. 不设置“目标分支”；branch 只表示记录时工作区实际所在分支。
2. 一个文件被多个阶段共享时，必须列入 `shared_files`，并使用 `hunk-level` 或 `mixed`。
3. 只有命令真实 exit 0 才能把 verification 标为 passed。
4. 未执行 commit 时 `submitted=false`、`submitted_at=null`、`commit_hash=null`。
5. 执行真实 commit 前，仍必须先用 `record --operation commit` 写入 `support/git-operations.json`。
6. commit 完成后再用 `mark-plan --status committed --commit-hash ...` 回填阶段账本。

两个 JSON 文件不可互相替代。

## 发布门禁

`run_release_verification.ps1` 默认执行 **no-touch** 发布验证，不调用任何真实 Agent，也不会启动、停止或修改外部 Headroom、RTK、cc-switch：

```powershell
support\scripts\run_release_verification.ps1
```

完整候选门禁包含：仓库结构、发布范围、版本一致性、发布合同与候选溯源、前端 Vitest、TypeScript、生产构建、Playwright E2E、Rust fmt、Clippy 和 workspace tests。对应的可复用入口为：

```bash
python3 support/scripts/check_structure.py
python3 support/scripts/check_release_scope.py
python3 support/scripts/check_release_version.py

cd code/apps/desktop
npm run test:run
npx tsc --noEmit
npm run build
npm run test:e2e

cd ../..
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

只有在用户已单独授权真实 OpenCode 验收时，才显式追加：

```powershell
support\scripts\run_release_verification.ps1 -IncludeOpenCode
```

发布门禁不测试 Codex。`-SkipOpenCode` 暂时保留为兼容参数，但默认行为本身已经是不调用 OpenCode。

### 版本与发布范围

- `check_release_version.py` 要求 `package.json`、`package-lock.json`、Tauri 配置、Cargo workspace、所有本地 Rust package 和 `CHANGELOG.md` 版本一致；tag 发布还要求 tag 严格等于 `v<version>`。
- `check_release_scope.py` 阻止本地 Agent 配置、版本化 `headroom-*/` 对照工作区、测试/支持产物、OS 元数据和签名私钥进入被跟踪的发布范围，并确认这些本地目录仍被忽略而没有被删除。
- `generate_release_metadata.py` 从锁文件和 Cargo metadata 生成 CycloneDX SBOM、依赖许可证 CSV、基础安全扫描报告、Release Notes 与构建信息。正式 tag workflow 要求 `cargo-audit` 可用，并对 npm high/critical 与 RustSec vulnerability fail closed。

### 候选溯源与平台包审计

每份发布验证报告会记录 Git HEAD、dirty 状态、候选指纹、操作系统/架构、PowerShell、Node、rustc 和 Cargo 版本。正式桌面打包时，`prepare_desktop_sidecars.ps1 -Release` 或 macOS 对应脚本会写入 `code/target/release/d-token-release-candidate.json`；bundle audit 要求该指纹与当前工作树一致，并要求 App 与安装包生成时间晚于候选戳。源码变化后必须重新打包，旧安装包不能继续冒充当前候选。

`run_macos_release_package_audit.sh` 在一次性临时目录挂载 DMG、复制 App、验证版本/identifier、主程序和三个 sidecar/launcher、项目与上游法律文件、签名、可选 notarization/Gatekeeper、临时启动与卸载，同时拒绝大型 `codebase-memory-mcp` 上游可执行文件、Headroom 参考目录、本地配置、测试产物、私钥和 OS 元数据进入包内。

Windows bundling 由 `tauri-with-sidecar.mjs` 调用 `sign_windows_sidecars.ps1`：只对 Tauri 实际引用的 MSVC externalBin 签名一次，再逐字节复制为 release 证据副本，随后同时验证 prepared、release 与安装后 sidecar 的 Authenticode。缺证书、signtool、时间戳或有效签名时 workflow 必须失败。

`run_release_gate_contract_test.ps1` 是轻量回归检查，可在不运行真实 Agent 或外部服务的情况下验证 CI、no-touch 门禁、候选溯源和旧产物拦截规则没有被删掉。`run_release_candidate_provenance_test.ps1` 会在系统临时目录创建一次性 Git 仓库，动态证明“源码变化会让旧候选戳失效”和“早于候选戳的产物会被拒绝”；测试结束会删除它自己创建的临时目录。

### 发布性能与稳定性门

`run_release_performance_gate.ps1` 将现有 frontend bundle budget 与真实 release-profile `dtoken-proxy` 本地 routed workload 合并成一份候选绑定、metadata-only 的 JSON。默认硬门为 p95 ≤ 250 ms、p99 ≤ 500 ms、失败率 0%、proxy peak RSS ≤ 256 MiB、至少 64 个计量请求；同时必须通过 8 并发批次、5 秒短 soak、预期上游错误、客户端断开后的存活复检和 Event Ledger 请求计数。macOS 与 Windows release workflow 都运行同一门，报告写入 `code/target/release/metadata/release-performance-gate.json` 并进入候选 SHA-256 manifest。

CI runner 如需明确校准，可设置 `DTOKEN_PERF_MAX_P95_MS`、`DTOKEN_PERF_MAX_P99_MS`、`DTOKEN_PERF_MAX_FAILURE_RATE_PERCENT`、`DTOKEN_PERF_MAX_PEAK_RSS_MIB`、`DTOKEN_PERF_MIN_REQUEST_COUNT`、`DTOKEN_PERF_MEASURED_REQUESTS`、`DTOKEN_PERF_CONCURRENCY`、`DTOKEN_PERF_SOAK_SECONDS` 和 `DTOKEN_PERF_SOAK_CONCURRENCY`。默认值保持发布严格，缺 RSS、路径证据、ledger 计数或报告时均 fail closed。RSS 只覆盖 runtime proxy；WebView 进程组、30 分钟与 24 小时 endurance 仍须平台专项验收，短 soak 不冒充长稳证据。

### Headroom capability snapshot 审计

`run_headroom_capability_coverage_audit.ps1` 只接受候选绑定的 metadata-only JSON snapshot，不再用源码字符串、前端文案或 `fixture_runtime` 成功状态推断能力完成。调用方必须显式传入 `-CapabilitySnapshot` 与 `-ExpectedCandidateId`，或传入 `-ReleaseReport` 且报告同时提供 `candidate.candidate_id` 和 `candidate.capability_snapshot_path`；报告内的相对路径以 release report 所在目录解析。脚本不会扫描工作树或自动选择历史报告来猜测候选版本。

Snapshot 合同：

```json
{
  "schema_version": 1,
  "source": "dtoken-owned-runtime",
  "revision": "<64-character candidate_id>",
  "source_registry_revision": "headroom-backend-capabilities-v2",
  "collected_at": "2026-07-30T12:00:00Z",
  "capabilities": [
    {
      "id": "compression",
      "state": "ready",
      "implementation_revision": "compression-pipeline-v2",
      "source_evidence_revision": "runtime-proof-v2",
      "evidence_revision": "<same candidate_id>",
      "freshness": "recent",
      "reason_codes": ["native_runtime_evidence"]
    },
    {
      "id": "optical_active_transform",
      "state": "implemented_disabled",
      "implementation_revision": "optical-active-v1",
      "source_evidence_revision": "optical-release-policy-v1",
      "evidence_revision": "<same candidate_id>",
      "freshness": "recent",
      "reason_codes": ["policy_shadow_only"]
    }
  ]
}
```

所有 active-required capability 都必须出现：`compression`、`ccr`、`memory_learn`、`output_shape`、`integrated_rtk`、`proxy_sdk_mcp`、`stats_history_metrics`、`routing_protocols`、`doctor_dashboard_logs_export`、`project_policy`、`backup_rollback`。`state` 只能用 `ready`、`degraded`、`unavailable`、`blocked`、`implemented_disabled`，`freshness` 只能用 `live`、`recent`、`stale`、`unknown`。active-required 项仅 `ready`/`degraded` 且 freshness 非 `unknown` 时计为完成；`unavailable`、`blocked`、`implemented_disabled` 一律阻断。每项 `evidence_revision` 必须与 snapshot 和候选 `revision` 完全一致；`implementation_revision` 与 `source_evidence_revision` 则保留原生 registry 的实现和证据修订，不得由转换器伪造完成态。每项都必须提供非空且不重复的小写 metadata `reason_codes`，不得放入原始错误、prompt、response 或凭据。

完整 release verification 在 mock/active runtime 步骤后运行 `run_candidate_capability_registry_snapshot.ps1`。该脚本不重新构建二进制，只启动已经构建的 `dtoken-proxy`，强制 `127.0.0.1`、显式空配置和随机 instance nonce，验证 runtime identity 后读取一次原生 `/capabilities` 并安全停止精确 PID。`generate_headroom_capability_snapshot.py` 随后严格校验原生 `CapabilityRegistrySnapshot` 的 schema、重复项、枚举值和 aggregate ID，再生成上述 wrapper；它只改变外层 candidate binding，逐项原样复制 `state`、`freshness`、`reason_codes`、`implementation_revision`，并把原生 `evidence_revision` 保存在 `source_evidence_revision`。任一步缺失或未知都会使门禁失败。

当前产品策略下 Optical production active 必须以 `implemented_disabled` 加 `policy_shadow_only` 或 `policy_disabled` 记录。它可单独证明 shadow/disabled 策略合规，但 `counts_toward_headroom_active_complete` 永远为 `false`，不能填补任何 Headroom active 缺口。脚本不访问网络、不启动服务，也不读取源码、fixture、auth、secret 或 raw prompt/response。

Windows PowerShell 5.1 可直接执行动态审计；无 PowerShell 的主机运行静态与 fixture 契约测试：

```bash
python3 support/scripts/test_headroom_capability_coverage_audit_contract.py
```

干净 checkout 在执行包含 Tauri crate 的 `cargo clippy/test --workspace` 前必须具备当前 host 的真实 `dtoken-proxy` externalBin。Linux CI 使用 `prepare_desktop_sidecar_for_checks.sh`，macOS 使用 `prepare_desktop_sidecars_macos.sh --debug`，Windows 使用 `prepare_desktop_sidecars.ps1`；生成物均被忽略，不作为源码提交。

### GitHub 自动化边界

- `.github/workflows/ci.yml` 对普通 push/PR 执行结构、发布范围、版本、前端、E2E 与 Rust 门禁，不创建发布。
- `.github/workflows/macos-release.yml` 是可复用且可手动触发的 macOS ARM64 unsigned prerelease 候选验证；它负责 DMG、包审计、SHA-256、SBOM/license/security/provenance 与 artifact upload，但不宣称 Developer ID、notarization、staple 或 Gatekeeper。
- `.github/workflows/windows-release.yml` 是可复用且可手动触发的 Windows x64 unsigned prerelease 候选验证；它负责 NSIS 静默安装/启动/卸载、文件一致性、SHA-256、SBOM/license/security/provenance 与 artifact upload，但不宣称 Authenticode 或 SmartScreen reputation。
- `.github/workflows/public-unsigned-prerelease.yml` 在 `v*` tag 下以同一 run/同一 commit 聚合 macOS ARM64 与 Windows x64 两个 reusable job。只有两端全部成功，且 release notes、SHA-256、SBOM、license、安全扫描与 provenance 证据齐备时，才创建或更新 GitHub public prerelease。
- 所有 workflow 都不提交或推送源码。创建 tag 与触发 tag workflow 仍由维护者显式完成。
