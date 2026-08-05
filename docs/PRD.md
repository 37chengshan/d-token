# d-token Living PRD

## P0.0 发布前最终收口：产品目标规格 v2.1

状态：2026-08-05 发布前最终迭代基线。
本节吸收 `d-token-product-target-spec` v2.1 的产品目标与执行约束；它是本 PRD 的补充输入，不是实现完成证明。若本节与后文历史草案冲突，以本节、当前代码证据和 `docs/PLAN.md` 的最新状态为准。

### P0.0.1 版本语义

后续产品、实现和验收文档统一使用以下优先级：

- **MUST**：目标版本的发布阻断条件，未满足不得称为可发布；
- **SHOULD**：目标版本优先交付，但可以在明确记录风险后延期；
- **MAY**：可选能力，不得阻塞核心发布。

未被当前候选的真实运行证据、正式入口、错误路径、安全降级、回滚和文档同时证明的能力，只能标记为 `preview`、`partial`、`unavailable`、`unknown` 或 `not_verified`。不得因为存在类型、接口、静态页面、metadata-only 事件、passthrough 或单元测试而标记为完成。

### P0.0.2 产品定义与用户闭环

d-token 是面向 AI 编码 Agent 的本地上下文控制面：

```text
发现 Agent / 项目配置 / 用户配置 / cc-switch
  → 建立带来源、时间、新鲜度和修订号的状态快照
  → 用户授权接入策略
  → 预览配置变更与风险
  → 备份、应用、验证、必要时回滚
  → 真实 Agent 请求进入 d-token-owned runtime
  → Context Relay / 可信压缩 / post-validation
  → 显式解析 Provider、模型、协议和 endpoint
  → 转发并采集 Provider 用量
  → 写入 Optimization Receipt 与 Event Ledger
  → Dashboard / Analytics / Doctor 展示同一事实投影
  → 外部配置变化后按授权策略重新发现、同步或请求用户决策
```

用户安装并授权一次后可以继续正常使用 Agent；只有在存在风险、冲突、低置信识别、新凭据或需要选择时才打扰用户。所有后台自动行为必须可在 Event Ledger 中追踪。

### P0.0.3 Agent 与配置闭环

所有被支持或被发现的 Agent 都必须进入统一配置模型，正式 UI 不得只因为 capability 字段相似就猜测“已配置”。每个 Agent 卡片必须能够显示：

- 当前上游、Provider、模型、协议、脱敏 endpoint、来源、更新时间和验证状态；
- `配置` 与 `清除` 两个独立动作；
- 最近一次配置/清除 receipt 和下一步动作；
- 配置格式未知、候选多义或 schema 未验证时的 `advanced-required` / `needs-selection` 状态。

配置与清除均遵循：

```text
detect → parse current upstream → plan → redacted diff
→ backup → user confirm → apply → reread/verify
→ receipt；失败则精确回滚原始字节
```

配置只写 d-token-owned 字段、provider 或 profile；清除只删除带 manifest/hash ownership 证据的内容，保留用户已有 Provider、模型和凭据，且必须幂等。OpenCode、OpenClaw 等多供应商 Agent 使用专用 multi-provider adapter；未知格式不得套用其他 Agent 的字段。

Rust-owned 合同必须是前后端唯一事实源：

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

前端只消费这些合同和 receipt，不根据 `capability`、文件存在或静态 `configured=true` 自行推断已接入。任何 plan 在 review 期间发现 source revision 或 fingerprint 变化，必须立即失效并重新发现。

状态语义固定为：

| 状态 | 允许表达的事实 |
| --- | --- |
| `detected` | 发现了文件、进程或配置 |
| `configured` | 配置中存在 d-token-owned 设置，尚未证明真实流量 |
| `connected` | 至少一条真实请求成功经过 d-token |
| `verified` | 当前连接、路由或能力具有有效证据 |
| `healthy` | 对应 Doctor 检查项在当前 revision 通过 |
| `active` | 当前 Runtime 正在使用该 route/capability |
| `supported` | 代码声明支持，不代表当前环境已验证 |

任何 Agent 卡片、Doctor 和 Dashboard 都不得把 `detected`、`configured` 或 `supported` 直接显示成 `connected`、`verified` 或 `healthy`。

### P0.0.4 cc-switch 与动态路由

`cc-switch` 永远是只读外部观察源，d-token 不写其数据库、配置或凭据。必须支持多 app type、profile/provider 关系、多 active candidate、schema 状态、脱敏 endpoint、credential-present、fingerprint 和 revision；无法高置信识别 profile 时保留 `unknown`，不得把 provider 名冒充 profile。

首次连接时用户明确选择 `follow`、`lock` 或 `observe`：

- `follow`：检测到 Provider、模型、协议、endpoint 或 profile 变化后，先以 `source_fingerprint + source_revision + runtime_identity` 绑定 route sync plan；高置信、无新凭据且只更新 d-token-owned route 时可自动同步；
- `lock`：只记录外部变化并提醒，不静默覆盖 d-token route；
- `observe`：只检测和展示，不写入 Agent、cc-switch 或 Runtime。

版本边界：`v0.2.0-beta.1` 默认只交付只读发现、`observe` 和明确冲突提示；`follow` 的自动 route sync 是 `v0.3` MUST，除非另有当前版本的真实证据、显式授权和回滚验证，不得把实现中存在的 watcher 误写成 Beta 已交付。

监听必须覆盖已确认的数据库、WAL/SHM、profile metadata 和父目录，采用 750ms debounce、单飞刷新和 5–15 秒 polling fallback；SQLite 锁定、损坏、未来 schema、缺失和 watcher 不可用都要留下明确证据。Runtime 采用可验证的原子 route snapshot：旧请求继续使用旧快照，新请求使用新快照；reload 失败保留 last-known-good，并记录 `external_config_detected`、`external_config_changed`、`route_sync_*`、`runtime_reloaded` 或 `runtime_reload_required`。

### P0.0.5 LeanCTX / Context Bus 独立能力域

LeanCTX（以及与其行为对齐的开源 Context Bus / Context Relay 能力）不是 Headroom codec 的别名，也不是可以被“Headroom 已迁移”一项吞掉的附属实现。它负责跨 Agent、跨会话和项目作用域的结构化上下文复用；Headroom 负责请求内的可信压缩与转发。两者必须分别记录来源、许可证、参考版本、变更范围、能力矩阵和发布证据。

LeanCTX 对齐的 d-token-owned 能力边界：

1. `Context Bus`：以用户、项目、Agent、Session 为隔离边界，传递脱敏的结构化上下文；
2. `Session Cache`：保存文件 hash、版本、修改时间、符号表、signatures、code map 和已验证结论，不保存 secret、凭据或未脱敏原文；
3. `Context Relay`：在 Agent B 请求同一项目同一版本内容时复用允许的结果，并记录来源、命中、节省和新鲜度；
4. `Query-aware Retrieval`：按当前任务选择相关上下文，保留恢复引用，不把 Relay 命中冒充压缩收益；
5. `Recovery Pointer / Manifest`：允许用户在授权边界内定位被省略内容，文件变化、TTL 到期、权限变化或来源不可信时立即失效；
6. `Stale / Superseded Context`：显式区分新鲜、过期和被替代上下文，禁止用旧 cache 继续回答；
7. `SharedContext / memory`：结构化、可审计、可导出/删除，不能把整段 prompt/response 当作普通记忆。
8. 同 Agent 自命中防护、Relay 内容大小边界和 Relay/普通 Session Cache 隔离；
9. 每次命中必须在 Receipt 中区分“压缩节省”和“Relay 避免的重复传输”，不得双计。

LeanCTX 与 CCR 的职责也必须分开：

| 能力 | 责任 |
| --- | --- |
| LeanCTX / Context Bus / Relay | 复用已验证的结构化项目上下文，强调 scope、freshness、provenance 和失效 |
| CCR | 为一次压缩请求提供可逆恢复的私有原文存储，强调加密、TTL、配额、权限、retrieve/delete |

当前仓库的 Headroom provenance 记录了上游移除 `RTK/lean-ctx` 的历史决策，但没有因此形成可发布的 LeanCTX 迁移证据。LeanCTX 的来源事实仍是开放决策：实现前必须登记上游仓库、固定参考 commit、许可证、NOTICE、行为差异和 d-token-owned 替代边界。除非另有独立的正式入口、隔离/失效/回退测试和真实跨 Agent 证据，否则 LeanCTX 状态必须是 `unavailable` 或 `partial`，不得宣称“已完成”。

版本边界：

- `v0.2.0-beta.1`：MUST 保留 Context/Relay/Memory 的安全接口、scope 和 fail-closed 合同；不把未完成的跨 Agent 复用包装成发布能力；
- `v0.3`：MUST 完成 Context Bus、Session Cache、Context Relay、Map/Signatures 复用、Query-aware Retrieval、Recovery Pointer、Stale/Superseded Context 和 CCR crash recovery；
- `v0.4`：SHOULD 完成多 Agent 长期运行、低风险自动修复和项目/团队策略。

任何 Relay 失败都必须回退正常读取；文件变化、项目变化、Agent/session 不匹配、权限不足、来源不明或无法验证新鲜度时必须拒绝命中并留下证据。

### P0.0.6 Headroom 完整迁移发布阻断矩阵与 Optical 边界

Headroom 只有在固定参考版本的能力矩阵逐项具备真实请求、错误路径、安全降级和回滚证据后，才允许在产品状态中标记“完整迁移”。发布阻断矩阵至少包括：

| 能力域 | 发布要求 |
| --- | --- |
| Library/SDK、proxy、wrap/unwrap、MCP | 正式入口可达，真实请求和错误传播可验证 |
| ContentRouter、JSON/log/search/build/diff/text/HTML/code codec | active 路径、语义/结构校验、负收益拒绝、超时/取消、secret redaction 和 rollback 证据 |
| CacheAligner、live-zone、缓存前缀 | 稳定前缀、失效、TTL 和 cache evidence 可验证 |
| CCR / retrieval / delete | 加密、TTL、配额、权限边界、崩溃恢复和精确删除证据 |
| memory / SharedContext / learn / output shaping | 授权隔离、provenance、失效、回退和 route-aware 真实使用证据 |
| ledger / stats / history / export / Doctor | 与同一 request id、route revision 和 receipt 对账 |
| integrated RTK | d-token-owned provenance、命令改写/过滤、tee/tracking、fail-safe、卸载和跨平台证据 |

`passthrough-only`、`metadata-only`、`preview-only`、测试专用实现或依赖外部 Headroom `8787` 的路径均不计为迁移完成。Optical 仅指“代码与上下文 → PNG → 多模态模型输入”的上下文优化链；普通图片 resize、re-encode、tile、ONNX 等不属于本产品范围。Optical active 默认关闭，只有签名 capability、确定性 renderer、post-validator、资源上限和目标模型质量/成本/延迟评估全部通过后才可达；此前只能是 shadow/preview。

本矩阵是“Headroom 完整迁移”长期发布阻断门，不等同于 `v0.2.0-beta.1` 的全部 MUST；Beta 只允许交付 P0.0.7 明确列出的 Headroom 核心可信压缩子集，其余能力保持 `partial`/`unavailable` 并进入后续版本门。

### P0.0.7 `v0.2.0-beta.1` 发布门

当前公开试用候选的 MUST 范围是：首次引导、Agent/cc-switch 发现、配置事务与恢复、OpenAI Chat/Responses 和 Anthropic Messages 路由、Headroom 核心可信压缩、Optimization Receipt、Local Token 与 Provider Token 分离、Doctor、OpenCode 与 Codex 各至少一条真实 routed-flow、macOS/Windows 候选安装包及安装/升级/卸载恢复证据。

其中 Codex routed-flow 不能由“配置事务已精确恢复”替代，必须有独立的真实请求报告，至少绑定 `agent_id=codex`、当前 provider/model/protocol、route verified、请求匹配数、Event Ledger/metrics 计数、失败分类和恢复状态；未有该报告时 Codex 只能显示 `configured`/`not_verified`。

发布前必须同时满足：

1. 所有正式 UI 动作接通 d-token-owned runtime、Tauri command 或 Event Ledger；禁止 mock 成功状态；
2. 同一候选 revision 绑定双平台构建、哈希、安装启动、运行链路、配置恢复和已知限制；
3. 压缩失败、Relay 失败、Receipt/Analytics 失败、Provider 失败和 reload 失败都安全降级或明确阻断；
4. `G0–G8` 全部通过，且每个能力域拥有当前 revision 的真实证据；否则 `release_ready=false`；
5. 当前状态快照必须诚实反映证据：Headroom 尚未完整迁移，任何未通过门不得在 App、README、发布说明或 PRD 中写成“已完成”。

物理 Token 变化、Local Token 估算和可声明收益必须分开：`estimated` 只能作为估算展示；`claimable` 只有在 Provider 计费/usage 证据和归因链完整时才可为 true；Relay 避免的重复传输不得计入压缩收益，shadow、fallback、Provider 失败不得计入 actual savings。

`v0.3` 发布门除能力实现外，还必须证明：两个 Agent 复用同一项目允许上下文；文件变化后旧 Relay 失效；跨项目污染为零；Relay 失败安全回退；Receipt 区分压缩节省和 Relay 避免的重复传输；cc-switch 切换后状态一致或明确提示冲突。

`v0.4` 发布门还必须证明：常见 Agent 与 cc-switch 长期稳定、Doctor 可修复大部分低风险问题、Token 节省和任务质量有公开可复现证据、隐私安全会话报告、企业代理/证书适配、稳定签名和签名自动更新。

### P0.0.8 信息架构对照与 Context 入口

规格中的 `Context` 不是新增工程化一级页面。为保持产品固定的八页 IA，Context Bus、LeanCTX Relay、Code Map、Signatures、Memory、Stale/Superseded Context 和 Recovery Pointer 按用户任务落位：

| 用户任务 | 正式入口 |
| --- | --- |
| 当前上下文、Relay 命中、新鲜度和节省摘要 | 总体概览、分析 |
| Agent/项目发现、上下文扫描和授权策略 | 安装与连接工作台的智能学习/高级安装上栏 |
| Code Map、Signatures 和索引状态 | 代码图谱（实验性） |
| Memory、SharedContext、失效和授权 | 分析详情、Doctor |
| Recovery Pointer、CCR retrieve/delete、变更证据 | 变更与回退、请求/Receipt 详情 |

不得为 Headroom、LeanCTX、CCR 或 Context Bus 另建用户级独立页面；新增页面若不能对应一个用户任务，发布验收不通过。

### P0.0.9 未决产品决策登记

以下决策在具体版本实现前必须显式落档，不得由临时代码默认值替代：

| 决策项 | 当前状态 |
| --- | --- |
| 默认配置模式 | `open`：首次使用必须明确选择 `observe`/`lock`；`follow` 需单独授权 |
| 默认压缩策略 | `resolved`：安全、可降级为默认；增强/有损必须显式确认 |
| Context Relay 持久化 | `open`：先满足项目隔离、TTL/quota、失效和删除，再确定默认存储 |
| 数据保留周期 | `resolved`：用户可配置，默认值需在隐私/诊断证据中标明 |
| Telemetry 默认状态 | `open`：必须可关闭，默认值随正式隐私说明一并决定 |
| 自动修复范围 | `resolved`：仅低风险、可逆、可验证动作可自动化 |
| Provider 回退策略 | `resolved`：禁止静默跨 Provider；任何 fallback 需用户策略与 receipt |
| 云端能力 | `resolved`：不是本地核心运行依赖，默认不上传代码、Prompt 或凭据 |
| LeanCTX 上游来源 | `open`：在实现前登记仓库、固定 commit、许可证、NOTICE 和差异说明 |

本规格未覆盖 Optical 的协议、安全和 UI 细节；该能力按本 PRD P0.0.6、§8、`AGENTS.md` 安全边界执行，默认 shadow/preview，不能以规格文本缺失为由扩大为普通图片压缩。

## P0 发布版产品决策：从当前版本到稳定发布

状态：2026-08-05 用户确认的发布前产品口径。本节优先级低于 `P0.0` 的最终收口条款、高于旧页面草案；旧技术链路、runtime 契约、测试契约继续作为实现约束保留。

### P0.1 产品形态

第一版 d-token 采用“普通用户友好 + 高级能力折叠”的混合型产品形态。

默认体验必须像一个能直接使用的本地桌面 App：用户打开后可以看到总体状态、启动服务、一键安装或配置本地 AI Agent、检查 cc-switch 与上游、查看用量分析、诊断问题、修复问题、查看日志、回退配置。

高级能力必须存在，但不能把普通用户直接推入工程语言。`runtime`、`route adapter`、`payload`、`Event Ledger`、`provider profile` 等概念只能在高级详情、开发日志或技术附录中出现。正式 UI 主文案应使用“本地服务”“连接路径”“模型服务商”“本地操作记录”“连接检查”“恢复配置”等用户能理解的表达。

### P0.2 发布目标

当前版本不按“内部 demo”标准验收，而按“可公开试用的稳定 Beta / 候选发布版”推进。发布前必须满足：

1. 所有主页面有完整 UI、完整中文文案、清晰空状态、加载状态、错误状态和成功状态。
2. 所有主流程功能可用，不允许只有静态展示或纯 mock 完成态。
3. 本地服务安装、启动、停止、状态检测、端口检测、Agent 检测、cc-switch 检测、配置预览、备份、应用、验证、日志、诊断、回退必须形成闭环。
4. fixture/mock 只能存在于 `support/`、自动化测试、CI 或隐藏 dev harness；正式 App 与普通浏览器预览均使用诚实空态，真实运行完成态必须来自 d-token 自有本地服务、Tauri command 或 d-token 自有本地记录。
5. UI 视觉目标是现代、干净、克制、可信、稳定，不能像开发调试面板，也不能像测试中心。
6. d-token 第一版只安装和管理 d-token 自有本地服务；对 Claude Code、Codex、OpenCode、Cursor、Cline、CommandCode、cc-switch 等第三方工具只做检测、读取配置、生成配置计划和受控配置写入，不自动安装第三方工具。
7. App 的运行状态必须跨页面、跨导航和跨 App 重启保持一致；不得因页面重新挂载、异步加载或局部请求失败把已运行服务闪回为“未启动”，也不得让“已连接”和概览无数据互相矛盾。
8. 候选发布版必须同时提供可安装 macOS App/DMG、Windows 安装包、产物哈希、平台运行证据和明确的已知限制；macOS 先完成真实窗口与功能验收，Windows 复用同一 React UI，并补齐平台桥接、安装、升级与回退验证。
9. 在任何真实 Agent / upstream 验收前，所有正式可见页面、二级页面、控件和状态必须先完成“真实数据源 → 统一状态 → 持久化 → refresh/invalidate → 切页保持 → App 重启恢复 → 错误/partial 语义”闭环。不得一边显示“未检测/未接入”，一边从另一组件展示已发现候选或已连接结果。
10. 所有正式可见动作必须接通真实 Tauri command、d-token-owned runtime 或 Event Ledger，并返回可读 action receipt；未实现能力不得以可点击按钮、假成功、永远 unavailable 的装饰功能留在正式主流程。确实暂未支持的能力必须隐藏到后续版本，或以不干扰主流程的明确只读限制呈现。
11. 真实测试开始前必须完成逐页功能—数据源—状态—持久化矩阵并通过真实 App 七页检查（含视觉压缩 Beta）。只有当页面间状态一致、关键动作可用、顶部通知/弹窗接通且重启不丢状态后，才允许运行 OpenCode 官方免费模型 routed / compression workload。

