# Agent 一键配置与 cc-switch 同步修复计划

生成日期：2026-07-19  
研究范围：`code/apps/desktop`、Tauri native、`dtoken-config-patcher`、`dtoken-core-api`、`dtoken-proxy`、正式产品文档、截图与本机 cc-switch 元数据  
目标：让用户选择 Agent 后进入一条真正可完成的“自动发现 → 必要决策 → 一键配置 → 验证”路径，并在用户切换/修改上游后，使 d-token 在安全边界内及时感知、同步或明确阻止旧路由继续生效。

> 本文件起初是研究与实施计划；2026-07-19 本轮已完成其中的首批安全闭环（Agent 可选/配置/清除入口、cc-switch schema/path 探测硬化、d-token-owned transport adapter）。尚未完成的 watcher、自动 reconcile、managed runtime reload 和 Headroom G0–G8 仍保持未完成标记。研究使用源码、测试、正式文档、两张用户截图和本机只读 metadata；没有把外部网页结论冒充为本地实现事实。

## 1. 执行结论

当前问题不是“页面少一个按钮”，而是六层契约断裂：

```text
Agent capability
  → discovery resolution
  → user task state
  → transport/config adapter
  → d-token route plan
  → runtime reload
  → Overview / Doctor / Changes / Logs evidence
```

现在每一层都有实现片段，但没有形成单一事实链：

1. 后端把 Claude Code、Codex 标为 `confirm-required`，前端只把 `one-click` 或已连接 Agent 放进主列表，用户无法选中它们进入主批次。
2. `confirm-required`、`prompt-only`、`manual-only` 是内部能力标签，却直接被渲染成“需手动连接/高级安装”，把工程限制变成用户任务。
3. `supportsAutomaticAgentIntegration()` 只表示 Claude/Codex/OpenCode 的 Hybrid Agent integration 能写入，不等于这些 Agent 的 provider/base URL transport 已经能自动改写；当前真正有完整 transport 配置事务证据的仍主要是 OpenCode。
4. cc-switch 发现只认一个默认 SQLite 路径，并把 schema `> 12` 全部判为 unsupported；本机真实数据库在 `~/.cc-switch/cc-switch.db`，schema 为 `13`，因此被当前代码判定为不可用，前端进一步显示“未检测到”。
5. cc-switch 同步只取第一个 active candidate，主要按 endpoint 比较，不绑定 app/profile/provider/model/protocol；多 Agent、多 profile 时会误判。
6. 外部配置变化不会触发 native refresh。前端只有 60 秒 fallback polling，`runtime-invalidated` 只由 d-token 自己的 mutating command 触发；`dtoken-proxy` 在启动时一次性加载配置，`AppState.config` 是不可热替换的 `Arc`。

因此本计划优先修复根因：统一“用户下一步动作”状态、补齐真实 Agent transport adapter、硬化 cc-switch adapter、增加外部变更 coordinator，再接通 runtime reload 和跨页面 evidence。

## 2. 已确认事实与证据

### 2.1 截图中的“手动”为什么出现

截图一中 Codex 同时出现“已识别，需手动连接”和“已自动解析”。这是两个内部维度被并排暴露：

- `capabilityLabel()` 将 `confirm-required` 渲染为“已识别，需手动连接”，`prompt-only` 渲染为“配置建议”，`manual-only` 渲染为“手动”：`code/apps/desktop/src/app/lib/install/presentation.tsx:163-168`。
- `presentBinding()` 又根据 resolution 显示“已自动解析/需要选择/需要凭据”：`code/apps/desktop/src/app/lib/install/presentation.tsx:153-160`。
- Install 只把 `capability === "one-click" || already_connected` 放进 `primaryBindings`：`code/apps/desktop/src/app/components/pages/Install.tsx:104-110`。
- secondary 行固定 `selectable={false}` 且 `included={false}`：`code/apps/desktop/src/app/lib/install/InstallView.tsx:197-210`。
- `installMachine` 的默认选择只纳入已连接或 `one-click + auto-resolved`：`code/apps/desktop/src/app/lib/installMachine.ts:234-243`。

结果是：即使后端已经解析出唯一 upstream，`confirm-required` Agent 仍不能由用户勾选；它只能停留在折叠的次级列表。

