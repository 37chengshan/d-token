# d-token 桌面控制台 UI（修订版）

## Context

`37chengshan/d-token` 是本地优先的 AI 编码 token 压缩 / 路由 / 可观测性桌面应用（Tauri 2 + React + Rust）。它在 AI 编码 agent（Claude Code、Codex、Cline、OpenCode 等）与 API provider 之间做多层压缩（RTK → Headroom runtime → route overhead）并做路由，记录每层的 token 节省与成本。当前实现处于 fixture-first 阶段（M1–M3），核心机制：
- 本地 runtime/proxy 由 Headroom 派生并被 d-token 打包管理（用户无需手动装 Headroom/RTK）；`headroom proxy --port 8787` 式的压缩代理。
- `detect_tools()` 用 `which`/`where` 探测 `rtk`/`headroom`/`opencode` 二进制及 `--version`；`/compatibility/agents` 返回每个 agent 的 RTK/Headroom 支持级别、平台支持（macOS 原生 / Windows 原生 / WSL）、配置方式（hook / 环境变量 / 配置文件）。
- 自动配置 agent = 按 agent 类型注入环境变量 / 打补丁配置文件 / 安装 hook。
- cc-switch：外部的 provider 切换工具，带本地代理端口；d-token 需自动识别它并决定 Claude 是走代理还是直连。
- Headroom 无命名"等级"，实际是 `HEADROOM_OUTPUT_SHAPER=1`（开/关）+ `HEADROOM_OUTPUT_HOLDOUT` 比例 + `headroom learn --verbosity`；d-token 将其封装为自有 `OutputVerbosity`（raw/summary/minimal）与压缩 level（off/safe/balanced/aggressive/max）。

本任务：在 Figma Make 中构建 d-token 的**前端原型**（fixture/mock 数据，纯前端 PureFrontend），视觉风格参考用户两张截图（图标侧栏、玻璃卡片、统计卡、活跃度热力图、设置页主题/语言/主题色切换器）。

设计能力内建到共享外壳，全站继承：**中英文切换**（中文优先）、**主题色切换**（accent 色板 + 分析面板主色）、**深/浅/跟随系统**、**液态玻璃**（需按下方规范做到位）、**交互动画 + 页面切换动画**（motion/react）。

## Sidebar 信息架构（8 项，设置固定底部）

1. **概览 Overview** — 简洁的节省概览：今日/本周 token 节省、累计节省成本、压缩率、活跃 agent 数、runtime/路由/端口状态灯、今日错误；1–2 个小趋势迷你图 + 快捷操作（去安装 / 去诊断）。保持轻量，不堆砌。
2. **安装 Install** — 一键上手主页面：
   - **一键安装并启动服务**：大主按钮，触发 安装→启动 runtime/proxy 的进度流（scan→plan→apply→verify→doctor 的可视化步骤，fixture 模拟）；显示服务运行状态、监听端口、版本。
   - **一键检测环境**：探测已安装的 agent（Claude Code/Codex/Cline/OpenCode，含二进制路径/版本/平台支持）、检测 cc-switch 是否存在及其代理是否开启、Headroom/RTK 是否就绪。结果以卡片/表格展示（状态徽章 + 证据）。
   - **选择并配置 agent**：检测出的 agent 列表带复选；用户勾选后点"配置"，对每个 agent 展示将采用的配置方式（hook/env/配置文件）与 diff 预览，确认后"自动配置"（fixture 模拟成功/失败态）。
3. **高级 Advanced** — 分区/Tabs（包含但不限于）：
   - **服务与开机启动**：开机自启动开关（macOS LaunchAgent / Windows 服务，fixture 文案）、绑定 host（默认 127.0.0.1）、端口、重启/停止服务。
   - **cc-switch 自动转发**：自动识别 cc-switch 路由接口开关；"自动模式"——按检测结果决定 Claude 是否经代理（有时需代理有时不需要）；显示当前判定与手动覆盖（强制直连 / 强制走 cc-switch）。
   - **Headroom 项目扫描**：选择/扫描项目根目录 → 分析体量与 token 使用 → 推荐 `OutputVerbosity`/压缩 level（含置信度）→ 一键应用并配置（映射 `HEADROOM_OUTPUT_SHAPER`/`HOLDOUT`）。展示每项目策略列表与 feature 开关（ccr/memory/learn/output_shaper/cache_aligner）。
   - **Providers & Routes（高级）**：provider 列表（官方/自定义/cc_switch/local_chain，健康状态、凭据存储位置、脱敏）、路由链路构建器（Agent→压缩→路由适配→目标，chain 模式）、端口发现（检测表+置信度+证据）。红线：自定义 URL 绝不冒充官方；WebView 不明文展示 key。
