# d-token 执行计划（Living Plan）

状态：Living Document
维护规则：本文件是 d-token 的唯一执行计划，按代码现状滚动维护。产品边界改 PRD，阶段任务改本文件；当前代码事实同步维护在 `docs/product/IMPLEMENTATION_STATUS.md`。PRD（`docs/PRD.md`）是产品真理源，本文件不得与其验收标准冲突。

## 0. 当前坐标

| 维度 | 现状 |
|------|------|
| PRD | 单一 living PRD，§18 已去计划化，指向本文件 |
| 文档 | 当前导航见 `docs/index.md`，代码事实见 `docs/product/IMPLEMENTATION_STATUS.md`；旧计划/矩阵/研究已归档 |
| 代码 | 18 个 Rust 业务 crate + 1 个 Tauri app crate + React 18/TypeScript/Vite 桌面 UI + 薄 TypeScript SDK |
| 已交付里程碑 | M1（no-touch 基础）、M2（Headroom 只读集成）、M3（GUI MVP） |
| 交付证据 | `docs/history/M1_NO_TOUCH_DEV_FOUNDATION_REPORT.md`、`docs/history/M2_EXTERNAL_READ_ONLY_INTEGRATION_REPORT.md`、`docs/history/M3_GUI_CONTROL_PLANE_MVP_REPORT.md`、`docs/history/M1_M3_DELIVERY_AUDIT.md` |
| 当前 Node | `v0.2.0-beta.1` 发布收口：G2 codec、G3 CCR/ACL 实现、G4 surface parity、Agent/cc-switch/RTK 本机候选证据已显著推进；仍须完成最新全仓复证、真实 OpenCode 当前候选、Windows MSVC runner、同 commit 双平台产物与 G6–G8 后才能发布 |
| 历史路线图 | `docs/archive/ROADMAP_M0_M6_HISTORICAL.md`（M0–M6 原文快照） |

当前 M4 历史设计输入：

- `docs/history/M4_REAL_CAPABILITY_OPENCODE_PLAN.md` — 当时的真实能力接入计划，仅作为历史设计和既有测试证据；当前执行状态与剩余工作以本文件、PRD、`docs/product/IMPLEMENTATION_STATUS.md` 和当前源码/测试为准。

代码现状盘点（2026-07-31）：

```text
code/crates/
  dtoken-types, dtoken-config, dtoken-router, dtoken-compress,
  dtoken-process, dtoken-doctor, dtoken-event-ledger,
  dtoken-preprocessor, dtoken-headroom-adapter, dtoken-project-policy,
  dtoken-config-patcher, dtoken-core-api, dtoken-proxy, dtoken-learning
code/apps/desktop/   Tauri 2 + React 18 + Vite，共享 Windows/macOS UI 与 native commands
code/runtimes/headroom/   Headroom-derived runtime provenance/兼容边界（非 Python sidecar）
support/fixtures/ support/fake-bin/   no-touch 测试资产
.github/workflows/ci.yml   structure / cargo test / no-touch smoke / desktop build
.github/workflows/macos-release.yml   macOS ARM64 unsigned prerelease reusable workflow（app/DMG、包审计、hash、SBOM/license/security/provenance）
.github/workflows/windows-release.yml Windows x64 unsigned prerelease reusable workflow（NSIS、静默安装/启动/卸载、hash、SBOM/license/security/provenance）
.github/workflows/public-unsigned-prerelease.yml 同一 tag/同一 commit 聚合 macOS ARM64 + Windows x64，成功后才创建 GitHub public prerelease
```

当前已交付基础：

1. d-token-owned Rust local service 已提供 OpenAI Chat/Responses、Anthropic Messages、Dashboard、stats/history/metrics、Provider/Route/Instance、Doctor、Event/Logs/Export 与 Optical shadow APIs。
2. Dashboard、Doctor、Analytics、Logs、Changes 和 Export 已统一从 Tauri command 与 d-token-owned local service 读取真实证据；桌面页面不再用浏览器 fixture 或 mock 数字补齐状态。
3. 当前 macOS 候选已完成真实 `OpenCode -> d-token 127.0.0.1:18001 -> OpenCode Zen` 验证：单请求物理减少 33,705 Token，`claimable=0`；持久化 runtime 汇总为 89 条 routed request、20 条可见 request record、物理减少 279,270 Token（约 4.0%）。
4. Tauri bundling 已启用：Windows NSIS，macOS `.app`/DMG；当前公开分发路径是同一 tag/同一 commit 的 GitHub **公开未签名 prerelease**。只有聚合 workflow 中 macOS ARM64 与 Windows x64 两个 reusable job 同时通过，并附带 SHA-256、SBOM、license、安全扫描和 provenance 证据时，才创建或更新 GitHub prerelease。它不宣称签名、公证或系统信誉。
5. OS credential store 写入、引用和出站短生命周期读取已有 Rust 实现；仍需认证 Provider 的候选版实机证据。
6. 桌面统一采用 AiMaMi 信息层级取向的纯色 shell/surface、默认全年 heatmap、分组设置、统一 `OperationDialog` 与持久化 `NotificationCenter`；成功/信息/警告/失败均即时进入顶栏 banner，短时收起与通知中心持久 receipt 解耦。侧栏图标 enter/leave 可逆动效与点击 scale 已按 PRD §12.5.7 落地（原则借鉴、不抄 AiMaMi 资产）。
7. OpenCode 配置与 d-token active route 均已通过最小 diff、双 hash、备份、验证和精确恢复闭环；恢复后 SHA-256 分别为 `47772c85307f02e1d910dc3cc93b85165a9fc37db5911b7d4201be766f85baab` 与 `37149d6c69561a4562a67d7d53bf03337e37e98104424c694aaa4a4038c45843`。Smart Learning 也已在临时目录证明 managed block 精确回退。
8. Hybrid Agent Integration 与 transport 已进入候选验收：Rust-owned 多文件事务覆盖 Claude Code/Codex/OpenCode 的规则、Hook/Plugin、Skill 与可选 MCP；独立 transport adapter 已覆盖 Claude Code、Codex、Cline CLI、Goose、Kilo Code、OpenCode、OpenClaw、Gemini CLI、Roo Code、Continue、Aider 的 Configure/Clear。2026-07-31 已按真实本机 schema 对 Claude Code、Codex、Cline 执行 preview → apply → reread verify → clear → exact restore，未启动 Agent、未发模型请求、未读取凭据值，原文件 SHA-256 与 `0600` 权限均恢复；2026-08-03 工作树将 Gemini CLI/Roo Code/Continue/Aider 加入同一 Configure/Clear 矩阵（fixture 级事务锁定，真实候选 probe 仍待执行）。
9. Rust 检测注册表当前覆盖 20 个 Agent；每个检测项都有显式 transport capability decision。11 个已验证 adapter 可受控配置/清除（Claude Code、Codex、Cline CLI、Goose、Kilo Code、OpenCode、OpenClaw、Gemini CLI、Roo Code、Continue、Aider），其余 9 个保持 detection-only/fail-closed，不再用通用 JSON 递归猜字段或写 metadata block 冒充路由。前端静态 6 项 Agent catalog 已删除，Install 只消费 Rust 发现结果；每行统一显示“配置”“清除”，未验证操作只返回无写入状态，不再把用户跳转到手工配置流程。支持 adapter 的上游解析覆盖 Claude Code、Codex、Cline、Goose、Kilo Code、OpenCode、OpenClaw、Gemini CLI、Roo Code、Continue、Aider；Goose 相邻 custom provider 文件也参与 watcher/fingerprint。
10. cc-switch 使用只读 DB/WAL/SHM watcher、750ms debounce 与 polling fallback；pending 变更回滚到稳定快照会被清除，下一次变更须重新完成 debounce。首次 hash-bound 授权后仅可自动更新 d-token 自有 route/runtime，绝不写 cc-switch。2026-07-31 本机已完成 A→B→A 静态 provider 切换识别与恢复，不做模型请求；最终 active route identity 与初始状态一致。
11. managed RTK macOS candidate probe 已通过安装/验证、命令过滤、成功/失败 exit propagation、usage metadata、精确卸载与私有根目录清理；最新 metadata-only receipt 为 `support/artifacts/managed-rtk-candidate/candidate-20260731T223754Z-7536.json`。Windows/WSL runner 证据仍待补齐。

当前发布边界：当前 `0.2.0-beta.1` 只允许作为 macOS ARM64 / Windows x64 的**公开未签名 prerelease**，不得写成已签名、公证、已通过系统安全检查或 production-certified 的安装包；未签名/ad-hoc macOS 构建出现系统安全警告属于预期，不能据此绕过系统安全保护。

1. 公开未签名 prerelease 聚合 workflow 已配置为 tag `v*` 下同一 run 聚合 macOS ARM64 与 Windows x64 reusable job，只有两端门禁、SHA-256、SBOM、license、安全扫描和 provenance 全绿后才创建 GitHub prerelease；当前本地 dirty tree 仍需以真实门禁结果为准，不能把配置存在当作发布完成。
2. macOS 仍缺 Developer ID/notarization/Gatekeeper 的正式签名路径；Windows 仍缺 AuthentiCode 的正式签名路径。公开 prerelease 不等于已具备系统信任链或生产签名。
3. 平台自动启动控制面已接通 macOS LaunchAgent 与 Windows HKCU Run，并具备写入校验与失败回滚；仍需同一候选在真实登录会话完成 macOS/Windows 启动、状态恢复和关闭验证。
4. Claude Code、Codex、Cline 的真实本机临时配置事务已完成并精确恢复；按授权未发送模型请求。Gemini CLI/Roo Code/Continue/Aider 已具备版本化 Configure/Clear adapter（fixture 级验证，真实本机候选 probe 待执行）。OpenCode 仍须在最终候选上重跑真实 routed compression；Cursor 因官方路由架构保持 detection-only（所有 BYO-key 请求经 Cursor 服务器），其余未验证 schema 的 Agent 保持 detection-only，不得伪造自动接入。
5. 真实认证 Provider 的 macOS Keychain/Windows Credential Manager 出站读取仍缺不暴露凭据的候选版实证。
6. Headroom §8 能力尚未完整迁移：当前多个同名端点仍是 metadata-only、preview-only、静态兼容声明或透传；在逐能力 runtime/eval/Agent 证据闭环前不得进入公开试用候选。
7. 截至 2026-07-31 当前 working tree，G0/G1 仍以最新全仓复证为准。G2 的本地 release probe 已将 JSON、log、search、build、diff、text、HTML 与覆盖 Rust/TS/JS/Python/Go/Java/C/C++ 的 code-compressor 全部标记 active；每类至少 128 个 property/semantic/rollback cases，negative/budget/cancellation 门通过，零失败，evidence revision 为 `d0cb2e0346339d8985ea949b1c6dbdb25fc7b347d3440e04e5584cfce9233b69`。G3 已实现 CCR lifecycle journal/reconciliation、Unix 权限与 Windows current-user+SYSTEM protected DACL；Windows GNU cross-check 通过，但 MSVC runner 仍须实际运行 ACL tests。G4 的 Rust/HTTP/MCP/TS SDK parity 已有定向回归。以上证据尚未绑定最终 clean candidate；G6–G8、真实 OpenCode 与双平台产物未全绿前不得发布。

## 1. 计划原则

1. **PRD 主产品契约，PLAN 主执行节奏**：边界/验收改 PRD，任务拆解/排期改 PLAN。
2. **事务优先**：任何真实写操作必须先有 fixture/fake-binary 故障验证、diff、backup、verify、rollback；真实路径仅在用户明确操作和受控边界内开启。
3. **每个任务带 DoD**：Definition of Done = 可验证项 + 涉及文件 + 测试 + 回滚证据。
4. **不破坏已交付里程碑**：M1–M3 验收项保持绿，新工作不得回退 no-touch 安全边界。
5. **Windows/macOS 一等公民**：任何 install/patch/rollback 任务必须显式覆盖 UTF-8、盘符路径、项目名连字符、`.cmd` shim、xattr/quarantine。
6. **无静默 fallback**：route/provider/安装失败必须可见、可解释、可审计。

## 1A. 当前最高优先级：自动上游解析与智能学习全面扫描（待实施）

状态：2026-07-13 已完成多维度审查并同步 PRD P0.7.1–P0.7.3、§19 验收契约；业务代码尚未开工。

本节用于落实 PRD P0.7.1–P0.7.3 与 §19 的产品契约，纠正当前“安装与连接”实现的关键偏差：

1. 一键安装的默认路径必须自动读取并解析用户现有上游，不得把“进入高级安装手填 URL/provider/model”作为正常前置条件。
2. 智能学习必须自动发现本机全部受支持 Agent 的会话记录与项目关联，不得要求用户先输入单个项目目录后才开始扫描。

### 1A.1 产品结论与不可退让边界

#### A. 上游读取与路由解析

默认产品逻辑应为：

```text
进入一键安装
  -> 自动检测本机 Agent、Agent 当前配置、OpenCode provider/model、cc-switch、d-token 已保存连接资料
  -> 自动识别每个 Agent 当前实际使用的上游和协议
  -> 自动生成“Agent -> d-token -> 原上游/cc-switch”的连接计划
  -> 只有存在多个同等有效候选或字段无法确认时，要求用户在已发现候选中选择/确认
  -> 用户审阅 diff、备份和连接路径后应用
  -> 验证真实连接并生成回退点
```

“自动读取”与“静默写入”必须严格区分：

1. 读取、识别、排序、生成连接计划可以自动执行。
2. 已有唯一且高置信度的当前上游时，UI 自动选中并直接进入连接计划，不要求用户手填 URL、协议或模型。
3. 多个有效 provider/upstream 并存时，完整展示来源、协议、模型、脱敏地址和当前激活状态，由用户显式选择；禁止静默猜测。
4. 任何 Agent 配置、d-token active config 或第三方配置写入仍必须经过摘要、diff、backup、确认、verify、rollback。
5. 高级安装只处理自动发现失败、未知协议、自定义链路或用户主动覆盖，不得成为默认入口。