### 2.2 当前 route 配置并没有自动写入所有 Agent

当前 `preview_connection_route_plan` / `apply_connection_route_plan`：

- 重新读取 discovery，并绑定 `discovery_revision`：`code/apps/desktop/src-tauri/src/main.rs:4434-4459`。
- 拒绝 `prompt-only` / `manual-only`：`code/apps/desktop/src-tauri/src/main.rs:4488-4495`。
- 只校验候选 upstream、protocol、model、URL 和 credential-free 条件，然后写 d-token 自有 active config：`code/apps/desktop/src-tauri/src/main.rs:4497-4650`。
- 日志明确写出“不修改 OpenCode、Headroom、RTK、cc-switch”：`code/apps/desktop/src-tauri/src/main.rs:4412-4413`。

Agent integration 另走 `preview/apply/verify_hybrid_agent_integration`，目标是 instructions、hook/plugin、skill、MCP 等 metadata/integration surface，并明确声明 transport 独立：`code/apps/desktop/src-tauri/src/main.rs:2742-2789`。因此不能把它当成 provider/base URL 的自动连接实现。

当前真正的用户写入流程是：route plan 写 d-token active config；OpenCode 有专用配置事务；Claude/Codex 的 Hybrid integration 可以另行写受管文件，但没有统一的 transport adapter 将它们指向 d-token listener。实现状态也明确写着“只有 OpenCode 达到真实配置事务/routed flow 证据等级”：`docs/product/IMPLEMENTATION_STATUS.md:89,116`。

### 2.3 cc-switch 未识别的本机复现根因

本机只读 metadata：

- 文件存在：`/Users/cc/.cc-switch/cc-switch.db`
- 文件大小约 17 MB，最近修改时间为 2026-07-19
- `PRAGMA user_version` 返回 `13`
- 表存在 `providers`、`proxy_config`、`profiles` 等

代码路径：

- `discover_cc_switch()` 只调用 `cc_switch_db_path()`：`code/apps/desktop/src-tauri/src/upstream_discovery.rs:439-445`。
- macOS 只拼 `$HOME/.cc-switch/cc-switch.db`，Windows 只拼 `%USERPROFILE%\\.cc-switch\\cc-switch.db`：`.../upstream_discovery.rs:1213-1229`。
- 发现器对 `version > CC_SWITCH_SCHEMA_VERSION` 直接返回 `unsupported_schema`，当前常量为 12：`.../upstream_discovery.rs:19,472-478`。
- `cc_switch_sync_state()` 只有在 source state 为 `loaded` 时才设 `detected=true`：`.../upstream_discovery.rs:966-985`。
- `InstallView` 仅按 `cc_switch_sync.detected` 显示“已检测/未检测”：`code/apps/desktop/src/app/lib/install/presentation.tsx:88-92`、`.../InstallView.tsx:224-229`。

所以本机不是“没有安装 cc-switch”，而是“路径找到后被 future schema 门拒绝，随后 UI 把 unsupported 隐藏成 not detected”。这是本轮必须优先修复的 P0。

### 2.4 cc-switch 同步与 runtime reload 现状

- Native 只从 `providers` 和 `proxy_config` 读取白名单字段，`profiles` 目前只统计数量，不建立 profile 与当前 provider 的关系：`.../upstream_discovery.rs:480-585`。
- `cc_switch_sync_state()` 取第一个 active `cc-switch:*` candidate：`.../upstream_discovery.rs:974-982`。
- `enrich_upstream_discovery()` 主要按 normalized endpoint 与 d-token active provider 比较：`code/apps/desktop/src-tauri/src/main.rs:6369-6455`；没有绑定 app_type/profile/provider/model/protocol 的完整签名。
- `ConnectionSetupProvider` 的 fallback discovery 为 60 秒：`code/apps/desktop/src/app/lib/connectionSetup.tsx:78-82,142-159`。
- `invokeNative()` 只在本地 mutating command 成功后派发 `dtoken:runtime-invalidated`：`code/apps/desktop/src/app/lib/native.ts:44-99`；第三方 cc-switch 变化不会产生事件。
- `dtoken-proxy` 启动时 `load_runtime_config()` 读取一次 active config，`AppState` 将 config 放入 `Arc`：`code/crates/dtoken-proxy/src/main.rs:28-48,111-159`、`code/crates/dtoken-core-api/src/lib.rs:688-745`。