4. **分析 Analytics** — Tabs：`RTK` / `Headroom` / `请求流与节省` / `趋势`。分层节省瀑布、请求流链路图、活跃度热力图（参考截图）、recharts 图表；显式"Fixture 数据"标签。
5. **诊断 Doctor** — Tabs：`健康检查`（5 层：Runtime/Port/Route/Provider/Security，状态徽章+可点击修复链接）/ `Runtime`（组件状态、区分 runtime/RTK/route/provider 故障）/ `维护`（重启、释放 d-token 拥有的端口/进程、重建注册表、清理失效路由；破坏性操作需确认）。
6. **变更与回滚 Changes & Rollback** — Tabs：`待批变更`（批准/拒绝）/ `配置 Diff`（before/after、风险等级、managed-block 标记）/ `备份与回滚`（时间线 + 受影响文件/端口/进程 + 回滚前后 Doctor 对比）/ `配置归属`（agent 原生 / d-token 数据 / 托管块 / 外部）。
7. **日志与安全 Logs & Security** — Tabs：`日志`（可审计事件流，脱敏，按类型/时间/agent 过滤、导出）/ `密钥凭据`（脱敏预览 `sk-***`、存储位置徽章、rotate/test/remove）。
8. **设置 Settings** — 外观（主题：浅/深/跟随系统；语言：中/English；主题色；分析面板主色；液态玻璃开关）、数据（自动切换、刷新频率、代理配置）、关于（版本 1.1.8）。底部固定。

顶栏：折叠按钮、页面标题、右侧图标操作区（参考截图）。全局状态条组件：runtime/当前路由/监听端口/安全模式(fixture/shadow/managed)/Doctor 状态，可展开详情。侧栏左下角：明暗快捷切换（参考截图）。

## 液态玻璃规范（重点优化）

之前效果不佳。正确做法（在 `.glass` / `.glass-strong` utility 与 GlassCard 中实现）：
- 半透明表层：浅色 `background: rgba(255,255,255,0.55)`，深色 `rgba(22,22,26,0.45)`——**不要**过实或纯灰。
- 真实模糊 + 饱和：`backdrop-filter: blur(20px) saturate(160%)`（含 `-webkit-` 前缀）。
- 边缘高光：`border: 1px solid rgba(255,255,255,0.25)`（深色 0.08）；顶部内高光用 `box-shadow: inset 0 1px 0 rgba(255,255,255,0.4)` + 外部柔和投影 `0 8px 32px rgba(0,0,0,0.12)`。
- 让模糊可见：在应用背景放置低透明度的彩色渐变光斑（跟随主题色的模糊 blob），玻璃卡浮于其上才有"液态"质感。
- 圆角 `--radius` 一致；hover 时轻微提升透明度/位移（motion）。
- 提供玻璃开关：`data-glass=on/off` 时 GlassCard 回退为实心 card（可访问性/性能）。
- 分浅色/深色两套变量，避免深色下发灰。

## 技术实现

- 复用 shadcn/ui（`src/app/components/ui/*`：card/tabs/badge/button/switch/table/dialog/progress/tooltip/scroll-area/checkbox/select/sidebar 等），**不重复造基础组件**。
- 图标 `lucide-react`；图表 `recharts` + 自定义热力图网格；动画 `motion/react`（`import { motion } from "motion/react"`）。
- 明暗：已装 `next-themes`（`.dark` 已在 theme.css 定义），light/dark/system。
- 主题色/分析主色：theme.css 追加 accent 变量集，根节点切换 `data-accent` / `data-chart-accent` 覆盖 `--primary`/`--sidebar-primary`/`--ring` 与图表色；组件只引用变量。
- i18n：自建 `I18nContext` + `zh`/`en` 字典（`src/app/lib/i18n.ts`），`useT()`；中文默认；全部文案走字典。
- 数据：集中 `src/app/lib/fixtures.ts`，结构对齐 PRD/代码实体（ProviderProfile / RouteProfile / ProjectProfile / ProjectCompressionPolicy / DtokenEvent / AgentSupportProfile / HeadroomStatus / CompatibilityCase / doctor checks / backups / metrics / heatmap）。UI 标注"Fixture 数据"。
- 页面切换用本地 state（`activePage`）+ motion `AnimatePresence` 做过渡；无需 react-router。

