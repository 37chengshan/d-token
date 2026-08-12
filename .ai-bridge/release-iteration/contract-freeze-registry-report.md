# CF-3 Contract Freeze Registry Report（CandidateEvidenceEnvelope schema + Agent 四来源并集矩阵 + 词表登记）

## Scope

- 切片：CF-3 contract-freeze-registry（波次 W1，依赖 CF-1/P0）
- Worktree/branch/PR：`codex/final/contract-freeze-registry` / `final-contract-freeze-registry` / PR-CF3
- 只写文件（本切片允许清单内）：
  - `support/schemas/candidate-evidence-envelope.schema.json`（新建 `support/schemas/`）
  - `support/fixtures/agent-capability-matrix.json`
  - `support/tests/test_contract_freeze_registry.py`
  - `.ai-bridge/release-iteration/contract-freeze-registry-report.md`（本报告）
- 未修改：Rust/TS 业务代码、docs、workflows、scripts、其他 fixture、`generate_release_gate_status.py`（PR-7 拥有）；无 git/commit/branch 操作。
- 验证命令：`python3 -m pytest support/tests/test_contract_freeze_registry.py -q` → **56 passed**。

## 1. CandidateEvidenceEnvelope schema 冻结（Draft 2020-12）

- 文件：`support/schemas/candidate-evidence-envelope.schema.json`（`$schema` = draft 2020-12，`$id` = d-token local URI，`schema_version` const `1.0.0`）。
- **严格性**：顶层 `additionalProperties:false`；`required` 覆盖全部 §13 基础字段（schema_version/node_id/status/reason_codes/candidate_commit/source_digest/registry_digest/corpus_digests/started_at/finished_at/platform/producer_revision/artifact_sha256/agent_id/capability_tier/privacy_assertions/not_applicable_reasons/evidence_kind + route/optical 9 个 digest 字段：route_decision/capability + 七个 optical），字段本身 required，N/A 只能显式 `null`。
- **pattern**：`candidate_commit` = `^[0-9a-f]{40}$`；所有 digest（source/registry/artifact_sha256/route_decision/capability/renderer/profile/font/tokenizer/pricing/cache/eval）= `^[0-9a-f]{64}$`；`started_at/finished_at` = RFC3339 date-time（format + pattern 双保险）。
- **reason_codes fail-closed**：`status != pass` 时 `reason_codes` 必须 `minItems:1`（allOf conditional）；`status=pass` 可保留空列表（符合设计示例 `reason_codes: []`）；reason_codes items 与 `not_applicable_reasons` 所有 reason 均拒绝纯空白（`pattern: \S`）。
- **platform strict**：`{os: macos|windows|linux, arch: x86_64|aarch64, toolchain}`，`additionalProperties:false`。
- **privacy_assertions**（必填，六项全部 `const:true`，缺失或任一为 false → 校验失败，即 blocked 语义）：
  `no_secret` / `no_complete_prompt_or_response` / `no_source_code` / `no_absolute_paths` / `no_raw_command_output` / `redaction_verified`。
- **N/A 语义（不得省略后当作通过）**：`agent_id`、`capability_tier`、`route_decision_digest`、`capability_digest` 及七个 optical digest 全部是 required 字段、值可为 null；`not_applicable_reasons` 为 required 对象，`additionalProperties:false`，每个可空字段一个非空 reason（minLength 1 + `\S`）。JSON Schema `if/then/else` conditional（共 14 个 allOf 块）保证：
  - 字段为 null → 对应 reason 必填且非空；字段非 null → 不允许矛盾 reason（`not` required）；
  - `evidence_kind=route` → `route_decision_digest`/`capability_digest` 必须非 null（字符串）且不允许 N/A reason；
  - `evidence_kind=optical|benchmark|paired_eval` → 七个 optical digest 必须非 null 且不允许 N/A reason。
