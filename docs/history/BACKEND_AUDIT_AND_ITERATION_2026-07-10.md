# d-token 后端多维审查与迭代建议

> 审查日期：2026-07-10
> 审查对象：当前工作树（分支 `optical`，审查时 HEAD `c914ad1`）
> 产品依据：`docs/PRD.md`
> 结论级别：**发布阻断，不可作为稳定 Beta / 候选发布版交付**
> 迭代复核：2026-07-10 后续已完成一批 P0 修复；原始问题保留用于追溯，当前状态以“迭代复核”小节为准。

## 0. 迭代复核（2026-07-10，当前工作树）

### 0.1 已修复并验证

| 原问题 | 当前状态 | 当前证据 |
|---|---|---|
| BE-001 workspace 无法编译 | **已修复** | `cargo test --workspace`：115 passed；`cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings` 均通过 |
| BE-002 无 Provider 伪成功 | **已修复** | Chat / Responses / Anthropic 无显式 Route/Provider 时 503 + `dtoken_route_not_configured`；生产请求不再生成 `d-token-ok` |
| BE-003 非 loopback 警告后继续 | **已修复** | `dtoken-proxy::parse_loopback_host` 仅接受 `127.0.0.1`、`::1`，拒绝 `0.0.0.0`、局域网、公网与非法地址 |
| BE-004 生产 mock 安装 API | **已修复** | `/install/scan`、`/install/plan` 已从正式 Router 移除，路由回归测试为 404 |
| BE-101 Provider timeout 未使用 | **已修复基础项** | 普通与流式转发均使用 `ProviderProfile.timeout_ms`；超时返回 504 / `provider_timeout` |
| 上游错误仍返回 200 / Ledger 假成功 | **已修复** | 401/403/4xx/5xx/网络/无效响应保留或映射正确 HTTP 状态；RequestLog、Route Event、ProviderForward Event 同步 Failed |
| transform 指标重复累计 | **已修复** | 多 transform 对 tokens_saved / tokens_before 做守恒归因，新增回归测试 |
| 前端开发依赖 critical 漏洞 | **已修复** | Vitest 升级至 3.2.6；完整 `npm audit` 为 0 vulnerabilities |

### 0.2 新增真实链路证据

2026-07-10 已完成一次**真实 OpenCode routed test**（未测试 Codex）：

```text
OpenCode 原配置
→ d-token :18001
→ d-token-owned Headroom-derived runtime
→ route=opencode-real-routed
→ provider=opencode-zen
→ 真实上游返回预期 marker
```

Dashboard 来源为 `dtoken-owned-runtime`，本次记录 saved_tokens=474，Event/Route/RequestLog/CCR JSONL 均落盘，CCR `raw_available=false`。脱敏证据见 `support/artifacts/opencode-real-routed-2026-07-10.json`。

### 0.3 仍然阻断发布的后端问题

1. **BE-005 凭据安全存储仍未闭环**：运行时元数据/API 已不再返回 `api_key_ref`，桌面端也只接受 `os-credential://` / `wincred://` / `keychain://` 形式的不透明引用；但 Windows Credential Manager / macOS Keychain 的真实写入与出站短生命周期解引用仍未接通。本次 OpenCode Zen 路径不代表需认证 Provider 已完整可用。
2. **BE-006 安装/配置/回退事务仍不完整**：真实备份、原子写入、写后语义校验、失败自动恢复和故障注入尚未达到 PRD。
3. **JSONL 到 SQLite 查询层、坏行隔离、轮转、锁与损坏诊断尚未完成**。
4. **Provider 并发/背压、有限重试、熔断与健康选择仍未完成**。
5. **Doctor 完整矩阵与 Windows/macOS 已安装包级验证仍不足**。

因此，原始综合分不再代表修复后的代码质量，但整体仍处于“核心链路已恢复可信、安装/凭据/持久化/发布工程未闭环”的阶段，不能提前宣称稳定发布。

### 0.4 并行复审后的新增结论与本轮修复

2026-07-10 使用后端安全、UI/UX、发布测试三个独立审查角色重新复核当前工作树，新增确认以下问题：

