# d-token 路线甲 TDD 执行文档（补齐 Headroom §8 全能力）

- 文档日期：2026-07-17
- 决策记录：**决策 A = 甲**（按 PRD 原标准补齐 §8 全能力后发布，不收敛承诺）；**决策 B = 本文 §1 裁定**（由主审查者判定）。
- 依据：`RELEASE_READINESS_SYNTHESIS_2026-07-16.md` 及同目录三份子报告（前端/后端/UX-性能）。
- 定位：**可直接逐任务动手的 TDD 计划**。每个任务给出 RED（先写失败测试）→ GREEN（最小实现，含精确接入点 file:line/函数名）→ REFACTOR → DoD（可验证）。
- 铁律：**先写测试并确认它失败（RED），再写最小实现使其通过（GREEN），最后重构（REFACTOR）。任何任务未达 DoD 不得标记完成。**

---

## 0. 如何使用本文档

### 0.1 全局验证命令（每个任务 DoD 至少跑相关子集）
```bash
# 结构
python3 support/scripts/check_structure.py
# 后端
cd code
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
# 前端
cd apps/desktop
npm run test:run
npx tsc --noEmit
npm run build
npm run test:e2e
```

### 0.2 任务模板含义
- **优先级**：P0=甲路线发布阻断；P1=强烈建议；P2=打磨。
- **依赖**：必须先完成的任务 ID。
- **RED**：先落地的失败测试（文件 + 测试名 + 关键断言）。写完先跑，**必须红**。
- **GREEN**：使测试通过的最小实现，标注精确接入点。
- **DoD**：完成判据（命令 + 通过标准 + 证据）。

### 0.3 Epic 总览与波次
| Epic | 主题 | 波次 |
|---|---|---|
| G 加固与可测性基线 | 进程回迁、property/fuzz、依赖下沉、命令单源 | 波1（最先，无能力依赖，可并行） |
| M Headroom §8 能力补齐（甲核心） | memory/output/MCP/Code/text/RTK/chain_bridge | 波2（依赖 G 的测试与管线基线） |
| P Provider 矩阵实证 | 五类 route 真实 routed evidence | 波3（依赖 M7 + G1） |
| F 前端治理与体验 | 类型化/拆分/selector/凭据/分页/虚拟化 | 波1-2（与后端并行） |
| R Release 实测与签名 | 性能 p95/双平台/签名公证 | 波4（依赖全部定稿 commit） |

### 0.4 并行策略（对齐全局"独立任务必须并行"原则）
- **波1 三线并行**：`G1‖G2‖G3‖G4`（后端加固）‖ `F1→F2/F3`（前端治理）‖ `F4/F5`（前端体验）。
- **波2**：M1‖M2 共享接入点需串行协调（同改 `anthropic_messages`/`chat` 主流程），其余 M3/M4/M5/M6/M7 相互独立可并行。
- **波3**：P1 依赖 M7（chain_bridge）与 G1（进程稳定）。
- **波4**：R1/R2/R3 依赖前三波定稿同一候选 commit。

---

## 1. 决策 B 裁定：cc-switch / chain_bridge 定位

**裁定：chain_bridge 采用"cc-switch 端口作为显式 provider + 协议 bridge + 单跳转发 + chain_mode 真正驱动执行 + 可审计 Ledger"，不实现 d-token 内部两次 HTTP 跳。**

理由：
1. cc-switch 本身是本地路由代理，暴露一个 HTTP endpoint。PRD §5.2 的"d-token 压缩后转发到 cc-switch，再由 cc-switch 路由到 final"中，**第二跳是 cc-switch 的职责，不是 d-token 的职责**。d-token 只需一次转发到 cc-switch 的 base_url。发明 d-token 内部的双 HTTP 跳是过度工程。
2. 当前缺口（主审查者已核实 `dtoken-core-api/src/lib.rs`）：`ChainMode::ChainBridge` 只出现在日志（:299/:5900）、CSV（:5865）、route 过滤（:7694）、状态报告（:7710），**没有任何执行分支让 chain_mode 驱动"把转发目标解析为 cc-switch endpoint 并触发协议 bridge"**。转发函数只有 4 个单跳 forward。
3. 因此"真实实现 chain_bridge"= 让 `chain_mode = compress_then_cc_switch` 真正驱动 route resolver 把 outbound provider 解析为 cc-switch endpoint，触发 input/output 协议 bridge（如 Anthropic→OpenAI），并在 RouteDecision/Event Ledger 完整记录两段链路语义（input_protocol/output_protocol/cc_switch endpoint 脱敏）。这落在 **M7**。