路由解析必须按 Agent 分别进行，不能继续使用单个全局 `selectedModel` 代表所有工具。目标模型应为：

```ts
type AgentUpstreamBinding = {
  agentId: string;
  currentConfigSource: 'agent-config' | 'cc-switch' | 'dtoken-saved' | 'catalog' | 'unknown';
  candidates: UpstreamCandidate[];
  resolvedCandidate?: UpstreamCandidate;
  resolution: 'auto-resolved' | 'needs-choice' | 'needs-credential' | 'unsupported' | 'unavailable';
  confidence: number;
  reasons: string[];
};
```

这样可以正确处理 Claude Code、Codex、OpenCode、Cursor、Cline 同时使用不同上游的情况，不强迫用户把所有 Agent 统一到同一 provider/model。

#### B. 智能学习全面扫描

智能学习默认逻辑应为：

```text
首次打开智能学习上栏
  -> 若没有新鲜缓存，自动启动本地轻量扫描
  -> 枚举全部受支持 Agent 的会话根目录和最近工作区来源
  -> 枚举全部会话记录，但只提取白名单 metadata，不读取/返回会话正文
  -> 解析 session -> project/workspace 关联
  -> 合并常用项目目录、Git 仓库、编辑器工作区和 d-token 历史
  -> 对项目做轻量索引、去重、评分和可解释排序
  -> 流式展示候选与扫描进度
  -> 用户选择高价值项目后再执行深度扫描、生成上下文、委派或受控写入
```

“扫描全部 Agent 会话记录”定义为：

1. 枚举受支持 Agent 的所有可识别 session/history/task 记录，而不是只检查某个根目录是否存在。
2. 轻量扫描阶段只提取 Agent、时间、项目路径/cwd、工作区标识、记录数量、文件大小区间、格式版本等白名单 metadata。
3. 不把完整 prompt、response、代码片段、命令输出、token、cookie 或认证内容写入 UI、日志、缓存和导出。
4. 对格式未知的记录只报告“存在但无法安全解析”，不得尝试宽松读取正文。
5. 深度理解仅针对用户选择或明确授权的高价值项目，且仍遵守敏感排除和读取范围限制。

### 1A.2 当前实现审查结论

#### 一键安装当前已具备的基础

1. `upstream_discovery.rs` 已能只读发现 OpenCode catalog、OpenCode 普通配置和 cc-switch SQLite。
2. cc-switch 读取已使用只读 SQLite、schema 上限和字段白名单，并避免返回密钥字段。
3. OpenCode 已有 preview -> 双 hash -> backup -> confirm -> apply -> verify -> rollback 事务。
4. Agent 前端 registry 已包含 Claude Code、Codex、OpenCode、Cursor、Cline、CommandCode 的命令、配置和会话路径提示。

#### 一键安装当前偏差

1. `Install.tsx` 把“没有已应用 d-token route”解释为“必须先进入高级安装”，与 PRD 的自动发现默认路径冲突。
2. 页面虽然自动调用 `discover_upstream_candidates`，但仍把结果当成用户手动选择模型的辅助数据，没有形成 Agent 级自动解析结果。
3. 当前发现源主要覆盖 OpenCode 与 cc-switch，尚未读取 Claude Code、Codex、Cursor、Cline 的当前配置与当前上游。
4. `selection_required` 固定为 true，没有区分唯一高置信度候选与多候选歧义。
5. cc-switch 当前 profile 切换尚未形成事件化同步状态，d-token 可能继续保留旧 active route。
6. 安装事务主要负责服务启动，自动发现结果尚未进入统一 route plan、diff、apply 和 verification 事务。

#### 智能学习当前已具备的基础

1. 已有 Agent `sessionPathHints` 注册表。
2. 已有 managed block preview/apply、双 hash、非管理内容保护、备份和 exact rollback。
3. 已有 Claude/Codex 委派入口、提示词模式、通知和本地设置持久化。
4. 当前 native discovery 能拒绝敏感根目录、符号链接和无效目录，并保证不返回文件内容。

#### 智能学习当前偏差

1. UI 必须先手填 `projectRoot`，并依赖 d-token runtime 聚合指标后才允许扫描；这与“自动发现全部 Agent 会话目录和项目目录”冲突。
2. UI 调用 discovery 时只传入 `[projectRoot]`，没有传入或自动生成 Agent 会话根目录、常用项目根、最近工作区。
3. native discovery 当前 `max_depth=0`、`max_files=0`，只把用户传入的根目录本身返回为一个候选，不枚举会话记录和子项目。
4. 没有 Agent session adapter、session -> project 关联解析、轻量索引、评分引擎、索引缓存、增量刷新。
5. 没有可持续任务、阶段进度、已扫描数量、停止/跳过入口；大规模扫描无法满足 PRD 的响应性要求。
6. 当前隐私文案写成“只使用聚合运行指标，不读取会话或源码”，实际产品契约应改为“全面本地扫描 metadata；深度读取仅限用户选择范围”。
7. 当前测试只证明单个用户目录不会泄漏文件正文，不能证明全 Agent 发现、排序、性能、取消、缓存或跨平台兼容。

### 1A.3 自动上游发现与同步设计

#### 发现源

按以下来源并行、只读发现：

1. Agent 自身当前配置：Claude Code、Codex、OpenCode、Cursor、Cline、CommandCode。
2. OpenCode provider/model catalog。
3. cc-switch profiles、providers、proxy config 和当前激活项。
4. d-token 已保存的非敏感 route/provider metadata。
5. d-token 当前 active route，作为现状比较依据，不作为覆盖自动发现的唯一真理源。

所有路径和格式规则应由 native 侧的版本化 adapter registry 管理，前端只消费结果，避免 TypeScript registry 与 Rust 实际探测逻辑漂移。

建议核心类型：

```rust
struct UpstreamDiscoverySnapshot {
    collected_at: String,
    source_states: Vec<DiscoverySourceState>,
    agent_bindings: Vec<AgentUpstreamBinding>,
    cc_switch_sync: Option<CcSwitchSyncState>,
    writes_performed: bool,
}

struct UpstreamCandidate {
    id: String,
    source: String,
    protocol: String,
    base_url_redacted: Option<String>,
    models: Vec<String>,
    active: bool,
    auth_capability: String,
    confidence: f32,
    reasons: Vec<String>,
}
```

#### 自动解析规则

1. 当前 Agent 配置明确指向 cc-switch 本地代理，且 cc-switch 当前 profile 可安全解析：自动解析为该 profile/upstream。
2. 当前 Agent 配置存在唯一的 active provider/model，且协议、地址和认证方式可安全复用：自动解析。
3. d-token 已有与当前 Agent 配置完全一致的 active route：状态为 `synced`，不重新要求配置。
4. 多个 active 或同置信度候选：状态为 `needs-choice`，仅让用户从已发现项中选择，不显示手填 URL 为主操作。
5. 只有地址已知但凭据无法安全复用：状态为 `needs-credential`；保留自动识别出的 URL/协议/model，只请求必要凭据，不让用户重复填写连接资料。
6. 未知协议或不支持格式：状态为 `unsupported`，提供高级安装或生成诊断提示词。
7. 不存在候选：状态为 `unavailable`，此时高级安装才成为回退路径。

#### cc-switch 自动同步

1. 只监视已定位的 cc-switch 数据库/配置文件，不监视整个 HOME。
2. 使用文件变化监听 + debounce；监听不可用时退化为低频状态刷新和手动刷新。
3. profile 变化后重新只读解析并比较 d-token active route。
4. 如果只需更新 d-token 自有 route，可自动生成同步计划；应用仍通过 d-token 配置事务确认。
5. 不自动修改 cc-switch 数据库、profile 或第三方配置。
6. UI 显示当前 profile、解析出的上游、d-token 当前路径和 `synced/out-of-sync/needs-confirmation/unsupported`。

### 1A.4 智能学习扫描引擎设计

#### Agent session adapter

每个 Agent 使用独立、版本化、fail-closed adapter：

```rust
trait AgentSessionAdapter {
    fn id(&self) -> &'static str;
    fn discover_roots(&self, platform: PlatformContext) -> Vec<SessionRoot>;
    fn detect_format(&self, path: &Path) -> FormatDetection;
    fn scan_metadata(&self, path: &Path, limits: &ScanLimits) -> SessionMetadataResult;
}
```

首批适配范围：

1. Claude Code：项目/会话目录，提取项目关联、最近活动与会话数量。
2. Codex：sessions/history，优先提取显式 cwd/project root，不从目录名反推项目路径。
3. OpenCode：session/storage 数据，读取版本化白名单 metadata。
4. Cursor：macOS/Windows workspace storage 与最近工作区映射。
5. Cline：VS Code 系列 globalStorage/task 历史，兼容稳定版与常见发行变体。
6. CommandCode：先实现 descriptor 探测和 unsupported-safe 状态；确认格式后再开放 metadata parser。

路径提示只是候选，运行时必须检测实际存在路径、格式版本和可读性；不能假定固定路径永久有效。

#### 扫描任务模型

全面扫描不能继续作为单次同步 command。应实现可取消、可观察的 native job：

```text
start_learning_discovery()
  -> 返回 job_id
  -> native worker 执行
  -> Tauri event 流式发送节流后的进度和候选增量
  -> get_learning_discovery_status(job_id) 作为恢复/兜底
  -> cancel_learning_discovery(job_id)
  -> list_learning_candidates(job_id)
```

阶段必须与 PRD 对齐：

1. 准备扫描。
2. 识别 Agent 会话来源。
3. 解析项目关联。
4. 建立轻量索引。
5. 评分和排序。
6. 生成候选结果。
7. 完成。

扫描过程中持续返回：已检查 Agent 数、会话记录数、解析成功数、项目候选数、跳过数、风险数、当前阶段和可取消状态。

#### 轻量索引

轻量索引允许读取：

1. canonical project path、目录名和修改时间。
2. `.git`、README、docs、src、package/Cargo/pyproject 等标记是否存在。
3. 文件数量和大小区间，但不读取文件正文。
4. Agent 文档是否存在。
5. 关联 Agent、会话数量和最近活动时间。
6. 语言/框架指纹，仅通过文件名和受限 manifest metadata 推断。

轻量索引禁止读取：完整会话正文、完整源码、认证文件、`.env`、token/cookie、密钥目录、依赖目录、构建产物和大型二进制。

#### session metadata 解析规则

1. 枚举所有 session 文件/记录，但只读取能够获得白名单 metadata 的最小范围。
2. JSON/JSONL/SQLite 必须使用 adapter 专用字段白名单和版本检查。
3. 对大型 JSONL 使用流式解析和字节上限，不整文件载入内存。
4. session id 在缓存和 UI 中使用稳定 hash，不暴露原始标识。
5. 项目路径优先使用显式 cwd/workspace/root 字段；不存在时才使用经验证的映射规则。
6. 解析失败只记录错误类型、Agent、格式版本和计数，不记录原始内容。

#### 去重、评分与解释

候选按 canonical project path 去重，并聚合所有 Agent 会话证据。评分采用 PRD 已定义维度：

```text
score =
  0.22 * recency +
  0.18 * agentActivity +
  0.18 * projectConfidence +
  0.14 * docGap +
  0.10 * complexity +
  0.08 * userPreference +
  0.06 * dtokenHistory -
  0.10 * riskPenalty
```

UI 至少显示前三条排序依据，例如“最近有 Codex 会话”“同时被 Claude Code 与 Cursor 使用”“缺少 AGENTS.md”“目录过大，仅完成轻量索引”。

#### 索引缓存

使用 d-token 自有、权限受限的 SQLite 索引，而不是把大量候选和 session metadata 堆入单个 JSON：

1. `scan_jobs`：任务状态、阶段、计数和时间。
2. `session_evidence`：Agent、session hash、project path、活动时间、格式版本和置信度。
3. `project_candidates`：轻量索引、评分、解释和风险。
4. `scan_roots`：已探测根目录、adapter 版本、mtime 和上次结果。

缓存只保留 metadata，不保存 prompt、response、源码、命令输出和凭据。缓存文件必须使用仅当前用户可读写权限；导出默认不包含本机完整路径。

### 1A.5 性能与响应性预算

1. 点击/自动开始后 100 ms 内显示任务已启动，不允许无反馈。
2. 首批可用候选目标在 1.5 秒内流式出现；完整轻量扫描不等待全部 Agent 完成后才一次性展示。
3. native 文件扫描和解析全部在阻塞任务池/worker 中运行，不占用 Tauri UI command 调度线程。
4. UI 主线程单次状态合并目标低于 50 ms；进度事件节流，避免每个文件触发一次 React 更新。
5. 默认并发有上限，按磁盘类型和 CPU 保守设置；同一物理根目录去重，避免多个 Agent adapter 重复遍历。
6. 每个 adapter 配置最大文件数、单文件读取字节数、总 metadata 字节数、最大深度和超时。
7. 未变化目录使用 mtime/size/adapter-version 增量缓存；用户“重新全面扫描”才强制刷新。
8. 支持停止、跳过不可读根目录和关闭某个 Agent 来源；停止后保留已完成的安全候选，不伪装为完整扫描。

### 1A.6 UI 信息架构调整

#### 一键安装：普通用户只面对结果，不面对内部组件

正常路径不应把“服务、provider、route、model、cc-switch、端口”依次做成用户必须理解的步骤。内部仍执行完整事务，但用户侧压缩为：

```text
自动读取本机环境
  -> 唯一方案：直接显示“已为你准备好”
  -> 有歧义：只要求选择候选或补充凭据
  -> 用户确认连接摘要
  -> d-token 自动安装/启动、应用配置、验证并生成回退点
```

普通路径最多要求用户做两类决策：

1. 当检测到多个已安装 Agent 或同等有效上游时，选择要连接的 Agent/候选。
2. 确认将要修改的内容；如凭据无法安全复用，只补充必要凭据。

页面主状态应为：