| 发现 | 严重度 | 当前处理 |
|---|---:|---|
| 安装页曾生成 `Provider base_url=http://127.0.0.1:18001/v1`，即把上游指回 d-token 自己 | P0 | **已封死**：桌面端使用结构化 URL 解析拒绝同一 loopback listener，并有 IPv4/localhost/IPv6 回归测试；前端遇到 native `ok=false` 不再继续应用或标记已配置 |
| OpenAI Chat 入口重建请求时丢弃 `tools/tool_choice/temperature/max_tokens/response_format` 等字段 | P0 | **已修复基础链路**：入口保留原始结构化 JSON，只让压缩器修改明确目标字段；新增 coding-agent tool 语义回归测试 |
| Anthropic → OpenAI bridge 把内容压成纯文本，丢失 system、tool_use、tool_result、tools 与生成控制 | P0 | **已修复非流式基础桥接**：新增 system、tool call/result、tool choice、max_tokens、temperature、top_p、stop_sequences 双向转换测试；流式 tool delta 仍需单独实现和实测 |
| `api_key_ref` 可能从 `/providers`、`/config/active`、Dashboard/export 元数据暴露 | P0 | **已修复外显面**：统一输出脱敏 Provider DTO，只返回 `credential_configured`；严格拒绝明文或非 OS 凭据引用 |
| 当前发布报告早于 PRD 和 2026-07-10 大量工作树变更，无法证明当前候选版 | P0 | **仍未闭环**：必须把 commit/dirty-state、前端测试、TypeScript、Clippy、当前安装包 hash、安装后 hash 与 macOS artifact 绑定到同一候选版 |
| JSONL 坏尾、并发、磁盘满、轮转与流式中途失败记账缺乏故障测试 | P1 | **未完成**：列为持久化/可观测下一批硬门禁 |

并行复审还确认：已有 `9/9 product readiness`、`11/11 Headroom coverage` 中部分证据属于 fixture/mock/源码存在性检查，不能再用来代替真实用户链路。后续报告必须明确区分“单元契约”“模拟协议”“真实 OpenCode”“安装包/平台实机”四类证据。

## 1. 执行摘要

当前后端已经具备一批可继续演进的基础能力：OpenAI/Anthropic 协议入口、路由模型、压缩处理、Event Ledger、若干 Dashboard API，以及带进程身份校验的 Tauri 服务生命周期逻辑。但这些能力还没有组成一条可信、可恢复、可发布的完整链路。

最严重的问题不是“功能少”，而是部分正式路径会制造与真实状态不一致的成功结果：没有配置上游时仍返回 `d-token-ok`；安装扫描 API 返回 `m1_mock`；智能学习与桌面数据又可能把预览状态包装成完成状态。与此同时，当前 Rust workspace 存在编译错误，非本机监听只警告不拒绝，Provider 凭据也没有真正接入出站请求。

因此，本报告给出的判断是：

- **当前不能发布。**
- 应先恢复“编译、格式、安全边界、失败语义”四条底线，再继续功能扩展。
- 所有“成功”必须能由 d-token-owned runtime、真实上游响应、Event Ledger 和验证结果共同证明。
- 旧的 `RELEASE_READINESS_REPORT.md`、`PLAN.md`、`FINAL_REVIEW.md` 只能作为历史材料，不能代表 2026-07-10 当前工作树的真实状态。

## 2. 审查范围与证据

### 2.1 审查范围

1. 产品与架构一致性：PRD、技术栈、总体架构、安全边界。
2. 本地服务入口：监听范围、协议入口、路由、请求转发、错误语义。
3. Provider / Route / Instance：配置、凭据、超时、健康、选择规则。
4. 安装与配置事务：扫描、计划、备份、应用、验证、回退。
5. 生命周期：启动、健康检查、进程身份、停止、孤儿进程风险。
6. 数据与可观测性：Event Ledger、JSONL、Dashboard API、日志、导出。
7. Doctor：诊断覆盖、事实来源、修复建议。
8. 稳定性与性能：并发、背压、超时、恢复、数据增长。
9. 测试与发布治理：构建、单元测试、格式、依赖、文档漂移。