12. 公开发布 workflow 必须 fail-closed：macOS 缺 Developer ID、notarization、staple 或 Gatekeeper 证据时不得产出可发布 artifact；Windows 缺 Authenticode 有效签名、NSIS 安装启动证据或卸载恢复证据时不得产出可发布 artifact。开发期 ad-hoc/unsigned bundle 只能标记为本机候选。
13. 每个平台的发布 artifact 必须携带 SHA-256 manifest、候选 provenance、版本、架构和已知限制，并与自动化门禁、真实安装启动及配置恢复绑定同一候选提交；源码或工作树变化后旧 artifact 自动失效。
14. 自动更新必须使用签名更新 artifact、固定公钥验证和 HTTPS feed；没有可信 feed/密钥时不得下载或安装更新，自动更新控件不得表现成已可用。远程发布、推送或创建 Release 仍需用户明确授权。
15. 公开试用候选不得只交付 Headroom 的 proxy/dashboard 外壳或 metadata-only 占位层。§8 列出的 Headroom 能力必须全部由 d-token-owned runtime 提供真实实现，或在不适合照搬时提供行为等价且安全边界更强的 d-token-native 实现；未完成的能力必须计入发布阻断，不能以“后续版本”或“接口已存在”视为完成。

### P0.2.1 统一状态与持久化契约

所有一级页、二级 UI、Topbar、StatusBar、操作弹窗和顶部通知必须消费统一的领域状态，不得各自在组件挂载时重复读取并形成互相覆盖的局部真相。每个数据切片至少包含：

```ts
type EvidenceSlice<T> = {
  data: T | null;
  source: "runtime" | "tauri" | "ledger" | "settings" | "derived";
  state: "loading" | "ready" | "partial" | "stale" | "unavailable" | "error";
  freshness: "live" | "recent" | "stale" | "unknown";
  collectedAt: string | null;
  revision?: string;
  error: string | null;
};
```

1. 统一状态至少覆盖 service、Agent detection、upstream discovery、connection plan/receipt、dashboard、traffic、routes、compression/accounting、Doctor、logs/events、transactions/rollback、appearance/settings 和 notifications。
2. 首次加载可以显示 skeleton；后续刷新必须保留最后一份可验证证据并更新 freshness，不得闪回“未启动”、清空列表或重复播放整页动画。
3. 服务、设置、配置事务、回退点和通知读取状态必须跨 App 重启恢复；请求、压缩、路由、Doctor 和日志状态从本地 runtime/Event Ledger 重建。不得只依赖 React 内存或当前页面生命周期。
4. start/stop、apply/rollback、Doctor 修复、设置保存、Agent/cc-switch 刷新和真实请求完成后必须触发对应切片 invalidate/refresh；所有页面在同一轮刷新中看到同一 revision/request id。
5. verified empty 与 unavailable 必须严格区分：真实 `[]` 可以显示 0；数据源未返回时显示 `—`/partial/unavailable。局部切片失败不得把在线服务降级为 stopped。
6. 同一事实只允许一个所有者：外观由唯一 AppearanceProvider，运行证据由 App 级 operational store，通知由唯一 NotificationStore，配置变更由 transaction ledger；页面组件只渲染和发起领域动作。
7. 所有持久化失败都必须产生顶部通知和可读错误状态；除非安全边界要求 fail closed，否则不得把用户当前会话中已生效的纯 UI 选择闪回旧值。

### P0.2.2 全 App 性能与响应性契约

性能是正式产品能力和发布门槛，不是功能完成后的可选优化。任何真实能力接入都必须同时评估冷启动、页面切换、状态刷新、Native IPC、文件/数据库读取、长列表、图表、动画和后台任务对用户响应性的影响。

1. 所有点击、页签切换和主操作必须在 100ms 内出现明确视觉反馈；按钮不得因为后台已有请求而静默无响应。
2. 已加载页面切换的内容稳定目标为 p95 ≤ 250ms；页面退出动画不得阻塞新页面挂载，不得用固定等待制造“按钮没反应”的感受。
3. release build 冷启动到 Shell 可交互目标为 p95 ≤ 1.5s，暖启动目标为 p95 ≤ 800ms。
4. 在线 runtime 摘要刷新目标为 p95 ≤ 250ms；本地服务离线、端口未监听或身份不符时必须快速失败，目标 p95 ≤ 500ms，不允许多个长超时串联。
5. App 采用 event-first、polling-fallback：服务、配置、route、Doctor、日志和智能学习状态优先通过事件失效对应数据切片；只保留一个 App 级低频兜底刷新，页面不得各自建立固定轮询。
6. Native 侧必须使用带 revision/freshness 的快照缓存；首屏只返回必要聚合摘要，完整日志、事件、route history、backup、Doctor detail 和智能学习候选按需分页或增量读取。
7. 文件扫描、CLI、SQLite、本地 HTTP 和其他阻塞操作必须进入受限 worker；用户操作优先级高于普通页面刷新，智能学习增量扫描优先级最低，并且必须支持取消和资源上限。
8. Frontend 必须按领域 slice/revision 细粒度订阅，避免一个大 payload 变化导致无关页面和状态栏整体重渲染；长日志、事件、Doctor 结果和智能学习候选超过阈值后必须分页或虚拟化。
9. 图表和大型依赖必须保持页面级按需加载；不可见页面不得持续执行动画、observer、tooltip、排序或图表绘制。窗口隐藏时停止非必要轮询和动画。
10. 正常页面交互不得出现超过 100ms 的 JavaScript long task；连续进度事件必须节流或批量合并，禁止每个文件、日志或候选触发一次 IPC 和 React 更新。
11. 窗口可见且无后台任务时平均 CPU 目标 ≤ 1%；30 分钟空闲与页面往返后内存不得持续单调增长，稳定区间漂移目标低于 5%。
12. 性能验收必须使用同一候选 commit 的 Tauri release build，记录平台、硬件、p50/p95、CPU、内存、IPC 次数、React commit 和 bundle manifest；浏览器开发模式或主观“感觉流畅”不能替代发布证据。

### P0.3 核心价值排序

第一优先级：稳定连接与一键配置。用户能把本地 AI Coding Agent 接入 d-token，并确认请求是否经过 d-token。

第二优先级：本地可控、安全、可回滚。任何真实写配置、改 hook、改 provider、改端口、改 cc-switch 连接方式之前，都必须展示人话摘要、详细 diff、备份位置、风险提示和恢复方式。

第三优先级：Token 节省与智能学习。压缩、学习、项目文档注入、全局 Agent 文档优化必须帮助用户减少重复上下文、提升 Agent 理解项目的能力，但不能默认暴露原始 prompt、原始代码、会话正文或密钥。

第四优先级：问题诊断与自动修复。Doctor 必须能检测已知问题，低风险问题可以自动修复，高风险问题必须预览变更并由用户确认。

### P0.4 Agent 支持策略

对外产品口径：d-token 支持多种 AI Coding Agent，只要该 Agent 能通过配置文件、环境变量、hook、代理端口或本地上游地址接入，就应进入 d-token 的统一接入模型。

第一版支持范围必须以“能力状态”展示，而不是简单宣传“全部已完成”。每个 Agent 至少有以下状态之一：

1. 已可一键配置。
2. 可检测，需用户确认配置。
3. 可生成提示词，由用户复制给自己的 Agent 执行配置。
4. 仅支持高级手动配置。
5. 暂未支持。

目标 Agent 包括但不限于：Claude Code、Claude Desktop、Codex、OpenCode、OpenClaw、Gemini CLI、Hermes、Cursor、Cline/Roo、Continue、Aider、Goose、OpenHands、Copilot CLI、Windsurf、Kilo Code、Google Antigravity、Mistral Vibe 等。

第一版必须重点验收的 Agent 是：Claude Code、Codex、OpenCode、Cursor、Cline、CommandCode。CommandCode 暂按用户指定名称进入目标清单；实现前需要补齐具体命令名、配置路径、hook 方式和上游配置格式。

OpenCode 仍然可以作为当前真实验收链路的优先入口，但 UI 信息架构不能以 OpenCode 为唯一中心。所有 Agent 必须复用同一套“检测 → 配置 → 预览 → 备份 → 应用 → 验证 → 日志/诊断/回退”的产品模型。

### P0.5 主页面信息架构

第一版主页面为 6 个主页面 + 底部设置入口：

1. 总体概览。
2. 安装与连接工作台。
3. 分析页面。
4. 视觉压缩（Beta）。
5. Doctor 问题诊断与修复。
6. 变更与回退。
7. 设置。

「视觉压缩（Beta）」是唯一允许带 Beta 徽标的一级实验能力入口；Advanced、Logs、Compatibility、Smart Learning 等仍只能作为工作台子页面或内部组件，不得再扩张为工程化顶级导航。Windows/macOS 共享同一导航顺序、中英语义与页面职责。

如果代码层仍保留 `Overview / Install / Advanced / Analytics / Doctor / Changes / LogsSecurity / Compatibility / Settings` 等旧页面名，UI 层必须按照本节重新组织中文显示名、页面职责和用户路径。旧页面可以作为实现组件来源，但不能继续以旧工程语义组织正式产品。

### P0.5.1 视觉压缩（Beta）页面合同

视觉压缩（Beta）是 Hybrid / Optical 的正式产品入口，信息架构对齐「安装与连接」：**一键启用 + 高级配置 + 状态与预览**。它是 Shadow 安装/配置与观测入口，不是工程 checklist 向导、开发测试中心或独立产品。

本产品所称“图片压缩/视觉压缩”特指 **代码与上下文 → Optical PNG → 多模态模型输入** 的上下文优化链。普通上传图片的 resize/re-encode/tile/ONNX 路由不是 d-token 产品目标，也不得作为发布阻断或另建一套图片文件压缩引擎。

页面必须回答：

1. d-token service、Agent transport、bridge 与 Claude Code/Codex/OpenCode 接入组件是否可用。
2. 能否对 Agent 规则、Hook/Plugin、Skill 和可选诊断 MCP 先预览逐文件 diff，再确认、验证与整笔回退。
3. 能否一键启用安全 Shadow 默认值（前置不足时明确 blocked，并指向安装与连接）。
4. 视觉压缩引擎是否已启用，effective mode 是什么。
5. active transform 当前是否可达（只读事实，默认应否）。
6. 高级配置下如何按 route 调整 Shadow 策略并安全应用。
7. capability / lifecycle 是否存在已验证签名证据。
8. 近期 shadow / plan / fallback 等 metadata-only 事件摘要，以及用户主动试跑的 shadow 规划结果。

#### 页内信息架构（安装 + 高级配置）

页面顶部使用上栏标签组织为 3 个区域：

1. **一键启用**：主路径。先提供独立的 Agent 接入工作台，再用安全默认值开启全局 Shadow 预览，并为现有 route 打开 tool_result 候选；两类事务各自预览 diff → 确认 → 写盘/backup/verify → 可撤销，不相互隐式开启或卸载。
2. **高级配置**：按需路径。编辑全局与 route 级 Shadow 策略（mode、tool_result/history 候选、有损确认），再走同一套预览/确认/应用/回退。
3. **状态与预览**：观测路径。status/capabilities/lifecycle/events 与可选 shadow preview；不承担安装主 CTA。

```text
理解 Beta 限制
→ 一键启用（安全默认 Shadow）或进入高级配置微调
→ 预览脱敏 diff + 风险摘要
→ 明确确认并应用（backup + before/after hash）
→ 重启 d-token-owned runtime 使配置生效
→ 在状态与预览验证 status/events 与可选 shadow preview
→ 可从本页或变更与回退恢复 backup
```

前置不足（无 active config / 无 route / 服务不可用）时，一键与高级均显示 blocked 说明，引导用户先完成「安装与连接」；不得用工程 checklist 作为主 UI。

Agent 接入合同：

1. 首批完整目标为 Claude Code、Codex、OpenCode；支持全局或明确绝对路径的项目作用域。
2. Transport 路由仍归「安装与连接」所有；Agent 接入事务不得读取或修改 Provider 凭据。
3. 静态规则、SessionStart Hook/OpenCode compaction Plugin 与恢复 Skill 默认勾选；metadata-only MCP 默认关闭。
4. Hook/Skill/MCP 只负责状态说明、诊断和恢复，不参与实际请求压缩；Hook 或 MCP 可用不得显示为 Active 已启用。
5. Codex Hook 写入后必须明确提示用户在 `/hooks` 中完成信任；不得绕过 hash trust。全局 Claude MCP 若需要读取或重写私有 client state 必须 fail closed，改用项目 `.mcp.json`。
6. 所有目标文件必须经过 before/after hash、redacted diff、明确确认、事务备份、写后校验和整笔回退；新会话要求必须可见。
7. Agent 接入、视觉 Shadow 与 production Active 是三个独立状态；关闭 Shadow 不删除 Agent 接入，卸载 Agent 接入不更改视觉策略或凭据。
8. Agent 接入必须以持久 transaction ID 绑定 review digest、目标顺序、planned-after hash、回退点和 verify/rollback 状态；App 重启后只能从该事务证据恢复，证据缺失或变化时不得继续显示“已验证”。
9. 打包的 Agent bridge 在执行任何 `identity`、Hook 或 MCP 子命令前必须先通过平台签名身份验证；bridge 只提供 metadata-only 状态，不读取 transcript/raw prompt，也不是压缩数据路径的授权条件。

产品可写字段白名单：

1. 全局：`optical.global_policy.enabled`、`default_mode`（仅 `off` / `shadow_preview`）。
2. Route：`route_policies[]` 的 `enabled`、`mode`（仅 `off` / `shadow_preview`）、`allow_tool_result`、`allow_history`、`confirmed_experimental_lossy`（有损确认，仍不打开 active）。
3. 写入时强制：`allow_active=false`、`public_summary_only=true`；未知 `route_id` 拒绝；不静默创建空生产 active config。
4. 禁止产品 UI 开启 production active，禁止把 `ToolResultsOnly` / `HistoryOnly` / `FullExperimental` 当作 active 生产路径暴露。
5. 写操作必须：用户可读摘要、redacted diff、backup、明确确认、verify、rollback evidence；配置生效依赖 managed service 重启读盘，不实现半热 reload。

数据与交互边界：

1. 真实数据只来自 d-token-owned local service 与桌面 control plane 对 active config 的事务；浏览器 preview 显示 unavailable，不得用 mock 数字补绿。
2. 导航项与页头必须始终可见 **Beta** 徽标；页内必须有一句限制说明：默认不改写真实请求、有损压缩、估算节省不等于账单。
3. 只展示 public summary 白名单字段；禁止回显用户粘贴的完整 body、PNG、base64、密钥、provider/route/model 敏感原值。
4. 不得提供开启 production active 的产品开关；不得宣称 verified savings。
5. 性能：页面 lazy 加载；仅在页面 active 时拉取 Optical 观测；配置 editor 不进入全局 dashboard 热路径；刷新失败保留上一份可验证证据。
6. UI 遵循 §12.5 现有纯色 shell，并复用安装工作台上栏标签语言；Windows/macOS 同一实现。1180×780 与 920×640、浅/深色、中/英、Reduce Motion 必须可读；缺 Windows 实机时不得把 macOS 单端截图写成双端完成。

### P0.6 第一页：总体概览

总体概览是用户打开 d-token 后看到的第一面。它是总体数据驾驶舱，不是功能入口页；不承担安装、配置、诊断或跨页面导航，只回答：

1. d-token 本地服务是否正在运行。
2. 已连接哪些 Agent。
3. 当前请求是否经过 d-token。
4. cc-switch 或自定义上游是否被检测到。
5. 今日/近 7 日估算节省了多少 token。
6. 最近是否有错误、断连、端口冲突、配置异常。
7. 当前状态对用户意味着什么。

空白状态必须是“空白状态 + 友好提示”，而不是冷错误。例如：

> d-token 还没有开始工作。启动本地服务后，你可以连接本机 AI 编码工具，并在这里看到连接状态、节省情况和最近问题。

概览不得放置“启动服务”“前往安装”“打开 Doctor”等功能入口或跨页面 CTA。无数据时只解释事实、影响和数据出现条件；真实操作留在对应一级页面。总体概览必须标注数据来源：真实数据来自 d-token-owned runtime、Tauri control plane 或 Event Ledger；正式 App 和浏览器预览均不得以示例数字、mock 曲线或假排行补齐暂无数据。

概览采用固定的数据阅读顺序：四格 KPI → 活动趋势 → 当前问题/异常 → 趋势或证据详情 → 运行概况。运行概况必须位于页面最底部，不得变成功能入口。活动趋势直接采用 AiMaMi 参考图中的紧凑 GitHub-style heatmap 语言，不得实现成占据整屏的大日历。时间范围默认“本年”，与 AiMaMi 参考界面一致，并提供“本周 / 本月 / 本年”三种不同的数据窗口与紧凑排布：

1. 本周：从周一开始的 7 个紧凑日格或短列，显示星期/日期标签，不扩张成七张大卡片。
2. 本月：按真实 5 或 6 周 × 7 日展示小方格，并保留必要的星期/月标签；相邻月份数据可弱化或省略，但 tooltip/ARIA 必须可核验日期。
3. 本年：按约 53 周 × 7 日展示连续小方格热力图，顶部标注月份，底部提供 Less/More 强度图例；不得拆成 12 张大月历卡。
4. 没有请求时仍显示完整时间范围的零强度方格和“暂无活动”；所有日期按用户本地时区构造，避免 UTC 偏移。
5. 每个数据格必须具备日期、活动等级、当天语义、tooltip 和可访问名称；状态不能只靠颜色表达，但不要求在每个小方格中永久印出日期数字。

概览与分析、Doctor、日志、变更和状态栏必须消费同一份运行证据快照。已验证的空请求列表是“真实零数据”，可以显示 `0`；数据源未返回、过期或不可用时显示 `—`、partial、stale 或 unavailable，二者不得混淆。

### P0.7 第二页：安装与连接工作台

安装与连接工作台是第一版最重要页面，承担“一键安装服务 + Agent 配置 + 智能学习 + 高级安装 + 日志”的核心任务。