---

## 2. Epic G — 加固与可测性基线（波1，可并行）

### G1　进程身份/服务生命周期回迁 `dtoken-process`，替换死桩
- 优先级 P0｜依赖 无｜涉及 `code/crates/dtoken-process/src/lib.rs`、`code/apps/desktop/src-tauri/src/main.rs`
- 目标：`dtoken-process::ProcessManager` 承载真实进程身份验证（当前死桩 `start_placeholder:236`/`stop:241` 无校验；真实 6 层逻辑散在 main.rs `process_birth_token:3102`/`executable_identity_matches:3193`/`listener_owned_by_pid:3211`/`stop_dtoken_service_impl:7105`）。
- **RED**（`dtoken-process/src/lib.rs` 内 `#[cfg(test)]` + 新 `tests/identity.rs`）：
  - `stop_verified_rejects_foreign_pid()`：构造一个非 d-token 出生标识的 PID，断言 `manager.stop_verified(...)` 返回 `Err(ProcessStopError::IdentityMismatch)`（当前无此函数 → 编译失败即红）。
  - `stop_verified_requires_exe_hash_match()`：exe hash 不符时拒绝。
  - `listener_ownership_checked_before_kill()`：端口被非本进程占用时拒绝。
- **GREEN**：把 `process_birth_token`/`executable_identity_matches`/`listener_owned_by_pid`/`terminate_process` + manifest 校验迁入 `dtoken-process`，定义 `ProcessIdentity`、`ProcessStopError`、`verify_service_identity()`/`stop_verified()`；`main.rs` 的 `stop_dtoken_service_impl` 改为调用 crate（保留 Tauri 侧仅做参数编排）。
- REFACTOR：`ProcessManager` 去掉 `start_placeholder` 死桩，服务生命周期状态机独立成 `dtoken-process::lifecycle` 子模块。
- **DoD**：`cargo test -p dtoken-process` 覆盖 6 层校验各一条；`main.rs` 不再直接实现身份比对（`rg "fn executable_identity_matches" src-tauri/src/main.rs` 零命中）；`cargo clippy -D warnings` 通过。

### G2　压缩/协议/CCR 引入 property + fuzz + fault 测试（G2/G3 门禁前置）
- 优先级 P0｜依赖 无｜涉及 `dtoken-compress`、`dtoken-context-store`、各自 `Cargo.toml`
- 目标：满足 PLAN §1C.2 G2/G3 的"property/fuzz/fault"门（当前全 workspace 无 proptest/quickcheck，467 测试全 example-based）。
- **RED**（新增 `dtoken-compress/tests/props.rs`、`dtoken-context-store/tests/ccr_fault.rs`）：
  - `prop_frozen_prefix_byte_identical(any_ir)`：对任意 IR，压缩后 FrozenPrefix 段字节完全等同原文。
  - `prop_secret_never_transformed(any_body_with_secret)`：含 secret/exact 的节点永不进入变换（分类命中即 PreserveText）。
  - `prop_json_minify_semantic_equivalent(any_json)`：JSON minify 前后 `serde_json::Value` 相等。
  - `prop_ccr_envelope_roundtrip(any_bytes, key)`：AES-256-GCM 封装→解封恒等；篡改 nonce/AAD 必失败。
  - `ccr_store_crash_recovery()`：写入中途注入 IO 错误，重开 store 不出现半写条目。
  - 先加 `proptest` 到两个 crate `[dev-dependencies]`，测试写完先跑必红（不变量尚未被保证/或 API 未暴露）。
- **GREEN**：为 ContentRouter/JSON minify/hybrid IR lowering/CCR envelope 暴露可测入口，修正任何被 property 证伪的不变量。
- **DoD**：`cargo test -p dtoken-compress -p dtoken-context-store` 全绿；`rg "proptest" code/crates/*/Cargo.toml` 命中；malformed corpus ≥ 20 例。

