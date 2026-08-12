# d-token v0.0.1 发布统一实施计划（唯一可执行计划）

- 唯一设计输入：`.ai-bridge/release-iteration/UNIFIED_RELEASE_DESIGN.md`（批准设计固化；本计划不重开其中任何冻结决策）
- 唯一执行索引：`.ai-bridge/release-iteration/TASKS.md`（切片状态/DAG/波次/共享文件串行/覆盖矩阵）
- 本文件约束：只创建/修改 `.ai-bridge/release-iteration/PLAN.md` 与 `TASKS.md`；不改业务代码、正式 docs、发布包、证据或其他任何文件；不运行项目命令（formatter/lint/tests/build/git 全部跳过）
- 执行者分工：主代理规划/派遣/审查/合并/验收/结论判定；实现 Agent 只改文件、运行范围内验证、写切片报告，不执行 git lifecycle（不 commit/push/branch/merge）
- 当前工作树很脏：所有现有改动视为用户/其他 Agent 工作，任何切片不得覆盖、回退或重构他人未完成工作；切片在隔离 worktree 内作业，主代理合并前逐 PR 审查
- 已废弃口径清单（本计划全篇不得出现）：旧权威顺序（版本包排 PRD 之前）、10% Token 基线、整批逆序回滚/二次确认弹窗、禁止实现 Agent 自证、Detection-only 必须 routed-flow、Optical 永久 preview、unsigned prerelease 无条件视为完成、未签名技术预览 `release_ready=false` 结论、`needs-source-fix`/`等待安全适配`/`来源冲突` 作为最终交付状态、onboarding 6 步、review-gated `automatic_apply_allowed=false`

---

## 1. 权威顺序（冲突裁决，冻结）

发生冲突时按以下顺序裁决（UNIFIED_RELEASE_DESIGN §1 原文口径）：

```text
本轮对话明确要求 / 用户逐节批准的发布设计（本文件与 UNIFIED_RELEASE_DESIGN.md 固化的内容，含 2026-08-12 前端边界补充）
→ docs/d-token_AI_product_target_spec_v2.md（目标规格 v2.1，F-01/F-05、§8、§9）
→ 固定版本上游真实能力与行为（Headroom / lean-ctx / pxpipe / RTK / Codebase Memory 等固定版本外部事实）
→ docs/PRD.md（唯一产品真理源；P0.0.10 为 Agent/Provider/Route 配置交互最高优先级契约）
→ docs/PLAN.md（唯一 Living Plan）
→ docs/releases/v0.0.1/（v0.0.1 具体执行分解，不是新的 Living Plan）
→ docs/product/IMPLEMENTATION_STATUS.md（代码事实快照）
→ ARCHITECTURE / TECH_STACK / SECURITY_BOUNDARIES
→ 当前专题文档（optical/、testing/、research/）
→ history/ → archive/
```

- 执行期发现旧文档冲突：以批准设计为准，不逐条修补旧文档；统一重写收敛为 DOC-1 切片的明确工作项（§19 冲突清单）。
- 源码只决定「当前实现状态 current」，不裁决设计优先级；实现现状不得覆盖批准设计、目标规格或固定上游事实。
- MUST/SHOULD/MAY 语义（PRD P0.0.1）：MUST=发布阻断，任一未满足 `release_ready=false`；SHOULD=优先交付但可记录风险后延期；MAY=可选。
- 诚实状态语义：未被当前候选真实运行证据同时证明的能力只能标记 `preview/partial/unavailable/unknown/not_verified`；不得因类型/接口/静态页面/metadata-only 事件/passthrough/单元测试存在而标记完成。

## 2. 目标与范围

- 目标版本：**v0.0.1**（用户指定；当前 manifest/PRD/PLAN 口径 `0.2.0-beta.1` 是 V-1 前的过程事实，不构成范围降级；V-1 前不得生成最终候选，V-1 后新证据只绑定 v0.0.1 `C_final`）。
- 本轮完成语义（冻结）：同一 commit/source digest 双平台完整功能与安全证据全绿（N0–N10 全部绑定 `C_final`）的公开未签名 prerelease 即算本轮发布完成；产物明确标 `unsigned`/`unnotarized`，不得宣称已签名/公证/系统信誉；`distribution_ready_public` 保持 false，不构成完成阻断。
- 发布阻断（冻结）：22 项 MUST（§18 覆盖矩阵）任一未满足，或任一关键证据缺席/失败 → blocked；未满足不得称可发布，任何文案/README/PR 不得写「可正式发布」。
- 非目标（冻结，切片越界即返工）：
  1. 不新增工程化一级页面、不重排八页 IA、不重构 shell（前端边界见 §7）。
  2. 不换前端技术栈/设计系统（React 18 + TS + Vite + Tauri 2、纯色 shell、现有视觉与动画、Reduce Motion、Windows/macOS 共用 UI）。
  3. 不实现上游 chooser：Provider/上游/模型/协议/endpoint/cc-switch profile 选择器一律不做（任何普通/高级 UI 均禁止）；Agent 清单勾选是允许的。
  4. 不引入外部 Headroom 8787 运行依赖；cc-switch 永远只读（含 OMP 只读边界）；外部仓库不得作为隐式运行依赖或直接复制进生产。
  5. 不提供 Optical 手动 active 控件、不扩大视觉压缩为普通图片压缩；exact 全门绿由 planner 自动 active，任一能力门不绿则同请求 PreserveText。
  6. 不把 Windows 实机/签名凭据、旧 OpenCode G7 证据、旧 DMG、浏览器 preview 当作最终候选证据；OpenCode 旧 bounded replay 不替代 N3 长工程验收。
  7. 不复制整份 HybridContextAccounting 进 receipt；不把完整 prompt/response/密钥/绝对路径写入日志、配置、导出或提交。
  8. 不在 release build 注册 benchmark/smoke/test-only commands；开发验证命令受 debug gate 限制。
  9. 不围绕 PR6 云端 CI 失败修 CI；独立发布证据脚本/工作流缺口仍须补齐，但不得借 PR6 跳过独立发布证据。
  10. 不推倒重写：从当前实现迁移收敛（§20），不重写 Rust runtime、不重建八页 UI、不另起第二套 pipeline。
  11. 前端严禁大范围重构（§7 执行规则）。

## 3. 关键冻结事实与迁移基线（切片不得误报缺失）

以下为批准设计确认的当前事实（来自 verified-facts-handoff、IMPLEMENTATION_STATUS、OMP 交叉评审，作为「不得误报缺失」的输入）：

- Rust 检测注册表覆盖 **23 个 Agent**，每个检测项有显式 transport capability decision；**11 个已验证 adapter**（Claude Code、Codex、Cline CLI、Goose、Kilo Code、OpenCode、OpenClaw、Gemini CLI、Roo Code、Continue、Aider）；其余 12 个保持 detection-only/fail-closed。
- 前端静态 6 项 Agent catalog 已删除；Install 只消费 Rust 发现结果；每行统一显示「配置」「清除」，未验证操作只返回无写入状态。
- 目标 Agent 矩阵（冻结）= 本轮要求 ∪ 目标规格 v2.1 ∪ Rust 正式注册表 ∪ cc-switch 实际 app type 的并集；单一来源出现的 Agent 也必须入矩阵并按官方能力层（route | mcp_hook | detection_only）闭环；N5 以 cc-switch 全量 app type 并集核对矩阵覆盖（不仅 openclaw）。
- 新增/边界 Agent 处理（冻结）：VS Code Copilot Chat 必须由扩展目录证明安装（不能仅凭共享 settings.json 误报），网络代理调试设置在无 HTTP CONNECT/forward-proxy 时不得冒充模型上游配置；Cortex `connections.toml` 只参与文件 metadata fingerprint，严禁读取内容。
- 当前真实 routed 汇总：89 条 routed request、20 条可见 request record、物理减少 279,270 Token（约 **4.0%**）；**5%** 为批准设计冻结基线（不重开）；物理节省与 claimable 分离，`attribution_basis=none` 时物理节省不冒充可声明收益。
- OMP 交叉评审结论（只读输入，不得误报缺失）：durable CCR 骨架已存在、自动 Agent lifecycle 已存在、8/8 页面真实 handlers 已存在、CBM manifest→graph 单版本（v0.9.1-rc.1）已工作。真实缺口（必须进计划）：receipt 未绑定请求、Native Responses 绕过 canonical、SendCommitReceipt 未生产接线、CBM 五正式版兼容缺失。
- 九类 codec 本地 probe 均 active，每类 ≥128 个 property/semantic/rollback cases；CCR 跨 JSONL/密文/Event/SQLite 原子 transaction 与自动 tool-call retrieve 仍是 G3 缺口。
- 过期证据隔离清单（不得绑定 `C_final`）：2026-08-04 G7 OpenCode real-routed、旧 DMG（c124d9f/60f7d063 ad-hoc 审计）、任何浏览器 preview 截图、任何非 `C_final` commit 的测试报告。
- 性能契约（P0.2.1）：在线 summary 刷新 p95 ≤ 250ms、离线 fail-fast p95 ≤ 500ms、无 >100ms JS long task、空闲 CPU ≤1%、30 分钟内存漂移 <5%、初始 JS gzip ≤160KB、长日志列表只渲染可见行。
- 版本事实：当前 manifest/PRD/PLAN 口径为 `0.2.0-beta.1`，V-1 统一前为过程事实（§19 C1）。

## 4. 唯一执行 DAG（冻结）

```text
P0（preflight baseline freeze）
→ BASE-1（baseline-absorption：dirty 36 项按文件 ownership 分流审查/提交或明确排除，之后才冻结实现基线）
→ [契约冻结波] CF-1（dtoken-types 合同）→ CF-2（dtoken-core-api 合同面）→ CF-3（证据信封 schema + Agent 四来源并集矩阵 + 词表）
→ 功能切片波（并行，按 §8 串行表；链内串行：config-patcher PR-13→PR-10→PR-11、前端 PR-13→PR-3→PR-14）：PR-1、PR-2、PR-3、PR-4、PR-6、PR-10、PR-11、PR-13、PR-14、PR-CBM3 + PROV（只读研究）
→ 第二批：PR-5、PR-7、PR-CBM4、PR-8（按依赖与串行槽）
→ C0A（schemas；dtoken-types 串行槽，PR-1 之后）→ C0B（sealed receipts）∥ C0C（registry/corpus）→ C0D（117-start）
→ C1A → C1B → C1C（single dispatch，含 Native Responses=PR-12）
→ C2 ∥ C3 ∥（C4U → C4A → C4B → C4C）∥ C5A
→ C5B ∥（C6A → C6B → C6C）
→ C7A → C7B → C7C → C5C → C5D
→ C8A（117 proof）→ C8B（real routes/platform）
→ V-1（版本统一 v0.0.1）→（DOC-1 文档统一 ∥ PR-9 verification orchestration）→ C8C（gate wiring）
→ C_final（冻结候选 commit）
→ N0（候选全量质量门）→ N1..N8 + N10（并行）→ N9（final gate/status 聚合）
→ Merge main + 发布报告
```

- 唯一 DAG：功能切片 → C8B → V-1 → DOC-1/PR-9 → C8C → C_final → N0 → N1..N8+N10 → N9；无其他平行收口路径；N10（U0–U9）是独立发布阻断（任一子门缺 → N10=blocked）。
- V-1 前不得生成最终候选；V-1 后新证据只能绑定 v0.0.1 `C_final`。
- 契约冻结波先行（本计划对设计 §14.2 波次表的唯一调整）：共享 Rust 合同/类型/状态机/证据信封/矩阵并集在功能切片之前冻结，功能切片只消费冻结合同；设计 §14.2 的 W1 并行组（PR-1/2/3/4/6/10/11/13/14）在本计划顺延至契约冻结波之后（W2），不改变切片内容、依赖与共享文件串行原则。

## 5. 波次表（W0–W12）

| Wave | 并行内容 | 出口门 |
|---|---|---|
| W0 | P0 preflight → BASE-1 baseline-absorption（dirty 36 项按文件 ownership 分流：归切片→该切片审查、共享文件→主代理审查、明确排除→记录；审查/提交或明确排除后）→ 冻结实现基线 | 主代理确认冻结 commit；git-operations.json 复核；实现基线含 BASE-1 决策清单 |
| W1 | 契约冻结波：CF-1 → CF-2 → CF-3（严格串行，共享合同文件一次一个切片） | 合同类型/状态机/信封/矩阵 schema 冻结 commit；无 owner/ID 漂移 |
| W2 | 功能切片并行：PR-1、PR-2、PR-3、PR-4、PR-6、PR-10、PR-11、PR-13、PR-14、PR-CBM3 + PROV（代码只读研究）；链内串行：config-patcher PR-13→PR-10→PR-11、前端 PR-13→PR-3→PR-14，其余可并行 | 各自自证门 + 切片报告落盘；共享 NOTICE 由主代理合并期串行处理 |
| W3 | PR-5（PR-1 后，dtoken-types/core-api 串行槽）、PR-7（PR-2/3/6 后）、PR-CBM4（PR-CBM3 后）；PR-8 在 PR-7 合并后启动 | 各自自证门 + 主代理审查门通过 |
| W4 | C0A（schemas；dtoken-types 串行槽，PR-1 之后） | schema review；无 owner/ID 漂移 |
| W5 | C0B 与 C0C（C0A 后可并行）+ C0D（C0C 后） | sealed receipt compile-fail 契约；117 registry/corpus 校验 |
| W6 | C1A → C1B → C1C（含 PR-12 Native Responses；core-api 串行槽） | single production pipeline 与 surface parity |
| W7 | 四轨道并行：C2（codecs+CacheAligner）∥ C3（Context Bus/CCR/九边界）∥（C4U 先行 → C4A → C4B → C4C）∥ C5A（optical 内核）；每轨道内部串行 | 每 stage production dispatch + 直接测试 |
| W8 | C5B（optical 门+shadow）∥（C6A → C6B → C6C） | exclusive actions、durability/fault、唯一 fallback 终态、SendCommitReceipt 生产消费 |
| W9 | C7A → C7B → C7C，之后 C5C → C5D | baseline 后才冻结 SLO；paired eval 后才 active |
| W10 | C8A → C8B | 117/117 verified production proof；real route/platform evidence producer 就绪 |
| W11 | V-1 →（DOC-1 ∥ PR-9）→ C8C | v0.0.1 单版本 + 文档统一；完整 verification DAG/gate wiring |
| W12 | C_final 冻结 → N0，然后 N1–N8/N10 并行，最后 N9 | 同一 C_final/source_digest；任一缺失 blocked |

## 6. 契约冻结波（W1，严格串行）

契约冻结波先行：共享 Rust 合同、schema、状态机、证据信封、Agent 矩阵并集、WritePolicy、ConfigTransactionState 在功能切片之前一次性冻结；功能切片（PR-1..14、C 系列）只消费冻结合同，不扩大合同面。共享文件（`dtoken-types`/`dtoken-core-api`/`support/fixtures`/`support/schemas`）在波内严格串行（一次一个切片，主代理合并后再开下一切片）。

### CF-1 contract-freeze-types（共享 Rust 合同冻结）

- 目标：在 `dtoken-types` 冻结以下合同类型（设计 §6.1/§6.2/§6.3/§8.1/§13 原文口径）：
  - 配置合同：`AgentConfigSnapshot`、`UpstreamSignature`（provider/model/protocol/normalized_endpoint/profile_or_app_scope；禁止 token/cookie/Authorization/完整 auth 配置）、`AgentConfigPlan`（operation(configure|clear)/target_files/redacted_diff/before_sha256/after_sha256/backup_or_rollback_id/runtime_reload_requirement）、`AgentConfigReceipt`（planned|applied|verified|rolled_back|failed、per-agent result、source revision、verification evidence、next action）。
  - Receipt 类型：`OptimizationReceipt`（PRD P0.0.7/§7.2 最小字段，Local/Provider/claimable 分离，`protected_content` 为最小字段之一）、`ValidationReceipt`（artifact-derived）、`RecoveryReceipt`（request/source/decision-bound）、`SendCommitReceipt`（transport-observable）、`RelayHitReceipt`/`CacheReuseReceipt`（Relay/cache 不计 compression savings）。
  - 状态机：`ConfigTransactionState`（`discovered → selected → planned → backed_up → applied → readback_verified → capability_verified → completed | rolled_back | failed`，逐 Agent 独立推进，不跨 Agent 共享原子事务）。
  - 写策略：`WritePolicy`（`AlwaysReadOnly`（OMP、cc-switch）/ `UserSelectedManaged`（一次点击授权事务化写入）/ `Unsupported`（Detection-only，fail-closed 不写））。
  - Agent 能力：`AgentCapability` 字段（agent_id / display_name / official_capability / discovery / configuration / authentication / change_detection / verification / rollback / platforms / provenance）。
  - 逐 Agent 事务 evidence 十字段字段：`transaction_id / source_revision / before_digest / redacted_diff_digest / backup_manifest / planned_after_digest / readback_digest / capability_evidence / receipt_id / rollback_receipt_id`。
  - canonical 类型骨架：Context IR node、NodeAction 枚举（Preserve|Reuse|HeadroomSemantic|RtkCommandOutput|Optical）——仅类型骨架，接线语义在 C0A/C1 系列。