### 2.2 已执行验证

| 验证项 | 结果 | 说明 |
|---|---|---|
| `cargo test --workspace` | **先失败，后续已修复** | 最新复核：115 passed |
| `cargo fmt --all -- --check` | **先失败，后续已修复** | 最新复核通过；Clippy `-D warnings` 同时通过 |
| 聚焦 Rust crate 测试 | 通过 | `dtoken-router`、`dtoken-event-ledger`、`dtoken-config-patcher`、`dtoken-doctor`、`dtoken-process` 共 12 项通过 |
| 前端生产构建 | 通过 | 仅证明前端能打包，不证明后端真实链路可用 |
| TypeScript 类型检查 | 通过 | `tsc --noEmit` 通过 |
| 完整依赖审计 | **先发现 1 个 critical，后续已修复** | Vitest 3.2.6；最新完整 `npm audit`：0 个已知漏洞 |
| 目录结构检查 | 通过 | `support/scripts/check_structure.py` 通过 |

### 2.3 代码图证据

本轮优先使用 codebase-memory-mcp 对项目建立结构化证据，索引项目名为 `D-D-TOKEN-d-token`，审查时约 13,589 个节点、20,085 条边。对于字符串、配置和非代码文件，再使用本地精确搜索与实际构建测试补证。

## 3. 当前评分

| 维度 | 得分 | 主要原因 |
|---|---:|---|
| PRD 与架构一致性 | 42/100 | d-token-owned runtime 方向正确，但安装目标、SQLite/keychain、数据真相边界尚未落地 |
| 核心请求链路 | 45/100 | 协议、压缩、路由、记账骨架存在；无上游时却会合成成功 |
| 安全边界 | 28/100 | 非 loopback 不拒绝，Router 缺系统性边界防护，凭据模型未闭环 |
| 安装与恢复 | 34/100 | 有 plan/backup 概念，但不是真实目标备份，写入非原子，回退未闭环 |
| 数据与可观测性 | 47/100 | Ledger 基础正确；JSONL 扫描、坏行恢复、锁、轮转、索引不足 |
| Doctor 与运维 | 36/100 | 有入口但检查范围明显不足，部分结论是硬编码 |
| 测试与发布治理 | 30/100 | workspace 无法编译、fmt 失败、旧发布结论漂移 |
| 性能与可维护性 | 43/100 | 关键函数过大，缺 timeout、背压、数据增长方案 |
| **综合** | **38/100** | **有可复用基础，但当前不具备发布可信度** |

## 4. 已确认的优点

1. `Router.decide` 在 route/provider 缺失时能够显式报错，没有在路由层偷偷切换未知 Provider。
2. OpenAI 与 Anthropic 两类协议入口、压缩、路由决策、请求日志和 Ledger 事件已经处于同一核心模块，可在现有基础上收敛，而非推倒重写。
3. JSONL Ledger 默认记录结构化元数据而非完整 prompt/response，符合隐私和脱敏方向。
4. Tauri 生命周期停止流程会校验 `managed_by`、identity、端口、PID 和健康身份，不盲目结束未知进程。
5. d-token 服务启动时使用固定本地地址 `127.0.0.1:18001`，这一实现应成为所有入口的统一约束。
6. 聚焦 crate 测试已有一部分可用基础，适合扩展为风险导向的回归套件。

## 5. 发布阻断项（Blocker / P0）

### BE-001：当前 Rust workspace 无法编译

- **影响**：任何后续“已修复、已发布、可打包”结论都失去基础；CI 和安装包构建无法可信通过。
- **证据**：`cargo test --workspace` 有 5 个编译错误，集中在 Optical 类型与 `CompressionResult` 结构变更未同步。
- **整改**：先统一结构体契约，再修调用点；为 `CompressionResult` 增加构造器或单一 builder，避免新增字段后多处漏填；为 `OpticalBucket` 补齐路由键需要的 trait。
- **验收**：workspace build/test、clippy、fmt 全绿；不允许只跑单个 crate 代替 workspace gate。

### BE-002：没有上游 Provider 时返回伪成功

