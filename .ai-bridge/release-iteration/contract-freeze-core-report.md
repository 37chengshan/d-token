# CF-2 Contract Freeze Core Report（dtoken-core-api 合同消费面冻结）

## Scope

- Baseline: `01bef84`（merge: freeze shared release contract types，CF-1 合并 commit）
- Owned implementation: `code/crates/dtoken-core-api/src/lib.rs`
- Direct behavior tests: 同一文件内 `mod tests`（CF-2 区块，19 个新增直接测试）
- 切片报告：`.ai-bridge/release-iteration/contract-freeze-core-report.md`
- 未修改：dtoken-types（只读）、其他 crate、desktop、support、docs、Cargo 文件；无 commit/push/branch 操作。
- 边界自证：owned 文件仅 `code/crates/dtoken-core-api/src/lib.rs` 与本报告；`git diff --check` 通过。

## 冻结的合同消费面（CF-2 目标逐项落实）

### 1. ConfigTransactionState 推进语义 + 逐 Agent 事务十字段（§6.1/§8.1）

- `AgentTransactionRecord { agent_id, state: ConfigTransactionState, evidence: AgentTransactionEvidence }`：
  复用 CF-1 冻结类型，不新建状态机/receipt；每一 Agent 独立持有状态与十字段 evidence，批量配置仅 UI 编排（§8.1）。
- `advance(target)`：先校验冻结迁移表（复用 `ConfigTransactionState::can_transition_to`），再校验目标状态所需的十字段 evidence 完整（fail-closed），非法迁移/缺失证据一律拒绝且记录保持原样。
- 十字段 evidence 校验矩阵（与冻结字段一一对应）：
  - `planned`：transaction_id / source_revision / before_digest / redacted_diff_digest / planned_after_digest 非空
  - `backed_up`：backup_manifest 存在
  - `applied` / `completed`：receipt_id 存在
  - `readback_verified`：readback_digest 存在且必须等于 planned_after_digest（不匹配 → ReadbackMismatch）
  - `capability_verified`：capability_evidence 存在
  - `rolled_back`：rollback_receipt_id 必须存在（无回滚 receipt 拒绝回滚，fail-closed）
  - `failed`：可从任何非终态进入，不要求额外 evidence
- `verify_readback(observed_digest, rollback_receipt_id)`：applied 后复读验证——digest 匹配 → `readback_verified`；不匹配 → 仅该 Agent 进入 `rolled_back` 并写入 rollback_receipt_id（BDD-1 直接实现）。
- `advance_agent_batch(records, requests)`：逐 Agent 独立推进；某 Agent 失败只影响该 Agent 的结果，不整批回滚；重复 `agent_id` fail-closed，避免静默首项匹配。
- `projection()`：只投影冻结十字段字段 + agent_id + 内部状态码（snake_case，仅技术详情/Doctor/日志），凭据/原始配置/绝对路径绝不进入投影；普通页面通过 `transaction_status_word` 收敛到正在配置/配置成功/配置失败。

### 2. RequestLog → OptimizationReceipt 投影（§6.2/§7.2，RUNTIME-9）

- `project_optimization_receipt(&RequestLog) -> OptimizationReceipt`：
  - local_original_tokens / local_optimized_tokens ← input_tokens_before / input_tokens_after（缺失保持 None）
  - provider_input_tokens / provider_output_tokens 仅在整个 accounting 校验通过且 evidence kind=`ProviderActual` 时取自 `provider_actual_sent.measurement.usage`；无效、非 actual 或缺失均保持 `None`（unavailable），绝不置 0。
  - claimable_tokens_avoided ← 仅取自 `claimable_signed_savings()` 的严格正数（归因有效且真实节省时）；零/负数保持 None，Provider usage 单独不提升 claimable（独立分账，§6.2）。
  - validation_status = `Unavailable`（RequestLog 本身无 artifact-derived 验证证据，不伪造 Passed/Failed）。
  - protected_content 保持空描述符（仅描述符、绝不携带内容，且不编造）。
  - skip_or_failure_reason ← log.error；provider_or_model_changed 保持 false（无 user-target 证据不猜测变更）。

### 3. §3.6 冻结状态词表映射

- `FROZEN_STATUS_WORDS`（10 个冻结词：未安装/未选择/正在配置/配置成功/配置失败/工具能力已接入/「已检测/不支持本地路由」/正在同步/同步成功/同步失败）+ `FROZEN_ASSET_LIST_WORDS`（已安装/未安装/已配置/未配置，MUST 1/§11.2）。
- `user_status_word_for(internal_code) -> Result<&'static str, StatusVocabularyError>`：内部码 → §3.6 词表；`adapter`/`route`/`credential_reference`/`needs-source-fix`/`source_conflict`/`healthy`/`active`/`supported` 及一切未知码 fail-closed（内部码只进技术详情/Doctor/日志，绝不进普通 UI）。
- `transaction_status_word(state)`：discovered→capability_verified 映射「正在配置」，completed 映射「配置成功」，rolled_back/failed 映射「配置失败」。
- `asset_list_status_word(installed, configured)`：仅返回冻结资产清单词。