1. `discovering`：正在自动读取本机 Agent、现有上游和 cc-switch。
2. `ready`：已得到唯一高置信度方案，可直接连接。
3. `needs-choice`：只展示发生歧义的 Agent 和候选，不让用户重复填写已识别字段。
4. `needs-credential`：保留已识别 URL、协议和模型，只请求缺失凭据。
5. `applying`：安装/启动服务并执行配置事务。
6. `verified`：连接已验证，显示真实连接路径和回退入口。
7. `already-connected`：现有配置已同步，不生成无意义 diff，不重复安装或重启。
8. `partial/blocked`：明确指出哪个 Agent 或哪一层失败，允许跳过不支持项或修复后重试。

页面应显示：

1. “已自动读取 X 个 Agent、Y 个上游、Z 个 cc-switch profile”。
2. 默认只展示一句人话结果，例如“OpenCode 将保留 OpenCode Zen，并通过 d-token 连接”。
3. 连接路径、协议、端口和 diff 放在可展开详情中，不占据主流程。
4. 唯一候选自动选中；多候选只展示选择器，不展示手填 URL 为默认动作。
5. `out-of-sync` 时直接显示“同步到当前 cc-switch profile”。
6. 只有 `unsupported/unavailable` 才显示“进入高级安装”。
7. 服务安装/启动是“连接”动作的内部阶段；除非发生端口冲突、权限错误或进程身份异常，不要求用户先单独操作服务。

#### 自定义入口：从自动方案渐进展开

高级安装必须完整保留 PRD 规定的自定义能力，但不能要求用户从空白表单重新配置。每个自动生成的连接方案都提供“自定义此方案”，进入高级安装时自动带入：

1. 已选择 Agent 和当前配置来源。
2. 已识别 provider、协议、base URL、模型和凭据引用状态。
3. 当前 d-token/cc-switch 连接路径与端口。
4. 自动计算的 chain mode、fallback、超时、重试和健康检查默认值。
5. 原方案 hash，确保用户编辑期间源配置变化时重新审阅。

高级安装按渐进层级组织：

1. 基础：Agent、目标上游、是否经过 d-token、是否经过 cc-switch。
2. 网络：d-token 端口、cc-switch 端口、base URL、协议和 chain bridge。
3. 可靠性：fallback、超时、重试和健康检查。
4. 诊断：日志级别、连接检查和风险详情。

必须提供“恢复自动推荐”按钮，且自定义后的方案仍走同一套 diff、backup、confirm、verify、rollback 事务，不能产生第二套写入逻辑。

#### 智能学习

移除“输入真实项目目录”作为扫描前置条件，改为：

1. 仅在用户进入智能学习上栏后自动全面扫描；App 冷启动时不启动全盘扫描。
2. 有新鲜缓存时立即显示缓存，再在后台做低优先级增量刷新。
3. 顶部扫描状态卡：阶段、Agent 覆盖、会话数、项目数、耗时、停止/重新扫描。
4. Agent 来源过滤：Claude Code、Codex、OpenCode、Cursor、Cline、CommandCode。
5. 候选列表：分数、最近活动、关联 Agent、会话数量、排序原因、风险。
6. “添加额外文件夹”作为次要补充入口，而不是主入口。
7. 用户选中项目后才进入深度扫描、LearningContext、提示词/委派和 managed block 写入。
8. 隐私说明改为“全面扫描在本地完成；默认只索引 metadata，不上传或展示会话正文与凭据”。

### 1A.7 实施顺序

#### Gate 0：契约和 fixture 先行

1. 固化 native 数据模型、source/freshness/state/error 语义。
2. 为六类 Agent 建立临时 HOME 下的 session/config fixture，禁止直接用开发机真实目录作为自动化测试输入。
3. 建立敏感字段和路径泄漏断言。
4. 先写当前失败用例：无手填 URL 自动发现、无 projectRoot 自动扫描、全 Agent coverage、取消与大目录响应。

#### Gate 1：自动上游发现闭环

1. 将 Agent config/current upstream 探测迁入 native adapter registry。
2. 扩展 upstream snapshot 和 Agent 级 binding。
3. 实现唯一候选自动解析、多候选显式选择、凭据缺口和 unsupported 状态。
4. 实现 cc-switch 同步比较与变化监听。
5. 将解析结果接入 route review/apply/verify 事务。
6. 最后改造一键安装 UI，删除默认“先进入高级安装”的错误分支。

#### Gate 2：智能学习全面发现

1. 建立 native scan job、取消 token、事件进度和状态恢复。
2. 实现六类 Agent session root 发现和 adapter 版本检查。
3. 枚举全部 session 记录并提取白名单 metadata。
4. 解析 session -> project 关联并 canonical 去重。
5. 合并 Git、常用项目根、编辑器工作区和 d-token 历史。

#### Gate 3：轻量索引、评分和缓存

1. 实现项目标记、Agent 文档、语言指纹和风险索引。
2. 实现 PRD 评分公式和解释。
3. 建立 SQLite metadata cache 和增量刷新。
4. 完成大目录、损坏记录、未知格式、符号链接循环和权限错误测试。

#### Gate 4：智能学习 UI 与深度流程

1. Smart Learning 打开即自动扫描或显示缓存并刷新。
2. 实现进度、Agent 覆盖、候选增量、停止、过滤和排序解释。
3. 接入用户选择后的深度扫描和 LearningContext。
4. 复用现有 managed block、委派和 exact rollback，但更新输入来源与隐私文案。

#### Gate 5：跨页面证据和发布门槛

1. 扫描/同步事件进入安装日志、全局日志、Changes 和 Doctor。
2. Dashboard/Settings 显示 Smart Learning coverage、缓存新鲜度和 cc-switch sync 状态。
3. macOS 与 Windows 使用相同 fixture contract，分别完成真实路径存在性和 UI 响应验证。
4. 未达到验收前，`IMPLEMENTATION_STATUS.md` 只能描述“基础实现存在”，不得宣称全面扫描或自动同步已完成。

### 1A.8 预计涉及文件与模块

现有文件：

1. `code/apps/desktop/src-tauri/src/upstream_discovery.rs`
2. `code/apps/desktop/src-tauri/src/main.rs`
3. `code/apps/desktop/src/app/components/pages/Install.tsx`
4. `code/apps/desktop/src/app/components/pages/SmartLearning.tsx`
5. `code/apps/desktop/src/app/lib/agents/registry.ts`
6. 对应 frontend/Rust/E2E 测试。

建议新增 native 模块：

1. `agent_discovery.rs`
2. `route_resolver.rs`
3. `learning_discovery.rs`
4. `learning_index.rs`
5. `learning_scoring.rs`
6. `learning_cache.rs`
7. `agent_session_adapters/` 下各 Agent adapter。

如果模块继续全部堆入 `main.rs`，会加剧当前单文件过大、测试和并发边界难以审查的问题，因此本任务应同时完成窄范围模块拆分，但不得借机重构无关 Tauri command。

### 1A.9 验收标准

#### 自动上游

1. 打开一键安装后自动读取现有配置，不要求先进入高级安装或手填 URL。
2. 唯一高置信度上游自动解析并显示完整连接路径。
3. 多 provider/upstream 时完整列出并要求显式选择，不静默覆盖。
4. 每个 Agent 可保留自己的原上游，不再依赖单个全局 model。
5. cc-switch profile 改变后 UI 识别 `out-of-sync`，不会继续静默使用旧上游。
6. 所有写入仍具有 diff、backup、confirm、verify 和 rollback。

#### 智能学习

1. 不输入项目路径、不依赖已有 d-token 请求，也能自动开始扫描。
2. 对所有已支持 Agent 的实际存在会话根执行枚举，并明确显示每个 Agent 的 scanned/missing/unsupported/error 状态。
3. 能从会话 metadata 发现真实项目，合并重复项目并按价值排序。
4. UI 实时显示阶段、会话数、候选数、跳过数，并可停止。
5. 大目录和大量会话不会阻塞 UI；首批结果流式出现。
6. 缓存支持增量刷新，未变化环境不会每次全量重扫。
7. 任何结果、日志、通知、缓存和导出都不包含完整 prompt、response、源码、token、cookie 或密钥。
8. managed block 写入仍只修改 d-token 管理块，用户原文保持不变并可精确回退。

### 1A.10 明确不在本轮默认范围

1. 静默修改 cc-switch 配置或自动安装第三方 Agent。
2. 默认上传会话记录或源代码到远端模型。
3. 在轻量发现阶段读取完整会话正文或完整项目源码。
4. 未经确认自动写入全局 CLAUDE.md/AGENTS.md 或第三方 Agent 配置。
5. 为未知 CommandCode 格式编写宽松猜测解析器。

本节获得确认后，实施必须严格按 Gate 0 -> Gate 5 顺序推进；不得先改 UI 文案掩盖 native discovery、任务进度和 adapter 缺失。

### 1A.11 多维度审查与修订结论

本轮从产品简单性、自定义能力、状态逻辑、事务安全、跨 Agent 一致性、隐私和性能七个维度复核。结论是：总体技术方向正确，但原计划仍偏工程视角，需要把内部复杂度进一步隐藏，同时补齐全 App 性能门禁。

| 维度 | 审查结论 | 修订要求 |
|---|---|---|
| 默认安装简单性 | 原四步对开发者合理，但普通用户仍可能被迫理解服务、route、provider、model 和 cc-switch | 普通路径改为“自动读取 → 必要时补一个选择/凭据 → 确认连接 → 自动完成” |
| 自定义能力 | PRD 字段完整，但从空白高级表单开始会让用户重复配置 | 高级安装必须从自动方案预填进入，并支持恢复自动推荐 |
| 状态逻辑 | 服务状态、上游状态、Agent 状态和验证状态容易在 UI 中互相覆盖 | 建立单一安装状态机；内部保留分层证据，主界面只展示当前阻塞点和下一动作 |
| 幂等性 | 已连接状态仍可能生成重复操作或无意义 diff | `already-connected` 必须为一等状态；相同 hash/route/config 不写入、不重启 |
| 多 Agent 事务 | 多个 Agent 同时配置时可能形成难以解释的部分成功 | runtime 安装与 Agent 配置分层事务；所选 Agent 配置批次默认原子回退 |
| 安全与隐私 | 原计划边界基本正确 | 保持只读自动发现、显式写入确认、凭据引用、双 hash、备份和精确回退 |
| App 性能 | 原计划只细化了智能学习扫描性能 | 新增 §1B 全 App 性能治理，覆盖启动、导航、状态刷新、Native IPC、日志、图表和后台任务 |

#### 安装状态机的最终逻辑

安装与连接只允许一个主状态驱动主 CTA，禁止页面同时出现多个看似同级但互相冲突的主按钮：

```text
idle
  -> discovering
  -> already-connected
  -> ready
  -> needs-choice / needs-credential
  -> review
  -> applying-runtime
  -> applying-agent-configs
  -> verifying
  -> verified
  -> partial / blocked / rolled-back
```

状态规则：

1. `discovering` 自动开始，不需要用户先点击“检测”。手动刷新只是重新扫描，不是正常必经步骤。
2. `already-connected` 显示当前连接路径和“重新验证”，不显示“安装”或“连接”主 CTA。
3. `ready` 的主 CTA 使用结果导向文案，例如“连接 OpenCode”，不使用“执行事务”“应用 route”等工程术语。
4. `needs-choice` 只展开有歧义的字段；其他 Agent 和上游保持自动解析结果。
5. `needs-credential` 只请求必要凭据，并写入操作系统凭据库；不要求重新填写 URL、协议和模型。
6. `review` 默认显示人话摘要，技术连接路径、文件 diff、hash 和备份位置按需展开。
7. `applying-runtime` 仅在 runtime 缺失、版本不兼容或未运行时执行；已正常运行时必须跳过。
8. `applying-agent-configs` 只修改用户已选择且可安全配置的 Agent；不支持项不进入批次。
9. `verified` 必须来自配置复读、服务健康和真实 route evidence，不能仅凭写文件成功。
10. `partial/blocked` 必须说明保留了什么、回退了什么、用户下一步做什么。

#### 安装事务分层

为兼顾简单性和可恢复性，安装不应把所有动作塞进一个不可解释的大事务：

1. **Runtime 事务**：负责 d-token runtime 安装/升级/启动、端口和进程身份验证。它独立幂等、独立回退。
2. **连接计划事务**：负责 d-token active route、cc-switch 同步计划和所选 Agent 配置。
3. **Agent 配置批次**：先为所有选中 Agent 完成 plan、diff 和 backup，再逐个应用与验证；任一选中 Agent 失败时，默认回退本批次已修改的 Agent 配置，避免用户得到无法理解的半连接状态。
4. Runtime 已安全安装但 Agent 批次失败时，保留 runtime，不伪装为全部失败；UI 明确显示“服务已安装，Agent 配置已回退”。
5. 用户可以在失败后缩小 Agent 选择重新执行，不需要重新安装 runtime。
6. review 期间任何源配置 hash 变化都必须使计划失效并重新扫描，防止 TOCTOU 覆盖。

#### 自定义能力的逻辑边界

1. 自动模式和高级模式必须共享同一数据模型、校验器和事务引擎。
2. 高级模式只增加 override，不复制一套 route/provider 写入实现。
3. override 分为全局默认和 Agent 级覆盖；Agent 级覆盖优先，未覆盖字段继续继承自动发现结果。
4. base URL、协议、端口和 chain bridge 必须实时交叉校验，禁止构造回环、端口冲突或协议不兼容路径。
5. fallback 默认关闭或沿用已验证配置；用户开启时必须显示触发条件和可能走向，禁止静默改走其他 provider。
6. 自定义方案应可保存为不含密钥的本地模板；凭据只保存操作系统引用，不进入模板、日志或导出。
7. “恢复自动推荐”必须只清除 override，不删除用户原有第三方配置。

#### 关键边界场景

实施和测试必须覆盖：