这意味着“用户在 cc-switch 切 profile 后 d-token 继续旧 upstream”是可预期的：没有外部 watcher/coordinator，也没有 runtime config reload 或 managed restart 的统一路径。

### 2.5 已有但未接通的设计资产

前端已有未接入 native source-of-truth 的 cc-switch 抽象：

- 多候选根目录：`code/apps/desktop/src/app/lib/ccswitch/types.ts:33-39`。
- `CcSwitchState`、profile/upstream/process/port/sync 状态：`.../ccswitch/types.ts:5-31`。
- 比较 protocol、host、port、path、provider/profile 的纯函数：`.../ccswitch/upstream.ts:26-67`。
- UI 对 `synced/out-of-sync/needs-confirmation/unsupported/unknown` 的人话动作：`.../ccswitch/upstream.ts:69-76`。

但这些模块没有被 `discover_upstream_candidates` 的真实 native snapshot 驱动，属于第二套潜在事实源；修复时应把它们收敛成 native contract 的纯展示/比较层，而不是再复制一套探测器。

## 3. 产品目标：把“能力表”改成“任务流”

### 3.1 用户看到的状态

普通用户不应看到 capability/resolution 的内部组合。每个 Agent 只显示一个主状态和一个下一步动作：

| 用户状态 | 何时出现 | 主动作 | 是否阻塞其它 Agent |
|---|---|---|---|
| 可立即连接 | 唯一高置信 upstream，或已授权的 transport adapter 可用 | 选中 / 连接 | 否 |
| 已连接 | 已指向 d-token 且验证证据新鲜 | 重新验证 | 否 |
| 需要选择 | 多个同等候选或模型歧义 | 选择已发现候选 | 只阻塞该 Agent |
| 需要补充凭据 | URL/protocol/model 已知，credential 不可安全复用 | 补充一次凭据 | 只阻塞该 Agent |
| 可生成配置建议 | adapter 不能安全自动写入，但可以生成脱敏 diff/步骤 | 生成建议 | 否 |
| 需要高级配置 | 未知协议、未知字段或没有安全 adapter | 打开预填高级配置 | 否 |
| 已失效需重扫 | review 后源 revision/fingerprint 变化 | 重新扫描 | 阻止旧计划应用 |
| 部分完成 | 批次中部分 Agent 已验证，部分仅有建议/失败 | 查看未完成任务 | 否 |

`confirm-required` 不应翻译成“手动连接”；产品语义应是“系统已准备好方案，需要你确认这一次连接”。只有真正不能自动写入时才显示“配置建议/高级配置”。

### 3.2 一键安装目标流程

```text
自动发现 Agent / 当前 upstream / cc-switch / d-token
  ↓
按下一步动作分组，并自动预选唯一高置信候选
  ↓
用户最多处理一次选择或凭据缺口
  ↓
展示一份人话摘要（技术 diff、backup、路径按需展开）
  ↓
一次确认
  ↓
按 Agent adapter 批量执行：Agent transport + d-token route + 必要 integration
  ↓
原子 receipt、服务启动/reload、真实 route verify
  ↓
Overview / Doctor / Changes / Logs 消费同一 revision 与 receipt
```

规则：一个 Agent 的失败不得阻塞其它可完成 Agent；批次失败时逆序回退已经写入的 Agent config，d-token runtime 可以保留但必须明确显示；建议型 Agent 不假装“已配置”，而是生成独立 suggestion receipt 和下一步按钮。

## 4. 推荐架构

### 4.1 统一 Agent adapter contract

新增 native-owned adapter registry，前端只消费结果，不复制 capability 真相：

```rust
struct AgentConnectionAdapter {
    agent_id: &'static str,
    detect: fn(&AgentScanContext) -> Result<AgentSnapshot>,
    build_transport_plan: fn(&AgentSnapshot, &RouteSelection) -> Result<TransportPlan>,
    preview: fn(&TransportPlan) -> Result<RedactedDiff>,
    apply: fn(&ReviewedPlan) -> Result<AgentReceipt>,
    verify: fn(&AgentReceipt) -> Result<VerificationEvidence>,
    rollback: fn(&AgentReceipt) -> Result<RollbackEvidence>,
}
```