### G3　Agent 类型下沉 `dtoken-types` + 统一时间工具
- 优先级 P1｜依赖 无｜涉及 `dtoken-types`、`dtoken-context-store/src/lib.rs:5`、`dtoken-config-patcher`
- 目标：解除 `dtoken-context-store` → `dtoken-config-patcher` 反向依赖（后端报告 M-1）；统一散落各 crate 的 `now_ms()`/`unix_seconds()`（L-4）。
- **RED**：`dtoken-types/tests/no_reverse_dep.rs` 断言 `AgentDiscoverySnapshot`/`DetectedAgent` 从 `dtoken_types` 可导入；`cargo tree -p dtoken-context-store` 不含 `dtoken-config-patcher`（先写脚本断言，当前红）。
- **GREEN**：把这两个类型移到 `dtoken-types::agent`；两侧改 `use dtoken_types::agent::...`；提取 `dtoken_types::time::now_ms()`。
- **DoD**：`cargo tree -p dtoken-context-store | rg config-patcher` 零命中；workspace 编译通过。

### G4　`invoke_handler` 单一来源
- 优先级 P2｜依赖 无｜涉及 `main.rs:10428`（debug）/`:10517`（release）
- 目标：消除两份手工维护、仅差 4 个测试命令的 handler 列表（后端 M-2）。
- **RED**：`src-tauri/tests/handler_parity.rs` 断言"release 列表 = 基表；debug 列表 = 基表 + {test_opencode,workload,smoke,...}"，当前两份手抄 → 断言漂移即红。
- **GREEN**：抽 `fn base_commands!()` 宏 + `cfg!(debug_assertions)` 拼接测试命令。
- **DoD**：release 构建 `rg "test_opencode|workload" $(bundle)` 零命中；parity 测试绿。

---

## 3. Epic M — Headroom §8 能力补齐（波2，甲核心）

> 接入点已核实：请求主流程在 `dtoken-core-api/src/lib.rs` 的 `anthropic_messages`/`chat`/`responses` handler，关键序列为 `apply_hybrid_preparation(&mut payload,&prepared)` → `prepare_ccr_for_request(...):7271` → `apply_routed_pipeline_payload(...):7293` → `forward_anthropic_messages(_stream)`。type-state 管线在 `dtoken-runtime-pipeline`：`PipelineInput→PreparedRequest→CandidateRequest→ValidatedRequest→ReadyToSend`。

### M1　memory / SharedContext 接入 request lifecycle（消除存储层孤立）
- 优先级 P0｜依赖 G3｜涉及 `dtoken-core-api/src/lib.rs`、`dtoken-context-store/src/lib.rs`
- 目标：`context-store` 的 `list_visible_memory(&MemoryAccessScope):566`、`upsert_memory:408`（真实且带 share-grant 可见性）接入请求准备与响应学习。当前 `core-api` 零调用（后端 C-1），`/memory/*` 全 metadata-only（lib.rs:6085）。
- **RED**（`dtoken-core-api/tests/memory_lifecycle.rs`）：
  - `prepared_request_includes_visible_project_memory()`：预置 `upsert_memory`（project=P, summary=S），构造 P 的 anthropic 请求，断言 prepare 后 context 注入了 S 的脱敏摘要（当前零调用 → 红）。
  - `memory_scope_isolation_enforced()`：agent A 的私有 memory 不出现在 agent B 未授权请求的 context。
  - `no_raw_prompt_persisted_as_memory()`：断言写入的 memory 只含结构化摘要，不含 raw prompt（脱敏）。
- **GREEN**：在 `anthropic_messages`/`chat` 的 `apply_hybrid_preparation` 之前，用 `RequestScope` 构造 `MemoryAccessScope`，调 `store.list_visible_memory(scope)`，把命中的 summary/keywords 注入 Protected Context IR 的 system/前缀区（受 byte budget 限制）；响应成功后按白名单 `upsert_memory` 结构化摘要（provenance 绑定 request_id）。
- REFACTOR：memory 注入策略抽为 `dtoken-runtime-pipeline` 的 prepare 插件，避免 core-api 主流程膨胀。
- **DoD**：memory_lifecycle 测试全绿；`/memory/summary` 返回真实 store 派生而非 metadata-only（注释 lib.rs:6085 移除）；隐私测试断言无 raw 落盘。

