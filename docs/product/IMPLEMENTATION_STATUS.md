# d-token 当前实现状态

更新时间：2026-07-31
状态类型：代码事实快照，不替代 `docs/PRD.md` 与 `docs/PLAN.md`

## 1. 当前结论

d-token 当前已形成可运行的 Rust-owned 本地 runtime、Tauri 2 桌面控制面、React 18/TypeScript UI 与薄 TypeScript SDK。当前工作树已补齐九类 production codec release evidence（id 列表由 compressor registry 编译期投影）、CCR lifecycle/reconciliation 与 Windows DACL 实现、Rust/HTTP/MCP/TS surface parity、20 项 Agent 检测/transport capability matrix、Claude/Codex/Cline 真实临时配置精确恢复、cc-switch A→B→A 只读静态识别以及 macOS managed RTK lifecycle probe。它仍不是可发布候选：最新全仓门禁、当前候选真实 OpenCode、Windows MSVC runner、同 commit 双平台安装包以及 G6–G8 尚未全部通过。公开未签名 prerelease 只有在这些硬门全绿后才允许创建。

## 2. 代码所有权

| 范围 | 当前定位 |
|---|---|
| `code/apps/desktop/` | 唯一正式桌面 App；Tauri 2 + React 18 + TypeScript + Vite |
| `code/crates/` | 18 个 Rust 业务 crate，负责类型、配置、路由、压缩、学习、进程、Doctor、事件、策略、ContextStore、runtime pipeline、Agent bridge、Codebase Memory、API 与 proxy |
| `code/apps/desktop/src-tauri/` | 桌面 native command、凭据存储、配置草稿/应用、服务生命周期、安装事务、Doctor 与导出 |
| `code/runtimes/headroom/` | Headroom-derived 能力迁移与 provenance 边界，不是 Python sidecar 或外部服务依赖 |
| `code/UI design for project/` | 设计原型/历史 UI 输入，不是生产代码入口 |
| `code/apps/desktop/src-old/` | 旧前端实现，不得作为新功能落点 |
| `code/packages/sdk/` | 薄 TypeScript SDK；与 Rust/HTTP/MCP 共用 compression decision parity contract |

## 3. 当前 Rust workspace

`code/Cargo.toml` 当前注册 18 个业务 crate 与 1 个 Tauri app crate：

```text
dtoken-types
dtoken-config
dtoken-router
dtoken-compress
dtoken-context-store
dtoken-process
dtoken-doctor
dtoken-event-ledger
dtoken-preprocessor
dtoken-headroom-adapter
dtoken-project-policy
dtoken-runtime-pipeline
dtoken-config-patcher
dtoken-agent-bridge
dtoken-codebase-memory
dtoken-learning
dtoken-core-api
dtoken-proxy
d-token-desktop
```

当前生产主链路：

```text
AI coding agent
→ 127.0.0.1 d-token listener
→ dtoken-core-api / dtoken-proxy
→ compression + route selection
→ configured upstream
→ redacted runtime evidence / Event Ledger / Dashboard / Doctor / App
```

## 4. 已存在的本地 API

`dtoken-core-api` 当前提供以下主要能力：

- OpenAI Chat Completions、OpenAI Responses、Anthropic Messages 入口。
- `/health`、`/dashboard/payload`、`/stats`、`/stats-history`、`/metrics`。
- Provider、Route、Instance、active config、chain status 与近期 route decision。
- Doctor、recent logs、JSON/CSV export、Event Ledger。
- CCR、memory/learn、output shaping、项目策略与兼容性信息。
- Headroom-derived 状态面，以及 d-token-owned managed RTK command filter/lifecycle evidence。
- Optical shadow preview、status、capabilities、lifecycle 与 recent events。

这些端点的存在只证明实现面存在，不自动等于已达到稳定 Beta。真实 release claim 仍需同一候选 commit 的构建、运行、路由、持久化和平台证据。

### Headroom 全能力迁移事实

当前固定对照基线为 `headroomlabs-ai/headroom@main` / `01df2452529a86c689cf226fecd5918cc5d19676`（未发布，2026-08-02 获取）；前一发布基线 `v0.33.0` / `28aa53dc7ca51e687cc719c3fe160f3be50c6570`，历史 `1725cd1f8376828849c0e48fd9008afa25d24fff` 仅用于解释旧证据。当前事实如下：