- 允许文件：`code/crates/dtoken-types/src/lib.rs`、`code/crates/dtoken-types` 直接测试文件。
- 禁止文件：除上述外一切文件（尤其不得改 core-api、compress、desktop）。
- 依赖：P0 冻结基线。
- 分支/worktree/PR：`codex/final/contract-freeze-types` / `/Users/cc/Documents/d-token-worktrees/final-contract-freeze-types` / PR-CF1。
- 共享接口（输出）：上述类型即全部后续切片的共享接口；字段以本切片冻结版为准，任何后续切片不得自行增删字段（需要变更走 §10 变更协议）。
- 步骤：1) 读取 P0 基线上 dtoken-types 现状，识别已存在类型与缺口；2) 按设计 §6.1/§6.2/§6.3/§8.1 原文逐类型定义/收敛字段；3) 为每个类型写直接单元测试（字段完整性、枚举封闭性、序列化 round-trip、敏感字段缺席断言）；4) 切片报告 `contract-freeze-types-report.md`（变更类型清单、与设计原文字段对照、测试结果）。
- BDD：
  - Given 冻结合同文档 §6.1/§6.2；When 编译 dtoken-types 并运行直接测试；Then 所有类型字段与设计原文一一对应、`UpstreamSignature` 与 receipt 类型中不存在 token/cookie/Authorization 字段、ConfigTransactionState 状态迁移封闭。
  - Given 实现 Agent 尝试在切片内添加未批准字段；When 提交自证门；Then 切片被主代理审查门拒绝（越界返工）。
- 验证命令：`cargo fmt --all -- --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test -p dtoken-types`
- 证据：切片报告 + 测试输出（本切片不产生 N 级 artifact）。
- 回滚：`git revert <commit>`（主代理执行，先 `python3 support/scripts/git_operation_audit.py record`）。
- 完成判据：类型冻结 commit 合并；串行槽内演进（PR-1 usage 字段、C0A IR 语义、C0B sealed 语义）按 §8 串行槽推进，无并行切片要求修改合同字段。

### CF-2 contract-freeze-core（core-api 合同面冻结）

- 目标：`dtoken-core-api` 冻结合同消费面：ConfigTransactionState 推进语义、逐 Agent 事务十字段落实、receipt 投影（RequestLog 补 Provider usage 字段 input/output，缺 usage 保持 unavailable）、状态词表映射（§3.6 冻结词表；内部码只进技术详情/Doctor/日志）。
- 允许文件：`code/crates/dtoken-core-api/src/lib.rs`、`code/crates/dtoken-core-api` 直接测试文件。
- 禁止文件：其余一切文件（dtoken-types 只读，不在此切片改）。
- 依赖：CF-1。
- 分支/worktree/PR：`codex/final/contract-freeze-core` / `final-contract-freeze-core` / PR-CF2。
- 共享接口（输出）：逐 Agent 事务推进 API、receipt 投影字段、状态词表映射函数。
- 步骤：1) 读取现状识别缺口；2) 按 CF-1 类型实现状态机推进与十字段；3) RequestLog Provider usage 字段投影（unavailable 语义）；4) 词表映射（内部码 → §3.6 词表，未知映射 fail-closed）；5) 直接测试（状态迁移、unavailable 保持、敏感字段缺席）；6) 切片报告。
- BDD：
  - Given 某 Agent 事务在 applied 后 readback 失败；When 推进状态机；Then 该 Agent 进入 `rolled_back` 且 `rollback_receipt_id` 存在，其他 Agent 事务不受影响（逐 Agent 独立，不整批回滚）。
  - Given RequestLog 无 Provider usage；When 投影 OptimizationReceipt；Then Provider 字段为 unavailable 而非 0。
- 验证命令：`cargo fmt --all -- --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test -p dtoken-core-api`
- 证据：切片报告 + 测试输出。
- 回滚：`git revert <commit>` + audit record。
- 完成判据：合同面冻结 commit 合并；PR-1/PR-3/PR-5 等消费方以冻结面为唯一事实源。

### CF-3 contract-freeze-registry（证据信封 + Agent 四来源并集矩阵 + 词表）

- 目标：
  - `CandidateEvidenceEnvelope` schema 冻结（§13 字段：schema_version/node_id/status/reason_codes/candidate_commit/source_digest/registry_digest/corpus_digests/started_at/finished_at/platform/producer_revision/artifact_sha256/agent_id/capability_tier/privacy_assertions/route_decision_digest/capability_digest；Optical/benchmark/paired eval 另有 renderer/profile/font/tokenizer/pricing/cache/eval digests；不适用字段显式 null+reason，不得省略后当作通过）。
  - AgentCapabilityMatrix 四来源并集：本轮要求 ∪ 目标规格 v2.1 ∪ Rust 正式注册表（23 Agent + 11 adapter）∪ cc-switch 实际 app type；单一来源出现的 Agent 也入矩阵；每项登记官方能力层（route | mcp_hook | detection_only）+ WritePolicy + AgentCapability 字段。
  - 状态词表（§3.6 冻结）与内部码映射登记。
- 允许文件：`support/fixtures/`（`candidate-evidence-envelope.schema.json`、`agent-capability-matrix.json`）、`support/schemas/`、`support/tests/`（对应契约测试）。
- 禁止文件：Rust/TS 业务代码、docs、workflows、`support/scripts/generate_release_gate_status.py`（PR-7 拥有）。
- 依赖：CF-1（AgentCapability 字段）、P0。
- 分支/worktree/PR：`codex/final/contract-freeze-registry` / `final-contract-freeze-registry` / PR-CF3。
- 共享接口（输出）：envelope schema 文件路径与版本、矩阵 JSON 结构（后续 PR-7 gate 聚合、PR-9 orchestrator、C8C gate wiring、N5 核对、N9 聚合消费）。
- 步骤：1) 四来源并集核对（含 cc-switch app type 全量，不只 openclaw；VS Code Copilot Chat/Cortex 边界规则落实）；2) 逐 Agent 登记 capability tier/write policy/官方能力证据来源/状态词表映射；3) envelope schema + 校验器（缺失必填字段 → 校验失败）；4) 契约测试（envelope 必填字段矩阵、不适用字段 null+reason、矩阵并集完整性）；5) 切片报告。
- BDD：
  - Given 一份缺 `privacy_assertions` 的 envelope fixture；When 运行 schema 校验器；Then 校验失败（blocked 语义）。
  - Given 四来源并集清单；When 生成矩阵；Then 每个出现过的 Agent 均在矩阵且三层分类无缺漏。
- 验证命令：`python3 -m pytest support/tests/<契约测试>.py -q`
- 证据：tracked schema/fixtures + 切片报告。
- 回滚：`git revert <commit>` + audit record。
- 完成判据：envelope schema 与矩阵冻结；PR-7/PR-9/C8C/N5/N9 以冻结文件为唯一事实源。

## 7. 前端边界执行规则（冻结，严禁大范围重构）

1. 不重写 shell：侧栏/Topbar/StatusBar/通知中心/操作对话框（OperationDialog/NotificationCenter）结构保持现有实现；只允许设计 §4.4 列出的小范围缺陷修复（点击命中/展开位置偏移、交互性能、reduced-motion 等）。
2. 不重排八页 IA：正式一级页面固定为 总体概览、安装与连接、代码图谱（实验性）、分析、视觉压缩（Beta）、Doctor、变更与回退、设置；`Advanced`/`LogsSecurity`/`Compatibility`/`SmartLearning` 等保持工作台子页面或内部组件；不为 Headroom/LeanCTX/CCR/Context Bus 另建用户级独立页面（P0.0.8）；Context/代码图谱入口按 P0.0.8 表落位。
3. 只允许小范围接线/修复/文案/状态/交互（设计 §4.4 范围）：顶部重复标题/留白、StatusBar 920 横滚（flex-nowrap+overflow-x-auto 风险）、reduced-motion chevron transform、代码图谱 experimental badge、Heatmap/Changes/Doctor 文案统一、Analytics 日志高度已压缩不得再大改；Onboarding 首次使用入口（右上角重复进入）只做 shell/topbar 连接点接线。
4. 架构收敛主要在 Rust：状态机、事务、合同、canonical pipeline、read model、capability 判定、receipt 聚合全部收敛在 Rust crate 与 Tauri native command；React 只消费 Rust 合同与 receipt，不自行推断接入状态、不承载业务规则。
5. 前端只读真实数据：Dashboard/Doctor/Analytics/Logs/Export 真实数据只来自 d-token-owned runtime、Tauri command 或 Event Ledger；禁止 mock/fixture 伪成功；无完整 source/freshness/action-result 证据时显示 unavailable/partial/preview/unknown，禁止示例数字与绿色状态补齐。
6. UI 动作必须翻译成用户任务（安装、连接、配置、预览变更、应用、验证、诊断、修复、查看状态、查看日志、导出、回退）；错误只面向结果（§2.1.8）。
7. 前端文件分区（主代理审查门必查，越区即返工）：
   - PR-3 拥有：Install 安装状态消费者与连接库（`Install.tsx`、`lib/installMachine.ts`、`lib/install/presentation.tsx`、`lib/connectionSetup.tsx`、`lib/copyPolicy.ts`、`lib/i18n.ts` 相关键）+ `src-tauri/src/upstream_discovery.rs` + native 阶段回调合同（O2，最小 command 面）+ 目标规格 F-01/F-05 文案同步。
   - PR-4 拥有：shell/pages/nav/sidebar/样式小范围视觉/文案/交互修复（`components/shell/*` 排除 Topbar/AppShell、`components/pages/{Overview,Analytics,Settings,Doctor,Changes,VisualCompression,CodebaseMemory}.tsx` 仅限 §4.4 项、`styles/*.css`、`Layout.920x640.test.tsx` 等既有测试）；**不新增 i18n 键（只消费 PR-3 已加入的键）**；**禁止触碰 `Onboarding.tsx`/`Topbar.tsx`/`AppShell.tsx`（PR-14 拥有）与 `Install.tsx`/连接库（PR-3 拥有）**。
   - PR-14 拥有：`components/pages/Onboarding.tsx` + `Topbar`/`AppShell` 首次使用入口连接点 + onboarding 测试；**不新增 i18n 键（消费 PR-3 已加入的键）**。
8. 前端切片自证门：`npx tsc --noEmit` + 相关 Vitest（含 `Layout.920x640.test.tsx`、copyPolicy/languageIsolation 契约测试）；920×640 无横滚由 Playwright 真实 layout engine 证明，最终由 N6 真实 Tauri 复核；禁止仅凭 JSDOM/Vitest 声称无横滚。

## 8. 共享文件串行表（冻结，违反即返工）

| 文件/目录 | 串行槽顺序（主代理逐槽合并后开下一槽） | 禁止并发改写 |
|---|---|---|
| `code/crates/dtoken-types/src/lib.rs` | CF-1 → PR-1 → C0A → C0B →（C 系列链内自然串行） | 任何并行切片不得同时改动 |
| `code/crates/dtoken-core-api/src/lib.rs` | CF-2 → PR-1 → PR-5 → C1C（含删除 `inject_memory_into_body`）→ C3（memory_blocks 消费/搬移）→ C6A/C6C →（C 系列链内自然串行） | 同上 |
| `code/crates/dtoken-compress/src/` | C2（真实平铺路径：content_router.rs、compressor_registry.rs、code_codec.rs、diff_codec.rs、text_codec.rs、hybrid.rs、context_pipeline.rs、lib.rs + 新建 cache_aligner 模块）→ C3（仅 lib.rs + context_pipeline.rs 搬移 reuse 策略；context_pipeline.rs 与 C2 串行：C2 先行）；C5A 只动 `optical/` 目录；跨分区接口以 CF-1/C0A 冻结类型为准 | 同文件并发改写禁止；C5C/C5D 在 C7 链后 |
| `code/crates/dtoken-config-patcher/src/` | PR-13（cc_switch.rs）→ PR-10（agent_transport.rs Codex 模块）→ PR-11（agent_transport.rs OpenClaw 模块）严格串行；lib.rs/shared tests 由主代理合并期串行承接 | 并行切片不得同时改动 |
| `support/scripts/generate_release_gate_status.py`、`support/tests/test_release_gate_status_aggregation.py` | 仅 PR-7 | 其他切片只读消费 |
| `.github/workflows/*` | 仅 PR-7 接线（证据/发布 workflow）；其他切片禁止 | — |
| desktop 前端 | 链内串行：PR-13 → PR-3（安装状态+连接库+upstream_discovery.rs+阶段回调，拥有 i18n 键）→ PR-14（Onboarding+Topbar/AppShell 接线，消费 PR-3 键）；PR-4（shell/pages/nav/sidebar/样式小修）只消费 PR-3 i18n 键且明确排除 Topbar/AppShell/Onboarding/Install；三区严格分区 | 越区即返工 |
| `code/apps/desktop/src-tauri/src/main.rs` | PR-3（O2 阶段回调最小面）→ PR-5（SecretClassifier+export 修复）→ 按波次串行（后续切片如需 native 面，进入下一串行槽） | 并行切片不得同时改动 |
| `support/git-operations.json`、`docs/product/IMPLEMENTATION_STATUS.md`、`docs/PLAN.md` | 主代理合并期统一维护 | 实现 Agent 不写 |
| `support/artifacts/` | gitignored 运行时证据输出，只由 producer 写 | — |

通用规则（设计 §14.3 原文）：每个切片 = 独立分支 + 隔离 worktree + 独立 PR，从 P0 冻结基线或上游切片 commit 切出；禁止直接在 `main` 或他人 worktree 改动；每切片只改「允许文件」列路径；`<切片名>-report.md`（`.ai-bridge/release-iteration/`）为所有切片的隐式允许文件，除此之外不得借 report 规则改其他 `.ai-bridge` 文件。

## 9. 主代理 Git 生命周期与审计（冻结）

- Git lifecycle 由主代理集中执行；实现 Agent 只改文件、运行范围内验证并写切片报告，**不执行任何 git 操作（不 commit/push/branch/merge/rebase）**。
- 每个 commit/branch create/switch/delete/rename/merge/rebase/push 前必须 `python3 support/scripts/git_operation_audit.py record ...`（AGENTS.md §9），append-only `support/git-operations.json`。
- 合并顺序 = §4 DAG 顺序；每次合并 `--no-ff`，冲突手工解决并保留双方语义（以冻结合同签名为准同步调用点，不引入新行为）；每个 merge/commit 前跑 audit record。
- 主代理审查门（每 PR 必查，任一项不合格即返工，不合并）：
  1. diff 是否越出允许文件清单（含前端三区、§8 串行表）。
  2. 是否有隐式依赖未声明。
  3. 是否泄漏 secret/绝对路径/raw prompt/response；cc-switch 是否只读；SecretClassifier 是否统一复用（禁止并行弱分类器）。
  4. 是否把 null/unavailable 变成 0 或绿色成功；Local/Provider Token 是否分开；claimable 是否有计费证据。
  5. 测试是否覆盖原失败路径、正常路径、边界、错误与回退；未运行验证不得声称通过。
  6. 是否引入 chooser / 静默 fallback；是否复制整份敏感 accounting。
  7. 是否把代码路径存在写成功能完成；是否伪造或复用过期证据。
  8. 是否改写既有 G0–G8 业务语义（能力再证明而非改语义）。

## 10. 变更协议（Plan Mutation Protocol，冻结）

- 实现 Agent **不得自行改写切片边界、依赖顺序、结论判定，不得自我扩张范围**；发现必须变更时，停止并回报主代理。
- 默认变更路径：主代理在 `.ai-bridge/release-iteration/` 追加 `<date>-mutation.md` 并更新 TASKS.md 对应切片状态；实现 Agent 按变更后的切片继续。
- 仅当用户明确要求直接修订设计本体时，主代理可直接更新 `UNIFIED_RELEASE_DESIGN.md` 并在「审核修订记录」记录。
- 任一 PR 返工超 2 轮仍失败：停止切片，主代理重新评估方案；不得靠扩大容错、弱化断言、`#[ignore]`/skip 掩盖。
- 冻结决策（§1–§3、§7 全部）不重开；前端边界不放松。

## 11. 自证门与验证命令约定（冻结）

- 每切片交付前必须运行自证门并附输出；不得跳过，不得为本切片失败新增/扩大 `#[ignore]`/skip/弱化断言。
- Rust 切片：`cargo fmt --all -- --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test -p <切片 crate>`
- TS 切片：`npx tsc --noEmit`（`code/apps/desktop`）+ 相关 Vitest
- support 切片：对应 `python3 -m pytest support/tests/<file>.py -q` + producer dry-run/fixture 模式
- 证据节点：按 §13 各节点指定 producer 脚本，产物必须含 CandidateEvidenceEnvelope 并绑定 `C_final`/source_digest。
- N0 候选全量质量门：structure 检查 + `cargo fmt --all -- --check` + `cargo clippy --workspace --all-targets -- -D warnings` + `cargo test --workspace` + 前端 `npx tsc --noEmit` + Vitest + build + E2E；失败 blocked，修复产生新 commit 后在新候选完整重跑 N0。
- 真实运行要求：任何声称「已连接/已路由/已验证/已运行」的证据必须来自真实执行（runtime/route decision/Event Ledger/export/真实 Agent flow 或真实 Tauri 窗口）；浏览器 preview 只证明 UI；单元测试/静态类型存在不构成功能完成。

