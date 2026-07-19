# d-token 后端各层能力发布前审查报告

审查日期：2026-07-16
审查范围：`code/crates/`（17 个业务 crate）+ `code/apps/desktop/src-tauri/src/`（Tauri native 桥接层）
审查方法：以源码为唯一事实源，逐层核对 PRD §8 / §0.0.1 / §0.0.2 / §15、PLAN §1C 与 IMPLEMENTATION_STATUS.md 的宣称；所有论断附 file:line 证据。
排除：`code/runtimes/headroom/`（上游 Python 参考）、`code/UI design for project/`、`code/packages/sdk/`、`support/` 下 eval/fixture。

---

## 1. 执行摘要

1. **后端工程质量整体高于文档给人的印象，但 Headroom §8 能力迁移的“广度”确实不足**：安全边界、事务/回退、进程身份、CCR 加密、类型化发送管线属于**可发布级别**的严谨实现；而 memory/SharedContext、MCP、output shaping、大部分 codec、integrated RTK 仍是**占位或仅存储层**，与 PRD §8「全能力迁移是发布门槛」的要求有实质差距。
2. **后端发布就绪度评分：62/100（内测候选偏下沿）**。依据：核心请求链路（路由→压缩子集→CCR 门控→发送→Ledger）真实且 fail-closed（+）；安全/隐私边界几乎无违反点（+）；但 §8 六大能力占位、无 property/fuzz/fault 测试、`main.rs` 13k 行单文件承载全部安全关键逻辑、`dtoken-process` crate 为死桩、缺 Provider 矩阵实证（−）。
3. **Headroom §8 迁移完成度估计：约 45%–50%**。真实：proxy/协议/显式路由、Ledger/stats/metrics/export、CCR 存储与门控、CacheAligner 通用 IR、ContentRouter、JSON/log/repeat 安全 codec、Doctor 域合同。占位/缺失：MCP、memory/SharedContext、output shaping、Code/Image codec active、Kompress 通用文本、integrated RTK 内部能力、Library/SDK。
4. **CRITICAL 问题：3 个；HIGH：7 个；MEDIUM：8 个；LOW：4 个。**
5. 三个最关键发现：
   - **（正面）安全与 fail-closed 是这套后端最强的地方**：loopback 强制绑定、凭据只走 OS keystore 且无明文回退、CCR AES-256-GCM + keychain 密钥 + 默认关闭、停服前 6 层进程身份验证（PID/exe/birth/port/health nonce）、发送前 CCR durable receipt 门控——这些都真实存在且有测试。
   - **（阻断）`dtoken-context-store` 有完整 memory schema（v4）但 `dtoken-core-api` 从不调用**：memory/SharedContext/output shaping 在 HTTP 面全是 metadata-only，存储层与运行时完全脱节（`upsert_memory` 等仅被自身 + 自身测试引用）。这是「文档结构宣称 vs 代码事实」最大的一处背离。
   - **（阻断/架构）`main.rs` 13,007 行承载全部服务生命周期、进程身份、Doctor executor、Agent 事务、bridge 验证等安全关键逻辑，而独立的 `dtoken-process` crate 是 M1 死桩**（`start_placeholder` 只打日志、`stop` 无身份校验）。PLAN §1C.0 规定的单向分层在 Tauri 层被打破。
6. 报告文件：`docs/research/BACKEND_CAPABILITIES_REVIEW_2026-07-16.md`

---

## 2. 现状全景

### 2.1 Crate 地图与规模

