# d-token 发布前审查：用户使用流程 + 整体性能与响应性

审查日期：2026-07-16
审查范围：跨前后端的**用户流程贯通**与**整体性能/响应性**（对照 PRD P0.6–P0.12、P0.2.2；PLAN §1A、§1B）
审查方式：只读源码审查 + 一次当前工作树 `npm run build`。**未运行真实 Tauri release build 实测**（冷启动、双平台、soak 等性能数字仍需实机证据）。
证据基准：以源码为准；区分"文档宣称"与"代码事实"，并区分"已有代码优化"与"已有可复现实测证据"。

---

## 1. 执行摘要

1. **用户主流程已高度成熟、基本闭环。** 七页信息架构、一键安装状态机（`installMachine.ts` 全 13 个 phase）、cc-switch 只读同步、智能学习后台 job（可取消/流式/缓存）、Doctor v3（扫描→规划→确认→执行→复检 + 证据不足 fail-closed）、变更与回退（两标签 + 完整性/可恢复性区分 + 反向备份预检）全部有真实 Tauri command 支撑，不是静态展示或 mock。PLAN §1A 记录的"没有 route 就强制进高级安装"这一核心偏差**已修复**（`blocked` 态把高级安装表述为可选回退，非强制）。
2. **性能架构已按 PRD P0.2.2 / PLAN §1B 做了系统性重构，且大部分 Perf Gate 0–2 已落地。** 页面切换串行退出动画（§1B.2 #1）已移除；Advanced 独立 3 秒轮询（§1B.2 #2）已删除；summary/detail 已在**前端和 native 双层**拆分（`get_desktop_summary` 轻量 + `get_desktop_data` 按需）；event-first + 单一兜底调度器（`KeyedRefreshScheduler`）+ visibility 感知 + single-flight 已实现；native 快照 300ms 缓存 + single-flight；loopback GET 快速失败（连接 120ms / 读 350ms）；初始递归 JS gzip **157.09 KiB**（过 160 KiB 门）；charts-vendor 严格 route-lazy 不进 entry；启动图标已降到 16KB。
3. **就绪度评分（分开）：**
   - **用户体验就绪度：8 / 10（接近内测可发布）。** 主流程可用、诚实空态、fail-closed 到位。扣分项：长列表无虚拟化只做硬截断（日志仅显示 80 行、无"加载更多"）；needs-credential 跳转到高级安装而非内联补录；VisualCompression 仍保留一处独立轮询。
   - **性能就绪度：6.5 / 10（架构达标，证据不足）。** 代码级优化扎实，但 PRD §19 #40 / PLAN §1B.7 #9 要求的"同一候选 commit、release build 的冷/暖启动、双平台、CPU/RSS/IPC、30 分钟/24 小时 soak"证据**尚未闭环**；现有实测只有 dirty working-tree + ad-hoc signed macOS build 的部分数据，且 selector store（Perf Gate 3）与真正虚拟列表未完成。**在补齐 release 实测前不得对外宣称"流畅/低占用/高性能"（这与 PRD §19 #40 一致）。**
4. **CRITICAL（发布阻断）：0 个**（纯流程/性能维度；发布证据类阻断项属发布 Gate，不计入本报告 CRITICAL）。
5. **HIGH：4 个；MEDIUM：6 个；LOW：4 个**（详见 §4）。

---

## 2. 用户旅程全景

逐主流程贯通度评估（证据均来自当前源码）：