该页面顶部使用上栏标签组织为 4 个区域：

1. 一键安装。
2. 智能学习。
3. 高级安装。
4. 日志。

#### P0.7.1 一键安装

一键安装区域负责把 d-token 从“未配置”推进到“真实可用”。普通用户只面对结果和必要决策，不需要依次理解本地服务、provider、route、model、cc-switch、协议和端口。正常路径最多要求用户处理一次歧义/凭据和一次最终确认。

用户侧核心流程：

```text
自动读取本机 Agent、现有上游、cc-switch 和 d-token 状态
  ↓
唯一高置信度方案：直接显示“已为你准备好”
  ↓
存在歧义或凭据缺口：只让用户选择候选或补充必要凭据
  ↓
展示人话连接摘要；技术路径、diff 和备份信息按需展开
  ↓
用户确认连接
  ↓
自动安装/启动 d-token runtime、应用配置并验证
  ↓
写入日志与回退点
```

内部仍必须完整执行环境检测、端口与进程身份检查、cc-switch/Agent 配置读取、连接计划、diff、backup、apply、verify 和 rollback，但这些步骤不得被拆成普通用户必须逐个完成的工程化向导。

一键安装主状态必须由单一状态机驱动，并且同一时刻只能有一个主 CTA：

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

1. `discovering` 自动开始，不要求用户先点击“检测”；手动刷新只是重新扫描。
2. `already-connected` 是一等状态：配置一致时不得重复写入、重启或生成无意义 diff，只显示当前连接路径和重新验证。
3. d-token runtime 安装/启动是“连接 Agent”的内部阶段；只有端口冲突、权限错误、版本不兼容或进程身份异常时才要求用户处理。
4. `needs-choice` 只展开存在歧义的 Agent/上游，其他已识别字段保持自动结果；`needs-credential` 只请求缺失凭据，不要求重复填写已识别 URL、协议和模型。
5. `verified` 必须来自配置复读、服务健康和真实 route evidence，不能仅凭写文件成功。
6. 多 Agent 同时配置时，runtime 事务与 Agent 配置批次分层处理；某个选中 Agent 失败时，默认回退本批次已经修改的 Agent 配置。runtime 已安全安装时可以保留，但 UI 必须明确说明“服务已安装，Agent 配置已回退”。
7. review 后任一源配置 hash 变化都必须使计划失效并重新扫描，防止并发修改被覆盖。

cc-switch 检测要求：

1. 自动定位 cc-switch 常见配置目录。
2. 列出检测到的所有配置文件，不显示密钥原文。
3. 自动识别每个配置对应的上游类型：官方上游、第三方中转、OpenAI-compatible、Anthropic-compatible、本地 provider、未知上游。
4. 检查 cc-switch 是否已经运行、需要启动、是否存在端口冲突。
5. 检查当前 Agent 是否需要先转发到 d-token，再由 d-token 转发到 cc-switch。
6. 检查是否需要端口转发、协议转换或 chain bridge。
7. 对无法确认的配置显示“需要用户确认”，不能静默猜测。
8. d-token 对 cc-switch 默认只读：只读取用户配置文件、profile、provider、upstream、端口和运行状态，不自动安装 cc-switch，不静默改写 cc-switch 配置。
9. 如果某个 Agent 当前不是官方上游配置，而是通过 cc-switch 或用户自定义配置走第三方上游，d-token 必须读取 cc-switch 中对应用户配置文件，并据此配置 d-token 的上游与连接路径。
10. 用户在 cc-switch 中切换配置/profile 时，d-token 必须能自动感知或在刷新后同步切换对应上游，避免 d-token 仍然指向旧上游。
11. cc-switch 自动同步是关键功能：UI 必须明确显示“当前 cc-switch profile / 当前上游 / d-token 当前连接路径 / 是否已同步”。
12. 上游选择默认来自自动发现：合并 Agent 自身配置、OpenCode provider/model catalog、cc-switch 只读配置和 d-token 已保存的非敏感连接资料；已有可发现候选时不得要求用户先手填 URL。
13. OpenCode 可以同时存在多个 provider/upstream；发现多个有效候选时必须完整列出来源、协议、模型和脱敏地址，让用户显式选择，禁止静默猜测或静默覆盖。
14. cc-switch 数据库与配置只读扫描必须版本化、字段白名单化并 fail closed；不得读取、返回或记录完整 profile blob、密钥、token、cookie 和私有 metadata。
15. 上游解析必须按 Agent 分别完成；Claude Code、Codex、OpenCode、Cursor、Cline 等可以保留各自原有 provider/model，不允许用单个全局模型覆盖所有 Agent。
16. 唯一且高置信度的当前上游必须自动解析并预选；只有多个同等候选、字段不足、凭据不可复用或格式未知时才要求用户介入。
17. cc-switch profile 变化后必须重新比较 d-token active route；不一致时显示 `out-of-sync`，禁止继续静默使用旧上游。

Agent 检测要求：

1. 检测本机已安装 Agent。
2. 检测每个 Agent 的配置文件位置、hook 能力、环境变量、可用命令、版本、当前上游。
3. 显示“可一键配置 / 可生成配置建议 / 仅高级手动配置 / 暂不可配置”。
4. 配置前必须备份，配置后必须验证。
5. 不允许在 UI 或日志中显示完整 API Key、认证 token、cookie、完整 prompt、完整 response。
6. 已识别 Agent 和当前上游必须先形成自动连接方案；高级安装不能成为可一键配置 Agent 的默认前置步骤。
7. 每个 Agent 的连接计划、验证结果和回退点必须独立可追踪，同时在多 Agent 批次中保持可解释的原子回退语义。

#### P0.7.2 智能学习

智能学习是 d-token 相比普通路由/配置工具的重要创新功能。它参考 Headroom 的项目理解、learn、prompt/document 注入思路，但必须形成 d-token 自己的产品形态。

智能学习目标：帮助 d-token 和用户的 Agent 更快理解项目、减少重复解释、自动生成或更新项目级/全局级 Agent 说明文档。

核心流程：

```text
用户进入智能学习上栏；有新鲜缓存则立即显示缓存，没有缓存则自动开始本地轻量扫描
  ↓
枚举全部受支持 Agent 的 session/history/task 根目录和所有可安全识别的会话记录
  ↓
只提取 Agent、时间、cwd/project/workspace、记录数量、格式版本等白名单 metadata，不返回会话正文
  ↓
解析 session -> project/workspace 关联，合并最近工作区、Git 项目、常用项目根和 d-token 历史
  ↓
轻量索引、去重、评分并流式展示候选和进度
  ↓
用户搜索、筛选、勾选需要重点理解的项目；额外文件夹只是补充入口
  ↓
仅对用户选择或明确授权的高价值项目做深度扫描
  ↓
生成项目理解、约定、架构摘要、常用命令、风险点、Agent 协作建议
  ↓
用户选择执行方式：直接委托 Claude/Codex 写入，或生成提示词交给自己的 Agent 执行
  ↓
直接写入项目或全局 Agent 文档的受控追加块 / 插入块，不重写、不删除、不改动文档原有内容
  ↓
完成后进入日志与回退记录
```

智能学习必须支持两种执行模式：

1. 直接委托模式：用户选择 Claude 或 Codex，由 d-token 注入提示词并委托其理解项目，随后只以受控追加块 / 插入块方式写入项目或全局文档。
2. 提示词模式：d-token 只生成高质量提示词，用户复制给自己的 Agent 执行配置或文档补充。
3. 无论哪种模式，智能学习都不允许重写整份文档，不允许删除用户原有内容，不允许改动无关段落。

智能学习进度条必须使用用户能理解的阶段：

1. 准备委托。
2. 正在识别文件夹。
3. 正在理解项目。
4. 正在生成建议。
5. 正在写入文档。
6. 正在验证结果。
7. 完成。

可写入的文档类型包括：

1. 项目级 `CLAUDE.md`。
2. 项目级 `AGENTS.md`。
3. 项目级 `GEMINI.md`。
4. 项目级 d-token 学习摘要。
5. 全局 Agent/Claude/Codex 说明文档。
6. d-token 自有项目策略文件。

智能学习安全边界：

1. 智能学习允许并且必须进行全面扫描：自动发现本机 Agent 会话目录、项目目录、最近活跃工作区和高价值上下文来源。
2. 全面扫描必须快速准确：先做轻量索引和置信度排序，再对高价值目录做深度识别，避免长时间无反馈。
3. UI 必须展示扫描进度、已识别目录数量、排序依据、可学习价值和用户可控的停止/跳过入口。
4. 扫描不等于上传：默认本地完成识别、摘要和索引，不默认上传完整代码、完整会话、密钥或认证文件。
5. 对 `.env`、密钥文件、认证目录、token、cookie、私密会话必须默认排除或脱敏。
6. 写入文档时可以直接写入，但只能插入或追加 d-token 管理的内容块，不允许删除、重排、覆盖、重写用户原有内容。
7. 每一次写入都必须生成备份和回退点，并写入变更与日志。
8. 智能学习不得在 App 冷启动时启动全量扫描；只在用户进入智能学习上栏后自动扫描，或按用户明确设置运行后台增量索引。
9. 有新鲜缓存时必须先展示缓存，再以低优先级增量刷新；未变化目录不得每次重新全量扫描。
10. 每个 Agent 来源必须显示 `scanned / missing / unsupported / error` 状态，并展示已枚举会话数、已解析项目数、跳过数和当前阶段。
11. 扫描任务必须可取消、可恢复、有限并发，并通过节流后的增量事件返回结果；大量会话或大目录不得阻塞 Shell、安装页和状态栏交互。
12. “全面扫描”指枚举所有可安全识别的会话记录并提取白名单 metadata，不等于默认读取、展示、缓存或上传完整 prompt、response、代码和命令输出。

#### P0.7.2.1 代码智能图谱（实验性 sidecar）

代码智能图谱通过独立一级页面「代码图谱（实验性）」提供可选本地工具接入，用于让已授权的 Agent 查询项目结构、调用路径与索引状态，并在满足运行证据时打开本地图谱网页。它不是 d-token 路由/压缩热路径，也不替代 Dashboard、Doctor 或 Event Ledger。

首期用户任务固定为：**打开固定官方下载页 → 下载并解压图谱组件 → 放入默认目录或选择自定义目录 → 自动识别与固定校验 → 自动扫描本机 Agent → 多选目标 Agent 与一个项目 → 预览批量接入 → 确认配置 → 由 Agent 启动 MCP 会话或由 d-token 单独启动图谱会话 → 查看分离状态 → 打开安全图谱页 → 停止 d-token-owned 会话、批量回退配置或删除索引**。工具名称、上游版本、许可证、二进制校验状态、项目范围、进程所有者和限制必须可见。

1. 代码智能图谱默认关闭，按显式用户授权启用；它是可选、实验性的外部本地 sidecar，不能成为 d-token runtime、路由、Dashboard 或发布启动所必需的依赖。
2. 页面进入时必须自动扫描本机 OpenCode、Codex 与 Claude Code，显示 `detected / missing / unsupported / error`，默认勾选全部已检测且受支持的 Agent，并允许多选、取消与重新扫描。「一键批量配置」必须为每个所选 Agent 分别生成脱敏 diff、backup、review digest、apply、复读 verify 与 receipt；任一目标失败时逆序回退本批次已写入目标，不得静默跳过。默认只添加最小 MCP entry；Hook 必须单独显示触发器、命令、匹配范围、Agent 信任/重启要求并二次确认，不得把不同 Agent 的 Hook 能力伪装成统一开关。
3. 所有 MCP、Hook、instruction 或项目配置写入必须复用 Agent Integration 的 `scan → diff → backup → confirm → atomic apply → parse/semantic verify → receipt → exact rollback` 事务。上游安装器和卸载器不得由 d-token 调用；未知、并发变化、验证失败或非受管目标必须 fail closed。
4. 首期发行选择是轻量分发：d-token App 只携带自有受限 launcher、管理 UI 与许可证/NOTICE，**不携带大型 headless upstream artifact**，也不代用户联网下载。页面只打开固定的官方 `v0.9.0` GitHub release；用户自行下载对应平台压缩包并解压到 App 展示的默认目录，或选择一个已存在的自定义绝对目录。App 必须自动识别目录根、`bin/` 或单层解压子目录中的目标文件，但只有固定 tag/commit、平台/arch、SHA-256 与允许的参数全部通过后才显示 verified 并开放配置、索引与图谱按钮。任意新版本必须先由 d-token 更新 manifest/allowlist；禁止信任任意 binary、调用上游 `install`/`update`/自更新或接受用户可控下载 URL。上游 UI variant 不进入正式 bundle。
5. 必须区分两类实例：`agent_mcp` 由选定 Agent 按 MCP stdio 配置启动，父进程和 stdio 归 Agent；`ui_host` 由 d-token 启动并拥有，用于受控索引和安全图谱页。两者不得用同一 PID、同一 stop 权限或一个泛化的 `running` 状态冒充；也不得在没有并发与锁证据时同时写同一缓存。`agent_mcp` 只有在专用 launcher 记录了 PID/birth token、exe hash、project/cache scope 和 MCP initialize/tool evidence 后才能显示 observed/verified，d-token 不得把 Agent-owned 子进程当成自己可停止的服务。launcher 必须执行版本化 MCP tool allowlist：只读工具可转发，索引必须绑定已授权项目并强制 `persistence=false`，cross-repo、ADR write、artifact export、任意路径覆盖与未知工具默认拒绝；请求/响应 payload 不得落日志或 receipt。
6. 上游的 MCP transport 是 stdio，原生 HTTP UI 生命周期也绑定该 MCP 事件循环；当前版本没有独立 daemon 或 generic `/health`。首期图形网页必须由 d-token-owned、带不可猜测会话授权且只读的 loopback host 提供，或等待上游提供等价的认证、allowed-root 与端点裁剪能力。不得直接发布当前原生 UI：其 `/api/browse` 可浏览项目根之外的目录，`/api/repo-info` 暴露根路径/remote，且存在 index、ADR、delete、RPC、logs 与 process-kill 等端点。打开前必须复检 host、端口所有权、实例身份和短期授权，只允许字面 `http://127.0.0.1:<managed-port>/`。
7. 索引与持续监测默认关闭。用户必须分别确认「立即索引」和「持续监测」；当前上游对 Git 工作区使用轮询监测，产品文案必须写作“Git 变更监测”，不得承诺文件系统级实时监控或非 Git 工作区持续监测。每个项目必须有可见的文件数、CPU/worker、内存、时间与缓存配额，并支持取消、停止与删除该项目索引。
8. 代码读取范围必须限制到用户选定且 canonicalized 的项目根；默认排除 `.env*`、凭据/auth 目录、密钥、私有恢复库、d-token state/config、生成物与缓存目录，以及任何无法安全解析的符号链接或权限边界。项目内 `.codebase-memory.json` 视为不可信输入，其扩展映射若扩大索引范围必须进入预览。首期强制 `persistence=false`，禁止创建或修改 `.codebase-memory/graph.db.zst`、`artifact.json` 和 `.gitattributes`；缓存必须按项目隔离并采用 OS 私有权限。
9. 状态必须至少分开显示：`package`（missing/verified/untrusted/incompatible）、`integration`（not_configured/review_ready/applied/verified/changed/partial/rolled_back）、`agent_session`（not_observed/starting/observed/verified/exited/stale）、`ui_session`（disabled/not_running/starting/ready/failed）和 `index`（none/indexing/current/stale/error）。每项都必须带 owner、source、observed_at、freshness 和 warnings；配置存在、Hook 存在、端口监听、索引存在和 Agent 实际调用是五类独立证据，不能互相推导。
10. 普通日志、Event Ledger、导出和 Dashboard 只能记录版本/hash、范围摘要、状态转换、资源桶、计数和事务 receipt，不得记录源码、snippet、完整路径/remote、ADR、MCP payload、原始 stderr 或图谱内容。`CBM_DIAGNOSTICS` 必须关闭，环境变量采用显式 allowlist；`CBM_ALLOWED_ROOT`、私有 `CBM_CACHE_DIR`、受限 `CBM_WORKERS` 和必要日志级别由 launcher 固定。
11. 「停止」只能停止具有完整 d-token 管理证据的 `ui_host` 或其他 d-token-owned 实例；Agent-owned `agent_mcp`、用户自行启动或已有的上游实例只读展示，禁止 kill/restart。Hook/配置回退、索引删除和缓存清理必须分别提供影响说明、确认、验证和 receipt；删除索引不得删除用户源码或非 d-token-owned 文件。

首期验收：在 macOS 与 Windows 各自的隔离测试配置中，证明安装包不含 upstream 大文件、默认/自定义目录导入可自动从 unavailable 变为 verified、坏 hash/平台/arch fail closed、全量 Agent 扫描与多选批量 MCP transaction、批次失败逆序回退、一个明确同意的 Hook transaction、`agent_mcp`/`ui_host` 所有权分离、项目范围与 repo-write 拒绝、安全网页授权与端点 allowlist、索引取消/删除和完整配置回退。真实用户 Claude/Codex 配置写入、原生 upstream UI 直出、常驻 daemon、跨项目图谱、ADR 写入与任意上游更新均不属于首期默认行为。

详细研究与实施分解见 `docs/research/CODEBASE_MEMORY_MCP_INTEGRATION_RESEARCH_2026-07-16.md` 与 `docs/PLAN.md` §1D。

#### P0.7.3 高级安装

高级安装面向熟悉 Agent、上游、端口和路由的用户。它不能替代一键安装，但必须允许用户完全自定义：

1. Agent 使用哪个上游。
2. Agent 是否先经过 d-token。
3. d-token 是否再转发到 cc-switch。
4. cc-switch 使用哪个 profile/config。
5. d-token 本地服务端口。
6. cc-switch 端口。
7. provider 协议：OpenAI-compatible、Anthropic-compatible、本地 provider、未知/自定义。
8. 是否启用端口转发或 chain bridge。
9. fallback 策略。
10. 超时、重试、健康检查、日志级别。

高级安装必须从自动发现和自动连接方案渐进展开，不能让用户从空白表单重新配置已经识别的内容。用户从“自定义此方案”进入时必须预填：Agent、当前配置来源、provider、协议、base URL、模型、凭据引用状态、d-token/cc-switch 路径、端口、chain mode 以及可靠性默认值。

1. 高级安装按“基础连接 → 网络与协议 → 可靠性 → 诊断”渐进组织，普通用户无需展开全部字段。
2. 自动模式和高级模式必须共享同一数据模型、校验器和事务引擎；高级模式只增加 override，不能复制第二套 route/provider 写入逻辑。
3. override 支持全局默认和 Agent 级覆盖；未覆盖字段继续继承自动发现结果。
4. 必须提供“恢复自动推荐”，它只清除 override，不删除或重写用户原有第三方配置。
5. 自定义方案仍必须通过 diff、backup、confirm、verify 和 rollback；源配置变化时必须重新审阅。
6. 高级安装必须自动显示检测到的端口、上游、配置文件和风险，但不能在用户未确认时自动修改。