| Crate | 行数 | 定位 | 能力状态 |
|---|---|---|---|
| dtoken-types | 2123 | 稳定契约（RequestScope/Capability/Usage/RouteDecision…） | 真实 |
| dtoken-config | 323 | 配置模型与 provider 查找 | 真实 |
| dtoken-router | 56 | 显式路由决策（instance+protocol 精确匹配，歧义即拒） | 真实（薄） |
| dtoken-compress | 14801 | Context IR/CacheAligner/ContentRouter/codecs/Optical | 部分 |
| dtoken-context-store | 1222 | SQLite v4：capability/request/CCR-meta/provider-cache/**memory** | 部分（memory 未接入） |
| dtoken-process | 334 | 进程管理 | **死桩（M1）** |
| dtoken-doctor | 1182 | DoctorSnapshotV3 + 16 模块 + plan/receipt 合同 | 真实（合同层） |
| dtoken-event-ledger | 487 | JSONL append + 崩溃修复 + 内存 ledger | 真实 |
| dtoken-preprocessor | 366 | integrated RTK adapter | 部分（仅外部检测） |
| dtoken-headroom-adapter | 371 | Headroom 检测/mock | 占位（fixture/native-unavailable） |
| dtoken-project-policy | 119 | 压缩等级推荐引擎 | 真实（小） |
| dtoken-runtime-pipeline | 290 | type-state 发送链（私有构造） | 真实 |
| dtoken-learning | 1598 | 学习缓存/索引/评分 SQLite | 部分 |
| dtoken-config-patcher | 4356 | 事务：diff/backup/verify/rollback + Agent 多文件事务 | 真实 |
| dtoken-agent-bridge | 369 | metadata-only sidecar（identity/hook/mcp 子命令） | 部分（bridge 本体薄） |
| dtoken-core-api | 12080 | loopback transport + 协议 + provider + 公共投影 | 部分 |
| dtoken-proxy | 211 | 二进制入口，强制 loopback 绑定 | 真实 |
| **d-token-desktop (main.rs)** | 13007 | Tauri native command + 服务生命周期 + 进程身份 + Doctor executor | 真实但过载 |

**缺失 crate：`dtoken-mcp`**。PLAN §1C.0（第 824 行）在依赖流中列出 `dtoken-mcp  stdio JSON-RPC transport（新增）`，但仓库中不存在该 crate，`/headroom/mcp` 端点 fail-closed 返回 `mcp_transport_not_implemented`（core-api lib.rs:7951）。

### 2.2 依赖关系（实测单向，无循环）

```
dtoken-types  ←  (所有 crate)
dtoken-config ← router, core-api, proxy
dtoken-compress ← config-patcher, core-api
dtoken-config-patcher ← context-store, core-api, (main.rs)
dtoken-context-store ← core-api
dtoken-runtime-pipeline ← core-api
dtoken-{doctor,event-ledger,headroom-adapter,project-policy,preprocessor} ← core-api
dtoken-core-api ← proxy, (main.rs)
dtoken-{agent-bridge,learning,process} ← (main.rs) 独立
```

- **无循环依赖**（cargo 依赖图为 DAG）。types 为根，core-api 为聚合点，proxy/main.rs 为入口——方向正确。
- **一处分层异味**：`dtoken-context-store` 依赖 `dtoken-config-patcher`（context-store/src/lib.rs:5 `use dtoken_config_patcher::agent::{AgentDiscoverySnapshot, DetectedAgent}`）。一个「存储投影」crate 反向依赖「配置写入」crate，只为复用 Agent 类型。应把这些类型下沉到 `dtoken-types`。

### 2.3 生产主链路（真实执行路径）

```
AI agent → 127.0.0.1:18001 (dtoken-proxy, 强制 loopback)
→ dtoken-core-api handler (anthropic_messages / chat / responses)
→ route 解析 (anthropic_route_payload，无匹配即 fail-closed)
→ Hybrid 预处理 (prepare_direct_anthropic_hybrid, 仅 Anthropic 同协议)
→ record_hybrid_pre_send_stages（失败→baseline fallback）
→ CCR prepare (prepare_ccr_for_request，durable receipt 门控)
→ runtime-pipeline (apply_routed_pipeline_payload → ReadyToSend)
→ forward_anthropic_messages / stream（凭据从 OS keystore，状态码分类透传）
→ Event Ledger / RequestLog / metrics（脱敏，raw 不落盘）
```

### 2.4 各 crate 能力状态表

| 能力（对照 Headroom §8） | 状态 | 关键证据 |
|---|---|---|
| Local proxy + OpenAI/Anthropic 协议 + 显式路由 | **真实（子集）** | proxy main.rs:39-60 loopback；core-api forward_* + classify_upstream_status:3114 |
| 无静默 fallback + 401/403/429/5xx 分类 | **真实** | classify_upstream_status:3116；`fallback_disabled`；route 缺失 fail-closed 测试:9481/9499 |
| ContentRouter | **真实** | content_router.rs：secret 预检/risk/exact-anchor/size/deadline budget/内容分类 |
| JSON minify codec | **真实** | 测试 json_minify_v1 + semantic_closed_tool_result:363-407 |
| Log/repeat/search/build/diff/text/html fold | **部分真实** | compress lib.rs:167 repeat_line_fold/log_prefix_normalize；Diff→Passthrough(content_router.rs:119) |
| CodeCompressor | **占位（EvaluationOnly）** | code_codec.rs：仅 Rust 有 syn@2 parser 但 `EvaluationOnly`+`macro-token-stringification-not-proven`，其余 7 语言 `LanguageNotImplemented`，production 全 passthrough |
| Kompress 通用文本压缩 | **占位** | 通用 `compress()` lib.rs:26 纯透传（tokens_saved:0） |
| CacheAligner / live-zone | **部分真实** | hybrid.rs FrozenPrefix/TransformableWindow/ProtectedTail + route/provider/model-bound cache key |
| Provider cache observation | **部分真实** | context-store provider_cache_observation:305；仅 explicit policy + fresh observation 才生成 evidence |
| CCR 可逆压缩 + retrieval | **真实（存储/门控），部分（生命周期）** | ccr_store.rs AES-256-GCM/nonce/AAD/TTL/quota/perm；发送前 durable receipt 门控 anthropic_messages:7271 |
| memory / per-project / cross-agent / SharedContext | **占位（存储层孤立）** | context-store schema v4 memory_entry/share_grant 齐全，但 core-api **零调用**；`/memory/*` 全 metadata-only(lib.rs:6085) |
| learn / Smart Learning | **部分真实** | dtoken-learning SQLite 缓存/checkpoint；learning_discovery.rs 扫描；失败挖掘/结构化委派未闭环 |
| output shaper / verbosity / effort | **占位** | `/output-shape` 明确 "not yet applied to outbound requests"(lib.rs:6122) |
| MCP server/tools | **占位（无 transport）** | headroom_mcp:7925 返回 unavailable；无 dtoken-mcp crate |
| Library/SDK/middleware | **缺失** | 无对外 Rust API 层；packages/sdk 空 |
| Agent wrap/unwrap | **部分真实** | OpenCode 事务真实；config-patcher agent_integration 多文件事务真实；三 Agent routed 未验收 |
| integrated RTK | **部分（仅外部检测）** | preprocessor：native 模式 unavailable，无 bundled RTK/command rewrite/tee/hook |
| durable ledger/metrics/stats/export/cost | **大部分真实** | event-ledger JSONL + 崩溃修复；core-api stats/history/metrics/analytics |
| Doctor | **真实（合同 + 部分 executor）** | doctor 16 模块；仅 core_service_lifecycle + Agent Integration 有真实 executor，其余锁定 |
| Optical code/context → PNG | **部分实验（默认关闭）** | optical/ 完整链；capability.rs:432 默认无 active-capable model；engine.rs:480 gate on allow_active |

---

## 3. 分级问题清单

### CRITICAL（发布阻断）

#### C-1　memory / SharedContext / output shaping 存储层与运行时完全脱节
- 证据：`dtoken-context-store/src/lib.rs` 有完整 memory schema v4（`memory_entry`:1146、`memory_share_grant`:1172、`upsert_memory`:408、`list_visible_memory`:566、`grant_memory_share`:487、`tombstone_memory`:595、`export_project_memory`:650），但全workspace `rg` 显示这些函数**仅被 context-store 自身和 `tests/migrations.rs` 引用**，`dtoken-core-api` 零调用。`/memory/summary`、`/memory/learn`、`/output-shape` 全部从 `recent_memory_records()`（JSONL metadata 读取器）派生，且注释明写 "Memory/learn metadata only"（lib.rs:6085）、"Output shaping is not yet applied to outbound requests"（lib.rs:6122）。
- 影响：PRD §8「Cross-agent memory、Per-project memory、SharedContext、output shaper」列为发布门槛能力；PLAN §1C.0 把 `dtoken-context-store` 列为生产依赖流中「memory/cache state」owner。当前是「盖了房子没通电」——schema 存在造成「已实现」的错觉，但产品链路里 memory 不生效。
- 根因：B3 切片（CCR/memory/learn/output）只完成了 CCR，memory/output 停在存储层，未接入 request lifecycle。
- 建议：要么（a）在 B3 完成 memory 检索接入 `anthropic_messages`/`chat` 的 prepare 阶段 + output shaping 接入发送前改写并计入 accounting；要么（b）诚实降级——从 PLAN/STATUS 明确标注 memory/output 为「后续版本」，并从 §8 发布门槛移出（需改 PRD）。不能以「schema 已建」当作迁移完成。

#### C-2　`main.rs`（13,007 行）承载全部安全关键逻辑，`dtoken-process` crate 为死桩
- 证据：`dtoken-process/src/lib.rs` 的 `ProcessManager::start_placeholder`（:236）只 `tracing::info!("M1 placeholder start requested")`，`stop`（:241）直接 `child.kill()` **无任何身份校验**。真正的服务生命周期、进程身份验证（`process_birth_token`:3102、`executable_identity_matches`:3193、`listener_owned_by_pid`:3211、`stop_dtoken_service_impl`:7105 六层校验）、Doctor executor、Agent 事务、bridge 验证全在 `main.rs`。command 数 75 个，`async fn` 仅 12 个——大量文件/进程/HTTP 阻塞逻辑集中在同步 command。
- 影响：（1）安全关键路径难以单元测试、难以并发审查、难以定位慢调用（PLAN §1B.2 第 8 条自陈）；（2）`dtoken-process` 死桩误导读者以为进程管理由该 crate 负责；（3）违反 PLAN §1C.0「Tauri 不承载业务规则」的单向分层。
- 根因：M1→M4 演进中安全逻辑就地堆在 main.rs，未回迁到领域 crate。
- 建议：将 `process_birth_token`/`executable_identity_matches`/`listener_owned_by_pid`/`terminate_process`/manifest 逻辑迁入 `dtoken-process`（替换死桩）并补单测；服务生命周期状态机独立成模块。这是发布前必须做的可维护性 + 可测性收敛（PLAN §1A.8 已提出窄范围拆分但尚未对 process/service 执行）。

#### C-3　压缩/协议核心零 property/fuzz/fault-injection 测试，与 G2/G3 门禁要求直接冲突
- 证据：全 workspace `rg proptest|quickcheck|arbitrary|fuzz_target` 无有效命中（5 个「命中」实为 `.expect("...for nion")` 字符串）；`proptest` 不在任何 `Cargo.toml`。467 个测试全部是 example-based 单元/集成测试。
- 影响：PLAN §1C.2 G2 明确要求「live-zone/CacheAligner/Router/全部 codec/validator 的 fixture/**property/fuzz/fault**/quality 门通过」，G3 要求 CCR/store 的 crash recovery/fault 测试。压缩是有损改写协议 body 的路径，缺 property/fuzz 意味着 malformed/adversarial/并发输入下的安全性未被系统性验证——正是「active send path 不可达」门禁最需要的证据。
- 根因：测试策略停留在举例覆盖，未引入生成式测试。
- 建议：至少对 ContentRouter、JSON minify、hybrid IR lowering、CCR envelope 编解码引入 proptest（不变量：压缩后语义等价、frozen prefix byte-identical、secret/exact 永不进入变换、round-trip 恒等）；对 renderer/patcher 引入 malformed corpus。这是 Optical/Code active 解锁的前置。

