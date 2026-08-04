# d-token 桌面前端 UI 优化迭代研究报告

> 日期：2026-07-13
> 修订：同日 **三轮多维度审查后 v2**；同日补 **§0.0 双端 1:1 硬约束**（原稿遗漏）
> 分支：`codex/prd-macos-polish`
> 方法：生产入口源码（`main.tsx` → `app/`）+ 已构建 `dist/` + AiMaMi inspect 对照
> 本文件是研究与迭代输入，**不替代** `docs/PRD.md` / `docs/PLAN.md` / `IMPLEMENTATION_STATUS.md`。

---

## 0.0 硬约束：Windows / macOS 1:1（原稿未写，现补正）

> **结论先说：v2 正文原先没有把双端 1:1 写成优化门禁。**
> 这不等于产品没有该要求——真理源始终是 PRD，不在研究稿。

### 产品契约（必须服从，优化不得削弱）

| 来源 | 原文要点 |
|---|---|
| `docs/PRD.md` §12.5.1-5 | **跨平台一致**：Windows/macOS 共享同一 React 组件、信息架构、字号、间距、状态和中英语义；平台差异只存在于窗口、凭据、进程、系统集成与打包适配层 |
| `docs/PRD.md` §12.5.2-7 | macOS drag region 仅空白区；**Windows 使用同一布局**，窗口控制差异由 Tauri/平台适配层处理 |
| `docs/PRD.md` §12.5.3 / 字体 | 字号、权重、行高、布局语义与截断规则跨端一致；字形可由系统字体决定 |
| `docs/TECH_STACK.md` §3 | Windows/macOS **1:1 视觉系统、布局、文案与交互语义** |
| `docs/ARCHITECTURE.md` | UI **1:1**，共享组件、token、文案与交互；平台说明仅在确有差异的操作处出现 |
| `docs/product/IMPLEMENTATION_STATUS.md` | 共用同一 React UI；**当前只完成 macOS 真实视觉验收**；Windows 视觉一致性证据仍缺 |

### 本优化工作的验收含义（UI + 动画 + 功能）

| 维度 | 1:1 要求 | 允许分叉的仅限 |
|---|---|---|
| **UI** | 同一 `app/` React 树、同一 shell/page 组件、同一 token（`theme.css` / `shell.css`）、同一中英 copy 与状态语义 | 窗口控件、titlebar/drag、系统字体栈回落、DPI 由 OS 渲染差异（布局数值仍一致） |
| **动画** | 同一 motion 语义：PageStage 入场、侧栏折叠 180–220ms、ASC 滑块、nav icon hover/click 配方、`prefers-reduced-motion` 路径 | 不得为「某端好看」单独加一套 keyframes 或 Framer 配置 |
| **功能** | 同一用户任务路径与 evidence 边界（六页 IA、Install 工作台、Settings 选择控件、Heatmap 视图/范围） | Keychain vs Credential Manager、LaunchAgent vs HKCU Run、路径/shim/签名——**不得**泄漏进共享 React 产品页逻辑 |

### 研究稿与实现的缺口（诚实）

1. **研究 v2 未列双端门禁** → 易被读成「只 polish macOS」。现以本节为优化门禁。
2. **实现证据不对称**：macOS 有真实窗口/截图候选；Windows 尚无同一 commit 的视觉 1:1 实机对照（IMPLEMENTATION_STATUS 已写）。
3. **本轮代码改动必须落在共享路径**（`code/apps/desktop/src/app/**`、`src/styles/**`），禁止 `platform === "win32"` 式 UI 分叉，除非 PRD 明确的系统集成层。
4. **动画验收**：改 PageStage / ASC / Heatmap / shell motion 时，验收口径是「两端同一 React 行为」；Windows 实机截图/录像可作为后续 P0 证据任务，**不能**用「只在 mac 上看过」宣称 1:1 已完成。

### 优化任务自检（每次 UI 改动前）

- [ ] 是否只改共享 React/CSS，未引入平台专用组件树？
- [ ] 字号/间距/圆角/时长是否仍与 PRD §12.5 桌面契约一致？
- [ ] 动画是否尊重 `prefers-reduced-motion` 且两端同一套？
- [ ] 功能是否仍走同一 evidence / session / 选择控件语义？
- [ ] 若声称「双端视觉通过」，是否绑定 **同一 commit** 的 macOS + Windows 证据？（无则只可写「共享实现已改，Windows 实机未验」）