- proxy、显式 route、Ledger、stats/history/metrics、Dashboard/export 已有真实 d-token-owned 基础，但 Provider/Agent 矩阵未完整验收。v0.33 delta 已开始吸收：九类 production codec 现由 Rust-owned declarative compressor registry 统一登记和门控，release evidence 的 codec id 列表由 registry 编译期投影（`production_registry_codec_ids()`），计数无法与登记表漂移；`POST /v1/compress` 使用显式 route/provider/model/protocol 生成 validated candidate body，不发送上游、不持久化请求，并与 MCP 和 TS SDK 共用合同。d-token 不加载 Python entry point 或任意第三方进程内插件，第三方中转只能通过显式 RouteProfile 接入。
- resolved production path 已接入 `FrozenPrefix → TransformableWindow → ProtectedTail`、route/provider/model-bound cache key、ContentRouter、manifest 与 post-validation。JSON、log、search、build、diff、text、HTML、config、code-compressor 九类 codec 在本地 release probe 中均为 active；每类至少 128 个 property/semantic/rollback cases，negative、budget/cancellation 门零失败。`config-codec` 已进入生产 evidence（registry 第 8 项），且混合/多类型内容已接入 IR-safe section router：`route_mixed_content` 逐 section 走同一 codec/secret/marker/负收益门并字节精确还原（统一 CRLF 保留、混合行尾 fail-closed），`compress_resolved` 与 `/v1/compress` 公共表面会把 closed historical tool result 中的 config 与 mixed 内容按 section 路由；manifest 以 `Mixed` codec 声明并用同一 section router 重推导校验。tool-schema PR-E1/PR-E2（工具表按名排序 + schema key 规范化）已接入 `compress_resolved` 与公共 surface：任一工具携带客户 `cache_control` 时跳过 E1（位置敏感），PR-E3 自动放置 `cache_control` 仍由 proxy 层在 fresh/exact provider cache evidence 门控下执行，engine 不擅自改变客户缓存意图。code-compressor corpus 覆盖 Rust/TS/JS/Python/Go/Java/C/C++。未知、secret、风险和负收益输入继续精确 passthrough。该 evidence 尚未绑定最终 clean candidate 或真实 Provider 质量/延迟，因此 G2 不能据此单独宣布完成。普通输入图片文件压缩明确不在产品范围。
- CCR 已实现 AES-256-GCM、TTL/quota、非破坏性 inspect、精确 retrieve/delete、`write_intent → pending_retained → terminal` 与 `delete_intent → deleted` journal、重启保守 reconciliation、JSONL 到 SQLite projection rebuild、幂等 tombstone 与 lifecycle lock。Unix 目录/DB/WAL/SHM/CCR 文件分别收紧为 `0700`/`0600`。Windows 实现 current user + SYSTEM、protected DACL、禁继承、`SetNamedSecurityInfoW` 与读回验证，并接入 DB/WAL/SHM 和 CCR root/temp/final；Windows GNU cross-check 通过，仍须 MSVC runner 实测。跨 JSONL/密文/Event/SQLite 原子 transaction 和自动 tool-call retrieve 仍是 G3 缺口。
- memory 已接入 typed `ContextStore` 写入、重启持久化、项目/Agent/session 精确 scope isolation、share grant/revoke 与敏感原始字段拒绝；v4 的 project-wide grant 迁移后保留审计但强制撤销。managed MCP capability 可注册 `headroom_memory_list/export/delete/write`，并将不透明授权、project/agent/session scope 绑定到 launcher，工具参数不能自报 scope；公开 HTTP summary 因缺少调用方授权 scope 而 fail-closed。route-aware output shaping、跨 Agent capability transaction 与本机授权审计仍未完成。
- `/headroom/mcp` 已提供有界 stdio JSON-RPC transport：`initialize`、`ping`、`tools/list`、`headroom_compress`、`headroom_stats`、受控 `headroom_retrieve`、返回 deletion receipt 的 `headroom_delete` 与 SharedContext 工具，取消账本和 payload 限制均有测试。Rust library、HTTP、MCP 与 TS SDK 对同一 fixture 的 decision/reason/artifact parity 定向回归已通过；完整协议覆盖、caller capability 分发与最终候选复证仍缺。
- MCP `headroom_compress` 现要求受限 `route_ref`，并从 d-token active config 重新解析 route/provider/model/protocol；缺失、未知、endpoint/model/policy 不匹配均 fail-closed 且不回显输入或凭据。OpenAI Chat → OpenAI Chat 与 direct Anthropic Messages → Anthropic Messages 已接 resolved compression；跨协议 bridge、Responses 和 streamable HTTP MCP 仍未完成，故 MCP 仍不是全协议完成态。
- `dtoken-proxy --mcp-stdio` 已成为 d-token-owned MCP stdio launcher，可复用同一持久化 runtime/config 而不绑定 HTTP listener。Agent MCP transaction 仍默认使用 metadata-only `dtoken-agent-bridge`；将 raw CCR retrieve capability 分发给独立 Agent MCP entry 前，不能把该 capability 写进常规 Agent 配置或日志。
- Smart Learning 已新增 `dtoken-learning` 有界安全项目索引、真实项目分析、Agent 会话 metadata 发现和 managed block transaction；草稿生成具备立即反馈、自动定位与顶部通知，保存必须先显示 diff 并确认，重启后可从持久化分析继续操作。失败模式挖掘、跨 Agent memory 仍未完成。
- Overview 已新增 Codex/Claude 5h 与 7d 额度摘要合同。Codex 优先消费当前 ChatGPT OAuth 请求按账号节流获取的 `/backend-api/wham/usage` allowlist 数字字段，并兼容旧式 `x-codex-*` 响应头；`ChatGPT-Account-Id` 与 credential 只用于生成不可逆的 account/context 引用。Codex cache 绑定完整 credential+account context，Claude cache 绑定 OAuth credential hash；两者均有 provider 级 single-flight、账户级 attempt floor、15 分钟硬 TTL 与 8 项上限，active context 消失或切换时禁止跨账户回落。Claude 仅在检测到真实 `sk-ant-oat*` OAuth 上下文时读取官方 usage endpoint。两条链均为只读后台刷新，不保存原始账号 ID、Authorization、OAuth credential、完整 headers 或原始 usage response；单窗口证据标为 partial，无证据时保持 unavailable。
- Analytics 已扩展 Model、Protocol、Project、relay chain、失败分类、缓存/浪费/流水线 availability 与请求明细；新增 `/analytics/requests` cursor API、Tauri 只读桥接和前端分页。Agent/Model/Protocol/Route/Provider/Chain/Error 与 30 日 daily 聚合由 Rust read model 提供，不再由最近 20 条摘要重建；分页独立显示 source/freshness/revision，支持 loading/error/invalid-cursor 自动恢复。当前 runtime 最多恢复/查询最近 10,000 条；Project、真实 cache alignment 与完整 route/upstream/end-to-end timing 尚无生产字段时保持 unavailable。`RequestLog.latency_ms` 只作为 compression-only 证据，route/provider/cc-switch/end-to-end 与 route overhead 不再用比例或 0 值推算。以上不能称为全量 lifetime analytics。
- 性能工作树已新增独立增量 Dashboard summary read model、Learning 锁外评分与原子取消、Native summary/detail single-flight、summary slice revision，以及 WebView/Native/core 的有界 metadata-only timing。release-profile core summary 在 0/1k/10k/100k fixture 下 p95 为 34/151/144/153 µs；最终重建的同一 macOS release `.app` 已验证全部正式页首次加载、深浅主题、服务启动和 Overview 从 stale 自动恢复为 live，真实运行中的 summary HTTP 50 次顺序请求 p95 为 7.643 ms，core summary build 56 样本 p95 为 5,936 µs。遍历全部懒页面后的 30 秒可见窗口样本为 App CPU 0.23% / RSS 113.37 MiB、runtime CPU 0.00% / RSS 11.87 MiB。以上仍是 dirty working-tree、ad-hoc signed macOS 证据，不能替代 clean candidate、冷启动/完整进程组/soak、Developer ID/notarization 或 Windows 证据。
- 前端 production build 已加入 manifest 递归 bundle gate。2026-07-15 最新干净 Tauri 候选的初始静态 JS 为 160,769 B（157.00 KiB），低于 160 KiB 门限；`charts-vendor` 仍只由 Analytics 动态加载。该门只证明产物体积合同，不证明 WebKit parse/evaluate 或真实冷启动达标。
- Native loopback GET 已记录 metadata-only connect/configure/write/read/validate/parse/total 阶段及 health cache hit/miss，并复用响应分配剥离 header。当前仍是 blocking `TcpStream + Connection: close + read_to_string`，没有 response cap 或共享 typed client，因此只算测量与小幅复制优化，不算 HTTP 架构收敛。
- terminal evidence 仍保持四个 durable JSONL sync 边界，未为速度删除 durability；当前已能记录各阶段 duration/sync count、ContextStore lock/projection 与 total 分位数。canonical commit、fault injection、双写/重放和持久化收敛仍未完成。
- Rust 检测注册表覆盖 20 个 Agent；前端不再维护第二份 Agent catalog。Claude Code、Codex、Cline CLI、Goose、Kilo Code、OpenCode、OpenClaw、Gemini CLI、Roo Code、Continue、Aider 有版本化 Configure/Clear adapter（2026-08-03 起）；CommandCode、Claude Desktop、Cursor、Hermes、OpenHands、Copilot CLI、Windsurf、Antigravity、Mistral Vibe 有显式 detection-only/fail-closed decision。Cursor 保持 fail-closed 的原因是官方确认所有 BYO-key 请求都经 Cursor 服务器转发，没有可验证的本地 upstream override；Gemini CLI 适配器写 `~/.gemini/settings.json` 的 `security.auth.selectedType=gemini-api-key` 与 `~/.gemini/.env` 的 `GOOGLE_GEMINI_BASE_URL/GEMINI_MODEL/GEMINI_API_KEY`（值只写 `dtoken-loopback` 标记，绝不写凭据）；Roo Code 写 `~/.roo/settings.json` 的 `openAiBaseUrl/openAiModelId/openAiApiKey/currentApiConfigName/listApiConfigMeta`；Continue 只写 `~/.continue/config.json`（存在权威 `config.yaml` 时 fail-closed）；Aider 写 `~/.aider.conf.yml` 的 `model/openai-api-base/openai-api-key`。未知 JSON 不再递归猜 upstream。支持 adapter 的 parser 只读取 provider/model/protocol/脱敏 endpoint 与 credential-present；Goose custom provider sidecar 参与 watcher/fingerprint。新 adapter 均带独立 ownership manifest、双 hash、revision-bound preview/apply、verify 与 exact clear；`cc_switch_fixture.rs` 与 in-crate 矩阵测试同步锁定该 11 项 Configure/Clear 矩阵。
- 2026-07-31 已在本机真实 Claude Code、Codex、Cline 配置上完成 preview → apply → reread verify → clear → exact restore，不启动 Agent、不发模型请求、不返回 credential value；最终 SHA-256 分别为 `341e468283ce253d7a31699dc6b514bb556a1da8b8ca71e9db6786f404460ed3`、`8af5fd43ea971d118cc533dc0c09eed23d34f7036c012c6b4777769bc9064c4b`、`9adf48a01f16427ed4c40fca78fcf8389fd06e387e263383459e4265c4d67a05`，权限均为 `0600`。2026-08-03 工作树新增 Gemini CLI/Roo Code/Continue/Aider 的 fixture 级 lifecycle 事务（configure → verify → clear → exact restore）并更新发布矩阵；真实本机候选 probe 与 Windows runner 证据仍未执行，OpenCode 仍须在最终候选上重跑真实 routed compression。
- cc-switch 保持只读：`CcSwitchSnapshot` 仅提供 schema/source 状态、app type、active upstream、脱敏 endpoint、protocol/model、credential-present、fingerprint/revision。它监听已定位的 DB/WAL/SHM 与父目录，使用 750ms debounce、single-flight 处理和 polling fallback；监听不可用时显式报告 fallback。pending 变更若回滚至稳定快照会被清除，之后再次出现必须完成新的完整 debounce。用户在一次 hash-bound 确认中授权后，高置信单一 upstream 的变化可自动生成并执行 d-token 自有 route sync/reload receipt；手动首轮 sync 只有 `runtime_reloaded` 或安全的 `runtime_not_running` receipt 才会持久化 auto-follow 授权，否则保留已应用 route 但不授予后续自动跟随。多 active、未知 schema、缺 model/新凭据一律 `needs-selection`/blocked，不写 cc-switch。
- d-token-owned managed RTK 已在 macOS candidate probe 通过 install/verify、ANSI/空行/重复行过滤、有界输出、成功与失败 exit propagation、usage metadata、exact uninstall 和 private root cleanup；receipt 明确 `command_argv_persisted=false`、`raw_output_returned=false`、`credential_value_read=false`。最新证据为 `support/artifacts/managed-rtk-candidate/candidate-20260731T223754Z-7536.json`。Windows/WSL、hook integrity 与最终候选 workflow 证据仍未完成，因此 integrated RTK 保持 partial。
- Doctor v3 已将 `agent_cc_switch_sync` 作为实际 domain module：同步、可修复 drift、需要选择和缺失证据分别由同一只读 discovery 产生。Doctor 尚未直接执行同步写入；用户仍从 Install 的 diff/确认流应用 d-token-owned route。
- 当前 gate 结论：G2 本地 codec release probe、G3 lifecycle/ACL 实现、G4 surface parity、G5 部分 Agent/cc-switch/macOS RTK 证据已推进；它们尚未绑定最终 clean candidate。G6–G8、当前候选真实 OpenCode、Windows runner 和双平台产物仍阻断发布。Optical code/context → PNG 保持默认关闭与独立 fidelity/cost/latency gate。