### M2　output shaping 接入发送前改写 + accounting
- 优先级 P0｜依赖 M1（共享 prepare 接入协调）｜涉及 `dtoken-core-api`、`dtoken-runtime-pipeline`
- 目标：`/output-shape`（当前注释"not yet applied to outbound requests" lib.rs:6122）真正参与 route-resolved 请求；measured/holdout-estimated/unknown 严格分账（PRD §0.0.2(11)）。
- **RED**（`dtoken-core-api/tests/output_shaping.rs`）：
  - `verbosity_terse_applied_to_outbound()`：策略 verbosity=terse 时，`ReadyToSend.body` 的 system 指令含 terse 约束（当前不改 outbound → 红）。
  - `error_paths_preserve_effort()`：错误/新问题请求不降 effort。
  - `shaping_accounted_as_estimated_not_measured()`：accounting.disposition 标 estimated，不计入 claimable。
- **GREEN**：在 `apply_routed_pipeline_payload` 生成 candidate 前应用 shaping 到请求体，经同一 validator 门控；写 `context_accounting` 的 shaping 分账字段。
- **DoD**：output_shaping 测试绿；shaping 改写经过 `ValidatedRequest` 门（不可越级）；Ledger 显示 estimated 分账。

### M3　新建 `dtoken-mcp` crate（stdio JSON-RPC + 真实 handler）
- 优先级 P0｜依赖 无（可与 M1/M2 并行）｜涉及 新 `code/crates/dtoken-mcp/`、`code/Cargo.toml`、`dtoken-core-api`（复用 compress/store）
- 目标：PLAN §1C.0:824 列出但缺失的 crate。当前 `/headroom/mcp` fail-closed 返回 `mcp_transport_not_implemented`（lib.rs:7951）。实现 `compress`/`retrieve`/`stats` 三个真实 tool。
- **RED**（`dtoken-mcp/tests/jsonrpc.rs`）：
  - `initialize_returns_tool_list()`：stdio JSON-RPC `initialize` 返回三个 tool schema。
  - `compress_tool_roundtrip()`：`tools/call compress` 复用 `dtoken-compress` 返回真实 artifact + reason。
  - `retrieve_requires_valid_ccr_ref()`：无效 ref fail-closed。
  - `malformed_jsonrpc_rejected()`：坏帧返回 -32700。
- **GREEN**：实现 stdio transport（`tokio` 逐行读 + JSON-RPC 2.0 编解码）、tool registry、权限/取消/deadline；handler 委托现有 `dtoken-compress`/`dtoken-context-store`。core-api 的 `/headroom/mcp` 状态在 crate 就绪后返回 `configured=true`（否则维持 fail-closed）。
- **DoD**：`cargo test -p dtoken-mcp` 绿；`initialize`→`tools/call` 集成测试通过；未接入时 `configured=false`（G4 门禁风格）。

### M4　Code AST 压缩：完成 Rust 全链（去 `EvaluationOnly`）
- 优先级 P0｜依赖 G2｜涉及 `dtoken-compress/src/code_codec.rs`
- 目标：Rust 现为 `EvaluationOnly`（`decision_for:170` reason `implementation-not-active`，macro stringification 风险未证），production passthrough。完成语义安全 transform 全链使 Rust active。
- **RED**（`dtoken-compress/tests/code_rust.rs`）：
  - `rust_compress_preserves_ast_semantics()`：对 corpus 中每个 Rust 文件，压缩→`syn` reparse，AST 语义等价（含 macro 调用保真）。
  - `rust_compress_roundtrip_compiles()`：压缩产物经 `syn` 再 emit 后可编译（fixture 项目）。
  - `rust_negative_savings_passthrough()`：收益为负则 passthrough。
- **GREEN**：实现 macro token 安全处理（保守：含未证 macro 的节点保留原文），把 `EvaluationOnly` 提升为 active（仅 Rust）；绑定 commit 的 corpus digest。
- REFACTOR：其余 7 语言保持 `LanguageNotImplemented`（本任务只交付 Rust；其他语言各自独立后续任务）。
- **DoD**：code_rust 测试绿（≥30 文件 corpus）；property 无反例；STATUS 可将 Rust Code codec 记为 active。

