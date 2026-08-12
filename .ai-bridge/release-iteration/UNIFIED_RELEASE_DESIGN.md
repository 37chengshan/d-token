# UNIFIED_RELEASE_DESIGN — d-token v0.0.1 发布统一设计（批准设计固化）

- 文档类型：批准设计固化文档（唯一输入，供后续 implementation plan 使用）
- 设计来源：`local://dtoken-approved-release-design.md`（用户已逐节批准，含 2026-08-12 前端边界补充）；旧文档冲突不可覆盖批准设计
- 核对依据（术语与事实）：`docs/PRD.md`（P0.0.1–P0.0.10、P0.4、P0.7、§12.5）、`docs/d-token_AI_product_target_spec_v2.md`（v2.1，F-01/F-05、§8、§9）、`docs/PLAN.md`（唯一 Living Plan）、`docs/product/IMPLEMENTATION_STATUS.md`（代码事实）、`.ai-bridge/release-iteration/{GOAL,PLAN,FINAL_BLUEPRINT,TASKS}.md`、`docs/releases/v0.0.1/`（v0.0.1 执行分解：README/GOALS/REQUIREMENTS/EXECUTION/TASKS/VERIFICATION/RISKS_DECISIONS/EVIDENCE/OSS_MIGRATION）
- 执行基线：分支 `codex/final-release-iteration`；执行基线由 P0 preflight 运行时 `git rev-parse HEAD` 采集并冻结，不硬编码任何历史 HEAD
- 本文件约束：只创建本文件；不改任何其他文件、不提交、不测试、不格式化、不运行项目命令；本文件是设计契约，不是实现/通过声明
- 版本事实：当前 manifest/PRD/PLAN 口径为 `0.2.0-beta.1`，用户指定最终发布目标为 **v0.0.1**（V-1 统一前为过程事实，见 §19）

---

## 1. 权威顺序（冲突裁决顺序）

发生冲突时按以下顺序裁决（批准设计原文口径；`docs/releases/v0.0.1/README.md` §4、`docs/index.md` §7 的旧排列将在统一重写时对齐）：

```text
本轮对话明确要求 / 用户逐节批准的发布设计（本文件固化的内容，含 2026-08-12 前端边界补充）
→ docs/d-token_AI_product_target_spec_v2.md（目标规格 v2.1，F-01/F-05、§8、§9）
→ 固定版本上游真实能力与行为（Headroom / lean-ctx / pxpipe / RTK / Codebase Memory 等固定版本外部事实）
→ docs/PRD.md（唯一产品真理源；其中 P0.0.10 为已与批准设计同步的 Agent/Provider/Route 配置交互最高优先级契约）
→ docs/PLAN.md（唯一 Living Plan）
→ docs/releases/v0.0.1/（v0.0.1 具体执行分解，不是新的 Living Plan）
→ docs/product/IMPLEMENTATION_STATUS.md（代码事实快照）
→ ARCHITECTURE / TECH_STACK / SECURITY_BOUNDARIES
→ 当前专题文档（optical/、testing/、research/）
→ history/ → archive/
```

1. **本轮明确要求 / 批准设计优先**：用户已逐节批准发布设计（本文件逐条固化，含 2026-08-12 前端边界补充）；任何旧 PRD/PLAN/版本包/目标规格中的相反旧口径不得覆盖批准设计；执行期发现冲突时以批准设计为准，并由后续文档统一重写收敛（见 §19 冲突清单）。
2. **目标规格次之**：`docs/d-token_AI_product_target_spec_v2.md`（v2.1）排在 PRD 之前；其 F-01/F-05 与 PRD P0.0.10 已在 2026-08-12 同步轮对齐，两者若冲突以批准设计为准。
3. **固定上游事实**：Headroom / lean-ctx / pxpipe / RTK / Codebase Memory 等固定版本上游的真实能力与行为是既定事实输入，不因本仓库文档口径而改变。
4. **PRD P0.0.10 契约优先（PRD 内部）**：`docs/PRD.md` §P0.0.10（2026-08-12 最新确认）是 Agent / Provider / Route 配置交互的最高优先级契约；与本 PRD、PLAN 或版本包中的相反旧口径冲突时，以 P0.0.10 为准并清除旧口径（2026-08-12 同步轮已把该合同写入 PRD/目标规格/PLAN/版本包）。
5. **源码只决定 current**：实际源码与当前测试证据只决定「当前实现状态」（current），不裁决设计优先级；实现现状不得覆盖批准设计、目标规格或固定上游事实。
6. **MUST/SHOULD/MAY 语义**（PRD P0.0.1）：MUST=目标版本发布阻断条件，未满足不得称可发布；SHOULD=优先交付但可记录风险后延期；MAY=可选能力，不阻塞核心发布。
7. **诚实状态语义**：未被当前候选真实运行证据、正式入口、错误路径、安全降级、回滚和文档同时证明的能力，只能标记 `preview`/`partial`/`unavailable`/`unknown`/`not_verified`；不得因类型、接口、静态页面、metadata-only 事件、passthrough 或单元测试存在而标记完成。
8. **版本口径**：用户指定目标版本 v0.0.1；当前 `0.2.0-beta.1` 是尚未执行 V-1 的事实，不构成范围降级；V-1 前不得生成最终候选，V-1 后新证据只绑定 v0.0.1 `C_final`。

## 2. MUST 与非目标

### 2.1 MUST（发布阻断，任一未满足 `release_ready=false`）

来自 GOAL（13 项用户要求）、PRD P0.0.7 发布门、版本包（全部 requirement 均为 MUST）、FINAL_BLUEPRINT §1.1 的合并口径：

1. **完整 Agent 清单与用户选择**（P0.0.10.1）：安装页显示正式支持矩阵全部 Agent，每项显示 `已安装/未安装/已配置/未配置/配置失败`；未安装项不可勾选，已安装项默认全部勾选且可取消，已配置项保持勾选。
2. **Agent 矩阵按官方能力分层闭环**（P0.0.10.2）：v0.0.1 目标矩阵按官方能力分 Route / MCP-Hook / Detection-only 三层，各层独立闭环（§3.4/§6.3）——Route 层做真实扫描、配置、复读验证、路由证据和恢复；MCP-Hook 层做真实 initialize/tool/hook 与回退且不冒充路由；Detection-only 层有官方架构证据 + 真实检测 + 诚实 UI（「已检测/不支持本地路由」）即完成，不要求 routed-flow。「等待安全适配」「来源冲突」「暂不支持自动配置」不得作为最终交付状态；任一要求适配 Agent 在其所属能力层未闭环即 `release_ready=false`。
3. **一次点击直接配置**（P0.0.10.3）：点击一次「配置」即本次明确授权并直接执行，无第二层 Diff/风险/备份确认弹窗；后台完成计划、脱敏 Diff、备份、配置、复读验证、恢复点和 receipt；正常结果只显示「配置成功」。
4. **幂等重配**（P0.0.10.4）：已配置 Agent 再参与配置时先检查当前状态；无变化只重新验证不重复写入；有变化自动更新并验证。
5. **逐 Agent 独立事务**（P0.0.10.5）：每个 Agent 独立 plan/backup/apply/verify/rollback/receipt；某 Agent 失败只回退该 Agent 本次未完成写入，其他成功 Agent 保持成功；失败项显示简短原因与单独「重试」，不得整批回退成功项。
6. **三类上游运行模型全部自动适配**（P0.0.10.6）：`agent_managed_auth`（OpenCode Free、GPT/ChatGPT、Claude 等账号/OAuth/已有 API Key，认证由原 Agent 持有，d-token 不重复登录、不持久化 token/cookie/Authorization）、`dynamic_local_proxy`（OpenCodex 等本地动态代理，固定接入代理链并原样透传 model 与 Responses/Chat 协议语义，多模型是正常能力不是冲突）、`cc_switch_managed`（cc-switch 只读监控并自动同步 d-token 自有 route/runtime）。
7. **无上游选择器**（P0.0.10.7）：用户只选择需要配置的 Agent；不在 d-token 内选择 Provider、上游、模型、协议、endpoint 或 cc-switch profile；按三类运行模型自动保持 Agent 当前实际运行语义。
8. **错误只面向结果**（P0.0.10.8）：普通页面不得出现 `needs-source-fix`、`has_resolution_gap`、`等待安全适配`、`来源冲突`、adapter、route 或 credential reference 等内部机制术语；无法配置时显示「配置失败」+ 一条用户可执行说明 + 「重试」，技术证据进详情与日志；Detection-only 项允许显示「已检测/不支持本地路由」这一批准设计的普通终态（§3.4），不显示内部机制细节。
9. **Route / MCP-Hook / Detection-only 诚实区分**（P0.0.10.9）：Route 配置证明模型流量经过 d-token；MCP/Hook/Skill 只证明工具或上下文能力，不能冒充路由成功（详见 §3.4）。
10. **首次引导**（P0.0.10.10）：按 Onboarding 五段执行（§3.7）——欢迎简介 → 有呼吸感动画和可视进度的服务启动 → 扫描已安装 Agent → 用户调整默认勾选并点击一次「配置」（内部 plan/apply/readback/verify、逐 Agent 独立结果）→ 开始使用；完成后逐 Agent 显示「配置成功/配置失败」。
11. **canonical pipeline 与单一 IR**（版本包 RUNTIME-7、GOALS 14）：一个请求只能进入一个 canonical Context IR；每个 IR node 恰一个终态动作（Preserve/Reuse/HeadroomSemantic/RtkCommandOutput/Optical）；HTTP/Rust/MCP/TS/Proxy 共用同一 resolved production pipeline；**single dispatch 必须包含 Native Responses（`/v1/responses`），不得绕过 canonical pipeline**；发送体=验证体。
12. **不可伪造 type-state 与 receipts**（RUNTIME-8、GOALS 16）：ReadyToSend 只能由 runtime pipeline 内部基于 artifact-derived ValidationReceipt、RecoveryReceipt 与最终 envelope 校验密封；调用方布尔值、任意 digest、时间戳哨兵不能铸造状态；**生产 transport 必须实际消费 SendCommitReceipt**；CCR 必须是真实 durable store→receipt→marker→restore 闭环。
13. **四维独立 accounting**（RUNTIME-5/9）：compression / command-output / provider-cache / relay 四维独立记录展示，禁止求和成一个 savings；Local estimate / Provider actual / claimable 分离；unknown≠0；claimable 需 paired Provider 计费/usage 与归因链完整。
14. **LeanCTX 九边界与 CCR**（CONTEXT-1/2/3/4）：Context Bus/Session Cache/Context Relay/Map-signature/Query-aware Retrieval/Recovery Pointer-Manifest/Stale-Superseded/SharedContext authorization/self-hit-size 九边界为 MUST；新 `dtoken-context-bus` 搬移而非复制策略，避免双权威；Reuse/memory 必须在 transform plan 前进入 IR，删除 post-validation injection。
15. **Headroom 117 项 / RTK / Optical 边界**（P0.0.6、P0.0.7）：117 项 tracked registry 最终 117/117 verified production，零 placeholder/missing/not-in-scope；RTK 三平台 hard-Blocked（RUNTIME-10）必须先解封（移除 Blocked 封锁 → 未配置=unavailable）再走 C4A/B/C 生产实现；Optical exact 全部能力门绿时由 planner 自动 active（无手动 active 控件），任一能力门不绿则同请求 PreserveText；shadow/metadata-only/passthrough 只证明负路径。
16. **真实 routed 验收**（P0.0.7、蓝图 N2/N3）：Codex 真实 routed-flow（`Codex → d-token:18001/v1 → ocxlocal:10100/v1 → 动态模型上游`，agent_id=codex、Responses 协议、无 OpenAI 认证语义、请求 model 透传不固定安装时值）不能由配置事务精确恢复替代；OpenCode Free 与 Go dsv4flash 两条长工程真实 routed 验收（详见 §10）。
17. **配置写入事务链**（P0.0.3、GOAL §4.3）：`detect → parse current upstream → plan → redacted diff → backup → user confirm → apply → reread/verify → receipt`；失败精确回滚原始字节；配置只写 d-token-owned 字段，清除只删带 manifest/hash ownership 证据的内容且幂等。
18. **证据绑定与 fail-closed 门**（EVID-1..6、GOAL 13）：最终候选 evidence 必须机器可验证绑定 `candidate_commit=C_final` 与同一 `source_digest`/registry/corpus digests；聚合器不得按 mtime/文件名/`status=pass` 接受旧 artifact；G0–G9 与 N0–N10 全部绑定同一 clean `C_final`，任一缺失 `release_ready=false`。
19. **凭据与隐私安全**（GOAL §4.1/4.2、AGENTS.md）：凭据只进操作系统凭据库，配置只保存不透明引用，禁止明文 fallback；API key/token/cookie/Authorization/完整 prompt-response/源码/大段原始输出不进入普通日志、配置、导出或提交；所有本地服务只绑定 `127.0.0.1`。
20. **UI 双平台双尺寸双语**（GOAL 11、蓝图 INV-12）：UI 在 1180×780 与 920×640、浅/深色、中/英、Reduce Motion 下通过；真实 Tauri 窗口验证关键交互；920×640 无横滚由 Playwright 真实 layout engine 证明，最终由 N6 真实 Tauri 复核。
21. **真实长工程端到端压缩率 ≥5% 与零字节保护**（GOAL 2）：`rate = (baseline input tokens − final sent input tokens) / baseline input tokens`，真实 Agent production ingress 到 final send，paired 条件相同，只计 actual 输入减少；fallback/failure/shadow/estimate 不计 actual；Relay/cache/RTK/Optical 四维分账（§3.1）；Provider 无 usage 时允许同版本 tokenizer 的 local measured，不冒充计费；system/developer、当前用户消息、代码/diff、secret、tool schema/arguments、开放工具状态零字节变化；恢复精确；发送前失败保留原始正确请求；上游已接收或 streaming 后禁止自动重发。
22. **CacheAligner 为 canonical pipeline 真实组件**（GOAL 1）：CacheAligner 进入同一 canonical Context IR / production path，做真实 cache alignment（对齐既有缓存命中面）并产出真实 receipt；其节省只计入 provider-cache 分账（与 compression/command-output/relay 独立，§6.2）；禁止仅 accounting/metadata/no-op/未接线路径冒充完成。