逐能力缺口、顺序与发布 Gate 见 `docs/PLAN.md` §1C。任何 metadata-only endpoint、静态布尔值、fixture smoke 或外部 8787 对照都不算迁移完成。

## 5. 当前 Tauri 能力

生产版 invoke 当前包含：

- App 设置读写。
- Provider credential 的操作系统凭据库写入/删除，配置中只保存引用。
- Route/provider 草稿保存、应用和 active config 状态读取。
- d-token active config、OpenCode 和 Smart Learning 单文件 ConfigPatcher rollback point；回退预览区分完整性与当前可恢复性，绑定当前 SHA-256/`missing` 状态，恢复前创建可再次使用的反向安全备份并返回文件/runtime 复检状态。Agent Integration 多文件 manifest 已聚合为事务级回退点，Optical/Agent 根据 `executor_kind` 调用来源 executor。
- 工具检测、安装计划预览、d-token 服务状态、启动和停止。
- 安装事务、Doctor v3 统一快照/模块注册表/动态 freshness、自动扫描并修复、专项模块、Agent 多选逐项事务、预览/确认/自动复检、产品 readiness 聚合、Dashboard 读取和脱敏导出。
- Agent route 配置验证与项目压缩策略推荐。
- OpenCode 配置事务：preview、脱敏 diff、双 SHA-256、backup、明确确认、apply、verify 和 exact rollback。
- Smart Learning：多 Agent 会话 metadata 项目发现、有界安全结构索引、项目分析、敏感路径排除、持久化结果恢复，以及带即时反馈、diff/confirm、managed block transaction 与 exact rollback 的文档动作。
- Codebase Memory（实验性）：固定上游 headless artifact、`dtoken-codebase-memory-launcher`、项目级私有 cache、MCP transaction、Claude Hook、d-token-owned index/UI host、五维状态，以及只读 graph open/start/stop/delete。上游原生 UI、运行时下载、repo persistence 与 Agent-owned 进程停止均保持禁止。
- Codebase Memory 真实验收：OpenCode 1.14.51 使用官方免费模型经项目事务连接 managed canonical MCP entry；metadata-only receipt 为 initialize=1、tools/list=1、allowed tools/call=3、denied=0，随后项目配置精确恢复，批准仓库未生成 `.codebase-memory/` 或 `artifact.json`。
- 统一 `OperationDialog` 与本地持久化 `NotificationCenter`；success/info/warning/error/action-required 均可即时占用顶栏 banner。普通成功/信息 5 秒后收起，error/action-required 8 秒后收起；持久 receipt 收起后仍保留在通知中心。安装与连接、Doctor、变更回退、视觉压缩、设置、Smart Learning、日志/安全等正式动作均使用页内中间态与顶部结果通知，失败详情经过统一脱敏并保持简短。

