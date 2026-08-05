# 前端 UI 与前后端对接审查报告

- 报告日期：2026-07-16
- 审查范围：`code/apps/desktop/src/`（生产前端代码 + Tauri invoke 对接前端侧）
- 排除：`code/runtimes/headroom/`（上游 Python）、`code/UI design for project/`（设计原型）、`code/apps/desktop/src-old/`（旧实现）、`support/`（测试/eval）
- 真理源：`docs/PRD.md`（§12.5、P0.2.1、P0.5、P0.12）、`docs/APP_FUNCTIONS_AND_PAGES.md`、`docs/product/IMPLEMENTATION_STATUS.md`
- 审查方式：只读审查。逐项对照 PRD 契约核查源码，所有论断标注 `file:line` 证据。

---

## 1. 执行摘要

d-token 桌面前端的**信息架构、设计系统一致性、统一状态模型、真实性空态、可访问性与前后端对接契约**已达到远超"内部 demo"的完成度，是一个纪律性极强、契约驱动的前端工程。7 个一级页面与 PRD P0.5 完全一致；`EvidenceSlice` 统一状态模型（`OperationalSlice<T>`）真实落地并被单一 App 级 store 拥有；`—`/verified-empty/unavailable/stale/partial 语义严格区分；无 `window.confirm/alert/prompt`、无 fixture/mock 数据、无 `Date.now()` 造时间、无 `as any`、无 TODO/FIXME、UI 文案无工程术语；脱敏、通知去重、签名更新门禁、键盘可访问性均已实现。

阻断内测发布的 **CRITICAL 问题为 0**。发现的问题集中在 **代码质量维护性（超大文件、`Record<string, any>` 未类型化的对接边界）** 与 **少量一致性细节**，均为 HIGH/MEDIUM/LOW，不影响功能闭环与安全边界。

**前端发布就绪度评分：A- / 88 分。**

评分依据：
- 架构与契约落地（+）：统一状态、单一所有者、event-first、真实性空态、脱敏、可访问性、动效契约全部达标，质量在同类桌面 App 中属上游。
- 扣分项（-）：5 个页面组件超 800 行上限（Doctor 1773 行为最）违反自身编码规范，长期维护与回归风险偏高；前后端对接边界完全依赖 `Record<string, any>` + 手写防御式解析，缺少 schema 校验层（109 处 `any` 类型标注），是最大的类型安全债务；少量硬编码图表色未走 token。
- 未在本报告扣分但需第三份报告确认：Native payload 的运行证据链贯通、真实 routed 流程、release build 性能（属报告三/后端范围）。

> 说明：本前端就绪度评分只针对"前端怎么实现的 + 对接契约的前端侧"。真实功能是否端到端生效（安装/路由/压缩/Doctor/回退）仍需 Tauri command 内部实现、runtime 证据与真实 Agent flow 证明（PRD 12.5.11、APP_FUNCTIONS_AND_PAGES §8），这属于后端报告与流程报告范围。

---

## 2. 现状全景

### 2.1 文件规模

- 118 个 `.tsx`、39 个 `.ts`（非测试）、38 个测试文件（28 组件/shell + 10 lib + e2e）。
- 一级页面代码行数：Doctor 1773、VisualCompression 1612、Advanced 1332、Install 1191、Overview 980、Analytics 800、SmartLearning 790、Settings 766、LogsSecurity 763、Compatibility 482、Changes 473、InstallWorkbench 121。

### 2.2 分层架构（清晰、健康）

```
App.tsx
 └─ AppProviders (theme-context.tsx: next-themes + AppearanceProvider + I18nContext)
     └─ RuntimeEvidenceProvider (useRuntimeDashboard.ts: App 级运行证据 store)
         └─ ConnectionSetupProvider (connectionSetup.tsx: 上游发现 store)
             └─ NotificationProvider (notifications.tsx: 唯一通知 store)
                 ├─ RuntimeNotificationBridge (运行状态 → 通知)
                 └─ AppShell (shell/: Sidebar + Topbar + PageStage + StatusBar)
                     └─ 页面 (components/pages/*.tsx)
                         └─ lib/*/client.ts 等 invoke 封装层
                             └─ native.ts (invokeNative → @tauri-apps/api/core)
```