### 2.2 非目标（明确不做）

1. **不新增工程化一级页面、不重排八页 IA、不重构 shell**（FINAL_BLUEPRINT §1.2.1；前端边界见 §4）。
2. **不换前端技术栈/设计系统**：保持当前 React 18/TypeScript/Vite + Tauri 2、纯色 shell、现有视觉风格与动画语言、Reduce Motion、Windows/macOS 共用 UI。
3. **不实现上游 chooser**：Provider/上游/模型/协议/endpoint/cc-switch profile 选择器一律不做（任何普通/高级 UI 均禁止）；Agent 勾选是允许的——完整清单按已安装/未安装展示，已安装项默认全部勾选且可取消（P0.0.10.1，§2.1.1）。
4. **不引入外部 Headroom 8787 运行依赖**；cc-switch 永远只读（含 OMP 只读边界，见 §3.2）；外部仓库不得作为隐式运行依赖或直接复制进生产。
5. **不提供手动 active 控件、不扩大视觉压缩为普通图片压缩**：exact 全门绿时由 planner 自动 active（无手动 active 控件），任一能力门不绿则同请求 PreserveText（§2.1.15）；普通图片 resize/re-encode/tile/ONNX 不在范围。
6. **不把 Windows 实机/签名凭据、旧 OpenCode G7 证据、旧 DMG、浏览器 preview 当作最终候选证据**；OpenCode 旧 bounded replay 不替代 N3 长工程验收。
7. **不复制整份 HybridContextAccounting 进 receipt**；不把完整 prompt/response/密钥/绝对路径写入日志、配置、导出或提交。
8. **不在 release build 注册 benchmark/smoke/test-only commands**；开发验证命令受 debug gate 限制。
9. **不围绕 PR6 云端 CI 失败修 CI**；独立发布证据脚本/工作流缺口仍须补齐，但不得借 PR6 跳过独立发布证据。
10. **不推倒重写**：从当前实现迁移收敛（见 §17），不重写 Rust runtime、不重建八页 UI、不另起第二套 pipeline。

## 3. 冻结决策（批准设计已冻结，不重开）

### 3.1 5%（Token 节省基线）

- **冻结决策**：批准设计以 **5%** 作为物理 Token 节省基线/目标门槛（“5%” 为批准设计原文口径，属于冻结决策，不重开）。
- 事实核对：`docs/PLAN.md` 当前真实 routed 汇总为 89 条 routed request、20 条可见 request record、物理减少 279,270 Token（约 **4.0%**）；该数值是当前候选的真实实测，不是 mock 或推算。
- 口径：物理节省与 claimable 分离——物理减少不自动等于可声明收益；`claimable` 仅在 Provider 计费/usage 证据与归因链完整时为 true（G7 报告 `attribution_basis=none` 时物理节省不冒充可声明收益）。5% 阈值只约束物理节省基线，不改变四维独立 accounting 与 claimable 规则（§2.1.13）。
- 边界：任何展示必须区分「真实 0」与「unavailable」，缺 Token/价格证据时不得以 0 或示例数字补齐（GOAL 6/7）。

### 3.2 cc-switch / OMP 只读

- **cc-switch 永远只读**（PRD P0.0.4、F-05）：d-token 不写其数据库、配置或凭据；使用只读 DB/WAL/SHM watcher、750ms debounce、单飞刷新和 5–15 秒 polling fallback；用户首次点击「配置」即授权持续只读跟随当前生效配置；Provider/模型/协议/endpoint/profile 变化后自动更新 d-token 自有 route/runtime，用户无需再选或确认；每次同步后台生成备份、复读验证、receipt 与 Event Ledger 记录并提供一键恢复；SQLite 锁定/损坏/未来 schema/缺失时保留 last-known-good，不猜测、不写 cc-switch；Runtime 使用原子 route snapshot（旧请求旧快照、新请求新快照，reload 失败保留 last-known-good）。
- **OMP 只读**（2026-08-09 交叉评审与产品收口沿用）：OMP 评审/原生 OMP 代理与 d-token 之间维持只读对照边界——OMP 只读审计/评审可作为计划与验收输入，d-token 不因 OMP 而写外部状态；C0–C8 修复由独立 worktree 的 Agent 完成，共享文件写入按 §14 串行原则由主代理统一维护；OMP 交叉评审结论（durable CCR 骨架已存在、自动 Agent lifecycle 已存在、8/8 页面真实 handlers 已存在、CBM manifest→graph 单版本已工作）作为「不得误报缺失」的事实输入；真实缺口（receipt 未绑定请求、Native Responses 绕过 canonical、SendCommitReceipt 未生产接线、CBM 五正式版兼容缺失等）作为必须进计划的事实输入。
- 边界：cc-switch 由主代理用 Computer 完成 A→B→A 真实切换验收；实现 Agent 只建 harness，不替主代理操控真实 UI/cc-switch（FINAL_BLUEPRINT §9.1.5）。

### 3.3 其他选中 Agent 一次授权写入

- 用户点击一次「配置」即构成对**所有勾选 Agent** 的一次性明确授权并直接执行（P0.0.10.3、F-01 MUST 3）：不再出现第二层 Diff/风险/备份确认弹窗；后台统一完成 plan、脱敏 Diff、备份、apply、复读验证、恢复点和 receipt。
- 授权边界：仅授权本次勾选集合的配置写入；不授权隐式写入（除「配置」按钮外无其他隐式写入）；写入只涉及 d-token-owned 字段，不写 cc-switch，凭据不落盘、不入日志。
- 逐 Agent 独立结果：即使一次点击批量授权，每个 Agent 仍是独立事务（成功项不被失败项回滚，失败项单独重试）。
- 与「Review-gated automatic_apply_allowed=false」旧口径的关系：旧版本包中 review-gated 整批确认/整批逆序回滚文本已由 2026-08-12 同步轮删除/改写；批准设计采用一次授权直接执行 + 逐 Agent 独立事务（详见 §19 冲突清单）。

### 3.4 Route / MCP-Hook / Detection-only 分层矩阵（诚实能力分层）

| 层级 | 定义 | 证明什么 | 允许显示 | 禁止冒充 |
|---|---|---|---|---|
| **Route（路由接入）** | 配置写 d-token-owned 字段后，真实模型流量经过 d-token loopback listener → runtime → 配置上游 | 模型流量经过 d-token；routed evidence（真实请求、route decision、Ledger/metrics、receipt） | `connected`/`verified`/`active`（有对应证据时）；「已跟随 cc-switch」或「配置成功」 | 不得用「已配置」冒充「已连接/已路由」 |
| **MCP / Hook / Skill（工具/上下文能力）** | managed MCP command、Hook/Plugin、Skill 的 configure/clear 事务（如 `headroom_*` MCP 工具、Agent MCP/Hook 分支） | 工具或上下文能力接入，不证明模型流量路由 | `configured`（有 receipt 时）；「配置成功」仅指工具/上下文配置成功 | 不得把 MCP/Hook/Skill 配置成功冒充路由成功（P0.0.10.9） |
| **Detection-only（仅检测）** | 真实检测（注册表识别到文件/进程/配置）+ 官方架构证据（官方文档/源码证明该 Agent 本机无路由适配面或按官方能力仅支持检测），无适配 adapter 或 fail-closed 拒绝写 | 发现存在 + 官方能力事实；无配置写入、无路由证明 | `detected`；「已检测/不支持本地路由」+ 原因 + 重试 | 不得显示 `supported/connected/configured`；不得用通用 JSON 递归猜字段或写 metadata block 冒充路由 |

矩阵事实（IMPLEMENTATION_STATUS §8、PLAN §0）：Rust 检测注册表覆盖 **23 个 Agent**，每个检测项有显式 transport capability decision；**11 个已验证 adapter** 可受控配置/清除（Claude Code、Codex、Cline CLI、Goose、Kilo Code、OpenCode、OpenClaw、Gemini CLI、Roo Code、Continue、Aider）；其余 12 个保持 **detection-only/fail-closed**。支持 adapter 的上游解析覆盖 Claude Code、Codex、Cline、Goose、Kilo Code、OpenCode、OpenClaw、Gemini CLI、Roo Code、Continue、Aider。前端静态 6 项 Agent catalog 已删除，Install 只消费 Rust 发现结果；每行统一显示「配置」「清除」，未验证操作只返回无写入状态。