1. 只检测到一个 Agent、一个当前上游。
2. 多个 Agent 各自使用不同 provider。
3. 同一 Agent 存在多个 provider/model，但只有一个当前激活项。
4. cc-switch 正在运行、未运行、端口冲突、profile 已切换和 schema 不支持。
5. 已识别 URL/model 但凭据引用不可用。
6. d-token 服务已运行、版本过旧、端口被非 d-token 进程占用。
7. Agent 配置在 review 后被其他程序修改。
8. 某个 Agent 不支持自动写入，但其他 Agent 可正常连接。
9. 已连接且配置完全一致，重复进入页面不得产生写操作。
10. 多 Agent 批次中途失败并精确回退。

## 1B. 全 App 性能与响应性治理（与 1A 同优先级，实施中）

状态：2026-07-15 已完成前后端性能审查与多维复核，并进入 Wave 0/1 实施。当前工作树已接入有界 metadata-only WebView/Native/core timing、独立增量 Dashboard summary、Learning 锁外评分/原子取消、Native summary/detail single-flight、summary slice revision 和真实 release-profile core summary microbenchmark；同一 macOS release `.app` 已完成页面切换、主题、服务启动以及 Overview 实时恢复验证。双平台、冷/暖启动分布、CPU/RSS/IPC、30 分钟/24 小时 soak 仍未完成。

### 1B.1 当前生产构建基线

2026-07-13 在当前工作树执行 `npm run build`：

1. 共转换 2,808 个模块，构建时间约 2.95 秒。
2. 初始主 chunk：430.54 kB，gzip 143.13 kB。
3. 图表共享 chunk：375.95 kB，gzip 104.19 kB；必须保持只在分析页按需加载。
4. `InstallWorkbench`：82.85 kB，gzip 23.13 kB。
5. CSS：152.82 kB，gzip 25.10 kB。
6. 启动图标 PNG：281.30 kB，需要确认是否进入冷启动关键路径并压缩。

这些数字只用于当前基线，不等同于真实 Tauri 冷启动、CPU、内存或交互延迟结论。

2026-07-15 当前工作树重新构建基线：2,832 modules、3.29 秒。移除把 Radix/Motion/Lucide 跨 Shell 与懒页面强制合并的大 vendor 后，Vite manifest 递归静态图的初始 JS gzip 为 160,007 B（156.26 KiB），低于 163,840 B 门限；charts 103.72 kB gzip 保持 route-lazy，不在 entry 静态图。`npm run build` 已串联 manifest 递归、gzip level 9 和 eager-charts 检查。该结果属于 dirty working-tree development baseline，不是发布候选证据。

当前代码级进度：

1. core `/dashboard/summary` 已使用启动时一次重建、terminal commit 后增量更新的 `RuntimeSummaryReadModel`，不再先构建 full payload，也不再每次扫描 10,000 request/CCR/memory records。
2. release-profile summary microbenchmark 在 0/1k/10k/100k 历史下 p95 分别为 34/151/144/153 µs，满足 core 内部 25 ms 预算；Tauri/WebKit 端到端 p95 仍待重建 App 实测。
3. Smart Learning final project index 已移出全局 jobs Mutex，取消使用每 Job `Arc<AtomicBool>`；阻塞 indexer 下 cancel acknowledgement 的确定性回归小于 250 ms。
4. WebView、Native 与 core 已建立 256 条有界 timing window；字段仅含固定 operation/phase、duration/count/cache hit/result/error code，不记录 invoke 参数、自由文本错误或内容数据。
5. Native summary/detail cache 已 single-flight；summary revision 改为固定大小的 slice revision tuple。detail revision、shared loopback client、cursor/delta、selector store、统一调度器和 bounded runtime rings 仍待后续 Wave。
6. terminal persistence 当前仍保留四次 durable JSONL sync；已增加每阶段 duration、sync count、ContextStore lock/projection 与 total p50/p95/p99 观测。canonical commit 只有完成 fault injection/双写/重放后才能切换。
7. production build 已强制执行 160 KiB 初始递归 JS 门；2026-07-15 最新干净 Tauri 候选构建为 160,769 B，且 `charts-vendor` 不得进入 entry 静态依赖图。
8. Tauri loopback GET 已记录固定 endpoint 枚举对应的 connect/configure/write/read/validate/http_total/parse/total 阶段和 health cache hit/miss；响应内容、URL、端口和业务身份不进入性能窗口。GET body 复用原分配剥离 header，避免 service JSON/text 再次切分复制；blocking `TcpStream + Connection: close`、响应上限和 typed shared client 仍未完成。
9. 前端 Wave 1 已加入按 key 合并、single-flight、visibility-aware 的统一刷新调度器并迁移 runtime summary 与 connection discovery；Smart Learning 的“生成文档草稿”和“保存项目理解”已补齐就地状态、顶部通知、自动定位、diff/confirm 受控写入，并允许从持久化分析结果继续操作。Visual Compression 与 Smart Learning running-job fallback 仍待迁移。

### 1B.2 代码静态审查发现的主要风险

1. `AppShell` 使用 `AnimatePresence mode="wait"`，页面切换需要等待约 180 ms 退出动画后再进入新页面，会制造按钮已点击但页面迟迟未切换的感受。
2. `Advanced` 自己每 3 秒刷新 active config、route drafts 和 project policy，与 App 全局 8 秒 runtime snapshot 并存，形成重复 IPC、文件读取和 React 更新。
3. 全局 `get_desktop_data` 每次刷新会读取 route drafts、rollback points，并顺序请求 health、dashboard、compatibility、protocol injection 和 chain status；本地 HTTP 读写超时当前可到 2 秒。
4. 全量 runtime payload 通过单一 React Context 更新，任何 slice 变化都会使当前页面、状态栏和其他消费者重新计算，缺少 selector 和结构共享。
5. Logs、Changes、Doctor、Analytics 等页面存在逐项 motion/stagger；数据量增长后会增加布局、合成和首次渲染成本。
6. 日志、事件、候选项目和会话结果尚未形成统一分页/虚拟列表策略。
7. Smart Learning 全面扫描如果在 App 冷启动执行，会与首屏和 runtime 状态读取争抢磁盘与线程；因此只能在进入对应上栏后启动。
8. Tauri `main.rs` 已接近 6,000 行，阻塞文件/进程/HTTP 操作和 command 注册集中，难以独立测量、限制并发和定位慢调用。
9. 图表库 chunk 较大，虽然已拆分，但仍需要防止 Overview 或 Shell 的共享 import 把它重新拉入启动路径。
10. 当前缺少统一的性能 telemetry，用户所感知的“卡”无法区分是 UI 动画、React 长任务、IPC、文件系统、服务 HTTP 还是外部 CLI 导致。

### 1B.3 性能架构原则

#### A. Event-first，Polling 只做兜底

1. 服务启动/停止、配置应用、route 变化、cc-switch 变化、Doctor 完成、智能学习进度均通过 Tauri event 主动失效对应 slice。
2. App 只保留一个全局低频兜底刷新；页面不得再创建独立固定间隔轮询。
3. 窗口隐藏时停止非必要轮询和动画；重新可见后只执行一次合并刷新。
4. 手动刷新加入同一 in-flight 请求，不并发重复执行。
5. 配置和文件监听必须限定到已识别文件，使用 debounce，不监视整个 HOME。

#### B. Native 快照缓存与按需详情

1. Native 建立 `OperationalSnapshotStore`，每个 slice 有 revision、source、freshness、collected_at 和 error。
2. `/dashboard/payload` 或 Tauri summary command 只返回首屏需要的聚合摘要；完整日志、事件、route history、backup 和 Doctor detail 按页分页读取。
3. health、dashboard 和兼容性读取并行化，或由 d-token core 提供一次聚合快照，避免每次轮询顺序发起多个本地 HTTP 请求。
4. 本地连接必须快速失败；不能让多个 2 秒超时串联形成数秒卡顿。
5. 配置文件、route drafts 和 rollback points 使用 revision/mtime 缓存，无变化时不重复反序列化全部内容。
6. 所有文件扫描、CLI、数据库和本地 HTTP 阻塞工作进入受限 worker；Tauri command 调度线程不直接执行重活。
7. 后台任务分优先级：用户点击和服务状态最高，页面数据其次，智能学习增量扫描最低。

#### C. Frontend 细粒度订阅

1. 将单一大 Context 改为 selector store 或 `useSyncExternalStore` 风格订阅；页面只因自己使用的 slice/revision 变化而重渲染。
2. 大数组不随全局 snapshot 每次复制；日志、events、changes、candidates 使用稳定 ID、增量合并和结构共享。
3. 对重计算函数设置明确 memo 边界，禁止仅为形式使用 `useMemo` 而仍依赖整个 payload object。
4. 页面导航立即更新选中状态；取消 `mode="wait"` 的串行退出等待，动画不得阻塞页面替换。
5. 页面级 lazy loading 保留，并在用户 hover/focus 导航项时做意图预加载；不得在启动时预加载所有页面。
6. motion 只用于小范围状态变化。实时列表、长列表和批量结果禁止逐行 stagger 动画。
7. 日志、Changes、Doctor 结果和智能学习候选超过阈值后必须虚拟化；搜索、过滤和排序放入 deferred/worker 或分批计算。
8. 图表只在可见页面和可见容器中创建；切页后释放不再使用的 observer、tooltip 和 animation。
9. 启动资产做尺寸和格式优化，避免大 PNG 进入首屏关键加载路径。

#### D. 后台扫描与长任务

1. Smart Learning 进度事件按时间窗口批量发送，禁止每个文件触发一次 IPC 和 React 更新。
2. 扫描任务支持取消、暂停、低优先级和磁盘/CPU 并发上限。
3. JSONL 使用流式解析，SQLite 使用只读分页查询，大目录不一次性载入内存。
4. 用户离开智能学习上栏时，轻量扫描可以按设置继续低优先级运行；深度扫描和委派必须显示持续任务状态，不能无提示占用资源。
5. App 退出、窗口关闭或任务取消后，worker、文件 watcher 和子进程必须释放，不允许后台泄漏。

### 1B.4 性能预算

以下为候选版目标，需要在固定参考硬件、相同候选 commit 和 release build 中记录 p50/p95：

1. 用户点击、切换标签或主 CTA 后 100 ms 内必须出现视觉反馈。
2. 已加载页面切换的内容稳定目标 p95 ≤ 250 ms；不得人为等待超过 100 ms 的退出动画。
3. 冷启动到 Shell 可交互目标 p95 ≤ 1.5 秒；暖启动目标 p95 ≤ 800 ms。
4. 在线 runtime summary 刷新目标 p95 ≤ 250 ms；本地服务离线时 fail-fast 目标 p95 ≤ 500 ms。
5. 正常页面交互不得出现超过 100 ms 的 JavaScript long task；连续进度更新不高于每秒 10 次。
6. 窗口可见且空闲时平均 CPU 目标 ≤ 1%；窗口隐藏时停止计划轮询，除明确后台任务外不持续占用 CPU。
7. 30 分钟空闲与页面往返后，内存不得持续单调增长；稳定区间漂移目标低于 5%。
8. 初始 JS gzip 暂设不超过 160 kB，任何增长必须解释；图表等重依赖必须保持 route-level lazy chunk。
9. 普通页面单个业务 chunk gzip 目标 ≤ 50 kB；超出时必须拆分或给出不可拆原因。
10. 10,000 条日志/事件、20,000 条 session metadata 的 fixture 下，列表只渲染可见行，滚动和过滤不得冻结 UI。
11. 智能学习首批候选目标 1.5 秒内出现，取消请求 250 ms 内进入 stopping/stopped 状态。
12. 所有 Native command 都记录 duration、result state 和非敏感 workload count；p95 超预算时进入性能回归报告。

### 1B.5 性能观测与基准方法

1. Frontend 使用 Performance API 标记 `app_boot`、`page_navigation`、`runtime_refresh`、`dialog_open`、`learning_first_result`。
2. React Profiler 记录各页面 commit 次数和最慢组件，重点检查 Runtime Context 更新后的无关重渲染。
3. Native 使用结构化 span 记录 command、文件数、HTTP 子请求、缓存命中和耗时，不记录路径正文、密钥或会话内容。
4. 建立 small/medium/stress 三档 fixture：Agent 数、session 数、项目数、日志数、route 数和损坏记录比例固定可复现。
5. 对本地服务在线、离线、端口冲突、配置文件损坏、cc-switch 大数据库分别测量。
6. 生产构建输出保存 chunk manifest，并在 CI 比较初始 gzip、页面 chunk 和大资产变化。
7. Tauri 真实窗口至少记录冷启动、连续切页、安装扫描、Doctor、日志滚动和智能学习扫描场景。
8. 性能证据必须绑定平台、版本、commit、release/debug 模式和参考硬件；开发浏览器成绩不能替代 Tauri release 候选。

### 1B.6 性能实施顺序

#### Perf Gate 0：先测量

1. 加入前端 Performance marks 和 Native duration spans。
2. 固化当前 production bundle manifest。
3. 建立 reference fixture 和 Tauri smoke 场景。
4. 在优化前记录 CPU、内存、IPC 次数、React commit 和 command p95。

状态：**in progress**。代码观测、Vite manifest、core summary release microbenchmark，以及同一 macOS release `.app` 的导航、全部懒页面、主题、服务启动和实时状态恢复已完成。最终重建包运行中的 `/dashboard/summary` 50 次顺序请求为 p50 3.220 ms、p95 7.643 ms、p99 11.523 ms、max 25.930 ms；core 内部累计 56 个 summary build 样本为 p50 1,216 µs、p95 5,936 µs、p99/max 21,522 µs。遍历全部懒页面后的 30 秒可见窗口样本中，App 主进程平均 CPU 0.23%、RSS 平均 113.37 MiB（84.19–117.67 MiB），runtime sidecar 平均 CPU 0.00%、RSS 11.87 MiB；该采样不含无法可靠归属的系统 WebKit helper，不能替代 30 分钟/24 小时资源门。以上数据只绑定当前 dirty working-tree、ad-hoc signed macOS release build；冷/暖启动、完整进程组 CPU/RSS/IPC、WebKit Long Task、soak、Developer ID/notarization 与 Windows 基线未完成。