- **Provider 层次**（`src/app/App.tsx:8-21`）：外观、运行证据、连接发现、通知四个单一所有者按序注入，符合 P0.2.1.6"同一事实只允许一个所有者"。
- **invoke 封装层**：页面不直接调 `invokeNative`，而经 `lib/optical/client.ts`（9 处）、`lib/optical/agentIntegration.ts`（8 处）等类型化包装函数；核心读取集中在 `useRuntimeDashboard.ts`（4 处）与 `connectionSetup.tsx`（2 处）。页面内的 `invokeNative` 只用于该页专属的写操作或按需分页读取。

### 2.3 状态模型（PRD P0.2.1 核心契约的真实落地）

- `OperationalSlice<T> = { data, source, state, freshness, collectedAt, error }`（`src/app/lib/useRuntimeDashboard.ts:19-26`）——与 PRD `EvidenceSlice<T>`（PRD.md:41-49）字段一一对应（仅 `revision` 未进入 slice，改由 payload `desktop_revision` 承载）。
- `RuntimeOperationalSnapshot` 覆盖 service / dashboard / traffic / routes / transactions / doctor 六个切片（`useRuntimeDashboard.ts:28-36`），满足 P0.2.1.1 的核心覆盖集合。
- `evidence.ts` 提供 `classifyEvidence()` 分类器（loading/verified/preview/unavailable/stale/partial），带 `requireRuntimeIdentity`/`requireTransactionProvenance` 门槛（`src/app/lib/state/evidence.ts:10-52`）。

---

## 3. 分级问题清单

### CRITICAL（发布阻断）

**无。** 未发现阻断内测发布的前端问题。所有 PRD 明确禁止项（`window.confirm/alert/prompt`、fixture/mock 冒充完成态、`Date.now()` 造时间、假绿态、工程术语暴露、非法顶级导航、玻璃卡片堆叠/流动渐变背景）均已合规。

---

### HIGH

#### H-1. 5 个一级页面组件超 800 行上限，违反自身编码规范，回归与维护风险高

- **证据**：
  - `src/app/components/pages/Doctor.tsx` 1773 行
  - `src/app/components/pages/VisualCompression.tsx` 1612 行
  - `src/app/components/pages/Advanced.tsx` 1332 行
  - `src/app/components/pages/Install.tsx` 1191 行
  - `src/app/components/pages/Overview.tsx` 980 行
  - （`Analytics.tsx` 800 行，恰在边界）
- **影响**：单文件既含 UI 渲染、又含大量派生逻辑（如 `Overview.tsx` 内 `overviewFromPayload`/`deriveOverviewOperationalSummary`/`deriveAccountQuotaSummary` 共 500+ 行纯逻辑）。文件过大导致：评审困难、局部改动的连带影响面大、Suspense/lazy 拆分收益被抵消、单测覆盖某一行为时必须加载整页。违反 `~/.claude/rules` coding-style "200-400 行典型，800 max" 与项目 CLAUDE.md 同款规范。
- **根因**：页面把"数据派生（payload → view model）"与"呈现"耦合在同一文件，没有把纯函数 view-model 抽到 `lib/`。
- **建议**：
  1. 把每个大页面的纯派生逻辑抽到 `lib/<page>/model.ts`（例：`Overview.tsx` 的 `overviewFromPayload`、`deriveOverviewOperationalSummary`、`deriveAccountQuotaSummary`、`buildActivityDaysFromLogs` 等移入 `lib/overview/model.ts`，页面只保留 JSX + hooks）。这些函数已是导出的纯函数（`Overview.tsx:183,449,765`），迁移风险低且能被现有单测直接复用。
  2. Doctor/VisualCompression 按上栏标签或子区域拆分子组件（Doctor 的 module registry 渲染、agent 配置 Dialog、v3 plan 详情各自成文件）。
  3. 目标：每个页面主文件 ≤ 500 行，纯逻辑文件 ≤ 400 行。

#### H-2. 前后端对接边界完全未类型化，109 处 `Record<string, any>`/`any`，无 schema 校验层