**目标 Agent 矩阵（冻结）** = 本轮要求 ∪ 目标规格 v2.1 ∪ Rust 正式注册表 ∪ cc-switch 实际 app type 的并集；逐项核对四来源差异，**单一来源出现的 Agent 也必须入矩阵**并按官方能力层（§6.3）闭环；N5 以 cc-switch 全量 app type 并集核对矩阵覆盖（不仅 openclaw）。

新增/边界 agent 处理：VS Code Copilot Chat 必须由扩展目录证明安装，不能仅凭共享 `settings.json` 误报；其网络代理调试设置在 d-token 无 HTTP CONNECT/forward-proxy 时不得冒充模型上游配置；Cortex `connections.toml` 只参与文件 metadata fingerprint，严禁读取内容。

### 3.5 unsigned prerelease

- 发布语义（批准设计 GOAL 11；PRD P0.0.7.7、PLAN §0、FINAL_BLUEPRINT §2 的旧口径按批准设计修正）：macOS Developer ID/notarization/Gatekeeper 与 Windows Authenticode 是**正式签名发布**的前提；**本轮完成语义**：同 commit/source digest 双平台完整功能与安全证据全绿、明确标注 `unsigned`/`unnotarized` 的公开 prerelease **即算本轮发布完成**（签名不阻断本轮完成，但产物不得宣称已签名/公证/系统信誉）；`distribution_ready_public`（公开签名分发就绪）保持 false，不构成完成阻断。
- 当前公开分发路径：同一 tag/同一 commit 的 GitHub **公开未签名 prerelease**；只有聚合 workflow 中 macOS ARM64 与 Windows x64 两个 reusable job 同时通过，并附带 SHA-256、SBOM、license、安全扫描和 provenance 证据时，才创建/更新 prerelease；产物明确标 `unsigned`/`unnotarized`，不宣称签名、公证或系统信誉（`.github/workflows/public-unsigned-prerelease.yml`）。
- 判定规则：`generate_release_gate_status.py`（fail-closed）聚合任一关键证据缺席/失败时为 blocked，任何文案/README/PR 不得写「可正式发布」；三种结论（正式签名发布 / 未签名技术预览=本轮完成 / blocked）只在最终候选验证完成后由主代理基于机器校验后的证据表判定并落盘。
- 候选结论判定表（FINAL_BLUEPRINT §2，按批准设计修正）：
  | 结论 | 必要条件 |
  |---|---|
  | 正式签名发布 | 同一候选 commit G0–G8 全 passed + macOS Developer ID/notarization/Gatekeeper + Windows Authenticode + 双平台同 commit fan-in ok=true；可宣称已签名 |
  | 未签名技术预览（**本轮发布完成**） | 同一候选 commit/source digest 双平台完整功能与安全证据全绿（N0–N10 全部绑定 `C_final`），macOS/Windows artifact contract 与同 commit fan-in 证据完整；产物明确标 unsigned/unnotarized，不得宣称已签名 |
  | 不可发布（blocked） | 任一关键证据缺席或失败 |

### 3.6 配置/同步结果状态词表（冻结）

- **配置/同步结果状态词表（冻结）**：`未安装`、`未选择`、`正在配置`、`配置成功`、`配置失败`、`工具能力已接入`、`已检测/不支持本地路由`、`正在同步`、`同步成功`、`同步失败`；资产清单状态另允许 `已安装/未安装/已配置/未配置`（MUST 1、§11.2）；页面证据说明允许用户语言。
- 内部码（`detected`/`configured`/`connected`/`verified`/`not_verified`/`healthy`/`active`/`supported`、adapter、route、credential reference、`needs-source-fix`、来源冲突等）不直接进普通 UI，只出现在技术详情/Doctor/日志（P0.0.10.8、§6.1）。
- 用户文案示例：Codex 无 routed 报告时显示「配置成功；尚无真实请求验证」（内部码仅详情/日志），不得把 `configured`/`not_verified` 原样显示。
- 验收：A8（§16.1）与 N6（§16.2）检查普通页面仅使用上述词表/用户文案，不得以内部状态或内部码冒充。

### 3.7 Onboarding 五段（冻结）

- **Onboarding 五段（冻结；verify 不是独立用户段）**：1) 欢迎简介 → 2) 有呼吸感动画和可视进度的服务启动 → 3) 扫描已安装 Agent → 4) 用户调整默认勾选并一次点击「配置」（内部 plan/apply/readback/verify、逐 Agent 独立结果）→ 5) 开始使用。
- 引用点：§2.1.10（MUST 首次引导）、§10.2（OpenCode 两链入口）、§14.1（PR-14）、§16.2 N6、§18 O3、§19 C4、§20 术语；五段为批准设计口径，6 步旧流程按本段对齐。

## 4. 前端边界（2026-08-12 用户最新补充：前端严禁大范围重构）

1. **不重写 shell**：侧栏/Topbar/StatusBar/通知中心/操作对话框（OperationDialog/NotificationCenter）结构保持现有实现；仅修复点击命中/展开位置偏移、交互性能与 reduced-motion 等小范围缺陷（GOAL 11），不改变 IA。
2. **不换技术栈/设计系统**：保持 React 18 + TypeScript + Vite + Tauri 2、现有纯色 shell、视觉风格、动画语言、中/英文、Reduce Motion、Windows/macOS 共用 UI；不引入新 UI 框架或设计系统。
3. **不重排八页 IA**：正式一级页面固定为 总体概览、安装与连接、代码图谱（实验性）、分析、视觉压缩（Beta）、Doctor、变更与回退、设置；`Advanced`/`LogsSecurity`/`Compatibility`/`SmartLearning` 等保持工作台子页面或内部组件，不得扩张为工程化顶级导航；不为 Headroom/LeanCTX/CCR/Context Bus 另建用户级独立页面（P0.0.8）；Context/代码图谱入口按 P0.0.8 表落位。
4. **只允许小范围接线/修复/文案/状态/交互**：具体为（FINAL_BLUEPRINT PR-4 范围）——顶部重复标题/留白、StatusBar 920 横滚（flex-nowrap+overflow-x-auto 风险）、reduced-motion chevron transform、代码图谱 experimental badge、Heatmap/Changes/Doctor 文案统一、Analytics 日志高度已压缩不得再大改；Onboarding 首次使用入口（右上角重复进入）只做 shell/topbar 连接点接线。
5. **架构收敛主要在 Rust**：状态机、事务、合同、canonical pipeline、read model、capability 判定、receipt 聚合全部收敛在 Rust crate（`dtoken-types`/`dtoken-core-api`/`dtoken-agent-bridge`/`dtoken-config-patcher` 等）与 Tauri native command；React 只消费 Rust 合同与 receipt，不自行推断接入状态、不承载业务规则。
6. **前端只读真实数据**：Dashboard/Doctor/Analytics/Logs/Export 真实数据只来自 d-token-owned runtime、Tauri command 或 Event Ledger；禁止 mock/fixture 伪成功；无完整 source/freshness/action-result 证据时显示 unavailable/partial/preview/unknown，禁止示例数字与绿色状态补齐（AGENTS.md §6）。
7. **UI 动作必须翻译成用户任务**：安装、连接、配置、预览变更、应用、验证、诊断、修复、查看状态、查看日志、导出、回退；错误只面向结果（§2.1.8）。

## 5. 总体架构

```text
OpenCode / Claude Code / Codex / Cursor / Cline / OpenClaw / 其他 AI coding agent
→ d-token loopback listener（127.0.0.1:18001 等，仅绑定 loopback）
→ d-token-owned Rust runtime（dtoken-core-api / dtoken-proxy）
→ canonical pipeline：route decision + compression / context optimization
   （FrozenPrefix → TransformableWindow → ProtectedTail；九类 codec；CCR 可逆恢复）
→ 配置上游：agent_managed_auth（原 Agent 持有认证）/ dynamic_local_proxy（OpenCodex 等动态代理）/ cc_switch_managed（cc-switch 只读跟随）
→ redacted Event Ledger / metrics / Dashboard / Doctor / App（八页 read model）
```

代码所有权（AGENTS.md §3、IMPLEMENTATION_STATUS §2/§3）：

- `code/apps/desktop/`：唯一正式桌面 App（Tauri 2 + React 18 + TypeScript + Vite）；`src/` 为当前 React 产品 UI，`src-tauri/` 为 native commands、平台适配与打包。
- `code/crates/`：18 个 Rust 业务 crate（dtoken-types、dtoken-config、dtoken-router、dtoken-compress、dtoken-context-store、dtoken-process、dtoken-doctor、dtoken-event-ledger、dtoken-preprocessor、dtoken-headroom-adapter、dtoken-project-policy、dtoken-runtime-pipeline、dtoken-config-patcher、dtoken-agent-bridge、dtoken-codebase-memory、dtoken-learning、dtoken-core-api、dtoken-proxy）+ 1 个 Tauri app crate（d-token-desktop）。
- `code/runtimes/headroom/`：Headroom-derived 能力 provenance 与迁移边界，不是 Python sidecar、不是外部 Headroom 服务依赖。
- `code/packages/sdk/`：薄 TypeScript SDK，与 Rust/HTTP/MCP 共用 compression decision parity contract。
- `support/`：测试、脚本、fixture、fake-bin、CI 与审计账本；`support/artifacts/` 仅作 gitignored 运行时 evidence 输出目录，可版本化 schema 必须放 tracked test/fixture/schema 路径。
- `docs/`：正式文档（PRD=产品真理源、PLAN=唯一 Living Plan、IMPLEMENTATION_STATUS=代码事实、版本包=v0.0.1 执行分解）；`docs/history/`、`docs/archive/` 为追溯材料。

非生产落点：`code/UI design for project/`（设计原型）、`code/apps/desktop/src-old/`（旧前端）、`code/packages/sdk/`（预留目录）。

关键事实（IMPLEMENTATION_STATUS §4/§6）：`dtoken-core-api` 提供 OpenAI Chat/Responses、Anthropic Messages 入口与 /health、/dashboard/payload、/stats、/stats-history、/metrics、Provider/Route/Instance、Doctor、Logs/Export、Event Ledger、CCR、memory/learn、output shaping、项目策略、Headroom-derived 状态面、managed RTK 生命周期、Optical shadow 等 API；端点存在只证明实现面存在，不等于稳定 Beta。

## 6. 共享 Rust 合同 / 类型 / 状态机

Rust-owned 合同是前后端唯一事实源（PRD P0.0.3），React 只消费这些合同与 receipt，不根据 `capability`、文件存在或静态 `configured=true` 自行推断已接入。

### 6.1 配置合同类型（PRD P0.0.3 原文契约）