## 12. 切片定义

切片编号与 §4 DAG 一一对应。每个切片 = 独立分支 + 隔离 worktree + 独立 PR；`<切片名>-report.md` 为隐式允许文件；所有切片遵守 §8–§11。

### 12.1 P0 preflight-baseline（W0）

- 目标：执行基线冻结。复核当前分支与 dirty 状态（现有改动视为用户/其他 Agent 工作，不覆盖），由 P0 preflight 运行时 `git rev-parse HEAD` 采集并冻结执行基线 commit（不硬编码任何历史 HEAD）；复核 `support/git-operations.json` 边界完整。
- 允许文件：`.ai-bridge/release-iteration/preflight-baseline-report.md`（只读复核结果落盘）；不新增/修改其他任何文件。
- 禁止文件：一切业务代码、docs、workflow、fixture。
- 依赖：—。
- 分支/worktree/PR：`codex/final/preflight-baseline` / `final-preflight-baseline` / PR-P0。
- 共享接口（输出）：冻结基线 commit 与 dirty 清单，全部后续切片从其切出。
- 步骤：1) 只读核对当前 HEAD/分支/worktree/dirty 状态；2) 核对 git-operations.json 无破损边界；3) 落盘冻结报告；4) 主代理 audit record 后冻结 commit。
- BDD：Given 当前工作树有他人未提交改动；When P0 冻结；Then 冻结报告记录 dirty 清单且 P0 不触碰这些文件。
- 验证命令：无（只读复核 + 报告）。
- 证据：`preflight-baseline-report.md`。
- 回滚：不适用（无代码改动）。
- 完成判据：主代理确认冻结 commit；实现基线在 BASE-1（12.1a）分流决策后冻结，后续切片均从该实现基线切出。

### 12.1a BASE-1 baseline-absorption（W0，P0 后）

- 目标：当前工作树 dirty 状态（约 36 项，视为 pre-P0 用户/代理 work，**不得删除**）按文件 ownership 分流后审查/提交或明确排除，之后才冻结实现基线。历史现存 worktrees 是 pre-design/superseded，**不自动复用**；同名分支禁止直接重建（先 audit 决策）。当前 dirty 文件归切片允许文件清单的 → 对应切片作业时审查该 diff；共享文件（`support/git-operations.json`、`docs/product/IMPLEMENTATION_STATUS.md`、`docs/PLAN.md` 等）→ 主代理审查；明确排除 → 记录原因。
- 允许文件：`.ai-bridge/release-iteration/baseline-absorption-report.md`（分流决策清单落盘）；不直接修改业务代码（提交归属由主代理按分流决策执行）。
- 禁止文件：一切业务代码、docs、workflow、fixture 的直接改动（本切片只产出决策报告；文件级改动由对应切片或主代理执行）。
- 依赖：P0。
- 分支/worktree/PR：`codex/final/baseline-absorption` / `final-baseline-absorption` / PR-BASE1（主代理直接审查执行，可无独立 worktree）。
- 共享接口（输出）：实现基线 = P0 commit + BASE-1 分流决策（哪些 dirty 进入基线、哪些归切片审查、哪些排除）；决策清单作为后续所有切片切出基线。
- 步骤：1) P0 冻结后收集 dirty 清单（git status 只读核对）；2) 逐项按 ownership 分流；3) 落盘决策报告；4) 主代理审查后冻结实现基线（audit record）。
- BDD：Given 36 项 dirty 工作树；When BASE-1 分流；Then 每项有归属（切片审查/主代理审查/明确排除）且无任何一项被静默删除或覆盖。
- 验证命令：无（只读核对 + 决策报告）。
- 证据：`baseline-absorption-report.md`。
- 回滚：不适用（无代码改动）。
- 完成判据：实现基线冻结（含分流决策清单）；后续切片从该基线切出。

### 12.2 PR-1 runtime-receipt（W2）

- 目标：`mark_upstream_outcome` 写 Provider actual usage → RequestLog → OptimizationReceipt 投影；Provider usage 单独不提升 claimable；逐请求 cost 字段（费用/差额/币种/单位/价格来源/更新时间）收口（冲突 C10）；N1 receipt 证据 producer 脚本。
- 允许文件：`code/crates/dtoken-types/src/lib.rs`（串行槽：CF-1 → PR-1 → C0A → C0B，PR-1 在 CF-1 后，usage 字段扩展归本切片）、`code/crates/dtoken-core-api/src/lib.rs`（串行槽：PR-1 在 CF-2 后）、对应直接测试、`support/scripts/run_receipt_evidence.py` + `support/tests/test_receipt_evidence.py`（N1 producer，dry-run 模式）。
- 禁止文件：desktop 前端、光学、RTK、其他 support 脚本、docs。
- 依赖：CF-1/CF-2（合同冻结）；core-api 串行在 CF-2 后；与 PR-5 在 core-api 严格串行（PR-1 先行）。
- 分支/worktree/PR：`codex/final/runtime-receipt` / `final-runtime-receipt` / PR-1。
- 共享接口（消费/输出）：OptimizationReceipt 字段（CF-1 冻结）；RequestLog Provider usage 字段；N1 证据 envelope 模板（CF-3 冻结）。
- 步骤：1) 在冻结类型上实现 usage 采集与投影；2) 逐请求 cost 计算仅在 Token 与价格证据同时存在时输出，否则 unavailable；3) 缺 usage 保持 unavailable 不置 0；4) N1 producer 脚本（fixture 模式契约测试 + 真实收集模式）；5) 直接测试（原失败路径：无 usage、无价格、claimable 归因链断裂）；6) 切片报告。
- BDD：
  - Given 上游响应带 usage 与已存价格；When 生成 OptimizationReceipt；Then Local/Provider/claimable 三分离，费用/差额/币种/单位/价格来源/更新时间齐全。
  - Given 上游无 usage 或无价格；When 投影；Then 对应字段 unavailable/not verified，绝不显示 0，claimable=false。
- 验证命令：Rust 自证门（`cargo test -p dtoken-types -p dtoken-core-api`）+ `python3 -m pytest support/tests/test_receipt_evidence.py -q`
- 证据：切片报告 + 测试输出；N1 正式产物在 W12 由 producer 在 `C_final` 上生成（`support/artifacts/final-candidate/<C_final>/N1-receipt-*.json`，CandidateEvidenceEnvelope）。
- 回滚：`git revert <commit>` + audit record；无配置写入。
- 完成判据：receipt 投影与 cost 口径收口（C10 关闭）；N1 producer 就绪且契约测试绿。

### 12.3 PR-2 codex-ocxlocal-acceptance（W2）

- 目标：Codex 真实 routed-flow 验收 producer：`Codex → d-token:18001/v1 → ocxlocal:10100/v1 → 动态模型上游`；agent_id=codex、Responses 协议、无 OpenAI 认证语义、请求 model 透传（`ProviderWireApi::OpenAiResponses`、models=[] 允许动态透传），不得把模型固定为安装时值（C8 关闭）；结构化 artifact（route verified、请求匹配数、Ledger/metrics、失败分类、恢复状态）。
- 允许文件：`support/scripts/run_codex_ocxlocal_routed_acceptance.py`、`support/tests/test_codex_ocxlocal_acceptance.py`、`support/fixtures/codex-ocxlocal-routed/{report.schema.json,report.template.json}`。
- 禁止文件：Rust/TS 业务代码、其他 support 脚本、docs、workflow。
- 依赖：CF-3（envelope schema）。
- 分支/worktree/PR：`codex/final/codex-ocxlocal-acceptance` / `final-codex-ocxlocal-acceptance` / PR-2。
- 共享接口（输出）：N2 证据报告 schema（CandidateEvidenceEnvelope + agent_id=codex + route_decision_digest/capability_digest）。
- 步骤：1) 基于现有 runtime 端口事实实现 producer；2) fixture 模式契约测试（schema 完整、脱敏断言）；3) 真实收集模式（W12 N2 执行）；4) 切片报告。
- BDD：Given Codex 真实请求经 d-token 路由到 ocxlocal；When producer 收集；Then 报告含 agent_id=codex、provider/model/protocol、route verified、请求匹配数、Ledger/metrics 计数、失败分类、恢复状态，且无 secret/完整 prompt/绝对路径。
- 验证命令：`python3 -m pytest support/tests/test_codex_ocxlocal_acceptance.py -q` + producer `--dry-run`
- 证据：切片报告；N2 正式产物 `support/artifacts/final-candidate/<C_final>/N2-codex-routed-*.json`（W12 执行）。
- 回滚：`git revert <commit>` + audit record。
- 完成判据：producer + 契约测试绿；Codex 配置事务精确恢复不再被当作 routed-flow 替代（C8 语义落实）。

### 12.4 PR-3 autoconfig-cutover（W2）

- 目标：一次点击直接配置 + 逐 Agent 独立事务的完整交互链路（C2/C3 冲突关闭）：清除 `needs-choice`/`selection_required` 生产残留（presentation.tsx、installMachine.ts、connectionSetup.tsx 等全链路）；安装页完整 Agent 清单（已安装/未安装/已配置/未配置/配置失败，默认全选可取消）；无第二层确认弹窗；逐 Agent 独立结果 + 单项重试；错误只面向结果（§3.6 词表，无内部机制术语）；幂等重配（无变化只重新验证）；native 阶段回调合同（O2：Tauri command 提供 plan/apply/verify 逐阶段进度回调）；目标规格 F-01/F-05 文案同步。
- 允许文件：前端 Install 区（§7.7：`Install.tsx`、`lib/installMachine.ts`、`lib/install/presentation.tsx`、`lib/connectionSetup.tsx`、`lib/copyPolicy.ts`、`lib/i18n.ts` 相关键、对应测试）、`code/apps/desktop/src-tauri/src/upstream_discovery.rs` 及其测试、`code/apps/desktop/src-tauri/src/main.rs`（仅 O2 阶段回调最小 command 面，串行槽）、`docs/d-token_AI_product_target_spec_v2.md`（F-01/F-05 与 P0.0.10 对齐的文案同步）。
- 禁止文件：其他页面/shell/样式、dtoken-types/core-api（串行槽外）、其他 docs。
- 依赖：CF-1/CF-2（合同冻结）、CF-3（矩阵）、PR-13（前端链内串行：PR-13 → PR-3 → PR-14）；PR-7 依赖本切片（cc-switch 只读暴露）。
- 分支/worktree/PR：`codex/final/autoconfig-cutover` / `final-autoconfig-cutover` / PR-3。
- 共享接口（消费/输出）：AgentCapabilityMatrix（CF-3）、ConfigTransactionState/WritePolicy（CF-1/2）；阶段回调合同（O2）。
- 步骤：1) 清除 needs-choice/selection_required 残留；2) 清单/勾选/一次点击配置/独立结果 UI 按冻结词表接线；3) 幂等重配；4) O2 阶段回调合同（native 最小面）；5) 目标规格 F-01/F-05 同步；6) 前端与 Rust 契约测试（copyPolicy/languageIsolation/installMachine/Install）；7) 切片报告。
- BDD：
  - Given 用户勾选 3 个 Agent 并点击一次「配置」；When 2 个成功 1 个失败；Then 成功项保持「配置成功」，失败项显示「配置失败」+ 用户可执行说明 + 单独「重试」，无第二层确认弹窗。
  - Given 已配置 Agent 再次参与配置且无变化；When 执行；Then 只重新验证不重复写入（change_required=false 时前后 SHA-256 一致）。
  - Given 页面存在内部机制术语（adapter/route/credential reference/needs-source-fix）；When 渲染；Then 不出现（词表映射 fail-closed）。
- 验证命令：`npx tsc --noEmit && npx vitest run src/app/lib/install src/app/components/pages/Install* src/app/lib/copyPolicy.test.ts src/app/lib/languageIsolation.test.ts`（+ Rust 自证门 `cargo test -p dtoken-config-patcher` 如涉及）
- 证据：切片报告 + 测试输出；A1/A3/A7/A8/A10/A11 验收由 N4/N6 执行。
- 回滚：`git revert <commit>` + audit record；配置写入经 ConfigPatcher exact restore（不在此切片实现，PR-6 验收）。
- 完成判据：C2/C3/O2 关闭；安装页交互符合 P0.0.10.1–.5/.7–.9 词表与行为。

### 12.5 PR-4 ui-polish（W2）

- 目标：设计 §4.4 小范围修复：顶部重复标题/留白、StatusBar 920 横滚（flex-nowrap+overflow-x-auto 风险）、reduced-motion chevron transform、代码图谱 experimental badge、Heatmap/Changes/Doctor 文案统一（§3.6 词表/用户语言）；Analytics 日志高度已压缩不得再大改。
- 允许文件：§7.7 PR-4 区（shell/pages/nav/sidebar/样式与既有测试；glob 明确排除 `Topbar.tsx`/`AppShell.tsx`/`Onboarding.tsx`/`Install.tsx` 及连接库；**不新增 i18n 键**，只消费 PR-3 已加入的键）。
- 禁止文件：Onboarding/Topbar/Install 区、Rust 业务代码、docs。
- 依赖：CF-3（词表，如需映射）。
- 分支/worktree/PR：`codex/final/ui-polish` / `final-ui-polish` / PR-4。
- 共享接口（消费）：§3.6 词表映射函数（CF-2）。
- 步骤：1) 逐项执行 §4.4 修复清单；2) 文案统一按词表/用户语言；3) 既有测试更新 + Layout.920x640 测试；4) 切片报告。
- BDD：Given 920×640 窗口渲染 StatusBar；When Playwright 真实 layout engine 测量；Then `documentElement.scrollWidth <= clientWidth`（无横滚）。
  - Given Reduce Motion 开启；When chevron 展开；Then 无 transform 动画。
- 验证命令：`npx tsc --noEmit && npx vitest run src/app/components/shell src/app/components/pages src/app/lib/copyPolicy.test.ts`（+ Playwright 窄窗用例）
- 证据：切片报告 + 测试输出；920×640 最终由 N6 真实 Tauri 复核。
- 回滚：`git revert <commit>` + audit record。
- 完成判据：§4.4 清单全项落地；无越区 diff；Analytics 日志高度零 diff。

### 12.6 PR-5 security-classifier-export（W3）

- 目标：全部 production active-config/merged-config/route-draft secret validation path 复用共享 `SecretClassifier`（bearer/JWT/Google/GitHub/PEM/cookie/obfuscation 全覆盖），不允许并行弱分类器；runtime JSON/CSV export 不泄漏绝对 config path；回归测试。
- 允许文件：`code/apps/desktop/src-tauri/src/main.rs`（串行槽）、`code/crates/dtoken-compress/src/optical/secret.rs`、`code/crates/dtoken-core-api/src/lib.rs`（串行槽，PR-1 之后）、`code/apps/desktop/src-tauri/Cargo.toml`、`code/Cargo.lock`、对应直接测试。
- 禁止文件：前端页面、其他 crate 业务逻辑、docs。
- 依赖：PR-1（dtoken-types/core-api 串行槽）。
- 分支/worktree/PR：`codex/final/security-classifier-export` / `final-security-classifier-export` / PR-5。
- 共享接口（输出）：SecretClassifier 统一复用面；export 安全投影（config_source/path 投影规则）。
- 步骤：1) 复用 SecretClassifier 替换并行弱分类器路径；2) export 路径泄漏修复（含绝对路径投影）；3) 回归测试（原失败路径：各 secret 形态、绝对路径泄漏、CSV/JSON 两格式）；4) 切片报告。
- BDD：Given 含绝对 config path 的 merged-config；When 导出 JSON/CSV；Then 输出不含绝对路径（安全投影）。
  - Given 弱分类器路径存在；When 审查；Then 全部替换为共享 SecretClassifier，无并行弱分类器残留。
- 验证命令：`cargo test -p dtoken-core-api -p dtoken-compress` + `cargo test -p d-token-desktop`（main.rs 内联测试）
- 证据：切片报告 + 测试输出。
- 回滚：`git revert <commit>` + audit record。
- 完成判据：SecretClassifier 统一复用（设计 §15.1.9）；export 无绝对路径泄漏（MUST 19 部分）。

### 12.7 PR-6 config-acceptance（W2）

- 目标：隔离临时根配置验收 harness：configure → reread → clear → exact restore；原文件仅 hash/权限证据且不输出内容；失败仅回退该 Agent 本次未完成写入（A12）。
- 允许文件：`support/scripts/run_agent_config_acceptance.py`、`support/tests/test_agent_config_acceptance.py`、必要 fixture（`support/fixtures/agent-config-acceptance/`）。
- 禁止文件：Rust/TS 业务代码、其他脚本、docs。
- 依赖：CF-1/CF-2（事务合同）、CF-3。
- 分支/worktree/PR：`codex/final/config-acceptance` / `final-config-acceptance` / PR-6。
- 共享接口（输出）：N4 配置验收证据（envelope）。
- 步骤：1) 隔离临时根 harness（复制 Agent 配置到临时根，不碰真实配置）；2) configure→reread→clear→exact restore 断言（双 hash、备份、receipt、复读验证、失败自动回退）；3) 多 Agent 独立失败注入测试；4) 切片报告。
- BDD：Given 隔离临时根中的 Agent 配置；When configure→reread→clear；Then 原始字节/hash/权限精确恢复，过程中不输出文件内容。
  - Given 某 Agent 写入失败；When 批量配置；Then 仅该 Agent 回滚本次未完成写入，其他成功项保持。