| 流程 | 是否闭环 | 主要断点/风险 | 关键证据 |
|---|---|---|---|
| 冷启动到可用（Overview 首屏） | 是 | 启动即拉 `discover_upstream_candidates`（app 级 provider），与首屏抢资源；但走调度器+single-flight，影响有限 | `App.tsx:11-18`；`connectionSetup.tsx:84-161`；Overview 用 summary（含 `recent_request_logs(20)`+aggregate），首屏有真实数据 `lib.rs:6186-6187` |
| 一键安装状态机 | 是 | 单一主 CTA；`blocked` 态高级安装为**可选回退**（已修复 §1A 偏差） | `installMachine.ts:207-243`；`Install.tsx:487-494, 553-556`；`presentInstallPhase blocked:946` |
| — already-connected 幂等 | 是 | 相同配置不重复写 | `installMachine.ts:211`；`Install.tsx:492` verifyConnection |
| — needs-choice | 是 | 内联 `<select>` 选已发现上游，不强制手填 URL | `Install.tsx:852-867` |
| — needs-credential | 部分 | **跳转高级安装补录**（非内联），落到已预填 base_url 的表单，但仍是换页 | `Install.tsx:491`；`Advanced.tsx:399-419` |
| — 多 Agent 原子回退 | 是 | 逐 Agent apply→verify，失败后逆序 rollback | `installMachine.ts:264-326` |
| 自动上游发现（Agent 级 binding） | 是 | 已形成 `AgentUpstreamBinding` + `resolution`；发现源 native 版本化 | `connectionSetup.tsx:32-68`；`upstream_discovery.rs` |
| cc-switch 只读同步 | 是 | UI 显示 profile/upstream/连接路径/synced 状态 | `Install.tsx:658-668, 887-892` |
| 智能学习扫描 | 是 | 后台 job + 取消 + 流式 emit（按 adapter/阶段，非按文件）+ SQLite 缓存 + checkpoint | `learning_discovery.rs:271-283, 384-432, 2181-2186` |
| — 首批候选/取消响应 | 是（代码） | 取消 <250ms（有测试）；首批 1.5s 目标需实测 | `learning_discovery.rs:2510-2517` |
| 分析页 | 是 | 游标分页 `get_analytics_request_page`（limit 1–100）；估算标注 | `main.rs:10371-10400` |
| Doctor 闭环 | 是 | 扫描→规划→确认→执行→复检分阶段；证据不足全锁定 fail-closed；主/次按钮正确 | `Doctor.tsx:240, 516-521, 674, 691, 827` |
| 变更与回退 | 是 | 两标签默认 backup；完整性 vs 可恢复性区分；预览→确认→反向备份 | `Changes.tsx:79, 118-147`；`CHANGES_TABS` |
| 视觉压缩 Beta | 是 | 一键/高级/状态三区；shadow-only；Beta 徽标 | `VisualCompression.tsx`（1612 行） |
| 横切：假成功/死按钮 | 良好 | 未发现"永远 unavailable 的装饰按钮"；mutation 后 event 失效刷新 | `native.ts:74-104`（MUTATING_COMMANDS 自动 invalidate） |
| 横切：文案规范 | 良好 | 生产 UI 未泄漏 payload/benchmark/fixture 等；有测试主动断言无泄漏 | `VisualCompression.test.tsx:197`；`LogsSecurity.test.tsx:45` |

**结论：** 用户能顺畅走完全部主流程，无"死路/永远 unavailable 装饰按钮/点击无反馈"级别的阻断。少数体验粗糙点（needs-credential 换页、日志硬截断）为打磨项，非阻断。

---

## 3. 性能现状全景

### 3.1 性能预算达成度总表（目标 vs 当前证据 vs gap）

对照 PRD P0.2.2 与 PLAN §1B.4：