```text
AgentConfigSnapshot
  agent_id, config_targets, current_upstream, provider_id, model,
  protocol, endpoint, config_fingerprint, credential_state, source_revision

UpstreamSignature
  provider, model, protocol, normalized_endpoint, profile_or_app_scope
  （禁止 token、cookie、Authorization 和完整 auth 配置）

AgentConfigPlan
  operation(configure|clear), target_files, redacted_diff,
  before_sha256, after_sha256, backup_or_rollback_id,
  runtime_reload_requirement

AgentConfigReceipt
  planned|applied|verified|rolled_back|failed,
  per-agent result, source revision, verification evidence, next action
```

- 前端不基于 `capability`/文件存在/静态 `configured=true` 推断已接入；任何 plan 在 review 期间发现 source revision 或 fingerprint 变化，必须立即失效并重新发现。
- **ConfigTransactionState（逐 Agent 状态机，冻结）**：`discovered → selected → planned → backed_up → applied → readback_verified → capability_verified → completed | rolled_back | failed`；每 Agent 独立推进，不跨 Agent 共享原子事务（§8.1）。
- **逐 Agent 事务 evidence 十字段（冻结）**：`transaction_id / source_revision / before_digest / redacted_diff_digest / backup_manifest / planned_after_digest / readback_digest / capability_evidence / receipt_id / rollback_receipt_id`；批量配置仅 UI 编排，不是共享原子事务。
- **WritePolicy（冻结）**：`AlwaysReadOnly`（OMP、cc-switch，只读不写）、`UserSelectedManaged`（用户勾选的其他选中 Agent，一次点击授权事务化写入，§3.3）、`Unsupported`（Detection-only，fail-closed 不写，§3.4）。
- 状态语义固定（P0.0.3 表）：`detected`（发现文件/进程/配置）、`configured`（配置含 d-token-owned 设置，未证明真实流量）、`connected`（至少一条真实请求成功经过 d-token）、`verified`（当前连接/路由/能力有有效证据）、`healthy`（Doctor 检查项当前 revision 通过）、`active`（当前 Runtime 正在使用该 route/capability）、`supported`（代码声明支持，不代表当前环境已验证）。任何卡片/Doctor/Dashboard 不得把 `detected`/`configured`/`supported` 显示成 `connected`/`verified`/`healthy`；普通页面只显示 §3.6 冻结状态词表，上述内部状态仅技术详情/Doctor/日志。

### 6.2 canonical Context IR 与 NodeAction（GOALS 14、RUNTIME-7）

```text
一个请求 → 一个 canonical Context IR → 每个 IR node 恰一个终态动作：
  Preserve | Reuse | HeadroomSemantic | RtkCommandOutput | Optical
```

- RouteDecision 是 route/provider/model/protocol/API revision/capability/deadline 的唯一权威；所有 stage evidence 绑定同一 decision digest（GOALS 15）。
- **single dispatch 必须包含 Native Responses（`/v1/responses`）**：Native Responses 当前绕过 canonical（parse-once/type-state）发送路径是已确认 Blocker，必须纳入 single dispatch 改造，禁止以「pending marker/no-op/type existence」冒充完成（verified-facts-handoff 10；RUNTIME-7）。
- Reuse/memory 必须在 transform plan 前进入 IR；发送体必须等于最终验证体（RUNTIME-4、CONTEXT-4）；删除 post-validation injection（`inject_memory_into_body`）。
- ReadyToSend 只能由 runtime pipeline 内部基于 artifact-derived ValidationReceipt、RecoveryReceipt 与最终 envelope 校验密封；调用方布尔值/任意 digest/时间戳哨兵不能铸造状态（GOALS 16、RUNTIME-8）。
- Receipt 类型：`OptimizationReceipt`（PRD P0.0.7/§7.2 最小字段，Local/Provider/claimable 分离）、`ValidationReceipt`（artifact-derived）、`RecoveryReceipt`（request/source/decision-bound）、`SendCommitReceipt`（transport-observable，生产 transport 必须实际消费）、`RelayHitReceipt`/`CacheReuseReceipt`（Relay/cache 不计入 compression savings）。
- 四维独立 accounting：compression / command-output / provider-cache / relay 独立 typed bucket，禁止求和成一个 savings；`RequestLog` 需补充 Provider usage 字段（input/output）后投影 receipt；缺 usage 保持 unavailable（RUNTIME-9、FINAL_BLUEPRINT 缺口 1）。
- **CacheAligner 是 canonical pipeline 真实组件**（GOAL 1、§2.1.22）：进入同一 canonical Context IR / production path，真实 cache alignment + 真实 receipt；节省只计入 provider-cache 分账；禁止仅 accounting/metadata/no-op 冒充完成。

### 6.3 AgentCapabilityMatrix 三层完成定义

对应 §3.4 分层矩阵的完成度定义（每层独立验收，不得互相冒充）：

1. **Route 层完成** = 该 Agent 的 detector（真实发现）→ parser（解析当前上游）→ executor（配置/清除事务）→ readback（复读验证）→ routed evidence（真实请求经 d-token：`agent_id`、provider/model/protocol、route verified、请求匹配数、Ledger/metrics、失败分类、恢复状态）→ restore（精确恢复原始字节/hash/权限）六维全闭环。
2. **MCP/Hook/Skill 层完成** = managed MCP command / Hook/Plugin / Skill 的 configure/clear 事务闭环（preview 只返回脱敏摘要、apply 绑定双 hash/备份/receipt/复读验证/失败自动回退），且明确不提供模型上游 route evidence。
3. **Detection-only 层完成** = 官方架构证据（官方文档/源码证明该 Agent 本机无路由适配面或按官方能力仅支持检测）+ 真实检测（注册表识别到文件/进程/配置，capability decision 显式 detection-only/fail-closed）+ 诚实 UI（显示「已检测/不支持本地路由」+ 原因 + 重试）；不猜字段、不写 metadata block 冒充路由；**不要求 routed-flow**。

支持矩阵 Agent 按其官方能力层（Route/MCP-Hook/Detection-only）分别闭环即满足 P0.0.10.2；任一要求适配 Agent 在其所属能力层的完成定义（§6.3.1–§6.3.3）未满足即 `release_ready=false`；Detection-only 层不要求 Route 六维。

- **AgentCapability 字段（冻结）**：`agent_id / display_name / official_capability / discovery / configuration / authentication / change_detection / verification / rollback / platforms / provenance`；每项必须记录官方来源/版本/配置路径/字段 ownership/write policy/auth owner/watcher/正常失败恢复验收/UI 状态（§3.6 词表）。

## 7. Canonical Request Pipeline

### 7.1 配置写入流水线（P0.0.3 / GOAL §4.3）

```text
detect → parse current upstream → plan → redacted diff
→ backup → user confirm（点击一次「配置」即授权）→ apply
→ reread/verify → receipt；失败则精确回滚原始字节
```

- 配置只写 d-token-owned 字段、provider 或 profile；清除只删除带 manifest/hash ownership 证据的内容，保留用户已有 Provider、模型和凭据，必须幂等。
- OpenCode、OpenClaw 等多供应商 Agent 使用专用 multi-provider adapter；未知格式不得套用其他 Agent 的字段（P0.0.3）。

### 7.2 请求处理流水线（目标规格 §8 与 RUNTIME-7 合并口径）

```text
Agent Identification
→ User / Project / Session Scope
→ 请求进入 d-token-owned listener（127.0.0.1）
→ 单一 canonical Context IR（parse-once、type-state lowering）
→ planner/arbitration（每 node 恰一终态动作）
→ compression / context optimization（FrozenPrefix → TransformableWindow → ProtectedTail；
   九类 codec：JSON/log/search/build/diff/text/HTML/config/code-compressor；Net-cost Gate、负收益原样透传）
→ CCR durable write_intent → receipt → marker → restore（pre-send 可逆恢复）
→ Reuse/memory 在 transform plan 前进入 IR（删除 post-validation injection）
→ RouteDecision（route/provider/model/protocol/API revision/capability/deadline 唯一权威）
→ 显式解析 Provider、模型、协议和 endpoint → 转发上游并采集 Provider usage
→ ReadyToSend（仅 runtime pipeline 密封）→ SendCommitReceipt 消费
→ 写入 OptimizationReceipt 与 Event Ledger
→ Dashboard / Analytics / Doctor 展示同一事实投影
→ 外部配置变化后按授权策略重新发现、同步或请求用户决策
```

- 一个请求只有一个 RouteDecision、一个 canonical Context IR、每 node 一个终态动作；HTTP/Rust/MCP/TS/Proxy 共用同一 resolved production pipeline（P0.0.7.2）。
- 所有公共 surface（HTTP、MCP、TS SDK、Proxy、Native Responses）对同一 fixture 的 decision/reason/artifact parity 必须字节一致；发送体=验证体。
- 失败降级：未知/secret/风险/负收益输入精确 passthrough；压缩失败回退原始请求；Relay 失败回退正常读取；reload 失败保留 last-known-good；OMP/cc-switch 变化用原子 route snapshot。

## 8. 逐 Agent 配置事务与热同步

### 8.1 逐 Agent 独立事务（P0.0.10.5、F-01 MUST 5）

- 每个 Agent 拥有独立 plan、backup、apply、verify、rollback 和 receipt；失败只回退该 Agent 本次未完成的写入，其他已成功 Agent 保持成功；失败项显示简短原因和单独「重试」，不得整批回退成功项。
- 一次点击「配置」批量授权不改变逐 Agent 独立性（§3.3）。
- 真实写入操作必须具备用户可读摘要、diff、backup、verify 和 rollback evidence（AGENTS.md §4.8）；确认即一次点击「配置」授权，无第二层确认弹窗（§2.1.3）；证据从详情、日志和「变更与回退」查看。

### 8.2 热同步（hot sync，PR-13 / 版本包口径）

- cc-switch 热同步：只读 watcher（DB/WAL/SHM + 750ms debounce + 单飞 + 5–15s polling fallback）自动跟随 d-token route/runtime；每次同步后台生成备份、复读验证、receipt 与 Event Ledger 记录，一键恢复；原子 route snapshot（旧请求旧快照、新请求新快照）；reload 失败保留 last-known-good（PRD P0.0.4）。
- **热同步 review-gated 证据与授权门**（FINAL_BLUEPRINT checklist 34/PR-13）：自动同步必须带授权门（首次 hash-bound 授权后仅自动更新 d-token 自有 route/runtime，绝不写 cc-switch）；授权/证据/receipt 必须持久化，未授权不得自动写。
- **完整热同步循环（冻结）**：独立 watcher/coordinator（debounce+single-flight）→ 重发现 → 与 last-known-good 比较 → 分类变化 → 自动更新 d-token-owned route 或首次授权 scope 内受管字段 → 复读 → 能力复验 → receipt/ledger；agent-managed auth 只更新 credential reference fingerprint（不复制凭据）；dynamic proxy 保持 request model passthrough（模型不固定为安装时值）；cc-switch 只更新 d-token route snapshot；旧请求用旧 snapshot、新请求仅在新 snapshot 验证后切换，失败保留 last-known-good。
- 逐 Agent 配置热同步：已配置 Agent 再次参与配置时先检查当前状态，无变化只重新验证不重复写入（幂等，P0.0.10.4）；MCP/Hook 配置与 route 配置分层热同步（§3.4）。
- pending 变更回滚到稳定快照会被清除，下一次变更须重新完成 debounce（cc-switch watcher 语义）。