- 验证命令：`python3 -m pytest support/tests/test_agent_config_acceptance.py -q`
- 证据：切片报告 + 测试输出；N4 正式产物 W12 生成。
- 回滚：`git revert <commit>` + audit record。
- 完成判据：A12 验收前置就绪；逐 Agent 独立事务失败语义经测试证明。

### 12.8 PR-7 cc-switch-aba-evidence（W3）

- 目标：cc-switch A→B→A artifact harness（只读 collect + verify；A→B→A 后 route identity 相同、`wrote_cc_switch=false`、ledger events）；release gate 状态聚合接线（fail-closed `generate_release_gate_status.py`，聚合任一关键证据缺席/失败 → blocked）；prerelease fan-in 随发布 workflow（同一 tag/同一 commit 双平台 reusable job 同时通过才创建/更新公开未签名 prerelease；产物标 unsigned/unnotarized）。
- 允许文件：`support/scripts/run_cc_switch_aba_check.py`、`support/scripts/generate_release_gate_status.py`、`support/tests/test_cc_switch_aba_check.py`、`support/tests/test_release_gate_status_aggregation.py`、`.github/workflows/{public-unsigned-prerelease.yml,macos-release.yml,windows-release.yml}`（仅证据接线）、`support/fixtures/cc-switch-aba/`。
- 禁止文件：Rust/TS 业务代码、其他 workflow、docs。
- 依赖：PR-2/PR-3/PR-6（gate 语义）、CF-3（envelope/矩阵）。
- 分支/worktree/PR：`codex/final/cc-switch-aba-evidence` / `final-cc-switch-aba-evidence` / PR-7。
- 共享接口（输出）：gate 聚合 API（后续 C8C 消费）；N5 harness（A→B→A collect/verify）；fan-in 证据。
- 步骤：1) A→B→A collect+verify harness（只读 DB/WAL/SHM，750ms debounce/单飞/5–15s polling 语义在 harness 断言）；2) gate 聚合 fail-closed；3) workflow fan-in 接线（同 commit 双平台 reusable job）；4) 契约测试（真实 DB schema 兼容、只读/fail-closed 语义）；5) 切片报告。
- BDD：Given cc-switch 配置 A→B→A；When harness collect+verify；Then route identity 相同、`wrote_cc_switch=false`、ledger events 记录、无凭据输出。
  - Given 任一关键证据缺席；When 运行 gate 聚合；Then 结论 blocked，任何文案不得写「可正式发布」。
- 验证命令：`python3 -m pytest support/tests/test_cc_switch_aba_check.py support/tests/test_release_gate_status_aggregation.py -q` + producer `--dry-run`
- 证据：切片报告；N5 正式产物 W12 由主代理 Computer 执行生成（`support/artifacts/final-candidate/<C_final>/N5-cc-switch-aba-*.json`）。
- 回滚：`git revert <commit>` + audit record；cc-switch 只读未写无需回滚。
- 完成判据：gate 聚合 + fan-in 接线就绪；N5 harness 契约测试绿。

### 12.9 PR-8 upgrade-autostart-credential（W3，PR-7 合并后）

- 目标：macOS 升级/回滚、autostart、凭据 round-trip 证据 producer（N7 前置）；managed RTK 候选测试 producer（N7 前置，与 C4 链交付后可用）。
- 允许文件：`support/scripts/run_macos_upgrade_rollback_audit.py`、`support/scripts/run_autostart_credential_roundtrip.py`、`support/scripts/run_managed_rtk_candidate_test_macos.sh`（仅证据接线）、`support/tests/test_upgrade_autostart_credential.py`、必要 fixture。
- 禁止文件：Rust/TS 业务代码、其他脚本、docs、workflow。
- 依赖：PR-7。
- 分支/worktree/PR：`codex/final/upgrade-autostart-credential` / `final-upgrade-autostart-credential` / PR-8。
- 共享接口（输出）：N7 证据 producer（envelope）。
- 步骤：1) upgrade/rollback audit 脚本（升级→回滚→证据，绑定候选）；2) autostart/credential round-trip（凭据只进 Keychain，不落盘）；3) managed RTK 候选脚本；4) 契约测试（dry-run/fixture）；5) 切片报告。
- BDD：Given 安装旧版后升级到候选；When 运行 audit 脚本；Then 产物含 upgrade/rollback 前后状态、版本、hash，无凭据泄漏。
  - Given autostart 与凭据 round-trip；When 运行；Then 凭据仅存在于 OS 凭据库（不透明引用），无明文 fallback。
- 验证命令：`python3 -m pytest support/tests/test_upgrade_autostart_credential.py -q`
- 证据：切片报告；N7 正式产物 W12 生成（`support/artifacts/final-candidate/<C_final>/N7-*.json`）。
- 回滚：`git revert <commit>` + audit record。
- 完成判据：N7 producer 就绪且契约测试绿。

### 12.10 PR-10 codex-toml（W2）

- 目标：Codex TOML 配置形态（`codex-toml`）的 parse/plan/apply/readback/verify/rollback 支持（N2 两种配置形态之一）；multi-provider 适配不套用其他 Agent 字段（§7.1）。
- 允许文件：`code/crates/dtoken-config-patcher/src/agent_transport.rs`（Codex 专用现有模块）、对应直接测试（config-patcher 串行槽：PR-13 → PR-10 → PR-11；lib.rs/shared tests 由主代理合并期串行承接）。
- 禁止文件：其他 crate、前端、docs。
- 依赖：CF-1/CF-2（合同）、CF-3（矩阵登记）、PR-13（config-patcher 串行槽，PR-13 先行）。
- 分支/worktree/PR：`codex/final/codex-toml` / `final-codex-toml` / PR-10。
- 共享接口（消费/输出）：AgentConfigPlan/Receipt（CF-1）；Codex TOML 适配能力登记进矩阵。
- 步骤：1) Codex TOML 解析（现状核对）；2) 配置/清除事务（d-token-owned 字段、幂等、exact restore）；3) 复读验证；4) 测试（原失败路径：未知格式 fail-closed、foreign 字段保留）；5) 切片报告。
- BDD：Given Codex TOML 配置含用户自有字段；When configure；Then 只写 d-token-owned 字段，用户字段与凭据保留，clear 幂等且只删带 ownership 证据内容。
- 验证命令：`cargo test -p dtoken-config-patcher`
- 证据：切片报告 + 测试输出。
- 回滚：`git revert <commit>` + audit record。
- 完成判据：Codex TOML 形态闭环（Route 层六维）。

### 12.11 PR-11 openclaw-adapter（W2）

- 目标：OpenClaw 专用 multi-provider adapter（多供应商 Agent 使用专用 adapter，未知格式不得套用其他 Agent 字段）；按官方能力层闭环。
- 允许文件：`code/crates/dtoken-config-patcher/src/agent_transport.rs`（OpenClaw 专用现有模块）、对应直接测试（config-patcher 串行槽：PR-13 → PR-10 → PR-11；lib.rs/shared tests 由主代理合并期串行承接）。
- 禁止文件：其他 crate、前端、docs。
- 依赖：CF-1/CF-2、CF-3（矩阵登记）、PR-10（config-patcher 串行槽）。
- 分支/worktree/PR：`codex/final/openclaw-adapter` / `final-openclaw-adapter` / PR-11。
- 共享接口：AgentConfigPlan/Receipt；OpenClaw 能力登记。
- 步骤：1) 现状核对；2) multi-provider adapter 配置/清除事务；3) 复读验证与回退；4) 测试；5) 切片报告。
- BDD：Given OpenClaw 多 provider 配置；When configure；Then 仅操作 d-token-owned provider/字段，未知格式 fail-closed 不写。
- 验证命令：`cargo test -p dtoken-config-patcher`
- 证据：切片报告 + 测试输出。
- 回滚：`git revert <commit>` + audit record。
- 完成判据：OpenClaw 适配闭环（所属能力层完成定义）。

### 12.12 PR-13 hot-sync（W2）

- 目标：cc-switch 只读热同步完整循环（§8.2 冻结）：独立 watcher/coordinator（DB/WAL/SHM + 750ms debounce + 单飞 + 5–15s polling fallback）→ 重发现 → 与 last-known-good 比较 → 分类变化 → 自动更新 d-token-owned route（或首次授权 scope 内受管字段）→ 复读 → 能力复验 → receipt/ledger；review-gated 授权门（首次 hash-bound 授权后仅自动更新 d-token 自有 route/runtime，绝不写 cc-switch；授权/证据/receipt 持久化，未授权不得自动写）；原子 route snapshot（旧请求旧快照、新请求新快照，reload 失败保留 last-known-good）；pending 变更回滚到稳定快照会被清除，下一次变更须重新完成 debounce。
- 允许文件：`code/crates/dtoken-runtime-pipeline/`（热同步协调/原子快照/last-known-good）、`code/crates/dtoken-config-patcher/src/cc_switch.rs`（只读 watcher；config-patcher 串行链 PR-13 先行）及其直接测试。若确需 core-api 新命令面，在切片报告中声明，由主代理在 core-api 串行槽承接（本切片不直接改 core-api/types）。
- 禁止文件：`dtoken-types`/`dtoken-core-api`（串行槽外）、前端、docs、workflow。
- 依赖：CF-1/CF-2、CF-3。
- 分支/worktree/PR：`codex/final/hot-sync` / `final-hot-sync` / PR-13。
- 共享接口（输出）：热同步循环事件/授权门/receipt 语义（供 PR-3 UI 状态与 N5 验收消费）。
- 步骤：1) watcher/coordinator 实现；2) 授权门持久化；3) 原子 route snapshot；4) 分类变化处理与 last-known-good；5) 直接测试（debounce 折叠、单飞、锁定/损坏/未来 schema/缺失 → last-known-good、未授权不写、pending 清除语义）；6) 切片报告。
- BDD：
  - Given cc-switch 变化 A→B；When watcher 触发；Then 仅自动更新 d-token 自有 route/runtime 并在新快照验证后切换，旧请求继续用旧快照，reload 失败保留 last-known-good，绝不写 cc-switch。
  - Given 未授权（无首次 hash-bound 授权）；When 变化到达；Then 不自动写，等待用户决策。
- 验证命令：`cargo test -p dtoken-runtime-pipeline -p dtoken-config-patcher`
- 证据：切片报告 + 测试输出；A6 由 N5 验收。
- 回滚：`git revert <commit>` + audit record；cc-switch 只读无回滚需求。
- 完成判据：完整热同步循环冻结语义落地（MUST 6 部分）。

### 12.13 PR-14 onboarding-5seg（W2）

- 目标：Onboarding 五段统一（O1/O3/C4 关闭）：欢迎简介 → 有呼吸感动画和可视进度的服务启动 → 扫描已安装 Agent → 用户调整默认勾选并一次点击「配置」（内部 plan/apply/readback/verify、逐 Agent 独立结果）→ 开始使用；verify 不是独立用户段；右上角可重复进入。
- 允许文件：`code/apps/desktop/src/app/components/pages/Onboarding.tsx`、`Topbar`/`AppShell` 首次使用入口连接点、对应测试（**不新增 i18n 键**，消费 PR-3 已加入的键）。
- 禁止文件：Install 区、其他页面/shell 重构、Rust 业务代码、docs。
- 依赖：CF-3、PR-13 → PR-3（前端链内串行：PR-13 先行、PR-3 后、PR-14 最后）。
- 分支/worktree/PR：`codex/final/onboarding-5seg` / `final-onboarding-5seg` / PR-14。
- 共享接口（消费）：配置事务状态（CF-2）、Agent 清单（CF-3/PR-3）。
- 步骤：1) 五段流程实现（服务启动段含呼吸感动画与可视进度）；2) 右上角重复进入接线；3) 测试（五段顺序、状态词表、可重复进入）；4) 切片报告。
- BDD：Given 首次使用；When 完成欢迎→服务启动→扫描→一次配置→开始使用；Then 流程为五段且逐 Agent 显示「配置成功/配置失败」，无独立 verify 用户段。
- 验证命令：`npx tsc --noEmit && npx vitest run src/app/components/pages/Onboarding* src/app/components/shell`
- 证据：切片报告 + 测试输出；N6 真实 Tauri 验收五段。
- 回滚：`git revert <commit>` + audit record。
- 完成判据：O1/O3/C4 关闭；五段流程与 §3.7 一致。

### 12.14 PR-CBM3 cbm-manifest-v2（W2）

- 目标：多版本 manifest v2：五正式版（v0.9.0/v0.8.1/0.8.0/0.7.0/0.6.1）登记 + v0.9.1-rc.1 明确标 prerelease（不计五正式版，固定 tag/commit、四平台 archive/binary SHA 与 Sigstore/provenance，不得放宽校验）；v0.8.1 无 windows-arm64 → Windows 平台矩阵 `incompatible`；无官方资产版本按 source-tag 登记（0.8.0/0.7.0/0.6.1）。
- 允许文件：`code/crates/dtoken-codebase-memory/`（manifest v2 相关模块）、`support/fixtures/cbm/`（manifest fixtures）、对应直接测试。
- 禁止文件：其他 crate、前端、docs、workflow。
- 依赖：CF-3（envelope/矩阵登记）、PROV（pin 事实）。
- 分支/worktree/PR：`codex/final/cbm-manifest-v2` / `final-cbm-manifest-v2` / PR-CBM3。
- 共享接口（输出）：manifest v2 schema/语义（PR-CBM4、N4/N8 消费）。
- 步骤：1) 五正式版 + rc.1 登记（pin 事实来自 PROV 研究）；2) 校验语义（tag+commit+asset hash/provenance；无资产 → unavailable+原因；存在但校验失败 → blocked）；3) 平台矩阵（windows-arm64 incompatible）；4) 测试（rc.1 不计五版本、无资产 unavailable、校验失败 blocked）；5) 切片报告。
- BDD：
  - Given manifest 查询 v0.9.1-rc.1；When 校验；Then 标记 prerelease 且不计入五正式版兼容。
  - Given 0.8.0 无官方资产；When 导入校验；Then 如实 unavailable+原因，不声明兼容、不伪造。
- 验证命令：`cargo test -p dtoken-codebase-memory`
- 证据：切片报告 + fixtures；N4/N8 验收消费。
- 回滚：`git revert <commit>` + audit record。
- 完成判据：manifest v2 冻结（C7 部分关闭）。

### 12.15 PR-CBM4 cbm-adapter-multiversion（W3）

- 目标：五版本适配器：v0.9.0 与 v0.8.1 官方二进制真实验证（initialize/tools → index fixture repo → graph query/call chain/dependency/impact → exact rollback）；0.8.0/0.7.0/0.6.1 source-tag 登记 + fail-closed + 诚实 unavailable（不因此阻断本轮完成）；独立修复官方 v0.9.0 正确 release 目录仍无法配置的 Bug（系统化复现、先补失败测试、再 preview/apply/verify/rollback 与原始 hash/权限恢复）；d-token 不内置下载/更新 CBM（官方资源+安装/接入说明，用户自行下载，只本地导入校验，不写用户仓库）。
- 允许文件：`code/crates/dtoken-codebase-memory/`（适配器相关模块）、`code/apps/desktop/src-tauri/src/codebase_memory_control.rs`（如需，串行槽声明）、`support/scripts/run_cbm_version_acceptance.py`（N4 CBM 验收 producer）+ `support/tests/test_cbm_version_acceptance.py`（契约测试）、`support/fixtures/cbm/`、对应直接测试。
- 禁止文件：其他 crate、前端页面、docs、workflow。
- 依赖：PR-CBM3；与 PR-CBM3 串行（同 crate）。
- 分支/worktree/PR：`codex/final/cbm-adapter-multiversion` / `final-cbm-adapter-multiversion` / PR-CBM4。
- 共享接口（输出）：五版本适配能力面（代码图谱页消费；N4 验收）。
- 步骤：1) 先补 v0.9.0 Bug 失败测试（系统化复现）；2) 修复（preview/apply/verify/rollback + 原始 hash/权限恢复）；3) v0.9.0/v0.8.1 真实二进制适配；4) 三版无资产 fail-closed unavailable；5) 测试（逐版本、exact rollback、无资产语义）；6) 切片报告。
- BDD：Given 官方 v0.9.0 正确 release 目录；When 配置；Then 成功且 exact restore 可验证（先失败测试证明原 Bug）。
  - Given 0.6.1 无官方资产；When 用户尝试导入；Then fail-closed unavailable+原因，不写用户仓库。
- 验证命令：`cargo test -p dtoken-codebase-memory`（+ 相关桌面 crate 测试如涉及）
- 证据：切片报告 + 测试输出；N4 CBM 验收 W12 执行。
- 回滚：`git revert <commit>` + audit record；配置 exact restore。
- 完成判据：C7 关闭；五正式版兼容闭环（两版真实 + 三版诚实 unavailable）。

### 12.16 PROV provenance-research（W2，只读）