- **证据**：
  - `invokeNative<Record<string, any>>("get_desktop_summary")`、`invokeNative<Record<string, any>>("get_desktop_data")`（`src/app/lib/useRuntimeDashboard.ts:142-143,271`）——native 返回的核心 payload 类型为 `Record<string, any>`。
  - 全前端生产代码 `: any` / `Record<string, any>` / `any[]` 共 **109 处**（跨 12+ 文件：`StatusBar.tsx`、`Overview.tsx`、`Doctor.tsx`、`Advanced.tsx`、`useRuntimeDashboard.ts` 等）。
  - `RuntimeOperationalSnapshot` 内多个切片为 `OperationalSlice<Record<string, any>>` / `OperationalSlice<any[]>`（`useRuntimeDashboard.ts:29-34`）。
- **影响**：这是整个前端最大的类型安全债务。整个运行证据链（`desktop_summary` / `desktop_data`）在前端是"无类型字典 + 手写防御式解析"（`objectValue`/`safeText`/`numberEvidence`/`firstPopulatedArray`，`Overview.tsx:709-753`）。后果：
  - 后端 command 契约变更（字段改名、结构调整）时前端**编译期零告警**，只会在运行时静默降级为 `—`/partial——违反 P0.2.1 精神（对接契约应可验证）。
  - 大量重复的手写取值/容错逻辑散落各页面，是 DRY 与可维护性隐患。
  - 无运行时 schema 校验（未使用 Zod 等），恶意/损坏 payload 只靠 `classifyEvidence` 的启发式识别。
- **根因**：Rust `get_desktop_data` 的返回结构未在前端建立对应 TypeScript 类型/Zod schema；`lib/runtime/types.ts` 存在但未覆盖 desktop payload 主结构。
- **建议**：
  1. 为 `desktop_summary`/`desktop_data` 建立**单一权威 TypeScript 类型**（放 `lib/runtime/types.ts`），与 Rust `transaction_provenance`/`desktop_revision`/`request_logs`/`providers`/`routes` 等字段对齐。
  2. 引入 **Zod schema**（rules/typescript 明确推荐）在 `invokeNative` 边界解析一次：`DesktopDataSchema.safeParse(raw)`，失败即进入 `partial`/`error` 状态并附可读原因，替代当前散落各页的防御式取值。
  3. 收敛后，页面 view-model 消费强类型对象，可删除大部分 `objectValue`/`safeText` 样板。
  4. 这与 H-1 可一并推进：抽 view-model 时同步引入类型。

#### H-3. `useRuntimeDashboard.ts` / `Overview.tsx` 复杂度偏高，切片派生逻辑集中且深

- **证据**：
  - `buildOperationalSnapshot`（`useRuntimeDashboard.ts:352-422`）单函数 70 行，含 stale 保留、slice 错误映射、多字段回退分支。
  - `deriveOverviewOperationalSummary`（`Overview.tsx:449-654`）单函数 200+ 行，`agents`/`route`/`issues`/`ccSwitch` 状态各有 5-8 层三元/条件分支。
- **影响**：这些是状态一致性的核心枢纽，逻辑正确但认知复杂度高，深层三元嵌套超过 rules 的"避免 >4 层嵌套"。虽有 Overview.test.tsx 17 个行为用例护栏（`Overview.test.tsx:82-217`），但新增状态时易引入回归。
- **根因**：把"多来源证据融合（runtime payload + connectionSetup snapshot）→ 用户可读状态"的所有分支写在单函数。
- **建议**：把 `agents`/`route`/`ccSwitch`/`issues` 各自拆为独立 `deriveXxxStatus` 纯函数（`deriveCcSwitchStatus` 已是范例，`Overview.tsx:656`），用早返回替代深三元；每个分支补最小单测。不改行为，仅降复杂度。

---

### MEDIUM

#### M-1. 图表/装饰色存在硬编码 hex，未完全走主题 token

- **证据**：
  - `Heatmap.tsx` 工具排行 `color: index % 2 === 0 ? "var(--chart-accent, #f97316)" : "#68c7ee"`（`src/app/components/common/Heatmap.tsx:445`）——第二色 `#68c7ee` 为裸 hex。
  - `theme.css` token-donut ring `background: conic-gradient(#68c7ee 0 99%, #10b981 99% 100%)`（`src/styles/theme.css:1193`）——两个数据色硬编码。