---

## 0. 执行摘要（v2）

### 0.1 一句话

壳层与证据边界已成形；与成品体感差距主要在 **折叠侧栏名称呈现、图标动画触发语义、CSS token 双轨、二级 Tab 无记忆、Analytics 包体积**。神话感/Claude 感应落在 **暖色材质 + display 标题 + 稀缺琥珀强调**，禁止装饰性星空/符文。**所有 polish 必须保持 Windows/macOS UI+动画+功能 1:1（§0.0），不得做成单端优化。**

### 0.2 本轮落地范围（审查后收窄）

三轮审查后，**立即实现**只做高 ROI、可测、不扩大架构的改动：

| 轮次 | 主题 | 交付 |
|---|---|---|
| 实现 R1 | 侧栏 | 折叠 Radix Tooltip；图标仅 hover/focus/click 播放，去掉 active 常驻动画 |
| 实现 R2 | 视觉 token / 字体 | 选中态统一 interaction；KPI tabular-nums；侧栏品牌轻微 display 气质 |
| 实现 R3 | 二级页与壳状态 | 折叠偏好持久化；Install 工作台 Tab session 记忆 + 窄窗副说明；页面级 session 记忆 |

**明确延后**：整文件拆 Advanced、去掉 recharts、Doctor 请求合并、全量 theme.css 物理拆分、keep-alive 多页缓存。

### 0.3 成熟度快照（源码复核）

| 维度 | 成熟度 | 证据摘要 |
|---|---|---|
| Shell / lazy | 高 | `AppShell` + 六页 lazy + `PageStage` |
| 证据模型 | 高 | `useRuntimeDashboard` 8s 轮询、detail 按需、snapshotKey |
| 侧栏交互 | 中 → 目标高 | 原 `title=`；目标 Tooltip + 动画语义修正 |
| Token / 字体 | 中 | display 已定义；双轨与 tabular 缺口 |
| 二级页状态 | 中偏低 | 多 `defaultValue`；Install 仅内存 `activeTab` |
| 性能 | 中偏低 | Analytics raw 395KB / gzip ~107KB；index raw 435KB / gzip ~141KB |

---

## 1. 三轮多维度审查记录

### Round 1 — 准确性（源码对账）

**维度**：事实是否可在代码中定位；数字是否区分 raw/gzip；路径是否生产入口。

| 原报告断言 | 判定 | 修正 |
|---|---|---|
| 生产入口 `main.tsx` → `styles/index.css` | **成立** | 保留 |
| `fonts.css` 为空 | **成立** | 保留；空文件不代表无字体（系统栈在 theme 后半） |
| 折叠仅 `title=` | **成立** | `Sidebar.tsx` L122 |
| `ui/sidebar.tsx` 未接入产品壳 | **成立** | 无产品 import；勿整页换 shadcn sidebar |
| active 态绑 keyframes | **成立** | `shell.css` 多处 `[data-active="true"]` |
| Analytics 395KB | **部分成立** | 补充 **gzip ≈ 107KB**；优化表述避免夸大「再装一个主包」而不提 gzip |
| index 435KB | **部分成立** | gzip ≈ 141KB |
| theme 双轨 | **成立** | 前半 glass/accent-500，后半 interaction 覆盖；运行时以后半为主 |
| 页切换整页卸载 | **成立** | `key={page}` + `mode="wait"` |
| Doctor 轮询放大 native | **假设偏强** | 降为「风险待测」，不作为 P0 实现依据 |
| AiMaMi 路径写本机 Codex 目录 | **脆弱** | 改为「本地 inspect 对照 + commit 297c7af…」；不依赖绝对路径可读性 |

**Round 1 结论**：主干事实正确；性能数字与 Doctor 断言需克制；实现不应依赖「整文件拆 theme」一次做完。

### Round 2 — 完整性（UX / 性能 / UI 设计）

**维度**：用户任务、二级页、切页、可访问性、包体积、视觉系统。