### 文件结构
- `src/app/App.tsx` — 挂载 Providers（Theme/I18n/Accent/Glass）+ AppShell，默认导出。
- `src/app/components/shell/`：`AppShell.tsx`、`Sidebar.tsx`、`Topbar.tsx`、`StatusBar.tsx`、`BackgroundBlobs.tsx`（玻璃背景光斑）。
- `src/app/components/pages/`：`Overview.tsx`、`Install.tsx`、`Advanced.tsx`、`Analytics.tsx`、`Doctor.tsx`、`Changes.tsx`、`LogsSecurity.tsx`、`Settings.tsx`。
- `src/app/components/common/`：`GlassCard.tsx`、`StatCard.tsx`、`Heatmap.tsx`、`FlowDiagram.tsx`、`StatusBadge.tsx`、`DataSourceBadge.tsx`、`StepProgress.tsx`（安装步骤）。
- `src/app/lib/`：`i18n.ts`、`fixtures.ts`、`theme-context.tsx`（accent + glass）。
- `src/styles/theme.css`：追加 accent / chart-accent / glass 变量（仅新增，不改现有 token 语义）。
- `src/styles/fonts.css`：如需字体（顶部导入）。

## 实现顺序
1. 基础设施：i18n、accent/glass context、theme.css 追加变量与 `.glass` utility、fixtures、BackgroundBlobs。
2. 共享外壳：Sidebar（8 项 + 底部明暗切换）、Topbar、StatusBar、AppShell（AnimatePresence 页面过渡）。
3. common 组件：GlassCard/StatCard/Heatmap/FlowDiagram/StatusBadge/DataSourceBadge/StepProgress。
4. 页面：Overview → Install → Advanced → Analytics → Doctor → Changes → LogsSecurity → Settings（各含加载/空/错误态 + fixture）。
5. Settings 接线全部开关并验证联动。

## 验证（Vite dev 已运行，勿手动 build/启动）
预览面板检查：
- 侧栏 8 项切换正常，页面切换有过渡动画；卡片 hover / 列表进入有交互动画。
- **Install**：一键安装按钮触发多步进度并显示服务运行/端口/版本；一键检测列出 agent + cc-switch 代理状态；勾选 agent → 配置 → diff 预览 → 成功/失败态。
- **Advanced**：开机自启开关、cc-switch 自动转发（自动/强制直连/强制代理）、Headroom 扫描 → 推荐等级 → 应用；providers/routes/端口高级配置可见。
- **Overview**：显示简洁 token 节省数据与状态灯，快捷操作可跳转。
- **Settings**：切换语言→全站中英；切换明暗→全站主题；切换主题色→按钮/高亮/侧栏选中色变化；切换分析主色→热力图/图表变化；玻璃开关→GlassCard 玻璃/实心切换。
- **液态玻璃**：卡片有真实模糊+饱和+边缘高光，背景光斑透过玻璃可见，浅/深色均不发灰。
- Doctor 红色项点击跳转到对应修复页（端口/密钥/变更/回滚）。
- 响应式：桌面为主，侧栏可折叠，窄屏收为图标。

---

# 修订 3 — 液态玻璃质感统一 + 深色重做 + 侧栏

## Context（本轮问题）
用户反馈当前 UI：①整体没有液态玻璃质感，很多选中/过渡态仍是"带色填充板"，各页样式不统一像"拼接产品"；②左侧栏点击时选中动画"从最上面掉下来"；③左侧栏仍太宽，需再缩小约 1/3；④深色模式发灰非常难看。
**已确认方向：克制专业风** —— 背景几乎中性（极淡流动），玻璃通透清晰、描边利落；深色用深青灰(slate)基调、单一强调色点缀。

## 目标产出
一套统一的玻璃表面 token 体系 + 一次性替换全站散落的临时填充（`bg-white/40`、`bg-muted/30`、`bg-primary/10`、`bg-background shadow-sm` 等），消除拼接感；修复侧栏动画与宽度；重做深色调色板。

## 1) 统一玻璃与表面体系（`src/styles/theme.css`）
- 降低背景流动强度：`.app-bg` 的 `color-mix` 由 ~22% 降到 **8–10%**，动画放慢（22s→~30s），浅色近中性、深色近深 slate；`BackgroundBlobs` 光斑透明度再降（≤0.15）或移除，保证"背景几乎中性"。
- 重做 `.glass` / `.glass-strong`：更通透（浅色 bg alpha ~0.5→表现清透而非奶白）、**利落描边**（1px，浅色 rgba(255,255,255,.7)/暗部边 rgba(0,0,0,.06)；深色 rgba(255,255,255,.08)）、精细双层阴影；弱化目前过强的白色对角 sheen（降低 `::after` opacity 到 ~0.25 或仅 hover 时增强），避免奶白发糊。统一圆角（卡片 `--radius` 一致）。
- 新增**统一的选项/分段片工具类**（取代各处手写 active 样式）：
  - `.chip`：非激活态（透明底 + hover 轻微玻璃）。
  - `.chip-active`：激活态玻璃框（半透明底 + 利落描边 + inset 顶部高光 + 文字用 `var(--accent-500)`）。
  - `.glass-inset`：卡片内的次级容器（当前用 `border-border bg-muted/*` 的小盒子统一改用它）。