| # | 预算目标（PRD P0.2.2 / PLAN §1B.4） | 当前代码事实 | 实测证据状态 | Gap / 结论 |
|---|---|---|---|---|
| 1 | 点击/切页/主 CTA 100ms 内反馈 | 页面切换 `key={page}` 立即换挂载（无 AnimatePresence 串行）；按钮点击立即进 pending；mutation 后 event 失效 | 无 release 实测 | 架构达标，数字待实测 |
| 2 | 已加载页切换 p95 ≤ 250ms；退出动画不阻塞 | `AppShell` 无 `AnimatePresence mode="wait"`；enter 仅 180ms opacity/y；`exit` prop 无 AnimatePresence 不生效 | 无 release 实测 | **串行退出已消除**；数字待实测 |
| 3 | 冷启动 ≤ 1.5s / 暖启动 ≤ 800ms（p95） | lazy 页面 + entry 157KiB gzip | **未闭环**（PLAN §1B.6 Gate0 明确冷/暖启动未做） | 需 release 实测 |
| 4 | 在线 summary 刷新 ≤ 250ms；离线 fail-fast ≤ 500ms | GET 连接 120ms/读 350ms/写 200ms；native 300ms 缓存 | PLAN 记录 dirty build `/dashboard/summary` 50 次 p95 7.6ms | 达标（但非 release 候选证据） |
| 5 | 无 >100ms JS long task；进度 ≤10/秒 | learning emit 按 adapter/阶段（~6+几次），远低于 10/秒 | 无 WebKit Long Task 实测（PLAN 承认未做） | 架构达标，long task 未实测 |
| 6 | 空闲 CPU ≤ 1%；隐藏停轮询 | 调度器 visibility hidden 时 `stopEntry` 全停 | PLAN 记录 dirty build 30s 窗口平均 CPU 0.23%（不含 WebKit helper） | 部分证据，非完整进程组/soak |
| 7 | 30 分钟空闲无内存单调增长（漂移<5%） | worker/watcher/timer 有 release 路径 | **未做**（无 30 分钟/24 小时 soak） | 需 soak 实测 |
| 8 | 初始 JS gzip ≤ 160KB；重依赖 route-lazy | **实测 157.09 KiB**（`check:bundle` 门内置于 build）；charts-vendor 独立 lazy | 本次 build 实测 ✅ | **达标**（余量仅 ~3KB） |
| 9 | 单页业务 chunk gzip ≤ 50KB | Doctor 23KB / InstallWorkbench 19KB / Advanced 16.6KB / VisualCompression 16KB gzip | 本次 build 实测 ✅ | **达标** |
| 10 | 1万日志/2万 session 只渲染可见行 | **无虚拟化**；日志硬截断 `.slice(0,80)`；候选 `.slice(0,8/16)` | 未做 stress fixture 渲染实测 | **未达标**（靠截断避免冻结，非虚拟化；且看不全） |
| 11 | 首批候选 1.5s；取消 250ms | 取消 <250ms（有单测） | 首批 1.5s 无 release 实测 | 取消达标；首批待实测 |
| 12 | 所有 Native command 记录 duration/result/count | `record_native_performance` + 前端 `recordPerformanceDuration` 全覆盖 | 有 256 条有界 timing window | **达标（观测已建）** |

### 3.2 关键瓶颈（按影响排序）

1. **单一大 Context 无 selector（Perf Gate 3 未完成）。** 运行证据仍走 `RuntimeEvidenceContext` 单一 Context（`useRuntimeDashboard.ts:73, 249-252`），任一 slice 变化→所有消费者（含 StatusBar 与当前页）重算。PLAN §1B.2 #4 / §1B.6 Gate3 自认"partial"。数据量增大后是首要 React 工作量来源。
2. **长列表无真正虚拟化。** 全仓无 `react-window/react-virtual/virtuoso`；日志 `.slice(0,80)`、学习候选/Doctor 直接全量渲染 + 硬截断。PLAN §1B.4 #10 目标未满足，同时造成"看不全"的功能限制。
3. **release 性能证据缺口。** 冷/暖启动、完整进程组 CPU/RSS/IPC、WebKit Long Task、30 分钟/24 小时 soak、Windows 基线全未闭环（PLAN §1B.6 Gate0 明示）。这是"性能就绪度"评分被压到 6.5 的主因。
4. **main.rs 13,007 行（PLAN §1B.2 #8 记为 ~6,000）。** 阻塞 IO / command 注册集中，已翻倍增长，独立测量、限制并发和定位慢调用更困难。非用户直接感知，但拖累后续性能治理与回归定位。
5. **app 级 discovery 在启动即运行。** `ConnectionSetupProvider` 在 Overview 也会跑 `discover_upstream_candidates`（可能调 provider CLI），与首屏争资源；有调度器+single-flight+60s 兜底缓解，但冷启动 CPU 会被推高。

---

## 4. 分级问题清单

> CRITICAL 仅用于真正阻断内测发布的问题。纯"发布证据未闭环"归入发布 Gate（见 §4.3），不重复计为 CRITICAL。

### 4.1 流程 / 体验