| 缺口 | 用户影响 | 是否本轮必做 |
|---|---|---|
| 折叠无悬浮名称 | 窄窗/折叠后认不出导航 | **是** |
| 图标 active 常驻动画 | 当前页「总在动」，不克制 | **是** |
| Tooltip 默认 `bg-primary` | 若直接套用破坏暖色神话感 | **是**（shell 专用样式） |
| Tab 无记忆 | 分析/Doctor/安装子页每次重置 | **部分**（Install + page session；不全量改所有 Tabs） |
| Install 副说明窄窗 | 920 宽挤压 | **是**（CSS 隐藏/压缩 description） |
| Analytics recharts | 首进分析页解析成本 | **否**（延后；需独立拆包任务） |
| 巨型 Advanced | 维护成本 | **否**（延后） |
| 神话感装饰 | 用户期望 | **否** 加装饰；**是** 用 token/字体克制强化 |
| 切页焦点到 h1 | 键盘用户 | **建议** 小改可做 |
| 双 collapse 按钮 | 轻微冗余 | 不改（Topbar + 侧栏均有价值） |

**Round 2 结论**：UX 最大单点是侧栏折叠命名；设计最大单点是 token/动画语义；性能最大单点是 Analytics，但实现成本高，不阻塞侧栏/视觉。

### Round 3 — 可执行性与风险

**维度**：改动半径、回归、许可证、与 PRD 冲突、测试可证伪。

| 方案 | 风险 | 决策 |
|---|---|---|
| 接入完整 `ui/sidebar` Provider | 引入 cookie/mobile sheet/宽度体系，偏离现有 184/64 壳 | **拒绝** |
| 仅 Tooltip + 现有 Sidebar | 小、可测、对齐 AiMaMi 交互意图 | **采用** |
| 物理删除 theme 前半大段 | 易误伤 activity/chip 仍引用的 class | **本轮只覆盖关键冲突选择器**，不全量删 |
| 图标去掉 remount、只靠 CSS | 需改测试「remount」假设 | **保留 click trigger 语义**；可继续用 motionToken，但 CSS 不再绑 active |
| keep-alive 多页 | 与 detail 订阅/内存耦合 | **延后**；用 sessionStorage 记 page/tab |
| 折叠 localStorage | 与窄窗强制折叠冲突 | **采用**：用户手动选择优先；仅 `matches` 变窄时折叠，不变宽时自动展开 |

**Round 3 结论**：计划必须可测、可回滚、不碰安全/证据路径；三轮实现后应用测试：`Sidebar.test` + 相关 vitest。

---

## 2. 源码地图（精简保留）

```text
index.html → src/main.tsx → styles/index.css + app/App
AppProviders → RuntimeEvidenceProvider → ConnectionSetupProvider
  → NotificationProvider → RuntimeNotificationBridge + AppShell
AppShell: Sidebar | workspace(Topbar, Banner, PageStage+lazy, StatusBar)
```

一级：`overview | install | analytics | doctor | changes | settings`
二级：InstallWorkbench 四栏；Analytics/Doctor/Changes/Logs/Advanced 内 Tabs。

未进生产：`src/styles.css`、`src-old/`、根级 `App.tsx` 旧入口。

---

## 3. 优化后的实施计划（立即执行）

### 实现 Round 1 — 侧栏

1. `TooltipProvider` 包裹侧栏（`delayDuration={0}`）。
2. 折叠时：导航 / 主题 / 折叠按钮使用 `Tooltip` + `side="right"`；展开时不显示名称 tooltip。
3. Shell 专用 tooltip class：popover 表面色，非 `bg-primary`。
4. `shell.css`：动画选择器改为
   - hover / focus-within
   - `[data-motion-play]` 或 click 触发的 `data-motion-trigger` 变化
   - **移除** `[data-active="true"]` 上的 transform 动画。
5. 测试：collapsed 时存在 tooltip 文案；click 仍提升 `data-motion-trigger`；expanded 无强制 title 依赖。

### 实现 Round 2 — 视觉与字体

1. `.sidebar-item-selected` 颜色统一 `var(--interaction-accent-strong)`（去掉对过时 `--accent-500` 的依赖 class）。
2. Overview KPI（及 Analytics 主数字若改动面小）加 `tabular-nums`。
3. 侧栏品牌字：略增 tracking/字重层次，H1 已用 display 不动业务页结构。
4. 侧栏 tooltip 样式写入 `shell.css`。

### 实现 Round 3 — 状态与二级页

1. `AppShell`：`collapsed` 读写 `localStorage` key `d-token.shell.sidebar-collapsed`；窄窗仅强制折叠。
2. `page` 读写 `sessionStorage` key `d-token.shell.page`。
3. `InstallWorkbench`：`activeTab` 读写 `sessionStorage`；窄窗 CSS 隐藏 `.install-workbench-nav-description` 或压缩导航高度。
4. 可选：`PageStage` 挂载后对页面 `h1` 做 `tabIndex=-1` focus（不滚动跳动则 skip 若成本高）。