- **时间顺序（下游强制，非 schema 跨字段比较）**：`finished_at >= started_at` 无法由 JSON Schema 跨字段校验，schema 描述与本报告明确声明该规则由 downstream aggregator/consumer 解析 RFC3339 后强制（`finished_at < started_at` 必须 blocked）；cross-artifact helper/测试覆盖结束早于开始被拒、合法时序通过（`test_timestamp_finished_before_started_rejected` / `test_timestamp_legal_order_passes`）。

### 新增 `evidence_kind` 字段的原因（合同要求 2 要求的说明）

设计 §13 原样只列了字段清单，没有判别字段。CF-3 冻结 schema 增加必填 `evidence_kind`（enum：`generic|route|optical|benchmark|paired_eval`），原因：

1. `capability_tier` 描述的是 **Agent 的官方能力层**（route | mcp_hook | detection_only），而 route digest 与 optical digest 的适用性描述的是 **artifact 的证据类别**（证据信封属于哪类证据）。两者正交：一个 `detection_only` 的 Agent 也可能产出 optical benchmark 证据，一个 route Agent 也可能产出 generic 聚合报告。
2. 没有判别字段时，无法用 JSON Schema conditional 表达「route 类 artifact 必须非 null route digests」与「optical/benchmark/paired_eval 类必须非 null 七个 digests」这两条互斥规则——digest 适用性取决于证据类别而非 agent tier。
3. `evidence_kind` 是 required、enum 受限，不引入开放性；向后兼容以 `schema_version` const 控制（变更需新契约冻结切片）。

## 2. Agent 四来源并集矩阵冻结

- 文件：`support/fixtures/agent-capability-matrix.json`（`matrix_schema_version` 1.0.0，frozen_date 2026-08-12）。
- **四来源**（root 明示 `source_contracts`）：
  1. 本轮直接要求（UNIFIED §5 产品链路行 + §10 OpenCode 两链 + PLAN CF-3）：opencode / claude-code / codex / cursor / cline / openclaw；
  2. 目标规格 v2.1（docs/d-token_AI_product_target_spec_v2.md §2 术语与命名规范术语表 Agent 行 + §17.1）：opencode / claude-code / codex / cline / cursor；
  3. Rust 正式注册表（agent.rs KNOWN_AGENT_ADAPTERS L318-572）：23 个 AgentAdapterDescriptor（测试解析 Rust 源提取 id，防手工清单漂移）；
  4. cc-switch fixture app types（support/fixtures/cc-switch/cc-switch.db，schema-12 providers.app_type）：claude / codex / gemini，经 aliases `claude→claude-code`、`codex→codex`、`gemini→gemini-cli` 落入矩阵。
- **测试真实锚定四来源（非矩阵自证）**：测试对每个来源独立读取其实物载体并断言与矩阵精确相等——
  - Rust 23：解析 `agent.rs KNOWN_AGENT_ADAPTERS` ids（`test_rust_registry_has_23_ids` / `test_matrix_covers_all_23_rust_registry_agents`）；
  - cc-switch：只读查询 committed fixture `select distinct app_type from providers union select app_type from proxy_config`，与 `source_contracts.cc_switch_fixture_app_types.app_types` 精确相等（`test_cc_switch_app_types_exact_from_fixture`）；**不执行 generator**（非 `--check` 模式会打印 secret marker），不读取 settings_config 内容；
  - target spec：局部 section/table 解析 §2 术语表 Agent 行（Claude Code/Codex/OpenCode/Cline/Cursor）→ ids，与 `target_spec_v2_1.agents` 精确相等（`test_target_spec_terms_exact`）；
  - UNIFIED：局部 section 解析 §5 产品链路行（OpenCode / Claude Code / Codex / Cursor / Cline / OpenClaw）→ ids，与 `this_round_direct_requirements.agents` 精确相等（`test_unified_product_chain_exact`）。