OpenCode workload、模型测试、Headroom capability smoke 等开发命令只在 debug build 注册；release build 不注册这些测试入口。

当前限制：

1. `install_tool` 对缺失的 RTK/Headroom 不执行真实下载，只做检测并安全失败。
2. OpenCode 已完成真实配置事务和恢复，但该授权与证据不扩展到 Claude Code、Codex、Cursor、Cline 或 cc-switch。
3. 平台自动启动控制面已接通 macOS LaunchAgent 与 Windows HKCU Run，具备快照、原子写入/注册表写入、写后校验与失败回滚；真实登录会话和覆盖升级后的自动启动证据仍待补齐。
4. 凭据库代码路径存在，但真实认证 Provider 的 macOS Keychain/Windows Credential Manager 候选版证据仍需补齐。
5. 通用回退页已直接执行普通 ConfigPatcher、Agent Integration 与 Optical 三类 executor；Agent 多文件事务聚合、反向安全备份和专项复检已有回归覆盖。Route/安装等尚无专项 executor 的 scope 仍必须锁定，双平台真实窗口与 runtime 离线回执证据待补。
6. Doctor v3 当前真正可写的专项 executor 是身份验证后的 d-token 服务生命周期与已有管理证据的 Claude Code/Codex/OpenCode Agent Integration；Route reload、listener TTL 清理、Agent transport、Optical Shadow、autostart 等模块已注册并显示能力状态，但未逐项完成 executor/双平台候选验收时保持锁定或返回来源工作台。