### 验收清单

- [x] 折叠侧栏 hover/focus 见中文/英文名称（Radix）
- [x] 当前页图标静止，hover/click 才动
- [x] 刷新保持折叠偏好与上次一级页（session/local 约定）
- [x] Install 子 Tab 刷新同会话保持
- [x] 二级页 Tabs session（Analytics/Doctor/Changes/Logs/Advanced）
- [x] 选择控件 soft measure-ASC（侧栏主题、Settings、Heatmap、Logs filter、Advanced cc mode）
- [x] PageStage 单次入场 + h1 焦点（含 lazy）
- [x] `npm run test:run` 全量 147 通过；`tsc --noEmit` 0
- [x] 无密钥/无复制 AiMaMi 资产；无共享 UI 平台分叉

### 延后 backlog（不在本提交）

- Analytics recharts 动态 import / 自绘
- theme.css 物理拆分
- Advanced/Doctor 拆文件
- Doctor 请求合并 profiling
- Heatmap range/view session（O3，可选）
- Windows 同 commit 六页视觉/动画实机 1:1 证据

---

## 4. 设计约束（Claude + 神话感，合规）

| 要 | 不要 |
|---|---|
| 暖米/暖黑画布、衬线 H1、琥珀 CTA | 星座、符文、渐变流场 |
| 侧栏 3px 指示条、短图标动效 | active 循环动画、大面积发光 |
| 折叠悬浮名称清晰 | 原生 title 长延迟 |
| 证据 `—` / partial | 假曲线、假绿 |

---

## 5. 关键路径

| 主题 | 路径 |
|---|---|
| 侧栏 | `code/apps/desktop/src/app/components/shell/Sidebar.tsx` |
| 动画 CSS | `code/apps/desktop/src/styles/shell.css` |
| 壳状态 | `code/apps/desktop/src/app/components/shell/AppShell.tsx` |
| 安装工作台 | `code/apps/desktop/src/app/components/pages/InstallWorkbench.tsx` |
| Token | `code/apps/desktop/src/styles/theme.css` |
| Tooltip 基元 | `code/apps/desktop/src/app/components/ui/tooltip.tsx` |
| 产品契约 | `docs/PRD.md` §12.5 |

---

## 6. 总结

三轮审查把原报告从「全面但偏宽」收敛为 **可立即执行的三轮应用改动**，并修正了性能表述与过强假设。下列章节之后的实现以 §3 为准。

---

## 7. 逐页源码审查（2026-07-13 晚，非笼统版）

> 方法：五路子代理结论 + 主代理对生产入口 `app/` 的二次对账。
> 每条必须可定位到 **文件 + 行/符号 + 现象 + 最小改法**。
> 约束：不大改版式；选择控件单独对照 AiMaMi measure-based ASC；禁止 primary 实心胶囊（AiMaMi `SegmentedOptions` 那套）直接进产品。

### 7.0 已落地（本轮实现 R1–R3，勿重复做）

| 项 | 证据 |
|---|---|
| 折叠侧栏 Radix Tooltip | `Sidebar.tsx` `CollapsedTip` + `shell-nav-tooltip` |
| active 图标不再常驻动画 | `shell.css` L171–223 仅 `:hover` / `:focus-visible` |
| 侧栏选中色 interaction | `Sidebar.tsx` L197 `var(--interaction-accent-strong)` |
| 折叠 localStorage / 页 session | `AppShell.tsx` `SIDEBAR_COLLAPSED_KEY` / `PAGE_SESSION_KEY` |
| Install 工作台 Tab session | `InstallWorkbench.tsx` `WORKBENCH_TAB_KEY` |
| KPI `tabular-nums` | `Overview.tsx` L219、`Analytics.tsx` L355 |
| 窄窗隐藏 Install 副说明 | `shell.css` `@media (max-width: 920px)` |

### 7.1 选择控件 — 现状清单与 AiMaMi 对照（优先）

**AiMaMi 真源**（本机 inspect，commit 同源注释）：

| 组件 | 路径 | 机制 | 视觉 |
|---|---|---|---|
| `AnimatedSegmentedControl` | `aimami-inspect/.../animated-segmented-control.tsx` | `getBoundingClientRect` 量 active 按钮 → indicator `left/top/width/height`；`ResizeObserver` 重测 | 样式外置（`indicatorClassName` 等） |
| `SegmentedOptions` | `.../segmented-options.tsx` | 包一层 ASC | **实心** `bg-primary` + `text-primary-foreground` 圆角胶囊 |