最小首批：OpenCode、Claude Code、Codex。Cursor/Cline/Gemini 等先按真实能力进入 suggestion/manual，不要把未实现的 adapter 标成一键。

关键区分：

- `transport adapter`：使 Agent 请求真正经过 `127.0.0.1:18001`，并维护 provider/model/协议。
- `integration adapter`：写 instructions、hook/plugin、skill、MCP 等辅助 surface。
- `route adapter`：写 d-token 自有 active config。

三者必须有独立 receipt 和状态，不能用 Hybrid integration 的成功替代 transport 成功。

### 4.2 cc-switch discovery adapter

拆分现有 `upstream_discovery.rs`：

1. `CcSwitchPathResolver`：macOS/Windows 候选根目录、环境变量 override、已发现路径的 metadata-only evidence。
2. `CcSwitchStorageSniffer`：SQLite / unsupported format / locked / missing；不依赖单一 schema 整数。
3. `CcSwitchSchemaContract`：检查必需表/列是否存在。schema 13 若结构满足白名单则标记 `loaded_compatible_newer`；缺少必需列才 `unsupported_schema`。
4. `CcSwitchSafeReader`：只读抽取 app_type、provider id/name、is_current、normalized base URL、protocol、model、proxy port、credential-present bool；禁止读取/返回完整 `settings_config`、token、cookie、auth。
5. `CcSwitchSnapshot`：

```text
source_path, storage_kind, schema_version, observed_at,
app_type, provider_id, provider_name, profile_id/profile_name?,
is_current, proxy_enabled, proxy_port, protocol, model,
credential_present, normalized_endpoint, fingerprint
```

profile name 只有在 schema/字段明确安全时才输出；否则显示“当前 profile 名称不可安全读取”，不能把 profile_count 冒充 profile。

6. `CcSwitchSyncStateV2` 按 app_type/profile 建模，不再只取第一个 active candidate：

```text
detected | missing_path | locked | unsupported_format | unsupported_schema
current_snapshot_fingerprint
current_profile_ref (opaque or safe name)
dtoken_route_fingerprint
status: synced | out-of-sync | needs-confirmation | stale | unknown
last_verified_at
reason
```

### 4.3 外部变更 coordinator

推荐“watch + debounce + fallback poll”，而不是只把 60 秒 polling 调小：

- 监听已确认的 cc-switch DB、WAL/SHM 和 Agent config 文件的 metadata；不能监听整个 home。
- 事件合并窗口 500ms–1s，单飞执行 discovery；连续写入只触发一次 refresh。
- watcher 不可用时使用 5–15 秒低频 metadata poll，并记录 `watcher_mode`。
- 任何 snapshot fingerprint 变化都先更新只读 evidence；不直接覆盖 route。
- 用户在首次连接时显式开启“跟随此上游”授权后，才允许进入自动 reconcile；否则状态为 `out-of-sync`，提供“查看同步计划”。
- reconcile 只改 d-token 自有 active config；默认不写 cc-switch。计划绑定 `snapshot_fingerprint + active_config_before_sha + reviewed_after_sha`。
- apply 成功后：托管 d-token service 走身份校验的 reload/restart + health/config/route verify；非托管实例只显示 `restart_required`，禁止 kill/restart。
- apply/reload 失败保持 last-known-good，不把半成品写入 runtime；记录 `cc_switch_changed`、`sync_needed`、`sync_applied`、`sync_failed`、`reload_required` metadata-only ledger event。

### 4.4 cc-switch 的优先连接策略

如果 cc-switch 的 proxy 已启用且端口稳定，优先生成：

```text
Agent → d-token listener → cc-switch local proxy → selected upstream
```

这样用户在 cc-switch 内切换 provider 时，d-token 不必复制 provider credential 或重写每个 Agent route，只需重新读取 metadata 并验证 proxy/profile/协议是否仍匹配。只有 proxy 关闭、端口变化或协议不兼容时，才生成 d-token route sync plan。

## 5. TDD 实施包

### A — Discovery contract 与 schema 兼容（P0，先行）

范围：`code/apps/desktop/src-tauri/src/upstream_discovery.rs`、native test fixtures。