- **并集完整性**：23 个 Agent 全部登记，单一来源 Agent 不丢失；cc-switch 三个 app type 经 alias 全量落入；OMP/cc-switch 作为 AlwaysReadOnly 外部控制边界放入 `read_only_controls`（不冒充 Rust Agent，矩阵 agents[] 不含 omp/cc-switch）。
- **分布**：route 11（claude-code/codex/cline/goose/kilo-code/opencode/openclaw/gemini-cli/roo-code/continue/aider）+ detection_only 12（cursor/commandcode/claude-desktop/hermes/copilot-cli/vscode-copilot-chat/pi/cortex-code/windsurf/antigravity/mistral-vibe/openhands）+ mcp_hook 0；与批准设计「11 verified Route + 12 detection-only」完全一致。
- **adapter 事实分离**：`rust_registry_23_agents` 中 `approved_verified_adapter_count: 11`（批准已验证 Route 数，等于 release_tier_summary.route）与 `code_declared_configurable_adapters: 15`（agent_transport.rs ADAPTERS 中 `can_configure: true` 实测计数，由测试解析源核对）作为两个独立事实，不再合并为一个 `adapter_count`。
- **每个 Agent 记录** CF-1 冻结字段：agent_id/display_name/official_capability/discovery/configuration/authentication/change_detection/verification/rollback/platforms/provenance；六个 aspect 各自包含 official_source/version/config_path/field_ownership/write_policy/auth_owner/watcher/failure_recovery_acceptance/ui_status。WritePolicy 仅 `always_read_only|user_selected_managed|unsupported`；capability tier 仅 `route|mcp_hook|detection_only`。

### envelope↔matrix fail-closed 绑定合同（cross-artifact，非纯 JSON Schema 能力）

JSON Schema 无法跨文件绑定矩阵，故消费规则冻结在**三处**并被测试强制：

1. **schema description**：声明 agent_id 在矩阵中时非 null `capability_tier` 必须等于该 agent `official_capability`；未登记 agent_id 不得声明 `capability_tier=route`；消费者 PR-7/PR-9/C8C/N5/N9 遇不匹配必须 blocked（fail-closed）。
2. **矩阵 `envelope_binding_rule`**（新增字段）：同上规则 + `enforced_by_consumers: ["PR-7","PR-9","C8C","N5","N9"]`。
3. **cross-artifact 契约测试**（`CrossArtifactContractTests`，明确标注非纯 schema 校验）：`test_codex_route_binding_passes`（codex+route 通过）、`test_hermes_route_binding_rejected`（hermes+route 被拒，矩阵 official_capability=detection_only）、`test_unknown_route_binding_rejected`（未登记 agent+route 被拒）；helper `envelope_matrix_binding_errors()` 供消费者执行并 fail-closed。

## 3. 发布合同与当前代码的差异清单（合同要求 5，供后续切片证明）

冻结发布合同 release tier（11 verified Route + 12 detection-only）与当前 `agent_transport.rs`/`agent_mcp.rs` 的 descriptor 存在差异。矩阵以**冻结发布合同为 release tier**（official_capability），同时登记 `declared_integration_kind` 与 `current_code_caveat` 如实记录当前代码；**代码路径存在一律不冒充 production verified**。

| agent_id | release tier（冻结） | 当前代码声明 | 差异说明（current_code_caveat 摘要） |
|---|---|---|---|
| claude-code / codex / cline / goose / kilo-code / opencode / openclaw / gemini-cli / roo-code / continue / aider | route（11） | route（ADAPTERS Route，can_configure=true） | 无差异：与批准 11 verified Route 集一致 |
| hermes | detection_only | **route**（ADAPTERS Hermes L262-270，can_configure=true） | **代码差异**：Route adapter 存在但不在批准 11 集；route 证据待后续切片证明 |
| openhands | detection_only | **route**（ADAPTERS OpenHands L298-306，can_configure=true） | **代码差异**：同上（V0 config.toml 写面声明） |
| pi | detection_only | **route**（ADAPTERS Pi L327-335，can_configure=true） | **代码差异**：同上（models.json/settings.json 声明） |
| mistral-vibe | detection_only | **route**（ADAPTERS MistralVibe L367-375，can_configure=true） | **代码差异**：同上（config.toml 声明） |
| cursor / commandcode / claude-desktop / copilot-cli / vscode-copilot-chat / windsurf | detection_only | mcp（MCP descriptor，can_configure=false） | MCP descriptor 存在 ≠ verified write surface；KNOWN_MCP_CONFIGS 已登记路径 |
| antigravity | detection_only | mcp（can_configure=false，无已核验 MCP 路径/schema，paths 冲突） | 无核验写面 |
| cortex-code | detection_only | hook（CortexCodeHook L336-346，can_configure=false） | Hook descriptor；需 desktop-resolved bridge 路径，不猜路径 |