### M5　通用文本 / HTML 质量 codec（去纯透传）
- 优先级 P1｜依赖 G2｜涉及 `dtoken-compress/src/lib.rs:26`（`compress()` 纯透传）
- 目标：通用 `CompressionEngine::compress` 当前 `tokens_saved:0` 透传；补一般 text/HTML 可精确重建 codec（保留现有 Search/Build/Diff/repeat fold 之外的通用文本）。
- **RED**（`dtoken-compress/tests/text_codec.rs`）：`general_text_fold_reconstructs_exactly()`、`html_fold_preserves_structure()`、`secret_bearing_text_passthrough()`。
- **GREEN**：实现可逆通用文本/HTML fold，取消/限时/负收益 fail-closed；接入 ContentRouter 分类。
- **DoD**：text_codec 测试绿；round-trip 精确；decision evidence 记录 reason。

### M6　integrated RTK 内部能力（去"仅外部检测"）
- 优先级 P1｜依赖 无｜涉及 `dtoken-preprocessor/src/lib.rs`（native 模式 `native_unavailable_status()`）
- 目标：PRD §0.4/§8 要求 RTK 为 runtime 内部能力。实现 bundled provenance + command rewrite/filter/exit/tee 子集。
- **RED**（`dtoken-preprocessor/tests/rtk.rs`）：`command_rewrite_reduces_noise()`（ANSI/空行/重复折叠）、`rtk_provenance_recorded()`（bundled binary hash/version）、`fail_safe_on_unknown_command()`。
- **GREEN**：实现内部 command output preprocessor（复用 §6.6 fallback preprocessor 能力升级）、provenance 记录；native 模式返回真实 profile 而非 unavailable。
- **DoD**：rtk 测试绿；`profile_from_status` 可产出 `RtkInternal`；savings 分账为独立层（不与 runtime savings 直接相加，PRD §10）。

### M7　chain_bridge / cc-switch 二次转发执行（决策 B 落地）
- 优先级 P0｜依赖 G1｜涉及 `dtoken-router/src/lib.rs`、`dtoken-core-api/src/lib.rs`（route 解析 + forward）
- 目标：让 `ChainMode::CompressThenCcSwitch`/`ChainBridge` **真正驱动**转发目标解析为 cc-switch endpoint + 协议 bridge（当前只记录，无执行分支，:7694/:7710 仅报告）。
- **RED**（`dtoken-core-api/tests/chain_bridge.rs`）：
  - `chain_mode_resolves_ccswitch_endpoint()`：route.chain_mode=compress_then_cc_switch 时，解析出的 outbound provider base_url == cc_switch_provider 的 endpoint（当前解析到单一 provider → 红）。
  - `anthropic_to_openai_bridge_on_ccswitch()`：input=anthropic、cc-switch 期望 openai 时触发协议 bridge。
  - `chain_ledger_records_both_hops_semantics()`：Event Ledger 含 input_protocol/output_protocol/cc_switch endpoint（脱敏），chain_mode 正确。
  - `no_silent_fallback_when_ccswitch_unreachable()`：cc-switch 不可达时显式错误，不回退官方上游。
- **GREEN**：在 route resolver 增加 chain_mode 分支：当 compress_then_cc_switch/chain_bridge 时，将 outbound target 解析为 `cc_switch_provider_id` 的 endpoint，标记需要的协议 bridge；`forward_*` 沿用单跳但目标为 cc-switch base_url；RouteDecision/Ledger 记录链路语义。
- **DoD**：chain_bridge 测试全绿；PRD §5.2 场景（Claude Code→d-token→cc-switch）有可重建 Ledger 证据；无静默 fallback。

---

## 4. Epic P — Provider 矩阵实证（波3）

### P1　custom_openai + custom_anthropic + cc_switch 真实 routed evidence
- 优先级 P0｜依赖 M7、G1｜涉及 `support/scripts`（新增 routed evidence 脚本）、`dtoken-event-ledger`
- 目标：五类 route 目前仅 OpenCode 单点 + fixture（后端 H-4）。补三类真实 routed 证据（G7 门）。
- **RED**：`support/scripts` 新增 `verify_routed_matrix.py`，对每类 route 断言"请求经 d-token → Ledger 可重建 input/output protocol/provider/chain_mode"；未接通前脚本失败。
- **GREEN**：用真实/免费 upstream（OpenCode 官方免费模型 + 一个 OpenAI-compatible + 一个 Anthropic-compatible 中转）跑通；cc_switch 用本地 cc-switch 端口。
- **DoD**：三类各一条 Ledger 可重建证据；无静默 fallback；脱敏无凭据；证据归档 `docs/history/`。

---

## 5. Epic F — 前端治理与体验（波1-2，与后端并行）