## 9. 5% / protected bytes / recovery

### 9.1 protected bytes（保护字节）

- 压缩链路必须保护的内容（protected content）：**system/developer 消息、当前用户消息、代码/diff、secret、tool schema/arguments、开放工具状态零字节变化**；代码块、标识符、URL、错误信息、尾部、exact 内容、secret/凭据、混合行尾（CRLF 保留）；`FrozenPrefix → TransformableWindow → ProtectedTail` 结构；九类 codec 的语义/结构校验与字节精确还原（IMPLEMENTATION_STATUS §8 九类 codec 本地 probe 均 active，每类 ≥128 个 property/semantic/rollback cases）。
- 保护规则：未知/secret/风险/负收益输入精确 passthrough；工具 schema 规范化（PR-E1/E2）与自动 `cache_control` 放置（PR-E3）不改变客户缓存意图；`claimable` 与物理节省分离（§3.1）。
- 展示规则：Local Token 与 Provider Token 分离统计；缺 Provider usage 不能当作 0；`protected_content` 为 OptimizationReceipt 最小字段之一（目标规格 §7.2 receipt 字段）。

### 9.2 recovery（可逆恢复）

- CCR（Context Compression Recovery）：AES-256-GCM 加密、TTL/quota、ACL/DACL、request/source/decision binding、journal、崩溃恢复、精确 retrieve/delete；`write_intent → pending_retained → terminal` 与 `delete_intent → deleted` journal；Unix `0700/0600`、Windows protected DACL；跨 JSONL/密文/Event/SQLite 原子 transaction 与自动 tool-call retrieve 仍是 G3 缺口（CONTEXT-2、IMPLEMENTATION_STATUS §8）。
- Recovery Pointer / Manifest：允许用户在授权边界内定位被省略内容；文件变化、TTL 到期、权限变化或来源不可信时立即失效；Relay/Recovery 与压缩节省不得双计（P0.0.5）。
- 配置恢复：ConfigPatcher backup/restore 与 exact restore 证据（双 hash、备份、receipt、复读验证、失败自动回退）；Agent 配置与 cc-switch 验收后恢复原始字节/hash/权限，不泄漏凭据（GOAL §4.3/完成定义）。
- 发布证据要求（CONTEXT-2 acceptance）：每个 pre-send fault 原文只发送一次；post-commit 无 retry；过期/错 scope 拒绝；跨存储 crash points 与 Windows MSVC durability 证据。

## 10. OpenCode Free / Go dsv4flash 真实工程

### 10.1 已通过的真实 routed 证据（G7，历史基线）

- 2026-08-04 G7 真实 OpenCode 已在当时候选通过：报告 `support/artifacts/opencode-real-routed/real-routed-20260804T112429Z-76558.json`（schema v2，candidate_id=G7，status=pass/exit 0）。OpenCode 1.14.51 以本机正常配置（`normal_user_state_used=true`、`credential_mode=explicit_none`）使用 `opencode/deepseek-v4-flash-free` 免费模型，route/provider 均 verified，10 条匹配请求全部成功；compression 输入 139116→137858 tokens、physical_delta=1258、`claimable=false`（attribution_basis=none，物理节省不冒充可声明收益）；Ledger 4 类（events 20/route_decisions 10/request_logs 10/ccr_metadata 14）；CCR retrieve→delete→repeat-delete 全生命周期通过；配置事务 `change_required=false`、前后 SHA-256 一致；privacy 六项全 false；runtime 已停止；报告未读取或输出任何 secret（docs/PLAN.md §0.12）。
- **该 G7 报告是历史证据，不得绑定最终候选 `C_final`**（过期证据隔离清单，蓝图 §8/版本包 EVIDENCE.md）。

### 10.2 N3 最终候选要求（FINAL_BLUEPRINT §8.4、checklist 20，按批准设计修正）

在 `C_final` 重跑真实 routed-flow，且包含两条长工程验收，**两链均绑定 dsv4flash**：

1. **OpenCode Free 真实 routed path**：模型明确为 `deepseek-v4-flash-free`（dsv4flash 免费档）；真实长时/长上下文工程 workload（同候选 `C_final` + source digest 绑定），至少一条完成真实工程编码、构建、运行、压缩、恢复和证据全流程；same-model baseline/optimized paired 对比；旧 bounded replay 不替代。
2. **Go dsv4flash 真实工程**：模型为 dsv4flash（Go 档，与 Free 同族 dsv4flash）；同样真实 routed path、长时/长上下文工程 workload、`C_final`/source digest、same-model baseline/optimized paired、exact restore。
3. **两链统一入口与证据**：均从真实安装后的 OpenCode App UI 启动 service → 扫描已安装 Agent → 勾选 → 一次配置 → 复读验证（Onboarding 五段 §3.7：欢迎简介 → 有呼吸感动画和可视进度的服务启动 → 扫描 → 调整默认勾选并一次配置 → 开始使用，verify 归配置段内部）；证据含 UI/AX 截图与坐标、Agent/provider/model/protocol/route、tokens/rate、protected digests、build/run、Receipt/Ledger/fallback/restore、原配置 bytes/hash/mode；不保存完整 prompt/response/source（AGENTS.md §5）。
4. 约束：真实 `OpenCode -> d-token 127.0.0.1:18001 -> 配置上游` 链路，关联 runtime identity、route/provider、Event Ledger 和脱敏报告；不读取或打印 `auth.json`、密钥、token、cookie、完整 prompt/response 或原始模型输出；OpenCode 是当前第一条真实 Agent 验收入口，不是产品信息架构中心。

### 10.3 Codex 首要正式场景（P0.0.7、P0.0.10.5、RUNTIME-6）

链路 `Codex → d-token:18001/v1 → ocxlocal:10100/v1 → 动态模型上游`；插入 d-token 前后保持 Responses 协议、无 OpenAI 认证语义、请求 model 透传（`ProviderWireApi::OpenAiResponses`、`models=[]` 允许请求 model 动态透传），不得把模型固定为安装时值；Codex routed-flow 不能由配置事务精确恢复替代，必须有独立真实请求报告（`agent_id=codex`、provider/model/protocol、route verified、请求匹配数、Ledger/metrics 计数、失败分类、恢复状态）；未有报告时 Codex 只显示 `configured`/`not_verified`。

## 11. 八页 read model（产品信息架构与真实数据投影）

正式一级页面（不可重排、不可新增工程化一级导航）：

1. **总体概览（Overview）**：累计节省与 累计/当周/当月/当天 四区间；所有值来自统一 runtime/Event Ledger/设置证据；真实 0 与 unavailable 区分；Codex/Claude 5h 与 7d 额度摘要（OAuth usage allowlist，只读不保存原始账号/凭据）；Dashboard read model（增量 summary、single-flight、slice revision）。
2. **安装与连接（Install & Connections）**：完整 Agent 清单（已安装/未安装/已配置/未配置/配置失败）、默认全选可取消、一次点击配置、逐 Agent 独立结果、cc-switch 只读跟随、全局启停（d-token route/runtime 全局启用/停用开关）；Onboarding 首次使用入口（右上角可重复进入）；MCP/Hook/高级安装为工作台子页。
3. **代码图谱（实验性）**：Code Map/Signatures/索引状态、资源（CBM 五版本 manifest）与导入、逐版本索引/图查询/调用链/依赖/影响分析；**不内置下载/更新 CBM**——提供官方资源+安装/接入说明，用户自行下载，d-token 只本地导入校验，不写用户仓库；无资产显示 unavailable+原因（§12）；不得重排/扩张。
4. **分析（Analytics）**：日志子页（不新增一级导航）；逐请求真实 d-token 信息：时间、Agent、Provider、模型、协议、route、状态、request id、Local Token、Provider Token（有上游 usage 证据才显示）、压缩前后 Token、节省 Token、耗时、transforms、失败分类、脱敏详情/receipt；**逐请求 cost：压缩前/后费用、差额、币种、单位、价格来源/更新时间/evidence；仅 Token 与价格证据同时存在才显示数值，否则 unavailable/partial/not verified，绝不显示 0；分页与导出字段同 receipt**；Agent/Model/Protocol/Route/Provider/Chain/Error 与 30 日 daily 聚合由 Rust read model 提供（不再由最近 20 条摘要重建）；分页显示 source/freshness/revision；最近 10,000 条上限；`latency_ms` 只作 compression-only 证据。
5. **视觉压缩（Beta）**：研究+计划+小范围迭代；不得扩大为普通图片压缩、不得提供手动 active 控件；**只读三态（冻结）**：① 精确能力不可用 + 原因（能力门不绿 / 该请求 PreserveText / 未启用）② 观察中（shadow，不计 actual）③ 已自动启用（exact 全门绿，展示当前证据）；中英文任务语言，无手动 active 控件；exact 全门绿由 planner 自动 active，任一不绿 PreserveText（§2.1.15）；N6 覆盖三态、重启一致、Reduce Motion；不得把 shadow/metadata-only 说成 verified。
6. **Doctor**：问题诊断与修复；自动修复仅低风险、可逆、可验证动作；真实数据来自 runtime/Tauri/Event Ledger；**每项检查含 source/executor/candidate/revision/collected_at/freshness/action result；陈旧或无证据显示 unavailable/unknown；修复后自动复检并展示修复前后结果，支持手动重检**（N6 场景验收）。
7. **变更与回退（Changes & Recovery）**：真实写操作的用户可读摘要、diff、backup、确认、verify、rollback evidence；**restorability + 验证时间；回退前执行 reverse-safety backup；每次回退有独立 rollback receipt（目标 revision、前后摘要/digest、readback、失败分类、再次恢复入口）；不可恢复项禁用回退并说明原因**；CCR retrieve/delete、Recovery Pointer；一键恢复。
8. **设置（Settings）**：策略/价格/隐私/保留/启动（policy/pricing/privacy/retention/startup），非 chooser（不提供 Provider/上游/模型/协议/endpoint/profile 选择，§2.2.3）；价格维护已有日志模型的输入/输出价格、币种、计价单位、来源和更新时间，支持编辑、保存、删除；缺价格不能当作 0；只有 Token 与价格证据同时存在才展示压缩前/后费用与差额，缺证据显示 unavailable/partial/not verified。

read model 事实：8/8 页面真实 handlers、数据全来自 d-token-owned producer、无生产 mock（verified-facts-handoff）；onboarding 6 步 vs 五段文案差异（B-2）、service 启动可视化（B-1）、native 无阶段回调（H-2）为已知缺口（进计划，见 §18 开放问题 O1–O3）。性能契约：在线 summary 刷新 p95 ≤ 250ms、离线 fail-fast p95 ≤ 500ms、无 >100ms JS long task、空闲 CPU ≤1%、30 分钟内存漂移 <5%、初始 JS gzip ≤160KB、长日志列表只渲染可见行（PRD P0.2.1/PLAN 性能项）。

## 12. CBM 五稳定版（Codebase Memory 五正式版）

