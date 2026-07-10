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

门禁明确包含以下质量检查：

- 前端 Vitest：`npm run test:run`
- TypeScript：`npx tsc --noEmit`
- 前端生产构建：`npm run build`
- Rust 格式：`cargo fmt --all -- --check`
- Rust 静态检查：`cargo clippy --workspace --all-targets -- -D warnings`
- Rust 全工作区测试：`cargo test --workspace`

只有在用户已单独授权真实 OpenCode 验收时，才显式追加：

```powershell
support\scripts\run_release_verification.ps1 -IncludeOpenCode
```

发布门禁不测试 Codex。`-SkipOpenCode` 暂时保留为兼容参数，但默认行为本身已经是不调用 OpenCode。

每份发布验证报告会记录 Git HEAD、dirty 状态、候选指纹、操作系统/架构、PowerShell、Node、rustc 和 Cargo 版本。正式桌面打包时，`prepare_desktop_sidecars.ps1 -Release` 会写入 `code/target/release/d-token-release-candidate.json`；bundle audit 要求该指纹与当前工作树一致，并要求 App 与安装包生成时间晚于候选戳。源码变化后必须重新打包，旧安装包不能继续冒充当前候选。

`run_release_gate_contract_test.ps1` 是轻量回归检查，可在不运行真实 Agent 或外部服务的情况下验证 CI、no-touch 门禁、候选溯源和旧产物拦截规则没有被删掉。

`run_release_candidate_provenance_test.ps1` 会在系统临时目录创建一次性 Git 仓库，动态证明“源码变化会让旧候选戳失效”和“早于候选戳的产物会被拒绝”；测试结束会删除它自己创建的临时目录。