## 6. 当前桌面信息架构

正式一级导航与 PRD 一致：

1. 总体概览。
2. 安装与连接工作台。
3. 代码图谱（实验性）。
4. 分析。
5. 视觉压缩（Beta）。
6. Doctor。
7. 变更与回退。
8. 设置。

「视觉压缩（Beta）」页对齐安装工作台信息架构：一键启用（安全 Shadow 默认）+ 高级配置（全局/route 策略）+ 状态与预览。通过 Tauri `get_optical_observability` / `preview_optical_plan` 读取 d-token-owned Optical metadata-only API，并通过 `get_optical_config_editor` / `preview_optical_config_change` / `apply_optical_config_change` 以 ConfigPatcher 事务编辑 active config 的 optical 段（强制 `allow_active=false`，应用后重启 managed service）。页面 lazy 加载、仅打开时拉取，不进全局 dashboard 热路径。Windows/macOS 共享同一 React 页；估算节省不得标为账单。代码图谱作为独立实验页展示 package/integration/agent session/UI session/index 五维状态，使用受限 launcher、项目级私有 cache、配置事务和 d-token-owned 只读 loopback graph host；上游原生 UI、跨项目查询、运行时下载和 Agent-owned 进程停止均被禁止。Advanced、Logs/Security、Compatibility、Smart Learning 等仍作为安装工作台子页面或内部组件，不应再扩张为工程化顶级导航。Smart Learning 已接入“安装与连接”二级页，并使用真实 native scan/preview/apply/rollback action result。

同一页的一键区域已增加独立 Agent 接入工作台：展示 service PID/port、transport route 数、bridge 可用性、Claude Code/Codex/OpenCode 检测结果、全局/项目作用域和规则/Hook/Plugin/Skill/MCP 开关；应用前逐文件展示脱敏 diff，应用后提供校验、Codex Hook trust/new-session 提示与整笔回退。Agent 接入不会修改 Provider 凭据，也不会自动打开或关闭视觉 Shadow/Active。

正式桌面页面只接受 Tauri command 与 d-token-owned local service 的 source/freshness/action-result 证据；缺失数据保持 unavailable/partial/unknown。非 Tauri 环境仅用于开发预览并 fail-closed，不向正式页面注入示例数字。

当前 UI 基础层为 AiMaMi 信息层级取向的纯色 shell/surface：侧栏和顶部工作区连成一个整体，正式页面不使用玻璃卡片堆叠或动态背景；概览默认展示全年 heatmap，设置使用 grouped rows。侧栏一级图标、主题与折叠使用各自独立的 enter/leave 零件配方与点击容器 scale（Reduce Motion 关闭 transform）；首次无偏好时侧栏默认折叠。Windows/macOS 共用同一套 React UI，当前只完成 macOS 真实视觉验收。

## 7. 当前 macOS 真实候选证据

2026-07-13 已在精确最新 `.app` 上完成以下验证：

