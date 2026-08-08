# REQUIREMENTS — v0.0.1 Requirement Registry

- 每个 requirement 有唯一 ID（类别-序号）；`owner` 指向 [EXECUTION.md](./EXECUTION.md) 切片或 [VERIFICATION.md](./VERIFICATION.md) 验证节点；`release claim` 标注该需求在 v0.0.1 的发布口径（MUST=阻断 / BOUNDARY=列入计划但默认不阻断，用户最终裁决 / N/A=过程性）。
- current = 2026-08-08 可引用的现有证据（引用仓库内文档/脚本）；gap = 尚缺什么；acceptance = 可独立验证的通过标准。
- 本表是 [TASKS.md](./TASKS.md) 与 [VERIFICATION.md](./VERIFICATION.md) 的追踪根源：任何 requirement 必须在 TASKS 中有任务、在 VERIFICATION 中有验证节点，缺一即计划不完整。

## 1. VERSION（版本）

| ID | 需求（来源） | current | gap | owner | acceptance | release claim |
|---|---|---|---|---|---|---|
| VERSION-0 | 目标版本 v0.0.1 一次性统一（用户指定；当前 manifest/PRD 为 0.2.0-beta.1） | `tauri.conf.json`/`package.json` = 0.2.0-beta.1；PRD P0.0.7 以 0.2.0-beta.1 为发布门；PLAN/IMPLEMENTATION_STATUS/CHANGELOG 同口径 | 无任何 v0.0.1 事实源；多版本事实源并存 | V-1 | 用户确认后：manifest、PRD 版本门、PLAN、IMPLEMENTATION_STATUS、CHANGELOG、release workflow、`check_release_version.py` 全部收敛为 v0.0.1；`python3 support/scripts/check_structure.py` 通过；旧版本号仅存于 history/archive | **MUST（阻断）** |
| VERSION-1 | 发布结论绑定版本与候选 commit（PRD P0.2 发布目标第 13 条「artifact 携带 SHA-256 manifest、候选 provenance、版本…绑定同一候选提交」、§19 验收标准第 32 条「发布候选必须绑定同一 commit 的 macOS/Windows 安装产物、哈希、自动化门禁、真实安装启动、功能链路、配置精确恢复与已知限制」） | `generate_release_gate_status.py` fail-closed 已存在；candidate binding 契约待接线 | 聚合器未强制 `expected_commit`/`expected_source_digest` | PR-7, PR-9 | 任何 artifact 缺 commit/digest 或不匹配即 blocked；结论文档必须写版本号 | MUST |

## 2. PRODUCT（产品功能）