### F1　desktop payload 强类型 + Zod 边界校验
- 优先级 P0｜依赖 无｜涉及 `code/apps/desktop/src/app/lib/runtime/types.ts`、`useRuntimeDashboard.ts:142-143,271`
- 目标：消除 `get_desktop_summary`/`get_desktop_data` 的 `Record<string,any>`（前端 H-2，109 处 any）。
- **RED**（`lib/runtime/desktopSchema.test.ts`）：`desktop_data_schema_rejects_missing_revision()`、`desktop_data_schema_parses_real_payload()`（用真实 payload fixture）、`invalid_payload_maps_to_partial_state()`——先定义 schema 期望，实现前红。
- **GREEN**：定义 `DesktopData`/`DesktopSummary` 类型 + Zod schema；在 `invokeNative` 边界 `DesktopDataSchema.safeParse(raw)`，失败进 partial/error。
- **DoD**：`npx tsc --noEmit` 通过；核心 payload 无 `any`；schema 测试绿。

### F2　抽离超大页面 view-model（Overview 样板 → Doctor/VisualCompression/Advanced/Install）
- 优先级 P1｜依赖 F1｜涉及 `pages/Overview.tsx:980`→`lib/overview/model.ts`，及 Doctor 1773/VisualCompression 1612/Advanced 1332/Install 1191
- **RED**：`lib/overview/model.test.ts` 直接测已导出纯函数 `overviewFromPayload`/`deriveOverviewOperationalSummary`（迁移后仍绿，作为回归护栏）。
- **GREEN**：纯派生函数移入 `lib/<page>/model.ts`，页面只留 JSX+hooks。
- **DoD**：各主文件 ≤550 行；`wc -l pages/*.tsx` 达标；原 Overview.test.tsx 全绿。

### F3　selector store / slice 订阅（消除单一大 Context 全量重渲染）
- 优先级 P1｜依赖 F1｜涉及 `useRuntimeDashboard.ts:73,249-252`（单 Context）、native `main.rs:10253 slice_revisions`
- **RED**（`lib/runtime/selector.test.tsx`）：`statusbar_not_rerendered_on_unrelated_slice_change()`——用 React Testing Library + render 计数，单 slice 变化时 StatusBar commit 次数不增（当前必红）。
- **GREEN**：迁到 `useSyncExternalStore` slice 订阅（install 机已用此范式），优先 StatusBar + Overview；复用 native slice_revisions。
- **DoD**：selector 测试绿；React Profiler 证明无关消费者不重渲染。

### F4　一键安装 needs-credential 内联补录弹窗
- 优先级 P0｜依赖 无｜涉及 `Install.tsx:491`、`installMachine.ts`、复用 `store_provider_credential`
- 目标：当前 needs-credential 跳转高级安装（换页，UX HIGH-2），改为内联 `OperationDialog` 补录。
- **RED**（`Install.credential.test.tsx`）：`needs_credential_opens_inline_dialog_not_advanced()`、`credential_submit_transitions_to_ready()`。
- **GREEN**：一键区加"仅补录凭据"OperationDialog；`installMachine` needs-credential→ready 转移。
- **DoD**：needs-credential 全程不换页；测试绿。

### F5　日志/事件加载更多（游标分页）
- 优先级 P0｜依赖 无｜涉及 `LogsSecurity.tsx:700`（`.slice(0,80)` 硬截断）、复用 `get_analytics_request_page` 范式
- **RED**（`LogsSecurity.pagination.test.tsx`）：`logs_show_load_more_when_truncated()`、`load_more_appends_next_page()`。
- **GREEN**：接游标分页 command + "加载更多"按钮。
- **DoD**：可翻页看全；测试绿。

### F6　长列表虚拟化
- 优先级 P1｜依赖 F5、F3｜涉及 `LogsSecurity`/`SmartLearning`/`Doctor` 列表、`@tanstack/react-virtual`（route-lazy）
- **RED**（`virtualList.test.tsx`）：`renders_only_visible_rows_for_10k_fixture()`——1万行 fixture 下 DOM 节点数 < 100。
- **GREEN**：引入 `@tanstack/react-virtual`（懒加载 chunk，守住 160KiB entry 门）。
- **DoD**：虚拟化测试绿；`npm run build` entry gzip 不越 160KiB；`check:bundle` 通过。

---