- 目标：只读研究：Headroom/lean-ctx/pxpipe/RTK/Codebase Memory 固定版本事实核对（版本/commit/SHA/provenance/官方能力），供 CF-3 矩阵、PR-CBM3 pin、DOC-1 NOTICE 对齐使用；不产生代码改动。
- 允许文件：只读；报告 `provenance-research-report.md`。
- 禁止文件：任何代码/文档修改。
- 依赖：P0。
- 分支/worktree/PR：无需分支（主代理直接调度只读研究）。
- 步骤：1) 固定版本外部事实核对；2) 报告（含 pin 事实与官方能力证据来源）；3) 共享 NOTICE 由主代理合并期串行处理（不在本切片改）。
- BDD：Given 固定版本上游清单；When 核对；Then 报告含版本/commit/SHA/provenance 且无代码修改。
- 验证命令：无（只读）。
- 证据：`provenance-research-report.md`。
- 回滚：不适用。
- 完成判据：pin 事实落盘，供 DOC-1/PR-CBM3 消费。

### 12.17 C0A schemas（W4）

- 目标：canonical Context IR / RouteDecision / NodeAction 语义 schema 冻结：一个请求只有一个 RouteDecision（route/provider/model/protocol/API revision/capability/deadline 唯一权威）、一个 canonical Context IR（parse-once、type-state lowering）、每 node 恰一个终态动作（Preserve|Reuse|HeadroomSemantic|RtkCommandOutput|Optical）；stage evidence 绑定同一 decision digest。
- 允许文件：`code/crates/dtoken-types/src/lib.rs`（串行槽）、`support/schemas/`（IR/decision schema）、对应直接测试。
- 禁止文件：其他 crate、前端、docs。
- 依赖：CF-1、PR-1（dtoken-types 串行槽：CF-1 → PR-1 → C0A → C0B）。
- 分支/worktree/PR：`codex/final/c0a-schemas` / `final-c0a-schemas` / PR-C0A。
- 共享接口（输出）：IR/RouteDecision/NodeAction 语义（C0B–C8C 全部消费）。
- 步骤：1) IR node/终态动作语义实现；2) RouteDecision 唯一权威 + decision digest；3) schema fixtures；4) 测试（每 node 恰一终态动作、parse-once、digest 绑定）；5) 切片报告。
- BDD：Given 一个请求；When lowering 为 IR；Then 每 node 恰一个终态动作且所有 stage evidence 绑定同一 decision digest。
- 验证命令：`cargo test -p dtoken-types`
- 证据：切片报告 + schema fixtures。
- 回滚：`git revert <commit>` + audit record。
- 完成判据：IR/decision schema 冻结（MUST 11 基础）。

### 12.18 C0B sealed-receipts（W5）

- 目标：sealed receipt 类型与密封逻辑：ReadyToSend 只能由 runtime pipeline 内部基于 artifact-derived ValidationReceipt、RecoveryReceipt 与最终 envelope 校验密封；调用方布尔值/任意 digest/时间戳哨兵不能铸造状态；sealed 语义（伪造路径无法编译/无法通过契约测试）。
- 允许文件：`code/crates/dtoken-types/src/lib.rs`（串行槽）、`code/crates/dtoken-runtime-pipeline/`（密封逻辑）、对应直接测试。
- 禁止文件：其他 crate、前端、docs。
- 依赖：C0A。
- 分支/worktree/PR：`codex/final/c0b-sealed-receipts` / `final-c0b-sealed-receipts` / PR-C0B。
- 共享接口（输出）：sealed ReadyToSend/ValidationReceipt/RecoveryReceipt 语义（C1C/C6C 消费）。
- 步骤：1) artifact-derived validation 密封；2) 伪造路径 compile-fail 契约测试；3) 直接测试（时间戳哨兵/任意 digest 拒绝）；4) 切片报告。
- BDD：Given 调用方传入任意 digest 与时间戳；When 尝试铸造 ReadyToSend；Then 编译失败或运行时拒绝（artifact-derived 校验不通过）。
- 验证命令：`cargo test -p dtoken-types -p dtoken-runtime-pipeline`
- 证据：切片报告 + 测试输出。
- 回滚：`git revert <commit>` + audit record。
- 完成判据：type-state 不可伪造（MUST 12 基础）。

### 12.19 C0C registry-corpus（W5）

- 目标：117 项 tracked registry 数据结构 + corpus fixtures（§13：tracked registry/corpus 位于 support/fixtures/schemas；运行 artifact 在 support/artifacts/）；registry 项定义（能力/证明/证据来源/状态语义），零 placeholder/missing/not-in-scope 终态由 C0D/C8A 保证。
- 允许文件：`support/fixtures/`（117-registry、corpus fixtures）、`support/schemas/`、对应契约测试。
- 禁止文件：Rust/TS 业务代码、docs。
- 依赖：C0A。
- 分支/worktree/PR：`codex/final/c0c-registry-corpus` / `final-c0c-registry-corpus` / PR-C0C。
- 共享接口（输出）：registry/corpus digest 事实源（C0D、C8A、N0/N9/N10 envelope 绑定）。
- 步骤：1) registry 结构 + corpus fixtures；2) digest 计算（registry_digest/corpus_digests）；3) 契约测试；4) 切片报告。
- BDD：Given registry fixtures；When 计算 digest；Then digest 稳定且与 envelope 绑定字段一致。
- 验证命令：`python3 -m pytest support/tests/<registry 契约测试>.py -q`
- 证据：tracked fixtures + 切片报告。
- 回滚：`git revert <commit>` + audit record。
- 完成判据：registry/corpus 事实源冻结（MUST 15 基础）。

### 12.20 C0D 117-start（W5，C0C 后）

- 目标：117 项 registry 初始覆盖：将现有 codec/能力/证据映射到 117 项（九类 codec 每类 ≥128 cases、CCR、RTK、Optical、LeanCTX 九边界、四维 accounting 等），每项标注当前证据状态（verified/partial/unavailable），后续 C2–C8 逐步补满至 117/117。
- 允许文件：`support/fixtures/`（117-registry 内容）、对应契约测试。
- 禁止文件：Rust/TS 业务代码、docs。
- 依赖：C0C。
- 分支/worktree/PR：`codex/final/c0d-117-start` / `final-c0d-117-start` / PR-C0D。
- 共享接口（输出）：117 项初始登记（C8A 终态校验输入）。
- 步骤：1) 逐项映射现有能力与证据；2) 标注状态（诚实语义）；3) 契约测试（项数=117、无重复 ID）；4) 切片报告。
- BDD：Given 初始映射；When 校验；Then registry 恰 117 项、ID 唯一、每项有证据状态且无 placeholder 冒充 verified。
- 验证命令：`python3 -m pytest support/tests/<registry 契约测试>.py -q`
- 证据：tracked registry + 切片报告。
- 回滚：`git revert <commit>` + audit record。
- 完成判据：117 项初始登记完成。

### 12.21 C1A dispatch-converge（W6）

- 目标：单一 canonical dispatch 收敛：HTTP/Rust/MCP/TS/Proxy 共用同一 resolved production pipeline（P0.0.7.2）；请求进入单一 canonical Context IR（parse-once、type-state lowering）。post-validation injection（`inject_memory_into_body`，定义/调用在 core-api）的删除与 Reuse/memory pre-plan 进入 IR 由 C1C 承担（本切片不做，见 12.23）。
- 允许文件：`code/crates/dtoken-router/`、`code/crates/dtoken-runtime-pipeline/`、对应直接测试。
- 禁止文件：core-api（串行槽外）、前端、docs。
- 依赖：C0A/C0B。
- 分支/worktree/PR：`codex/final/c1a-dispatch-converge` / `final-c1a-dispatch-converge` / PR-C1A。
- 共享接口（输出）：统一 dispatch 入口语义（C1B/C1C 消费）。
- 步骤：1) dispatch 收敛（parse-once、单一 IR 进入）；2) 测试（各入口进入同一 canonical IR、parse-once、无旁路）；3) 切片报告。
- BDD：Given 任一入口请求；When dispatch；Then 进入同一 canonical IR（parse-once、type-state），dispatch 收敛无旁路（reuse pre-plan 与 injection 删除由 C1C 完成）。
- 验证命令：`cargo test -p dtoken-router -p dtoken-runtime-pipeline`
- 证据：切片报告 + 测试输出。
- 回滚：`git revert <commit>` + audit record。
- 完成判据：dispatch 收敛基础（MUST 11 部分）。

### 12.22 C1B surface-parity（W6）

- 目标：公共 surface parity：HTTP、MCP、TS SDK、Proxy 对同一 fixture 的 decision/reason/artifact parity 字节一致；发送体=验证体（RUNTIME-4/CONTEXT-4）。
- 允许文件：`code/crates/dtoken-proxy/`、`code/crates/dtoken-core-api/src/lib.rs`（串行槽，如需）、`code/packages/sdk/`、对应测试。
- 禁止文件：前端页面、docs。
- 依赖：C1A。
- 分支/worktree/PR：`codex/final/c1b-surface-parity` / `final-c1b-surface-parity` / PR-C1B。
- 共享接口（输出）：parity 契约测试集。
- 步骤：1) 各 surface 接入统一 dispatch；2) 同一 fixture parity 断言（字节一致）；3) 发送体=验证体断言；4) 切片报告。
- BDD：Given 同一 fixture 请求；When HTTP/Rust/MCP/TS/Proxy 分别处理；Then decision/reason/artifact 字节一致且发送体=最终验证体。
- 验证命令：`cargo test -p dtoken-proxy -p dtoken-core-api` + `npm test`（code/packages/sdk）
- 证据：切片报告 + parity 测试输出。
- 回滚：`git revert <commit>` + audit record。
- 完成判据：surface parity 全绿（MUST 11 部分）。

### 12.23 C1C native-responses（W6，含旧 PR-12）

- 目标：**Native Responses（`/v1/responses`）纳入 single dispatch**（C5 冲突关闭；verified-facts-handoff 10）：禁止绕过 canonical（parse-once/type-state）发送路径，禁止 pending marker/no-op/type existence 冒充完成；生产 transport 必须实际消费 SendCommitReceipt；发送体=验证体；**删除 post-validation injection `inject_memory_into_body`（定义/调用在 core-api）并让 Reuse/memory 在 transform plan 前进入 IR**。
- 允许文件：`code/crates/dtoken-core-api/src/lib.rs`（串行槽，PR-5 之后）、`code/crates/dtoken-runtime-pipeline/`（transport 消费点）、对应直接测试。
- 禁止文件：前端、docs。
- 依赖：C1B、C0B（sealed receipts）；core-api 串行槽在 PR-5 之后。
- 分支/worktree/PR：`codex/final/c1c-native-responses` / `final-c1c-native-responses` / PR-C1C（旧 PR-12 responses 并入本切片，TASKS 记为 superseded→并入）。
- 共享接口（输出）：Native Responses single dispatch + SendCommitReceipt 消费面（C6C 完成 terminal 语义）。
- 步骤：1) Native Responses 路由改造为 canonical pipeline（保留 Responses 协议语义、model 透传、无 OpenAI 认证语义）；2) 生产 send 消费 SendCommitReceipt；3) 删除 `inject_memory_into_body`（core-api 定义/调用）并实现 Reuse/memory 在 transform plan 前进入 IR；4) 回归测试（绕过路径不存在、receipt 消费断言、post-validation injection 不存在、reuse pre-plan 顺序断言）；5) 切片报告。
- BDD：
  - Given 请求经 `/v1/responses`；When 处理；Then 走同一 canonical IR/type-state 路径且 SendCommitReceipt 被生产 transport 实际消费。
  - Given 旧绕过路径与 post-validation injection；When 审查与测试；Then 均不存在（无 pending marker/no-op 冒充；`inject_memory_into_body` 已删除，Reuse/memory 在 transform plan 前进入 IR）。
- 验证命令：`cargo test -p dtoken-core-api -p dtoken-runtime-pipeline`
- 证据：切片报告 + 测试输出；N10 U0/U1 最终证明。
- 回滚：`git revert <commit>` + audit record。
- 完成判据：C5 关闭；Native Responses 纳入 single dispatch（MUST 11/12 部分）。

### 12.24 C2 codecs-cachealigner（W7，轨道 A）

- 目标：九类 codec（JSON/log/search/build/diff/text/HTML/config/code-compressor）生产内核收敛（FrozenPrefix → TransformableWindow → ProtectedTail；Net-cost Gate、负收益原样透传；每类 ≥128 个 property/semantic/rollback cases 保持）；**CacheAligner 为 canonical pipeline 真实组件**：真实 cache alignment（对齐既有缓存命中面）+ 真实 receipt；节省只计入 provider-cache 分账（与 compression/command-output/relay 独立）；禁止 accounting/metadata/no-op 冒充（MUST 22）。
- 允许文件（真实平铺路径）：`code/crates/dtoken-compress/src/{content_router.rs, compressor_registry.rs, code_codec.rs, diff_codec.rs, text_codec.rs, hybrid.rs, context_pipeline.rs, lib.rs}` + 新建 `cache_aligner` 模块（创建/接线）及对应直接测试；`context_pipeline.rs` 与 C3 串行（C2 先行）。
- 禁止文件：`dtoken-compress/src/optical/`（C5A 区）、`dtoken-core-api`、其他 crate、前端、docs。
- 依赖：C0A/C1 链（接入 canonical IR）。
- 分支/worktree/PR：`codex/final/c2-codecs-cachealigner` / `final-c2-codecs-cachealigner` / PR-C2。
- 共享接口（输出）：CacheAligner receipt（provider-cache 分账）、codec 终态动作接线（HeadroomSemantic 等）。
- 步骤：1) 九类 codec 收敛核对（语义/结构校验、字节精确还原、负收益透传）；2) CacheAligner 真实 alignment + receipt；3) provider-cache 分账接线（独立 bucket）；4) 测试（≥128 cases 保持、cache alignment 命中面断言、receipt 真实性）；5) 切片报告。
- BDD：
  - Given 缓存中已有命中面；When 请求经 CacheAligner；Then 产生真实 cache alignment 与 receipt，节省仅计入 provider-cache 分账，无 metadata-only/no-op。
  - Given 负收益输入；When 压缩；Then 原样透传（不写入、不改字节）。
- 验证命令：`cargo test -p dtoken-compress`
- 证据：切片报告 + 测试输出；验收 C5 由 N10 U0/U1 + N3 证明。
- 回滚：`git revert <commit>` + audit record。
- 完成判据：MUST 22 关闭；九类 codec 生产内核收敛。

### 12.25 C3 context-bus-ccr（W7，轨道 B）

- 目标：LeanCTX 九边界（MUST 14）：Context Bus / Session Cache / Context Relay / Map-signature / Query-aware Retrieval / Recovery Pointer-Manifest / Stale-Superseded / SharedContext authorization / self-hit-size 九边界为 MUST；**搬移而非复制**：策略从 `dtoken-compress`/`dtoken-context-store` 搬移到新 `dtoken-context-bus`（避免双权威）；`dtoken-context-store` 只实现 persistence port；`dtoken-compress` 只消费 reuse decision；CCR 真实 durable store→receipt→marker→restore 闭环（AES-256-GCM、TTL/quota、ACL/DACL、request/source/decision binding、journal、崩溃恢复、精确 retrieve/delete；Unix 0700/0600、Windows protected DACL）；跨 JSONL/密文/Event/SQLite 原子 transaction 与自动 tool-call retrieve（G3 缺口关闭）。
- 允许文件（真实位置）：`code/crates/dtoken-context-bus/`（新 crate）、`code/crates/dtoken-context-store/`（persistence port 收窄）、`code/crates/dtoken-compress/src/lib.rs` 与 `code/crates/dtoken-compress/src/context_pipeline.rs`（仅消费/搬移 reuse 策略所需；context_pipeline.rs 与 C2 串行：C2 先行）、`code/crates/dtoken-core-api/src/lib.rs`（memory_blocks 消费/搬移所需；core-api 串行槽在 C1C 之后）、`code/crates/dtoken-runtime-pipeline/`（CCR 闭环，如需要）、对应直接测试、`code/Cargo.toml`（新 crate 登记；workspace 归属由主代理合并时统一处理）。
- 禁止文件：`dtoken-compress/src/optical/`（C5A 区）、除上述外的其他 core-api/compress 文件、前端、docs。
- 依赖：C1C（core-api 串行槽：C1C 之后）、C2（context_pipeline.rs 串行槽：C2 先行）。
- 分支/worktree/PR：`codex/final/c3-context-bus-ccr` / `final-c3-context-bus-ccr` / PR-C3。
- 共享接口（输出）：Context Bus 九边界语义、CCR receipt/marker/restore 语义（C6A/B 消费 durable 面）。
- 步骤：1) 策略搬移（非复制，删除旧双权威）；2) 九边界实现；3) CCR 闭环（durable store→receipt→marker→restore、journal、崩溃恢复、跨存储原子性、tool-call retrieve）；4) 测试（九边界逐项、CCR 全生命周期、崩溃点、ACL/DACL）；5) 切片报告。
- BDD：
  - Given 同一 context 同时被请求；When 经 Context Bus；Then 单权威路径生效（无双权威），共享授权/self-hit-size 语义正确。
  - Given CCR 写入后崩溃；When 恢复；Then journal 精确恢复（write_intent→pending_retained→terminal），pre-send fault 原文只发送一次，post-commit 无 retry。
