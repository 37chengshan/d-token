# d-token 桌面端 UI/UX 设计规范 (Design Specification)

本文档定义了 `d-token` 桌面端控制面的视觉语言、组件体系、颜色系统与交互契约，供设计师或 AI 辅助重构时作为真理源参考。

---

## 1. 核心设计哲学 (Design Philosophy)

1. **Local-First & Developer-Centric (本地优先，开发者导向)**：
   - 界面整体呈现沉浸式深冷青灰（Cool Slate）工业极客质感，避免刺眼的纯黑，采用有深度的层次感；
   - 信息呈现结构紧凑、数据密度适中、无无意义的装饰性留白；
   - 优先展示确定性数据，无假数据、无伪造的“100% 成功”或示例数字。

2. **Micro-Glassmorphism & Crisp Borders (微拟态与精细边框)**：
   - 卡片与面板使用极其微妙的内发光边缘：`box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.10)`；
   - 边框采用半透明浅白：`border: 1px solid rgba(255, 255, 255, 0.12)`；
   - 背景层叠使用层级灰度区分深度，而非重阴影。

3. **Restrained Accent Colors (克制的强调色)**：
   - 主色调采用活力燃橙（Vibrant Orange: `#f97316` / `#ea580c`）；
   - 状态色彩：翠绿（成功/运行中 `#10b981`）、天蓝（代理/请求 `#0ea5e9`）、紫罗兰（实验特性 `#a855f7`）、绯红（警告/回滚 `#ef4444`）。

---

## 2. 色彩系统 (Color Palette)

### 2.1 深色模式 (Dark Mode - 默认)
- **全局背景 (Background)**: `oklch(0.21 0.02 250)` (`#11141c`)
- **面板卡片 (Card Surface)**: `oklch(0.23 0.02 250)` (`#171b26`)
- **次级卡片 (Secondary Surface)**: `oklch(0.27 0.02 250)` (`#222736`)
- **侧边栏 (Sidebar)**: `oklch(0.19 0.02 250)` (`#0d0f17`)
- **主文本 (Foreground)**: `oklch(0.92 0.01 250)` (`#e6e8ee`)
- **弱化文本 (Muted Foreground)**: `oklch(0.60 0.02 250)` (`#94a3b8`)
- **边框线 (Border)**: `oklch(0.30 0.02 250)` (`rgba(255, 255, 255, 0.12)`)
- **输入框背景 (Input BG)**: `oklch(0.25 0.02 250)` (`#1b202e`)

### 2.2 功能强调色 (Functional Accents)
- **品牌橙 (Primary Orange)**: `#f97316`（悬浮: `#ea580c`，软背景: `rgba(249, 115, 22, 0.12)`）
- **健康绿 (Success Green)**: `#10b981` / `#22c55e`（运行正常、完整性校验通过、已连接）
- **代理蓝 (Proxy Sky Blue)**: `#0ea5e9` / `#38bdf8`（HTTP 路由、流量与端口）
- **实验紫 (Experimental Purple)**: `#a855f7` / `#c084fc`（代码图谱、AST 内存）
- **回滚红 (Danger/Destructive)**: `#ef4444` / `#f87171`（回滚配置、解除授权）

---

## 3. 字体排版与阶梯 (Typography)

- **主字体族**: `-apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif`
- **代码与数值等宽字体**: `ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace`
- **字号阶梯**:
  - `大指标 (KPI Large)`: `24px` / `text-2xl`，`font-weight: 700`
  - `页面主标题`: `16px` / `text-base`，`font-weight: 600`，`tracking: -0.018em`
  - `卡片标题`: `14px` / `text-sm`，`font-weight: 600`
  - `标准正文`: `12px` / `text-xs`，`line-height: 1.5`
  - `次要辅助信息 / 徽章`: `11px` / `text-[11px]`，`line-height: 1.2`
  - `极小标识 (Pill Mini)`: `10px` / `text-[10px]`，`font-weight: 600`

---

## 4. 关键组件结构契约 (Component Contracts)

### 4.1 胶囊式分段切换器 (Segmented Control / Chips)
- 外框采用完全圆角胶囊形：`border-radius: 999px`，背景为 `rgba(255, 255, 255, 0.045)`；
- 激活项：背景为橙色微光 `rgba(249, 115, 22, 0.15)`，边框为橙色半透，文字为高亮橙色。

### 4.2 核心指标卡 (MetricStrip & StatCards)
- 卡片统一采用 `rounded-xl border border-border/70 bg-card p-4`；
- 顶部为指标名 + 右侧图标；中间为特大数字 + 涨跌/节省百分比；底部为一句话解释。

### 4.3 状态指示灯 (Status Dot)
- 绿色呼吸灯：`size-1.5 rounded-full bg-emerald-400 animate-pulse`；
- 配合外围徽章：`bg-emerald-500/10 text-emerald-400 border border-emerald-500/30`。

### 4.4 差异视窗 (Diff Container)
- 纯黑半透明底色：`bg-black/40 border border-border/50`；
- 红色删除行：`bg-red-500/15 text-red-400 border-l-2 border-red-500`；
- 绿色新增行：`bg-green-500/15 text-green-400 border-l-2 border-green-500`。