- **影响**：Agent 会收到 HTTP 成功和 `d-token-ok`，Dashboard/Ledger 也会记录成功，用户无法发现请求根本没有到达真实上游。这是数据真实性与核心业务语义的双重故障。
- **证据**：`code/crates/dtoken-core-api/src/lib.rs::routed_openai_payload`；默认模型 `dtoken-test-provider`，合成 assistant content `d-token-ok`，并生成 `event_ledger.recorded=true`。
- **整改**：删除正式请求路径中的本地成功 fallback。未配置、Provider 不可用、凭据缺失必须返回类型化配置错误；启动验证和首次连接引导应提前发现问题。
- **验收**：无 Provider、禁用 Route、缺凭据、上游不可达四种场景均得到不同且可操作的错误；Ledger 记录失败原因但不记录秘密；不存在任何正式路由返回 `d-token-ok` 的测试替身。

### BE-003：非 loopback 监听是“警告后继续”

- **影响**：一旦误配置到局域网或公网，日志、配置、导出、Doctor、请求入口都可能被其他设备访问。
- **证据**：`code/crates/dtoken-proxy/src/main.rs::main` 对非 `127.0.0.1` 只记录 warning 后继续 bind。
- **整改**：默认 fail closed，仅接受 `127.0.0.1`/`::1`；若未来确需远程模式，必须是独立显式功能，并同时具备强认证、权限控制、Host/CORS、请求体限制和清晰风险确认。
- **验收**：自动化测试证明 `0.0.0.0`、局域网 IP、空 host 均启动失败；默认端口只能从本机访问。

### BE-004：生产 Router 混入 mock 安装接口

- **影响**：前端可能把 `m1_mock` 结果当作真实扫描/计划，破坏安装与回退可信度。
- **证据**：`dtoken-core-api` 的 `/install/scan`、`/install/plan` 返回 `status: "m1_mock"`，且与正式 API 同级注册。
- **整改**：正式 Router 移除 mock handler；测试 fixture 放进 `support/` 或编译期 dev feature，生产包不可达。真实安装扫描应由 Tauri/native service 返回明确的 evidence 和 freshness。
- **验收**：release build 路由表中不存在 mock/fixture endpoint；契约测试校验扫描结果来源、时间、平台和证据。

### BE-005：Provider 凭据没有真正接入

- **影响**：需要认证的官方或第三方 Provider 实际不可用；表面上的 ProviderProfile 不能形成真实请求链路。
- **证据**：`api_key_ref` 不解引用，出站仅添加已脱敏的 `x-dtoken-api-key-ref`，未从 OS 安全存储取得真正凭据。
- **整改**：实现统一 CredentialStore：Windows Credential Manager / macOS Keychain（可由 keyring/Stronghold 适配）；配置只保存引用；仅在出站请求组装的最短生命周期内取得秘密；日志和错误永不携带真实值。
- **验收**：有凭据的真实 OpenCode 路由测试可到达用户原 Provider；缺失、撤销、过期各自返回正确错误；日志、Ledger、导出和崩溃信息通过秘密扫描。

### BE-006：安装、配置、备份与回退不构成真实事务

- **影响**：可能出现“配置写了一半、备份不可用、界面仍显示成功”，导致用户原有 Agent 配置受损。
- **证据**：`run_install_transaction` 仍以 `rtk/headroom` 为安装目标；backup 主要写 plan manifest 而非真实目标内容；external restore 仍 gated；`ConfigPatcher` 直接 `fs::write`，读取失败被当成空文件。
- **整改**：按 PRD 收敛为只安装/管理 d-token，第三方只检测和受控配置。采用 `preflight → durable backup → diff → explicit apply → atomic write → parse/semantic verify → health verify → commit` 状态机；任何一步失败自动恢复。
- **验收**：对权限拒绝、磁盘满、进程退出、写入中断、验证失败、陈旧 plan、用户并发修改做故障注入；每个失败点都能恢复原文件，且 Ledger 有脱敏事务记录。

## 6. 高优先级问题（P1）

### BE-101：Provider timeout、重试和背压未落地