RED：

- schema 12 compatible fixture loaded；schema 13 本机形态 loaded-compatible；schema 14 缺必需列 unsupported。
- 默认路径、候选路径、env override、locked DB、missing DB、非 SQLite 文件分别得到不同 source state。
- 多 app_type、多 active provider 不丢行；profile_count 不再冒充 active profile。
- fixture 中包含至少 20 个 malformed/credential-shaped 字段，断言无秘密进入 snapshot/log/export。

GREEN：实现 `CcSwitchPathResolver`、schema contract、safe reader、fingerprint。

REFACTOR：删除 UI-only dead detector 的重复事实源，保留纯比较函数并改为消费 native snapshot。

DoD：本机 schema 13 显示“已识别/版本较新但兼容”，不再显示“未检测到”；unknown schema 仍 fail closed。

### B — 用户任务状态与 Agent 选择（P0，A 后可并行）

范围：`installMachine.ts`、`Install.tsx`、`InstallView.tsx`、`presentation.tsx`、相关 Vitest。

RED：

- `confirm-required + auto-resolved` Agent 出现在主列表，可勾选并进入 review。
- 多个 Agent 可分别选择不同 provider/model，不被一个全局 selection 阻塞。
- 缺凭据只弹一次凭据框，已解析 URL/protocol/model 保留。
- suggestion/manual Agent 不把整个安装状态设为 blocked；有“生成建议/打开预填高级配置”动作。
- partial 状态有明确下一步，不再只显示“仍需手动完成”；接通当前未使用的 `onOpenAdvanced`。

GREEN：将 capability/resolution 映射为单一 `UserConnectionTask`，主列表按 next action 分组。

REFACTOR：统一 Install/Advanced 的 route model 和审查组件，移除 capability badge 与 resolution badge 的冲突文案。

### C — 真实 transport adapters（P0，A/B 后）

范围：`dtoken-config-patcher`、Tauri commands、Agent fixtures。

RED：

- Claude Code、Codex、OpenCode 各自有 config fixture，preview diff 能证明 Agent 请求将指向 d-token listener。
- apply 前后 hash、backup、verify、rollback 各有测试；只写 allowlisted transport 字段。
- Hybrid integration 成功但 transport 未写入时不能显示“连接已验证”。
- source config 在 review 后变化时 apply 返回 stale，不覆盖用户新配置。

GREEN：实现 `AgentConnectionAdapter`，把 transport 与 hybrid integration receipt 分开并由 batch coordinator 统一回退。

REFACTOR：删除 Install 内按 Agent id 分支的重复 command 编排，改成 adapter registry。

### D — cc-switch sync coordinator 与 runtime reload（P0，A 后；与 C 部分并行）

范围：新增 native coordinator/watcher、`main.rs` command、`dtoken-proxy` / `dtoken-core-api` config reload 或受管 restart。

RED：

- 三次连续 DB/config change 只触发一次 discovery。
- external change → new snapshot revision → out-of-sync evidence；旧 review 不可 apply。
- 已授权 follow mode 下，cc-switch proxy port/profile 变化能生成 d-token-only sync plan。
- apply/reload 失败保留 last-known-good；非托管服务不自动重启。
- runtime config reload 或 managed restart 后，`/config/active`、health、route decision 与新 fingerprint 一致。

GREEN：实现 `CcSwitchSyncCoordinator`、single-flight、debounce、fallback poll、LKG state、reload/restart receipt。

REFACTOR：把当前 Install 专用 stop/start 逻辑下沉到共享 `reload_managed_runtime_if_needed`。

### E — Doctor / Overview / Changes / Logs 接线（P1，B/D 后）

范围：`lib/doctor/*`、Overview derivation、Changes、Logs、connection setup store。

RED：

- Overview 只在 snapshot + active route + runtime verification 三者一致时显示 synced。
- Doctor 的 `ccswitch-out-of-sync` 真正来自 native evidence，并提供 d-token-only preview/apply；未实现时保持 unavailable，不显示假修复。
- Changes 能看到 sync transaction/rollback receipt；suggestion-only 记录为只读建议，不冒充已应用。
- page remount、runtime invalidation、App restart 后同一 revision/freshness 保持一致。