- **影响**：违反 rules "禁止硬编码值（使用常量或 token）"。用户在设置中切换 chart accent（`theme-context.tsx:38-45` 支持 6 种）时，这些硬编码色不会跟随，深浅色下对比度也未验证。属视觉一致性缺陷，非功能阻断。
- **根因**：图表色只部分接入 `--chart-accent`，副色未定义 token。
- **建议**：定义 `--chart-accent-2`（及必要的语义色 token）并替换裸 hex，确保随主题/chart accent 切换。

#### M-2. 对接边界缺少统一超时与快速失败，依赖后端各 command 自身超时

- **证据**：
  - `invokeNative`（`src/app/lib/native.ts:74-104`）只做性能计时与 `native_unavailable` 判定，未包裹超时/取消（AbortController）。
  - `useRuntimeDashboard` 用 `Promise.allSettled` 并发拉取（`useRuntimeDashboard.ts:141-144`），但单个 command 挂起时前端无自身超时兜底。
- **影响**：PRD P0.2.2.4 要求"本地服务离线/端口未监听/身份不符时快速失败，p95 ≤ 500ms，不允许多个长超时串联"。当前前端把快速失败完全托付给 Rust 侧；若某 command 未实现快速失败，前端会一直显示 loading/stale。属性能与响应性隐患（深度性能分析留给报告三，此处仅指出前端侧机制缺口）。
- **根因**：invoke 边界无前端级 deadline。
- **建议**：在 `invokeNative` 或读取 store 层加可配置 deadline（如摘要 1.5s、发现 3s），超时归类为 `unavailable` 并保留上一份可验证证据。需与后端确认避免与其超时叠加。

#### M-3. `next-themes` 与自定义 `AppearanceProvider` 并存，存在双写与职责重叠

- **证据**：`AppProviders` 同时挂 `NextThemeProvider`（`theme-context.tsx:297-305`）与 `AppearanceProvider`；后者又在 `applyPatch` 里调用 `setNextTheme(next.theme)`（`theme-context.tsx:114`）并写同一 `NEXT_THEME_STORAGE_KEY`（`theme-context.tsx:239`）。
- **影响**：两套主题状态需手动保持同步（当前已同步，`applyAppearanceToRoot` 与 `useTheme` 都写 root class/colorScheme）。存在概念冗余，未来改动一处易漏另一处；`next-themes` 的 SSR 假设在 Tauri 桌面场景无收益。属架构整洁度问题，当前无 bug。
- **根因**：历史上先用 next-themes，后引入自有 AppearanceProvider 承载 accent/chartAccent/lang/glass，但未移除 next-themes。
- **建议**：评估移除 `next-themes`，由 `AppearanceProvider` 单独承载 theme（仅 toast 渲染器 `sonner` 依赖 `useTheme`，可改为读 AppearanceContext）。若保留，在代码注释中固化"AppearanceProvider 为唯一写入方，next-themes 仅镜像"。

#### M-4. Doctor Agent 配置使用裸 `Dialog`（非 `OperationDialog`），与统一操作弹窗规范存在偏差

- **证据**：Doctor 的 agent 接入工作台用设计系统 `Dialog`（`src/app/components/pages/Doctor.tsx:1062`）而非统一 `OperationDialog`；确认修复用的是 `OperationDialog`（`Doctor.tsx:1029,1042`）。
- **影响**：PRD 12.5.8.1/8.7 要求所有有副作用动作用统一 `Dialog`/`AlertDialog` 且来自同一设计系统。此处 `Dialog` 确实来自 `ui/dialog`（合规于"同一设计系统"），但它是一个含多步表单（scope/项目路径/勾选项）的复杂弹窗，与 `OperationDialog`（摘要+影响+diff+确认）的统一形态不一致。属"细节完成度"边界情况，非违规——因为它是配置**编辑**而非纯确认，`OperationDialog` 形态不适配多步表单。
- **根因**：`OperationDialog` 只覆盖"预览-确认"场景，未覆盖"编辑+预览+确认"多步场景。
- **建议**：可接受现状；若追求极致一致，抽一个 `OperationEditDialog`（承载表单 + 底部统一确认/取消/pending），让 Doctor agent 配置与 VisualCompression 高级配置复用同一编辑弹窗骨架。

#### M-5. Heatmap 内部视图切换用 `AnimatePresence mode="wait"`，切换有短暂空档

