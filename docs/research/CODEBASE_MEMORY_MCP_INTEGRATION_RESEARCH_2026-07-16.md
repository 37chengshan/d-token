# codebase-memory-mcp 并入 d-token 研究与实施边界

更新时间：2026-07-16

## 结论

`DeusData/codebase-memory-mcp` 适合以 **显式授权、由专用 launcher 约束的实验性本地 sidecar** 接入：它为 Agent 提供结构化代码图谱查询与项目索引；d-token 保留 artifact、配置写入、项目范围、缓存、状态真实性、日志脱敏、安全网页与回退的控制权。

不建议复制上游 C runtime、调用其 `install`/`uninstall`/`update`，或使其成为 d-token 路由、压缩、Dashboard、Doctor 或 App 启动的必需依赖。上游当前 MCP transport 为 stdio，原生 UI 生命周期绑定其 MCP event loop；Agent-owned MCP session 与 d-token-owned UI/index session 必须分开建模。当前原生 UI 还包含目录浏览、路径/remote、mutation、日志和进程终止端点，不能直接作为 d-token 的“一键安全网页”。最终分发决策是 App 只 bundle 轻量受限 launcher：大型 headless artifact 由用户从固定官方 release 手工下载/解压，d-token 在默认或自定义目录自动发现后按 pinned manifest 验证，并提供带短期授权、只读 endpoint allowlist 的图谱页。

正式产品合同见 [`../PRD.md`](../PRD.md) `P0.7.2.1`；可执行任务与 DoD 见 [`../PLAN.md`](../PLAN.md) §1D。本文件是研究输入，不取代这两个真理源。

## 1. 范围、方法与证据等级

### 1.1 用户目标

目标能力是：

1. 在 d-token 中一键预览并配置指定 Agent 的 MCP/可选 Hook。
2. 选择一个项目后分别观测 Agent-owned MCP session，并按需启动 d-token-owned 索引/UI session。
3. 在验证实例仍归属 d-token 时一键打开本地图形化网页。
4. 保持 macOS/Windows、本地优先、最小写入、可审阅、可验证、可回退的既有产品边界。

### 1.2 审查输入