### HIGH

#### H-1　MCP 完全缺失（无 crate、无 transport、无 handler）
- 证据：`headroom_mcp`（lib.rs:7925）返回 `state: unavailable`、reason "MCP transport and tool handlers have not been implemented"（:7951）；无 `dtoken-mcp` crate（PLAN §1C.0:824 列出但不存在）。
- 影响：PRD §8 列 MCP server 为发布门槛；G4「无 transport 时 MCP 不得 configured」当前靠返回 unavailable 满足（诚实），但能力本身缺失。
- 说明：这是**诚实的占位**（fail-closed，有测试 lib.rs:11083 强制），不是「假装已实现」。但 §8 完成度上必须计为缺失。
- 建议：新建 `dtoken-mcp` crate 实现 stdio JSON-RPC + `compress`/`retrieve`/`stats` 真实 handler，或改 PRD 将 MCP 降为后续版本。

#### H-2　CodeCompressor 与通用文本压缩在生产为 passthrough，"code 压缩"能力名不副实
- 证据：`code_codec.rs` 仅 Rust 有 parser 且 `EvaluationOnly`（decision_for:170 返回 `reason_code:"implementation-not-active"`），其余 7 语言 `LanguageNotImplemented`（:198）；通用 `CompressionEngine::compress`（lib.rs:26）纯透传。
- 影响：PRD §8「CodeCompressor、Kompress/text compressor」列为门槛。当前只有「历史闭合 tool_result 内的 JSON/log/repeat」子集真实压缩，代码块本身不压缩。
- 说明：honest（EvaluationOnly + 明确 macro 风险注释），不是欺骗，但完成度低。
- 建议：完成 ≥1 语言的 AST 安全 transform 全链（parser/eval/compile/test fixture）再宣称 Code 可用；否则 STATUS 保持 passthrough 口径。