- `ProviderProfile.timeout_ms` 未用于请求客户端，`reqwest::Client::new()` 使用默认行为。
- 建议区分 connect/read/total timeout；只对可安全重放的失败做有限重试；每个 Provider 设置并发上限与排队预算；超载时快速返回可解释错误。
- 验收需覆盖慢连接、慢响应、连接重置、429、5xx、客户端取消和流式中断。

### BE-102：路由决策规则不足

- 当前主要按 instance + protocol 线性匹配，缺少 enabled、health、模型兼容、priority、冲突检测和 cc-switch profile 同步规则。
- 建议先建立确定性规则表和 `RouteDecisionReason`，再做策略扩展；每次决策都可解释“为何选它/为何跳过其他项”。

### BE-103：JSONL Ledger 无法承受长期运行

- `recent(limit)` 从文件头读取全部记录，坏一行可能让整个查询失败；append 缺少文件锁、轮转、索引和明确 durability 策略。
- 建议 JSONL 保留为可审计原始账本，同时增加 SQLite 查询层；坏行隔离并记录 recovery diagnostics；设置大小/日期轮转、保留期和导出策略。
- 近期可以先实现倒序索引或分段读取，避免 Dashboard 每次 O(n) 全扫描。

### BE-104：启动时数据损坏会被静默吞掉

- AppState 对多个 JSONL 的读取使用 `if let Ok`，失败后页面可能悄悄变空。
- 应将 corruption、skipped lines、recovered rows、last good offset 暴露给 Doctor 和 Logs，并禁止把“空数据”解释成“运行正常”。

### BE-105：Doctor 仍是骨架

- `run_basic_doctor` 主要检查端口，local-only-bind 甚至直接写为成功。
- 应覆盖：runtime identity、进程身份、监听地址、Provider 凭据可用性、Route/模型、Event Ledger 可写可读、回退点、磁盘空间、Agent 连接、cc-switch 只读同步、Windows/macOS 权限。
- 每项必须返回 `status + evidence + user_action + safe_fix`，而不是只有 ok/warn。

### BE-106：生命周期 commit 边界不正确

- `start_dtoken_service` 在 manifest 写失败时只 warning，却返回启动成功，可能留下无法由 App 安全停止的孤儿进程。
- manifest 耐久写必须成为启动提交条件；失败时立即终止刚启动的受管进程并清理临时文件。

### BE-107：同步等待阻塞桌面命令

- `start_dtoken_service` 同步 sleep 最长约 4 秒，函数约 183 行、复杂度 22。
- 应拆成异步状态机：resolve binary、spawn、probe、commit、publish state；支持取消、超时和进度事件，避免冻结 Tauri command 线程。

### BE-108：数据架构没有兑现 SQLite + OS keychain

- 当前依赖与源码未发现 SQLite 或 keychain/Stronghold 的正式实现。
- 架构文档若仍保留此承诺，就应将其纳入发布计划；若产品决定变化，必须先更新 PRD/ARCHITECTURE，再改实现，不能让文档与代码长期分叉。

## 7. 中优先级问题（P2）

1. `openai_chat_completions`、`run_install_transaction`、`get_desktop_data` 等函数体积和认知复杂度过高，应按领域边界拆分，而不是按“再抽一个 util”机械拆分。
2. 通用 `ProcessManager.start_placeholder` 仍是假实现；应删除重复入口，统一复用 Tauri 已较完整的受管进程语义。
3. 配置写入需要保留原文件权限、换行、编码和注释；支持 managed block 的幂等写入与冲突说明。
4. Dashboard API 应统一 envelope：`source`、`freshness`、`generated_at`、`partial`、`warnings`、`schema_version`。
5. 日志与 Ledger 需要统一 correlation id，连接 Agent 请求、压缩阶段、route decision、upstream、响应与 UI 展示。
6. 导出需要字段白名单、秘密扫描、体积限制和用户预览；不能直接打包原始运行目录。
7. Provider、Route、Instance 的 schema 需要版本化与迁移测试，避免升级后旧配置直接失效。

## 8. 分阶段迭代方案

### Phase 0：恢复可信基线（1 个短迭代）