- **五正式版定义**：`v0.9.0` / `v0.8.1` / `0.8.0` / `0.7.0` / `0.6.1`（FINAL_BLUEPRINT checklist 33、版本包 README §3.7）。
- **CBM 不内置下载/更新**：d-token 不下载、不更新、不写用户仓库；提供官方资源+安装/接入说明，用户自行下载，d-token 只做本地导入校验（tag+commit+asset hash/provenance → 导入验证 → Agent MCP → initialize/tools → index fixture repo → graph query/call chain/dependency/impact → exact rollback）；无官方资产 → unavailable+原因（§12 语义）。
- 验证方式：**v0.9.0 与 v0.8.1 使用官方二进制真实验证**；**0.8.0/0.7.0/0.6.1 官方无资产 → source-tag 登记 + fail-closed + 诚实 unavailable（如实记录、不声明兼容、不伪造），不因此阻断本轮完成**；**v0.9.1-rc.1 明确标注 prerelease，不计入五正式版**；官方资产存在但校验失败（tag/commit/archive SHA/provenance 不匹配）才 blocked。
- 交付物：多版本 manifest v2 + 适配器（PR-CBM3/4）；manifest→graph 单版本链路已工作（v0.9.1-rc.1），五正式版兼容是 Blocker，三版无资产按 source-tag 登记 + 诚实 unavailable 处理、不因此阻断（verified-facts-handoff 按 §12 语义修正）；v0.9.1-rc.1 需固定 tag/commit、四平台 archive/binary SHA 与 Sigstore/provenance，不得放宽校验（GOAL 9）；并独立修复官方 v0.9.0 正确 release 目录仍无法配置的 Bug（系统化复现、先补失败测试、再 preview/apply/verify/rollback 与原始 hash/权限恢复）。
- 平台矩阵：v0.8.1 无 windows-arm64 → 在 Windows 平台矩阵中标 `incompatible`（N8）；`v0.9.1-rc.1` 不计五正式版，不得把 prerelease pin 写成五版本兼容。

## 13. CandidateEvidenceEnvelope（统一证据信封）

- **定义**（版本包 VERIFICATION.md §1「证据公共 envelope，所有 N0–N10 必填」）：每个机器 artifact 顶层必须包含：

```yaml
schema_version:
node_id:
status:
reason_codes: []
candidate_commit:      # = C_final
source_digest:         # dirty=false
registry_digest:
corpus_digests: {}
started_at:
finished_at:
platform: {os, arch, toolchain}
producer_revision:
artifact_sha256:
agent_id:                # 必填；不适用显式 null + reason
capability_tier:         # 必填：route | mcp_hook | detection_only；不适用 null + reason
privacy_assertions: {}   # 必填：无 secret/完整 prompt/绝对路径等断言；缺失/不匹配 blocked
# route 类 artifact 亦必填：
route_decision_digest: capability_digest:
# Optical/benchmark/paired eval 还必须包含：
renderer/profile/font/tokenizer/pricing/cache/eval digests:
```

- 不适用字段显式为 `null` 并给 reason，不能省略后被当作通过。
- **证据卫生规则**（EVIDENCE.md §4）：artifact 只能由其 tracked producer 生成；producer/schema/registry/corpus digest 必须进入统一 envelope；聚合器必须消费显式 artifact path manifest 并逐项复算 SHA-256，禁止目录扫描/`latest_file()`/mtime/文件名/`status=pass` 选证；候选变化即失效，`candidate_commit`/`source_digest`/registry/corpus/eval/platform 任一不匹配都 blocked；聚合时 `privacy_assertions` 缺失或与 producer 声明不匹配 → blocked；route 类 artifact（Route 层证据）缺 `route_decision_digest`/`capability_digest` → blocked；截图/Computer evidence 必须来自主代理真实 Tauri/cc-switch 操作，附窗口/坐标/时间，浏览器 preview 不能证明 native/runtime。
- 运行 artifact 统一位于 `support/artifacts/final-candidate/<C_final>/`（gitignored）；tracked schema/registry/corpus/template 位于 `support/fixtures/`、`support/schemas/` 或 `support/scripts/`。
- 过期证据隔离清单（不得绑定 `C_final`）：2026-08-04 G7 OpenCode real-routed、旧 DMG（c124d9f/60f7d063 ad-hoc 审计）、任何浏览器 preview 截图、任何非 `C_final` commit 的测试报告。

## 14. DAG / waves / 共享文件串行原则

### 14.1 唯一执行 DAG（FINAL_BLUEPRINT §5/§8、版本包 EXECUTION.md §4 一致口径）

```text
P0（preflight baseline freeze）
→ 功能切片 PR-1..14（含 PR-10 codex-toml / PR-11 openclaw / PR-12 responses / PR-13 hot-sync / PR-14 onboarding-5seg + PR-CBM3/4）
→ C0A（schemas）→ C0B（sealed receipts）& C0C（registry/corpus + C0D 117-start）
→ C1C（single dispatch，含 Native Responses）→ C2..C7（stage 内核，含 C4U RTK unblock 前置）
→ C8A（117 proof）→ C8B（real routes/platform）→ V-1（版本统一 v0.0.1）
→ PR-9（verification orchestration）→ C8C（gate wiring）→ N0（候选全量质量门）
→ N1..N8 + N10（并行）→ N9（final gate/status 聚合）→ Merge main + 发布报告
```

- **唯一 DAG**：功能切片→C8B→V-1→PR-9→C8C→N0→N1..N8+N10→N9；无其他平行收口路径；N10（U0–U9）是独立发布阻断（任一子门缺 → N10=blocked）。
- V-1 前不得生成最终候选；V-1 后新证据只能绑定 v0.0.1 `C_final`。

### 14.2 波次表（wave）

| Wave | 并行内容 | 出口门 |
|---|---|---|
| W0 | P0 preflight + baseline freeze | 主代理确认冻结 commit |
| W1 | provenance 研究（代码只读）+ PR-1/2/3/4/6 + PR-10/11/13/14 | 各自自证门 + 报告落盘；共享 NOTICE 由主代理串行合并 |
| W2 | PR-C0A；PR-CBM2/3/4；PR-5/7/8 | schema review；无 owner/ID 漂移 |
| W3 | PR-C0B 与 PR-C0C（C0A 后可并行）+ C0D | sealed receipt compile-fail；117 registry/corpus 校验 |
| W4 | PR-C1A → C1B → C1C（含 PR-12 Native Responses） | single production pipeline 与 surface parity |
| W5 | PR-C2/3/4（C4U 先行）/C5A 各链内部串行，四轨道并行 | 每 stage production dispatch |
| W6 | PR-C5B + C6A/B/C | exclusive actions、durability/fault graph、唯一 fallback 终态 |
| W7 | PR-C7A→B→C 后 C5C→D | baseline 后才冻结 SLO；paired eval 后才 active |
| W8 | PR-C8A→C8B | 117/117 proof + real route/platform evidence |
| W9 | V-1→PR-9→C8C | v0.0.1 单版本；完整 verification DAG/gate wiring |
| W10 | N0，然后 N1–N8/N10 并行，再 N9 | 同一 C_final/source_digest；任一缺失 blocked |

### 14.3 共享文件串行原则

- **共享文件不得并发改写**：`dtoken-types`/`dtoken-core-api` 只允许 PR-1 → PR-5 串行；gate 聚合与工作流只允许 PR-7；desktop 前端按 PR-3（安装状态 + `upstream_discovery.rs`）与 PR-4（shell/pages/nav/sidebar/i18n）分区；`support/git-operations.json` 与正式状态文档由主代理合并期统一维护（FINAL_BLUEPRINT §5 文件重叠与串行化原则、§4 通用规则）。
- 每个切片 = 独立分支 + 隔离 worktree + 独立 PR，从 P0 冻结基线或上游切片 commit 切出；禁止直接在 `main` 或他人 worktree 改动。
- 每切片只改「允许文件」列路径；`<切片名>-report.md`（`.ai-bridge/release-iteration/`）为所有切片的隐式允许文件，除此之外不得借 report 规则改其他 `.ai-bridge` 文件。
- Git lifecycle 由主代理集中执行：每个 commit/branch create/switch/delete/rename/merge/rebase/push 前必须 `python3 support/scripts/git_operation_audit.py record ...`（AGENTS.md §9），append-only `support/git-operations.json`；实现 Agent 只改文件、运行范围内验证并写切片 report，不执行 git lifecycle。
- 主代理审查门（每 PR）：diff 范围是否越界 → 隐式依赖是否声明 → secret/绝对路径/raw prompt 是否泄漏 → unavailable 是否被变 0/绿色 → Local/Provider Token 是否混淆 → 测试是否覆盖原失败路径与回退 → 是否引入 chooser/静默 fallback → 是否伪造证据。任一项不合格即返工，不合并。
- 自证门（每切片交付前）：Rust 切片 `cargo fmt --all -- --check` + `cargo clippy --workspace --all-targets -- -D warnings` + `cargo test -p <切片 crate>`；TS 切片 `npx tsc --noEmit` + 相关 Vitest；support 切片契约测试；不得跳过，不得为本切片失败新增/扩大 `#[ignore]`/skip/弱化断言。

## 15. 安全与错误处理

### 15.1 安全边界（AGENTS.md §4、GOAL §4、FINAL_BLUEPRINT §1.3）

1. 真实数据只来自 d-token-owned runtime / Tauri command / Event Ledger；浏览器 preview 只证明 UI。
2. 所有本地服务默认只绑定 `127.0.0.1`；Provider/Route 选择显式、可解释；禁止静默猜测与静默 fallback。
3. 凭据只进操作系统凭据库（Keychain/Credential Manager）；配置只保存不透明引用，禁止明文 fallback；API key/token/cookie/Authorization/auth 文件/完整 prompt-response/源码/大段原始命令输出不进入普通日志、配置、导出或提交。
4. 所有真实写操作必须具备用户可读摘要、diff、backup、verify 与 rollback evidence；确认即一次点击「配置」（无第二层确认弹窗）；失败精确回滚。
5. 进程停止前必须验证 PID、可执行文件、端口所有权与 d-token identity，防止误杀或 PID reuse。
6. `detected/configured/supported` 不得冒充 `connected/verified/healthy/active`；无证据显示 unavailable/partial/unknown。
7. cc-switch、配置源、Provider 凭据探测一律只读；Optical 无手动 active 控件——exact 全门绿由 planner 自动 active，任一能力门不绿则 PreserveText（§2.1.15）；shadow/metadata-only 只证明负路径。
8. 浏览器 preview 只能证明 UI；真实功能必须由 runtime、route decision、Event Ledger、export 或 routed Agent flow 证明。
9. SecretClassifier 统一复用：全部 production active-config/merged-config/route-draft secret validation path 复用共享 `SecretClassifier`（bearer/JWT/Google/GitHub/PEM/cookie/obfuscation 全覆盖），不允许并行弱分类器（FINAL_BLUEPRINT 缺口 5）；runtime JSON/CSV export 不泄漏绝对 config path。
10. 每个 commit/branch/merge 均有 git-operations.json 审计记录（AGENTS.md §9）。

### 15.2 错误处理（fail-closed）