GREEN：所有页面消费同一个 `CcSwitchSyncStateV2` / transaction receipt，不再各自推导。

### F — 文档、跨平台与发布门（P1/P0 release gate）

范围：`docs/PRD.md`、`docs/PLAN.md`、`docs/product/IMPLEMENTATION_STATUS.md`、`docs/ARCHITECTURE.md`、`docs/SECURITY_BOUNDARIES.md`、support fixtures/workflows。

必须更新：

- 明确“只读探测 cc-switch + d-token 受控同步”，不暗示写 cc-switch。
- 明确“自动同步”必须有用户授权、fingerprint、LKG、verify 和可回退证据。
- macOS/Windows 路径、权限、SQLite/WAL、应用安装和非托管服务差异各有 fixture。
- 同一候选 commit 提供外部 profile 切换 → d-token route 更新 → runtime verify → ledger receipt 证据。

## 6. 依赖与批次安排

```text
A Discovery contract/schema
├── B User task state / selection
├── D Watch + sync coordinator
└── C Transport adapters
       └── E Cross-page evidence
B + C + D ───────────────┘
E ──> F docs / cross-platform / release gate
```

建议分三轮集中实施：

1. **Round 1（P0 根因）**：A + B。先让 schema 13 被正确识别，且 Codex/Claude 等已解析 Agent 能被选中进入主 CTA；不改变外部写入边界。
2. **Round 2（P0 闭环）**：C + D。补齐真实 transport adapter、cc-switch watch/sync、managed runtime reload；这是“选择后真的一键配置”和“切换后及时转发”的核心。
3. **Round 3（P1 收口）**：E + F。统一跨页状态、Doctor 修复、Changes receipt、文档和双平台 release evidence。

## 7. 验收标准

### 用户路径

1. 只有 Codex 且当前 upstream 唯一可解析时，首屏显示“已为你准备好”，Codex 可选中，主 CTA 可进入一次 review，不出现“需手动连接”。
2. Claude Code/Codex/OpenCode 可同时选中，允许各自使用不同 upstream/model；一次确认后逐项 receipt 可见。
3. 缺 credential 时只要求输入 credential；不要求用户重新输入已发现 URL、protocol、model。
4. Cursor/Cline/Gemini 等未完成 adapter 的 Agent 显示“可生成建议/需高级配置”，有明确动作，不阻塞其它 Agent，也不假报已完成。
5. apply 后失败能说明已保留、已回退和未写入对象；Changes 可打开对应 receipt。

### cc-switch

1. 本机 `~/.cc-switch/cc-switch.db` schema 13 显示 detected + compatible-newer，不再显示 not detected。
2. `loaded / missing / locked / unsupported-format / unsupported-schema` 状态可区分，用户能知道下一步而不是看到空红牌。
3. 多 app_type/profile/provider 不丢失，UI 不再只显示第一个 active candidate。
4. 不读取或输出完整 `settings_config`、api key、token、cookie、auth、profile blob；snapshot/fingerprint/ledger 只有 allowlisted metadata。
5. 用户在 cc-switch 切换 profile/provider 后，在 debounce 窗口加 refresh 预算内产生新 fingerprint；d-token 显示 synced/out-of-sync/stale 的真实状态。
6. 已授权 follow mode 且 proxy 仍稳定时，d-token 通过 cc-switch proxy 继续使用新 upstream；若需要改 d-token route，则走 preview → diff → backup → confirm → apply → managed reload → verify。
7. 非托管 d-token 服务不被自动 kill/restart，只显示 restart required；reload 失败保持 last-known-good。

### 安全与发布

1. 外部 cc-switch 永远只读；d-token 只改自己的 active config，除非未来单独设计并审查第三方写入边界。
2. 所有自动同步均绑定授权 scope、snapshot fingerprint、before/after SHA、transaction ID、rollback ID 和 runtime verification。
3. macOS/Windows 均有路径、schema、锁文件、权限、proxy port、unmanaged service fixtures。
4. 未有真实外部变化到 runtime route 的同候选 evidence 前，不得在 UI 或实现状态中写“已支持自动同步”。

## 8. 明确不在本计划范围