#### P0.7.4 日志

安装与连接工作台内的日志上栏只展示与“安装、连接、配置、智能学习、高级安装”相关的过程日志，不替代全局日志系统。

日志必须包含：

1. 检测了什么。
2. 计划修改什么。
3. 备份到哪里。
4. 应用了什么配置。
5. 哪一步失败。
6. 失败原因和下一步建议。
7. 可复制给 Agent 的修复提示词。

日志必须脱敏，不显示完整密钥、token、cookie、认证头、完整 prompt、完整 response 或用户未授权展示的代码内容。

### P0.8 第三页：分析页面

分析页面负责展示 d-token 的价值结果，而不是工程指标堆砌。

核心信息：

1. 估算节省 token。
2. 请求数量。
3. 压缩前后对比。
4. 最近 7 日 / 30 日趋势。
5. Agent 使用分布。
6. provider / cc-switch / 上游分布。
7. 错误率与失败原因分布。
8. 高价值项目或高消耗项目。
9. 智能学习带来的项目文档覆盖情况。

当节省数据不是精确计费数据时，必须写“估算节省”，并说明基于本地请求记录、压缩记录和路由记录计算。

### P0.9 第五页：Doctor 问题诊断与修复

Doctor 是问题修复中心，不是单纯系统检查面板。

Doctor 必须检测已知问题，并给出：

1. 问题标题。
2. 严重程度。
3. 影响范围。
4. 可能原因。
5. 自动修复按钮或手动修复步骤。
6. 可复制给 Agent 的修复提示词。
7. 相关日志入口。
8. 相关回退点。

Doctor 顶部必须展示最后扫描时间、动态 freshness、覆盖范围、问题数和可修复数。主按钮固定为“自动扫描并修复”，次按钮为“仅扫描”；前者不代表静默写入，仍必须在写入前停在确认步骤。默认闭环为：

```text
显示上次诊断快照
→ 扫描
→ 生成候选修复
→ 展示摘要与脱敏逐文件 Diff
→ 用户一次确认
→ 顺序执行
→ 自动复检
→ 展示总回执与逐 Agent 回执
```

扫描、规划、等待确认、执行、验证、部分成功和失败必须使用不同阶段状态，不能用一个 loading 覆盖。通过项默认折叠；问题按核心服务、Agent 接入、路由与上游、上下文优化、安全与恢复、平台集成分组。页面下方使用任务化维护列表，单项展示标题、影响、当前状态、风险与一个主操作，不复制参考产品的代码、资产或品牌。

一次扫描只允许产生一个带 `schema / revision / source / scan_scope / collected_at / expires_at / data-source revisions / coverage` 的 `DoctorSnapshotV3`。freshness 必须按当前时间和 TTL 动态计算；旧 v2 快照可以兼容读取，但不得沿用持久化字面量 `fresh`。不得分别调用 Dashboard、readiness 聚合和路由验证后在前端拼成互相不同时点的“全绿”结果；无 Agent、无 runtime identity、配置证据不完整或数据过期时必须 fail closed。扫描只读取本机状态，不发起 Provider 计费请求，不读取凭据值，不修改 cc-switch；保存脱敏诊断快照不视为配置写入。

Doctor 预制模块目录至少包括：

1. 自动扫描并修复：扫描服务、Agent、cc-switch、Route、Hybrid、Optical Shadow、备份与平台集成，只选择证据明确的 d-token 管理漂移。
2. d-token 服务修复：只启动或重启身份完整验证的 d-token 服务。
3. 本地监听与临时状态修复：复检端口归属，只清理白名单内且满足 TTL 的 d-token 临时文件。
4. d-token 路由修复：重建或重载活动 Route，阻止 loopback 循环上游、失效 Provider 引用和运行时配置不一致。
5. 已配置 Agent 接入修复：只恢复已有 d-token 管理证据的目标，不承担首次安装。
6. Agent Hybrid 接入重新注入：作为受管组合恢复 Instructions、Hook/Plugin 和 Recovery Skill；不得宣称因此开启视觉压缩。
7. Agent Hook/Plugin 重新配置：Claude Code SessionStart Hook、Codex Hook 和 OpenCode Plugin 使用各自适配器，并展示信任与新会话要求。
8. Agent Instructions / Recovery Skill 修复：只恢复受管区块，保留用户非受管内容。
9. Agent metadata MCP 修复：高风险高级模块且默认不选；只提供 metadata 诊断，不声称进入压缩热路径。
10. Agent 连接配置修复：由动态 capability registry 决定；首期只有已验证 executor 的 Agent 可自动修复，其余明确锁定。
11. Optical Shadow 配置修复：只允许 `off/shadow_preview`，Doctor 永远不得开启 Optical Active。
12. 平台自启动修复：只操作 d-token 自有 LaunchAgent/HKCU Run 等托管项。
13. cc-switch 同步检查：只读发现 cc-switch，只修复 d-token 侧 Route，不写其 profile 或数据库。
14. 备份与回退链路复检：验证完整性、事务归属和当前可恢复性；损坏证据只报告并引导到“变更与回退”。

低风险候选可以默认勾选，但实际动作仍必须由用户点击确认。服务启停前必须重新验证 PID 出生标识、可执行文件路径/hash、端口所有权、精确 runtime identity/version/instance nonce；legacy manifest 或任一身份字段缺失时禁止自动重启。外部进程占用端口时必须保护性阻止，禁止自动停止或替换；仅跳转到其他页面不得标记为“已修复”。

Agent 类模块使用统一弹窗，流程为“选择 Agent → 选择作用域 → 审阅 → 执行结果”。Agent 与 executor 来自动态 registry；只允许选择已有 d-token 接入证据且该模块存在 executor 的 global/project 目标，用户勾选最终作用域，禁止遍历或修改无接入证据的任意项目。审阅必须按 Agent 分页展示异常、影响文件、脱敏逐文件 Diff、before/after SHA-256、备份/回退策略、服务重启、信任和新会话要求。

多选 Agent 后只确认一次，但按 Agent 顺序执行，避免文件锁与配置竞争；同一 Agent 的多个 surface 合并成一个事务。每个 Agent 必须拥有独立 plan、transaction、backup、verify、rollback 和 receipt；单个 Agent 失败后继续其他 Agent，成功 Agent 不自动回退。最终逐 Agent 显示 `verified / partial / failed / blocked`，并提供 Diff、日志和精确回退入口。

Doctor v3 的模块注册、扫描模型、修复规划和回执归 `dtoken-doctor` 所有；Tauri 只采集平台证据并调用现有 Service、Route、Agent Integration、Optical 执行器，禁止在 Doctor 内重写第二套配置 patcher。核心合同为 `DoctorSnapshotV3`、`DoctorRepairModule`、`DoctorRepairPlan` 和 `DoctorRepairReceipt`。桌面窄命令为 `get_doctor_snapshot`、`run_doctor_scan`、`get_doctor_repair_modules`、`preview_doctor_repair`、`apply_doctor_repair`、`verify_doctor_repair`；兼容期内部适配命令不得扩大前端写入权限。执行前必须重新校验 snapshot revision、plan digest、文件哈希、服务身份和短 TTL 确认凭证；前端不得提交任意路径、命令或配置正文。

Doctor 还必须能生成“修复提示词”，让用户复制给 Claude/Codex/OpenCode 等 Agent 协助修复项目配置或解释错误。

验收：无 Agent 时不得全绿；一次点击只生成一个统一快照；外部端口占用不得自动杀进程；修复必须经过预览、确认和自动复检；多 Agent 允许部分成功且逐 Agent 有独立回执/回退；Optical Active、cc-switch 写入、无管理证据 Agent 和无专用 executor 操作全部阻止；浏览器 preview、stale、partial 与无完整回执场景全部 fail closed。

### P0.10 第六页：变更与回退

变更与回退页面负责让用户知道 d-token 改过什么，并能安全恢复。

必须展示：

1. 变更时间。
2. 变更来源：一键安装、智能学习、高级安装、Doctor 修复、手动设置。
3. 影响对象：Agent 配置、cc-switch 配置、provider、端口、项目文档、全局文档、d-token 配置。
4. 人话摘要。
5. 详细 diff。
6. 备份位置。
7. 验证结果。
8. 一键恢复。
9. 恢复后的验证结果。

页面只保留两个顶级标签，并默认进入“备份与回滚”：

1. 备份与回滚。
2. 变更记录。

不再提供顶级“待批变更”和“配置 Diff”。未完成草稿由安装与连接、智能学习、视觉压缩或 Doctor 等来源流程负责审阅和确认；本页只给出条件提示并返回来源流程。Diff 必须保留在每次真实写入前的确认弹窗，以及已提交变更记录的详情中，并绑定权威的 before/after SHA-256，禁止使用前端临时拼接摘要冒充权威 Diff。变更记录只收录持久状态变更；服务启停、刷新和纯诊断进入 Event Ledger、Doctor 或日志，不作为配置变更。

备份状态必须明确区分：

1. `integrity_verified`：备份元数据与内容本身完整。
2. `currently_restorable`：当前目标哈希、配置归属、事务范围和运行前置条件仍允许恢复。
3. `blocked_reason`：损坏备份、并发修改、未知/旧 scope 或事务不完整时的保护性阻止原因。

“备份完整”不得自动显示成“现在可恢复”。用户点击回退前必须重新预检完整性、当前哈希和归属；确认后先为当前状态创建反向安全备份，再按整笔事务原子恢复，按需 reload/restart，执行文件与 runtime 专项复检，并把 rollback receipt 作为新的持久变更记录。反向安全备份必须绑定回退后的真实目标状态；原文件不存在时也必须显式绑定 `missing` 前置条件，不能生成一个看似存在但无法再次使用的恢复点。多文件 Agent 集成必须按 transaction 聚合为一个回退点，视觉压缩和其他专项事务必须标注 `executor_kind` 并调用各自事务级 rollback executor，不能把底层单文件 backup 平铺后绕过原子性与专项复检；尚未接通来源 executor 的 scope 必须锁定。

回退页面必须支持智能学习写入文档的回退，不能只支持 route/provider 配置回退。

验收：首次进入和遗留 session 值都落到“备份与回滚”；顶级待批/Diff 消失；损坏备份、unknown scope、无来源 executor 和并发哈希变化在预览阶段阻止；Agent 多文件事务只显示一个事务回退点并由 Agent Integration executor 整笔恢复，Optical 使用 Optical executor；成功恢复、文件恢复但 runtime 离线、失败/拒绝三种结果必须清楚区分并持久化回执；反向恢复点必须可通过同样的预览与并发保护再次使用。

### P0.11 第七页：设置页面

设置页面从底部按钮进入，负责偏好和安全，不作为高级配置大杂烩。

第一版设置包括：

1. 语言。
2. 主题。
3. 本地服务端口。
4. 数据保留时间。
5. 日志脱敏策略。
6. 智能学习全面扫描开关、扫描强度、索引频率和扫描范围管理。
7. 默认排除目录与敏感文件规则。
8. 自动启动 d-token 服务。
9. 更新通道。
10. 高级/开发模式开关。

### P0.12 文案总规范

所有页面必须通过三个问题验收：

1. 我现在在哪里。
2. 这个页面能帮我做什么。
3. 下一步我该点哪里。

禁用或减少以下面向普通用户的直接表达：

1. payload。
2. fixture。
3. benchmark。
4. smoke test。
5. runtime payload unavailable。
6. route adapter failed。
7. event ledger append failed。
8. headroom external dashboard。
9. fake binary。
10. test harness。

推荐替换为：

1. 本地服务暂时没有返回数据。
2. 还没有真实记录；连接本地服务并产生请求后，这里会显示数据。
3. 连接检查未通过。
4. 请求没有进入 d-token。
5. 本地操作记录写入失败。
6. 配置已备份，可以恢复。
7. 需要你确认上游地址。
8. 端口被占用，请换一个端口或关闭占用程序。
9. 已生成修复提示词，可以复制给你的 Agent。

### P0.12.1 发布界面文案治理

1. 中文和 English 必须表达同一用户任务；页面、空态、加载、失败、确认弹窗、通知、表单提示和 ARIA 文本均使用中央双语词典。
2. 采用“事实 → 影响 → 下一步”的短句；按钮使用“动词 + 对象”。不使用系统第一人称、拟人承诺、空泛鼓励、反问或营销式保证。
3. 正式界面不得出现 `审查 / 审阅 / 审核 / Review / Audit`，改用查看变更、确认、验证或需要处理等实际动作；不得出现 `测试 / Test / Testing / Fixture / Mock / Benchmark / Smoke / Sample / Demo`。真实产品含义的“预览 / preview”保留。
4. native summary、异常文本和开发措辞不得直接进入页面、通知或弹窗。已知结果映射为本地化操作说明；未知结果仅显示下一步和安全的状态/请求标识。原始诊断信息只保留在既有脱敏运行证据边界内。
5. 生产源由 AST 策略检查覆盖，禁止未经授权的可见直写文案、禁用词和原生错误直传；协议字段、命令名、内部 ID 与测试代码不受此限制。

### P0.13 从现在到发布的实现计划

#### 阶段 1：PRD 与信息架构落地

1. 按本节重构 PRD。
2. 固定主页面结构：总体概览、安装与连接工作台、代码图谱（实验性）、分析、视觉压缩（Beta）、Doctor、变更与回退、设置。
3. 明确第二页四个上栏：一键安装、智能学习、高级安装、日志。
4. 输出每页 UI 文案清单、空状态、错误状态、成功状态。

验收：产品语义清楚，不再以开发测试、benchmark、fixture 作为正式 UI 主流程。

#### 阶段 2：一键安装与 cc-switch / Agent 检测闭环

1. 实现 d-token 自有本地服务安装/启动/停止/状态检查。
2. 对第三方工具只做检测，不自动安装 Claude Code、Codex、OpenCode、Cursor、Cline、CommandCode 或 cc-switch。
3. 实现端口检测和端口冲突提示。
4. 实现 cc-switch 配置目录探测、配置文件列表、上游识别、端口识别、运行状态识别。
5. 实现 cc-switch 当前 profile/config 读取，并在用户切换 cc-switch 配置时自动同步 d-token 上游。
6. 实现 Agent 检测、配置能力分级、hook/配置文件/环境变量识别。
7. 实现配置计划、diff、备份、应用、验证、回退点写入。

验收：用户能在第二页完成 d-token 服务启动、cc-switch 只读检查、cc-switch 上游同步、Agent 检测和至少一种真实 Agent 配置闭环。

#### 阶段 3：智能学习可用

1. 实现 Agent 会话/项目工作目录全面扫描。
2. 实现快速索引、置信度评分、活跃度评分、会话价值评分和项目关联度排序。
3. 实现搜索、筛选、勾选重点文件夹，并允许用户停止、跳过或重新扫描。
4. 实现敏感文件排除、脱敏、扫描进度展示和本地索引缓存。
5. 实现学习结果生成。
6. 实现 Claude/Codex 委托模式。
7. 实现提示词模式。
8. 实现项目级和全局文档的直接写入，但只能追加或插入 d-token 管理块，不允许改动其他内容。
9. 实现所有写入的备份、回退和变更记录。
10. 实现进度条阶段：准备委托、识别文件夹、理解项目、生成建议、写入文档、验证结果、完成。

验收：系统能全面快速扫描真实项目与 Agent 会话目录，生成可用学习结果，并以受控追加/插入方式安全写入或生成提示词。

#### 阶段 4：分析、Doctor、日志、回退完善

1. 分析页接入真实请求、压缩、路由、错误、Agent 使用数据。
2. Doctor 接入已知问题库与自动修复动作。
3. Doctor 支持生成修复日志和 Agent 修复提示词。
4. 变更与回退支持所有配置变更和文档写入变更。
5. 全局日志脱敏、筛选、导出。

验收：服务异常、配置异常、端口冲突、cc-switch 异常、Agent 配置异常、智能学习写入异常均可诊断、解释、修复或回退。

#### 阶段 5：UI 完整度与稳定性

1. 全部页面中文文案重写。
2. 全部页面 loading / empty / error / success 状态补齐。
3. 所有危险操作加确认、diff、backup、rollback。
4. 所有真实数据标注来源。
5. 正式 App 与普通浏览器预览不加载示例数据；fixture/mock 只留在测试与隐藏 dev harness。
6. Windows/macOS 路径、端口、权限、编码、shell 差异测试。
7. 前端 build、后端测试、手工桌面验收、真实链路验收全部通过。

验收：所有功能可用、UI 完整、文案友好、功能稳定，可以进入公开试用发布。

## 0.0 当前最高优先级链路定义

d-token 的目标不是“外部 Headroom Dashboard 的套壳”，也不是“Desktop 直接读取一个已经运行的 Headroom 服务”。正确产品链路是：

```text
OpenCode / Claude Code / Codex / Cursor / Cline / other AI coding agent
↓
d-token local entry / managed wrap
↓
d-token 本地服务
= Headroom 能力移植、吸收、改造后的 d-token-owned runtime
= compression / CCR / memory / learn / output shaping / metrics / stats / history / dashboard primitives
= integrated RTK internal capability
↓
d-token route adapter
↓
官方上游 / 第三方中转上游 / cc-switch route interface / local provider
↓
d-token Event Ledger / Dashboard / Doctor / Logs / App
```

本链路具有最高解释权：