- **歧义 fail-closed**：配置格式未知、候选多义或 schema 未验证 → 不猜测、不提供上游 chooser（Agent 清单勾选是允许的，§2.2.3）、不写；显示失败结果 + 用户可执行说明 + 重试；技术证据进详情与日志（P0.0.10.8）。
- **路由 fail-closed**：缺失/未知/endpoint/model/policy 不匹配均 fail-closed 且不回显输入或凭据（MCP `headroom_compress` 语义）；禁止静默跨 Provider fallback；任何 fallback 需用户策略与 receipt。
- **压缩失败降级**：负收益、secret、未知、风险输入精确 passthrough；失败回退原始请求；Recovery/CCR 失败不影响发送（pre-send fault 原文只发送一次，post-commit 无 retry）。
- **同步失败保留 last-known-good**：cc-switch SQLite 锁定/损坏/未来 schema/缺失 → 保留 last-known-good route，不写 cc-switch，Agent 行显示简短失败结果与重试入口。
- **原子 route snapshot**：旧请求继续使用旧快照，新请求使用新快照；reload 失败保留 last-known-good。
- **Anti-patterns 红线**（出现即返工）：用「代码路径存在」写「已完成/可发布」；metadata-only/preview/fixture 冒充 verified；Provider usage 缺失显示成 0/绿色；Local Token 当 Provider Token；估算当 claimable；为修绿删/ignore/弱化测试；实现 Agent 替主代理做真实 UI/cc-switch 切换并冒充 Computer 证据；release build 注册 test-only commands；并发改共享文件；仅凭 artifact mtime/文件名/`status=pass` 接受旧候选证据；仅凭 JSDOM/Vitest 声称 920×640 无横滚；改写既有 G0–G8 业务语义。
- **Plan Mutation Protocol**：执行 Agent 不得自行改写切片边界/依赖顺序/结论判定；默认变更通过主代理在 `.ai-bridge/release-iteration/` 追加 `<date>-mutation.md` 并更新 TASKS.md；仅当用户明确要求直接修订蓝图本体时主代理可直接更新 FINAL_BLUEPRINT.md 并在「审核修订记录」记录；任一 PR 返工超 2 轮仍失败：停止切片，主代理重新评估方案，不得靠扩大容错掩盖。
- **Rollback**：代码回滚 `git revert <commit>`（先 git_operation_audit record）；配置回滚 ConfigPatcher 既有 backup/restore 与 exact restore 证据；凭据隔离 entry 删除；cc-switch 只读未写无需回滚；合并后发现问题从 `main` revert 该 PR commit，重开修复切片。

## 16. 验收矩阵

### 16.1 配置交互验收（P0.0.10 + F-01/F-05 验收标准合并）

| # | 验收项 | 判定 |
|---|---|---|
| A1 | 完整 Agent 清单按 已安装/未安装/已配置 展示；未安装不可勾选，已安装默认全选，用户可取消 | 通过/失败 |
| A2 | 所有选中 Agent 按其官方能力层分别闭环：Route 层真实 routed-flow+复读 verify+restore+receipt；MCP-Hook 层真实 initialize/tool/hook+回退且不冒充路由；Detection-only 层官方架构证据+真实检测+诚实 UI（不要求 routed-flow）；任一要求适配 Agent 在其所属能力层未闭环 → `release_ready=false` | 通过/失败 |
| A3 | 每个选中 Agent 独立结果与单项「重试」，部分失败不撤销其他成功项 | 通过/失败 |
| A4 | 认证由 Agent 自持（账号/OAuth/API key）；d-token 不重复录入、不持久化凭据 | 通过/失败 |
| A5 | OpenCodex 动态代理多模型 passthrough 保持原协议语义，模型不固定为安装时值 | 通过/失败 |
| A6 | cc-switch 只读自动跟随 d-token route/runtime（A→B→A 后 route identity 相同、`wrote_cc_switch=false`、ledger events）；d-token 不写 cc-switch | 通过/失败 |
| A7 | 全程无 Provider/上游/model/protocol/endpoint/cc-switch profile chooser（Agent 清单勾选除外，§2.2.3） | 通过/失败 |
| A8 | 普通 UI 仅用 §3.6 配置/同步结果状态词表与用户文案（无 `needs-source-fix`/来源冲突/等待安全适配等内部术语）；Detection-only 项诚实显示「已检测/不支持本地路由」（允许的正常终态，不冒充 connected/configured）；Codex 无 routed 报告显示「配置成功；尚无真实请求验证」 | 通过/失败 |
| A9 | 配置备份、写入验证和恢复路径均可验证；首次流程结束时存在真实 Receipt | 通过/失败 |
| A10 | 配置事务 `change_required=false` 时前后 SHA-256 一致（restore_status=not_required_identical） | 通过/失败 |
| A11 | Route/MCP-Hook/Detection-only 分层不互相冒充；detection-only 项不显示 connected/configured | 通过/失败 |
| A12 | 逐 Agent 配置复制到隔离临时根：configure→reread→clear→exact restore；原文件仅 hash/权限证据且不输出内容；失败仅回退该 Agent 本次未完成写入，不回滚成功项 | 通过/失败 |

### 16.2 最终候选验证矩阵（N0–N10，FINAL_BLUEPRINT §8）

| 节点 | 内容 | 绑定 |
|---|---|---|
| N0 | 候选全量质量门：structure、Rust fmt/clippy/workspace tests、前端 Vitest/tsc/build/E2E | 失败 blocked；修复产生新 commit 后完整重跑 |
| N1 | Receipt：PRD 最小字段、Local/Provider/claimable 分离、逐请求 cost（费用/差额/币种/单位/价格来源/更新时间）、Analytics 与 export 脱敏（§11.4） | C_final |
| N2 | Codex：Responses、动态 model、无 OpenAI auth、两种配置形态、ledger/metrics/恢复 | C_final |
| N3 | OpenCode Free + Go dsv4flash 两条长工程真实 routed 验收（§10） | C_final + source digest |
| N4 | Agent 配置验收（隔离副本）+ CBM 五正式版逐版本验证（不内置下载/更新；无资产 unavailable+原因，§12） | C_final |
| N5 | cc-switch 主代理 Computer A→B→A；cc-switch 全量 app type 并集核对目标 Agent 矩阵（§3.4），不只 openclaw | C_final |
| N6 | 真实 Tauri UI：八页/主题/语言/窄窗/Reduce Motion/native action/重启恢复；Onboarding 五段（§3.7）；普通页面仅用 §3.6 配置/同步结果状态词表与用户文案；视觉压缩三态覆盖+重启一致（§11.5）；Doctor 每检查 freshness/action result 场景（§11.6）；品牌图标合同（§16.3）；八页文案 OMP DS copy sign-off | C_final |
| N7 | macOS：安装/升级/卸载回滚/autostart/凭据 round-trip；RTK 解封后 managed lifecycle/hook/tee；Dock/app bundle/window switcher 图标（§16.3） | C_final |
| N8 | Windows 产物/DACL/NSIS/PowerShell G6/同 commit fan-in；CBM 五版本平台矩阵（v0.8.1 无 windows-arm64 → incompatible）；EXE/taskbar/window/installer/uninstaller 图标（§16.3） | C_final；无法取得即 external/blocked |
| N10 | U0 ContractTest → U1 RegistryTest → U2 PropertyFuzz → U3 FaultDurability → U4 ReplayQuality → U5 PerfSLO → U6 ProviderPaired → U7 OpticalExactActive → U8 PlatformDeterminism → U9 CoverageSeal | C_final；独立发布阻断 |
| N9 | 仅在 N1–N8 与 N10 全部结算后，以 C_final + source digest 为强制输入聚合三种结论（正式签名发布 / 未签名技术预览=双平台证据全绿即本轮完成 / blocked）；未签名档不得宣称已签名 | 逐 artifact 校验 candidate binding |

### 16.3 UI 与平台验收

- UI 1180×780 与 920×640、浅/深色、中/英、Reduce Motion 可读；920×640 `documentElement.scrollWidth <= clientWidth` 由 Playwright 真实 layout engine 证明，最终 N6 真实 Tauri 复核（FINAL_BLUEPRINT checklist 27）。
- 侧栏专项：collapsed/expanded、窄窗、浅/深色、键盘 focus、hover、点击、tooltip 命中矩形一致；repeated click 不累积动画/effect；Reduce Motion 关闭 transform；小猫图标为产品自有 SVG/path、不替代文字可访问名称；真实 macOS Tauri 窗口至少一轮点击坐标/截图/布局复核。
- **品牌图标合同（统一）**：macOS Dock/app bundle/window switcher、Windows EXE/taskbar/window/installer/uninstaller、shell 小猫图标均为产品自有批准资产（透明、尺寸正确）；不以上游商标替代产品图标、不替代可访问名称；以真实安装包截图/AX 验收（N6/N7/N8）。
- Windows/macOS 共享同一 React UI、信息架构和产品语义；平台差异只放在凭据、进程、系统集成和打包适配层。

### 16.4 统一压缩质量与恢复验收（批准设计质量与验收设计）

| # | 验收项 | 判定 |
|---|---|---|
| C1 | 真实长工程端到端压缩率 ≥5%：`rate=(baseline input tokens−final sent input tokens)/baseline input tokens`，真实 Agent production ingress 到 final send，paired 条件相同，只计 actual 输入减少；fallback/failure/shadow/estimate 不计 actual；Relay/cache/RTK/Optical 四维分账；Provider 无 usage 时允许同版本 tokenizer local measured、不冒充计费（§3.1/§2.1.21） | 通过/失败 |
| C2 | system/developer、当前用户消息、代码/diff、secret、tool schema/arguments、开放工具状态零字节变化；九类 codec 语义/结构校验与字节精确还原（§9.1） | 通过/失败 |
| C3 | 恢复精确：CCR retrieve/delete/restore 全生命周期、pre-send fault 原文只发送一次、post-commit 无 retry（§9.2） | 通过/失败 |
| C4 | 发送前失败保留原始正确请求；上游已接收或 streaming 后禁止自动重发（§9.2/§7.2 失败降级） | 通过/失败 |
| C5 | CacheAligner 真实 cache alignment + 真实 receipt，进入同一 canonical IR/production path；节省只计入 provider-cache 分账；无 accounting-only/metadata-only/no-op（§2.1.22/§6.2） | 通过/失败 |

## 17. 迁移原则（从当前实现迁移，而非推倒重写）