**d-token 现状：**

| ID | 位置 | 实现 | 问题（具体） |
|---|---|---|---|
| S1 | `shell/AnimatedSegmentedControl.tsx` L59–61 | 等分：`width: 100/n%` + `translateX(index*100%)` | 文案不等长时滑块与按钮对不齐；**无 measure**；与 AiMaMi 核心机制不一致 |
| S2 | `Sidebar.tsx` L96–106 | 唯一生产使用 ASC（主题） | 两项等长，缺陷不明显，不能代表产品级选择控件 |
| S3 | `Settings.tsx` L589–626 `Segmented`/`SegBtn` | `chip` / `chip-active` 拼按钮，**无滑动指示器** | 保留天数 / 扫描深度 / 更新通道 / 主题 / 语言 五组；与侧栏 ASC 两套交互语言 |
| S4 | `Heatmap.tsx` L89–118 | `activity-tab` + `motion.span layoutId` | 双轨：view 与 range 各一套 layoutId；不是 shared ASC；依赖 Framer layout 而非 measure |
| S5 | `LogsSecurity.tsx` L127–142 | `.log-filter-item` + `.active` | 筛选 chip，无 radiogroup 滑动；与 Settings chip 第三套 |
| S6 | `Advanced.tsx` L1018–1027 | `chip` / `chip-active` 三列 grid | cc mode 选择；无键盘 radiogroup 语义（button 无 `role="radio"`） |
| S7 | Install / Analytics / Doctor / Changes / Advanced 顶层 `Tabs` | Radix `TabsList`/`TabsTrigger` | **内容分区**，不是「同一表单字段二选一」；**不迁 ASC** |

**产品决策（写死）：**

1. **要**：AiMaMi ASC 的 **measure 滑块**（left/width 跟真实按钮）。
2. **不要**：AiMaMi `SegmentedOptions` 的 primary 实心填充（与 PRD 暖色 soft pill / chip 材质冲突）。
3. **外观**：沿用 d-token soft pill — 轨道 `muted`/`sidebar-accent`，滑块 `card` + 轻阴影（现有 `shell-segmented*`），可加 `variant="soft"`。
4. **迁移目标**：S3 Settings 多选组 → ASC soft；S4 Heatmap 两行 → ASC soft（可去掉 layoutId）；S5 日志筛选 → ASC soft 或保持 filter bar 但统一 active token；S6 Advanced mode → ASC soft equalWidth。
5. **不迁**：S7 工作台/章节 Tabs；色板 `Swatch`。

**最小实现步骤：**

```text
1. 扩展 AnimatedSegmentedControl：
   - containerRef + itemRefs + measure + ResizeObserver（对齐 AiMaMi）
   - 保留 equalWidth 可选；默认 measure
   - reducedMotion：transition duration 0
   - 可选 disabled per item（Settings 强制 redaction）
2. shell.css：indicator 改为 left/width 过渡，不再依赖 % translate 唯一路径
3. Settings：删除本地 SegBtn/Segmented，改用 ASC（5 组 value 受控）
4. Heatmap：activity view + range 改 ASC；删 layoutId indicator
5. Advanced cc mode：三选项 ASC
6. 测试：键盘 Arrow、measure 在不等长 label 下 width 变化；Settings vitest 若有则更新
```

### 7.2 壳层 / 动画 / 通知

| ID | 严重度 | 文件:位置 | 现象 | 最小改法 |
|---|---|---|---|---|
| C1 | **P1** | `PageStage.tsx` L15–17 **且** `shell.css` L92–98 | 同一次入场：Motion `initial/animate` **再加** CSS `@keyframes dtoken-page-enter`，双重 opacity+y | **二选一**：删 CSS animation **或** 去掉 motion initial/animate，只留一边 |
| C2 | P2 | `AnimatedNavIcon` + Sidebar remount `key` | click 靠 remount 重播；hover 靠 CSS；两套触发可接受 | 保持；不要再绑 `data-active` 动画 |
| C3 | P2 | `Sidebar.test.tsx` L68–76 | Radix Tooltip 在 jsdom 可能缺 `ResizeObserver` 导致失败 | `beforeEach` mock `global.ResizeObserver` |
| C4 | 信息 | `NotificationCenter.tsx` L15–27 | 已用 Radix `Popover`（portal）；`shell.css` `.notification-popover` 是样式类名，**不是** absolute 自绘定位 bug | 勿再当「absolute vs portal」重写；可清理重复 absolute 假设文档 |
| C5 | P3 | `AppShell` 窄窗 L74–75 | 进入窄窗强制 `setCollapsedState(true)` **不写** localStorage | 有意为之；若用户宽屏偏好折叠，窄→宽后仍折叠（local 未改）OK |