#### Perf Gate 1：消除感知卡顿

1. 去除页面切换串行等待动画。
2. 所有按钮和扫描动作在 100 ms 内进入 pending 状态。
3. 删除 `Advanced` 独立 3 秒轮询，统一进入 event-first snapshot。
4. 禁止长列表逐项进入动画。

#### Perf Gate 2：缩减数据和 IPC

1. Native snapshot cache + revision。
2. 聚合 summary 与分页 detail 分离。
3. 并行/聚合本地 HTTP 读取，离线快速失败。
4. 日志、events、backups 和 route history 按需加载。

状态：**partial**。core summary read model、Native 300 ms keyed single-flight、summary slice revision、loopback HTTP 分阶段观测和 GET body 单分配复用已完成；detail cursor、typed/shared loopback client、response cap、连接复用和历史分页未完成。

#### Perf Gate 3：减少 React 工作量

1. selector store 与 slice revision。
2. 大数组结构共享、虚拟列表和 deferred filtering。
3. 页面 intent prefetch 和重依赖可见时加载。
4. 优化启动图像和不必要的共享 import。

状态：**partial**。强制跨页面 vendor 合包已移除，初始递归 JS 从 175,680 B 降至 160,007 B，并加入 build-time 门禁；selector store、虚拟列表、intent prefetch 和大型详情增量合同仍未完成。

#### Perf Gate 4：长任务与资源治理

1. 智能学习 worker、取消、节流、缓存和优先级。
2. watcher、子进程、timer 和 observer 生命周期测试。
3. 大型 fixture 压力与 30 分钟 soak test。

#### Perf Gate 5：CI 与发布门禁

1. bundle size budget。
2. 核心 command benchmark 与回归阈值。
3. macOS/Windows 真实窗口 p95 证据。
4. 未达到预算的候选不得使用“流畅”“低占用”作为发布声明。

### 1B.7 性能验收标准

1. 页面点击后立即有反馈，不再出现按钮按下无状态变化。
2. 页面切换没有固定退出等待，首次 lazy load 有明确骨架状态。
3. App 只有一个运行证据调度器，不存在页面级重复固定轮询。
4. runtime summary 不再携带所有日志、历史和详情数据。
5. Native 慢操作可定位到具体 command 和子阶段。
6. 长列表使用分页/虚拟化，数据量增长不会线性增加 DOM 数量。
7. Smart Learning 全面扫描不影响 Shell 导航、状态栏和安装页交互。
8. 窗口隐藏后非必要刷新停止，重新显示时状态可正确恢复。
9. chunk、CPU、内存、IPC 和交互 p95 均有绑定候选 commit 的可复现实测报告。
10. 性能优化不允许牺牲 source/freshness、错误可见性、事务验证和回退安全。

## 1C. 当前唯一实施主线：Headroom 后端全能力迁移（实施中）

当前参考基线固定为 `headroomlabs-ai/headroom@main` / `01df2452529a86c689cf226fecd5918cc5d19676`（2026-08-02，未发布 tag）；前一发布基线 `v0.33.0` / `28aa53dc7ca51e687cc719c3fe160f3be50c6570`，历史 commit `1725cd1f8376828849c0e48fd9008afa25d24fff` 只解释旧证据，不再作为完成口径。当前先做行为与接口对照；如复制或修改上游源码，必须在动手前补齐 NOTICE、文件清单和修改说明。外部 Headroom 8787 只允许 no-touch 只读对照。

v0.33 delta 必须纳入 G0–G8：compressor registry、route-bound `/v1/compress` gateway/SDK surface、递归/混合/config 内容路由、cold-prefix/thinking compaction、tool-schema savings、streamable MCP、CCR sliding TTL、persistent metrics 与新增 Provider/Agent 路由均须逐项标为真实、部分、缺失或不在范围。2026-08-02 首批已接入 Rust-owned 内置 compressor registry 和不转发请求的显式 route-bound `/v1/compress`；动态 Python entry point/进程内第三方插件继续禁止，第三方中转通过 d-token RouteProfile 与受限 HTTP/MCP/SDK 合同接入。

状态口径：`真实` 表示已有 d-token-owned production 执行路径与运行证据；`部分` 表示仅覆盖子集；`占位` 表示只有同名 endpoint/metadata/UI；`缺失` 表示没有可用实现；`不安全默认` 表示有代码但 release 默认可能返回 fixture、测试身份或未经计算的成功证据。占位、缺失和不安全默认均为发布阻断。

实施顺序合同：Headroom 全能力迁移的完成声明必须先研究/实现/验证后端，G0–G8 全部通过前不得用前端显示掩盖后端缺口。该约束不阻止修正现有正式页面的错误信息架构、安全门禁或 fail-closed 语义；这类改动仍必须绑定当前真实 Tauri/Rust 合同，且不能据此宣称 Headroom 迁移完成。1A/1B 中与自动发现、学习、性能有关的 **Rust 后端** 会并入本节对应切片；全量迁移后的页面能力统一与候选实窗验收留到 G9。

| 能力 | 当前事实 | 目标与最小完成证据 | 优先级 |
|---|---|---|---|
| Local proxy、OpenAI/Anthropic 协议与显式路由 | 部分真实：OpenCode routed flow 已通过，其他 Agent/Provider 矩阵未闭环 | official/custom/cc-switch/chain 五类 route，错误透传、无静默 fallback、每类 Ledger 证据 | P0 |
| Library/SDK/middleware | 部分真实：Rust API、HTTP、MCP 与薄 TS SDK 已共享 decision contract；新增 `/v1/compress` 返回 route-bound validated candidate body、不发送上游、不持久化请求，并与 MCP/SDK 共用 execution contract。framework adapters 与最终候选复证仍缺 | 稳定 Rust API；薄 TS binding/framework adapters 与 proxy 共用同一 pipeline 与生命周期 | P1 |
| Agent wrap/unwrap | 部分真实：20 项检测矩阵均有显式能力状态；7 个版本化 transport adapter 具备 Configure/Clear，Claude/Codex/Cline 已完成真实临时事务与精确恢复；其余 fail-closed | 每个宣称支持自动写入的 Agent 使用 detection → preview → backup → apply → verify → rollback；未验证项保持 unavailable | P0 |
| MCP server/tools | 部分真实：有界 stdio JSON-RPC、`compress`/`retrieve`/`delete`/`stats`、权限、取消与集成测试已实现；`headroom_compress` 与 `/v1/compress` 现复用同一 execution contract，OpenAI Chat 和 direct Anthropic 同协议 route 可解析，跨协议与 streamable HTTP MCP 尚未闭环 | 全协议 route-resolved MCP、受控 capability transaction、错误传播、streamable HTTP 与最终候选 integration evidence | P0 |
*|| ContentRouter | 本地 gate 通过：resolved production path 已接类型化分类、confidence/size/deadline budget、JSON/log/search/build/diff/HTML/code/text/config/passthrough、manifest/post-validation；混合/多类型内容已接入 IR-safe section router（逐 section 走同一 codec/secret/marker/负收益门，统一 CRLF 保留、混合行尾 fail-closed），config 与 mixed 均已进入 `compress_resolved`/`/v1/compress` 生产路径；tool-schema PR-E1/E2 已接入且对客户 `cache_control` 位置敏感 fail-safe，PR-E3 自动放置仍由 proxy 层在 provider cache evidence 门控下执行；v0.33 风格 Rust-owned compressor registry 已登记九类 active lossless codec（evidence id 列表由 registry 编译期投影），生产路径不动态加载第三方代码；普通图片文件压缩不在范围 | 将同一 active decision 绑定最终 Provider/Agent 真实质量、延迟与 Led...
| SmartCrusher / JSON compressor | active candidate：lossless minify、sealed manifest/semantic validator、128+ property/semantic/rollback、negative/budget/cancel 门零失败 | 绑定最终候选 commit 与真实 Provider 质量/延迟证据 | P0 |
| CodeCompressor | active candidate：Rust/TS/JS/Python/Go/Java/C/C++ 均进入 production codec evidence；128 property/semantic/rollback、8 corpus、negative/budget/cancel 门零失败 | 绑定最终候选 commit、真实 Agent 请求与 provider quality/latency evidence | P0 |
| Kompress/text compressor | active candidate：Log/Search/Build/Diff/Text/HTML 均具备可逆 transform、manifest 重建验证与 release evidence；风险/secret/负收益严格 passthrough | 绑定最终候选 commit、真实模型质量与性能预算 | P0 |
| Optical code/context → PNG compression | 部分实验：Optical shadow/严格门控路径是 d-token 的正式方向；普通输入图片 resize/re-encode/tile/ONNX 明确不在产品范围 | 完成 Optical provider cache、fidelity/cost/latency、post-validation 与跨平台证据；production active 继续默认关闭 | P0 |
| CacheAligner / live-zone | 部分：通用 Hybrid IR 已生成 FrozenPrefix/TransformableWindow/ProtectedTail 与 route/provider/model-bound stable-prefix cache key；validator 禁止 frozen/non-target mutation；schema v5 已按 exact route/provider/model/protocol/config identity 持久化 Anthropic/OpenAI 成功响应中的 allowlisted cache counters。Direct Anthropic SSE 在 `message_stop` 无 error 时仅从有界瞬时 parser 写入 final usage/cache evidence；不完整/error/drop 不写 observation。显式 cache policy registry 提供 TTL/min-prefix/strategy，且只有 exact fresh observation 同时存在时，OpenAI Chat、Responses-to-Chat 与 Anthropic-to-OpenAI bridge 才生成 64-hex evidence revision 并接入 resolved plan。Direct Anthropic Hybrid、provider 实际 breakpoint 与 raw-wire prefix 证据仍缺 | 冻结前缀 byte-identical、仅压缩新 zone、history 不丢失、provider cache breakpoint/policy 与回归测试 | P0 |
| CCR reversible compression / retrieval | 部分真实：AES-256-GCM、TTL/quota、inspect/retrieve/delete、lifecycle journal/reconciliation、SQLite projection rebuild、Unix permissions 与 Windows protected DACL 实现已完成；Windows GNU cross-check 通过。仍缺 MSVC runner DACL 实测、自动 tool-call retrieve 与跨存储原子 crash evidence | 完成最终候选 `prepare → receipt/marker → send → finalize/delete`、权限/审计、tool-call 与双平台故障测试 | P0 |
| Local/per-project/cross-agent memory、SharedContext | 部分：typed 脱敏存储、provenance、去重、项目/Agent/session isolation、session-bound share grant/revoke、删除/导出与受控 MCP list/export/delete/write 已实现；v4 project-wide grants 迁移后 fail-closed | capability transaction 分发、真实 managed Agent 检索/跨 Agent 授权证据与 route-bound application | P0 |
| learn / Smart Learning | 部分：会话 metadata 项目发现、安全结构索引、managed block/rollback 已有；失败挖掘与结构化委派未闭环 | 多 Agent 增量发现、真实项目理解、失败模式提炼、preview/direct 两模式、受控写入与真实回退 | P0 |
| output shaper / verbosity / effort routing | 部分真实：route-resolved policy 已在 core API 应用，保护 error/new-question，支持 holdout 与 measured/estimated/unknown 分账；仍缺真实 Provider/Agent 质量证据 | 绑定最终候选 route evidence、holdout 对照与跨协议验证 | P0 |
| lifecycle/hooks/extensions | 部分 | proxy/library/MCP 共用类型化 request lifecycle；pre/post hooks 有 deadline、取消、隔离、Ledger | P1 |
| durable ledger、metrics、stats/history/export、cost/breakdown | 大部分真实 | 统一 denominator、per-model/agent/project、signed delta、跨重启一致性与 Dashboard/export 对账 | P0 |
| Doctor/perf/benchmark/troubleshooting | 部分真实；benchmark 仅开发面 | release Doctor 只检查产品能力；perf/eval 留在 CI/support；故障注入、资源门与修复回执 | P0 |
| integrated RTK | 部分真实：d-token-owned managed macOS probe 已通过 install/verify、filter、success/failure exit propagation、usage evidence、exact uninstall 与 root cleanup；不保存 argv/raw output/credential value。Windows/WSL 与最终候选 workflow 证据仍缺 | 绑定同一候选的 macOS/Windows/WSL lifecycle、hook integrity、tracking 与 uninstall evidence | P0 |

### 1C.0 后端架构与能力覆盖合同

完整矩阵按 117 个可寻址标题/能力项覆盖以下领域：

1. loopback/protocol/provider/route/auth/header/body/limit/SSE/WS/SSL/retry/quota/request scope。
2. Context IR、live-zone、CacheAligner、ContentRouter、JSON/log/search/diff/HTML/text/code/build/image/Optical、manifest/post-validation。
3. CCR、prompt/compression/semantic cache、memory/SharedContext/relevance、Smart Learning、output/effort/holdout。
4. canonical lifecycle、hooks/tool injection、Rust library、MCP、TS SDK/framework adapters、Agent detection/wrap/config/MCP transaction/unwrap、integrated RTK。
5. JSONL/SQLite、metrics/history/cost/tokenizer/model/offline、Doctor/performance/security/license/SBOM/cross-platform/release evidence。

新增/调整 crate 边界：

```text
dtoken-types                stable contracts only
dtoken-router               route/provider/model capability authority
dtoken-compress             Context IR + CacheAligner + codecs + artifact/validator
dtoken-context-store        SQLite + CCR/private stores + memory/cache state（新增）
dtoken-runtime-pipeline     prepare/validate/finalize orchestration（新增）
dtoken-core-api             loopback/protocol/provider transport + public projection
dtoken-mcp                  stdio JSON-RPC transport（新增）
dtoken-learning             Agent session discovery/mining/managed writer
dtoken-preprocessor         integrated RTK internal capability
```

依赖只能从 types/domain/store 向 pipeline，再向 HTTP/MCP/Tauri 入口单向流动；Tauri 与 React 不承载业务规则。