- 验证命令：`cargo test -p dtoken-context-bus -p dtoken-context-store -p dtoken-compress -p dtoken-core-api -p dtoken-runtime-pipeline`
- 证据：切片报告 + 测试输出；验收 C3 由 N10 U3 证明。
- 回滚：`git revert <commit>` + audit record。
- 完成判据：MUST 14 关闭；G3 缺口关闭。

### 12.26 C4U rtk-unblock（W7，轨道 C 先行）

- 目标：RTK 三平台解封（C6 冲突关闭）：macOS/Windows/Linux Install/Configure/Verify/Rollback/Uninstall 全部 `ManagedRtkTransactionState::Blocked` 移除封锁（未配置=unavailable 而非 Blocked），为 C4A/B/C 生产实现开路。
- 允许文件：`code/crates/dtoken-preprocessor/`（RTK 状态面）、对应直接测试。
- 禁止文件：其他 crate、前端、docs。
- 依赖：C1 链（类型面）。
- 分支/worktree/PR：`codex/final/c4u-rtk-unblock` / `final-c4u-rtk-unblock` / PR-C4U。
- 共享接口（输出）：unblocked 状态语义（C4A/B/C 消费）。
- 步骤：1) 移除三平台 Blocked 封锁；2) 未配置=unavailable 语义；3) 测试（三平台状态迁移）；4) 切片报告。
- BDD：Given 未配置 RTK 且平台为 macOS/Windows/Linux；When 查询状态；Then unavailable（非 Blocked），不宣称已配置。
- 验证命令：`cargo test -p dtoken-preprocessor`
- 证据：切片报告 + 测试输出。
- 回滚：`git revert <commit>` + audit record。
- 完成判据：C6 关闭；RTK 解封。

### 12.27 C4A rtk-lifecycle（W7，轨道 C）

- 目标：RTK Install/Configure 生产实现（解封后）：未配置=unavailable、受控配置事务（plan/backup/apply/readback/verify/receipt/rollback），不动摇既有管理语义。
- 允许文件：`code/crates/dtoken-preprocessor/`、`code/crates/dtoken-agent-bridge/`（RTK 接线）、对应直接测试。
- 禁止文件：其他 crate、前端、docs。
- 依赖：C4U。
- 分支/worktree/PR：`codex/final/c4a-rtk-lifecycle` / `final-c4a-rtk-lifecycle` / PR-C4A。
- 步骤：1) Install/Configure 状态机生产实现；2) 事务证据；3) 测试；4) 切片报告。
- BDD：Given 用户配置 RTK；When 事务执行；Then plan/backup/apply/readback/verify/receipt 完整，失败精确回滚。
- 验证命令：`cargo test -p dtoken-preprocessor -p dtoken-agent-bridge`
- 证据：切片报告 + 测试输出。
- 回滚：`git revert <commit>` + audit record；配置 exact restore。
- 完成判据：RTK Configure 闭环。

### 12.28 C4B rtk-verify-rollback（W7，轨道 C）

- 目标：RTK Verify/Rollback/Uninstall 生产实现：managed lifecycle 全状态、exact restore（原始字节/hash/权限）、uninstall 幂等。
- 允许文件：`code/crates/dtoken-preprocessor/`、`code/crates/dtoken-agent-bridge/`、对应直接测试。
- 禁止文件：其他 crate、前端、docs。
- 依赖：C4A。
- 分支/worktree/PR：`codex/final/c4b-rtk-verify-rollback` / `final-c4b-rtk-verify-rollback` / PR-C4B。
- 步骤：1) Verify/Rollback/Uninstall 实现；2) exact restore 测试（hash/权限）；3) 切片报告。
- BDD：Given 已配置 RTK；When Rollback/Uninstall；Then 原始字节/hash/权限精确恢复，幂等。
- 验证命令：`cargo test -p dtoken-preprocessor -p dtoken-agent-bridge`
- 证据：切片报告 + 测试输出。
- 回滚：`git revert <commit>` + audit record。
- 完成判据：RTK 生命周期闭环。

### 12.29 C4C rtk-hook-accounting（W7，轨道 C）

- 目标：RTK hook/tee/filter/planner/accounting 生产接线：真实命令输出经 RTK（RtkCommandOutput 终态动作）、command-output 独立分账（四维 accounting 之一）、三平台语义（macOS/Windows；Windows 能力不足如实记录）。
- 允许文件：`code/crates/dtoken-preprocessor/`、`code/crates/dtoken-runtime-pipeline/`（RtkCommandOutput 接线）、`code/crates/dtoken-config-patcher/`、`code/crates/dtoken-core-api/src/lib.rs`（串行槽，如需）、`code/crates/dtoken-event-ledger/`、对应直接测试。
- 禁止文件：前端、docs。
- 依赖：C4B。
- 分支/worktree/PR：`codex/final/c4c-rtk-hook-accounting` / `final-c4c-rtk-hook-accounting` / PR-C4C。
- 步骤：1) hook/tee/filter 生产接线；2) RtkCommandOutput 终态动作；3) command-output 分账；4) 测试（四维独立、RTK 真实输出路径）；5) 切片报告。
- BDD：Given 真实命令经 RTK；When 处理；Then 产生 RtkCommandOutput 终态且节省只计入 command-output 分账，与 compression/provider-cache/relay 独立。
- 验证命令：`cargo test -p dtoken-preprocessor -p dtoken-runtime-pipeline -p dtoken-config-patcher -p dtoken-core-api -p dtoken-event-ledger`
- 证据：切片报告 + 测试输出；N7 managed RTK 验收消费。
- 回滚：`git revert <commit>` + audit record。
- 完成判据：MUST 15（RTK 部分）/MUST 13（command-output 分账）关闭。

### 12.30 C5A optical-core（W7，轨道 D）

- 目标：Optical 生产内核基础：renderer（pxpipe v0.12.0 固定版本行为/provenance 参考）、history、cache、attestation；不扩大为普通图片压缩；不做手动 active 控件。
- 允许文件：`code/crates/dtoken-compress/src/optical/*`（§8 分区）、`support/fixtures/optical/`、对应直接测试。
- 禁止文件：codecs/context_reuse 区、前端、docs。
- 依赖：C0A/C1 链。
- 分支/worktree/PR：`codex/final/c5a-optical-core` / `final-c5a-optical-core` / PR-C5A。
- 共享接口（输出）：Optical 能力门数据面（C5B 消费）、renderer/profile/font digest（envelope 字段）。
- 步骤：1) renderer/history/cache/attestation 生产实现基础；2) 能力门数据面；3) 测试；4) 切片报告。
- BDD：Given 图片输入；When renderer 处理；Then 产物与固定版本 pxpipe 行为一致（digest 可验证），无普通图片压缩扩张。
- 验证命令：`cargo test -p dtoken-compress`
- 证据：切片报告 + 测试输出。
- 回滚：`git revert <commit>` + audit record。
- 完成判据：Optical 内核基础就绪。

### 12.31 C5B optical-gates-shadow（W8）

- 目标：Optical exact 能力门 + 只读三态（冻结）：① 精确能力不可用 + 原因（能力门不绿/该请求 PreserveText/未启用）② 观察中（shadow，不计 actual）③ 已自动启用（exact 全门绿，展示当前证据）；重启一致；shadow/metadata-only 不得说成 verified。
- 允许文件：`code/crates/dtoken-compress/src/optical/*`、对应直接测试。
- 禁止文件：其他 crate 区、前端、docs。
- 依赖：C5A。
- 分支/worktree/PR：`codex/final/c5b-optical-gates-shadow` / `final-c5b-optical-gates-shadow` / PR-C5B。
- 步骤：1) 能力门判定（renderer/profile/font/tokenizer/pricing/cache/eval 全部门）；2) 三态模型（shadow 不计 actual）；3) 重启一致性；4) 测试；5) 切片报告。
- BDD：Given 任一能力门不绿；When 请求到达；Then 该请求 PreserveText（三态①或②），shadow 不计 actual。
  - Given 全部门绿；When 请求到达；Then 三态③（自动启用），展示当前证据。
- 验证命令：`cargo test -p dtoken-compress`
- 证据：切片报告 + 测试输出；N6 三态 UI 验收消费。
- 回滚：`git revert <commit>` + audit record。
- 完成判据：三态模型落地（MUST 15 Optical 部分）。

### 12.32 C6A scheduler-durability（W8）

- 目标：scheduler/durability：deadline/queue/稳定顺序；durable CCR write_intent journal 调度面；terminal 发送前的持久化状态。
- 允许文件：`code/crates/dtoken-core-api/src/scheduler.rs`（串行槽）、`code/crates/dtoken-runtime-pipeline/`、`code/crates/dtoken-context-store/`（durability）、对应直接测试。
- 禁止文件：前端、docs。
- 依赖：C3（CCR durable 面）、C1C。
- 分支/worktree/PR：`codex/final/c6a-scheduler-durability` / `final-c6a-scheduler-durability` / PR-C6A。
- 步骤：1) scheduler deadline/queue/稳定顺序；2) durable 调度面；3) 测试；4) 切片报告。
- BDD：Given 多请求进入队列；When 调度；Then 稳定顺序、deadline 语义正确，调度状态持久化可恢复。
- 验证命令：`cargo test -p dtoken-core-api -p dtoken-runtime-pipeline -p dtoken-context-store`
- 证据：切片报告 + 测试输出。
- 回滚：`git revert <commit>` + audit record。
- 完成判据：调度/持久化面就绪。

### 12.33 C6B fault-recovery（W8）

- 目标：fault graph/恢复状态机：故障注入与恢复（durability/fault graph、唯一 fallback 终态）；pre-send fault 原文只发送一次；post-commit 无 retry；过期/错 scope 拒绝；跨存储 crash points（Windows MSVC durability 证据）。
- 允许文件：`code/crates/dtoken-runtime-pipeline/`、`code/crates/dtoken-context-store/`、对应直接测试（fault 注入 fixtures）。
- 禁止文件：前端、docs。
- 依赖：C6A。
- 分支/worktree/PR：`codex/final/c6b-fault-recovery` / `final-c6b-fault-recovery` / PR-C6B。
- 步骤：1) fault graph 与恢复状态机；2) 唯一 fallback 终态；3) 故障注入测试（崩溃点、scope/过期拒绝、单次发送断言）；4) 切片报告。
- BDD：Given 发送前故障；When 恢复；Then 原文只发送一次（无自动重发），post-commit 无 retry，过期/错 scope 拒绝。
- 验证命令：`cargo test -p dtoken-runtime-pipeline -p dtoken-context-store`
- 证据：切片报告 + 测试输出；验收 C3/C4 由 N10 U3/U4 证明。
- 回滚：`git revert <commit>` + audit record。
- 完成判据：fault/恢复闭环（MUST 12 部分、C3/C4 验收支持）。

### 12.34 C6C terminal-send（W8）

- 目标：terminal send：生产 transport 实际消费 SendCommitReceipt（transport-observable）；ReadyToSend 密封后发送；发送体=验证体；发送后写 OptimizationReceipt 与 Event Ledger。
- 允许文件：`code/crates/dtoken-runtime-pipeline/`、`code/crates/dtoken-core-api/src/lib.rs`（串行槽）、`code/crates/dtoken-event-ledger/`、对应直接测试。
- 禁止文件：前端、docs。
- 依赖：C6B、C0B（sealed receipts）。
- 分支/worktree/PR：`codex/final/c6c-terminal-send` / `final-c6c-terminal-send` / PR-C6C。
- 步骤：1) 生产 transport 消费 SendCommitReceipt；2) 发送后 receipt/ledger 写入；3) 测试（消费断言、发送体=验证体）；4) 切片报告。
- BDD：Given 请求密封 ReadyToSend；When 发送；Then SendCommitReceipt 被生产 transport 消费，发送体=验证体，发送后 OptimizationReceipt/Event Ledger 落账。
- 验证命令：`cargo test -p dtoken-runtime-pipeline -p dtoken-core-api -p dtoken-event-ledger`
- 证据：切片报告 + 测试输出；MUST 12 最终由 N1/N10 U0 证明。
- 回滚：`git revert <commit>` + audit record。
- 完成判据：MUST 12 关闭（SendCommitReceipt 生产接线）。

### 12.35 C7A perf-baseline（W9）

- 目标：统一性能 baseline 冻结：真实 perf corpus（长工程 fixture）、baseline 指标（压缩率/延迟/吞吐/内存），供 SLO 冻结（C7C）与 5% 门槛（C1 验收）对比；baseline 后才冻结 SLO。
- 允许文件：`support/fixtures/perf-corpus/`、`support/scripts/run_perf_baseline.py`（含 dry-run）、`support/tests/test_perf_baseline.py`、`code/crates/dtoken-compress/`（bench 入口，仅 bench 代码）。
- 禁止文件：release build 注册（debug gate）、前端、docs。
- 依赖：C2/C3/C4C/C5B（stage 内核齐备）。
- 分支/worktree/PR：`codex/final/c7a-perf-baseline` / `final-c7a-perf-baseline` / PR-C7A。
- 步骤：1) perf corpus；2) baseline 采集脚本（真实运行）；3) baseline 冻结报告；4) 测试；5) 切片报告。
- BDD：Given 同一 corpus；When 两次运行 baseline；Then 指标稳定（漂移 < 阈值），冻结 baseline 可复现。
- 验证命令：`python3 -m pytest support/tests/test_perf_baseline.py -q` + `cargo test -p dtoken-compress`（bench 相关）
- 证据：baseline 冻结报告（support/artifacts/，gitignored）+ 切片报告。
- 回滚：`git revert <commit>` + audit record。
- 完成判据：baseline 冻结；SLO 未冻结前不得宣称性能达标。

### 12.36 C7B replay-quality（W9）

- 目标：replay/paired quality：真实请求 replay 质量（优化后语义/结构等价、字节精确还原）、paired 对比（same-model baseline/optimized）；relay/cache/RTK/Optical 四维分账在 paired 中保持独立。
- 允许文件：`support/scripts/run_replay_quality.py`、`support/tests/test_replay_quality.py`、`support/fixtures/replay/`、`code/crates/dtoken-compress/`（replay 校验入口）。
- 禁止文件：前端、docs、release build。
- 依赖：C7A。
- 分支/worktree/PR：`codex/final/c7b-replay-quality` / `final-c7b-replay-quality` / PR-C7B。
- 步骤：1) replay 质量 harness（真实请求回放）；2) paired 对比（同模型）；3) 四维分账断言；4) 测试；5) 切片报告。
- BDD：Given 真实请求回放；When 优化→恢复；Then 语义/结构等价且字节精确还原；paired 对比中四维分账独立不求和。
- 验证命令：`python3 -m pytest support/tests/test_replay_quality.py -q`
- 证据：切片报告 + replay 报告（gitignored artifacts）。
- 回滚：`git revert <commit>` + audit record。
- 完成判据：replay/paired 质量闭环（N10 U4 支持）。

### 12.37 C7C slo-freeze（W9）

- 目标：SLO 冻结与性能门：在线 summary 刷新 p95 ≤ 250ms、离线 fail-fast p95 ≤ 500ms、无 >100ms JS long task、空闲 CPU ≤1%、30 分钟内存漂移 <5%、初始 JS gzip ≤160KB、长日志列表只渲染可见行；5% 物理节省基线门槛归入 U5。
- 允许文件：`support/scripts/run_slo_check.py`、`support/tests/test_slo_check.py`、`support/fixtures/perf-corpus/`（如需扩展）、`code/crates/dtoken-compress/`（bench 入口）。
- 禁止文件：前端、docs、release build。
- 依赖：C7A（baseline 后才冻结 SLO）。
- 分支/worktree/PR：`codex/final/c7c-slo-freeze` / `final-c7c-slo-freeze` / PR-C7C。
- 步骤：1) SLO 阈值登记（基于 C7A baseline）；2) 检查 harness；3) 测试；4) 切片报告。
- BDD：Given baseline 已冻结；When 运行 SLO 检查；Then 各项阈值判定通过/失败，失败即 blocked 输入。
- 验证命令：`python3 -m pytest support/tests/test_slo_check.py -q`
- 证据：切片报告 + SLO 报告（gitignored artifacts）。
- 回滚：`git revert <commit>` + audit record。
- 完成判据：SLO 冻结（U5 前置）。

### 12.38 C5C optical-paired-eval（W9，C7 链后）

- 目标：Optical paired eval：same-model baseline/optimized paired 对比（真实数据）、exact 能力证据（renderer/profile/font/tokenizer/pricing/cache/eval digests 进 envelope）；paired eval 后才允许 active。
- 允许文件：`support/scripts/run_optical_paired_eval.py`、`support/tests/test_optical_paired_eval.py`、`support/fixtures/optical/`、`code/crates/dtoken-compress/src/optical/`（eval 入口）。
- 禁止文件：前端、docs、release build。
- 依赖：C7A/B/C（paired eval 后才 active）。
- 分支/worktree/PR：`codex/final/c5c-optical-paired-eval` / `final-c5c-optical-paired-eval` / PR-C5C。
- 步骤：1) paired eval harness（同模型对比）；2) exact 证据 digests；3) 测试；4) 切片报告。
- BDD：Given paired eval 完成且证据完整；When 评估；Then active 资格判定有据（否则不 active）。
- 验证命令：`python3 -m pytest support/tests/test_optical_paired_eval.py -q` + `cargo test -p dtoken-compress`
- 证据：切片报告 + paired eval 报告（gitignored）。
- 回滚：`git revert <commit>` + audit record。
- 完成判据：paired eval 就绪（U7 前置）。