1. **在既有实现上收敛**：18 个 Rust crate、Tauri 2 + React 18 桌面 UI、九类 codec 生产 evidence、CCR、23 Agent 检测矩阵、11 个已验证 adapter、cc-switch 只读 watcher、managed RTK lifecycle、真实 G7 OpenCode 证据均为迁移基线；不重写 runtime、不重建 UI、不另起第二套 pipeline。
2. **策略搬移而非复制**：LeanCTX/Context Bus 策略从 `dtoken-compress`/`dtoken-context-store` 搬移到新 `dtoken-context-bus`，避免双权威（P0.0.5、CONTEXT-1）；`dtoken-context-store` 只实现 persistence port，`dtoken-compress` 只消费 reuse decision。
3. **版本统一（V-1）**：C0–C8 与功能/平台切片完成后一次性把 manifest、PRD 版本门、PLAN/status/CHANGELOG/release workflow、`check_release_version.py` 收敛为 v0.0.1；旧版本字符串只存 history/archive/CHANGELOG 历史。
4. **旧证据失效**：V-1 后新证据只绑定 v0.0.1 `C_final`；旧 G7/DMG/browser preview/非 C_final 测试报告全部按过期证据隔离（§13）。
5. **能力再证明而非改语义**：既有 G0–G8 gate 语义保持稳定；Codex/config/cc-switch/upgrade 等 supplemental evidence 不得重定义 G7 Real OpenCode 等既有 gate（FINAL_BLUEPRINT §9.1.14）。
6. **每步可回滚**：单切片 revert、配置 exact restore、凭据隔离删除、cc-switch 只读无回滚需求；发布结论判定：双平台证据全绿的 unsigned prerelease 即本轮完成（§3.5），任一关键证据缺席/失败则保持 blocked 并如实报告。
7. **缺口修复路径**：Native Responses 纳入 single dispatch、SendCommitReceipt 生产接线、RTK 三平台解封、CBM 五正式版兼容——都是对既有实现的收敛改造，不是重写。

## 18. 开放问题（仅列真实未决，不得重开冻结决策）

以下为本文件确认的真实未决事项（进计划/验收节点；§3 冻结决策与本文件其余内容不在此重开）：

| # | 开放问题 | 影响 | 处置 |
|---|---|---|---|
| O1 | Onboarding 服务启动可视化（B-1）：欢迎简介 → 有呼吸感动画和可视进度的服务启动尚未接线 | GOAL 3 首次引导体验与 N6 验收 | PR-14/N6 接线并验收 |
| O2 | native 配置流程无阶段回调（H-2）：Tauri native command 一次性返回，UI 无法展示 plan/apply/verify 逐阶段进度 | 一次点击配置的进度反馈 | 定义阶段回调合同后由 Rust/Tauri 侧收敛（PR-3） |
| O3 | Onboarding 6 步 vs 五段差异（B-2）：当前 onboarding 6 步流程与批准设计五段（welcome → service-start 动画+可视进度 → scan → configure 一次点击 → start-use，verify 归入配置段内部，§3.7）不一致 | 首次引导一致性 | PR-14 统一 |
| O4 | Windows runner/VM 可用性：Windows 实机/runner 不可用时 N8/Windows 证据任务未完成（批准设计错误语义：不可用即任务未完成，不伪装通过） | N8/Windows 验收与同 commit fan-in | 主代理调度 runner；无法取得即 external/blocked 如实记录 |
| O5 | 外部资产可得性：CBM 官方资产（0.8.0/0.7.0/0.6.1 等）、签名凭据、固定版本上游 clone 的可得性依赖上游/用户环境 | 五正式版验证与正式签名发布 | 官方资产不存在按 §12 诚实 unavailable 不阻断；存在但校验失败才 blocked（§12） |

## 19. 当前活跃文档冲突清单与下一阶段统一重写

批准设计已确认：**当前活跃 PRD / 目标规格 / PLAN / 版本包仍存在旧冲突，下一阶段必须统一重写**。本文件固化批准设计口径（冲突时以批准设计与 PRD P0.0.10 为准），执行期间不再逐条修补旧文档；统一重写作为下一阶段的明确工作项。

| # | 冲突点 | 活跃文档现状 | 批准设计口径 | 处置 |
|---|---|---|---|---|
| C1 | 版本号 | PRD P0.0.7、PLAN、IMPLEMENTATION_STATUS、CHANGELOG、manifest 为 `0.2.0-beta.1` | 目标版本 **v0.0.1**（用户指定） | V-1 切片统一重写 |
| C2 | Agent 选择器 | 目标规格 v2.1 F-01 旧口径仍写「选择需要接入的 Agent/支持跳过部分 Agent」；`needs-choice`/`selection_required` 生产残留（presentation.tsx、installMachine.ts、connectionSetup.tsx 等） | Agent 勾选允许（完整清单，已安装默认全选可取消）；仅禁 Provider/上游/model/protocol/endpoint/cc-switch profile chooser；`needs-choice`/`selection_required` 等上游选择残留全链路清除 | PR-3 清除残留并同步旧口径 |
| C3 | 整批确认/回滚 | 旧版本包 review-gated `automatic_apply_allowed=false`、whole-batch 逆序回滚、二次 confirm 文本 | 一次点击直接执行 + 逐 Agent 独立事务，部分失败不回滚成功项 | 2026-08-12 同步轮已改写；本文件 A12 已按逐 Agent 独立事务语义修正（失败仅回退该 Agent 未完成写入），剩余残留由 PR-3/版本包统一清理 |
| C4 | Onboarding 步数 | 八页审计：onboarding 6 步 vs 5 段（welcome/service/agents/config/verify） | Onboarding 五段：welcome → service-start（动画+可视进度）→ scan → configure（一次点击，内部 plan/apply/readback/verify）→ start-use（§3.7） | PR-14/N6 统一 |
| C5 | Native Responses 路径 | Native Responses 绕过 canonical pipeline；production send 未消费 SendCommitReceipt | single dispatch 必须包含 Native Responses；发送体=验证体；receipt 必须生产消费 | PR-C1C/PR-12 + PR-C0B/PR-C6C |
| C6 | RTK 状态 | macOS/Windows/Linux Install/Configure/Verify/Rollback/Uninstall 全部 `ManagedRtkTransactionState::Blocked` | 解封（未配置=unavailable 而非 Blocked）后走 C4A/B/C | PR-R1/PR-C4U + PR-C4A/B/C |
| C7 | CBM 版本 | manifest→graph 单版本（v0.9.1-rc.1）已工作；五正式版兼容缺失 | 五正式版 v0.9.0/v0.8.1/0.8.0/0.7.0/0.6.1；rc.1 标 prerelease 不计五正式版 | PR-CBM3/4 + N4/N8 |
| C8 | 配置恢复证据 | 文档曾把「配置事务精确恢复」写成可替代 Codex 真实 routed-flow | 独立真实 routed-flow 报告必须存在；Codex 无报告只显示 configured/not_verified | PR-2 + N2 |
| C9 | 前端范围 | 视觉/动画类 plan 文件存在扩张风险（`plans/` 动画计划、UI design 目录） | 前端严禁大范围重构（§4）；架构收敛主要在 Rust | 以 §4 为界，越界切片返工 |
| C10 | 价格/费用 | 设置价格与费用估算路径存在未统一口径 | 只有 Token 与价格证据同时存在才展示费用与差额；缺证据 unavailable；缺价格不能当作 0 | PR-1/N1 收口 |

统一重写范围（下一阶段）：PRD（合并 P0.0 与历史草案、清除 0.2.0-beta.1 口径）、目标规格 v2.1（F-01/F-05 与 P0.0.10 对齐）、PLAN（滚动维护）、IMPLEMENTATION_STATUS（代码事实）、版本包九文件（版本/切片/证据索引）、README/CHANGELOG；重写后旧文档口径以批准设计为准归档。本文件是重写前的统一设计基线。

## 20. 术语对照表

| 术语 | 定义（批准设计口径） |
|---|---|
| Route | 模型流量真实经过 d-token 的配置层证明（§3.4） |
| MCP/Hook/Skill | 工具/上下文能力接入层，不证明路由（§3.4） |
| Detection-only | 仅检测存在，无适配写入、fail-closed（§3.4） |
| AgentCapabilityMatrix | 23 个 Agent 检测注册表 + 11 个已验证 adapter 的显式能力分层矩阵（§3.4/§6.3） |
| canonical Context IR | 一个请求的唯一结构化中间表示，每 node 恰一个终态动作（§6.2/§7.2） |
| NodeAction | Preserve/Reuse/HeadroomSemantic/RtkCommandOutput/Optical（§6.2） |
| RouteDecision | route/provider/model/protocol/API revision/capability/deadline 唯一权威（§6.2） |
| OptimizationReceipt | PRD 最小字段脱敏 per-request receipt，Local/Provider/claimable 分离（§6.2/§7.2） |
| CandidateEvidenceEnvelope | 所有 N0–N10 机器 artifact 必填统一信封（schema_version…artifact_sha256）（§13） |
| C_final | 最终候选 commit；所有证据必须绑定（§13/§14） |
| source_digest | 候选源码 digest（dirty=false）；证据绑定必填（§13） |
| 热同步 | cc-switch 只读 watcher 自动同步 route/runtime；review-gated 授权门（§8.2） |
| 逐 Agent 事务 | 每 Agent 独立 plan/backup/apply/verify/rollback/receipt，失败不回滚成功项（§8.1） |
| protected bytes | 压缩必须保护的字节（代码块/标识符/URL/尾部/exact/secret/混合行尾）（§9.1） |
| CCR | 可逆恢复存储：AES-256-GCM、TTL/quota、journal、retrieve/delete（§9.2） |
| CacheAligner | canonical pipeline 真实组件：真实 cache alignment + receipt，只计 provider-cache 分账，禁 accounting/metadata/no-op（§2.1.22/§6.2） |
| claimable | 仅 Provider 计费/usage 证据与归因链完整时为 true（§3.1） |
| unsigned prerelease | 同一 tag/commit 的公开未签名技术预览；双平台证据全绿即本轮发布完成（§3.5） |
| release_ready | 全部 MUST/证据绑定满足时为 true（不含签名要求）；否则 fail-closed（§2/§13） |
| distribution_ready_public | 公开签名分发就绪：Developer ID/notarization/Authenticode 齐备；unsigned prerelease 保持 false，不阻断本轮完成（§3.5） |
| CBM 五稳定版 | v0.9.0/v0.8.1/0.8.0/0.7.0/0.6.1（§12） |
| Onboarding 五段 | welcome → service-start（有呼吸感动画和可视进度）→ scan → configure（一次点击，内部 plan/apply/readback/verify，verify 非独立用户段）→ start-use（§3.7） |

## 21. 自审与使用说明

- 本文件逐条固化批准设计（§1–§20），与 PRD P0.0.10、目标规格 v2.1（F-01/F-05）、PLAN、IMPLEMENTATION_STATUS、GOAL、FINAL_BLUEPRINT、版本包核对过术语与事实；冲突以批准设计为准（§19）。
- 本文件只创建 `.ai-bridge/release-iteration/UNIFIED_RELEASE_DESIGN.md`；未修改任何其他文件，未提交，未测试，未格式化，未运行项目命令；本文件是设计契约，不含任何「已实现/已通过」声明。
- 后续 implementation plan 以本文件为唯一输入：不遗漏已批准内容（权威顺序、MUST/非目标、5%、cc-switch/OMP 只读、一次授权写入、分层矩阵、unsigned prerelease、前端边界、总体架构、共享 Rust 合同、canonical pipeline、逐 Agent 事务与热同步、protected bytes/recovery、OpenCode Free/Go dsv4flash、八页 read model、CBM 五稳定版、evidence envelope、DAG/waves/共享文件串行、安全/错误、验收矩阵、迁移原则、冲突清单），不重开冻结决策（§3）。