### 1C.1 实施顺序

1. **B0 真实性与安全基线**：release 默认 native/unavailable；移除测试 Agent/project identity；删除自报 cache-stable；统一 capability/reason/freshness revision。
2. **B1 合同与持久化**：`RequestScope → ContextEnvelope → Prepared → Validated → ReadyToSend → Finalized`；JSONL 审计 + SQLite projection/migration；独立 CCR/memory/cache stores。
3. **B2 核心管线/codecs**：live-zone、CacheAligner、Router、manifest/validator；依次完成 JSON/log/search/diff/HTML/text/build；Code/Image/ML 默认关闭但实现和 eval 完整。
4. **B3 可逆/记忆/学习/output**：CCR two-phase/tool/retrieve；memory/SharedContext/BM25/optional embedding；Smart Learning registry/scanner/miner/writer；verbosity/effort/holdout。
5. **B4 公共入口/Agent/RTK**：OpenAI Chat/Responses/Anthropic 完整协议；Rust library；真实 MCP；薄 TS SDK/adapters；动态 Agent config/MCP/wrap/unwrap transaction；integrated RTK。
6. **B5 可观测/性能/运行强化**：统一 stage/usage/cost/history，worker/cancel/limits，Doctor/backup/restore，安全/供应链/跨平台。
7. **B6 后端验收**：fixture/property/fuzz/fault/quality/perf/security、全 workspace、真实 OpenCode 官方免费模型、macOS/Windows backend/bundle、candidate manifest。
8. **F1 前端阶段**：只有 B6/G0–G8 全部完成后，才可宣称前端已覆盖完整 Headroom 迁移能力，并执行全量 source/freshness/revision/action-result 与 macOS 实窗/重启/实时同步候选验收；既有正式页面的安全修复与错误语义纠正不等待该 gate。

### 1C.2 Headroom 迁移发布 Gate

1. **G0 Truth**：release 无 fixture 默认、无测试身份硬编码、无自报成功证据；capability registry 是唯一状态源。
2. **G1 Contracts**：RequestScope、pipeline type states、CCR transaction、store migration、Agent adapter 与 availability schema 契约测试通过。
3. **G2 Pipeline**：live-zone/CacheAligner/Router/全部 codec/validator 的 fixture/property/fuzz/fault/quality 门通过；未知/失败/负收益 passthrough。
4. **G3 Stores**：CCR、memory、cache、JSONL/SQLite migration、ACL、TTL/quota、delete/export/crash recovery 全部通过。
5. **G4 Surfaces**：HTTP、Rust library、MCP、TS SDK 对同一 fixture 产生同一 decision/reason/artifact；无 transport 时 MCP 不得 configured。
6. **G5 Agent/RTK**：动态 Agent registry、每 Agent config/MCP/wrap/unwrap transaction 与 integrated RTK 在 macOS/Windows fixture 通过。
7. **G6 Ops**：metrics/history/cost/redaction/Doctor/performance/security/SBOM/license 门通过。
8. **G7 Real OpenCode**：正常配置使用当时官方免费模型，证明 Agent → d-token → 全 pipeline → upstream → Ledger/metrics/CCR，且精确恢复、不泄密。
9. **G8 Candidate**：full fmt/Clippy/test/audit/build，macOS/Windows 产物同 commit，哈希、SBOM、NOTICE、已知限制绑定 manifest；矩阵零占位/缺失/不安全默认。
10. **G9 Frontend completeness gate**：仅 G0–G8 全绿后进入“完整 Headroom 迁移已在前端闭环”的候选验收；不得把它解释为冻结现有 UI 的安全修复。

当前 gate 进度（2026-08-03 working tree）：G0/G1 需在最新全仓门禁中复证；G2 九类 production codec release probe 已全绿（`config-codec` 已纳入，evidence id 由 registry 投影），混合/config 内容与 tool-schema PR-E1/E2 已接入 resolved 生产路径，但仍须绑定最终 candidate 与真实 Provider/Agent 质量证据；G3 lifecycle/权限实现已补齐到 Windows DACL 代码与 cross-check，仍缺 Windows MSVC 实测和剩余 crash/tool-call 证据；G4 Rust/HTTP/MCP/TS parity 定向通过；G5 macOS managed RTK、真实 Claude/Codex/Cline 临时事务与 cc-switch 静态切换有证据，其他 Agent 未验证项保持 unavailable；G6–G8、当前候选 OpenCode 和双平台产物仍未完成，因此不能发布。

## 1D. 实验性代码智能图谱接入（实施中，仍未达到发布候选）

产品契约：PRD `P0.7.2.1`。研究输入与上游事实见 `docs/research/CODEBASE_MEMORY_MCP_INTEGRATION_RESEARCH_2026-07-16.md`。

定位：将 `DeusData/codebase-memory-mcp` 作为可选、受 d-token 管理的外部本地 sidecar 接入，而不是复制其 C runtime、调用其 installer，或让它成为 d-token 的路由/压缩/Dashboard 依赖。本工作不改变 §1C Headroom 后端迁移的优先级，也不得以“图谱已打开”替代 G0–G8 的任何发布证据。

### 1D.0 本轮审查修正与固定决策

本轮多维度审查修正了第一版计划中两个会导致错误实现的假设：

1. Agent 通过 stdio 启动的 MCP 进程由 Agent 拥有；d-token 为网页/索引启动的进程由 d-token 拥有。它们不是同一个“受管服务”，不能共享 PID、stdio、停止权限或一个 `running` 状态。
2. 上游原生 UI 不是安全的只读 project-scoped viewer：它包含 `/api/browse`、`/api/repo-info`、index、ADR、delete、RPC、logs 和 process-kill 等端点；直接打开随机 loopback 端口不足以满足 d-token 的权限边界。

固定决策：

- [x] 首期采用轻量分发：App bundle 只含受限 launcher、管理 UI 与许可证/NOTICE，不含大型 upstream；页面仅打开固定官方 `v0.9.0` release，由用户下载解压到默认或自定义目录，App 自动识别并固定 hash/platform/arch 校验。
- [x] 新增独立 `dtoken-codebase-memory-launcher`，负责 artifact 验证、环境/项目/缓存约束、stdio 策略代理和 metadata-only lifecycle receipt；未扩张 metadata-only `dtoken-agent-bridge`。
- [x] Agent 配置指向 launcher，由 launcher spawn upstream；d-token App 只观察 launcher receipt，不接管 Agent-owned stdio 或进程。
- [x] 一键图谱网页由 d-token-owned、带短期授权、只读 endpoint allowlist 的 loopback host 提供；不暴露 upstream 原生 HTTP UI。
- [x] 首期强制 `persistence=false`，并在索引前后检查 `.codebase-memory/graph.db.zst`、`artifact.json` 与 `.gitattributes` 零写入边界。
- [x] 独立 daemon、跨 Agent 共享进程、跨项目图谱、ADR write、上游自动更新、默认 Hook、默认索引和默认监测保持关闭。

### 1D.1 可执行顺序与依赖

| ID | 工作包 | 前置 | 产出 | Gate |
| --- | --- | --- | --- | --- |
| CBM-0 | provenance 与 artifact | 无 | 固定 tag/commit、平台 archive、SHA-256、签名资料、MIT/第三方 NOTICE | 无独立 upstream revision 或 hash 则停止。 |
| CBM-1 | 领域合同与 fake launcher | CBM-0 | package/integration/session/index/status 类型、launcher protocol、fake binary | raw payload 进入日志或状态折叠则停止。 |
| CBM-2 | 项目/缓存/零写入边界 | CBM-1 | canonical root、私有 cache、env allowlist、quota/TTL、repo-write deny | symlink escape、敏感路径或项目写入任一可达则停止。 |
| CBM-3 | Agent MCP/Hook 事务 | CBM-1、CBM-2 | 一个 Agent 的 MCP-only transaction；Hook 独立 consent/receipt | 无 exact rollback 或 Agent-owned 进程被接管则停止。 |
| CBM-4 | d-token-owned index/UI host | CBM-1、CBM-2 | 独立 ui_host、只读 graph projection、短期授权、open-before-recheck | 原生 UI 高风险端点或无授权 loopback 可达则停止。 |
| CBM-5 | Desktop 状态与 UX | CBM-3、CBM-4 | 独立实验性一级页、五维状态、source/freshness/action receipt | 配置/端口/索引被误报为实际 Agent 使用则停止。 |
| CBM-6 | 双平台候选 | CBM-0–5 | macOS/Windows artifact、fixture、实窗和授权后 Agent 证据 | 任一平台只有编译证据则保持 experimental/unavailable。 |

当前工作树状态（2026-07-19）：

- [x] CBM-0 provenance 与 artifact：固定 `v0.9.0`/commit、平台 archive、SHA-256、许可证与 NOTICE 已进入 manifest；运行时只接受当前平台的 pinned binary SHA-256 或明确登记的 macOS universal 派生 SHA-256，同签名者/同证书不替代内容身份；正式 sidecar 准备脚本不下载或打包 upstream，release smoke 另行准备用户导入样本做验证。
- [x] CBM-1 领域合同与 launcher：`dtoken-codebase-memory` crate、受限 launcher、bounded MCP envelope、tool allowlist、metadata-only receipt 已实现。
- [x] CBM-2 项目/缓存/零写入边界：canonical root、私有 cache namespace、quota/TTL、repo write deny、`persistence=false` 与敏感路径拒绝已实现。
- [x] CBM-3 Agent MCP/Hook 事务：自动扫描 OpenCode/Codex/Claude，默认选中全部已安装且受支持 Agent；MCP 支持多选批量 preview/apply/read-back verify，并在任一失败时逆序 rollback；批量 transaction set 持久化并可在刷新/重启后恢复整批回退入口，verify/rollback 从各自 manifest 恢复 Agent/scope/surface 上下文；配置写入后的控制面状态落盘失败会立即补偿回退，回退不完整时返回 durable recovery handle。Hook 当前仅对 Claude Code 暴露并保留独立 consent/rollback。
- [x] CBM-4 d-token-owned index/UI host：独立 index/UI host、短期授权 loopback、只读 graph projection、open-before-recheck 与 stop validation 已实现。
- [x] CBM-5 Desktop 状态与 UX：独立一级“代码图谱（实验性）”页面、五维状态、固定 `Codebase Memory MCP v0.9.0` 官方下载入口和可见版本文案、默认/自定义组件目录、自动识别、Agent 多选批量事务、明确的按钮阻塞原因、index/UI start-stop-open/delete 已接入。批量配置在 Native 返回前即显示 Agent、阶段和 current/total 进度，逐 Agent 串行调度以避免并发 hash storm；artifact 校验、状态构建、配置事务、进程和 loopback 操作移入 blocking worker。261 MB upstream 的 debug 冷校验由约 41.00 秒降至 866 ms，30 秒 single-flight 缓存命中约 25 us，文件 identity 改变时 fail-closed 失效。Shell 横幅只呈现本次运行最新事件、历史保留通知中心；页面切换不再主动聚焦标题。
- [ ] CBM-6 双平台候选：macOS ARM/x64 与 Windows ARM64/x64 四个官方 artifact 的 hash+Cosign 验证、Windows target check、release `.app`/DMG 实构建与真实 OpenCode MCP 验收已有证据。最新 macOS 候选已通过 `build_and_run.sh --verify` 构建并启动；真实 OpenCode 1.14.51 回执为 initialize 1 次、tools/list 1 次、允许 tool call 1 次、拒绝 0 次，随后配置与仓库写入边界精确回退。双平台 workflow 已改为证明安装包不含 upstream；Windows 安装后从独立用户目录导入 pinned artifact，再执行 launcher MCP initialize/tools/list/tool-call、受控索引、只读图页、安全端点、浏览器分发和仓库零写入门禁。仍缺 Windows 远程 runner 报告，因此继续标记 experimental。

CBM-0–6 不进入 §1C G0–G8 的完成计算；即使本节实现完成，也不能替代 Headroom 主线发布 gate。

### 1D.2 领域、launcher 与实例所有权

建议新增 `code/crates/dtoken-codebase-memory/` 作为 d-token 自有领域/策略层，以及一个单用途 `dtoken-codebase-memory-launcher` external binary；不复制上游 C runtime：

- [x] `CodebaseMemoryPackageManifest`：artifact identity、license/provenance、target triple、SHA-256、allowed args/env、headless/UI capability。
- [x] `CodebaseMemoryIntegrationRequest/Plan/Receipt`：一个 Agent、一个 project/global scope、MCP/Hook 分离选择、review digest、target hashes、backup、verify/rollback。
- [x] `CodebaseMemoryInstanceManifest`：`kind=agent_mcp|ui_host`、`owner=agent|dtoken`、instance id、project/cache digest、PID/birth token、launcher/upstream path+hash、stdio/port evidence、started/observed timestamps、resource limits；上游未回显的 nonce 不得伪造为 identity proof。
- [x] `CodebaseMemoryStatusSnapshot`：package/integration/agent_session/ui_session/index 五维状态以及 owner/source/freshness/warnings。
- [x] launcher 对 MCP stream 做有界 envelope 解析和版本化 tool allowlist：普通只读 tool 转发；`index_repository` 必须匹配已授权 canonical root、拒绝 cross-repo 并强制 `persistence=false`；ADR write、artifact export、任意路径覆盖和未知 tool 默认拒绝。launcher 只持久化 method/tool/category 计数，不持久化 params/result，不记录 snippet、graph、prompt、response 或 stderr 原文。崩溃、EOF 和退出回执必须可恢复读取。

DoD：Agent-owned 与 d-token-owned 实例的 stop/restart 权限由类型约束；unknown/partial/unmanaged/stale/changed 可序列化；单元测试覆盖坏 artifact、无效 scope、伪造 receipt、PID reuse、EOF、超限 frame 和 payload redaction。

### 1D.3 供应链、项目范围、缓存与仓库零写入