#### H-3　integrated RTK 仅外部检测，无 bundled/reimplemented 内部能力
- 证据：`dtoken-preprocessor` native 模式返回 `native_unavailable_status()`（"integrated RTK preprocessing is not available in the native runtime yet"）；无 command rewrite/tee/hook integrity/uninstall 实现，`profile_from_status` 只产出 `RtkExternal`（:153）。
- 影响：PRD §0.4/§8 把 integrated RTK 作为 runtime 内部能力门槛；PLAN §1C 标其为「不安全默认/部分」。
- 说明：release（Native）下 fail-closed 为 unavailable（`RuntimeMode::from_profile(_, cfg!(debug_assertions))`，release→Native），**并非**「release 落 fixture」的不安全默认——这一点比 PLAN 的担忧要好。但能力本身缺失。
- 建议：明确 RTK 内部能力为后续版本（改 PRD），或实现 bundled provenance + command rewrite/filter/tee。

#### H-4　Provider/Agent 矩阵仅 OpenCode 单点验收
- 证据：STATUS.md 第 85 行 + 代码事实——`dtoken-process` opencode_command_plan 专用；Claude/Codex/Cursor/Cline 只有 config-patcher 事务代码但「隔离目录验证」，无真实 routed request 证据。
- 影响：PRD §8.1 要求多 Agent 覆盖；§8.2 五类 route（official/custom_openai/custom_anthropic/cc_switch/chain_bridge）仅 fixture 覆盖，无跨 provider 实证。
- 建议：至少补 custom_openai + custom_anthropic + cc_switch 三类的真实 routed evidence（Ledger 可重建），否则路由能力停在「有代码、缺实证」。