1. `D:\D-TOKEN\headroom` 是源码参考、能力迁移来源和兼容行为参考，不是 d-token App 的运行时数据源。
2. 已存在的外部 Headroom 服务，例如 `http://127.0.0.1:8787/dashboard`，只能作为迁移对照和 no-touch 只读参考；不能作为成品 App Dashboard 的依赖。
3. d-token 必须拥有自己的本地服务、端口、stats/history/metrics、Event Ledger、ProviderProfile、RouteProfile、InstanceProfile 和 Dashboard 数据模型。
4. Headroom Dashboard 的所有数据与页面能力要迁移到 d-token App，但数据应来自 d-token-owned runtime，而不是直接轮询外部 Headroom Dashboard。
5. OpenCode 是第一条真实测试入口：测试请求必须从 OpenCode 打到 d-token，再由 d-token 完成压缩、路由和上游转发；发布验收优先使用 OpenCode 官方目录中当时可用的免费模型，并在执行前显式展示所选 provider/model，不消耗或静默切换用户的付费上游。
6. 正确测试链路是 `OpenCode -> d-token local service -> d-token Headroom-derived runtime -> d-token route adapter -> official/custom/cc-switch upstream`。
7. 任何实现如果把链路写成 `App -> external Headroom 8787 -> Dashboard data`，都是错误方向。
8. 开发期可用 external Headroom 做只读对照，但不得启动、停止、重启、改配置、占端口或依赖其作为产品主路径。
9. OpenCode 真实测试使用本机正常 OpenCode 环境和真实配置文件位置，不用 fake HOME 或 fake config；测试候选由自动发现结果显式选取，发布验收默认选择 OpenCode 官方免费 provider/model。测试目标是证明真实 OpenCode 流量经过 d-token local service，而不是证明 OpenCode 可直连官方模型。
10. OpenCode 是当前阶段唯一允许使用真实用户 agent 配置进行测试的入口；该例外不得扩展到 Claude Code、Codex、Cursor、Cline、Roo、Continue、Windsurf、系统代理或 shell profile。
11. 若为了验证真实 routed path 需要给 OpenCode 配置增加 d-token provider/route，必须先备份原配置，只做最小变更，记录 backup 路径和 diff 摘要；禁止读取、打印、持久化或提交 OpenCode 密钥和会话凭据。
12. 开发测试不能产品化成正式 UI：真实测试、workload benchmark、smoke test、fixture/mock/fake-bin 结果属于开发脚本、CI gate、测试报告和内部诊断证据，不能作为最终用户主界面里的“测试卡片”或“测试中心”长期保留。正式 App UI 只面向用户任务：安装/启用 d-token 服务、选择 agent、配置 provider/route/cc-switch、查看运行状态、Doctor 诊断修复、Dashboard/Logs/Export。
13. 最终 App 必须兼容所有目标 agent；OpenCode 只是当前第一条真实验收入口，不代表产品只服务 OpenCode，也不能把 OpenCode 测试逻辑硬编码成最终信息架构。当前阶段的真实验收口径应是：安装/启动 d-token-owned 服务后，在正式安装/配置流程中选择 OpenCode，点击 OpenCode 的安装/配置/连接动作，随后由开发侧脚本或 CI 证明请求确实经过 d-token。
14. 任何 UI、安装流、Dashboard、Doctor 或测试入口设计前，必须先按产品经理视角判断用户任务：用户是在“安装/连接/配置/诊断/查看状态”，不是在“跑测试”。只有完成这个产品语义翻译后，才能落到代码实现。
15. 新增或重做 App 流程时，必须先参考成熟桌面控制面产品的应用逻辑，例如连接状态、配置入口、诊断分区、日志导出、备份回滚，而不是把开发脚本、benchmark 或 smoke test 直接搬进正式 UI。
16. 浏览器、桌面窗口或截图验收只能证明界面显示正常；不得用它替代真实功能接入证明。真实功能接入必须同时看到 d-token-owned local service、route adapter、Event Ledger、metrics/export 和 OpenCode routed flow 等证据。

### 0.0.1 当前实现契约

1. d-token 本地服务默认绑定 `127.0.0.1`，当前开发端口以 d-token 自有端口为准，不能占用或依赖外部 Headroom `8787`。
2. App Dashboard、Doctor、Logs、Analytics 读取 d-token 自己的 API / Tauri command / Event Ledger；禁止把外部 Headroom Dashboard JSON 当作产品数据源。
3. Headroom 迁移工作以完整行为能力为准：proxy/stat/history/dashboard primitive、compression pipeline、CCR、memory/SharedContext、learn、output shaping、MCP、agent integration 与 integrated RTK 均属于当前公开试用候选的发布门槛。迁移可以分批实施，但任何 metadata-only、透传或只读对照层都必须明确标为未完成。
4. Route adapter 必须同时面向官方上游、第三方 OpenAI-compatible / Anthropic-compatible 上游、cc-switch route interface、本地 provider。
5. 所有 route decision 必须写入 Event Ledger，至少包含 agent、route、provider、input protocol、output protocol、chain mode、fallback policy、latency、token in/out、compression result。
6. OpenCode 测试必须证明真实请求经过 d-token local service，不允许只证明 OpenCode CLI 自己能请求官方模型。
7. 测试报告必须包含功能测试、路由测试、压缩率、压缩耗时、端到端延迟、错误分类、日志脱敏、Dashboard 数据一致性。
8. 当前 d-token runtime 必须把结构化运行数据写入 d-token-owned persistence：`events.jsonl`、`route-decisions.jsonl`、`request-logs.jsonl`。这些文件只允许保存聚合/路由/计量/错误信息，不保存 raw prompt、raw response、CCR 原文或真实密钥。
9. 当前 d-token runtime 必须支持 Anthropic-shaped `/v1/messages` 输入显式桥接到已配置的 OpenAI-compatible / cc-switch-style provider；禁止在 route 缺失时自动回退到官方 Anthropic。
10. d-token runtime 必须提供真实可逆 CCR：压缩时生成不可猜测 recovery reference，原文只进入有 TTL、容量上限、加密静态存储与精确删除语义的本地隔离区；普通日志、Dashboard、Event Ledger 与 export 仍只保存 metadata。默认关闭原文存储；启用后 retrieval 必须经过本机权限边界、引用校验、审计和过期检查，禁止把 CCR 原文混入普通 API、错误或导出。
11. d-token runtime 必须提供真实 local/cross-agent/per-project memory、SharedContext、learn 与 output shaping。memory 只保存用户允许的结构化/脱敏学习内容与 provenance，不把整段 raw prompt/response 当记忆；learn 必须支持 preview、managed block、backup、verify、rollback；output shaping 必须实际参与 route-resolved 请求，并将 measured、holdout-estimated 与 unknown 严格区分。
12. 当前 OpenCode 验证以真实 routed path 为主：`OpenCode 正常配置位置 -> 经用户确认的 d-token provider/baseURL 最小变更 -> d-token :18001 -> 用户显式选择的官方免费或已发现 upstream`。OpenCode routed test 不使用隔离 HOME、fake config 或 fake provider；任何临时路由都必须走产品的 preview/confirm/apply/verify/rollback 事务。早期 direct command / fake-bin 测试只能作为诊断辅助，不能作为真实接入完成证据。
13. 当前 UI 实现契约：安装页可以有 OpenCode 的正式安装/配置入口，但不得以“运行真实链路测试”“workload benchmark”“能力自检测试”作为面向最终用户的主操作。开发验收可以通过隐藏 dev mode、脚本、CI 或测试报告触发同一后端能力；正式 UI 文案必须转换为用户可理解的“连接 OpenCode”“配置路由”“验证配置”“查看诊断结果”。
14. 当前前端所有真实运行数据来源必须指向 d-token 自己的服务或 Tauri command 汇总结果；普通 browser preview 使用诚实空态，不加载 Fixture/mock。Fixture/mock 与外部 Headroom 对照只能存在于 `support/`、自动化测试、隐藏 dev harness 或报告，不能进入生产 UI，也不能成为“已接入真实能力”的判定依据。
15. 当前 OpenCode 安装/配置验收必须模拟真实用户路径：选择 OpenCode agent，应用经过备份的最小配置 diff，验证请求进入 d-token Event Ledger，再回到 Dashboard/Logs/Doctor 展示脱敏状态。不得把该验收设计成面向最终用户的“OpenCode 测试页”。

### 0.0.2 Hybrid Context Optimization Engine 后端合同

Hybrid Context Optimization Engine 是 d-token-owned runtime 的协议安全优化层，在 passthrough、受保护的文本/语义压缩与 Optical PNG 压缩之间做可解释选择。后端能力已实现；正式桌面 UI 允许接入 **视觉压缩（Beta）** 观测页与 shadow preview，Windows/macOS 共享同一 React 页与 Tauri command 合同。Beta 页只消费 metadata-only 控制面与 public summary，不得默认开启 production active，不得渲染 PNG/base64/raw prompt，不得把 estimator 标为已验证账单节省。

这里的 Optical PNG 是代码/上下文的多模态表示，不是对用户上传图片文件做通用压缩；后者属于明确不在范围的能力。

1. 所有 codec 必须先经过协议 Context IR。IR 必须标记 system/developer、当前轮、近期历史、开放工具状态、tool schema/arguments、代码/diff、secret/exact、既有图片、未知 block 与候选内容；未知或无法证明安全的节点默认 `PreserveText`。禁止继续递归改写请求中的任意 JSON 字符串。
2. route/provider/endpoint/model/API version/capability 必须来自已解析的真实 outbound RouteDecision，不能信任请求体或 preview API 自报字段作为 active 授权。
3. 第一批 Optical active 只允许 exact Anthropic `/v1/messages` 同协议上游、已闭合且已成为历史的低风险 tool result。system/tool schema/current/recent/open tool state/code/diff/secret/exact/opaque 内容不得只存在于图像。OpenAI Chat/Responses 在各自 inventory、patcher、协议 fixture 与模型 eval 完成前只能 passthrough/shadow。
4. active capability 默认空且关闭。启用记录必须绑定 exact provider/route/model revision、API contract、renderer/font/profile hash、版本化视觉 token 公式、eval artifact digest、签发/验证/过期时间与可信 authority；wildcard 和未知模型一律拒绝 active。UI 不得提供一键开启 production active 的产品开关。
5. 规划、渲染、patch 与 post-validation 必须是不可越级的类型化状态链。每个 patch 必须带稳定 node/path、node kind 与 precondition digest；post-validator 必须从真实 PNG、patch set、原/新 IR 与协议 envelope 推导证据，不能接受调用方自报的安全布尔值。
6. renderer 必须确定性、跨 Windows/macOS 可复现，并在 patch 前自解码验证 PNG。首版执行器必须同时实现 deadline、取消、并发背压以及 node/anchor/page/像素/PNG bytes/base64 bytes/outbound bytes/峰值内存硬上限；超限或不确定时保留原文本，Optical 不与 truncation/summary 叠加。
7. shadow 必须保留 raw ingress bytes。active 在上游 send commit 前的任何 plan/render/patch/validation/Ledger 失败，只能丢弃变换并发送一次未优化的正确 outbound baseline；请求可能已写入上游、收到响应或开始 streaming 后禁止自动原文重试。
8. cache planner 必须显式建模 `tools -> system -> messages` 稳定前缀、automatic/explicit cache、最多四个 breakpoint、20-block lookback、TTL 和首次 text/image 切换 burn。冻结 section 的 PNG/base64/capsule bytes 必须 append-stable；profile/font/capability 变化使旧 cache evidence 失效。
9. 计量必须区分 estimated/actual、text/vision/cache write/cache read/output/encoded bytes/latency，并支持 signed delta。shadow、fallback、provider failure 不得计入 actual savings；第三方 benchmark、base64 字符数或旧 `char/4` 估算不能标为真实账单节省。Beta UI 中所有 `net_saved_tokens` 与 economics 摘要必须标注「估算」。
10. 同一 request id 必须关联 plan/render/patch/fallback/send-commit/provider outcome 的类型化 Event Ledger、最终 RequestLog、metrics 与 Doctor 状态。任何持久化、日志、错误、导出、调试元数据和 Beta UI 都不得包含 raw prompt/response、PNG、base64、secret、cookie 或 auth；active 审计无法持久化时在 send commit 前回退 passthrough。
11. active send path 在确定性协议/资源/隐私/回退测试和目标 provider/model 的配对质量、成本、延迟 eval 全部通过前必须不可达；使用默认关闭 feature gate、空可信 registry 与 sealed `ReadyToSend`（或等价不可伪造状态）共同保证。
12. 桌面 **视觉压缩（Beta）** 页允许通过 Tauri 代理读取 `/compression/optical/status|capabilities|capabilities/lifecycle|events/recent` 与 `POST /compression/optical/preview` 的 public summary。数据仅在用户打开该页时按需拉取，不得并入全局 dashboard 强制轮询；浏览器 preview 必须 fail-closed 为 unavailable。

状态：Living Document  
唯一规则：本文件是 d-token 的唯一 PRD。后续任何产品目标、边界、技术路线、目录结构调整，都直接修改本文件，不再另起 v0.1/v0.2/v0.3 多版本 PRD。

## 0. 当前基线决策：d-token App + Headroom-derived Runtime + Integrated RTK

### 0.1 核心判断

`headroomlabs-ai/headroom` 已经包含并管理 RTK：Headroom wrap 链路会确保 RTK binary、注入 RTK instructions、启动 proxy 与 agent，并通过 `rtk gain --format json` 汇总 RTK 指标。因此 d-token 不再把 RTK 与 Headroom 视为两个默认平级、默认独立安装的外部系统。

新的产品基线是：

```text
当前公开试用候选：d-token = 完整 Headroom 行为能力的 d-token-owned Rust runtime + 可视化与路由增强
后续版本：在保持能力与兼容性不缩水的前提下继续创新和替换内部实现

Headroom-derived runtime = compression core + proxy / SDK / MCP + agent wrap / unwrap + integrated RTK
d-token App              = Windows/macOS 图形化安装器、控制面、Provider/Route/cc-switch 编排、Doctor、Dashboard、可观测与回滚层
RTK                      = runtime 内部命令执行层能力，不作为用户侧独立产品路径暴露
```

### 0.2 最终用户安装模型

最终用户不应被要求先手动安装 Headroom 或 RTK。目标体验是：

```text
下载 d-token App
  ↓
打开图形化安装向导
  ↓
选择 agent / provider / cc-switch / 压缩策略
  ↓
d-token 自动安装或启用内置 runtime
  ↓
启动服务并在 Dashboard 看到链路、压缩、路由、错误和成本
```

开发期仍允许连接外部 Headroom 做 no-touch 验证；但成品视角下，Headroom 能力应被打包、吸收、修改并作为 d-token runtime 的组成部分提供。

### 0.3 不再保留 Standalone RTK 产品路径

1. d-token 不提供用户侧 Standalone RTK 模式。
2. d-token 不默认单独安装 RTK。
3. d-token 不要求用户理解或选择 RTK。
4. RTK 只作为 Headroom-derived runtime 内部能力出现。
5. Doctor 可以检测 standalone RTK 冲突，但只是为了避免重复 hook / PATH 污染，不作为推荐路径。
6. d-token 不把 RTK 放进 proxy hot path 当作 tool-result compressor。

### 0.4 产品创新点

1. 把 Headroom 的命令行安装、wrap、proxy、dashboard、doctor、learn、RTK 状态、provider 配置，做成 Windows/macOS 可视化 App。
2. 用户可以图形化选择安装/接入哪些 agent：Claude Code、Codex、Cursor、Cline、OpenCode、Aider、Continue、Goose、OpenHands、Copilot CLI 等。
3. 用户可以图形化选择官方上游、自定义第三方 OpenAI-compatible / Anthropic-compatible 上游、cc-switch / chain bridge 路由。
4. 支持直接压缩后发到自定义 URL：官方 URL、第三方中转 URL、本地 provider URL。
5. 支持需要二次路由的场景：先压缩，再转发到 cc-switch，再由 cc-switch 路由到最终 provider。
6. 典型场景：Claude Code 中使用 OpenCode Go 订阅，需要 cc-switch 路由；d-token 负责先压缩、再转发一次到 cc-switch。
7. 支持 Claude Code/Codex 等 agent 的协议差异、provider 差异、路由差异可视化，避免请求误走官方上游。
8. 支持中文界面和中文日志场景，Windows 上 UTF-8、盘符路径、项目名连字符、`.cmd` shim 等必须作为一等兼容目标。
9. 当前候选同步完成可视化、路由与 compression/runtime 全能力吸收；后续创新只能增强或替换内部实现，不能把未迁移能力推迟成默认产品缺口。

## 1. 项目一句话

`d-token` 是一个面向 AI Coding 的本地优先桌面控制面与运行时：在 Windows/macOS 上把 Headroom-derived compression runtime、integrated RTK、agent wrap、provider route、cc-switch / chain bridge、project compression policy、Doctor、Dashboard、backup/rollback 与端到端 token savings attribution 合并成一个用户下载即用、可安装、可观测、可回滚的图形化 App。

## 2. 用户目标

目标用户包括：

1. 使用 Claude Code、Codex、Cursor、Cline、OpenCode 等 AI Coding 工具的开发者。
2. 希望减少 token 浪费、压缩日志/文件/tool output/命令输出的重度 AI Coding 用户。
3. 同时使用官方 provider 与第三方中转 provider 的用户。
4. 需要 cc-switch 或多段路由的用户。
5. 想使用 OpenCode Go、第三方中转、自定义 URL，但又需要 Claude/Codex 等 agent 兼容的人。
6. 不想手动配置 Headroom/RTK/hook/base URL/proxy/dashboard 的普通用户。
7. 在 Windows 上使用 Claude/Codex/Headroom/RTK 时遇到编码、路径、端口、shim 问题的用户。

用户最终应能完成：

```text
打开 d-token 桌面 App
  ↓
选择傻瓜模式或高级模式
  ↓
选择 agent
  ↓
选择压缩能力与项目策略
  ↓
选择 provider：官方 / 第三方自定义 URL / cc-switch / chain bridge
  ↓
预览 diff / 风险 / 回滚点
  ↓
一键安装或启用内置 runtime
  ↓
启动服务
  ↓
在 Dashboard 看到请求、压缩、RTK、路由、成本、错误和恢复建议
```

## 3. 开发期 no-touch 与成品 managed install

### 3.1 两条线必须分开

```text
开发测试线：No-touch Dev Mode
  目标：保护开发者本机已经部署好、正在使用的 agents / Headroom / RTK / cc-switch
  原则：不调用真实工具、不读写真实配置、不改 hook、不占真实端口、不启动/停止真实服务

成品安装线：Production Managed Install
  目标：让最终用户一键安装 / 接入 / 配置 / 升级 / 卸载 d-token App 与内置 runtime
  原则：可以写配置、装工具、改 hook、启动服务，但必须有事务、备份、校验、回滚、可解释日志

开发验收线与成品 UI 线必须分离：开发者需要的真实测试按钮、benchmark、mock/fake/fixture 入口只能存在于开发脚本、CI、隐藏开发工具或测试报告中；用户下载后的正式 App 不展示测试语义，只展示安装、连接、配置、状态、诊断和修复。
```

### 3.2 开发期禁止行为

开发期默认禁止：

1. 重新安装 Headroom。
2. 重新安装 RTK。
3. 修改已有 agent hook。
4. 修改 shell profile。
5. 修改 RTK config。
6. 修改 Headroom config。
7. 修改 cc-switch config。
8. 占用真实 proxy 端口。
9. 启动、停止、重启真实服务。
10. 读取真实 prompt、代码、CCR 原文、tee 文件、session 原文。

开发期默认必须通过 fixture、fake binary、临时 HOME、临时 PATH、临时端口、mock provider、sandbox config 来验证 d-token 自身逻辑。

OpenCode Real Route Track 是当前用户明确授权的例外：为了验证真实链路，OpenCode 测试可以使用用户正常 OpenCode 环境和真实配置文件位置，不强制隔离 HOME，也不要求先造 fake config；provider/model 必须来自自动发现并由用户显式选择，发布验收默认使用 OpenCode 官方免费模型。该例外只适用于 OpenCode routed / streaming / workload test，不得扩展到 Claude Code、Codex、Cursor、Cline、外部 Headroom、RTK、cc-switch、系统代理或 shell profile。