- 不自动安装、升级、启动或写 cc-switch 本体。
- 不读取第三方认证文件或复制明文 credential；credential 仍只进入 OS credential store/受控 provider ref。
- 不让 d-token 通过外部 Headroom 或 standalone RTK 提供 Dashboard/runtime 真相。
- 不为未有安全 adapter 的 Agent 伪造一键成功。
- 不把“watch 到文件变化”误写成“所有 Agent 都能无条件热更新”；无法安全解析或需要新 credential 时必须停在明确的 needs-action 状态。

## 9. 研究来源与方法

### 本地实现证据

1. `code/apps/desktop/src/app/components/pages/Install.tsx`
2. `code/apps/desktop/src/app/lib/installMachine.ts`
3. `code/apps/desktop/src/app/lib/install/presentation.tsx`
4. `code/apps/desktop/src/app/lib/install/InstallView.tsx`
5. `code/apps/desktop/src/app/lib/connectionSetup.tsx`
6. `code/apps/desktop/src-tauri/src/upstream_discovery.rs`
7. `code/apps/desktop/src-tauri/src/main.rs`
8. `code/apps/desktop/src-tauri/src/agent_registry.rs`
9. `code/crates/dtoken-config-patcher/src/agent.rs`
10. `code/crates/dtoken-config-patcher/src/agent_integration.rs`
11. `code/crates/dtoken-proxy/src/main.rs`
12. `code/crates/dtoken-core-api/src/lib.rs`
13. `code/apps/desktop/src/app/lib/ccswitch/{types,detector,upstream}.ts`（现有未接通前端抽象）

### 产品与安全证据

- `docs/PRD.md:228-303,398-420`
- `docs/PLAN.md:68-99,141-188`
- `docs/APP_FUNCTIONS_AND_PAGES.md:30-49,80-83`
- `docs/product/IMPLEMENTATION_STATUS.md:89,101-120`
- `docs/SECURITY_BOUNDARIES.md:139-147`
- 用户截图：`/var/folders/q8/c82vj8fs04n4bt80pynrqsvc0000gn/T/codex-clipboard-521d2998-e59d-41e5-a39c-93b4529f880a.png`、`/var/folders/q8/c82vj8fs04n4bt80pynrqsvc0000gn/T/codex-clipboard-16f26529-1a53-46d8-bcaf-cc64bb98c9b3.png`

### 实机只读验证

- `~/.cc-switch/cc-switch.db` 存在。
- `PRAGMA user_version` 为 `13`。
- 仅读取 schema/table metadata；未读取 provider rows、profile payload、credential 或 auth 内容。

置信度：当前代码根因和本机 schema 结论为高；其它 cc-switch 版本的路径/字段兼容性为中，必须由 A 包的 fixture 与双平台实机门确认。

## 10. 2026-07-19 本轮实现回执

- 安装页不再按 `one-click` 把已检测 Agent 隐藏到不可选的次级列表；所有已安装 Agent 都可选择，并提供“配置/清除”入口。`prompt-only`/`manual-only` 不再把整个页面误报为 blocked，而是进入 partial 并保留高级配置路径。
- `dtoken-config-patcher::agent_transport` 新增统一 d-token-owned namespace：Claude Code/Gemini CLI JSON、Codex TOML、Cursor/Cline extension JSON。配置与清除均只操作 `managed_by = "d-token"` 的字段，未知或未标记结构 fail-closed，使用 ConfigPatcher 的 hash/backup/rollback/verify receipt。
- Tauri 暴露 `configure_managed_agent_transport`、`clear_managed_agent_transport`、`verify_managed_agent_transport`。OpenCode/OpenClaw 仍走多供应商高级适配器，不被通用 transport 命令误写。
- cc-switch 路径支持 `CC_SWITCH_DB_PATH`、macOS Application Support、Windows `LOCALAPPDATA/APPDATA/USERPROFILE` 候选；schema 13 及结构兼容的未来 schema 通过白名单列探测后标记兼容状态，不再把“未识别”伪装成“未安装”。
- 尚未完成：第三方配置 watcher/debounce、cc-switch 到 d-token route 的自动 reconcile、运行中 proxy 热 reload、Doctor `agent_cc_switch_sync` 可执行修复，以及 Headroom G0–G8。上述能力仍必须以真实跨层 receipt 验收后才能标记完成。