#### H-5　Optical/Hybrid 巨大实现面（~14.8k 行 compress）默认全部关闭，投入产出比风险
- 证据：`optical/` 30 文件、renderer/patcher/capability/economics/eval 齐全，但 capability.rs:432「默认无 model active-capable」、engine.rs:480 gate on `allow_active`，无仓库内置可信模型、无真实 Provider fidelity/cost/latency eval。
- 影响：这是后端最大的单块投入，但对内测发布**零用户可见收益**（active 永远关闭，Beta 页只做 shadow/metadata 观测）。同时它拉高了 compress crate 的复杂度与审查成本。
- 建议：内测发布应将 Optical 明确定位为「实验轨道，默认关闭」（PRD 已如此），不要让它阻塞主线；把 property/fuzz 测试预算优先投给**会真实执行**的 JSON/log/CCR/route 路径而非 Optical active。

#### H-6　`estimate_tokens` 为粗略启发式，非真实 tokenizer——所有"节省"均为估算
- 证据：`compress/src/lib.rs:287` `estimate_text_tokens` = `split_whitespace().count().max(chars/4)`。所有 `original_tokens`/`tokens_saved`/`savings_percent` 基于此。
- 影响：与 PRD §10「统一 denominator」和 §0.0.2(9)「estimator 不得标为已验证账单节省」一致（前端须标"估算"），但后端若未来要 signed delta / 真实 accounting，`char/4` 精度不足。真实 macOS 候选已正确用 `claimable=0` fail-closed（STATUS:142）。
- 建议：保持「估算」口径即可满足内测；若要 per-model 精确计量需接入真实 tokenizer（后续版本）。