- [x] 固定并验证 upstream tag/commit、archive URL、target、SHA-256、平台签名资料；同步 `docs/legal/THIRD_PARTY_NOTICES.md`。本地 `.ai-bridge` 研究目录不作为供应链来源。
- [x] launcher 清空继承环境，仅允许固定的 `CBM_ALLOWED_ROOT`、私有 `CBM_CACHE_DIR`、受限 `CBM_WORKERS` 和必要 `CBM_LOG_LEVEL`；显式关闭 diagnostics、自更新、auto-index 和 auto-watch，不接受 `CBM_DOWNLOAD_URL`。
- [x] 每项目创建 Agent/UI 分离的私有 cache namespace，带 quota、TTL、crash 状态恢复和显式删除；同一 d-token cache 的 index/UI 操作串行并显示 busy。
- [x] canonical root 预检拒绝 symlink escape、根目录/磁盘根、d-token state、CCR/private recovery、auth/credential/key、`.env*`、cache/generated/vendor 和超界 workspace。
- [x] 项目 `.codebase-memory.json` 不被 launcher 信任；d-token 生成隔离配置并对未知调用/路径 fail closed。
- [x] 所有 `index_repository` 调用固定 `persistence=false`；索引前后检查用户仓库，发现 `.codebase-memory/` 或 `.gitattributes` 非计划变化立即失败且不自动删除用户原有文件。

DoD：macOS/Windows fixture 覆盖 Unicode/盘符、symlink/junction、目录替换、敏感路径、磁盘根、并发 cache、artifact export 请求和 repo dirty-before/after；任何失败不残留不受控子进程或项目写入。

### 1D.4 Agent MCP 与 Hook 事务

- [x] 扩展 `dtoken-config-patcher`，新增独立 `CodebaseMemoryIntegrationRequest`，目标 command 是 launcher，不混淆现有 Hybrid/metadata MCP。
- [x] 自动扫描本机受支持 Agent，默认多选全部检测结果；为每个选中 Agent/scope 分别生成最小 MCP entry、脱敏 diff、backup 与 review digest，逐目标验证，任一失败逆序回退且不静默跳过。global 配置仍绑定显式批准 root，不随 cwd 静默变化。
- [x] launcher 从已审阅的固定 argv 解析 canonical project 与 cache scope；不接受 Agent 通过 env 覆盖安全边界。
- [x] Hook 独立于 MCP：默认 off，展示 trigger、命令摘要、trust/restart 要求，并产生独立 consent 与 receipt；当前仅 Claude Code 支持。
- [x] 复用 redacted diff、双 hash、durable backup、confirm、atomic write、parse/semantic verify、transaction lock 和 exact rollback；真实 Claude/Codex 配置继续需要单独授权。

DoD：一个 Agent 在隔离目录完成 preview→apply→verify→Agent-spawned launcher receipt→exit→rollback；覆盖并发 hash、半写、launcher 缺失、artifact 失配、项目 moved 和 Hook-only/MCP-only 状态不混淆。

### 1D.5 d-token-owned 索引与安全图谱网页

- [x] `ui_host` 是 d-token-owned 独立实例，通过 headless upstream stdio 读取授权项目的 graph projection；不与 Agent-owned session 共享 stdin/stdout 或 cache namespace。
- [x] 安全网页只暴露授权项目的只读 health/architecture/search projection；不提供 filesystem browse、repo remote/root、raw logs、index mutation、ADR write/delete、project delete、RPC passthrough 或 process kill。
- [x] loopback host 绑定随机可用端口，使用不可猜测、短期、单实例授权；Host/Origin/方法/路径均 allowlist，TTL 到期后授权失效。浏览器只打开 d-token 刚生成并复检的字面 URL。
- [x] 索引、取消、删除 cache 是独立 Tauri commands：确认、状态、取消和 receipt 由 d-token control plane 管理，不由网页直接 mutation。
- [x] 未适配或 bundle upstream UI 源码/assets；高风险原生 UI 端点保持不可达。

DoD：端口抢占、Host/Origin 绕过、过期/重放 token、目录枚举、RPC/mutation 路径、PID reuse、ui_host crash、index cancel 和 cache delete 均有故障注入；网页不能访问未授权项目或使用户仓库发生变化。

### 1D.6 状态、验证矩阵与发布判定

- [x] Desktop 使用独立一级“代码图谱（实验性）”页面显示 package/integration/agent_session/ui_session/index；每项有 owner/source/observed_at/freshness/warnings/action receipt。
- [x] “配置已验证”来自文件复读；“Agent 会话已验证”来自 launcher+upstream identity 与 MCP initialize/tool method evidence；“UI ready”来自 d-token host/授权/端口；“索引 current”来自受控任务与 freshness。四者不能互相推导。
- [x] Rust/fixture 覆盖领域类型、事务、launcher、scope/cache/repo-write、进程身份、安全 host；Desktop 覆盖 unavailable/partial/changed/unmanaged、中英/浅深色/Reduce Motion。
- [ ] macOS/Windows 覆盖 ARM/x64 artifact、私有权限、path/Unicode、browser open、Agent parent exit、d-token host stop 和 exact rollback。
- [x] 用户授权后以仓库 `code/` 目录验证真实 OpenCode → launcher → pinned upstream：最新 receipt 记录 initialize=1、tools/list=1、allowed tools/call=1、denied=0；项目配置精确恢复为 absent，仓库无 `.codebase-memory/`/`artifact.json`/`.gitattributes`，验收输出不含 auth、源码、graph、prompt/response 或 raw MCP payload。

首期完成定义：CBM-0–6 全部通过并绑定同一候选 manifest；产品明确标注实验性、用户导入、固定版本校验、Agent-owned/d-token-owned 会话差异和 Git 轮询限制。独立 daemon、跨项目图谱、原生 upstream UI、ADR write、repo artifact export、App 代为下载/更新和默认 Hook/auto-watch 继续关闭并另立设计审查。

## 2. M4：Production Managed Installer（实施中）

目标：成品支持一键安装、图形化配置、可回滚。用户只需下载 d-token App，不用先装 Headroom/RTK。

M4 的实际落地先走 `OpenCode Real Route Track`：使用用户原本 OpenCode 配置，不隔离 HOME、不覆盖正常认证配置；临时写入前创建权限受限备份，只写最小 provider/model/baseURL 变更，再由 d-token 完成压缩、路由、Event Ledger 和 upstream 转发。当前 macOS 候选已完成真实请求与 UI 精确回退，配置 hash 已恢复到操作前值。历史设计见 `docs/history/M4_REAL_CAPABILITY_OPENCODE_PLAN.md`。

### M4.S1 安装事务引擎（InstallationTransaction）

落位：`code/crates/dtoken-config-patcher`（扩展）或新建 `dtoken-install-transaction`。

- [x] 实现 `InstallationTransaction` 状态机：scan → plan → show diff → backup → confirm → apply → verify → doctor → commit rollback point。
- [x] 失败语义：只停 d-token 自有新进程；kill 前校验进程身份；恢复 backup；清理 partial；输出精确失败与恢复动作。
- [x] 事务状态、动作结果和回滚证据进入桌面运行证据与 Event Ledger。
- DoD：fixture 下跑通 apply→fail→rollback 全链路；真实写路径只有在同一事务门、明确确认和可验证恢复证据下开启。当前 OpenCode/macOS 路径已满足，其他 Agent/平台仍关闭。
- 涉及：`dtoken-config-patcher`、`dtoken-event-ledger`、`dtoken-process`。

### M4.S2 Managed d-token runtime 安装/启用/升级/停止/卸载

- [x] runtime 打包策略：正式产物是本仓库 Rust workspace 的 `dtoken-proxy` external binary；`code/runtimes/headroom/` 仅保留 Headroom-derived provenance/迁移边界，不作为 Python 或外部服务运行目录。
- [ ] install/update/uninstall 全部走 InstallationTransaction，并保留 binary hash、version、target triple 与候选 commit provenance。
- [ ] integrated RTK 只能通过 d-token runtime 内部能力启用，不暴露 standalone RTK 为产品主路径。
- DoD：fake-binary 模式下完成 install/update/uninstall 全流程；真实 bundle 能证明 binary identity、版本与候选来源。
- 涉及：`dtoken-process`、`dtoken-config-patcher`、`dtoken-proxy`、Tauri sidecar 准备脚本、`support/fake-bin/`。

### M4.S3 Provider / Route / cc-switch 安装与配置

- [x] OpenCode 已验收的 ProviderProfile / RouteProfile patch 走 transaction；ChainBridge 与其他 Agent 矩阵仍留待后续。
- [x] route decision 进 Event Ledger（input/output protocol、target provider、route identity）。
- [x] 401/403/404/408/429/5xx 已稳定归类为 provider auth、route not found、rate/timeout 与 server error；原状态码进入响应/Ledger，且禁止静默 fallback 到其他 Provider。
- [x] OpenCode 真实 route 使用用户原本 OpenCode 配置；临时 provider/model/baseURL 变更已完成 backup、双 hash、verify、真实 request 与精确恢复。
- DoD：fixture 下覆盖 official / custom_openai / custom_anthropic / cc_switch / chain_bridge 五条路径，Event Ledger 可重建请求走向。
- 涉及：`dtoken-router`、`dtoken-config-patcher`、`dtoken-proxy`、`dtoken-event-ledger`。

### M4.S4 Agent wrap / Shell profile / 配置 patch

- [ ] agent wrap setup（Claude Code / Codex / Cursor / Cline / OpenCode 等矩阵，PRD §8.1）。
- [ ] shell profile / agent config patch：bash / zsh / powershell / cmd / wsl-bash。
- [x] Smart Learning managed instruction block 已在临时目录完成 diff、backup、并发保护与 exact rollback；真实用户项目默认仍需明确确认。
- DoD：每个 agent 在 fake-binary + 临时 HOME/PATH 下完成 patch 与回滚；CompatibilityMatrix 状态可写回。
- 涉及：`dtoken-config-patcher`、`dtoken-process`、前端 agent 矩阵页。

### M4.S5 Managed 真实路径 + Install Doctor

- [x] OpenCode 与 d-token active route 已在 InstallationTransaction 之上完成真实写入、backup、verify 和 rollback；其他 Agent/平台仍未开放为已验收路径。
- [ ] Install Doctor：runtime / proxy / 端口占用 / 进程身份 / integrated RTK / 外部 standalone RTK 冲突 / provider route / cc-switch route / config backup / Windows UTF-8/shim / macOS xattr/quarantine/keychain/launch agent。
- [x] rollback point 已落盘并可在 GUI 调用；坏备份 fail-closed，missing-file 与并发修改语义已有回归覆盖。
- [x] Doctor v3 领域合同已下沉 `dtoken-doctor`：动态 freshness/TTL 的 `DoctorSnapshotV3`、16 项模块注册表、repair selection/plan/review digest/receipt 与覆盖率测试已接入；v2 快照只兼容读取。
- [x] Doctor 正式页已改为“自动扫描并修复 / 仅扫描 + 专项修复工具”：明确展示扫描、规划、待确认、执行、验证、partial/failed 阶段，以及覆盖范围、问题数、可修复数和任务化模块列表。
- [x] Doctor Agent 弹窗已接入动态 registry、global/project 作用域、Instructions/Hook/Plugin/Recovery Skill/metadata MCP surface、逐 Agent 脱敏 Diff 与一次确认；每个 Agent 独立事务并顺序执行，单项失败继续其他 Agent。
- [x] Doctor v3 已接入服务与 Agent Integration executor；服务重启前校验 manifest schema、PID 出生标识、exe path/hash、端口归属、runtime identity/version/instance nonce，无既有管理证据 Agent 与 Optical Active 均 fail closed。
- [ ] Route reload、listener TTL 清理、Agent transport、Optical Shadow、autostart、cc-switch 只读同步与 recovery 复检等专项模块需要逐项完成专用 executor 和双平台故障注入；没有 executor 的模块保持锁定/返回来源工作台。
- [x] 变更与回退默认进入“备份与回滚”，顶级待批/Diff 已移除；变更记录只投影持久事务事件，未完成草稿返回来源工作流。
- [x] 回退预览区分 backup integrity 与 current restorability，并以 preview revision/current SHA-256 绑定确认；恢复前创建绑定“回退后真实状态”的反向安全备份（含原目标不存在的 `missing` 前置条件），恢复后返回文件/runtime 复检状态。
- [x] Changes 通过 `executor_kind` 分发来源 executor：多文件 Agent Integration manifest 聚合为一个事务回退点，顺序创建逐目标反向安全备份后整笔调用 Agent rollback；Optical 使用专用 rollback/restart/runtime verify，普通配置继续走通用 executor。
- [x] unknown/legacy/corrupt scope、损坏事务 manifest、并发 hash 变化和已回退事务在预览阶段 fail closed；Agent/Optical 底层单文件 backup 不再作为可独立恢复点展示。
- [ ] Route/安装等后续专项事务继续补齐事务 manifest 与专用 rollback executor；未具备来源 executor 的 scope 保持锁定。
- DoD：在隔离 VM/临时机上完成覆盖安装、回滚、再升级；Install Doctor 全绿。
- 涉及：`dtoken-doctor`、`dtoken-config-patcher`、桌面 Doctor 页。

### M4.S6 前端接通本地 API

- [x] Tauri Dashboard 主路径从 `dtoken-core-api` 读取 d-token-owned runtime；非 Tauri 环境 fail-closed，不向正式页面注入示例数据。
- [x] InstallationTransaction / route decision / Doctor 状态、source/freshness/action result 在 GUI 实时可见。
- [x] Optical shadow 默认关闭 active transform，仅提供 preview/metadata observability。
- DoD：Tauri 窗口不以 fixture/硬编码冒充本机事实，managed 链路的 source/freshness/action result 完整可见。
- 涉及：`code/apps/desktop/src/*`、`dtoken-core-api`。

### M4 验收（对齐 PRD §18 M4 与 §19）