OpenCode 真实测试的默认动作是“在正常配置位置通过受控事务接入 d-token，再用显式选择的官方免费候选跑通链路”。只有在需要把 OpenCode provider/baseURL 指向 d-token 时，才允许对 OpenCode 配置做最小变更；变更前必须备份，报告中只记录 backup 路径和脱敏 diff 摘要，验证后必须逐字节恢复，禁止读取、打印、保存或提交密钥、token、cookie、auth 文件、完整 prompt/response。

OpenCode 真实测试对应的产品动作不是“测试 OpenCode”，而是“在安装向导中接入 OpenCode”：启动/安装 d-token-owned 服务，选择 OpenCode agent，生成并预览配置 diff，备份 OpenCode 配置，应用最小配置变更，验证配置生效，回到 Dashboard 显示真实路由与压缩数据。测试脚本只负责在幕后证明该产品动作有效。

### 3.3 成品安装事务

所有真实写操作必须进入 InstallationTransaction：

```text
scan
  ↓
plan
  ↓
show diff
  ↓
backup
  ↓
confirm
  ↓
apply
  ↓
verify
  ↓
doctor
  ↓
commit rollback point
```

失败时必须：

```text
stop only d-token-owned new processes
  ↓
verify process identity before killing anything
  ↓
restore backups
  ↓
remove partial files
  ↓
report exact failure and recovery action
```

## 4. 产品分层与 Ownership

### 4.1 Headroom-derived runtime owns

d-token 将直接吸收和修改 Headroom 能力，形成 Headroom-derived runtime。该 runtime 负责：

1. Context compression pipeline。
2. ContentRouter / CacheAligner / SmartCrusher / CodeCompressor / Kompress text compressor。
3. CCR reversible compression 与 local retrieval。
4. Local proxy / SDK / MCP server。
5. Agent wrap / unwrap。
6. memory / SharedContext 的请求内授权接口与 provenance 记录；跨 Agent 复用的 Context Bus 由独立的 LeanCTX 对齐层负责。
7. learn / verbosity / output shaper。
8. Metrics、stats、history、dashboard primitives。
9. Integrated RTK install / hook / wrap-side metrics bridge。
10. Windows/macOS 兼容性补丁：UTF-8、路径、shim、端口、进程身份。

### 4.1.1 LeanCTX / Context Bus-derived owns

LeanCTX 对齐层不是外部 Python sidecar，也不是 Headroom proxy 的隐式依赖。它必须由 d-token-owned service 以独立 capability 和证据域提供：

1. Context Bus、Session Cache、Context Relay 和 Query-aware Retrieval；
2. map、signatures、code map、脱敏摘要和已验证结论的项目级复用；
3. 用户、项目、Agent、Session scope、freshness、provenance、deduplication 和失效；
4. Recovery Pointer/Manifest、Stale/Superseded Context 和文件变化后的安全拒绝；
5. 跨 Agent 命中与压缩收益分离计量，Relay 失败回退正常读取；
6. export/delete、TTL/quota、崩溃恢复和授权审计。

该层的上游来源、许可证、参考 commit、修改边界和行为差异必须独立登记。没有真实跨 Agent 证据时，只能显示 `partial`/`unavailable`，不得因为 Headroom codec 或 CCR 通过而自动变绿。

### 4.2 Integrated RTK owns

RTK 作为 runtime 内部能力负责：

1. command rewrite。
2. command execution shim / hook。
3. command-specific filtering。
4. raw-output tee recovery。
5. command-output savings analytics。
6. missed savings opportunities。
7. session adoption statistics。

RTK 不作为用户可选的独立产品路径出现。

### 4.3 d-token App owns

1. Windows/macOS desktop app。
2. 图形化安装向导与工具选择。
3. 傻瓜模式与高级模式。
4. 内置 runtime 的安装、启用、升级、停止、卸载、回滚。
5. ProviderProfile / RouteProfile / InstanceProfile / ProjectProfile。
6. 官方 provider、第三方 provider、自定义 URL、cc-switch、chain bridge 路由。
7. Anthropic-compatible 与 OpenAI-compatible 协议注入、转换与审计。
8. ProjectCompressionPolicy 与推荐理由。
9. Dashboard、Doctor、Logs、Export、Backup、Rollback。
10. Event Ledger 与 layered savings attribution normalization。
11. Windows/macOS 兼容性诊断与修复提示。
12. 中文 / 英文 i18n。
13. 安全边界：本地优先、默认 127.0.0.1、密钥脱敏、无静默 fallback。

## 5. 最终产品形态

### 5.1 直接压缩到官方或第三方自定义 URL

```text
AI Coding Tool
  ↓
d-token managed agent wrap
  ↓
Integrated RTK command rewrite / shell output reduction
  ↓
d-token Headroom-derived compression runtime
  ↓
d-token route adapter
  ↓
Official Provider / Custom OpenAI-compatible URL / Custom Anthropic-compatible URL
  ↓
Final upstream provider
  ↓
d-token Event Ledger / Dashboard / Doctor / GUI
```

适用场景：

1. 官方 OpenAI / Anthropic。
2. 第三方 OpenAI-compatible 中转。
3. 第三方 Anthropic-compatible 中转。
4. 本地模型服务。
5. 用户自己提供的 base URL。

### 5.2 先压缩，再转发到 cc-switch

当用户在 Claude Code 等 Anthropic-shaped agent 中使用 OpenAI-format URL、OpenCode Go 订阅、cc-switch 或多上游中转时，d-token 必须支持二次转发：

```text
Claude Code / Anthropic-compatible client
  ↓
d-token local entry
  ↓
d-token Headroom-derived compression runtime
  ↓
d-token protocol/route adapter
  ↓
cc-switch OpenAI-compatible or Anthropic-compatible route
  ↓
Final upstream provider
```

要求：

1. 先压缩，再转发。
2. route decision 必须记录 input protocol、output protocol、target provider、cc-switch endpoint。
3. 不允许静默从第三方 provider fallback 到官方 Anthropic/OpenAI。
4. Provider 401 必须显示为 route/provider mismatch，而不是泛化为 token 错误。
5. Header stripping、Authorization 选择、base URL 选择必须可审计。
6. GUI 必须展示“当前请求实际会去哪里”。
7. 用户不需要理解底层两次转发，只需要在图形界面选择“使用 cc-switch 路由”。

### 5.3 d-token-native 替代路径

当前版本即以 d-token-owned runtime 承载完整能力；内部模块可按以下路径持续替换，但每一步都必须先保持行为兼容与回退：

```text
Phase 1: package and modify Headroom-derived runtime
Phase 2: replace fragile CLI/config/dashboard pieces with d-token-native Rust/TS modules
Phase 3: replace selected compression/router/observability core with d-token-native implementation
Phase 4: keep compatibility with Headroom concepts, but user-facing runtime becomes d-token-owned
```

## 6. 模式设计

### 6.1 Fixture Mode：开发默认

完全读取 `support/fixtures`，不调用任何真实命令。

用于测试：parser、API response、Dashboard、EventLedger、LayeredSavings、ProjectPolicy、ProviderRoute、cc-switch route plan。

必须保证：

1. 不调用真实 `rtk`。
2. 不调用真实 `headroom`。
3. 不调用真实 cc-switch。
4. 不读真实 agent config。
5. 不改任何真实配置。
6. 只使用测试端口。

### 6.2 Fake-binary Mode：开发命令调用测试

通过 `support/fake-bin` 提供假的 `headroom` / `rtk` / `cc-switch`，测试 command runner、stdout/stderr、exit code、Windows `.cmd` shim、error boundary。

必须使用临时 HOME、临时 PATH、临时端口。

### 6.3 Shadow Mode：开发者主动只读检测

只读检测真实环境，例如 existing Headroom、RTK binary provenance、cc-switch health endpoint、provider health。

Shadow Mode 必须由开发者主动开启，不能作为默认行为。

### 6.4 External Mode：连接已有安装，仅用于开发/迁移

适合已经手动装好 Headroom / RTK / cc-switch 的开发者或早期用户。d-token 只做检测、状态展示、analytics import、Dashboard、Doctor 和 route/project 管理。

External Mode 是迁移辅助模式，不是最终用户主路径。

### 6.5 Managed Mode：成品默认路径

适合新用户或希望 d-token 托管工具生命周期的用户。

默认 Managed Mode 安装顺序：

1. 安装/启用 d-token desktop。
2. 安装/启用 d-token Headroom-derived runtime。
3. 通过 runtime 内部路径启用 integrated RTK。
4. 可选安装/连接 cc-switch。
5. 图形化配置 agent wrap。
6. 图形化配置 provider routes。
7. 图形化配置 project compression policy。
8. 生成 rollback point。

### 6.6 Built-in Fallback Mode

当完整 runtime 不可用时，d-token 可提供基础 CLI output preprocessor：ANSI 清理、空行压缩、重复行折叠、超长输出截断、error-only mode。

该模式只能作为 emergency fallback，不声称等价于完整 runtime。

## 7. Integrated RTK 适配范围

Standalone RTK 不作为用户路径保留。d-token 必须适配 integrated RTK：

1. runtime 内部 RTK binary 是否存在。
2. RTK binary 来源、版本、校验状态。
3. agent wrap 是否启用了 RTK instructions。
4. runtime metrics 中 RTK invocation / savings 是否可见。
5. wrap session 结束后的 RTK savings summary。
6. Agent instruction 文件中 d-token/runtime/RTK block 的 diff、backup、rollback。
7. Windows native / WSL / macOS 下 hook 支持等级。
8. 是否存在外部 standalone RTK 冲突、重复 PATH、重复 hook。
9. 冲突只做提示和修复建议，不把 standalone RTK 暴露成推荐选项。

## 8. Headroom-derived runtime 适配范围

当前公开试用候选必须完整保留、吸收并实现：Library / SDK、Local proxy、Agent wrap / unwrap、MCP server、Content-aware compression pipeline、ContentRouter、CacheAligner、SmartCrusher、CodeCompressor、Kompress/text compressor、**Optical context compression（代码/上下文 → PNG → 多模态模型输入）**、live-zone compression、CCR reversible compression、local retrieval、Cross-agent memory、Per-project memory、SharedContext、learn、output shaper、verbosity steering、effort routing、durable savings ledger、metrics endpoints、Historical stats、JSON/CSV export、Cost tracking、Per-model/per-agent/per-project breakdown、Doctor/performance/troubleshooting、integrated RTK。

普通图片文件的 resize、re-encode、tile、ONNX 推理或通用图片上传压缩不属于 d-token 范围，也不得被当作 Optical 迁移完成条件。

完成判定必须逐能力同时满足：真实 Rust-owned 执行路径、公开/内部接口、正常与失败路径测试、隐私与资源上限、Event Ledger/metrics 证据，以及至少一条实际 Agent routed flow。只有路由定义、Tauri command、metadata summary、preview、fixture smoke 或 UI 入口不算迁移完成。对上游实现只做行为参考时必须记录参考 commit 与许可证；复制或修改代码时必须额外记录 NOTICE、文件范围与变更说明。

开发期适配原则：不读真实 CCR 原文、不改真实 Headroom config、不占真实端口。  
成品适配原则：通过 d-token App 安装、启动、配置、更新和卸载，但必须可回滚。

## 8.1 Agent 覆盖要求

Agent matrix 以 d-token managed agent wrap 支持为主。

必须覆盖：

```text
Claude Code
Codex CLI
Cursor
Cline / Roo Code
OpenCode
OpenClaw
Aider
Continue
Goose
OpenHands
GitHub Copilot CLI
VS Code Copilot Chat
Gemini CLI
Windsurf
Kilo Code
Google Antigravity
Mistral Vibe
Pi
Hermes
Cortex Code
```

每个 agent 在 GUI、Doctor 与 CompatibilityMatrix 中至少展示：

1. d-token managed wrap 是否支持。
2. integrated RTK 是否生效。
3. Agent 配置方式：wrap、manual proxy、plugin、rules、instructions、library only、memory/learn only。
4. 官方 provider 配置状态。
5. 第三方 provider 配置状态。
6. 自定义 URL 配置状态。
7. cc-switch route 配置状态。
8. macOS 支持等级。
9. Windows native 支持等级。
10. Windows WSL 支持等级。
11. 限制说明与修复建议。

## 8.2 Provider / Route / Protocol 要求

Headroom-derived runtime 能通过 proxy / SDK / middleware 接入 OpenAI-compatible 与 Anthropic-compatible 流量，d-token 必须把协议级接入作为一等能力，而不是只按具体 agent 列表处理。

### Provider 类型

```ts
interface ProviderProfile {
  id: string;
  name: string;
  kind: 'official' | 'custom' | 'cc_switch' | 'local_chain';
  protocol: 'openai' | 'anthropic' | 'auto';
  base_url: string;
  api_key_ref?: string;
  models?: string[];
  timeout_ms: number;
  healthcheck?: string;
  notes?: string;
}
```

### Route 类型

```ts
interface RouteProfile {
  id: string;
  name: string;
  input_protocol: 'openai' | 'anthropic';
  output_protocol: 'openai' | 'anthropic' | 'same_as_input';
  agent_id?: string;
  project_id?: string;
  runtime_instance_id?: string;
  target_provider_id: string;
  cc_switch_provider_id?: string;
  chain_mode: 'direct' | 'compress_then_provider' | 'compress_then_cc_switch' | 'chain_bridge';
  compression_policy_id?: string;
  bind_host: '127.0.0.1';
  local_port: number;
  fallback_policy: 'disabled' | 'explicit_only';
}
```

### 路由要求

1. 官方 provider 与第三方 provider 必须显式区分。
2. Claude/Anthropic traffic 不得因为配置缺失自动发往 `api.anthropic.com`。
3. OpenAI-compatible provider 不得伪装成 official OpenAI，必须展示 base URL。
4. cc-switch 必须作为 ProviderProfile 或 ChainBridgeProfile 显式配置。
5. route decision 必须进入 Event Ledger。
6. Dashboard 必须显示当前 agent → compression → route → provider 的完整链路。
7. 所有 fallback 默认禁用；如支持 fallback，必须显式展示并要求用户确认。
8. 用户可以选择“压缩后直连自定义 URL”或“压缩后转发到 cc-switch”。

## 9. Headroom-derived 项目级压缩策略

### 9.1 ProjectProfile

```ts
interface ProjectProfile {
  id: string;
  name: string;
  root_path: string;
  language_stack: string[];
  agents: string[];
  default_instance_id?: string;
  compression_policy_id?: string;
  provider_route_id?: string;
  cc_switch_route_id?: string;
  created_at: string;
  updated_at: string;
}
```

### 9.2 ProjectCompressionPolicy

```ts
interface ProjectCompressionPolicy {
  id: string;
  project_id: string;
  level: 'off' | 'safe' | 'balanced' | 'aggressive' | 'max';
  recommended_level: 'safe' | 'balanced' | 'aggressive' | 'max';
  recommendation_confidence: 'low' | 'medium' | 'high';
  enable_ccr: boolean;
  enable_memory: boolean;
  enable_learn: boolean;
  enable_mcp: boolean;
  enable_output_shaper: boolean;
  enable_cache_aligner: boolean;
  enable_code_compressor: boolean;
  enable_json_smartcrusher: boolean;
  enable_text_kompress: boolean;
  enable_integrated_rtk: boolean;
  output_verbosity: 'auto' | 'terse' | 'normal' | 'detailed';
  holdout_ratio?: number;
  notes?: string;
}
```

### 9.3 CompressionRecommendationEngine

推荐等级必须基于：项目语言栈、最近请求 token 规模、命令输出噪音比例、runtime 历史 savings、integrated RTK 历史 savings、LLM 回答质量反馈、retrieve 调用频率、失败率与回退率、用户手动覆盖偏好、高风险任务标记、provider/cc-switch 延迟与错误率。

```text
safe:       默认推荐，适合新项目、高风险任务、未知 provider route
balanced:   常规编码任务推荐
aggressive: 大日志、大测试输出、大搜索结果推荐
max:        只对低风险探索/批量日志/RAG chunks 推荐，必须提示可能损失细节
```

## 10. 分层压缩与路由指标模型

```text
raw_cli_tokens
  → rtk_cleaned_tokens
  → runtime_input_tokens
  → runtime_final_tokens
  → provider_input_tokens

rtk_savings = raw_cli_tokens - rtk_cleaned_tokens
runtime_savings = runtime_input_tokens - runtime_final_tokens
route_overhead = provider_input_tokens - runtime_final_tokens
end_to_end_savings = raw_total_context_tokens - final_llm_input_tokens
```

禁止把 `rtk_savings + runtime_savings` 直接相加当作总 savings，除非 denominator 和 baseline 已经统一。

### 10.1 Runtime 数据来源

1. RTK savings 来自 d-token runtime metrics 或 integrated RTK metrics。
2. Context savings 来自 d-token runtime stats/history/metrics。
3. d-token 按 project/agent/route/provider 拆分展示。
4. d-token 不直接轮询 standalone `rtk gain` 作为产品路径。

### 10.2 Provider / cc-switch 指标

Dashboard 必须展示：

1. route decision count。
2. provider request count。
3. provider 401/403/429/5xx。
4. cc-switch double-hop latency。
5. compression latency。
6. route adapter latency。
7. provider final latency。
8. compressed tokens before forwarding。
9. final provider input tokens。
10. fallback 是否发生；默认不允许静默 fallback。

## 11. Event Ledger

```ts
interface DtokenEvent {
  event_id: string;
  timestamp_ms: number;
  project_id?: string;
  agent_id?: string;
  instance_id?: string;
  command_id?: string;
  request_id?: string;
  stage: 'rtk_rewrite' | 'rtk_filter' | 'agent_wrap' | 'runtime_route' | 'runtime_compress' | 'runtime_retrieve' | 'dtoken_route_decision' | 'protocol_adapt' | 'cc_switch_forward' | 'provider_forward' | 'install_step' | 'config_patch';
  source: 'rtk' | 'runtime' | 'dtoken' | 'cc_switch' | 'provider';
  status: 'ok' | 'passthrough' | 'failed' | 'disabled' | 'skipped';
  input_tokens?: number;
  output_tokens?: number;
  saved_tokens?: number;
  baseline_kind?: 'raw_cli' | 'rtk_cleaned' | 'runtime_input' | 'runtime_final' | 'provider_input' | 'final_llm_input';
  input_protocol?: 'openai' | 'anthropic';
  output_protocol?: 'openai' | 'anthropic';
  provider_id?: string;
  route_id?: string;
  error_code?: string;
  metadata?: Record<string, unknown>;
}
```

开发期 Event Ledger 不保存 full prompt、full response、full file content、full command output。成品默认也不保存，除非用户显式开启本地调试模式。