#### H-7　`dtoken-headroom-adapter` 仍是 M1 检测/mock 层，`mock`/`fixture` 方法留在生产 crate
- 证据：`headroom-adapter/src/lib.rs` `cache_summary_mock()`（:99）、`stats_history_json` 返回 `"fixture_missing"`（:169）、`headroom_cache_summary_mock()`（:208）。native 模式 fail-closed 为 unavailable（:33）。
- 影响：release 下不返回 mock（Native→unavailable），故非「不安全默认」；但 `_mock` 公共方法留在生产 crate 是清理欠账，易被误接。
- 建议：将 mock/fixture 方法移入 `#[cfg(test)]` 或 support harness；该 crate 若无真实 Headroom 能力承载，考虑合并进 preprocessor/删除。

### MEDIUM

- **M-1**　`dtoken-context-store` 反向依赖 `dtoken-config-patcher`（lib.rs:5）——存储层依赖配置写入层的类型。应将 `AgentDiscoverySnapshot`/`DetectedAgent` 下沉 `dtoken-types`。（分层健康）
- **M-2**　两个 `invoke_handler` 列表（main.rs:10428 debug / 10517 release）手工维护、几乎重复，仅差 4 个测试命令。易漂移。建议用宏或 `cfg!` 拼接共享基表。（可维护性；但 release 正确排除了 test_opencode/workload/smoke，门控本身正确）
- **M-3**　服务启动含 `std::thread::sleep(100ms)` × 最多 40 次的同步健康轮询（main.rs:6941-6956），在同步 command 线程执行。对 UI 有阻塞风险（PLAN §1B.4 冷启动预算）。建议移入 spawn_blocking 或异步。
- **M-4**　`terminate_process` 非 Windows 用 `kill <pid>`（SIGTERM，main.rs:3261）无超时/SIGKILL 升级；若目标忽略 SIGTERM 则停服静默不完全（调用点 `let _ =` 忽略结果，:7212 有检查但升级缺失）。
- **M-5**　Doctor 仅 2 类模块有真实 executor（core_service_lifecycle + Agent Integration Instructions/Hook/Skill/MCP）；Route reload、listener TTL、Agent transport、Optical Shadow、autostart、cc-switch sync、recovery 复检 7+ 模块锁定（main.rs:8448-8456 dispatch 只映射到这两类）。PRD §P0.9 列 16 模块，实机可修复面偏窄。诚实但需在 STATUS 明示。
- **M-6**　`dtoken-router`（56 行）是最小实现：仅按 instance_id+input_protocol 精确匹配，歧义即拒（正确）。但 chain_mode（compress_then_cc_switch/chain_bridge）的真实二次转发逻辑不在此 crate——PRD §5.2 的 cc-switch 二次转发能力实现位置需进一步验证（本轮未见 core-api 中 cc_switch 二跳转发实测）。
- **M-7**　`dtoken-learning` crate 提供 cache/index/scoring，但真实扫描 worker 在 `learning_discovery.rs`（3747 行 Tauri）——学习逻辑再次跨 crate/main 分裂，与 C-2 同类问题（业务逻辑漏进 Tauri 层）。
- **M-8**　`dtoken-compress` 单 crate 14.8k 行、30 文件，其中 optical 占 ~11k。虽已按文件拆分，但 hybrid.rs(1155)、content_router.rs(816)、optical/engine.rs(1462)、optical/inventory.rs(1001) 仍偏大，接近 800 行软上限的多个文件。