- [x] macOS 候选可安装/启动 d-token-owned runtime；OpenCode 与 active route 写操作可追踪、可备份、可验证、可回滚。
- [x] 用户不需要独立安装 Headroom；Headroom 缺失不阻断 Doctor readiness。
- [x] 真实 OpenCode 请求没有静默 fallback，route/provider 与脱敏 Ledger 可审计。
- [x] 历史 macOS 候选的 product-readiness 聚合为 9/9；该证据不等于新的 Doctor 问题→修复→复检闭环已完成跨平台实机验收。Dashboard 重启后保留 89/20 请求证据。
- [ ] 新 Doctor 主动扫描/常规修复与新 Changes 事务回退仍需绑定同一候选，在 macOS/Windows 实窗完成外部端口占用、runtime 离线、并发 hash、损坏备份和跨重启回执验收。
- [ ] Windows UTF-8/path/shim 检查进入 Install Doctor 且完成 NSIS 实机 gate。

## 3. M5：Production Hardening + Release

目标：可以放心让外部用户下载、覆盖安装、回滚、升级、排障。

### M5.S1 打包与签名（构建配置已启用，发布证据未闭环）

- [x] Windows NSIS 与 macOS `.app`/DMG 的 Tauri bundle 配置。
- [x] Windows unsigned prerelease workflow：Windows runner、当前候选静默安装/启动/卸载、SHA-256 manifest、SBOM/license/security/provenance 与 artifact upload；明确 unsigned，不宣称 AuthentiCode。
- [x] 平台自动启动控制面：macOS LaunchAgent 与 Windows HKCU Run 均使用精确启动参数、原值快照、写入后校验和失败回滚；设置页读取 native 状态并使用统一确认弹窗。
- [x] 干净 checkout 在 workspace Clippy/test 前按 Linux/macOS/Windows host 构建并准备真实 `dtoken-proxy` externalBin，避免本地残留生成物掩盖 Tauri build-script 失败。
- [ ] 同一候选在 macOS 与 Windows 真实登录会话验证自动启动、重复启用、禁用、损坏配置和 App 升级路径。
- [x] macOS unsigned prerelease workflow：ARM64 `.app`/DMG、包审计、SHA-256 manifest、SBOM/license/security/provenance 与 artifact upload；明确 unsigned，不宣称 Developer ID/notarization/Gatekeeper。
- [x] 聚合 public prerelease workflow：tag `v*` 在同一 run/同一 commit 调用 macOS ARM64 与 Windows x64 reusable job；两端全部成功后才创建 GitHub public prerelease。
- [ ] Windows 安装后 UTF-8、路径、credential、`.cmd` shim 与回退实机验收。
- [x] macOS 本机 `.app`/DMG、安装后 runtime、真实窗口与 artifact hash 候选证据。
- [ ] Apple Developer ID、notarization、xattr/quarantine 分发验收。
- [ ] signed updater feed、通道与版本 provenance。
- DoD：干净机器上下载即装、覆盖安装、回滚、升级四场景通过。

### M5.S2 兼容性与 E2E

- [ ] Integrated RTK 兼容矩阵（PRD §16）。
- [ ] Provider / cc-switch E2E 兼容套件（official / custom_openai / custom_anthropic / cc_switch / chain_bridge）。
- [ ] runtime install/uninstall rollback 压力测试。
- DoD：兼容矩阵无 untested 之外的状态在受控平台收敛；E2E 套件 CI 可跑。

### M5.S3 隐私与合规审计

- [ ] privacy / export / redaction audit（PRD §15）。
- [ ] upstream license / NOTICE / base commit / 变更说明审计。
- [ ] 不上传 tracking DB / tee / prompt / 代码 / 命令输出；日志默认脱敏二次确认。
- DoD：审计报告归档 `docs/`；导出样本经脱敏校验。

### M5.S4 稳定性与文档

- [ ] crash recovery、benchmark/evals。
- [ ] user guide / developer guide / troubleshooting guide。
- DoD：文档覆盖安装、配置、回滚、排障四大路径，可被 CI 引用结构检查。

### M5 验收

- 对齐 PRD §19 全部 21 条验收中 M5 范围项。
- CI gate 含 structure check、cargo check、cargo test、no-touch smoke、desktop build；平台 release workflow 另执行签名、安装启动和产物 provenance 门禁。

## 4. M6：d-token-native Runtime Innovation

目标：在 §1C 完成 Headroom 全能力行为迁移后，继续创新内部实现；M6 不再承接任何“以后再迁移”的功能欠账。

- [ ] 替换脆弱 CLI/config/dashboard glue → Rust-native。
- [ ] dtoken-router 升级为 native route adapter（保留 Headroom-compatible API）。
- [ ] dtoken-event-ledger 升级 native observability（统一 denominator，落实 PRD §10 分层 savings）。
- [ ] dtoken-proxy / 协议适配层 native 化（OpenAI/Anthropic）。
- [ ] dtoken-project-policy 压缩策略引擎 native 化。
- [ ] 保留必要 Headroom-compatible 迁移层，可逐模块灰度替换并回退；能力不得缩水。
- DoD：每个模块替换前有对比基准 + 回退路径；用户仍只下载 d-token App。

## 4A. Experimental Track：Hybrid Context Optimization / Optical

定位：Optical 是混合上下文优化引擎中的一种有损 codec。后端 active 执行链已实现但默认关闭；只有精确签名 Capability、通过的绑定 eval、同协议 Anthropic 路由和全部安全/资源门同时满足时才可达，当前尚无仓库内置可信模型或默认 active。

当前完整迭代合同：`docs/experiments/HYBRID_CONTEXT_OPTIMIZATION_ENGINE_PLAN_V8.md`。V8 将本轮目标从 renderer 单点调优提升为统一 Hybrid Plan、内容路由、风险防火墙、cache/economics、provider state、post-validator、accounting、Ledger/Doctor 的完整后端闭环；不接前端、不默认启用 active。

2026-07-15 本轮执行顺序：

1. [x] Strategy isolation：拆分 `SafeTextCodec` 与 `OpticalImage` target；JSON 与决策敏感日志不得进入 renderer；SafeLogCodec 不继承通用长文本摘要。
2. [x] Risk/exactness：扩充 exact、secret、decision-sensitive 分类与回归 fixture。
3. [x] Unified Plan/cache：绑定 content/risk/action/cache/economics evidence，stable digest 切换 SHA-256。
4. [x] OpenAI Responses：独立 shadow inventory/state graph；active 继续不可达。
5. [x] Renderer/economics：动态最后页高度、per-page token 估算和 conservative/expected/optimistic 区间。
6. [x] Runtime/accounting：统一 plan 接入 direct Anthropic 热路径，补 Ledger/Doctor metadata-only 证据。
7. [x] 发布前加固与最终本地门完成：共享 SecretClassifier/recent-six/byte budget、stream 终态 Ledger、持久 Agent transaction、bridge 签名先验验证与 UI fail-closed 回归已闭环；Rust workspace `467 tests / 50 suites`、桌面 Vitest `35 files / 175 tests`、系统 Chrome Playwright `10/10`、support Python `7/7` 全部通过，严格 Clippy、workspace fmt、TypeScript/build、structure、macOS sidecar dry-run 与 diff check 通过。Playwright WebKit、Windows 实机、真实 Provider 与本轮 routed Agent flow 未执行；V3/V4 模型评测仍只产生 lab evidence，Capability registry 保持为空。

截至 2026-07-13 已完成：

- [x] 安全默认关闭的全局/路由策略与版本化 Capability Snapshot。
- [x] Capability 生命周期：schema v1、active/revoked/superseded、撤销 revision 屏障、重复 snapshot ID 阻断。
- [x] Request-local TextArena、真实文本风险分类、Capability-driven geometry、selected-only economics。
- [x] Anthropic 工具依赖图、并行 tool use/result、orphan/duplicate/recent-tail fail-closed。
- [x] OpenAI Chat `tool_calls` / `tool` 的独立 inventory 与状态机 lowering；尚未接入 active 决策。
- [x] Raw request bytes SHA-256 与 Canonical JSON SHA-256 v1 计划证据。
- [x] Preview、Status、Capabilities、Capability Lifecycle、Recent Events API。
- [x] metadata-only 事件：状态、模式、候选计数、图计数、经济摘要和 Capability 规则计数。
- [x] `support/pending-commits.json` schema v2：阶段、时间、文件、验证、共享文件暂存方式、是否提交、提交哈希。
- [x] Protected Context IR 与安全 semantic codec；协议结构、近期文本、exact/secret、schema、tool id/arguments 始终保留。
- [x] 确定性纯 Rust PNG renderer、自解码、分页以及 deadline/取消/并发/像素/PNG/编码/内存/body 预算。
- [x] Anthropic sealed patcher、artifact-derived post-transform validator 和 send-commit fallback 状态机。
- [x] 同协议 Anthropic JSON/SSE 转发、shadow/passthrough 原始 bytes 保持和跨协议 Optical active 阻断。
- [x] Ed25519 route-bound Capability/eval attestation、signed multimodal accounting、typed Ledger、metrics 与 Doctor/status evidence。
- [x] OpenAI Responses `message/function_call/function_call_output` shadow inventory、`OpenAiResponsesV1` 状态机与 recent-six/open/orphan/duplicate fail-closed；不生成 active target。
- [x] `anthropic_dense_v2` 动态末页、artifact 实际尺寸 token/像素核算、三点经济区间与 metadata-only action/cache/risk 计数。

下一实施顺序：

1. 取得安全的 OS credential reference 或明确 auth-free upstream，运行真实 OpenCode → d-token → Anthropic-compatible upstream 候选验收。
2. 用真实 Provider 结果生成 fidelity/cost/latency eval artifact，并由配置 authority 签署精确 provider/route/model/endpoint Capability。
3. 增加 malformed/parallel/adversarial/fault-injection corpus；OpenAI Provider-specific planner 继续保持 shadow-only。
4. 后续独立实现 history collapse/cache-zone 持久协议；当前已有 conservative/expected/optimistic 单请求区间，但不等价于生产缓存因果证据。
5. [x] 前端「视觉压缩（Beta）」一级页：Tauri 聚合观测 + shadow preview public summary；lazy/按需拉取；Windows/macOS 共享 UI；不开启默认 active，不渲染 PNG/base64。
6. [x] 视觉压缩页安装+高级配置 IA：一键启用安全 Shadow 默认值、高级 route 策略、状态与预览；diff 预览/确认/apply/restart/rollback；产品路径强制关闭 active。

发布禁令：真实模型 fidelity/cost/latency gate、精确签名 Capability 和同候选版真实 routed evidence 完成前，禁止默认开启 production active transform；post-commit 永不自动原文重试。

事实源：

- `docs/experiments/OPTICAL_HYBRID_CONTEXT_OPTIMIZATION_PLAN_V7.md`
- `docs/optical/implementation-status.md`
- `docs/optical/review-round-5.md`
- `support/pending-commits.json`

## 5. CI Gate（贯穿 M4–M6）

当前 `.github/workflows/ci.yml` 已含 structure、前端 test/typecheck/build、Rust fmt/clippy/test 和 no-touch fixture smoke；tag `v*` 的 GitHub public prerelease 现在改为聚合 unsigned workflow：只有 macOS ARM64 与 Windows x64 两个 reusable job 在同一 run/同一 commit 下同时完成安装/启动审计、SHA-256、SBOM/license/security/provenance，才创建 GitHub prerelease。它不宣称 Developer ID/notarization/Gatekeeper 或 Authenticode。2026-07-13 当前本地候选门禁为前端 125 项通过、Playwright E2E 20 项通过、`npm audit` 0 个漏洞；macOS 六页基线截图位于 `support/artifacts/macos-ui/final-candidate-2026-07-12T18-52-02-502Z/`，当前 autostart/窄窗口证据位于 `support/artifacts/macos-ui/final-candidate-2026-07-13/`。本机 ad-hoc 产物哈希由候选专属 manifest 绑定 commit，源码文档不复用旧哈希；生产级正式发布仍须在候选 commit 上重新签名构建。计划延续并加强：

1. `python support/scripts/check_structure.py` —— 目录边界。
2. `cargo fmt --all -- --check` / `cargo check --workspace --tests` / `cargo test --workspace`。
3. no-touch fixture smoke（`support/scripts/run_fixture_tests.*`）。
4. `npm run build`（`code/apps/desktop`）。
5. M4 起：InstallationTransaction 回滚单测、Install Doctor 静态检查。
6. M5：E2E 兼容套件、Windows installer 与 macOS 签名产物成为候选版硬门禁。

## 6. 风险与对策

| 风险 | 影响 | 对策 |
|------|------|------|
| 回退到静默 fallback | route 误走官方、安全/成本/合规风险 | route decision 强制进 Ledger；401 显式分类；CI 加 route 静默 fallback 检测单测 |
| Headroom 上游漂移 | runtime 吸收失锚、审计断裂 | 锁 upstream base commit + NOTICE/变更说明 + 定期 rebase 计划 |
| Windows 路径/编码 | 覆盖安装回滚失败、项目名连字符误反推 | UTF-8 强制 + session `cwd`/显式 project root 优先 + `.cmd` shim fallback + CI 加 Windows 兼容用例 |
| no-touch 边界被 Managed 写穿 | 开发期破坏本机环境 | InstallationTransaction 不在开发默认路径生效；fixture/fake-binary 为 gate |
| savings 双计 | Dashboard 误导、对外不可解释 | M6 native ledger 统一 denominator，禁止 rtk+runtime 直接相加（PRD §10） |
| 打包/签名复杂度 | 构建配置存在但被误判为可发布 | 同一候选 commit 绑定签名、notarization、artifact hash、安装后运行与实机验收 |

## 7. 文档维护约定

- 新里程碑交付后写交付报告 `docs/history/M{n}_*.md` 并更新本文件 §0 当前坐标。
- `docs/index.md` 同步增删条目。
- 旧计划只进 `docs/archive/`，不就地改写历史。
- PRD 边界变更先于 PLAN：先改 PRD，再在 PLAN 引用新契约，避免计划与产品真理源漂移。