### 12.39 C5D optical-auto-active（W9）

- 目标：Optical exact 全门绿由 planner 自动 active（无手动 active 控件）；任一能力门不绿则同请求 PreserveText；重启一致；证据展示当前状态（三态③ 含 renderer/profile/font/tokenizer/pricing/cache/eval digests）。
- 允许文件：`code/crates/dtoken-compress/src/optical/`（planner 接线）、对应直接测试。
- 禁止文件：其他 crate 区、前端、docs。
- 依赖：C5C（paired eval 后）。
- 分支/worktree/PR：`codex/final/c5d-optical-auto-active` / `final-c5d-optical-auto-active` / PR-C5D。
- 步骤：1) planner 自动 active 接线（exact 全门绿）；2) PreserveText 同请求降级；3) 无手动控件断言；4) 测试；5) 切片报告。
- BDD：Given 全能力门绿且 paired eval 通过；When 请求到达；Then planner 自动 active（无手动控件），证据含全部 digest。
  - Given 任一能力门不绿；When 请求到达；Then 同请求 PreserveText。
- 验证命令：`cargo test -p dtoken-compress`
- 证据：切片报告 + 测试输出；U7 最终证明。
- 回滚：`git revert <commit>` + audit record。
- 完成判据：MUST 15（Optical 自动 active）关闭。

### 12.40 C8A 117-proof（W10）

- 目标：117/117 tracked registry 最终 verified production：零 placeholder/missing/not-in-scope；registry validator（基于 C0C/C0D 结构）逐项核对生产代码路径与证据。
- 允许文件：`support/scripts/validate_117_registry.py`、`support/tests/test_validate_117_registry.py`、`support/fixtures/`（117-registry 终态）、`code/crates/dtoken-headroom-adapter/`（registry 消费，如需）。
- 禁止文件：前端、docs。
- 依赖：C0C/C0D、C2–C7 链（各项能力就绪）。
- 分支/worktree/PR：`codex/final/c8a-117-proof` / `final-c8a-117-proof` / PR-C8A。
- 步骤：1) 逐项核对 registry 状态与生产证据；2) validator（缺项/placeholder → 失败）；3) 终态 117/117 verified production；4) 测试；5) 切片报告。
- BDD：Given registry 117 项；When 运行 validator；Then 全部 verified production（无 placeholder/missing/not-in-scope）。
- 验证命令：`python3 -m pytest support/tests/test_validate_117_registry.py -q`
- 证据：registry_digest + 切片报告；N0/N9/N10 U1 消费。
- 回滚：`git revert <commit>` + audit record。
- 完成判据：MUST 15（117/117）关闭。

### 12.41 C8B real-routes-platform（W10）

- 目标：real routes/platform evidence producer：真实 Agent production ingress 到 final send 证据（OpenCode Free/Go dsv4flash 两链 producer：`support/scripts/run_opencode_real_routed_test_macos.sh` 含两链模式、managed RTK 候选 producer、Codex producer 已在 PR-2）；平台差异如实记录；producer 产物绑定 C_final/source digest。
- 允许文件：`support/scripts/run_opencode_real_routed_test_macos.sh`（两链：Free=deepseek-v4-flash-free、Go=dsv4flash）、`support/scripts/run_managed_rtk_candidate_test_macos.sh`（证据接线）、对应契约测试、`support/fixtures/opencode-real-routed/`（schema/template）。
- 禁止文件：Rust/TS 业务代码、docs、workflow（PR-7 区）。
- 依赖：C8A、PR-2（producer 模式）、C4C（managed RTK）。
- 分支/worktree/PR：`codex/final/c8b-real-routes-platform` / `final-c8b-real-routes-platform` / PR-C8B。
- 共享接口（输出）：N3/N7 证据 producer（envelope；OpenCode 两链统一入口）。
- 步骤：1) OpenCode 两链 producer（同一真实安装 UI 启动→扫描→勾选→一次配置→复读验证，Onboarding 五段入口；长时/长上下文工程 workload；same-model paired；exact restore；UI/AX 截图与坐标由主代理执行时提供）；2) managed RTK producer；3) 契约测试（dry-run/fixture、脱敏断言）；4) 切片报告。
- BDD：Given 真实 OpenCode 长工程 workload；When 两链（Free/Go dsv4flash）producer 执行；Then 产物绑定 C_final/source digest，含 route/provider/model/protocol、tokens/rate、protected digests、build/run、Receipt/Ledger/fallback/restore、原配置 bytes/hash/mode，无 secret/完整 prompt/response/source。
- 验证命令：`python3 -m pytest support/tests/<opencode-real-routed 契约测试>.py -q` + producer `--dry-run`
- 证据：切片报告；N3 正式产物 W12 由主代理调度执行（两条长工程）。
- 回滚：`git revert <commit>` + audit record。
- 完成判据：N3/N7 producer 就绪（MUST 16 前置）。

### 12.42 V-1 version-unification（W11）

- 目标：版本统一 v0.0.1（C1 冲突关闭）：manifest/PRD 版本门/PLAN/status/CHANGELOG/release workflow/`check_release_version.py` 收敛为 v0.0.1；旧版本字符串只存 history/archive/CHANGELOG 历史；V-1 后新证据只绑定 v0.0.1 `C_final`。
- 允许文件：`code/Cargo.toml`、`code/Cargo.lock`（workspace 包版本机械更新）、`code/apps/desktop/package.json`、`package-lock.json`、`src-tauri/tauri.conf.json`、`docs/PRD.md`（版本门语义）、`docs/PLAN.md`（当前坐标）、`docs/product/IMPLEMENTATION_STATUS.md`（版本事实）、`CHANGELOG.md`、`support/scripts/check_release_version.py` 及其直接测试、必要 release workflow 静态版本字符串、`docs/releases/v0.0.1/`（TASKS/REQUIREMENTS 版本门）。
- 禁止文件：Rust/TS 业务逻辑代码、前端页面、其他 docs 内容。
- 依赖：C8B（功能链完成后统一版本）。
- 分支/worktree/PR：`codex/final/version-unification` / `final-version-unification` / PR-V1。
- 共享接口（输出）：v0.0.1 版本事实（后续证据绑定）。
- 步骤：1) 版本清单机械更新；2) check_release_version.py 收敛与测试；3) 版本门文档同步；4) 切片报告。
- BDD：Given 全仓版本清单；When 运行 check_release_version.py；Then 全部 v0.0.1 一致，旧版本字符串仅在 history/archive/CHANGELOG 历史。
- 验证命令：`python3 -m pytest support/tests/<check_release_version 测试>.py -q` + `python3 support/scripts/check_release_version.py`
- 证据：切片报告 + 版本门输出。
- 回滚：`git revert <commit>` + audit record。
- 完成判据：C1 关闭；V-1 完成前不生成最终候选。

### 12.43 DOC-1 docs-unification（W11，与 PR-9 并行）

- 目标：文档统一重写（§19 统一重写范围）：PRD（合并 P0.0 与历史草案、清除 0.2.0-beta.1 口径与 C2/C3 旧文案）、目标规格 v2.1（F-01/F-05 与 P0.0.10 对齐）、PLAN（滚动维护）、IMPLEMENTATION_STATUS（代码事实）、版本包九文件（版本/切片/证据索引，权威顺序 §1 对齐）、README/CHANGELOG、docs/index.md/DOCS_ORGANIZATION 入口；OSS NOTICE 对齐（Headroom/lean-ctx/pxpipe/RTK/Codebase Memory pin 事实，来自 PROV）。
- 允许文件：`docs/`（§19 统一重写范围内的正式文档）、根 `THIRD_PARTY_NOTICES.md`、`docs/legal/THIRD_PARTY_NOTICES.md`、`code/runtimes/headroom/README.md`（provenance 事实节）、`code/runtimes/codebase-memory/README.md`（pin 事实节）、`CHANGELOG.md`（历史记录部分）、`.ai-bridge/release-iteration/`（切片索引更新）。
- 禁止文件：Rust/TS 业务代码、前端、workflow。
- 依赖：V-1（版本事实先行，docs/PRD.md 与 V-1 串行）。
- 分支/worktree/PR：`codex/final/docs-unification` / `final-docs-unification` / PR-DOC1。
- 共享接口（输出）：统一后文档（权威顺序、版本、状态词表、切片/证据索引）。
- 步骤：1) 按 §19 逐文档重写；2) 权威顺序对齐（§1）；3) 清除全部已废弃口径（§0 清单）；4) NOTICE/pin 事实对齐；5) 交叉一致性核对；6) 切片报告。
- BDD：Given 统一重写后的文档集；When 交叉核对；Then 无已废弃口径残留（旧权威顺序/10%/整批回滚/6 步 onboarding/needs-source-fix 终态等），版本/状态词表/切片索引与代码事实一致。
- 验证命令：文档一致性人工核对 + `python3 support/scripts/check_structure.py`（如适用）
- 证据：切片报告 + 文档 diff 清单。
- 回滚：`git revert <commit>` + audit record。
- 完成判据：文档统一完成（C1–C10 文案侧全部关闭）。

### 12.44 PR-9 verification-orchestration（W11，与 DOC-1 并行）

- 目标：N0 完整候选门 → N1–N8 与 N10 并行真实证据 → N9 聚合的 orchestrator：候选 commit 变化后重跑 N0；聚合器消费显式 artifact path manifest 并逐项复算 SHA-256（禁止目录扫描/latest_file()/mtime/文件名/status=pass 选证）；候选变化即失效。
- 允许文件：`support/scripts/run_final_candidate_verification.py`、`support/scripts/run_tauri_ui_acceptance.py`（N6 记录器）、`support/tests/test_final_candidate_verification.py`、`support/fixtures/final-candidate-verification/`（node-artifact.schema.json 等）、`.ai-bridge/release-iteration/final-candidate-evidence.md`（过程文件模板，明确允许）。
- 禁止文件：业务代码、producer 脚本（PR-2/6/7/8/C8B 拥有）、docs。
- 依赖：V-1、PR-7（gate 聚合接口）、CF-3（envelope）。
- 分支/worktree/PR：`codex/final/verification-orchestration` / `final-verification-orchestration` / PR-9。
- 共享接口（输出）：N 节点执行清单（node_id→producer→envelope 校验）、聚合接口（C8C 消费）。
- 步骤：1) orchestrator（N0→N 并行→N9）；2) artifact manifest + SHA-256 复算；3) N6 记录器（真实 Tauri 窗口/WebKit，仅记录）；4) 契约测试（manifest 校验、候选变化失效、三平台 receipt 角色夹具）；5) 切片报告。
- BDD：
  - Given artifact path manifest 指向旧候选产物；When 聚合；Then 校验失败（candidate_commit/source_digest 不匹配即 blocked），不按 mtime/文件名/status=pass 接受。
  - Given 候选 commit 变化；When orchestrator；Then 在新候选完整重跑 N0 后才允许继续证据。
- 验证命令：`python3 -m pytest support/tests/test_final_candidate_verification.py -q`
- 证据：切片报告；N0/N1–N8/N10/N9 正式产物 W12 生成。
- 回滚：`git revert <commit>` + audit record。
- 完成判据：orchestrator 契约测试绿（MUST 18 前置）。

### 12.45 C8C gate-wiring（W11）

- 目标：gate wiring：release gate status 聚合消费接线（消费 PR-7 的 `generate_release_gate_status.py` 接口，不修改该文件）、evidence envelope 校验接入（CF-3 schema）、N0/N9 门语义接线（任一关键证据缺席/失败 → blocked；N10 独立发布阻断）。
- 允许文件：`support/scripts/`（新 gate 接线脚本，不含 PR-7 拥有的文件）、`support/tests/`（对应契约测试）、`.ai-bridge/release-iteration/final-candidate-evidence.md`（模板完善）。
- 禁止文件：`support/scripts/generate_release_gate_status.py` 及其测试（PR-7 拥有）、workflow（PR-7 区）、业务代码、docs。
- 依赖：PR-9、PR-7、CF-3。
- 分支/worktree/PR：`codex/final/c8c-gate-wiring` / `final-c8c-gate-wiring` / PR-C8C。
- 共享接口（输出）：N0–N10/N9 门判定接线（W12 执行）。
- 步骤：1) envelope 校验接入；2) N0/N9 门接线；3) N10 独立阻断接线；4) 契约测试；5) 切片报告。
- BDD：Given N1–N8 与 N10 全部结算且绑定 C_final；When 运行 N9 聚合；Then 三种结论判定（正式签名发布/未签名技术预览=本轮完成/blocked），未签名档不得宣称已签名。
- 验证命令：`python3 -m pytest support/tests/<gate wiring 契约测试>.py -q`
- 证据：切片报告 + 门接线测试输出。
- 回滚：`git revert <commit>` + audit record。
- 完成判据：gate wiring 就绪；C_final 冻结后可直接执行 W12。

## 13. 证据节点（N0–N10）与执行方

所有机器 artifact 顶层必须包含 CandidateEvidenceEnvelope（CF-3 冻结 schema）：schema_version/node_id/status/reason_codes/candidate_commit(=C_final)/source_digest(dirty=false)/registry_digest/corpus_digests/started_at/finished_at/platform/producer_revision/artifact_sha256/agent_id/capability_tier/privacy_assertions/route_decision_digest/capability_digest（route 类必填）+ Optical/benchmark/paired eval 附加 digests；不适用字段显式 null+reason。运行 artifact 统一位于 `support/artifacts/final-candidate/<C_final>/`（gitignored）；tracked schema/registry/corpus/template 位于 `support/fixtures/`、`support/schemas/` 或 `support/scripts/`。

| 节点 | 内容 | producer/执行方 | 绑定 |
|---|---|---|---|
| N0 | 候选全量质量门：structure、Rust fmt/clippy/workspace tests、前端 Vitest/tsc/build/E2E | PR-9 orchestrator（主代理执行） | 失败 blocked；修复产生新 commit 后完整重跑 |
| N1 | Receipt：PRD 最小字段、Local/Provider/claimable 分离、逐请求 cost（费用/差额/币种/单位/价格来源/更新时间）、Analytics 与 export 脱敏 | `run_receipt_evidence.py`（PR-1）+ 主代理调度 | C_final |
| N2 | Codex：Responses、动态 model、无 OpenAI auth、两种配置形态、ledger/metrics/恢复 | `run_codex_ocxlocal_routed_acceptance.py`（PR-2） | C_final |
| N3 | OpenCode Free（deepseek-v4-flash-free）+ Go（dsv4flash）两条长工程真实 routed 验收（§10） | `run_opencode_real_routed_test_macos.sh`（C8B；主代理调度真实工程执行；UI/AX 截图与坐标由主代理真实操作提供） | C_final + source digest |
| N4 | Agent 配置验收（隔离副本，PR-6 harness）+ CBM 五正式版逐版本验证（PR-CBM3/4；无资产 unavailable+原因，不内置下载/更新） | `run_agent_config_acceptance.py` + `run_cbm_version_acceptance.py` | C_final |
| N5 | cc-switch 主代理 Computer A→B→A（harness 由 PR-7 提供 collect/verify；A→B→A 真实切换由主代理用 Computer 完成，实现 Agent 不代劳）；cc-switch 全量 app type 并集核对目标 Agent 矩阵（CF-3，不只 openclaw） | 主代理 Computer + PR-7 harness | C_final |
| N6 | 真实 Tauri UI：八页/主题/语言/窄窗/Reduce Motion/native action/重启恢复；Onboarding 五段（§3.7）；普通页面仅用 §3.6 状态词表与用户文案；视觉压缩三态覆盖+重启一致；Doctor 每检查 freshness/action result；品牌图标合同（§16.3）；八页文案 OMP DS copy sign-off | `run_tauri_ui_acceptance.py` 记录器（PR-9）+ 主代理真实窗口操作；DS copy sign-off 由主代理收集 | C_final |
| N7 | macOS：安装/升级/卸载回滚/autostart/凭据 round-trip（PR-8）；RTK 解封后 managed lifecycle/hook/tee（C4 链 + C8B producer）；Dock/app bundle/window switcher 图标（§16.3） | PR-8 + C8B producer + 主代理调度 | C_final |
| N8 | Windows 产物/DACL/NSIS/PowerShell G6/同 commit fan-in（PR-7 workflow）；CBM 五版本平台矩阵（v0.8.1 无 windows-arm64 → incompatible）；EXE/taskbar/window/installer/uninstaller 图标（§16.3） | 主代理调度 Windows runner（O4）；无法取得即 external/blocked 如实记录，不伪装通过 | C_final；无法取得即 external/blocked |
| N10 | U0 ContractTest → U1 RegistryTest → U2 PropertyFuzz → U3 FaultDurability → U4 ReplayQuality → U5 PerfSLO → U6 ProviderPaired → U7 OpticalExactActive → U8 PlatformDeterminism → U9 CoverageSeal | C 系列各切片测试/脚本聚合（主代理按 U0–U9 顺序执行） | C_final；独立发布阻断 |
| N9 | 仅在 N1–N8 与 N10 全部结算后，以 C_final + source digest 为强制输入聚合三种结论（正式签名发布 / 未签名技术预览=双平台证据全绿即本轮完成 / blocked） | C8C gate wiring + PR-7 gate 聚合（主代理判定并落盘） | 逐 artifact 校验 candidate binding |