**HIGH-1 长列表无虚拟化 + 硬截断导致"看不全"（兼性能）**
- 证据：`LogsSecurity.tsx:700` `rows.sort(...).slice(0, 80)`；`SmartLearning.tsx:612,663` `.slice(0,8/16)`；全仓无虚拟化库。
- 影响：用户日志只能看最近 80 条，无"加载更多"/分页入口；智能学习候选、Doctor 结果同样受限。既是体验缺陷（信息不可达），也是 PLAN §1B.4 #10 性能预算未达标。
- 根因：以"截断"替代"虚拟化+分页"。
- 建议：日志/事件接 `get_analytics_request_page` 式游标分页或引入轻量虚拟列表（`@tanstack/react-virtual`，gzip ~3KB，可懒加载不进 entry）；至少提供"加载更多"。

**HIGH-2 needs-credential 跳转高级安装而非内联补录**
- 证据：`Install.tsx:491` `if (machine.phase === "needs-credential") return onOpenAdvanced?.()`。
- 影响：PRD P0.7.1 #4 要求"只请求缺失凭据，不要求重复填写已识别 URL/协议/模型"。当前把用户推到高级安装页（`Advanced.tsx` 虽预填 base_url，line 399-419），仍是换页 + 面对完整高级表单，与"普通用户最多处理一次凭据"的目标有落差。
- 根因：一键安装区未内置最小凭据录入弹窗。
- 建议：在一键安装区加一个"仅补录凭据"的 `OperationDialog`（复用 `store_provider_credential`），补录后回到 `ready`，避免换页。

### 4.2 性能

**HIGH-3 单一大 Context 无 selector 细粒度订阅（Perf Gate 3 未完成）**
- 证据：`useRuntimeDashboard.ts:73`（单 Context）、`249-252`（整对象 memo）；PLAN §1B.6 Gate3 自认 partial。
- 影响：任一运行 slice 变化触发全体消费者重渲染（Overview/StatusBar/当前页），是 PRD P0.2.2 #8、PLAN §1B.4 长列表增长后卡顿的结构性来源。
- 根因：尚未迁移到 selector store / `useSyncExternalStore` slice 订阅（install 机已用 `useSyncExternalStore`，可作范式）。
- 建议：把 `operational` 各 slice 拆为独立可订阅单元（slice revision 已在 native 侧就绪，`main.rs:10253` slice_revisions），页面只订阅所需 slice。

**HIGH-4 release 性能证据未闭环（同时是发布 Gate）**
- 证据：PLAN §1B.6 Gate0 "冷/暖启动、完整进程组 CPU/RSS/IPC、WebKit Long Task、soak、Developer ID/notarization 与 Windows 基线未完成"；PRD §19 #40。
- 影响：无法证明 P0.2.2 #3/#6/#7/#11 达标；按 PRD §19 #40 不得宣称"流畅/低占用/高性能"。
- 建议：见 §5 阶段 D。

**MEDIUM-1 VisualCompression 仍有独立固定轮询**
- 证据：`VisualCompression.tsx:74` `REFRESH_MS=20_000`、`377` `window.setInterval(...)`（visibility 门控）。
- 影响：与 PLAN §1B.3 A #2"页面不得各自建立固定轮询"相悖（虽已 visibility 门控 + 20s，影响小）。
- 建议：迁移到 `appRefreshScheduler` + event 失效（PLAN §1B.1 #9 已列为待迁移）。

**MEDIUM-2 app 级 discovery 在非安装页也启动**
- 证据：`connectionSetup.tsx:84-87`（Provider 包裹全 App，mount 即 `performRefresh`）。
- 影响：Overview 冷启动即跑上游发现（可能调 provider CLI），推高冷启动 CPU/耗时。
- 建议：把 discovery 激活收敛到进入"安装与连接"页时（类似 detail 的 `activate()` 引用计数模式），Overview 只显示 summary。

**MEDIUM-3 main.rs 13k 行，阻塞 IO/command 集中**
- 证据：`wc -l src/main.rs` = 13007（PLAN §1B.2 #8 记 ~6000，已翻倍）。
- 影响：难以独立测量/限制并发/定位慢 command；后续性能回归排障成本高。
- 建议：按域拆分（loopback client / desktop snapshot / install / doctor bridge / learning 已独立），把 command handler 与阻塞 IO 分层。