- 目的：所有分段控件、Tab、筛选 pill、选项盒共用同一套类 → 视觉统一。

## 2) 全站替换散落填充（消除拼接感）
用上面工具类替换以下代表性位置（同一模式，批量处理）：
- `src/app/components/ui/tabs.tsx`：TabsList 用 `.glass`，TabsTrigger active 用 `.chip-active`（去掉现在的 `data-[state=active]:bg-white/40...` 长串）。
- `src/app/components/pages/Settings.tsx`：`SegBtn`/`Segmented` → `.chip`/`.chip-active`。
- `src/app/components/pages/Advanced.tsx`：cc-switch 三个模式按钮、`Field` 内输入、项目策略/provider/route/port 的 `border-border/50` 小盒 → `.glass-inset`。
- `src/app/components/pages/LogsSecurity.tsx`：筛选 pill → `.chip`/`.chip-active`。
- `src/app/components/pages/Doctor.tsx`、`Changes.tsx`、`Install.tsx`、`Overview.tsx`：把 `rounded-*border border-border/50`、`bg-muted/30`、`bg-primary/10` 的次级容器统一换 `.glass-inset`；保持卡片本体用 `GlassCard`。

## 3) 侧栏（`src/app/components/shell/Sidebar.tsx`）
- **宽度再缩 ~1/3**：展开 188px → **约 124px**（`w-[124px]`）；折叠保持 56px。相应缩小 logo、隐藏副标题（仅显示名称或 logo），导航文字可略小。
- **修复"从顶部掉下来"**：弃用每个 item 内条件渲染的 `layoutId` motion.span（重挂载导致从原点动画）。改为在 `<nav>` 容器内渲染**单个常驻**的绝对定位玻璃指示器（`.chip-active` 外观），用 `motion` 的 `animate={{ y: activeIndex * itemHeight }}`（或 `top`）做**纵向滑动**；`transition` 用 spring/tween 平滑。容器 `relative`，item 固定高度以便计算。这样切换时指示器在竖直方向平滑移动，绝不从顶掉落。
- 激活项文字/图标用 `var(--accent-500)`；非激活 hover 用轻玻璃。

## 4) 深色模式重做（`src/styles/theme.css` `.dark`）
- 用**深青灰 slate** 基调取代当前近黑纯灰：
  - `--background` ≈ `oklch(0.21 0.02 250)`（深 slate，带极轻冷色）；`--card`/`--popover` 比背景略亮一档；`--muted`/`--secondary` 用同色相中灰；`--foreground` 柔白（非纯白）；`--muted-foreground` 提升对比。
  - `--border` 用低透明冷灰；`--sidebar` 比背景略深。
  - 深色玻璃变量：`--glass-bg` 用 slate 半透明（非纯黑）、`--glass-border` rgba(255,255,255,.08)、sheen 极弱，避免发灰糊白。
- 校验图表（recharts 用 `--chart-*`/`--chart-accent`）、终端(`Terminal` 固定深色，本就 OK)、状态色在 slate 深色下清晰。
- 明暗切换（next-themes，已接好）无缝；accent 与 chart-accent 变量在深色同样生效。

## 不改动
- 信息架构（8 栏）、页面内容与 fixtures、i18n、Analytics(Headroom 迁移)、终端日志、折线图（已无柱状图）保持不变，仅做视觉统一与上述修复。

## 验证（Vite dev 已运行，勿手动 build）
- 背景：浅色近中性、仅极淡流动；玻璃卡通透、描边利落、不奶白发糊；浅/深一致无拼接感。
- 侧栏：宽度明显更窄（~124px）；点击切换时选中玻璃框**纵向平滑滑动**，不再从顶部掉落；文字为强调色。
- 选项统一：Tab、分段控件、筛选 pill、卡内小盒 全部同一玻璃语言。
- 深色：slate 基调、不发灰；文字/图表/状态色清晰；明暗切换无缝。
- 主题色/分析主色/玻璃开关仍全站联动。