N 节点执行约束（冻结）：截图/Computer evidence 必须来自主代理真实 Tauri/cc-switch 操作，附窗口/坐标/时间，浏览器 preview 不能证明 native/runtime；实现 Agent 只建 harness 与 producer，不替主代理操控真实 UI/cc-switch；每节点产物写 `support/artifacts/final-candidate/<C_final>/`，聚合器逐项复算 SHA-256。

### 13.1 既有 G0–G9 gate 语义映射（不新增并行节点，语义保持稳定）

- 既有 G0–G8 gate 业务语义保持稳定（§20.5：能力再证明而非改语义），其验证执行纳入对应 N 节点 producer 的**同 C_final 再验证**，避免新增并行节点且满足证据绑定；G9 = N9 聚合。映射如下：

| 既有 gate | 语义 | 再验证执行位置 |
|---|---|---|
| G0 | 结构/基线 | N0 |
| G1 | 合同/类型 | N10 U0/U1 |
| G2 | codec 内核 | N10 U1/U2 |
| G3 | CCR/durability | N10 U3 |
| G4 | replay/质量 | N10 U4 |
| G5 | 性能 SLO | N10 U5 |
| G6 | PowerShell/Windows | N8 |
| G7 | Real OpenCode routed | N3（C_final 重跑；旧 G7 报告按过期证据隔离） |
| G8 | Windows/双平台同 commit fan-in | N8/N9 |
| G9 | final gate 聚合 | N9 |

- §14 结论判定表中「G0–G8 全 passed」即上表映射下的同 C_final 再验证全部通过；supplemental evidence（Codex/config/cc-switch/upgrade）不得重定义上述既有 gate 语义。

## 14. 发布结论判定（冻结）

`generate_release_gate_status.py`（fail-closed）聚合任一关键证据缺席/失败时为 blocked；三种结论只在最终候选验证完成后由主代理基于机器校验后的证据表判定并落盘：

| 结论 | 必要条件 |
|---|---|
| 正式签名发布 | 同一候选 commit G0–G8 全 passed + macOS Developer ID/notarization/Gatekeeper + Windows Authenticode + 双平台同 commit fan-in ok=true；可宣称已签名 |
| 未签名技术预览（**本轮发布完成**） | 同一候选 commit/source digest 双平台完整功能与安全证据全绿（N0–N10 全部绑定 `C_final`），macOS/Windows artifact contract 与同 commit fan-in 证据完整；产物明确标 unsigned/unnotarized，不得宣称已签名 |
| 不可发布（blocked） | 任一关键证据缺席或失败 |

- `release_ready` = 全部 MUST/证据绑定满足时为 true（不含签名要求）；否则 fail-closed false。
- `distribution_ready_public` 保持 false，不构成完成阻断。
- 任何文案/README/PR 不得写「可正式发布」除非结论①达成。

## 15. 回滚策略（冻结）

- 代码回滚：`git revert <commit>`（先 `python3 support/scripts/git_operation_audit.py record ...`）；合并后发现问题从 `main` revert 该 PR commit，重开修复切片（§10）。
- 配置回滚：ConfigPatcher 既有 backup/restore 与 exact restore 证据（双 hash、备份、receipt、复读验证、失败自动回退）；逐 Agent 独立，不整批回滚成功项。
- 凭据隔离：entry 删除（凭据只进 OS 凭据库，不落盘）。
- cc-switch：只读未写，无回滚需求；last-known-good 保留。
- 运行时：原子 route snapshot；reload 失败保留 last-known-good；pre-send fault 原文只发送一次，post-commit 无 retry。
- 发布：未签名技术预览被 blocked 时保持 blocked 并如实报告，不降级文案。

## 16. 开放问题与真实外部依赖

| # | 开放问题/依赖 | 影响 | 处置（本计划） |
|---|---|---|---|
| O1 | Onboarding 服务启动可视化未接线 | GOAL 3 首次引导体验与 N6 | PR-14 接线并 N6 验收 |
| O2 | native 配置流程无阶段回调 | 一次点击配置的进度反馈 | PR-3 定义阶段回调合同（Rust/Tauri 侧收敛） |
| O3 | Onboarding 6 步 vs 五段差异 | 首次引导一致性 | PR-14 统一 |
| O4 | Windows runner/VM 可用性 | N8/Windows 证据与同 commit fan-in | 主代理调度 runner；无法取得即 external/blocked 如实记录（不伪装通过） |
| O5 | 外部资产可得性：CBM 官方资产（0.8.0/0.7.0/0.6.1）、固定版本上游 clone | 五正式版验证与来源事实 | 无资产按 §12 诚实 unavailable 不阻断；存在但校验失败才 blocked（PR-CBM3/4 + PROV） |
| DEP-1 | N3 两条长工程真实 workload 需要真实安装 OpenCode 与网络 | N3 发布阻断 | 主代理调度真实工程执行；producer 由 C8B 提供 |
| DEP-2 | N5 A→B→A 需要本机 cc-switch 真实配置 | N5 发布阻断 | 主代理 Computer 执行（只读，不写 cc-switch） |
| DEP-3 | N6/N7 需要本机真实 Tauri 构建与 macOS 环境 | N6/N7 | 主代理调度；本机可执行 |

## 17. 验收矩阵（冻结）

- A1–A12（配置交互验收，设计 §16.1）：由 PR-3/PR-6/PR-10/PR-11/PR-13/CF-3 实现，N4/N5/N6 验收。
- C1–C5（统一压缩质量与恢复验收，设计 §16.4）：由 C2/C3/C6B/C6C/C7 链实现，N3/N10 验收。
- N0–N10（最终候选验证矩阵，设计 §16.2）：§13 执行；N10（U0–U9）为独立发布阻断。
- UI 与平台验收（设计 §16.3）：1180×780 与 920×640、浅/深色、中/英、Reduce Motion、侧栏专项、品牌图标合同 → PR-4/PR-14 实现 + N6/N7/N8 验收。
- 词表验收（设计 §16.1 A8）：PR-3/PR-4 实现 + N6 检查普通页面仅用 §3.6 词表。

## 18. MUST → 切片 → 测试/证据 覆盖矩阵（全部 22 项，无遗漏无降级）

| # | MUST（设计 §2.1 原文） | 实现切片 | 验收节点/测试 |
|---|---|---|---|
| 1 | 完整 Agent 清单与用户选择（已安装/未安装/已配置/未配置/配置失败；未安装不可勾选，已安装默认全选可取消） | CF-3、PR-3 | A1 → N4/N6 |
| 2 | Agent 矩阵按官方能力分层闭环（Route/MCP-Hook/Detection-only 各层独立闭环；任一要求适配 Agent 在其所属能力层未闭环即 `release_ready=false`；不得以「等待安全适配/来源冲突/暂不支持自动配置」收尾） | CF-3、PR-3、PR-10、PR-11、PR-13 | A2 → N4/N6/N5（cc-switch 并集核对） |
| 3 | 一次点击直接配置（无第二层确认弹窗；后台 plan/脱敏 diff/备份/apply/复读/恢复点/receipt；正常结果只显示「配置成功」） | CF-2、PR-3 | A3、A9 → N4/N6 |
| 4 | 幂等重配（无变化只重新验证不重复写入；有变化自动更新并验证） | PR-3 | A10 → N4 |
| 5 | 逐 Agent 独立事务（失败只回退该 Agent 本次未完成写入，其他成功保持；单项「重试」，不得整批回滚成功项） | CF-1/CF-2、PR-3、PR-6 | A3、A12 → N4 |
| 6 | 三类上游运行模型全部自动适配（agent_managed_auth / dynamic_local_proxy / cc_switch_managed；多模型是正常能力不是冲突；不重复登录、不持久化 token/cookie/Authorization） | PR-3、PR-10、PR-11、PR-13 | A4、A5、A6 → N2/N4/N5 |
| 7 | 无上游选择器（不选 Provider/上游/模型/协议/endpoint/cc-switch profile；Agent 清单勾选除外） | PR-3 | A7 → N6 |
| 8 | 错误只面向结果（普通页面无内部机制术语；「配置失败」+用户可执行说明+「重试」；Detection-only 允许「已检测/不支持本地路由」这一批准终态） | CF-2、PR-3、PR-4 | A8 → N6 |
| 9 | Route/MCP-Hook/Detection-only 诚实区分（MCP/Hook/Skill 不得冒充路由成功） | CF-3、PR-3、PR-10、PR-11 | A11 → N4/N6 |
| 10 | 首次引导五段（welcome → service-start 动画+可视进度 → scan → 一次点击配置 → start-use；verify 归配置段内部） | PR-14 | 设计 §3.7 → N6 |
| 11 | canonical pipeline 与单一 IR（一请求一 IR、每 node 恰一终态动作；HTTP/Rust/MCP/TS/Proxy 共用同一 pipeline；single dispatch 必须包含 Native Responses；发送体=验证体） | C0A、C1A、C1B、C1C、C2、C3、C4C、C5 | N10 U0/U1 + N3 |
| 12 | 不可伪造 type-state 与 receipts（ReadyToSend 仅 runtime pipeline 密封；调用方布尔值/任意 digest/时间戳哨兵不能铸造；生产 transport 必须消费 SendCommitReceipt；CCR 真实 durable 闭环） | CF-1、C0B、C6C、C3 | A9 → N1/N10 U0 |
| 13 | 四维独立 accounting（compression/command-output/provider-cache/relay 独立，禁止求和；Local estimate/Provider actual/claimable 分离；unknown≠0；claimable 需 paired 证据与归因链） | PR-1、C2（provider-cache）、C3（relay）、C4C（command-output）、C5（Optical） | N1/N10 U6 |
| 14 | LeanCTX 九边界与 CCR（九边界 MUST；搬移而非复制；Reuse/memory 在 transform plan 前进入 IR，删除 post-validation injection） | C3、C1C | N10 U1/U3 |
| 15 | Headroom 117 项 / RTK / Optical 边界（117/117 verified production 零 placeholder；RTK 三平台先解封再生产实现；Optical exact 全门绿 planner 自动 active，任一不绿 PreserveText） | C0C、C0D、C8A（117）；C4U、C4A/B/C（RTK）；C5A–C5D（Optical） | N10 U1/U7 |
| 16 | 真实 routed 验收（Codex→18001→ocxlocal→动态模型上游，agent_id=codex、Responses、无 OpenAI auth、model 透传；OpenCode Free/Go dsv4flash 两条长工程；配置事务精确恢复不能替代） | PR-2（Codex）、C8B（OpenCode 两链） | N2、N3 |
| 17 | 配置写入事务链（detect → parse → plan → redacted diff → backup → 一次点击授权 → apply → reread/verify → receipt；失败精确回滚原始字节；只写 d-token-owned 字段；清除只删带 ownership 证据内容且幂等） | CF-1/CF-2、PR-3、PR-6、PR-10、PR-11 | A9、A10、A12 → N4 |
| 18 | 证据绑定与 fail-closed 门（证据绑定 C_final 与 source_digest/registry/corpus digests；聚合器逐项复算 SHA-256，禁止 mtime/文件名/status=pass 选证；G0–G9 与 N0–N10 绑定同一 C_final；任一缺失 release_ready=false） | CF-3、PR-7、PR-9、C8C | N0/N9 |
| 19 | 凭据与隐私安全（凭据只进 OS 凭据库、配置只保存不透明引用、禁止明文 fallback；secret/完整 prompt/绝对路径不进日志/配置/导出/提交；本地服务只绑定 127.0.0.1；SecretClassifier 统一复用） | PR-5、CF-1、PR-2/6/7/8/C8B（producer 脱敏断言） | N1/N6/N8 + 各 producer 契约测试 |
| 20 | UI 双平台双尺寸双语（1180×780 与 920×640、浅/深色、中/英、Reduce Motion；920×640 无横滚由真实 layout engine 证明、N6 真实 Tauri 复核） | PR-4、PR-14 | N6/N8 |
| 21 | 真实长工程端到端压缩率 ≥5% 与零字节保护（rate 口径；真实 Agent production ingress 到 final send、paired 条件相同、只计 actual；fallback/failure/shadow/estimate 不计；protected content 零字节变化；恢复精确；发送前失败保留原始正确请求；上游已接收或 streaming 后禁止自动重发） | C2、C3、C6B、C7A/B/C、C8B | C1–C4 验收 → N3/N10 U4/U5/U6 |
| 22 | CacheAligner 为 canonical pipeline 真实组件（真实 cache alignment + 真实 receipt；只计 provider-cache 分账；禁止 accounting/metadata/no-op 冒充） | C2 | C5 验收 → N10 U0/U1 + N3 |

## 19. 冲突清单 C1–C10 处置（全部进计划，不遗漏）

| # | 冲突点 | 批准设计口径 | 处置切片 | 验证 |
|---|---|---|---|---|
| C1 | 版本号：活跃文档 `0.2.0-beta.1` vs 目标 v0.0.1 | 目标版本 **v0.0.1** | V-1（版本统一） | `check_release_version.py` + N0 |
| C2 | Agent 选择器旧口径：`needs-choice`/`selection_required` 生产残留 vs 仅禁上游 chooser | Agent 勾选允许；上游 chooser 全链路清除 | PR-3 | A7 + 词表契约测试 → N6 |
| C3 | 整批确认/回滚旧文案（review-gated、whole-batch 逆序回滚）vs 一次点击+逐 Agent 独立事务 | 一次授权直接执行；部分失败不回滚成功项 | PR-3、PR-6、DOC-1（文案清除） | A3、A12 → N4/N6 |
| C4 | Onboarding 6 步 vs 五段 | 五段（verify 归配置段内部） | PR-14 | §3.7 → N6 |
| C5 | Native Responses 绕过 canonical；SendCommitReceipt 未生产消费 | single dispatch 必须包含 Native Responses；发送体=验证体；receipt 生产消费 | PR-C1C（含旧 PR-12）、PR-C0B、PR-C6C | N10 U0/U1 + N1 |
| C6 | RTK 三平台 `ManagedRtkTransactionState::Blocked` | 解封（未配置=unavailable）后走 C4A/B/C 生产实现 | PR-C4U + PR-C4A/B/C | N7 + U 系列 |
| C7 | CBM 五正式版兼容缺失；rc.1 误当五版本 | 五正式版 v0.9.0/v0.8.1/0.8.0/0.7.0/0.6.1；rc.1 标 prerelease 不计 | PR-CBM3/4 | N4/N8 |
| C8 | 「配置事务精确恢复」冒充 Codex 真实 routed-flow | 独立真实 routed-flow 报告必须存在；Codex 无报告只显示 configured/not_verified | PR-2（producer）、N2 | N2 |
| C9 | 前端范围扩张风险（plans/ 动画计划、UI design 目录） | 前端严禁大范围重构（§7） | §7 边界执行 + 主代理审查门 | 每 PR 审查 |
| C10 | 价格/费用口径未统一 | 仅 Token 与价格证据同时存在才展示费用；缺证据 unavailable；缺价格不能当作 0 | PR-1/N1 收口 | N1 |

## 20. 迁移原则（从当前实现迁移，不推倒重写）

1. 在既有实现上收敛：18 个 Rust crate、Tauri 2 + React 18 桌面 UI、九类 codec 生产 evidence、CCR、23 Agent 检测矩阵、11 个已验证 adapter、cc-switch 只读 watcher、managed RTK lifecycle、真实 G7 OpenCode 证据（历史）均为迁移基线；不重写 runtime、不重建 UI、不另起第二套 pipeline（§2 非目标 10）。
2. 策略搬移而非复制：LeanCTX/Context Bus 策略搬移到新 `dtoken-context-bus`（C3），`dtoken-context-store` 只实现 persistence port，`dtoken-compress` 只消费 reuse decision。
3. 版本统一（V-1）：C0–C8 与功能/平台切片完成后一次性收敛 v0.0.1；旧版本字符串只存 history/archive/CHANGELOG 历史（§12.42）。
4. 旧证据失效：V-1 后新证据只绑定 v0.0.1 `C_final`；旧 G7/DMG/browser preview/非 C_final 测试报告全部按过期证据隔离（§3）。
5. 能力再证明而非改语义：既有 G0–G8 gate 语义保持稳定；Codex/config/cc-switch/upgrade 等 supplemental evidence 不得重定义既有 gate（主代理审查门 8）。
6. 每步可回滚：单切片 revert、配置 exact restore、凭据隔离删除、cc-switch 只读无回滚需求（§15）。
7. 缺口修复路径：Native Responses 纳入 single dispatch（C1C）、SendCommitReceipt 生产接线（C6C）、RTK 三平台解封（C4U）、CBM 五正式版兼容（PR-CBM3/4）——都是对既有实现的收敛改造，不是重写。

---

本计划与 `.ai-bridge/release-iteration/TASKS.md` 互为唯一执行索引：切片 ID/波次/DAG/串行表/覆盖矩阵两文件一致；执行期变更按 §10 变更协议同步两文件。