### 7.3 总体概览 + Heatmap

| ID | 严重度 | 文件:位置 | 现象 | 最小改法 |
|---|---|---|---|---|
| O1 | **P1** | `Heatmap.tsx` L386–392 | Token 柱 `width={48}` 固定；`x = left + index*step - 24` | 多点时柱体重叠。改为 `barW = min(48, step*0.6)`，`x = center - barW/2` |
| O2 | P1 | 同 §7.1 S4 | 视图/范围选择非统一 ASC | 迁 soft ASC |
| O3 | P2 | `Heatmap.tsx` L77–78 | range/view 仅 `useState`，切页丢失 | 可选 `sessionStorage` key `d-token.heatmap.range/view` |
| O4 | 已做 | Overview KPI | `tabular-nums` | — |

### 7.4 安装与连接（二级）

| ID | 严重度 | 文件:位置 | 现象 | 最小改法 |
|---|---|---|---|---|
| I1 | 已做 | InstallWorkbench session Tab | — | — |
| I2 | P2 | Advanced / SmartLearning / Logs 子页内 `Tabs defaultValue` | 从 Install 切走再回来：工作台 Tab 记得，**子 Tabs 重置** | 仅对高频子 Tab 加 session（如 Advanced `service`）；或接受重置 |
| I3 | P3 | `install-workbench-nav` 高度 5rem + 副文案 | 窄窗已 hide description | 保持 |

### 7.5 分析 / Doctor

| ID | 严重度 | 文件:位置 | 现象 | 最小改法 |
|---|---|---|---|---|
| A1 | P2 | `Analytics.tsx` L96 `defaultValue="live"` | 切到其他一级页再回，Tab 回 live | 受控 + session `d-token.analytics.tab` |
| A2 | P2（延后体积） | `Analytics.tsx` L16–21 recharts | 历史 Tab 才需要图，却静态 import 整页 | 动态 `import("recharts")` 或 History 子组件 lazy — **独立任务** |
| A3 | P2 | `Doctor.tsx` L448 `defaultValue="checks"` | 同上 Tab 健忘 | session 可选 |
| A4 | P3 | Doctor 多请求 | 风险待测，非本轮 UI 必做 | profiling 后再动 |

### 7.6 变更 / 设置 / Advanced / Logs

| ID | 严重度 | 文件:位置 | 现象 | 最小改法 |
|---|---|---|---|---|
| G1 | P1 | Settings `SegBtn` | 见 §7.1 S3 | 迁 ASC soft |
| G2 | P2 | `Changes.tsx` L270 `defaultValue="pending"` | Tab 健忘 | session 可选 |
| G3 | P2 | `LogsSecurity.tsx` L112 + filter chips | Tab 健忘 + 筛选 UI 第三套 | filter → ASC 或统一 chip token |
| G4 | P2 | `Advanced.tsx` L284 + L1018–1027 | 章节 Tabs 健忘；mode 用 chip grid | mode → ASC；Tabs session 可选 |
| G5 | P3 | Advanced 单文件过大 | 维护成本 | **延后**拆文件，不本轮 |

### 7.7 推荐落地顺序（小改、可测）

| 序 | 项 | 预估 | 验收 |
|---|---|---|---|
| 1 | **C1** 去掉 PageStage 双重入场 | 5 行 | 切页只一次 fade/slide |
| 2 | **S1** ASC measure + soft 样式 | ~80 行 | 不等长 label 时滑块贴边 |
| 3 | **G1** Settings 五组迁 ASC | Settings 局部 | 无 SegBtn；键盘可切 |
| 4 | **O2** Heatmap 两行 ASC | Heatmap 局部 | 无 layoutId 双 indicator |
| 5 | **O1** token 柱宽按 step | 10 行 | 年视图柱不重叠 |
| 6 | **C3** ResizeObserver mock | 测试 | Sidebar tooltip 测绿 |
| 7 | （可选）A1/A3/G2 二级 Tab session | 各 10 行 | 同会话保持 |
| 8 | （延后）A2 recharts 拆包 | 大 | 独立 PR |