- 真实链路：`OpenCode -> d-token 127.0.0.1:18001 -> OpenCode Zen`。
- 关键 request `bc88fde0-3ce9-45ad-8e7f-b733096e9f0f`：压缩前 365,650、压缩后 331,945，物理减少 33,705；处理步骤为 `repeat_line_fold` 与 `semantic_closed_tool_result`。
- `tokens_saved/claimable=0` 是有意的 fail-closed 语义：没有上游归因证据时不把物理减少宣称为可计入节省。
- 重启后 runtime 仍保留 89 条 routed request、20 条可见 request record；物理减少汇总 279,270，约 4.0%，claimable 为 0。
- OpenCode 配置已恢复到 SHA-256 `47772c85307f02e1d910dc3cc93b85165a9fc37db5911b7d4201be766f85baab`；d-token active route 已恢复到 `37149d6c69561a4562a67d7d53bf03337e37e98104424c694aaa4a4038c45843`，当前 route 为 `Local verification`。
- 历史 Doctor/product-readiness 在真实窗口为 9/9；Headroom 缺失是正常可选状态，OpenCode 和 RTK 检测通过。该证据早于当前主动扫描/修复/复检工作台，不能替代新闭环的跨平台实机验收。
- Changes 旧页面曾显示 0 项待处理；当前工作树已移除顶级待批/Diff，默认展示备份与回滚，并从 desktop transaction ledger 投影持久变更记录。
- Smart Learning 在临时目录完成 managed block apply 与 exact rollback，没有改写用户项目。
- macOS 六页基线截图位于 `support/artifacts/macos-ui/final-candidate-2026-07-12T18-52-02-502Z/`；当前工作树新增 Overview、Settings autostart 与 920×640 实机证据位于 `support/artifacts/macos-ui/final-candidate-2026-07-13/`。本机 ad-hoc 哈希由候选专属 manifest 记录，不写死在源码状态文档中，也不等同于正式签名发布产物。

## 8. Optical / Hybrid Context Optimization

当前后端已实现默认关闭、证据门控的完整 Hybrid 执行链：

- 版本化 Capability Snapshot 与 revoked/superseded 生命周期。
- Anthropic/OpenAI Chat/OpenAI Responses 工具调用依赖图与 fail-closed 规则；Responses 只生成 shadow evidence。
- request-local TextArena、exact/decision/secret 风险分类、content/action/cache evidence、动态 geometry 与三点 economics 计划。
- raw-byte/canonical JSON fingerprint 计划证据。
- preview/status/capabilities/lifecycle/events API。
- route-resolved Protected Context IR 与仅批准 closed historical low-risk tool result 的 semantic/Optical codec。
- 确定性纯 Rust PNG renderer、动态末页、自解码、deadline/取消/并发/内存/像素/编码预算；active accounting 使用实际 artifact 页尺寸。
- Anthropic typed patcher、artifact-derived post-transform validator 和 pre-send 原文 fallback。
- Anthropic typed patcher 在每组 PNG 周围注入可验证的 `RenderedContextPointer`、可选 secret-first `ExactFactSheet` 与 `HistoryBoundaryGuard`；当前请求、近期六个闭合工具对及 open/orphan/malformed 状态仍保持原生文本。
- 同协议 Anthropic JSON/SSE 转发；shadow/passthrough 保留 ingress 原始 bytes，跨协议不授予 Optical active。
- Ed25519 route-bound Capability/eval attestation、signed multimodal accounting、typed Ledger action/cache/risk 聚合与 Doctor/status schema v2 evidence。

active 代码路径存在但生产默认仍关闭：必须同时满足全局/路由显式授权、同协议 Anthropic、精确签名 Capability、通过且绑定 renderer/font/corpus 的 eval、经济门、资源门、patch/post-validation 与 durable pre-send Ledger。当前没有真实 Provider fidelity/cost/latency 候选证据、OpenAI active patcher、history collapse 或 post-commit 自动 rollback；任何文档和 UI 都不得把本地估算或第三方数据宣称为已验证节省。

桌面 Beta UI 已接入 metadata-only 观测与 shadow preview；macOS/Windows 代码路径相同。Windows 实窗证据仍待补齐时不得把 macOS 单端截图写成双端完成。

## 9. 仍然阻断发布的事项

1. Headroom §8 仍未达到“完整迁移”发布声明：九类 codec 已 active，但最终候选真实 Provider/Agent 质量证据、G3 Windows runner/剩余 crash/tool-call、全协议 MCP/capability 分发、Agent/RTK 双平台矩阵与 G6–G8 尚未全部闭环。
2. GitHub public prerelease workflow 已改为 unsigned-only 聚合发布：同一 tag/同一 commit 的 macOS ARM64 与 Windows x64 reusable job 全部通过后，才创建 GitHub prerelease；它明确不宣称 Developer ID/notarization/Gatekeeper 或 Authenticode。
3. 生产级签名发布仍未完成：macOS 仍缺真实 Apple Developer ID/notarization/Gatekeeper 证据；Windows 仍缺具备签名凭据的 Authenticode 结果，以及 UTF-8/路径、Credential Manager、回退和视觉一致性证据。
   Linux CI、macOS release 与 Windows release 均会在 Rust workspace 门禁前从当前源码准备 host 对应的真实 `dtoken-proxy` externalBin，干净 checkout 不再依赖仓库内或开发机残留二进制。