## BDD 对照

| BDD | 实现 | 测试 |
|---|---|---|
| 某 Agent applied 后 readback 失败 → 该 Agent 进入 `rolled_back` 且 `rollback_receipt_id` 存在，其他 Agent 不受影响 | `verify_readback` 不匹配路径 + 单 Agent 独立状态 | `readback_failure_rolls_back_only_the_failing_agent`、`batch_advance_is_per_agent_independent` |
| RequestLog 无 Provider usage → 投影 OptimizationReceipt 的 Provider 字段为 unavailable 而非 0 | provider_input/output_tokens 仅取自 provider_actual_sent，缺失保持 None | `projection_keeps_provider_usage_unavailable_when_missing` |

## 直接行为测试（19 个新增，全部通过）

1. `agent_transaction_advances_through_full_legal_chain` — 合法全链 discovered→…→completed
2. `illegal_state_transitions_are_rejected` — 跳级/无备份回滚/终态吸收
3. `missing_cross_section_evidence_rejects_the_transition` — 十字段证据缺失拒绝（fail-closed）
4. `readback_verified_requires_matching_digest` — readback_digest 与 planned_after_digest 不匹配拒绝
5. `readback_failure_rolls_back_only_the_failing_agent` — 单 Agent 回滚不影响其他 Agent
6. `readback_success_advances_only_the_verified_agent` — 成功仅推进该 Agent
7. `rollback_without_receipt_is_rejected_fail_closed` — 无 rollback_receipt_id 拒绝回滚
8. `batch_advance_is_per_agent_independent` — 批量推进逐 Agent 独立 + 未知 Agent fail-closed
9. `projection_keeps_provider_usage_unavailable_when_missing` — usage 缺失保持 None 非 0
10. `projection_sources_provider_usage_from_provider_actual_sent` — Provider usage 正确投影且单独不提升 claimable
11. `transaction_projection_exposes_only_cross_section_fields` — 敏感字段和绝对路径缺席
12. `vocabulary_maps_known_codes_and_fails_closed_on_unknown` — 已知码映射、未知/机制码 fail-closed
13. `transaction_states_map_to_frozen_user_words` — 事务状态完整映射到三种批准用户词
14. `invalid_or_non_provider_accounting_keeps_provider_usage_unavailable` — 无效/非 actual usage fail-closed
15. `duplicate_agent_records_fail_closed_without_mutation` — 重复 Agent identity 不静默匹配
16. `evidence_matrix_rejects_missing_applied_capability_and_completed_fields` — evidence 矩阵遗漏路径
17. `estimated_active_accounting_projects_claimable_savings` — 有效 EstimatedActive 正向 claimable 投影
18. `valid_local_estimate_in_provider_slot_stays_unavailable` — accounting 有效但 evidence 非 ProviderActual 时 usage 仍 unavailable
19. `negative_signed_savings_are_not_claimable_avoided_tokens` — 零/负 signed savings 不声明 avoided tokens

## Verification

- `cargo fmt --all -- --check`：PASS
- `cargo clippy --workspace --all-targets -- -D warnings`：PASS（fresh worktree 先按 package.json 声明流程准备 sidecar/dist；生成物均 gitignored）
- `cargo test -p dtoken-core-api`：最终复验见主代理验证输出；新增测试纳入完整 214-test package 结果（1 ignored）。
- `git diff --check`：PASS；owned diff 为 core-api 文件 + 本报告。

## Risks / Blockers

- 无 blocker。待办观察（不影响本切片）：
  - PR-1 将在 dtoken-types 串行槽扩展 RequestLog usage 字段后，本投影可消费更直接的 usage 来源；当前已按冻结类型取 provider_actual_sent，语义不变。
  - 词表映射目前为消费方（PR-3/PR-4）提供 10+4 冻结词；CF-3 将登记 §3.6 词表与内部码映射的正式矩阵，本切片函数与其保持一致。

## Independent review closure

- 独立复审：`APPROVE`，初审 1 Medium + 4 Low 全部关闭。
- 复审新增 3 Low（报告 Scope 过期、负 savings claim、ProviderActual kind 独立负测）已在提交前全部修复。
- 最终提交前复验 fmt/test/clippy/diff-check。