- **证据**：`Heatmap.tsx:143` `<AnimatePresence mode="wait">` 包裹 activity/tokens/tools 视图切换（key=`${view}-${range}`）。
- **影响**：`mode="wait"` 会等旧内容退场（180ms）再入场新内容，用户在切换 tab 时能看到短暂空白。PRD P0.2.2.2 要求"页面退出动画不得阻塞新页面挂载"。此处是**页内视图切换**而非页面路由（页面路由 `PageStage.tsx` 正确地用 `key={page}` 而非 mode="wait"，无此问题），影响面小；但仍是可感知的 100-180ms 延迟。
- **根因**：页内 tab 切换沿用了 wait 模式。
- **建议**：改为默认（同时进出）或直接切换无退场等待；Reduce Motion 下已由 `MotionConfig reducedMotion="user"` 覆盖，但正常模式下建议去掉 wait。

---

### LOW

#### L-1. i18n 字典保留旧页面导航键，虽未渲染为顶级导航但存在冗余

- **证据**：`i18n.ts` 仍含 `nav.advanced`/`nav.logs`/`nav.compatibility`（`src/app/lib/i18n.ts:18,23,24`），而 `nav.ts` 顶级导航只有 6 项（`src/app/lib/nav.ts:19-26`）。
- **影响**：这些键被子流程标签/面包屑复用（合规），但键名 `nav.*` 暗示顶级导航，易误导。无功能问题。
- **建议**：将子流程标签键从 `nav.*` 迁到 `in.tab.*`/`workbench.*` 命名空间，或加注释说明用途。

#### L-2. `PageStage` 用 1500ms setTimeout 兜底聚焦 h1，极端慢加载下焦点移动可能滞后

- **证据**：`PageStage.tsx:49-53` MutationObserver 找不到 h1 时 1500ms 后强制聚焦。
- **影响**：正常 lazy 加载几十毫秒即完成，兜底几乎不触发；仅极端慢加载时键盘用户焦点移动滞后。属边缘可访问性细节。
- **建议**：可接受；若优化，缩短兜底至 800ms 并在超时仍无 h1 时聚焦 stage 容器本身。

#### L-3. `Overview` 组件仍接收 `setPage` prop 但未使用（已用 `_props` 标注忽略）

- **证据**：`Overview.tsx:29` `export function Overview(_props: { setPage: (p: PageId) => void })`——签名保留 setPage 但 body 不用（符合 PRD P0.6"概览不得放置跨页面 CTA"）。
- **影响**：AppShell 仍传入（`AppShell.tsx:95`），是无害的死参数。
- **建议**：移除 `Overview` 的 `setPage` prop 与 AppShell 传参，减少误用可能。

#### L-4. 通知横幅 `aria-live="polite"`，`action-required` 级别未用 `assertive`

- **证据**：`NotificationBanner` 固定 `aria-live="polite"`（`NotificationCenter.tsx:107`）；`action-required`/`error` 也走 polite。
- **影响**：需要用户立即处理的危险状态用 polite 播报，屏幕阅读器可能延迟到当前朗读结束才播。PRD 12.5.8.10 要求 aria-live（已满足），但未区分紧急度。
- **建议**：`error`/`action-required` 用 `aria-live="assertive"`，其余 polite。

---

## 4. 修改计划

### P0（发布前建议完成 — 均为质量/可维护性，非功能阻断）

| 项 | DoD（可验证） | 涉及文件 | 依赖 |
|---|---|---|---|
| P0-1 抽离超大页面的纯 view-model（H-1 首要目标 Overview/Doctor） | Overview.tsx ≤ 550 行；`lib/overview/model.ts` 承载全部派生纯函数；现有 Overview.test.tsx 全绿无改动 | `Overview.tsx`→`lib/overview/model.ts`；`Doctor.tsx` 拆子组件 | 无 |
| P0-2 建立 desktop payload 类型 + Zod 边界校验（H-2 最小可行） | `lib/runtime/types.ts` 定义 `DesktopData`/`DesktopSummary`；`useRuntimeDashboard` 在边界 `safeParse`，失败进 partial/error；`npx tsc --noEmit` 通过 | `lib/runtime/types.ts`、`useRuntimeDashboard.ts` | 与 P0-1 协同 |