**MEDIUM-4 detail 激活的页面在后台仍订阅失效事件**
- 证据：`useRuntimeDashboard.ts:318-330` detail 在 `enabled` 期间监听 `RUNTIME_INVALIDATED_EVENT`；多个 detail 消费页（Changes/Advanced/LogsSecurity/Compatibility/SmartLearning）共享 `detailConsumers` 引用计数，但一次 mutation 会触发一次 `get_desktop_data`（重）。
- 影响：在重 payload 未做 cursor/delta 前（PLAN Gate2 未完成项），频繁 mutation 会重复拉全量 detail。
- 建议：detail 也接 revision 短路（summary 已有 `desktop_revision` 门控，detail 可复用），并推进 cursor/delta。

### 4.3 发布 Gate（证据类，非本报告 CRITICAL，但发布前必须满足）

- **MEDIUM-5** 冷/暖启动、双平台、CPU/RSS/IPC、soak 实测缺失（PRD §19 #40，PLAN §1B.7 #9）。
- **MEDIUM-6** Windows 平台性能与 E2E 基线缺失（PLAN §1B.6 Gate5 #3）。

### 4.4 LOW

- **LOW-1** 281KB `dtoken-cat-icon.png` 已完全无引用（`rg` 零命中），应删除以免误入未来关键路径。
- **LOW-2** `Compatibility.tsx:139` 英文 "No local payload passed..." 用户可见串含 `payload`（工作台子页、仅英文、影响小）。
- **LOW-3** entry gzip 157.09KiB 距 160KiB 门仅 ~3KB 余量；任何新增共享依赖易越界，需在 PR 门禁提示。
- **LOW-4** `PageStage.tsx:70-77` 定义了 `exit` 动画但无 `AnimatePresence` 包裹，`exit` 实际不生效——属无害死代码，建议移除以免误导后续开发者重新引入串行等待。

---

## 5. 修改计划（分阶段，含依赖顺序与 DoD）

> 原则：先做用户可感知且低风险的体验/感知性能修复，再做结构性 React 治理，最后补 release 实测证据。阶段 A/B 无数据依赖，可并行；C 依赖 B 的 slice 订阅；D 依赖前三者定稿的候选 commit。

**阶段 A（体验闭环，并行）**
- A1 一键安装内联凭据补录弹窗（HIGH-2）。涉及 `Install.tsx`、复用 `store_provider_credential`、`installMachine` needs-credential→ready 转移。
- A2 日志/事件"加载更多"或游标分页（HIGH-1 前半）。涉及 `LogsSecurity.tsx`、可复用 `get_analytics_request_page` 模式。
- A3 删除无引用 281KB PNG（LOW-1）、移除 `PageStage` 死 `exit`（LOW-4）、修正 Compatibility 英文串（LOW-2）。
- DoD：needs-credential 全程不换页；日志可翻页看全；`rg dtoken-cat-icon.png` 零命中且 build 无该资产。

**阶段 B（感知性能收敛，并行）**
- B1 VisualCompression 迁移到 `appRefreshScheduler` + event 失效（MEDIUM-1）。
- B2 discovery 激活收敛到安装页（MEDIUM-2，引用计数模式）。
- DoD：生产 src 内 `setInterval` 归零；Overview 冷启动不触发 `discover_upstream_candidates`。

**阶段 C（React 结构治理，依赖 B）**
- C1 引入轻量虚拟列表（`@tanstack/react-virtual`，route-lazy）替换硬截断（HIGH-1 后半 / MEDIUM）。
- C2 selector store / slice 订阅迁移（HIGH-3），复用 native `slice_revisions`；detail revision 短路（MEDIUM-4）。
- C3（可选）main.rs 按域拆分（MEDIUM-3）。
- DoD：1万日志 fixture 下只渲染可见行、滚动不冻结；React Profiler 显示无关页面在单 slice 变化时不重渲染；entry gzip 不增（虚拟库须在 lazy chunk）。

**阶段 D（release 证据闭环，依赖 A–C 定稿 commit）**
- D1 同一候选 commit 的 macOS release build 冷/暖启动 p50/p95、完整进程组 CPU/RSS/IPC、WebKit Long Task、30 分钟 soak（HIGH-4 / MEDIUM-5）。
- D2 Windows 基线（MEDIUM-6）。
- DoD：达到 PRD P0.2.2 / PLAN §1B.7 全部数字后方可宣称"流畅/低占用"；未达标则更新文案口径。