4. macOS LaunchAgent 与 Windows HKCU Run 已接通，但尚未绑定同一候选完成两端真实登录、禁用和覆盖升级验证，不能宣称平台 autostart 已通过发布验收。
5. Claude Code/Codex/Cline 已完成真实临时配置事务并精确恢复，但按授权未发送模型请求；Gemini CLI/Roo Code/Continue/Aider 已完成 fixture 级事务与矩阵锁定，但真实本机候选 probe 尚未执行；OpenCode 仍待最终候选真实 routed compression。Cursor 因官方路由架构（所有请求经 Cursor 服务器）保持 detection-only，其余未验证 schema Agent 必须继续显示 unavailable/detection-only，不能计入自动接入完成率。
6. 真实认证 Provider 的 Keychain/Credential Manager 引用、短生命周期出站读取和不泄露凭据的候选版实证仍缺失。
7. `code/UI design for project/` 与 `src-old/` 等旧实现需要在后续清理任务中决定保留、迁移或删除；它们不是生产入口。

## 10. 当前验证入口

```bash
python3 support/scripts/check_structure.py

cd code
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace

cd apps/desktop
npm run test:run
npx tsc --noEmit
npm run build
npm run test:e2e
```

2026-07-13 历史候选门禁结果：

- `python3 support/scripts/check_structure.py`：通过。
- `npm run test:run`：125 项前端测试通过。
- `npx tsc --noEmit` 与 `npm run build`：通过。
- `npm run test:e2e`：20 项 Playwright E2E 通过。
- Rust route lifecycle、配置 patch/rollback 与 workspace gates 通过。
- `npm audit`：0 个漏洞。
- `npm run tauri:build`：macOS `.app` 与 DMG 生成成功；bundle `codesign --verify --deep --strict` 通过 ad-hoc 签名校验。该结果不等于 Developer ID/notarization。

2026-07-15 当前工作树最终本地门：Rust workspace `467 tests / 50 suites`、桌面 Vitest `35 files / 175 tests`、Chrome Playwright `10/10`、support Python `7/7` 全部通过；TypeScript、Vite production build、workspace fmt、严格 Clippy、structure、macOS sidecar dry-run 与 diff check 通过。WebKit 因缺少本机二进制未执行；Windows 仅有编译/脚本/workflow 合同证据。真实 Claude/Codex 用户配置未改写，本轮未执行新的真实 OpenCode routed flow，Provider fidelity/cost/latency 与签名 Capability 仍缺失。

2026-07-18 Codebase Memory 本地交付证据：Rust workspace `555 passed / 2 ignored`、桌面 Vitest `234/234`、TypeScript、Vite 与首屏预算、Chromium/WebKit Playwright `20/20`、structure、fmt、严格 Clippy 和 diff check 通过。macOS ARM/x64 与 Windows ARM64/x64 四个官方 archive 的 SHA-256、提取 binary SHA-256 与 Cosign blob bundle（固定 workflow identity/OIDC issuer）均验证通过；Windows GNU target 的领域/配置 crate `cargo check` 通过并包含私有 backup ACL 路径。Tauri debug `.app`/DMG 完成实构建、ad-hoc deep signature 校验，bundle 内 launcher 对签名后 upstream 完成 initialize、tools/list 与真实 tool call。真实 OpenCode 1.14.51 使用官方免费模型完成 3 次允许 tool call、0 次拒绝，事务后项目配置与仓库零写入边界复检通过。Windows workflow 已新增安装后 launcher MCP、索引、只读图页、安全端点、浏览器分发与仓库零写入 smoke gate，但尚无远程 runner 通过报告，因此不写成 Windows 实机通过。

2026-07-18 晚间按产品反馈调整分发与交互：正式 Tauri `externalBin`、macOS/Windows sidecar 准备、签名、安装审计与产物 manifest 已移除大型 upstream，App 仅保留受限 launcher 与法律资源；页面增加固定官方 `v0.9.0` 下载入口、默认/自定义导入目录与每秒 metadata 变化自动识别，执行前仍以 pinned platform/arch/SHA-256 fail closed。页面自动扫描 OpenCode/Codex/Claude，默认多选全部已安装受支持目标，批量预览/逐目标应用与复读验证，失败时逆序回退；2026-08-03 起 Cursor、Windsurf 等未被支持 schema 的 Agent 保持不可自动接入，而 Gemini CLI/Roo Code/Continue/Aider 已具备 Configure/Clear 能力。Shell 横幅只显示本次运行最新事件，历史保留通知中心；所有页面切换均不再主动聚焦 `<h1>`。实窗复核还发现图谱 Tauri command 的 Rust 参数为 `request: Payload`，而前端曾错误发送双层 `request`，导致批量配置按钮报 `missing field request`；现已统一为单层 payload，并用 Rust contract test 锁定 preview/apply/verify/rollback/start/index/delete/directory 命令。状态构建、配置预览/应用、UI host 启动与索引启动已移入 `spawn_blocking`，避免这些文件校验和进程启动工作占用窗口线程。