> P0 说明：以上两项不改变用户可见行为，属"发布前应做但不阻断"。若时间紧张，可将完整 109 处 `any` 收敛推迟到 P1，P0 仅覆盖核心 desktop payload。

### P1（发布后第一个迭代）

| 项 | DoD | 涉及文件 | 依赖 |
|---|---|---|---|
| P1-1 拆分 VisualCompression/Advanced/Install 逻辑与呈现（H-1 剩余） | 各主文件 ≤ 550 行 | 对应 pages + 新 `lib/*/model.ts` | P0-1 模式 |
| P1-2 降低 `buildOperationalSnapshot`/`deriveOverviewOperationalSummary` 复杂度（H-3） | 拆为 per-status 纯函数，嵌套 ≤ 4 层，补最小单测 | `useRuntimeDashboard.ts`、`Overview.tsx` | P0-1 |
| P1-3 收敛全部 109 处 `any`（H-2 完成） | 生产代码 `Record<string, any>` 归零或仅存必要边界；tsc strict 通过 | 全前端 | P0-2 |
| P1-4 invoke 边界统一 deadline（M-2） | `invokeNative` 支持超时；离线快速失败 p95 ≤ 500ms（与后端联测） | `native.ts`、store 层 | 需后端确认超时不叠加 |

### P2（打磨）

| 项 | DoD | 涉及文件 |
|---|---|---|
| P2-1 图表色全 token 化（M-1） | 定义 `--chart-accent-2` 等；无裸 hex；深浅色对比达标 | `theme.css`、`Heatmap.tsx` |
| P2-2 评估移除 next-themes（M-3） | AppearanceProvider 单一所有者；sonner 改读 AppearanceContext | `theme-context.tsx` |
| P2-3 Heatmap 去 `mode="wait"`（M-5）+ 通知 assertive 分级（L-4）+ 清理 setPage 死参数（L-3）+ i18n 键命名（L-1） | 对应小改 | Heatmap/NotificationCenter/Overview/AppShell/i18n |

---

## 5. 最佳迭代方案

### 5.1 推荐落地路径

1. **先立"对接契约类型"再拆文件**：P0-2（desktop payload 类型 + Zod 边界）应先于或同步于 P0-1。原因：抽 view-model 时若目标类型已确定，纯函数签名可直接从 `Record<string, any>` 升级为强类型，一次到位，避免二次改写。
2. **以 Overview 为样板**：Overview 的派生函数已是导出纯函数且有 17 个行为单测护栏，是零风险的首个重构对象。完成后把同一"页面=JSX+hooks / lib=纯 view-model+类型"模式复制到 Doctor→VisualCompression→Advanced→Install。
3. **复用既有优秀范式**：`deriveCcSwitchStatus`（`Overview.tsx:656`）已是"单一职责纯函数"范例，H-3 拆分直接照此风格；`connectionSetup.tsx` / `notifications.tsx` 是单一所有者 store 的范例，任何新增领域状态照此。

### 5.2 可并行项（无依赖，可同时进行）

- P2-1（图表 token 化）、P2-3 系列小改（Heatmap wait / 通知 assertive / setPage 清理 / i18n 键）彼此独立，可并行。
- P0-1 各页面拆分之间无依赖（Overview 与 Doctor 可并行），但都建议在 P0-2 类型确定后开始。

### 5.3 关键取舍

- **是否强制 P0 前完成重构**：本前端**功能与安全已就绪**，H-1/H-2 是可维护性债务而非阻断项。取舍建议：内测发布**可不阻塞于重构**，但应把 P0-1/P0-2 列入发布后第一个迭代的最高优先级，避免债务在 1773 行的 Doctor 上继续累积。
- **Zod 引入成本**：`rules/typescript` 明确推荐 Zod；引入一个边界校验点收益（编译期 + 运行期契约保护）远大于 bundle 成本，且可显著减少各页手写容错代码（净减代码量）。
- **不要过度类型化**：真正需要 `any` 的极少数动态字段（如透传的第三方 metadata）保留 `unknown` + 局部窄化即可，避免为 100% 消除 `any` 而制造过度复杂的类型体操（YAGNI）。

### 5.4 明确不建议做的事