### 7.8 明确不做

- 整页换 `ui/sidebar` Provider
- 星空/玻璃/神话装饰层
- 复制 AiMaMi primary 实心 `SegmentedOptions`
- Install 四栏工作台改成 ASC
- theme.css 物理大拆、Advanced 拆文件、Doctor 请求合并
- **任何仅 macOS 或仅 Windows 的共享 UI/动画分叉**（违反 §0.0 / PRD §12.5.1-5）

### 7.9 双端 1:1 与本审查条目的关系

| 审查项 | 共享实现路径 | 双端含义 |
|---|---|---|
| C1 PageStage 双入场 | `PageStage.tsx` + `shell.css` | 两端同一 motion；删 CSS 重复后两端一致受益 |
| S1 ASC measure | `AnimatedSegmentedControl.tsx` | 单一组件；禁止 per-OS indicator |
| O2 Heatmap ASC | `Heatmap.tsx` | 概览子控件两端同一 |
| O1 token 柱宽 | `Heatmap.tsx` TokenTrend | 纯 SVG 数学，无平台 API |
| G1 Settings ASC | `Settings.tsx` | 设置选择控件两端同一 |
| 二级 Tab session | 各 page sessionStorage | WebView 行为一致；勿用 OS 专用存储 |
| recharts 体积（延后） | Analytics lazy | 两端同一 bundle 策略 |

**证据债（不在本轮改代码范围，但文档必须点名）**：Windows 与 macOS 同一候选 commit 的六页截图 / Reduce Motion / 920×640 / 动画抽帧对照仍缺；在补齐前不得写「双端视觉验收通过」。

---

## 9. 计划落地完成表（2026-07-13 实现收口）

| 计划项 | 状态 | 证据 |
|---|---|---|
| R1 侧栏 Tooltip + 图标语义 | 完成 | `Sidebar.tsx` CollapsedTip；`shell.css` hover/focus only |
| R2 interaction 色 + tabular + 品牌 | 完成 | interaction-accent-strong；Overview/Analytics KPI |
| R3 折叠/页/Install session + 窄窗 | 完成 | AppShell keys；InstallWorkbench；CSS 920 |
| C1 双入场 | 完成 | 已删 `dtoken-page-enter` CSS；仅 Motion |
| S1 ASC measure | 完成 | getBoundingClientRect + ResizeObserver |
| G1 Settings ASC | 完成 | 无 SegBtn；6 组 ASC |
| O2 Heatmap ASC | 完成 | 无 layoutId |
| O1 token 柱宽 | 完成 | `barW = min(48, max(4, step*0.6))` |
| C3 ResizeObserver 测试 | 完成 | Sidebar.test stub |
| 二级 Tab session | 完成 | `sessionTab.ts` + 五页 |
| Advanced mode / Logs filter ASC | 完成 | soft ASC |
| PageStage h1 焦点 | 完成 | rAF + MutationObserver（lazy） |
| §0.0 双端 1:1 文档门禁 | 完成 | 共享路径无 platform UI 分叉 |
| A2 recharts 拆包等 | **延后** | 见 backlog |

**验证（本机执行）**：`npm run test:run` → **30 files / 147 tests passed**；`npx tsc --noEmit` → **0**。

---

## 10. 实现后三轮多维度审查（收口）

> 对象：§9 已落地代码 + 测试结果。
> 方法：源码对账、grep 反证、全量 vitest、tsc；**未**跑 Windows 实机 UI。

### Round A — 准确性（代码是否真的做了）

| 断言 | 判定 | 证据 |
|---|---|---|
| 无 CSS 双入场 | **成立** | `shell.css` 无 `dtoken-page-enter`；PageStage 仅 motion |
| ASC 为 measure 非等分 translate | **成立** | `AnimatedSegmentedControl` indicator left/width |
| Settings 无 SegBtn | **成立** | grep 无 `SegBtn` 实现 |
| Heatmap 无 layoutId 双轨 | **成立** | 两路 ASC |
| 产品页无 `Tabs defaultValue` | **成立** | 仅 `ui/slider` 保留 defaultValue API |
| session keys 隔离 | **成立** | `d-token.{shell,install,analytics,doctor,changes,logs,advanced}.*` |
| 无 shared UI `win32/darwin` 分叉 | **成立** | app 内无 platform UI 分支（Install 仅 install_root UA 属系统路径，非视觉） |
| 147 全绿 | **成立** | 本机 vitest 输出 |
| 双端视觉验收通过 | **不成立** | 仅共享实现；Windows 证据债仍在 |