### LOW

- **L-1**　多处 `CapabilityEvidence::new(...).expect("validated ... must be valid")`（如 core-api:665）——静态有效构造，可接受，但 `.expect` 在生产代码中建议改为 `unwrap_or_else` 记 warn 或编译期常量。
- **L-2**　`candidate_proxy_paths()`（main.rs:3275）/`candidate_agent_bridge_paths()` 硬编码大量相对路径候选（含 `code/apps/...`、`target/release`），生产 bundle 中多数无效，属开发便利残留。
- **L-3**　`event-ledger` 内存版 `EventLedger::mock_m1()`（:33）留在生产 crate；建议移入测试。
- **L-4**　`now_ms()`/`unix_seconds()` 在多个 crate 各自实现（context-store、ccr_store、event-ledger…），可提取 `dtoken-types` 共用。

---

## 4. 修改计划（对齐 PLAN B0–B6 / G0–G8，按依赖排序）

**阶段 A：可维护性 + 可测性收敛（不改能力，降风险）— 对应 B0/B5**
- A1（C-2）将进程身份/服务生命周期从 main.rs 迁入 `dtoken-process`，替换死桩，补单测。DoD：`dtoken-process` 提供 `verify_service_identity`/`stop_verified`，main.rs 只调用；6 层校验有单测。涉及：dtoken-process、main.rs。
- A2（M-1/L-4）将 Agent 类型与时间工具下沉 `dtoken-types`，解除 context-store→config-patcher 反向依赖。DoD：cargo 依赖图 store 不再依赖 patcher。
- A3（M-2）invoke_handler 共享基表 + cfg 测试命令。DoD：单一命令来源，release 仍排除 4 测试命令。

**阶段 B：测试门禁补齐 — 对应 G2/G3（C-3 前置）**
- B1（C-3）为 ContentRouter/JSON minify/hybrid lowering/CCR envelope 引入 proptest 不变量 + malformed corpus + CCR crash-recovery fault 测试。DoD：G2/G3 property/fuzz/fault 门有实证。涉及：dtoken-compress、dtoken-context-store、ccr_store。

**阶段 C：§8 能力二选一决策（C-1/H-1/H-2/H-3）**
- 路线甲（补齐）：C1 memory 检索接入 request prepare + output shaping 接入发送改写并计 accounting；C2 新建 dtoken-mcp（stdio + 3 handler）；C3 完成 ≥1 语言 Code AST 全链；C4 integrated RTK 内部能力。DoD：逐能力 runtime+eval+Agent 证据（G2/G4/G5）。
- 路线乙（诚实降级）：改 PRD §8，将 memory/MCP/Code/RTK/output 明确移入「后续版本」，内测发布只承诺 proxy/route/压缩子集/CCR/Ledger/Doctor/Agent 接入。DoD：PRD/PLAN/STATUS 三处口径一致，发布阻断清单据实缩小。
- **建议先做路线乙的诚实盘点**，再按资源决定甲的范围——避免为凑"全能力"而降低已实现部分的质量。

**阶段 D：Provider 矩阵实证（H-4）— 对应 G7**
- D1 用真实/免费 upstream 补 custom_openai + custom_anthropic + cc_switch 三类 routed evidence。DoD：Ledger 可重建三类链路，无静默 fallback。

**阶段 E：运行细节（M-3/M-4/M-8/H-5）**
- E1 服务启动健康轮询移出同步线程；E2 terminate 加 SIGTERM→SIGKILL 超时升级；E3 拆分 optical/engine.rs、hybrid.rs 超大文件；E4 Optical 保持默认关闭、不阻塞主线。

---

## 5. 最佳迭代方案：内测发布的"最小必要后端"