- 不建议为"消除 next-themes"（M-3）投入大改，除非顺带处理主题相关 bug——当前双写已同步，收益有限。
- 不建议改动已合规的动效/空态/脱敏/通知体系——这些是本前端的强项，改动只会引入回归风险。

---

## 6. 附录

### 6.1 审查方法

- 精读 PRD §12.5.1-11、P0.2.1、P0.2.2、P0.5、P0.5.1、P0.6、P0.7、P0.12 与 APP_FUNCTIONS_AND_PAGES 全文。
- 逐维度（A 信息架构 / B 设计系统 / C 动效 / D 组件质量 / E 统一状态 / F 对接契约 / G 通知 / H i18n / I 可访问性 / J 空态真实性 / K 测试）对照源码核查。
- 精读关键文件：`App.tsx`、`nav.ts`、`state/evidence.ts`、`useRuntimeDashboard.ts`、`notifications.tsx`、`connectionSetup.tsx`、`theme-context.tsx`、`native.ts`、`security/redaction.ts`、`shell/AppShell.tsx`、`shell/PageStage.tsx`、`shell/Sidebar.tsx`、`shell/AnimatedNavIcon.tsx`、`shell/AnimatedSegmentedControl.tsx`、`shell/StatusBar.tsx`、`common/OperationDialog.tsx`、`common/NotificationCenter.tsx`、`common/DataSourceBadge.tsx`、`common/EmptyState.tsx`、`common/Heatmap.tsx`、`common/RuntimeNotificationBridge.tsx`、`ui/button.tsx`、`pages/Overview.tsx`（全文）、`pages/InstallWorkbench.tsx`（全文）、`pages/Analytics.tsx`（关键段）、`pages/Doctor.tsx`（关键段）、`pages/VisualCompression.tsx`（关键段）、`pages/Settings.tsx`（关键段）。
- 使用 `rg`/`wc`/`grep` 做全局模式扫描（禁止项、`any`、`Date.now()`、fixture/mock、工程术语、gradient/blur、文件规模、命令清单）。

### 6.2 契约合规核查结果（逐项）

| PRD 契约 | 结果 | 关键证据 |
|---|---|---|
| P0.5 / 12.5.2.8 七个一级页面 + 唯一 Beta | ✅ 符合 | `nav.ts:19-26`（6 项 + `visual` beta），`SETTINGS_NAV`；`AppShell.tsx:94-102` 只路由 7 页 |
| 工作台子页降级（Advanced/Logs/Compatibility/SmartLearning） | ✅ 符合 | `InstallWorkbench.tsx:100-117` 作为 4 上栏标签的子内容，非顶级导航 |
| 12.5.2 连续 App Shell（侧栏贯穿 + inset workspace + Topbar 为首行） | ✅ 符合 | `AppShell.tsx:107-122`；侧栏 48px 基线、Topbar 在 workspace 内 |
| 12.5.5/6 纯色画布、无玻璃堆叠/流动渐变 | ✅ 符合 | `theme.css:315-316,1543-1544` `backdrop-filter:none!important`；gradient 仅为极浅单向高光叠加（`theme.css:172,496,604,730`）；`theme-context.tsx:227-232` 移除 `--flow-*` |
| 12.5.7 动效独立可逆配方 + Reduce Motion | ✅ 符合 | `AnimatedNavIcon.tsx:11-24`（9 个独立 recipe）；`Sidebar.tsx` engage/disengage + whileTap 0.92；`PageStage.tsx:70-77` 淡入 4px + reducedMotion 分支；`MotionConfig reducedMotion="user"`（`AppShell.tsx:106`） |
| 12.5.8.7 禁止 window.confirm/alert/prompt + 裸 JSON 冒充 UI | ✅ 符合 | 全局搜索 0 命中；`OperationDialog.tsx` 统一弹窗；仅有的 JSON.stringify 是用户可编辑预览输入（`VisualCompression.tsx:201`） |
| 12.5.8 二级 UI 同一设计系统 | ✅ 符合 | `ui/` 完整 shadcn 组件集；`OperationDialog` 基于 `ui/alert-dialog` |
| P0.2.1 EvidenceSlice 模型 + 单一所有者 | ✅ 符合 | `OperationalSlice<T>`（`useRuntimeDashboard.ts:19-26`）；四 Provider 单一所有者（`App.tsx`） |
| P0.2.1.2/5 保留最后可验证证据 / verified-empty vs unavailable | ✅ 符合 | `hasVerifiedPayloadRef`（`useRuntimeDashboard.ts:164,201`）；Overview `trafficAvailable = ... && Array.isArray(request_logs)`（`Overview.tsx:774`）；`unavailableValue="—"`（`Analytics.tsx:44`） |
| P0.2.2.5 event-first + 单一低频兜底 | ✅ 符合 | `RUNTIME_INVALIDATED_EVENT` 在 mutation 后 dispatch（`native.ts:90-92`）；`appRefreshScheduler` 单一兜底 |
| 12.5.9 真实性空态、无假数据/假时间 | ✅ 符合 | 无 fixture/mock（全局 0 命中）；`Date.now()` 仅用于 staleness 比较（`Overview.tsx:197`、`Doctor.tsx:500,665,1501`）非造时间 |
| 12.5.10 键盘/ARIA/状态不只靠色 | ✅ 符合 | `AnimatedSegmentedControl.tsx:114-132`（Arrow/Home/End + roving tabindex + role=radio）；Heatmap gridcell aria-label（`Heatmap.tsx:342`）；`aria-current`（`Sidebar.tsx:258`） |
| 12.5.8.9 通知稳定 id/severity/去重/脱敏 | ✅ 符合 | `notifications.tsx:76-89` dedupeKey + count；`sanitizeInput` 调 `redactString`（`notifications.tsx:113-127`） |
| P0.12 无工程术语暴露 | ✅ 符合 | i18n 与页面 JSX 均 0 命中 benchmark/smoke/fixture/payload unavailable 等 |
| P0.2.14 自动更新签名门禁 | ✅ 符合 | `autoUpdateNotification` 仅 `signature_verified===true` 才标 available（`RuntimeNotificationBridge.tsx:108-119`） |
| 安全：脱敏/凭据引用 | ✅ 符合 | `redaction.ts` 完整（禁止字段 + 密钥指纹 + `auditExportForSecrets`）；`store_provider_credential` 只存引用 |
| i18n 中英完整性 | ✅ 结构性保证 | `DICT` 每键强制 `{zh,en}`（`i18n.ts:5`），TypeScript 编译期保证双语；缺键返回 key（显式暴露非静默）|