**Round A 结论**：实现断言与源码一致；不得把「测试绿」写成「Windows 视觉 1:1 已验收」。

### Round B — 完整性（UX / 动画 / 功能 / 双端）

| 维度 | 结论 | 残留 |
|---|---|---|
| UX 侧栏命名/动画 | 达标 | Topbar 折叠钮仍 `title=`（展开态有文案，可接受） |
| 选择控件语言 | 基本统一 soft ASC | Heatmap 工具栏 CSS 仍挂 `activity-tabs` 类名，可能与旧 button 样式叠层 → **视觉需实机看一眼** |
| 二级状态记忆 | 章节 Tabs 全覆盖 | Heatmap view/range 仍仅内存（O3 未做） |
| 动画 | 单次 PageStage；icon hover/click；ASC 滑块；reduced-motion 有 data 与 duration 0 | PageStage 焦点在无 h1 页会 no-op（安全） |
| 功能/证据边界 | 未改 native 证据路径 | 无 |
| 双端 1:1 实现 | 共享 React/CSS | 缺 Windows 同 commit 截图/动画证据 |
| 性能 | 未做 recharts 拆包 | A2 仍为最大体积债 |
| a11y | ASC radiogroup；h1 focus | Logs filter label 内嵌 count，可读性 OK |

**Round B 结论**：计划内 polish 完整；缺口集中在 **延后体积**、**Heatmap 子状态 session**、**实机双端证据**、**activity-tabs CSS 叠层风险**。

### Round C — 可执行性 / 风险 / 回归

| 风险 | 等级 | 处理 |
|---|---|---|
| sessionStorage 污染 vitest | 已缓解 | 相关测试 beforeEach clear key |
| ASC 首帧 indicator opacity 0 | 低 | measure 后显示；jsdom 有 RO mock |
| Logs filter 塞进 shell-segmented 与 `.log-filter-bar` 旧样式 | 中 | 建议后续实机确认；若冲突只改 CSS 作用域 |
| Advanced mode 默认 `"auto"` 当 null | 低 | 展示用；保存仍显式点击 |
| PageStage MutationObserver 1.5s 上限 | 低 | 超时仍尝试 focus |
| 工作区未提交混有其他功能 diff | 流程 | UI polish 与 Install/learning 等同树；提交时应拆分或明确范围 |
| 宣称发布级双端 UI | 禁止 | 需 Windows 证据 |

**Round C 结论**：可合并为工程 polish 候选；**不可**单独作为 macOS/Windows 公开发布视觉门禁。

### 审查总评

| 维度 | 分（1–5） | 说明 |
|---|---|---|
| 计划完成度 | 5 | 立即项与 §7.7 1–7 已完成；延后项诚实保留 |
| 源码准确性 | 5 | 对账通过 |
| UX 一致性 | 4 | ASC 统一；toolbar CSS 叠层待实机 |
| 动画克制 | 5 | 双入场已消；active 常驻已消 |
| 双端 1:1 | 4 实现 / 2 证据 | 共享路径正确；Windows 证据不足 |
| 测试 | 5 | 147 + tsc |
| 安全/证据边界 | 5 | 未触密钥与假数据 |

**下一步（非本轮必须）**：① 实机看 Heatmap/Logs ASC 与旧 CSS；② recharts lazy；③ 同 commit Windows UI 证据包。

---

## 11. 一句话收口

计划内 UI 迭代（壳层、选择控件、二级 session、PageStage、Heatmap）已在 **共享 React 路径** 落地并通过 **147** 单测；三轮审查确认实现准确、范围克制、双端 **实现 1:1 / 证据未齐**。延后项仅体积拆包与 Windows 视觉实证。

## 8. 对用户可见的一句话结论

壳层 R1–R3 已到位；**下一刀应打在「选择控件统一为 measure soft-ASC」+「PageStage 双动画」+「Heatmap 柱宽」**，全部是可点名的文件行，而不是「整体再 polish 一下」。**所有改动默认 Windows/macOS 1:1；原稿 §0 未写双端门禁是文档缺陷，已以 §0.0 / §7.9 补正。**