后续切片（PR-10 codex-toml、PR-11 openclaw、PR-2/N2 等）以本矩阵为唯一事实源按 Agent 关闭 route 证据；hermes/openhands/pi/mistral-vibe 若要在 release tier 升为 route，必须由独立切片提供 route 闭环证据（detector→parser→executor→readback→routed evidence→restore），不得引用本矩阵的 declared_integration_kind 作为证明。

## 4. 边界规则落实（合同要求 6）

- **VS Code Copilot Chat discovery 必须 extension evidence**：矩阵 caveat 明示共享 settings.json 不足；extension_prefix `github.copilot-chat-` + `extension_evidence` 校验（agent.rs L850-884 / 测试 L1079-1125）为必需；无 extension evidence 不判定已检测。
- **Cortex connections.toml 仅 metadata fingerprint**：不读内容（field_ownership 明示）；配置面 hooks.json 需 desktop-resolved bridge 路径，adapter 不猜路径。
- **词表**：普通 UI 状态只用冻结 10 词（`frozen_ui_status_words_10`，精确 10 项）；资产清单状态只用 4 词（已安装/未安装/已配置/未配置）；内部码（detected/configured/connected/verified/healthy/active/supported/route/mcp_hook/detection_only/adapter/credential_reference/needs-source-fix/source_conflict/事务状态码/WritePolicy 码等）单独登记为 `technical_only_internal_codes`，只进技术详情/Doctor/日志。

## 5. 契约测试（56 passed）

`python3 -m pytest support/tests/test_contract_freeze_registry.py -q` → 56 passed, 0 failed, 0.21s。

覆盖（合同要求 7 逐项）：
- schema 自身有效（Draft 2020-12 meta-schema `check_schema`）；strict additionalProperties:false；全部基础字段 required；
- 完整 generic / route / optical fixture 正向通过；
- 负向：缺 privacy_assertions 失败；缺 required 基础字段失败；null 无 reason 失败；非 null 带矛盾 reason 失败；route digest null 失败；optical digest null 失败；secret/raw/absolute-path privacy false/true 组合失败；未知 status/tier 失败；顶层/platform 多余字段失败；
- **reason_codes fail-closed**：`status != pass` 空 reason_codes 失败、带 reason 通过；`status=pass` 空列表通过（设计示例）；reason_codes/not_applicable_reasons 纯空白失败、含非空白字符通过；
- **四来源真实锚定**：Rust 源解析 23 ids 与矩阵全等；cc-switch committed fixture 只读 SQLite 查询 app_types 与矩阵精确相等；target spec §2 术语表 Agent 行转换 ids 与矩阵精确相等；UNIFIED §5 产品链路行转换 ids 与矩阵精确相等；ID 唯一；cc-switch aliases 全量精确；三层/write policy 合法；每个 aspect 字段完整；检测层 configuration/rollback = unsupported、route 层 = user_selected_managed；release tier 计数与批准 11+12 一致；批准 11 Route 集精确；`approved_verified_adapter_count=11` 与 `code_declared_configurable_adapters=15`（解析 agent_transport.rs 核对）两事实分离；Hermes `HERMES_CONFIG (L246)` 引用防漂移；状态词精确（10 词 + 4 词 + 内部码分离）；OMP/cc-switch always_read_only 且不在 agents[]；VS Code extension evidence 边界；Cortex metadata fingerprint 边界；hermes/openhands/pi/mistral-vibe route caveat；
- **cross-artifact 契约**（明确非纯 JSON Schema 校验）：envelope↔matrix binding（codex+route 通过、hermes+route 被拒、unknown+route 被拒，helper `envelope_matrix_binding_errors`）；矩阵 `envelope_binding_rule` 字段与消费方 PR-7/PR-9/C8C/N5/N9 冻结断言；RFC3339 时间顺序（结束早于开始被拒、合法时序通过，helper `timestamp_order_errors`；schema 描述声明该规则由 aggregator/consumer 强制）。