### 6.3 覆盖文件范围

- **完整精读**（20 文件）：见 6.1 列表中标注全文/核心的文件。
- **模式扫描覆盖**：`src/app/**/*.tsx`、`src/app/**/*.ts`（非测试）全量，`src/styles/*.css`。
- **测试文件清点**：38 个（未逐一精读全部断言，抽样 Overview.test.tsx 确认为行为测试而非仅渲染）。

### 6.4 未覆盖 / 需进一步验证项

1. **未逐一精读的大页面完整实现**：Doctor/VisualCompression/Advanced/Install 只精读了关键段（对接、弹窗、状态、IA），未逐行审查全部 UI 分支——建议在 H-1 重构时随迁移逐段复核。
2. **未逐一核查全部 38 个测试的断言质量**：仅抽样 Overview.test.tsx。K 维度"是否只测渲染不测行为"需对 Doctor/VisualCompression/Install 等大页面测试再抽查。**建议后续验证**。
3. **未运行测试/构建**：本次为纯静态只读审查，未执行 `npm run test:run` / `tsc --noEmit` / `npm run build` / `npm run test:e2e`（属实现-验证闭环的验证步骤，且 CLAUDE.md 要求此类命令由对应流程执行）。**"全绿"结论需实际运行确认**。
4. **前后端对接的运行时贯通**：本报告只审前端侧契约与机制（类型、状态、错误语义、receipt 关联）。Native payload 是否真的带正确 `source`/`freshness`/`transaction_provenance`、同一请求是否跨页一致、真实 routed 流程——属后端报告与流程报告范围。
5. **release build 性能实测**：M-2 指出的 invoke 无前端超时是机制层面观察；实际冷启动/切页/刷新 p95、大 Context 重渲染、内存漂移的量化，留给性能报告（报告三）。
6. **e2e 覆盖广度**：只确认存在 `desktop-control-plane.spec.ts` + `e2e/pages/`，未评估其覆盖的关键流程完整性。**建议进一步验证**。