## 12. Windows/macOS 图形化 App 要求

### 12.1 App 技术路线

```text
Tauri 2 + React + TypeScript + Vite
Rust orchestration core
Headroom-derived packaged runtime
Integrated RTK default runtime ability
Optional cc-switch adapter
SQLite + JSONL event ledger + local JSON/YAML config
```

### 12.2 GUI 必须取代的命令行/网页体验

正式 GUI 的信息架构参考成熟桌面控制面产品逻辑：主界面承载安装、连接、状态、配置和常用管理动作；诊断能力放在 Doctor/Troubleshoot/Logs；开发测试和 benchmark 不进入用户主流程。公开参考包括 Docker Desktop 的 Troubleshoot/日志/诊断分区、Tailscale Windows 的托盘状态与配置入口、OpenCode 的 provider connect/config 流程。d-token 吸收的是产品逻辑，不复制任何外部源码或界面。

实现 GUI 前必须先完成“产品语义翻译”：把开发侧动作翻译成用户能理解的产品动作。例如 `run_opencode_workload_benchmark.ps1` 在正式 UI 中不叫“运行 workload benchmark”，而对应“验证 OpenCode 配置是否生效”；能力 smoke test 在正式 UI 中不叫“能力自检测试”，而对应 Doctor 的“诊断连接/检查配置/查看修复建议”。如果一个动作无法翻译成用户任务，它只能留在 `support/scripts`、CI、隐藏开发工具或测试报告中。

GUI 必须覆盖并逐步取代普通用户对以下命令或页面的依赖：

1. agent wrap / unwrap。
2. local proxy start / stop / restart。
3. doctor。
4. dashboard。
5. learn / verbosity。
6. RTK savings 聚合查看。
7. provider/base URL 配置。
8. cc-switch route 配置。
9. backup / rollback / diff。

高级用户仍可使用 CLI，但 GUI 是默认主入口。

### 12.3 GUI 页面

必须包含：

1. Welcome / Setup Wizard。
2. 傻瓜模式 / 高级模式选择。
3. Agent Selection。
4. Provider Setup：官方 / 第三方自定义 URL / cc-switch。
5. Route Builder：agent → compression → provider / cc-switch。
6. Runtime Status。
7. Integrated RTK Status。
8. Project Policy。
9. Dashboard。
10. Doctor。
11. Logs。
12. Config Diff。
13. Backup / Rollback。
14. Compatibility Matrix。
15. Settings。
16. 中文 / English 切换。

### 12.4 正式 UI 与开发测试边界

正式 UI 必须遵守以下边界：

1. 不展示“测试 workload”“benchmark”“smoke test”“fixture test”“mock test”“fake binary test”等开发验收概念。
2. 不把 OpenCode 当前测试入口设计成长期独立测试页；OpenCode 应作为 Agent Selection / Install Wizard / Route Builder 中的一个 agent。
3. 安装页的主路径是：服务状态 → 选择 agent → 配置 provider/route → 预览 diff/backup → 应用 → 验证 → Doctor/Dashboard。
4. 对用户可见的验证文案应是“验证配置”“诊断连接”“检查路由是否生效”，不能是“跑真实测试”。
5. 所有真实功能仍必须可被自动化测试覆盖，但触发位置应在 `support/scripts`、CI、Tauri command-level harness、隐藏 dev mode 或测试报告中。
6. OpenCode 当前验收只允许验证“安装/启动服务后，点击 OpenCode 的安装/配置/连接动作是否生效”；该验收不能把最终 App 变成 OpenCode 专用测试工具。
7. Dashboard 和 Logs 展示真实运行数据与诊断结果，不展示完整测试 prompt/response，不保存或暴露密钥、token、cookie、auth 文件。
8. 多 agent 兼容是最终产品原则。Claude Code、Codex、Cursor、Cline、OpenCode 等都应共享同一套 agent 接入模型、路由模型、Doctor 模型和 Dashboard 模型。
9. 浏览器或桌面窗口人工验收只能证明 UI 显示、交互、状态切换和可读性正常，不能被写成“真实功能已接入”的证据。真实接入完成必须由 d-token-owned runtime/API/Tauri command、Event Ledger、metrics/export、OpenCode routed flow 等运行证据共同证明。
10. 正式 App 中的 OpenCode 入口只能表达为“连接 OpenCode”“配置 OpenCode”“应用配置”“验证配置”“查看运行状态/日志/诊断”，不能表达为“测试 OpenCode”“跑 benchmark”“跑 workload”。同一条真实验证链路可以由开发脚本复用，但不能变成用户主界面的测试产品。
11. 安装/连接流程必须以 d-token 自有服务为中心：启动或连接 d-token-owned local service，选择 agent，生成 route/provider 配置，备份并应用最小 diff，验证请求是否进入 d-token Event Ledger。不得把外部 Headroom、外部 RTK 或外部 cc-switch 服务当作 UI 数据源或完成态依据。
12. OpenCode 是第一条真实验收入口，不是产品信息架构中心。后续 Claude Code、Codex、Cursor、Cline 等 agent 必须复用同一套 agent 接入、配置 diff、备份回滚、路由验证、Doctor 和 Dashboard 模型。
13. 测试结果可以进入开发报告、CI artifact、`support/artifacts` 或隐藏 dev mode，但正式 UI 只展示用户能理解和能操作的产品状态：已连接、待配置、配置已备份、路由已生效、服务运行中、诊断通过、日志可查看。
14. 若后续需要在 UI 中触发“验证配置”，按钮背后的实现可以调用真实 OpenCode 或其他 agent，但 UI 必须只展示脱敏后的配置生效、路由命中、事件入账和诊断结果，不展示完整 prompt/response、密钥、cookie、auth 文件或开发 benchmark 细节。
15. UI 中不得为了“方便测试”增加长期可见的测试中心、测试标签页、测试按钮、测试报告卡片；开发人员需要的验证入口必须放在脚本、CI、Tauri command harness、隐藏 dev mode 或文档化报告中。允许存在正式产品页「视觉压缩（Beta）」用于 metadata 观测与 shadow 规划预览，但其文案与交互必须面向用户任务，不得呈现 lab/benchmark/workload/fixture 开发语义。
16. 安装页不承担测试中心职责；它只承担服务安装/启动、agent 选择、provider/route 配置、diff/backup/apply、验证配置、跳转 Doctor/Dashboard/Logs 的职责。
17. Dashboard 不展示“测试是否通过”，而展示真实运行事实：请求量、压缩收益、路由目标、provider/cc-switch 状态、错误类型、延迟、Event Ledger 入账状态。
18. Logs 不展示完整测试 prompt/response，不保存或暴露密钥、token、cookie、auth 文件；只展示脱敏后的运行日志、路由日志、配置变更日志、错误日志和导出入口。
19. Doctor 可以执行诊断动作，但文案必须面向修复和状态解释，例如“检查 OpenCode 配置”“检查 d-token 服务”“检查路由是否生效”“查看修复建议”，不得把开发 smoke test 原样暴露给普通用户。
20. 每次新增 UI 功能前必须检查：它是否服务普通用户日常任务；是否复用多 agent 模型；是否读取 d-token-owned 数据；是否泄露开发测试概念；是否能通过脚本/CI 独立验证真实能力。

### 12.5 桌面视觉、布局与交互产品契约

本节是 Windows/macOS 共享 React UI 的唯一设计真理源。不得再建立独立的 Claude 风格设计文档、平行设计系统或多版本视觉规范。实现可以研究公开产品和开源项目，但 d-token 必须保留自己的品牌、文案、数据语义和许可证边界。

#### 12.5.1 设计目标

d-token 是本地 AI Coding 路由与上下文优化控制面，不是营销页、测试面板、聊天克隆或命令行外壳。视觉必须同时满足：

1. 简洁科技感：结构清晰、数据和状态优先，避免装饰性面板堆叠。
2. 亲和力：暖中性色、人文排版、自然语言和舒展留白，避免冷硬运维后台。
3. 克制空间感：浅色与深色都使用稳定纯色画布，由间距、细边界、轻阴影和排版建立层次；不使用渐变流场、星座、符文、字符场或无关装饰。
4. 事实一致性：没有证据时显示 `—`、unknown、partial、unavailable 或真实空态；绿色只表示已验证成功。
5. 跨平台一致：Windows/macOS 共享同一 React 组件、信息架构、字号、间距、状态和中英语义；平台差异只存在于窗口、凭据、进程、系统集成与打包适配层。

#### 12.5.2 AiMaMi 参考边界与连续 App Shell

布局结构和低幅度动画可以高相似度参考 `borawong/AiMaMi`，研究基线为 commit `297c7af56f10fb371b77bc9b6b65aa320afcbe7e`。该项目采用 Apache-2.0；d-token 默认只借鉴结构、尺寸关系和动效原则，独立实现现有组件，不复制 AiMaMi 名称、Logo、图标、截图、人物、二维码、品牌文案或商标资产。若未来逐行复用代码，必须保留许可证和相关版权/修改声明，并逐文件核验第三方来源。

正式 Shell 必须是一个连续窗口结构，禁止“独立玻璃侧栏 + 独立玻璃顶栏”拼接：

1. 左侧栏贯穿窗口上下，展开宽约 184px，折叠宽 64px；桌面过渡 180–220ms。首次无用户偏好时默认折叠为图标轨；用户显式展开后持久化该偏好。
2. 右侧只有一个 inset workspace slab；Topbar 是 workspace 的第一行，与内容区共享同一表面、圆角和边界，不是单独悬浮部件。
3. 侧栏顶部、workspace Topbar 和 macOS 拖拽区共享 48px 水平基线。
4. workspace 桌面外边距建议 18px，圆角 12px，1px 细边界与极轻单层阴影；920×640 时可收紧外边距并自动折叠侧栏，但不得改变信息架构。
5. Topbar 只保留窗口拖拽、当前上下文和必要全局控制；不得重复页面 H1、品牌区或工程状态墙。
6. 侧栏容器建议 8px 内距；导航按钮圆角 8px、图标约 16px、低对比 hover；选中态只使用一层暖色轻染和一个清晰指示。
7. macOS drag region 只能覆盖空白区；按钮、导航和可编辑控件必须显式 no-drag。Windows 使用同一布局，窗口控制差异由 Tauri/平台适配层处理。
8. 一级导航固定为：总体概览、安装与连接、代码图谱（实验性）、分析、视觉压缩（Beta）、Doctor、变更与回退、设置。除「视觉压缩（Beta）」与「代码图谱（实验性）」这两处已批准的实验能力入口外，工作台子页面不得重新扩张为顶级工程导航。

#### 12.5.3 内容栅格与四格概览

用户已确认概览四格结构，必须保留为真实总体数据驾驶舱：

1. 宽屏可用区 `>=1040px` 使用 4 列；`720–1039px` 使用 2 列；更窄使用 1 列。
2. 四格间距 16px；KPI 卡建议最小高 98–112px、内距 16px、圆角 16px、细边界和极轻阴影。
3. 标签 12–13px/500，数值 26–28px/600 并启用 tabular numerals，详情 12–13px。
4. 四格只展示可追溯的请求、Token/压缩、路由/Agent、错误/健康等总体事实；不成为功能入口，不使用大面积彩色光晕。
5. 页面横向内距：常规桌面 24px，较窄窗口 16px；主要纵向节奏 24px；主卡圆角 16px、inner panel 12px、按钮/导航 8px。
6. 四格下方立即展示活动趋势；运行概况固定在页面最底部。概览首屏最多两个完整边界主表面，禁止 card-inside-card，内部优先使用分隔线、平铺行、日历和留白。
7. 活动趋势默认“本年”，整体尺寸、密度、方格、月份标签和强度图例高相似度参考 `evidence/ui/01-dashboard-activity-year.png`：本周为 7 个紧凑日格/短列，本月为真实 5/6 周 × 7 日小方格，本年为约 53 周 × 7 日连续热力图。不得使用大日期格、独立日卡或 12 张迷你月历卡。
8. 零活动也必须显示完整数据窗口；日期使用本地时区且从周一开始。1180×780 与 920×640 都必须保持趋势模块紧凑、可读、无横向滚动；窄窗口可缩小 gap/方格或使用受控内部滚动，但不能放大为日历。

#### 12.5.4 字体与双语排版

字体采用“Claude 类编辑感标题 + 人文系统正文”，但不得使用 Anthropic 专有字体、名称或品牌资产：

```css
--font-display: ui-serif, "Iowan Old Style", "Palatino Linotype",
  "Songti SC", STSong, SimSun, serif;
--font-ui: ui-sans-serif, -apple-system, BlinkMacSystemFont,
  "SF Pro Text", "Segoe UI Variable Text", "Segoe UI",
  "PingFang SC", "Microsoft YaHei UI", "Noto Sans CJK SC", sans-serif;
--font-mono: "SFMono-Regular", Consolas, "Liberation Mono", monospace;
```

1. 页面 H1 使用 display stack，24–28px、500、紧凑字距；每页只出现一个 H1。
2. 区块标题 16–18px/600；正文 14–15px/400、行高 1.5–1.6；按钮和标签 13–14px/500。
3. KPI 数字使用 UI stack/600；只有代码、真实路径和脱敏日志使用等宽字体。
4. 禁止用全大写、超宽字距、粗黑字体或伪终端文字制造科技感。
5. 所有正式页面、空态、按钮、错误和状态必须提供中文/English；翻译应表达同一产品任务，不允许一侧遗留测试文案或更详细的敏感信息。
6. macOS/Windows 字形可由系统字体决定，但字号、权重、行高、布局语义和截断规则必须一致。

#### 12.5.5 色彩、主题与表面

1. 画布使用暖米白/暖灰黑；主文本为暖墨色/柔和米白。
2. 珊瑚橙/琥珀是稀缺主强调，只用于主动作、焦点、选中和少量关键数据；成功绿、警告黄、错误红仅表达语义状态。
3. 浅色、深色、跟随系统必须保留；背景使用各模式对应的固定纯色。用户可选择强调色和分析图表色，但不能把强调色扩张为大面积背景装饰。
4. 表面优先使用明度差和细边界；阴影仅用于 workspace、浮层和少量关键卡片，禁止玻璃卡片连续嵌套。
5. 同一屏不得出现大面积渐变、厚阴影、彩色描边、发光或高对比背景动画。
6. 所有交互状态必须区分 default、hover、focus-visible、pressed、disabled、selected；hover 不放大、不漂浮，pressed 位移最多 1px，focus-visible 在深浅色中清晰可见。

#### 12.5.6 纯色画布与层次

1. App 根画布、侧栏和 workspace 只使用稳定纯色；浅色以暖白/浅暖灰为主，深色以暖黑/深灰为主，不使用流动渐变、渐变色团或纹理背景。
2. Shell、Topbar 和内容区的层次通过 0.5–1px 边界、微小明度差、局部单层阴影和留白表达；不得依赖 backdrop blur、玻璃折射或持续动画。
3. KPI 可以像 AiMaMi 参考图一样使用极浅的单向色调或顶部短色条，但面积与饱和度必须克制，不能演变为页面背景。
4. 页面背景与语义色严格分离；success/warning/error 只用于状态、图标、短边线和标签，不铺满大区域。
5. 主题切换必须立即更新根节点、系统 `color-scheme` 和所有浮层；持久化失败只提示，不把当前会话闪回旧主题。
6. 移除渐变预设、自定义背景三色、流速设置及其生产代码、CSS、测试和用户文案；不得保留不可见的后台动画。

#### 12.5.7 动效

```text
instant    100–120ms  按下与图标反馈
fast       140–180ms  hover、focus、短淡入
standard   180–220ms  页签、折叠、对话框、侧栏
```

1. 页面切换只使用低幅度淡入和最多 1–4px 位移；不使用夸张 spring、持续弹跳或卡片悬浮。
2. 列表次级动作可在 hover/focus-within 出现，但键盘用户必须同样可达。
3. 侧栏一级导航、主题和折叠图标必须各有**独立、语义相关**的短动画配方（约 200–280ms）；禁止全体同一旋转、缩放或弹跳。零件动效允许使用项目已有 motion 库或等价 CSS/SVG 实现，但必须满足本条与下列 enter/leave 契约。
4. **指针进入（hover）与离开** 均须有可感知反馈：进入播 forward 配方，离开播**反向或 settle-back 到 idle**，不得仅在 hover 时播一次 forward 后硬切回静态（例如设置齿轮进入顺时针约 90°、离开逆时针回 0°）。
5. **按下/点击** 须有**容器级**轻量 scale（约 0.92→1，instant 档），可与零件动画叠加，但不能替代各图标独立配方。
6. **focus-visible** 须与 enter 同等可达；焦点离开时与 leave 同样 settle。
7. 无 idle/仅选中态下的图标常驻循环；选中态以颜色与指示轨表达。指针或焦点**保持**在图标上时，允许该图标在 engaged 期间持续播放其独立配方（离开后必须 settle 回 idle）；不得在未交互时整侧栏空转。
8. 分段控件的指示器、开关、进度步骤、图表数据进入、tooltip、复制成功和状态刷新必须有清晰但克制的反馈；轮询刷新不得反复重播整页动画。
9. Reduce Motion 下移除页面位移、图标零件 transform、容器 scale 弹、图表绘制和所有非必要循环动画，只保留颜色、边界和文本状态变化。
10. “动画代码存在”不构成完成证据：侧栏图标必须在真实 App 中可重复触发 **enter / leave / click** 并肉眼区分各配方；页面切换必须实际触发 180–240ms 的淡入与最多 4px 位移，不能被路由挂载、Suspense 或复用节点吃掉。
11. 动画验收必须保存真实窗口录像或至少两帧时间差证据，并同时证明 Reduce Motion 下 transform/位移与容器 scale 弹被禁用。

#### 12.5.8 二级 UI 与细节完成度