- 上游公开项目：[`DeusData/codebase-memory-mcp`](https://github.com/DeusData/codebase-memory-mcp)。
- 用户提供的本地源码目录：`.ai-bridge/codebase-memory-mcp-0.9.0/`。
- d-token 当前产品与安全契约：`docs/PRD.md`、`docs/ARCHITECTURE.md`、`docs/SECURITY_BOUNDARIES.md`、`docs/APP_FUNCTIONS_AND_PAGES.md`、`docs/product/IMPLEMENTATION_STATUS.md`。
- 现有接入/事务实现：`code/crates/dtoken-config-patcher/src/agent_integration.rs`、`code/apps/desktop/src-tauri/src/main.rs`、`code/apps/desktop/src/app/components/pages/Install.tsx`、`code/apps/desktop/src/app/lib/useRuntimeDashboard.ts`。

本地上游目录没有独立 `.git` 元数据。因此目录名 `0.9.0` 只表明研究输入，**不是**可用于供应链锁定的 tag/commit。实施前必须从受审查的 release artifact 重新固定 upstream tag/commit、平台 archive、SHA-256、签名资料、许可证和 NOTICE；不得把本地工作区的父仓库 HEAD 当成上游 revision。

### 1.3 证据规则

本报告把“源码可证实的行为”“README/配置文档声明”“d-token 架构推导”分开书写。任何未在真实 macOS/Windows artifact 上运行的行为均不写成已验证。上游版本、发行构件和签名链尚未固定时，不能声称已具备可发布的供应链证据。

### 1.4 多维度复审摘要

2026-07-16 第二轮复审从产品、架构、安全、运行、UX、供应链、跨平台、测试与文档一致性九个维度重新检查第一版结论，得到以下修正：

| 维度 | 第一版缺口 | 修正后的决定 |
| --- | --- | --- |
| 产品真实性 | 把“配置、进程、UI、索引、Agent 使用”压成近似同一条状态链 | 拆成 package/integration/agent_session/ui_session/index，证据不可互相推导。 |
| 进程所有权 | 暗示 d-token 能管理 Agent 启动的 stdio 子进程 | Agent-owned session 只观察；d-token 只停止自己启动的 ui_host。 |
| transport | 未说明一个 stdio 只能有一个 owner | Agent MCP 通过专用 launcher 透传；UI/index 使用独立 d-token-owned session。 |
| 网页安全 | 只依赖 loopback、Host/CORS 与随机端口 | 原生 UI 直出阻断；d-token host 使用短期授权与只读 endpoint allowlist。 |
| 文件边界 | 只限制 cache，遗漏项目仓库写入 | 强制 `persistence=false`，禁止 `.codebase-memory/`、`artifact.json` 和 `.gitattributes` 变更。 |
| 项目配置 | 未处理 `.codebase-memory.json` | 视为不可信输入，扩大扩展/范围时必须预览或拒绝。 |
| 供应链 | 未决定 bundle 与运行时下载 | App 不 bundle upstream、不代为下载；只打开固定官方 release，用户导入后按 pinned platform/arch/hash 验证，禁止 installer/updater/任意版本。 |
| 可观测性 | 计划使用上游未回显的 nonce | 不伪造 nonce；使用 launcher receipt、PID/birth token、exe hash、stdio/port 与 MCP method evidence。 |
| 可执行性 | Phase 只有主题，没有依赖/停止条件 | PLAN 改为 CBM-0–6 依赖链，每步带 Gate 与 DoD。 |

上述修正不意味着功能缩水，而是把“一键”建立在可证明的所有权和安全边界上，避免实现后再返工进程模型与网页权限。

## 2. 上游能力事实

### 2.1 核心形态与本地存储

上游是 C 实现的 MCP server，使用 stdin/stdout JSON-RPC。README 声明提供结构化图查询、代码索引、调用链、架构摘要、Cypher-like 查询、语义/文本检索与跨服务关系；这些能力由 Agent 调用 MCP 工具，而不是内嵌 LLM。

其默认缓存目录为 `~/.cache/codebase-memory-mcp`，可由 `CBM_CACHE_DIR` 覆盖；运行时设置存入 `_config.db`，UI 设置存入 `config.json`。`CBM_ALLOWED_ROOT` 可将 `index_repository` 的 canonicalized 路径限制在一个根目录内。该变量是 d-token 受管启动的必要条件，但它不能替代 d-token 对 symlink、敏感目录、配额和项目授权的预检。

### 2.2 生命周期：不是独立 daemon

上游 `src/main.c` 先创建 watcher/UI 后台线程，再调用阻塞的 `cbm_mcp_server_run(g_server, stdin, stdout)`。当 stdin EOF、信号或 MCP event loop 返回时，它立即停止 HTTP UI、watcher 与 store 并退出；非 Windows 还存在父进程退出监测。

这意味着：

1. 仅启动 binary 并不等价于可长期服务；必须由某个父进程持有 stdio 会话。
2. UI 不是单独可独立守护的服务；它与 MCP session 同生共死。
3. 现有版本没有通用 `/health` 或 `/healthz` endpoint。
4. d-token 可以为索引/UI 单独启动并持有 child stdio；这不等于拥有 Agent 启动的 MCP session，也不能宣传成已解决跨 Agent 会话的 daemon 需求。

若产品确实需要登录后常驻、跨 Agent 复用的图谱服务，应另立设计：上游新增 daemon 模式、d-token 维护最小 adapter/fork，或以新的受鉴权 local protocol 取代 stdio 占位。该工作需要单独的安全、维护、许可证和跨平台评审，不属于首期。

### 2.3 UI、端点与真实状态

上游 UI 是可选的 embedded-asset variant。请求 `--ui=true --port=9749` 时，只有 binary 包含 UI assets 才会监听；headless variant 只会警告，不能显示网页。配置文档把默认 UI 端口列为 9749，listener 绑定 loopback。

从 `src/ui/http_server.c` 可确认下列端点：

| 类型 | 端点 | 用途 | d-token 处理 |
| --- | --- | --- | --- |
| 只读补充状态 | `GET /api/index-status` | 索引进度 | 仅作为 index evidence。 |
| 只读补充状态 | `GET /api/project-health` | 数据库可打开、节点/边计数 | 不是 process health。 |
| 敏感读取 | `GET /api/browse` | 从 home/root 浏览目录名 | 没有按 `CBM_ALLOWED_ROOT` 裁剪的证据；正式网页禁用。 |
| 敏感读取 | `GET /api/repo-info` | 返回 root path、branch、remote | 正式网页禁用；不进入日志/导出。 |
| 敏感读取 | `GET /api/processes`、`GET /api/logs` | 上游进程/日志视图 | 正式网页禁用；不导入普通 d-token 日志。 |
| 变更 | `POST /api/index` | 启动索引 | 仅在 d-token 受控命令中显式调用或由用户在外部 UI 自行操作。 |
| 变更 | `DELETE /api/project` | 删除 graph DB | 不能绕过 d-token 的确认/receipt。 |
| 变更 | `POST /api/adr`、`POST /rpc` | 写 ADR/执行 RPC | 首期不作为 d-token 控制面。 |
| 高风险 | `POST /api/process-kill` | 终止进程 | d-token 永不委托；只按自身 identity manifest 停止。 |

上游有 loopback Host header/CORS 防护和请求体上限，这降低了浏览器跨站风险；但 loopback 不等同于“仅 d-token 可以调用”。其他本机进程也能访问该端口，目录浏览又没有证据表明受 `CBM_ALLOWED_ROOT` 约束，且部分端点会改变图谱数据。因此随机端口加风险提示仍不足以成为正式方案：当前原生 UI 直出是发布阻断，首期网页必须由 d-token-owned host 提供授权后的只读 projection。

### 2.4 索引与监测

上游提供自动索引和 watcher 选项。配置文档中 `auto_index` 默认 `false`；研究输入中的 watcher 对 Git 工作区轮询变更，非 Git 项目不纳入该持续监测路径。产品文案必须使用“Git 变更监测”，不能写成文件系统级实时检测。

索引会读项目源码，且上游 `get_code_snippet` 能读取当前文件。因此 d-token 的隐私边界不能只依赖“全部本地”；仍必须限制项目范围、排除敏感路径、隔离缓存、设置资源预算并禁止将内容回写入 Event Ledger、普通日志、导出或 Dashboard。

`index_repository` 的 `persistence` 参数默认 `false`，但启用后会写入项目内 `.codebase-memory/graph.db.zst` 与 `artifact.json`，并确保 `.gitattributes` 含 merge rule。首期必须固定 `persistence=false`，索引前后比较项目写集；若 Agent 请求 artifact export，launcher/策略层必须拒绝，而不是依赖提示词约束。项目级 `.codebase-memory.json` 还能增加 extension mapping，应视为不可信项目输入并进入预检。

### 2.5 配置与 Hook

上游 installer 能探测多个 Agent 并写 MCP entries、instructions 与 hooks。它适合单独安装时的便利流程，却不满足 d-token 对跨文件事务的要求：d-token 必须展示脱敏 diff、绑定 before/after hash、保存 durable backup、显式确认、写后语义校验并支持整笔精确回退。

研究输入中的具体能力并不统一：Claude 对 Grep/Glob 使用非阻断 `PreToolUse` 辅助；Codex 为 SessionStart reminder；OpenCode 以配置/instructions 为主。故不能实现“所有 Agent 同样安装一个 hook”的产品开关。MCP entry 和 Hook 必须是独立 consent、独立 receipt、独立 status 的配置面。

## 3. 与 d-token 的架构契合点

现有 d-token 已具备本次接入所需的基础边界：

1. `dtoken-config-patcher` 已有多文件 Agent Integration 的计划、diff、backup、apply、verify 和 rollback 语义。
2. Tauri `main.rs` 已有受管服务的 manifest、PID birth token、executable hash/path、loopback port owner、runtime identity 与 nonce 验证模型；可复用验证思想，但上游没有回显 d-token nonce，不能把本地生成值伪装成 upstream identity proof。
3. React 的安装与连接工作流已有真实数据/证据状态、transaction receipt 与统一刷新调度；代码智能适合作为「智能学习」子工作流，不应增加一级页面。
4. `dtoken-agent-bridge` 被限定为 metadata-only，不能承载或代理上游 MCP 工具、源码片段或 graph 内容。

建议新增 d-token 自有的 `dtoken-codebase-memory` 领域 crate 与单用途 launcher，并把两个进程所有权路径显式分开：

```text
Agent path
Agent → dtoken-codebase-memory-launcher → verified headless upstream (stdio)
          │ metadata-only lifecycle receipt
          └→ d-token observes; Agent owns parent/stdin/stdout/exit

UI/index path
d-token App/Tauri → dtoken-owned ui_host → verified headless upstream (stdio)
          │ read-only sanitized graph projection
          └→ authenticated loopback page; d-token owns start/stop
```

两个路径可以指向同一项目，但在证明 upstream cache 的跨进程锁与崩溃一致性前不得同时写同一 cache；首期可串行或为 UI host 使用只读/独立 namespace。launcher 不能是完全透明的 pipe：它必须有界解析 MCP envelope，以版本化 allowlist 允许普通只读 tool，并对 `index_repository` 校验 canonical root、拒绝 cross-repo、强制 `persistence=false`；ADR write、artifact export、任意路径覆盖和未知 tool 默认拒绝。解析后的 params/result 只在内存转发，不落日志或 receipt。React 只消费 `StatusSnapshot` 与 receipt，不拼接命令、不直接写 Agent 配置、不持有 child stdin/stdout、也不自行判断端口或进程归属。现有 `dtoken-agent-bridge` 继续保持 metadata-only，不代理这套 MCP payload。

## 4. 推荐产品与状态模型

### 4.1 受管 sidecar，而非 embed/fork

| 方案 | 结论 | 原因 |
| --- | --- | --- |
| 直接运行 upstream installer | 拒绝 | 会绕过 d-token diff、backup、consent、verify、rollback 和 supply-chain 控制。 |
| 复制/嵌入上游 C runtime | 不推荐 | 引入大规模语言/许可证/维护边界，且与 Rust-owned runtime 方向不符。 |
| 用户导入 pinned headless artifact + bundle 专用 launcher | 最终首期 MCP 路径 | 显著减小 App；Agent 保持 stdio owner，launcher 约束 artifact/env/project/cache 并只回传 metadata evidence。 |
| 直接打开 upstream UI variant | 拒绝首期 | 目录/路径/remote、mutation、logs、RPC 和 process-kill 端点不满足最小权限。 |
| d-token-owned 只读图谱 host | 推荐首期网页路径 | d-token 拥有进程、授权、端点 allowlist、浏览器 URL 和 stop 权限。 |
| 维护 daemon adapter/fork | 后续候选 | 只有产品明确需要跨会话常驻后才审查；不是首期假设。 |

### 4.2 五维状态

状态不能折叠为“已连接”。建议同时返回：

| 维度 | 状态 | 证据 |
| --- | --- | --- |
| package | missing / verified / untrusted / incompatible | release manifest、hash、platform、headless capability。 |
| integration | not_configured / review_ready / applied / verified / changed / partial / rolled_back | transaction manifest、config reread、hash、receipt。 |
| agent_session | not_observed / starting / observed / verified / exited / stale | launcher receipt、Agent ownership、PID/birth token、exe identity、MCP initialize/tool method evidence。 |
| ui_session | disabled / not_running / starting / ready / failed | d-token ownership、short-lived authorization、loopback owner、allowlisted probe。 |
| index | none / indexing / current / stale / error | explicit index task、project-health、index-status、freshness。 |

每一项还必须带 `owner`、`source`、`observed_at`、`freshness` 和 `warnings`。integration 被复读验证只代表配置正确；实际 Agent 已使用要求 launcher/upstream identity 与 MCP method evidence；UI ready 要求 d-token-owned host 与授权仍有效；index current 要求同项目的受控任务与 freshness。四者不能互相推导。

### 4.3 用户流程

```text
Discover (read-only)
  → choose one Agent + one project + MCP/Hook scope
  → review redacted diff, artifact and resource summary
  → explicit confirm
  → apply/verify transaction
  → Agent starts launcher/upstream stdio session (Agent-owned)
  → d-token observes metadata-only receipt
  → explicit index / optional Git change monitoring in d-token-owned session
  → start authorized read-only graph host
  → open verified loopback page
  → stop d-token host / rollback config / delete owned index
```

任何已发现的 unmanaged upstream process 都只能展示为 `unmanaged`；Agent-owned session 即使 verified 也不能由 d-token App 直接 kill/restart。d-token 的 Stop 只作用于它拥有的 ui_host/index task。

## 5. 安全与隐私要求

### 5.1 供应链

1. 首期 d-token bundle 只分发自有 launcher 与 license/NOTICE；页面固定打开官方 `v0.9.0` release，用户把对应平台 headless artifact 解压到默认或自定义目录。d-token 记录 source URL、revision、target、SHA-256 与验证时间，并在每次目录 metadata 变化后重新验证；上游 UI variant 不进入正式 bundle。
2. 每次启动前校验 artifact 的 hash、平台/arch 与 d-token manifest；失配 fail closed。
3. 禁止 upstream 自更新与 `CBM_DOWNLOAD_URL`；更新是一个新的用户可读 review 和 release 审查，不是后台行为。

### 5.2 Agent 配置

1. 默认 MCP-only，Hook 默认 off；每个 Hook 的 trigger、matcher、命令摘要、scope、信任/重启要求和回退点必须可见。
2. 必须复用 `AgentIntegrationTransactionManifest`，而不是用上游的文本替换逻辑。
3. 必须按用户选择的 Agent/scope 最小写入；不自动为所有已探测 Agent 创建配置、instruction 或 skill。

### 5.3 进程与浏览器

1. instance manifest 必须包含 kind/owner。Agent-owned session 绑定 launcher/upstream identity、允许参数/env、allowed root、cache namespace、PID/birth token 和 stdio/exit evidence；d-token-owned ui_host 另绑定 port owner、短期授权与资源上限。上游不回显的 nonce 不作为身份事实。
2. stop/restart/open UI 都要按 owner 再次验证；任一证据缺失时仅允许展示与诊断。d-token 不能停止 Agent-owned 或 unmanaged 进程。
3. 浏览器只打开 d-token-owned host 的字面 loopback URL；授权不可猜测、短期、单实例，Host/Origin/method/path/body 全部 allowlist。不得直接打开 upstream UI，也不允许网页透传任意 RPC/mutation。

### 5.4 数据范围与可观测性

1. 每个项目使用私有缓存 namespace、配额、TTL 和显式删除；不复用全局 upstream cache。
2. 强制项目 canonical root，预检并拒绝 escape、symlink/权限异常和敏感目录。
3. 默认排除 auth、credential、key、`.env*`、CCR/private recovery、d-token state/config、cache/generated/vendor。`.codebase-memory.json` 是不可信输入，新增 extension mapping 必须可见。路径规则必须持续测试并可解释。
4. 首期固定 `persistence=false`，索引前后验证仓库零写入；拒绝 `.codebase-memory/graph.db.zst`、`artifact.json` 与 `.gitattributes` 变更请求。
5. `CBM_DIAGNOSTICS`、auto-index、auto-watch、自更新和 download URL 均显式关闭；不继承未知环境变量。
6. Event Ledger、日志、导出只保留 instance/version/hash/scope digest、状态转换、资源桶、索引计数、错误类别和 transaction ID；不保留代码、graph、snippet、remote、ADR、MCP payload、原始命令、stderr、prompt/response 或密钥。

## 6. 分阶段实施与验收

### Phase A：供应链与领域类型

锁定 headless release artifact，建立 package/integration/agent_session/ui_session/index/event 类型与 owner 约束；完成许可证/NOTICE 登记和 fake-binary hash/target 测试。

### Phase B：launcher、项目范围与仓库零写入

实现专用 launcher 的 artifact/env/scope/cache 策略和有界 stdio 透传；验证 `persistence=false`、项目写集不变、敏感路径/symlink 拒绝、私有 cache/TTL/quota 和 payload 不落日志。

### Phase C：事务化 Agent 接入

在隔离配置目录中，为一个已支持 Agent 实现指向 launcher 的 MCP-only preview/apply/verify/rollback；再实现单独 Hook consent。覆盖 source hash 变化、半写、Agent parent exit、artifact 失配、语义校验失败与 exact rollback。

### Phase D：d-token-owned 索引与安全图谱网页

实现独立 ui_host、index/cancel/delete control plane 和只读 graph projection；网页使用短期授权与 endpoint allowlist，不复用 upstream 原生 HTTP UI。覆盖 Host/Origin、token 过期/重放、端口冲突、PID reuse、cache 并发与 mutation/RPC/目录枚举拒绝。

### Phase E：桌面状态、双平台与真实授权验收

在桌面中分别呈现 package/integration/agent_session/ui_session/index，再于 macOS/Windows 候选构件验证 artifact、私有缓存、路径/Unicode、Agent/UI owner、浏览器授权与停止身份；仅获得用户逐 Agent 授权后，验证一个真实 Agent method-level evidence 和完整回退。

首期 DoD 是所有隔离测试和双平台候选证据均通过，并明确展示实验性/会话绑定/Git 轮询限制。它不是 Headroom G0–G8 的替代 gate，也不解除真实用户 Agent 配置写入授权要求。

## 7. 许可证与后续决策

研究输入中的上游 `LICENSE` 为 MIT（Copyright 2025 DeusData）；`THIRD_PARTY.md` 还列有 tree-sitter grammars、SQLite、mimalloc、yyjson、xxHash、TRE、LZ4、zstd 与 embedding-derived data 等第三方材料。若 d-token 实际分发上游 binary、源码或其 NOTICE，必须在 `docs/legal/THIRD_PARTY_NOTICES.md` 添加准确的最终 artifact/revision、许可证文本/NOTICE 和修改边界；在此之前不得宣称已完成第三方适配。

本轮已经固定的决策：

1. 首期由 d-token release bundle 携带经审查的轻量 launcher，不携带大型 **headless** artifact。用户从固定官方 release 手工导入；“发现文件”与“信任文件”严格分离，只有 pinned platform/arch/hash 通过才可执行。App 不运行时下载，不 bundle upstream UI variant。
2. 首期不投入跨 Agent 常驻 daemon；Agent MCP 与 d-token ui_host 分离，后续只有明确收益和协议/维护评审后才重新讨论。
3. upstream 原生 UI 不直接开放；首期由 d-token 提供短期授权、只读 endpoint allowlist 的本地图谱页。
4. repo artifact export、ADR write、auto-index、auto-watch、Hook 和跨项目图谱默认关闭。

仍需在实施前确认的外部授权只有：哪些 Agent/Hook surface 获得逐平台真实配置写入与验收授权。在授权前，全部停留于隔离配置、fake launcher 与临时项目测试。