**必须真实（已基本满足，重点是补测试与实证）：**
1. loopback proxy + OpenAI/Anthropic 协议 + 显式路由 + 错误分类 + 无静默 fallback（✅ 真实，补 D1 三类 provider 实证）。
2. 安全边界：OS keystore 凭据、CCR 加密默认关闭、进程身份验证、脱敏 Ledger（✅ 真实，A1 迁 crate + 补测试）。
3. 事务/回退：config-patcher + Agent 多文件事务 + Doctor 两类 executor（✅ 真实）。
4. 压缩子集：ContentRouter + JSON/log/repeat 安全 codec + CCR 门控发送（✅ 真实，B1 补 property/fuzz）。
5. Ledger/stats/metrics/export（✅ 真实）。

**诚实标记为后续版本（避免充数拉低质量）：**
- memory/SharedContext（C-1）、MCP（H-1）、Code/Image codec active（H-2）、通用文本压缩、output shaping、integrated RTK 内部能力（H-3）、Library/SDK、Optical production active（H-5，保持实验轨道）。

**并行机会（无依赖，可同时推进）：**
- 阶段 A（可维护性收敛）‖ 阶段 B（测试门禁）‖ 路线乙的 PRD/STATUS 诚实盘点——三者互不依赖，可并行。
- D1（Provider 实证）依赖 A1 稳定，其余独立。

**取舍建议：** 后端的**深度质量已达内测水准**（安全、事务、CCR、发送管线严谨），短板是**广度（§8 覆盖）**与**测试形态（缺 property/fuzz）**。最优路径不是继续堆 §8 占位能力，而是：(1) 用路线乙把发布承诺收敛到已真实的能力；(2) 用阶段 A/B 把「已实现部分」加固到可发布可测；(3) §8 缺口作为公开的已知限制随后续版本迭代。这与 PRD §P0.2(15)「未完成能力计入发布阻断，不能以'接口已存在'视为完成」一致——但「计入阻断」可以通过**缩小承诺范围**而非**假装完成**来解决。

---

## 6. 附录

### 6.1 审查方法
- 以 `rg`/`wc`/依赖图静态分析定位，Read 精读安全关键与代表性文件（runtime-pipeline、context-store、ccr_store、content_router、code_codec、compress lib、stop_dtoken_service_impl、resolve_provider_credential、verify_dtoken_agent_bridge、doctor apply）。
- 对每处「文档宣称已实现」的能力，回代码核实是否存在真实执行路径 vs 占位；对 memory/MCP/output/RTK 逐一确认调用方。

### 6.2 覆盖范围
- 17 业务 crate 全部至少做依赖 + 结构 + 关键函数核查；core-api（12k）、main.rs（13k）、config-patcher（4.3k）做重点段落精读。
- 安全维度逐条核对 PRD §15：loopback（✅ proxy:55）、凭据引用/keystore（✅ :3022/3043）、脱敏（✅ process:84/event 测试:316）、无静默 fallback（✅ :3116）、进程身份（✅ :7105）、CCR 隔离默认关闭（✅ ccr_store + :1391）、raw 不落盘（✅ 测试断言）。

### 6.3 需进一步验证项（本轮未完全展开）
1. **cc-switch 二次转发（chain_bridge / compress_then_cc_switch）的真实执行路径**：dtoken-router 只做单跳决策，core-api 中二跳转发实测未定位到，需确认 PRD §5.2 是否有真实实现或仅 route 定义（建议下一轮聚焦 core-api 中 chain_mode 分支）。
2. **`verify_platform_before_agent_bridge_execution` 在 debug build 是否也强制签名**：macOS/Windows 实现真实，Linux fallback 为 no-op `Ok(())`；需确认非目标平台不会成为绕过点（当前 Linux 非目标平台，风险低）。
3. **Provider cache observation → cache evidence → resolved plan 的端到端正确性**：存储与读取真实，但「explicit policy + fresh observation 才生成 evidence」的完整因果链需 provider 实测（G2 部分）。
4. **learning_discovery.rs（3747 行 Tauri）** 的扫描 metadata 白名单/取消/并发上限本轮未逐行核（属前端桥接层深度，可并入学习专项）。
5. **stream（SSE）路径的终态 Ledger 与 CCR finalize**：forward_*_stream 存在，StreamTerminalLedgerGuard 存在，但 stream 下 CCR finalize/delete 全链证据 STATUS 自陈未闭环，需专项。