| ID | 需求（来源） | current | gap | owner | acceptance | release claim |
|---|---|---|---|---|---|---|
| PRODUCT-1 | OptimizationReceipt 最小字段 + Local/Provider/claimable 分离（PRD P0.0.7、spec v2 §7.2） | 前端已声明 Provider Token；Rust `RequestLog` 无 provider input/output 字段；无 receipt 类型/投影 | PR-1 实现 | PR-1 → N1 | 新增 PRD 最小 receipt；`mark_upstream_outcome` 写 Provider actual usage；Provider usage 单独不提升 claimable；缺 usage 显示 unavailable | MUST |
| PRODUCT-2 | 首页节省 + 四区间（累计/当周/当月/当天），真实 0 与 unavailable 区分（GOAL G7） | 区间切换已实现（`code/apps/desktop/src/app/lib/overview/savings.ts` + `overview/components.tsx` 的 SavingsWindowCard；`Overview.test.tsx` 有窗口边界单测） | 最终候选统一证据重验 | N1 | 四区间数值均来自同一 runtime/Event Ledger/设置证据；无 mock 补齐 | MUST |
| PRODUCT-3 | 分析页日志子页逐请求真实字段（GOAL G4、PRD §12.5） | 日志子页已实现（`code/apps/desktop/src/app/components/pages/Analytics.tsx` 日志子页；`/analytics/requests` cursor API + Tauri 只读桥接 + 前端分页，见 `docs/product/IMPLEMENTATION_STATUS.md` 已存在的本地 API 节） | 最终候选证据 + 脱敏复核 | N1, N6 | 字段清单与 GOAL G4 一致；无 Provider usage 不显示 Provider Token；无 raw/secret | MUST |
| PRODUCT-4 | 设置页模型价格维护（GOAL G5/G6、PRD） | 价格表已实现（`docs/PLAN.md` 当前坐标 14：Settings `model-pricing` 价格表，Overview/Analytics 据此估算 input cost before/after/delta，unknown/mixed/过期 fail-closed） | 最终候选复验：编辑/保存/删除、币种/单位/来源/更新时间 | N1, N6 | 缺价格≠0；费用仅在 Token+价格证据同时存在时展示 | MUST |
| PRODUCT-5 | 首次配置入口 + 右上角重复进入（GOAL G2） | onboarding 已实现（`code/apps/desktop/src/app/components/pages/Onboarding.tsx` + `lib/onboarding.ts`：首次无完成标记自动进入、右上角可重复进入、完成后回总体概览；`AppShell.onboarding.test.tsx`/`Onboarding.test.tsx` 覆盖） | 最终候选复验 + 与无 chooser 契约一致性 | N3, N6 | 首次无配置进入；完成后回产品；右上角可重复进入；无 chooser | MUST |
| PRODUCT-6 | 代码图谱（实验性）状态诚实区分 package/integration/agent_session/ui_session/index（PRD P0.0.9、1D） | 五维状态 UI 已实现；persistence=false 强制 | CBM Windows 收口 + 候选证据 | PR-CBM2, N6 | 状态不冒充；坏 hash/平台/arch fail-closed | MUST |
| PRODUCT-7 | 不新增一级导航；日志在分析页子页（GOAL G3） | 已满足 | 结构断言 | N6 | 导航八页无新增；Analytics 日志高度无 diff | MUST |

## 3. RUNTIME（运行时代理/事件）