---

## 6. 最佳迭代方案（发布前推荐落地路径与取舍）

1. **把"内测可发布"与"性能声明可发布"分成两道门。** 当前用户体验已达内测水平；建议以阶段 A + B 为**内测发布必做**（低风险、可感知），阶段 C 的 selector/虚拟化为**内测强烈建议**（数据量小的内测环境可延后 C1 的虚拟化，但 HIGH-3 selector 建议进内测因其是结构性隐患），阶段 D 为**公开"高性能"声明前必做**。
2. **并行机会：** A（前端 UI）、B（调度收敛）、D 的 Windows 桥接彼此独立，可并行；C2（selector）与 C1（虚拟化）都依赖 slice 边界，宜同一波做，避免二次重构。
3. **取舍建议：**
   - 长列表优先"游标分页 + 加载更多"（复用已有 `get_analytics_request_page` 范式，改动小、风险低），虚拟化作为第二步（引入依赖需守住 160KiB 门）。二者取其一即可解 HIGH-1 的"看不全"，虚拟化额外解性能预算 #10。
   - selector 迁移不必一次性替换全 Context：可先把 **StatusBar + Overview**（最高频重渲染）迁到 slice 订阅，收益最大、风险最小。
   - main.rs 拆分收益在"可维护/可测量"，非用户可感知；建议排在体验与 selector 之后，避免大规模改动引入回归。
4. **文案纪律：** 在阶段 D 完成前，所有对外材料按 PRD §19 #40 避免"流畅/低占用/高性能"绝对表述，改用"本地优先、事件驱动、按需加载"等可核验描述。

---

## 7. 附录

### 7.1 审查方法
- 精读 PRD P0.1–P0.12（尤其 P0.2.2、P0.6–P0.11）、PLAN §1A（自动上游/智能学习）、§1B（全 App 性能治理，逐 Perf Gate）。
- 源码审查：`App.tsx`、`context.tsx`、`refreshScheduler.ts`、`useRuntimeDashboard.ts`、`connectionSetup.tsx`、`installMachine.ts`、`AppShell.tsx`、`PageStage.tsx`、`native.ts`、`Install.tsx`、`Advanced.tsx`、`Changes.tsx`、`Doctor.tsx`、`VisualCompression.tsx`、`Overview.tsx`；native `main.rs`（summary/detail/cache/loopback）、`learning_discovery.rs`（job/cancel/emit）、`upstream_discovery.rs`；core `dtoken-core-api/src/lib.rs`（`dashboard_summary_for_state`）。
- 一次当前工作树 `npm run build`（3.62s）取真实 bundle 数字 + `check:bundle` 门结果。
- `rg` 交叉验证：虚拟化库缺失、`setInterval` 分布、forbidden dev 术语泄漏、PNG 引用、AnimatePresence 使用点。

### 7.2 覆盖范围
- 覆盖：七页用户流程贯通、感知性能（切页/按钮/动画）、IPC/刷新架构（summary vs detail、event-first、调度器、visibility）、native 快照缓存与 loopback 超时、learning job 响应性、bundle/资产、文案规范横切。
- 按边界要求，**未重复**前端组件级设计系统细节（前端报告）与后端 crate 能力完整性（后端报告），仅从流程/性能视角引用。

### 7.3 未覆盖 / 需进一步验证（须真实 Tauri release build 实测）
1. 冷启动/暖启动 p50/p95（PRD P0.2.2 #3）。
2. 完整进程组（含 WebKit helper）CPU/RSS（P0.2.2 #6/#11）。
3. WebKit Long Task > 100ms 实测（P0.2.2 #10）。
4. 30 分钟 / 24 小时 soak 内存漂移（P0.2.2 #7）。
5. 1万日志 / 2万 session metadata stress fixture 下的实际渲染/滚动/过滤帧率（P0.2.2 #8、PLAN §1B.4 #10）。
6. Windows 平台全部上述指标（PLAN §1B.6 Gate5 #3）。
7. summary/detail 端到端（Tauri→WebKit）p95（PLAN 记录 core 内部 µs 级，但端到端未实测）。