1. 修复 5 个 Rust 编译错误。
2. 统一运行 `fmt`，只处理本轮相关文件并审查差异。
3. 建立强制 gate：workspace test、clippy、fmt、前端 build/typecheck、结构检查。
4. 将旧发布报告标注为历史快照，禁止其继续作为当前发布结论。

**退出条件**：全 workspace 能编译和测试；没有被忽略的 gate。

### Phase 1：修复错误语义与网络边界

1. 删除无 Provider 的伪成功。
2. 非 loopback fail closed。
3. mock 安装接口移出生产 Router。
4. 增加 request body limit、Host/CORS 本地策略、统一错误 envelope。
5. 用契约测试覆盖未配置、不可达、超时、取消和流式错误。

**退出条件**：任何失败都不能显示成功；生产路由不含 mock；网络仅本机可用。

### Phase 2：打通真实 Provider / Route / Credential

1. 实现 OS CredentialStore。
2. 将 ProviderProfile 的超时、健康、协议、模型和凭据真正用于请求。
3. 建立确定性路由规则与可解释决策。
4. 用 OpenCode 原配置做唯一真实 Agent 验收入口，不读取或输出秘密；**不得用 Codex 真实配置测试**。

**退出条件**：真实 OpenCode 请求完整经过 d-token，并由 Ledger、route decision、upstream response 和 Dashboard 同步证明。

### Phase 3：重做安装与恢复事务

1. 从“安装第三方”改为“安装/启动 d-token + 检测/连接第三方”。
2. 建立耐久备份、原子写、验证、自动恢复状态机。
3. 合并 ProcessManager 与 Tauri lifecycle，消除 placeholder。
4. 加入故障注入和平台差异测试。

**退出条件**：所有失败点都能恢复；不存在“已应用但无回退点”状态。

### Phase 4：数据、Doctor 与长期运行

1. 引入 SQLite 查询层，JSONL 作为审计账本。
2. 加坏行恢复、轮转、保留期、导出和迁移。
3. Doctor 覆盖完整产品链路，并提供安全可执行建议。
4. 做 8–24 小时持续流量、文件增长、内存、CPU、句柄和恢复测试。

**退出条件**：数据增长不拖慢 Dashboard；重启和局部损坏可恢复；Doctor 不再依赖硬编码结论。

## 9. 发布验收门槛

以下条件全部满足前，不得使用“release candidate / 稳定 Beta / 完成”字样：

- [ ] Rust workspace build/test/clippy/fmt 全绿。
- [ ] 正式代码和正式 Router 中无 mock、fixture、`d-token-ok` fallback。
- [ ] 默认只监听 loopback，错误配置会拒绝启动。
- [ ] Provider 凭据来自 OS 安全存储，日志/Ledger/导出无秘密。
- [ ] OpenCode 真实请求完整穿过 d-token，失败/流式/取消也有证据。
- [ ] 安装与配置事务支持真实备份、原子应用、验证和自动回退。
- [ ] Dashboard 所有事实来自 d-token-owned runtime，并携带来源与新鲜度。
- [ ] Doctor 覆盖 runtime、route、provider、agent、ledger、rollback、权限和平台。
- [ ] JSONL 局部损坏不会让全部历史消失；查询性能有长期运行证据。
- [ ] Windows 与 macOS 均有安装、升级、卸载、恢复和最小权限测试。
- [ ] 不使用 Codex、Claude Code、Cursor、Cline 的真实用户配置做本轮真实测试。

## 10. 文档治理说明

1. `docs/PRD.md` 是唯一产品事实源。
2. `docs/RELEASE_READINESS_REPORT.md` 是旧阶段报告；其“release candidate”结论已被本次实际编译结果否定。
3. `docs/PLAN.md` 与当前实现状态存在漂移，应在修复阶段同步更新。
4. `docs/FINAL_REVIEW.md` 是 2026-07-01 的方案/目录审查，不是当前代码发布审计。
5. 本报告记录的是 2026-07-10 当前工作树的证据；代码变化后应通过自动 gate 和新的审查记录更新状态，而不是手工沿用旧结论。