| ID | 需求（来源） | current | gap | owner | acceptance | release claim |
|---|---|---|---|---|---|---|
| RUNTIME-1 | RequestLog Provider usage 字段 + `mark_upstream_outcome` 写 actual input/output（PRD P0.0.7） | 现有 `mark_upstream_outcome` 只写 HybridContextAccounting/Event | 字段 + 投影 | PR-1 | 单测证明写入后投影；serde 兼容旧字段 | MUST |
| RUNTIME-2 | runtime/Tauri command/Event Ledger 是唯一事实源（GOAL B1、INV-1） | 已成立 | 候选证据复核 | N1 | 页面数据无 mock/fixture 冒充 | MUST |
| RUNTIME-3 | 默认只绑定 127.0.0.1（GOAL B3、INV-2） | 已实现（`dtoken-proxy --host` 默认 `127.0.0.1`，`parse_loopback_host` 只接受 loopback） | 门禁断言 | N0 | N0 断言通过：`cargo test -p dtoken-proxy loopback_host_validation` + 非回环 bind grep 零命中（见 [VERIFICATION.md](./VERIFICATION.md#1-n0--候选全量质量门w5所有切片合并后)） | MUST |
| RUNTIME-4 | 不复制整份 HybridContextAccounting 进 receipt（INV-7、PRD §10） | 无 receipt 类型 | PR-1 边界 | PR-1 | 审查确认 receipt 只含 PRD 最小字段 | MUST |
| RUNTIME-5 | 事件/Ledger/指标统一；savings 分账不双计（GOAL G7、PRD §10：禁止 rtk+runtime 直接相加） | ledger 统一；RTK 分账契约已定 | 候选复验 + PR-R3 契约测试 | PR-R3, N1 | 分母统一；`rtk_savings + runtime_savings` 无相加路径 | MUST |
| RUNTIME-6 | Codex 动态 model 透传、Responses 协议、无 OpenAI 认证语义（PRD P0.0.10.5） | `ProviderWireApi::OpenAiResponses`、`models=[]` 已实现 | 真实 routed acceptance | PR-2, N2 | 链路 `Codex→18001→10100→动态 model` 有 artifact | MUST |

## 4. AGENT（Agent 接入/路由）

| ID | 需求（来源） | current | gap | owner | acceptance | release claim |
|---|---|---|---|---|---|---|
| AGENT-1 | Codex 真实 routed-flow（PRD P0.0.7/P0.0.10.5、验收 45） | 代码链路已实现；无 acceptance 脚本 | `run_codex_ocxlocal_routed_acceptance.py` + artifact | PR-2 → N2 | artifact 含 agent_id=codex、provider/model/protocol、route verified、匹配数、ledger/metrics、失败分类、恢复、隐私断言；不写真实 Codex 配置 | MUST |
| AGENT-2 | OpenCode 真实 routed-flow 当前候选重跑（GOAL G7 语义、PLAN G7） | 2026-08-04 G7 通过（candidate_id=G7） | 绑定 `C_final` 的重跑 | N3 | `C_final` 上真实 OpenCode 请求经 d-token 全链路；旧 G7 只作历史 | MUST |
| AGENT-3 | 无 chooser；`needs-source-fix`/`has_resolution_gap` 全链路（PRD P0.0.10.1–3、验收 42/43） | `needs-choice`/`selection_required` 残留于 presentation/installMachine/connectionSetup | clean cutover + 文案同步（spec v2.1 F-01、PLAN 1A.6） | PR-3 | 全仓生产无残留；cc-switch 未知 schema blocked；无 chooser 断言保留 | MUST |
| AGENT-4 | 23 Agent 检测/transport matrix 诚实（PLAN、PRD §12.4） | 注册表 23 项、11 个验证 adapter | 候选复验 + 新 Agent 变化复核 | N4 | detection-only 不冒充 configured/connected | MUST |
| AGENT-5 | cc-switch 只读 + A→B→A（GOAL G12、PRD 44） | `read_cc_switch_snapshot` 只读、未知 schema blocked；2026-07-31 静态 A→B→A 已识别 | 结构化 artifact harness + 真实 UI 切换 | PR-3（只读暴露）, PR-7（harness）→ N5 | artifact：route identity 前后相同、`wrote_cc_switch=false`、ledger events；真实切换由主代理 Computer 执行 | MUST |
| AGENT-6 | 配置复制/应用/复读/恢复验收方法（GOAL G12） | ConfigPatcher preview/apply/verify/rollback 已存在 | 隔离临时根 harness | PR-6 → N4 | 副本上 configure→reread→clear→exact restore；原文件仅 hash/权限证据 | MUST |
| AGENT-7 | MCP/Hook 诚实区分（PRD P0.0.10.6、验收 46） | Install MCP/Hook 分支已接线 | 候选复验 | N4 | MCP/Hook 可用不显示为 Active route | MUST |

## 5. CONFIG（配置事务）

| ID | 需求（来源） | current | gap | owner | acceptance | release claim |
|---|---|---|---|---|---|---|
| CONFIG-1 | 配置写入事务 detect→parse→plan→redacted diff→backup→confirm→apply→reread→verify→receipt；失败精确回滚（GOAL B5、INV-4） | ConfigPatcher/InstallationTransaction 已存在；Claude/Codex/Cline 真实临时事务已精确恢复 | 候选全链路复验 + 故障注入 | PR-6, N4 | 每个失败点可恢复原文件；receipt 脱敏 | MUST |
| CONFIG-2 | 原文件只取 hash/权限证据且不输出内容；不读 auth/密钥（GOAL G9/G12） | 2026-07-31 恢复 SHA-256 与 0600 权限 | harness 断言 | PR-6 | 断言脚本证明不读 auth、不输出内容 | MUST |
| CONFIG-3 | whole-batch reverse rollback（PRD P0.0.10.4、验收 44） | 批次事务设计已存在 | 多 Agent 批次失败注入测试 | PR-6 | 批次内任一失败 → 逆序回退已写入目标 | MUST |
| CONFIG-4 | 凭据只进 OS 凭据库；配置只保存不透明引用（INV-3、GOAL B4） | Keychain/Credential Manager 实现存在 | 候选 round-trip 证据 | PR-8, N7 | set/get/delete round-trip 唯一隔离 entry；无明文 fallback | MUST |

## 6. CONTEXT（LeanCTX / Context Bus / CCR）

| ID | 需求（来源） | current | gap | owner | acceptance | release claim |
|---|---|---|---|---|---|---|
| CONTEXT-1 | LeanCTX 九边界 Rust-owned 对齐层（PRD P0.0.5、GOAL K6） | 仅来源登记与行为对照（`code/runtimes/headroom/README.md`）；无真实跨 Agent 证据 | Context Bus / Session Cache / Context Relay / Query-aware Retrieval / Recovery Pointer / Stale-Superseded / SharedContext / 自命中防护 / 收益分离计量 9 项实现+证据 | PR-L1..L3 | 九边界逐项 runtime/eval/Agent 证据闭环；状态 partial→verified | BOUNDARY（PRD 标 v0.3 MUST；列入 v0.0.1 计划，默认不阻断，用户裁决） |
| CONTEXT-2 | CCR lifecycle：加密/TTL/配额/权限/retrieve/delete/崩溃恢复/精确删除（PRD P0.0.5、G3） | G3 实现证据：journal/reconciliation、Unix 权限、Windows DACL；G7 报告 CCR retrieve→delete→repeat-delete 通过 | 绑定 `C_final` + Windows MSVC 实测 | N1, N3, N8 | lifecycle 全链路通过且日志只存 metadata | MUST（P0.0.7 范围） |
| CONTEXT-3 | Context Relay 命中不得计入压缩收益；savings 分账（PRD §10、INV-7） | 契约已定 | 实现+断言 | PR-L2, PR-R3 | Relay 避免的重复传输不计入压缩收益，有测试 | BOUNDARY |

## 7. OPTICAL（视觉压缩）

| ID | 需求（来源） | current | gap | owner | acceptance | release claim |
|---|---|---|---|---|---|---|
| OPTICAL-1 | Beta 只 shadow/preview；production active 不可由产品 UI 打开（GOAL G10、PRD 12.5、INV-11） | `visual.config.activeForced`（allow_active=false）已实现；V3 评测 harness 存在 | 候选复验 + UI 断言 | N6 | UI 无 active 开关；active 默认空且关闭 | MUST |
| OPTICAL-2 | 不扩大为普通图片压缩（GOAL G10） | 范围已限定 | 边界断言 | PR-O3 | 无普通图片压缩入口 | MUST |
| OPTICAL-3 | pxpipe 评测配方 pin 重评估（旧 pin `7b7d6efa` 未评估；上游已发布 v0.12.0） | 法律声明记录旧 pin | 新 pin 评估 + capability matrix 刷新 | PR-O1 | pin 唯一；评估记录落盘；1928×1928 非生产 profile 声明保留 | BOUNDARY（参考轨，不阻断） |
| OPTICAL-4 | Spleen 5×8 字体数据来源/许可证/分发边界收口 | 2.2.0/BSD-2-Clause 已登记 | 渲染确定性 + 分发边界审计 | PR-O2 | 字体数据随分发的声明完整；renderer 跨平台确定性有测试 | MUST |
| OPTICAL-5 | active 门槛：renderer/font/profile hash、版本化 token 公式、eval digest、可信 authority（PRD 12.5） | active 默认关闭 | 门槛文档 + shadow 证据 | PR-O3 | active 不可达有测试；shadow 保留 raw ingress | MUST |
| OPTICAL-6 | shadow 保留 raw ingress；post-commit 永不自动原文重试（PRD 12.5） | 契约已定 | 候选复验 | N6 | 断言测试 | MUST |

## 8. OSS（开源轨道与 provenance）

| ID | 需求（来源） | current | gap | owner | acceptance | release claim |
|---|---|---|---|---|---|---|
| OSS-1 | 每个外部来源 pin 唯一且无漂移（GOAL G8/G9、[OSS_MIGRATION.md](./OSS_MIGRATION.md)） | ✅ Headroom D2 已闭合（PR-H1）：唯一 pin = `v0.34.0`/`9fd5ae3d`，旧基线 `01df2452` 标历史，main `7f6950be` 只作 post-release delta；仍待闭合：CBM 根 NOTICE v0.9.0 vs manifest v0.9.1-rc.1 漂移；pxpipe 旧 pin 未评估；lean-ctx v3.9.17 vs main 95a1b7bc；RTK v0.45.0 新克隆待审计 | 逐轨道 pin 决议 + 单一事实源 | PR-CBM1, PR-H1✅, PR-O1, PR-L0, PR-R1, PR-A | 全轨道 pin 唯一；漂移清单清零；结构检查通过 | MUST |
| OSS-2 | NOTICE/许可证副本随分发（GOAL G8、AGENTS.md） | root + docs/legal 双份 NOTICE；AiMaMi LICENSE 副本在仓 | 随包分发清单复验 | PR-A, N0 | SBOM/license 门通过且包含全部轨道声明 | MUST |
| OSS-3 | 外部仓库只读研究；不复制进生产；不新增运行依赖（GOAL G8、INV-4） | 本地副本全部在 `/Users/cc/Documents/d-token-external/`（仓库外） | 依赖扫描断言 | N0 | Cargo/npm 依赖无外部轨道运行时依赖；无复制源码 | MUST |
| OSS-4 | CBM v0.9.1-rc.1 固定 tag/commit + 平台 archive/binary SHA + Sigstore/Cosign；不得放宽校验（GOAL G9） | manifest 已 pin `ee9833df` + SHA；Cosign blob bundle 校验流程已定义 | 根 NOTICE 漂移修复 + Windows 收口实机 | PR-CBM1, PR-CBM2 | 根/内 notice 一致；四平台（macOS/Windows 各目标）archive+binary SHA 与 Cosign 校验证据 | MUST |

## 9. UI（界面/侧栏/可访问性）

| ID | 需求（来源） | current | gap | owner | acceptance | release claim |
|---|---|---|---|---|---|---|
| UI-1 | 顶部重复标题/留白修复（audit C1/C2） | Topbar 上下文标签与展开侧栏重复；Overview ~44pt 留白 | PR-4 | PR-4 | 折叠态显示上下文标签、展开态隐藏；留白回收；sr-only H1 保留 | MUST |
| UI-2 | 术语统一 + i18n key（audit C3/C4） | 活动/活跃术语不一；Changes/Doctor 硬编码 | PR-4 | PR-4 | Heatmap 术语统一；Changes/Doctor 用 i18n key | MUST |
| UI-3 | 920×640 无横滚 + Reduce Motion chevron（audit C6） | StatusBar `flex-nowrap+overflow-x-auto` | PR-4 | PR-4 → N6 | Playwright 真实 layout engine 断言 `scrollWidth <= clientWidth`；Reduce Motion 下无 transform | MUST |
| UI-4 | 侧栏专项（GOAL 侧栏 S1–S4） | 侧栏修复与小猫图标已落地（`docs/PLAN.md` 当前坐标 6：侧栏图标 enter/leave 可逆动效与点击 scale 按 PRD §12.5.7 落地；`docs/product/IMPLEMENTATION_STATUS.md` §6：折叠/Reduce Motion 关闭 transform；产品自有小猫图标 `code/apps/desktop/src/app/components/brand/BrandMark.tsx` 使用 `assets/dtoken-cat-icon-96.png`，接入 `components/shell/Sidebar.tsx`） | 真实 Tauri 窗口一轮复核 | N6 | S1–S4 全过；命中矩形/焦点/hover/点击一致；图标为产品自有 SVG | MUST |
| UI-5 | Analytics 日志高度无 diff（PR-4 边界） | 已压缩 | diff 断言 | PR-4 | PR-4 diff 不含 Analytics 日志高度 | MUST |
| UI-6 | 1180×780/920×640、浅/深、中/英、Reduce Motion 可读（INV-12） | 2026-08-07 视觉审计部分实测 | 全组合真实窗口 | N6 | 全组合通过；浏览器 preview 不冒充 | MUST |
| UI-7 | 代码图谱 experimental badge（audit C4） | badge 未实现 | PR-4 | PR-4 | nav/sidebar/i18n 双语可访问 badge | MUST |

## 10. SECURITY（安全）

| ID | 需求（来源） | current | gap | owner | acceptance | release claim |
|---|---|---|---|---|---|---|
| SECURITY-1 | 全部 production secret validation path 统一复用共享 SecretClassifier（PRD §12.4.18、AGENTS.md §4.6） | `dtoken-compress/src/optical/secret.rs` 共享实现存在（bearer/JWT/Google/GitHub/PEM/cookie/credential/high-entropy） | main.rs active/merged/route-draft 路径枚举与复用 | PR-5 | 无并行弱分类器；无 fork 副本；覆盖矩阵+回归单测 | MUST |
| SECURITY-2 | runtime JSON/CSV export 无绝对 config path 泄漏（GOAL B4） | core-api export 有 `redacted=true` 断言，path 泄漏无系统回归 | 修复+回归 | PR-5 | 含绝对路径 fixture 下 export 无泄漏 | MUST |
| SECURITY-3 | secret/token/cookie/auth 不进日志/配置/导出/提交（GOAL B4、INV-3） | grep 断言惯例已存在 | 候选 grep 断言脚本 | PR-5, N0 | 全仓断言通过；报告附输出 | MUST |
| SECURITY-4 | 不泄漏 prompt/response/源码/大段原始输出（GOAL B4） | 脱敏契约已定 | 候选断言 | N0 | 断言通过 | MUST |
| SECURITY-5 | 默认 loopback only（GOAL B3） | 已实现 | 门禁断言 | N0 | N0 断言通过（同 RUNTIME-3：`cargo test -p dtoken-proxy loopback_host_validation` + 非回环 bind grep 零命中） | MUST |
| SECURITY-6 | detected/configured/supported ≠ connected/verified/healthy/active（GOAL B6、INV-6） | fail-closed 状态模型已建立 | 候选 UI/API 复核 | N4, N6 | 无冒充 | MUST |
| SECURITY-7 | cc-switch 只读（GOAL B2、INV-11） | 只读实现 + 测试 | 候选复验 | PR-3, PR-7, N5 | `wrote_cc_switch=false` 断言 | MUST |
| SECURITY-8 | 进程停止前验证 PID/exe/端口/identity（INV-5） | 契约已定 | 候选复验 | N0 | 断言 | MUST |
| SECURITY-9 | no-touch 开发边界（fake-bin/poison-bin、临时 HOME/PATH/端口） | M1 基础已交付 | 候选 N0 复跑 | N0 | fixture/fake-bin smoke 通过；无真实调用 | MUST |

## 11. PLATFORM（平台/打包/签名）

| ID | 需求（来源） | current | gap | owner | acceptance | release claim |
|---|---|---|---|---|---|---|
| PLATFORM-1 | macOS 安装/升级/回滚/autostart/credential round-trip（GOAL K3、G6/N7） | 包审计 ad-hoc 通过（SHA-256 60f7d063…，未签名）；LaunchAgent 控制面已接通 | 候选 upgrade/rollback + round-trip 脚本与证据 | PR-8 → N7 | DMG 安装→升级→卸载/回滚 hash/权限证据；LaunchAgent/Keychain 唯一隔离 round-trip | MUST |
| PLATFORM-2 | Windows NSIS 安装/卸载/DACL/MSVC/G6 PowerShell（GOAL K1/K2、N8） | DACL 测试接入真实 runner；NSIS/upgrade/rollback 契约脚本存在；本机无 pwsh | 实机/runner 证据 | PR-8 → N8 | runner 通过报告；无则 external/blocked | MUST（外部凭据缺则标 external） |
| PLATFORM-3 | 双平台同 commit fan-in（G8） | `run_fan_in_artifact_contract.ps1` 已修复（同 commit 非空校验） | 真实 fan-in artifact | PR-7, N8 | fan-in ok=true 且 commit/digest 非空一致 | MUST |
| PLATFORM-4 | 签名三结论判定（PRD P0.2 12/13、FINAL_BLUEPRINT §2） | 判定规则已定义 | 最终候选执行 | PR-9 → N9 | 三结论之一落盘；unsigned 时 `distribution_ready_public=false`、`release_ready=false` | MUST |
| PLATFORM-5 | managed RTK：macOS probe + Windows/WSL 证据（PLAN 当前坐标 11、GOAL K6） | macOS probe 通过（receipt `support/artifacts/managed-rtk-candidate/candidate-20260731T223754Z-7536.json`）；RTK v0.45.0 待审计 | Windows/WSL runner + hook integrity + v0.45.0 pin | PR-R1, PR-R2 | 全平台 managed lifecycle 证据；命令过滤/exit 传播/usage metadata/精确卸载 | MUST |

## 12. EVIDENCE（证据治理）

| ID | 需求（来源） | current | gap | owner | acceptance | release claim |
|---|---|---|---|---|---|---|
| EVID-1 | 过期证据隔离 + candidate binding（GOAL §7、PRD P0.2 发布目标第 13 条/§19 验收标准第 32 条、INV-13） | 聚合器部分依赖 mtime/latest | `--expected-commit`+`--expected-source-digest` 强绑定 | PR-7, PR-9 | 旧 pass artifact 不可穿透；任一绑定缺失 blocked | MUST |
| EVID-2 | N0 完整候选质量门；修复产生新 commit 后完整重跑（FINAL_BLUEPRINT §8） | 概念已定 | 编排脚本 | PR-9 | dry-run + 候选变化失效测试 | MUST |
| EVID-3 | 机器验证 same commit + source digest（INV-13） | fan-in 已按 digest 锚定 | 全证据消费点接线 | PR-7, PR-9 | 所有机器消费 evidence 匹配 `C_final` | MUST |
| EVID-4 | 证据索引维护（本包 [EVIDENCE.md](./EVIDENCE.md)） | 本包建立 | 执行期滚动更新 | DOCS-3 | 每次切片报告同步索引 | N/A（过程） |
| EVID-5 | 真实 Tauri/Computer 证据（GOAL 侧栏 S4、INV-1） | 2026-08-07 原生截图部分组合 | N6 全组合 | N6 | 主代理 Computer 截图/坐标证据落盘 | MUST |
| EVID-6 | G0–G8 门禁聚合 fail-closed（GOAL B8） | 聚合器已 fail-closed；15 项契约测试（`support/tests/test_release_gate_status_aggregation.py` 实际 `def test_*` 数） | supplemental evidence 接线 | PR-7 | 缺任一 → exit 1；G7 语义不变 | MUST |
| EVID-7 | PR 治理：不同分支不同 PR、审查+返工、合并 main（GOAL G13、AGENTS.md §9） | 既有 PR 流程与 git 审计账本 | 执行期持续遵守 | Main | 每 PR 独立分支/worktree；审计记录完整；返工经审查 | MUST |

## 13. DOCS（文档/发布收口）

| ID | 需求（来源） | current | gap | owner | acceptance | release claim |
|---|---|---|---|---|---|---|
| DOCS-1 | v0.0.1 九文件计划包（本任务） | 本包已建 | 执行期保持同步 | Main | 九文件存在且无断链；`check_structure.py` 通过 | N/A（本包） |
| DOCS-2 | 入口更新：`docs/index.md`、`docs/DOCS_ORGANIZATION.md`、`docs/PLAN.md`（本任务） | 本任务同步修改 | — | Main | 入口指向本包；DOCS_ORGANIZATION 允许 `docs/releases/<version>`；PLAN 顶部标注本包定位 | N/A（本包） |
| DOCS-3 | 发布收口文档更新（GOAL 完成定义） | 历史快照 | 最终候选后同步 PRD 版本门/PLAN/IMPLEMENTATION_STATUS/README/发布报告 | PR-9 → N9 | 收口文档与 `C_final` 一致；本包滚动更新 | MUST |
| DOCS-4 | 版本统一后文档同步（VERSION-0 配套） | 版本未统一 | V-1 执行 | V-1 | 活跃文档无 0.2.0-beta.1 残留（除历史） | MUST |

## 14. 覆盖核对

- 用户 GOAL 13 项 → G1..G13 全部映射（见 [GOALS.md](./GOALS.md) 第 1 节表）。
- 侧栏专项 4 项 → UI-4；硬边界 8 项 → B1..B8 映射；发布阻断 6 项 → K1..K6 映射。
- PRD P0.0.7 发布门 MUST → PRODUCT-1/RUNTIME-1/AGENT-1/AGENT-2/PLATFORM-*；P0.0.10 硬合同 → AGENT-3/AGENT-5/AGENT-7/CONFIG-3。
- Headroom 完整迁移（PRD §8、PLAN 1C）→ CONTEXT-1/2、PR-H 切片、G0–G9（见 [VERIFICATION.md](./VERIFICATION.md#2-g0g9-门禁)），release claim 为 BOUNDARY（用户裁决）。
- LeanCTX 九边界 → CONTEXT-1 与 PR-L1..L3；pxpipe/Optical → OPTICAL-3/4 与 PR-O1/O2。