1. Dialog、AlertDialog、Popover、Tooltip、Dropdown、Select、Tabs、Drawer、Toast、表格行操作和空态必须来自同一设计系统，统一字号、圆角、边界、阴影、焦点环和关闭行为。
2. 二级页面沿用相同 Shell 和内容密度，不得退化成旧工程页、测试页或另一套视觉；返回、标题、面包屑或上栏标签必须稳定且可键盘操作。
3. 设置页采用 grouped rows；Doctor/维护采用“检查项 → 说明/证据 → 状态 → 动作”的平铺分组；Changes/Logs 使用紧凑表格、时间线或列表；只在明确分组和危险确认时使用完整边界表面。
4. 图表必须具备可读坐标、图例、tooltip、空态、真实来源与时间范围切换；周/月/年或不同指标如果语义不同，应使用不同布局而不是缩放同一张图。
5. 所有 hover、focus-visible、pressed、selected、disabled、loading、success、warning、error 状态逐组件验收；图标按钮必须有 tooltip/accessible name，危险按钮不得只靠颜色表达。
6. 文案、图标、数字、路径、表格列和按钮在中英、1180×780 与 920×640 下不得抖动、截断关键含义或产生孤立残片。
7. 所有配置应用、回退、停止服务、Doctor 修复、导入、导出和其他有副作用动作必须使用统一的正式 `Dialog` / `AlertDialog`；弹窗至少展示动作标题、影响对象、真实摘要、风险/备份/恢复信息、取消和确认状态。禁止使用浏览器原生 `window.confirm` / `alert` / `prompt`，也禁止把裸 JSON、原始 command output 或单段无层次文本冒充操作界面。
8. Topbar 提供通知入口、未读状态和可访问名称；重要状态在 Topbar 下方以有设计的顶部通知呈现，包括服务断连、配置已应用/已恢复、Doctor 完成/失败、操作失败和需要用户确认。普通成功可以短暂消失，危险或待处理状态必须保留到用户确认。
9. 通知由 App 级统一状态管理，具备稳定 id、severity、timestamp、read/dismiss、details/action receipt 关联和去重策略；轮询刷新不得重复刷出相同通知，通知与导出不得包含密钥、原始 prompt/response、完整配置 blob 或未脱敏路径。
10. 顶部通知、通知列表和操作弹窗必须支持键盘、焦点恢复、`aria-live`、中英和 Reduce Motion；Reduce Motion 下不使用滑入位移，只保留即时出现/淡入和状态颜色变化。

#### 12.5.9 真实性、空态与文案

1. 无运行证据时不得显示默认端口、默认路径、假 Agent、假 provider、假 fallback、假曲线、假日志、假成本或绿色成功态。
2. 缺失数值显示 `—`，不得显示 `0 Token`、`0%`、`$0.000` 或用 `Date.now()` 补造时间。
3. 空态包含事实、影响和数据出现条件；正式 App 与浏览器预览均不得加载 fixture 作为内容。
4. 概览展示总体事实；安装、配置、验证、诊断、日志、导出和回退只在对应任务页面出现。
5. UI 动作必须使用用户语言：安装、连接、配置、预览变更、应用、验证、诊断、查看状态、查看日志、导出、回退；不得出现 benchmark、smoke、fixture、mock、fake-bin 或 testing center。
6. 所有一级页共享 App 级运行状态与 freshness 模型；首次加载可以显示 skeleton，后续刷新必须保留最后一份可验证证据，不得因为切页重新显示“未启动”或反复播放加载动画。
7. 服务状态与数据状态必须分开表达：服务在线但尚无请求时显示“服务在线，暂无请求”和真实零值；服务在线但某数据切片失败时只将该切片标为 partial/unavailable，不得把整个 App 降级为 stopped。

#### 12.5.10 可访问性与跨平台窗口验收

1. Tab/Shift+Tab、方向键、Home/End、Enter/Space、Esc 必须完成对应任务；焦点顺序与视觉顺序一致。
2. 状态不能只靠颜色；图标和文字必须同时表达。
3. 1180×780、920×640 和全屏下无横向滚动、内容遮挡、卡片残片或状态栏覆盖；920×640 必须实测展开/折叠侧栏。
4. 浅色、深色、跟随系统、自定义主题、中英切换、Reduce Motion 必须在真实 macOS Tauri 窗口验收；Windows 使用同一 UI 自动化契约，并在可用 Windows CI/机器上补实包验证。视觉压缩（Beta）页与其他一级页共享该合同；缺 Windows 机器时不得把 macOS 单端截图写成双端完成。
5. 浏览器 E2E 只证明布局和交互；功能完成还必须由 Tauri command、d-token-owned runtime、Event Ledger、export 与真实 routed Agent flow 证明。
6. 正式构建产物不得包含 mock、fixture、benchmark、smoke、fake-bin、testing center 等测试语义。

#### 12.5.11 UI 完成证据

UI 达到内测水准必须同时具备：

1. React 单元测试、TypeScript、Vite production build、Chromium/WebKit E2E 全绿。
2. macOS `.app` 真实窗口截图与可访问树证据，覆盖中英、深浅色、1180×780、920×640、键盘和 Reduce Motion。
3. 最小流程：启动/连接 d-token → 选择 Agent → 配置 Provider/Route → 预览 diff/backup → 应用 → 验证 → 在概览/日志看到真实结果。
4. 扩展流程：多 Agent、cc-switch/custom upstream、Doctor、日志导出、配置变更与 rollback。
5. OpenCode 真实 routed test 使用正常配置位置与自动发现的官方免费候选，证明流量命中 d-token、route/provider/model/CCR 与 Event Ledger，并逐字节恢复临时配置；UI 只显示脱敏后的“配置已验证/路由已生效”等产品状态。
6. OpenCode 长上下文验收必须使用非敏感合成输入并证明压缩真实触发，而不是只完成路由冒烟测试；同一 request id 的压缩决策、context accounting、signed savings、route decision、RequestLog、CCR metadata 与 provider outcome 必须可关联，且不得持久化原始 prompt/response 或测试 nonce。
7. 同一条真实请求必须一致出现在概览 KPI/活动趋势、分析、日志/本地操作记录、Doctor 路由证据、变更历史和状态栏；重启 App 后最后可验证证据、服务 PID/端口和连接状态仍保持一致。

## 13. Doctor 要求

### 13.1 开发期 Doctor

开发期 Doctor 必须支持 no-touch 检查：

1. 当前 profile 是否为 fixture/fake/shadow。
2. 是否误调用真实 binary。
3. 是否使用测试端口。
4. 是否使用临时 HOME。
5. 是否存在外部配置写入风险。
6. 是否误连接真实 Headroom / RTK / cc-switch。

### 13.2 成品 Doctor

成品 Doctor 必须检查：

1. Local proxy。
2. 端口占用与旧进程残留。
3. 进程身份校验：kill 前必须确认是 d-token-owned 或用户确认。
4. d-token runtime/proxy/MCP/stats/CCR summary。
5. Integrated RTK binary/version/hook/metrics。
6. 外部 standalone RTK 冲突检测。
7. Provider route。
8. cc-switch route。
9. Project policy。
10. Config backup/rollback。
11. Event ledger。
12. Installation transaction。
13. Windows UTF-8、盘符路径、`.cmd` shim。
14. macOS xattr/quarantine/keychain/launch agent。

## 14. 已知问题沉淀为产品要求

以下问题必须转化为 d-token 的 Doctor、GUI、路由和安装验收项。

### 14.1 Provider / Route / Dashboard UX

1. 第三方 provider 401：不得因路由缺失把请求静默发到官方 Anthropic/OpenAI；GUI 必须展示真实 target provider。
2. 重启后端口占用：必须检测旧进程、端口占用、PID 身份，不能盲目启动新服务。
3. Dashboard 看不到请求：必须提供 route chain probe，确认 agent 请求是否真的经过 d-token runtime。
4. Output token savings 提示误导：可选功能说明必须和错误提示视觉区分。
5. learn/verbosity 项目上下文不明确：GUI 必须告诉用户需要选择项目、当前目录或 all projects。
6. 仪表盘无中文：d-token GUI 必须支持中文。

### 14.2 Windows 功能性兼容

1. UTF-8 编码：读取 session、JSONL、配置、日志时必须显式 UTF-8，不能依赖 Windows 默认 GBK。
2. 盘符路径：必须正确处理 `D:\work\project`、`D--work-project` 等编码/解码格式。
3. 项目名连字符：不能仅靠 `-` 反推路径；应优先从 session `cwd` 或显式 project root 反查真实路径。
4. npm 安装 CLI shim：Windows CreateProcess 失败后必须尝试 `.cmd` / `.exe` / `.bat` shim，但要记录实际执行路径。

## 15. 安全、隐私与开源合规原则

1. 默认只监听 `127.0.0.1`。
2. API key 只存在本地。
3. 日志默认脱敏。
4. Authorization / Cookie / API key 永不明文展示。
5. 导出日志二次脱敏。
6. RTK/CLI 输出预处理层不得默认记录完整命令输出。
7. d-token 不自动开启 RTK telemetry。
8. d-token 导入 RTK analytics 时默认只读取聚合统计。
9. d-token 不上传 RTK tracking database、tee files、prompt、代码、命令输出到远端。
10. d-token 不默认读取 CCR 原文缓存。
11. 所有 managed binary 必须记录来源与校验信息。
12. 成品覆盖安装必须有 backup / rollback。
13. 开发期测试必须默认 no-touch。
14. 所有非 loopback 绑定必须拒绝或要求强认证与显式确认。
15. 所有 provider route 与 fallback 必须可见、可解释、可审计。
16. 不提供账号托管、账号共享、账号池、转售或代登录能力。
17. 不帮助隐藏真实使用来源，不绕过第三方平台条款。
18. 吸收 Headroom/RTK 代码必须遵守上游开源 license、保留必要版权声明、NOTICE、变更说明与依赖清单。
19. d-token 修改 Headroom-derived runtime 时必须记录 upstream base commit，方便安全审计、升级和回滚。

## 16. CompatibilityMatrix

必须维护 OS / arch / shell / agent / runtime mode / provider route / cc-switch mode 的兼容矩阵：

```ts
interface CompatibilityCase {
  id: string;
  os: 'macos' | 'windows' | 'linux';
  arch: 'x64' | 'arm64';
  shell: 'bash' | 'zsh' | 'powershell' | 'cmd' | 'wsl-bash';
  agent: string;
  runtime_mode: 'fixture' | 'external_headroom_migration' | 'managed_dtoken_runtime' | 'disabled';
  rtk_source: 'integrated' | 'conflicting_external_detected' | 'disabled';
  provider_route_mode: 'official' | 'custom_openai' | 'custom_anthropic' | 'cc_switch' | 'chain_bridge';
  expected_hook_support: 'full' | 'partial' | 'none';
  expected_proxy_support: 'full' | 'partial' | 'none';
  status: 'untested' | 'pass' | 'fail' | 'blocked';
  notes?: string;
}
```

Mac 与 Windows 都是首要目标平台。Windows 必须区分 native 与 WSL：native 模式通常只能获得部分 hook/规则能力，WSL 才能获得完整 shell hook 体验。

## 17. 目录结构

仓库根目录允许：

```text
README.md
AGENTS.md
CLAUDE.md
docs/
code/
support/
.github/workflows/
.gitignore
```

目录含义：

```text
README.md  面向 GitHub 访问者的项目入口介绍
docs/      所有正式文档，包含唯一 PRD、架构、技术决策、安全边界、审查报告、研究报告
code/      所有应用代码、crate、package、runtime、配置样例
support/   测试、脚本、开发辅助、CI 模板、fixture、fake-bin、临时杂项
```

## 18. 里程碑与执行计划

d-token 的阶段执行计划、任务拆解、风险与 CI gate 不再写入 PRD，统一存放于 `docs/PLAN.md`，按代码现状滚动维护。

- 当前活跃计划：`docs/PLAN.md`
- 历史路线图快照（M0–M6 原文）：`docs/archive/ROADMAP_M0_M6_HISTORICAL.md`
- 已交付里程碑证据：`docs/history/M1_NO_TOUCH_DEV_FOUNDATION_REPORT.md`、`docs/history/M2_EXTERNAL_READ_ONLY_INTEGRATION_REPORT.md`、`docs/history/M3_GUI_CONTROL_PLANE_MVP_REPORT.md`、`docs/history/M1_M3_DELIVERY_AUDIT.md`

PRD 只保留产品真理源：目标、边界、技术契约与验收标准。计划文档不得与 PRD 验收标准冲突；若产品边界变化，先改 PRD，再同步 PLAN。

## 19. 验收标准

1. 仓库只有一个 PRD：`docs/PRD.md`。
2. 所有正式文档都在 `docs/`。
3. 所有正式代码都在 `code/`。
4. 所有测试、脚本、fixture、fake-bin 都在 `support/`。
5. 开发期默认 no-touch，不破坏开发者本机已有 agents/RTK/Headroom/cc-switch。
6. 成品支持覆盖安装，但必须有事务、备份、校验和回滚。
7. 用户只需下载安装 d-token App，不应被要求先安装 Headroom 或 RTK。
8. Headroom 功能吸收不能删项；公开试用候选必须完成 §8 全能力迁移，后续创新替代必须保持能力、兼容性与可回退性。
9. Dashboard 迁移不能缩水，GUI 必须逐步取代普通用户对网页 dashboard 与命令行安装的依赖。
10. 默认 RTK 路径是 integrated RTK，不保留 standalone RTK 用户路径。
11. Dashboard 必须能区分 RTK 命令输出预处理收益、runtime 上下文压缩收益、端到端总收益、provider/cc-switch route overhead。
12. 用户必须能按项目自定义压缩等级，并查看推荐等级与推荐理由。
13. 用户必须能选择官方 provider、第三方自定义 URL、cc-switch 或 chain bridge。
14. 用户必须能图形化选择 agent 并安装/接入对应 wrap/config。
15. 所有 agent/runtime/provider/cc-switch/shell 配置修改必须支持 diff、backup、rollback。
16. CI 必须至少覆盖 structure check、cargo check、cargo test、no-touch smoke、desktop build。
17. M3 GUI 开发默认 fixture，不允许默认扫描或修改真实环境。
18. M4/M5 成品安装器允许覆盖安装，但必须可追踪、可解释、可恢复。
19. Windows UTF-8、盘符路径、项目名连字符、`.cmd` shim 必须进入兼容性测试。
20. 不允许请求因为配置缺失静默走官方 provider；route decision 必须可见、可解释、可审计。
21. d-token 吸收/修改 Headroom/RTK 代码必须保留开源合规记录、upstream base commit、NOTICE 与变更说明。
22. 正式 UI 不得出现 workload benchmark、smoke test、fake/fixture test、testing center 等开发测试产品化入口；这些能力只能存在于脚本、CI、隐藏开发模式和报告中。
23. 真实功能验收必须区分两类证据：桌面/浏览器验收负责证明界面正确，运行时验收负责证明 OpenCode 或其他 agent 的真实请求经过 d-token-owned service、route adapter、upstream provider，并写入 Event Ledger。
24. 安装页验收以产品流程为准：打开后自动读取 Agent、现有上游和 cc-switch；唯一方案直接准备完成，有歧义时只补充选择或凭据；用户确认后自动安装/启动 d-token、应用最小 diff、验证并生成回退点。不得要求普通用户先进入高级安装、手填已可发现 URL，或把“点击测试按钮跑 workload”作为最终用户验收路径。
25. OpenCode 验收必须证明“正式安装/配置流程里的 OpenCode 连接动作有效”，而不是证明“UI 里有 OpenCode 测试按钮”。测试实现可以复用脚本，但用户看到的是连接、配置、验证、诊断和运行状态。
26. 每个真实能力接入完成时，必须同时提供 UI 证据和运行时证据：UI 证据来自浏览器/桌面窗口检查；运行时证据来自 d-token-owned API/Tauri command、Event Ledger、metrics/export、OpenCode routed flow 或对应 agent flow。
27. 功能实现前必须先对照 PRD 和成熟应用逻辑完成产品判断；如果实现方案会把开发测试、fixture、mock、benchmark、smoke 直接暴露给用户，则验收不通过。
28. d-token 最终 UI 必须保持多 agent 控制面结构；任何只适配 OpenCode、只展示 OpenCode 测试、或把 OpenCode 作为唯一产品中心的实现都不满足 PRD。
29. 概览必须默认本年，并以 AiMaMi 式紧凑热力图展示本周、本月和本年不同数据窗口；零活动时对应时间格仍完整可核验，活动趋势紧随四格 KPI，运行概况位于页面最底部。任何整屏大月历或大日期卡实现均不通过。
30. 页面切换和 App 重启不得丢失已验证运行状态；所有一级页必须基于同一份带 source、freshness、collectedAt 和 error 的运行证据，局部失败不得伪装成全局停止。
31. 上游必须按 Agent 优先自动发现和解析；唯一高置信度候选自动选中，OpenCode 多 provider、cc-switch 多候选或字段不确定时才由用户显式选择。发布验收默认使用 OpenCode 官方免费模型完成真实 routed 与长上下文压缩测试。
32. 发布候选必须绑定同一 commit 的 macOS/Windows 安装产物、哈希、自动化门禁、真实安装启动、功能链路、配置精确恢复与已知限制；不能用旧截图、旧 DMG 或不同 commit 的测试结果拼接发布结论。
33. 所有正式可见的有副作用操作必须通过统一产品弹窗完成，并产生可关联的动作结果；Topbar 必须提供重要状态通知与未读入口。浏览器原生确认框、无来源 toast、重复轮询通知或裸文本操作结果均不通过。
34. 真实 Agent 测试前必须有逐页功能—数据源—持久化矩阵证据，证明所有正式可见功能已接通或按 PRD 明确隐藏；八个一级页（含视觉压缩 Beta 与代码图谱实验页）、所有二级 UI、Topbar、StatusBar、弹窗和通知不得出现互相矛盾的检测、连接、运行或保存状态。
35. 统一状态必须在真实 App 中证明三类更新：后台真实请求发生后无需切页即可实时刷新；切换页面后同一事实保持一致；退出并重开 App 后服务、设置、事务、回退点和最后可验证证据正确恢复。完成这三类证据前不得开始最终 OpenCode routed/compression 验收。
36. 一键安装必须证明 `already-connected` 幂等状态、多 Agent 分层事务、配置并发修改失效和失败回退；相同配置不得重复写入、重复重启或制造无意义变更。
37. 高级安装必须由自动方案预填进入，并与一键安装共享同一校验和事务引擎；恢复自动推荐不得删除用户原有第三方配置。
38. 智能学习必须在进入对应上栏后自动枚举本机全部受支持 Agent 的可安全识别会话记录，不输入项目路径、不依赖已有 d-token 请求也能发现项目；必须证明 metadata-only、增量缓存、进度、取消、排序解释和大数据量下 UI 不阻塞。
39. 正式 App 只能有一个 App 级运行证据调度器；页面级重复固定轮询、全量 payload 携带全部详情、长列表完整 DOM 渲染或退出动画阻塞导航均不通过性能验收。
40. 发布候选必须提供同一 commit、同一 release build 的启动、导航、runtime 刷新、空闲 CPU/内存、Native command p95、长列表和智能学习压力测试证据，并满足 P0.2.2 性能预算；未达标不得宣称“流畅”“低占用”或“高性能”。
41. Headroom 全能力验收必须使用固定上游参考 commit 的能力矩阵逐项证明 Library/SDK、proxy、wrap/unwrap、MCP、ContentRouter 与各类 compressor、CacheAligner/live-zone、CCR retrieval、memory/SharedContext、learn、output shaping、ledger/stats/export、Doctor 与 integrated RTK；任何 passthrough、metadata-only、preview-only、测试专用或外部 8787 依赖项均判定未完成。