## 6. Epic R — Release 实测与签名（波4，依赖前三波定稿）

### R1　macOS release 性能实测闭环
- 优先级 P0｜依赖 全部功能定稿同一候选 commit｜涉及 `support/scripts` 性能采集、`docs/history/`
- 目标：PRD P0.2.2 / PLAN §1B.7 的冷/暖启动 p50/p95、完整进程组 CPU/RSS/IPC、WebKit Long Task、30 分钟 soak（当前只有 dirty/ad-hoc 部分数据，UX HIGH-4）。
- **RED**：性能门脚本 `perf_gate.py` 断言各指标达 PRD 预算（冷启动 p95≤1.5s、切页 p95≤250ms、空闲 CPU≤1%、内存漂移<5%）；未达标 exit≠0。
- **GREEN**：同一候选 commit 的 macOS release `.app` 采集 + 1万日志/2万 session stress fixture 渲染帧率。
- **DoD**：全部预算达标并归档；达标前对外文案不含"流畅/低占用/高性能"（PRD §19 #40）。

### R2　Windows 平台基线
- 优先级 P0｜依赖 R1 方法｜涉及 Windows runner
- **DoD**：Windows 冷启动/切页/CPU/RSS + UTF-8/路径/Credential Manager/`.cmd` shim + E2E 基线证据。

### R3　签名 / 公证 / NSIS 实机
- 优先级 P0｜依赖 R1/R2｜涉及 `.github/workflows/macos-release.yml`、`windows-release.yml`
- **DoD**：macOS Developer ID + notarization + staple + Gatekeeper 实机通过；Windows Authenticode + NSIS 静默安装/启动/卸载 + SHA-256 manifest；同一候选 commit 绑定 provenance。

---

## 7. 执行波次与并行图

```text
波1（加固+前端治理起步，三线并行）
  后端: G1 ‖ G2 ‖ G3 ‖ G4
  前端: F1 → (F2 ‖ F3)   ‖   F4 ‖ F5
        └── F1 是 F2/F3 前置

波2（§8 能力补齐；M3/M4/M5/M6/M7 相互独立可并行，M1→M2 串行协调）
  M1 → M2        （同改请求主流程，串行避免冲突）
  M3 ‖ M4 ‖ M5 ‖ M6 ‖ M7    （依赖 G2/G1，彼此独立）
  前端: F6（依赖 F5+F3）

波3（实证）
  P1（依赖 M7 + G1）

波4（发布证据，依赖前三波定稿同一 commit）
  R1 → R2 → R3
```

---

## 8. 甲路线发布门槛（全局 DoD，对齐 PRD §19 / PLAN G0-G8）

- [ ] G0-G8 全绿：无 fixture 默认、能力矩阵零占位/缺失/不安全默认（PLAN §1C.2）。
- [ ] §8 能力矩阵逐项 real：memory(M1)/output(M2)/MCP(M3)/Code-Rust(M4)/text(M5)/RTK(M6)/chain_bridge(M7) 各有 runtime+测试+（可用时）eval 证据。
- [ ] property/fuzz/fault 门（G2）通过，解锁 Optical/Code active 前置。
- [ ] Provider 五类 route 各有真实 routed Ledger 证据（P1）。
- [ ] 前端：核心 payload 类型化、超大文件 ≤550 行、selector 无关不重渲染、长列表可翻页/虚拟化、needs-credential 不换页。
- [ ] Release：macOS/Windows 同候选 commit 性能预算达标 + 签名/公证/NSIS 实机 + SHA-256 manifest。
- [ ] 全局验证命令（§0.1）全绿；`cargo clippy -D warnings`、`tsc --noEmit`、`npm run build`、E2E 通过。
- [ ] PRD/PLAN/STATUS 三处口径与代码事实一致（甲路线不缩小承诺，但已实现即如实标 real）。

---

## 附：与三份子报告的对应关系
- 后端 C-1→M1、C-2→G1、C-3→G2、H-1→M3、H-2→M4、H-3→M6、H-4→P1、M-1/L-4→G3、M-2→G4、M-6/§6.3→M7。
- 前端 H-1→F2、H-2→F1、H-3→F3。
- UX/性能 HIGH-1→F5/F6、HIGH-2→F4、HIGH-3→F3、HIGH-4→R1、MEDIUM-3→G1、发布 Gate→R1/R2/R3。