## 6. BDD 对照（PLAN CF-3）

| BDD | 实现 | 测试 |
|---|---|---|
| Given 缺 `privacy_assertions` 的 envelope；When 校验；Then 失败（blocked） | schema required privacy_assertions + 六项 const:true | `test_missing_privacy_assertions_fails`、`test_privacy_false_combinations_fail` |
| Given 四来源并集清单；When 生成矩阵；Then 每个出现 Agent 均在矩阵且三层分类无缺漏 | source_contracts 明示四来源 + agents[] 23 项 + release_tier_summary | `test_four_source_union_complete`、`test_matrix_covers_all_23_rust_registry_agents`、`test_release_tier_counts_match_approved_design`、`test_cc_switch_app_types_exact_from_fixture`、`test_target_spec_terms_exact`、`test_unified_product_chain_exact` |
| Given envelope agent_id 与矩阵能力不一致（如 hermes+route）；When 消费者校验；Then blocked | schema description + 矩阵 envelope_binding_rule + helper | `test_hermes_route_binding_rejected`、`test_unknown_route_binding_rejected`、`test_codex_route_binding_passes` |
| Given 结束时间早于开始时间；When aggregator 解析 RFC3339；Then blocked | schema/报告声明由下游强制（非 schema 跨字段） | `test_timestamp_finished_before_started_rejected`、`test_timestamp_legal_order_passes` |

## 7. 自证与边界

- 只写 4 个允许文件/目录（`support/schemas/` 新建目录 + `support/fixtures/agent-capability-matrix.json` + `support/tests/test_contract_freeze_registry.py` + 本报告）；`git status --short` 仅显示上述路径。
- 无 secret/真实路径/raw payload 写入：矩阵 config_path 全部为 home-relative 模板路径（`~/.xxx`、`%APPDATA%/...`）；cc-switch fixture 引用仅为文件路径与 pinned sha256（9df5d00a...，来自 generator 常量）；**测试只查询 app_type，不读取 settings_config/secret marker 内容，不执行 generator**；无完整 prompt/response/source code/raw command output。
- provenance 引用仓库路径+行号/合同章节；外部官方事实（各 Agent 官方能力/版本）未固定，全部写「待后续 PROV 核验（fail-closed）」。
- 自证运行：`python3 -m pytest support/tests/test_contract_freeze_registry.py -q`（56 passed）；`python3 -m json.tool` 两个 JSON 通过；`git diff --check` 无空白错误；`git status --short` 仅 4 个允许路径。
- 未执行 git 生命周期；未修改 `generate_release_gate_status.py`；未装依赖；未联网。

## 8. Risks / Blockers

- 无 BLOCKER。风险记录：hermes/openhands/pi/mistral-vibe 当前代码声明 Route adapter 但 release tier 保持 detection_only，属**有意冻结差异**，需后续切片按 §3 清单证明后才能升级；`evidence_kind` 为 schema 相对设计原样新增的必填判别字段，变更需新契约冻结切片。
- 合同性质声明：envelope↔matrix 绑定与 RFC3339 时间顺序属 **cross-artifact 合同**——JSON Schema 单独无法跨文件绑定矩阵或比较两个时间字段，冻结消费规则（schema 描述 + 矩阵 `envelope_binding_rule` + 本报告）依赖 PR-7/PR-9/C8C/N5/N9 等消费者执行 helper 并 fail-closed，本切片未伪称纯 JSON Schema 能独立完成这两项验证。