同日晚间最终本地门：Rust workspace `558 passed / 2 ignored`、桌面 Vitest `40 files / 239 tests`、Chromium/WebKit Playwright `20/20`、support Python `11/11` 全部通过；当时候选的 TypeScript、Vite production build、workspace fmt、严格 Clippy、structure、diff check 与 `npm audit`（0 vulnerabilities）通过。最终重打包时工作树还并发加入了其他 UI 改动，首屏递归 JS 为 `136,747 B / 180,224 B`。重新生成的 macOS arm64 release 候选 `.app` 为 32 MB、DMG 为 14 MB，bundle 资源清单只有 `dtoken-codebase-memory-launcher` 与 Codebase Memory LICENSE/NOTICE，不含 261 MB upstream；`codesign --verify --deep --strict` 通过 ad-hoc 签名校验。用户导入 pinned upstream 后，真实 OpenCode 1.14.51 使用官方免费模型完成 `codebase-memory-mcp_list_projects` tool event（completed）；事务随后精确回退，`code/opencode.json` 不存在，批准仓库没有新增 `.codebase-memory/`、`artifact.json` 或 `.gitattributes`。这些是 macOS 本机、dirty working-tree、ad-hoc signed 证据；Windows workflow 虽有静态和合同门禁，仍无真实 Windows runner 报告。上述门禁完成后，23:24 并发新增的未跟踪 `src/app/lib/copyPolicy.test.ts` 因缺少 Node 类型造成当前全量 `tsc --noEmit` 的 6 个错误；该文件不属于本轮代码图谱改动，未在本轮擅自修正，因此当前 dirty working-tree 不能标为全门通过。

Hybrid 发布前加固已落地：统一 SecretClassifier v4、三协议 recent-six、共享 in-flight byte budget、stream 终态 Ledger、原子持久 Agent transaction 与跨进程锁、共享 secret diff redaction、MCP 有界读取、UI 事务证据丢失 fail-closed、签名后 bridge identity workflow，以及任何 bridge sidecar 执行前的平台签名验证。Capability registry 为空；production Active 代码存在但当前不可达。

2026-07-19 桌面发布文案治理：`src/app/lib/i18n.ts` 已作为正式双语词典，Codebase Memory、Optical、Doctor 的稳定检查项和操作结果使用一致的中英文表达。`userCopy.ts` 将 native 失败映射为可执行的本地化说明和安全状态代码；设置、日志、智能学习、更新、视觉压缩与 Doctor 不再向页面、通知或弹窗直接回显原始异常、native summary 或路径/凭据相关文本。AST 文案策略测试同时阻止审核口吻、测试/演示词、AI 腔和可见原生错误直传；内部协议字段与测试源码保留。最终门禁结果以本次候选的桌面 Vitest、TypeScript、Vite build 与 E2E 实际输出为准。

2026-07-19 Codebase Memory 批量配置性能修复：261 MB pinned upstream 的 Rust debug 单次 SHA 校验原约 41.00 秒，且旧链路可能为每个 Agent 重复校验并并发占用窗口线程。当前实现将 SHA debug 热点优化、同一文件 identity 的 30 秒 single-flight 校验缓存和 verified launch 合并，实测冷校验 866 ms、缓存命中 25 us；缓存绑定 canonical path、size、mtime 与平台文件 identity，校验期间或随后发生变化时 fail-closed。Tauri 的状态、preview/apply/verify/rollback、UI/index 生命周期和 loopback 操作均移入 blocking worker；前端批量配置逐 Agent 串行调度，并在首个 Native 调用返回前显示 Agent、阶段和 current/total 进度。Code Graph Vitest `9/9`、TypeScript、Vite build、Chromium/WebKit Playwright `22/22`、workspace fmt/严格 Clippy、structure、support Python `11/11` 与 diff check 通过；全量 Rust 仅出现一次无关 Optical deadline timing failure，目标连续复跑 `2/2` 通过。最新 macOS release `.app`/DMG 已经 `build_and_run.sh --verify` 重建并启动。真实 OpenCode 1.14.51 回执为 initialize 1 次、tools/list 1 次、允许 tool call 2 次、拒绝 0 次，配置与批准仓库零写入边界随后精确回退；Windows 远程 runner 证据仍缺失。

同日完成安全与恢复一致性复审：运行时不再接受“同一签名者但 hash 不同”的 upstream，只允许 manifest 固定的平台 binary SHA-256 或明确登记的 macOS universal 派生 SHA-256；自动发现不再把 `.ai-bridge` 或相邻旧 bundle 作为供应链候选。页面明确标注 `Codebase Memory MCP v0.9.0`。Agent 批量配置状态由单一最后事务升级为 bounded v2 transaction set，刷新/重启后仍能恢复全部 active transaction 的批量回退入口；verify/rollback 从对应 manifest 恢复 Agent/scope/surface，写配置后若控制面状态落盘失败则自动补偿回退或返回 recovery handle。修复后 Code Graph Vitest `11/11`、全量桌面 Vitest `247/247`、Rust workspace `561 passed / 3 ignored`、Chromium/WebKit Playwright `22/22`、TypeScript、目标严格 Clippy 与 macOS release `.app`/DMG 重建启动通过。最新真实 OpenCode 验收回执为 initialize=1、tools/list=1、allowed tool call=1、denied=0，事务后 `code/opencode.json`、`.codebase-memory/`、`artifact.json`、`.gitattributes` 均为 absent。Windows 远程 runner 仍是 CBM-6 唯一外部证据缺口。

Tauri 构建必须通过 `npm run tauri:dev` 或 `npm run tauri:build`，以便先准备对应平台的 `dtoken-proxy` external binary。
