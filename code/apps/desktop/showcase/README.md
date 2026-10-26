# d-token 桌面端纯 HTML 原型展示与重构套件

本目录是 `d-token` 桌面端应用前端（React + Tailwind）转换出的**纯静态 HTML/CSS 原型工程**。

它的核心目的是让您可以**彻底脱离 Tauri 桥接环境、脱离 Rust 本地服务依赖、脱离 React 复杂的 Hook 与编译链路**，直接在浏览器中自由重构整个应用的 UI 布局、信息架构、视觉细节与动效节奏。

---

## 1. 快速开始预览

### 方式一：直接双击打开（最简单）
在系统访达（macOS Finder）或文件管理器中，直接双击 `index.html`，或在终端中执行：

```bash
open code/apps/desktop/showcase/index.html
```

> **注意**：`index.html` 中所有 8 大页面、引导向导和托盘面板内容均已做完全自包含与内嵌，采用 `file://` 协议直接打开也能 100% 正常渲染与交互，零网络请求。

### 方式二：本地静态服务（可选）
如果您习惯使用本地 HTTP 服务预览：

```bash
cd code/apps/desktop/showcase
# 使用 Python 启动
python3 -m http.server 8080
# 或使用 Node serve
npx serve .
```
然后在浏览器中访问 `http://localhost:8080` 即可。

---

## 2. 目录结构与功能划分

```text
code/apps/desktop/showcase/
├── index.html                 # 主控制台：整合 AppShell、顶栏快速切换工具条、8 大页面与弹窗
├── app-bundle.css             # 完整预编译的 Tailwind CSS + d-token 设计系统变量（178KB 自包含）
├── showcase.css               # 展示台扩展样式、弹窗遮罩、切换动画、暗色与高亮规则
├── showcase.js                # 原生 Vanilla JS 交互（页面切换、深浅色、侧栏折叠、弹窗、子标签）
├── assets/
│   └── dtoken-cat-icon-96.png # 本地猫咪品牌图标
└── pages/                     # 独立模块化 HTML 文件（便于针对单一页面做独立精细重构）
    ├── overview.html          # 总体概览：4 组 KPI、24h 流量与压缩走势折线图、活动热力图、上游状态
    ├── install.html           # 安装与连接工作台：4 个子标签（一键配置、智能学习、高级端点、安全日志）
    ├── codebase-memory.html   # 代码图谱（实验性）：实体/关系拓扑、Graph Trace 交互查询控制台
    ├── analytics.html         # 分析页面：削减趋势、模型计费效益表、延迟网络分解、Agent 流量占比
    ├── visual.html            # 视觉压缩 (Beta)：Optical 引擎状态、Shadow 模式、图像对比检视器
    ├── doctor.html            # Doctor：100 分自检仪表盘、6 层安全诊断项、诊断终端输出视窗
    ├── changes.html           # 变更与回退：还原点清单、前后配置 Diff 视窗、回退确认弹窗
    ├── settings.html          # 系统设置：外观主题、强调色、127.0.0.1 端口安全、凭据库、数据导出
    ├── onboarding.html        # 首次引导向导：欢迎屏幕、环境检测、Agent 扫描、一键进入
    └── tray-panel.html        # 系统托盘面板：360px 紧凑悬浮窗、服务启停开关、今日节省简报
```

---

## 3. 顶部演示控制条功能

在打开 `index.html` 后，顶部悬浮着一条便捷控制条：
- **页面切换胶囊**：点击 `总体概览`、`安装与连接`、`代码图谱` 等直接平滑切换主体视口。
- **🚀 引导向导**：点击弹出全屏新手配置向导（含 3 步进度指示器）。
- **📌 托盘面板**：点击弹出 360px 紧凑浮窗托盘监视器。
- **☀️/🌙 切换外观**：点击即刻在深色青灰模式与明亮浅色模式之间切换，方便您检验双色下的可读性。
- **收起/展开侧栏**：模拟 980px 或紧凑窗口下的 Icon-rail 图标轨形态。

---

## 4. 如何重构

您可以根据自己的设计目标，在以下两处进行修改：
1. **整体结构修改**：直接在 `index.html` 中重构 `<aside class="shell-sidebar">`、`<header class="shell-topbar">` 或各 `<section id="page-...">` 区域；
2. **单一页面修改**：在 `pages/*.html` 中独立编辑单一模块；
3. **视觉样式修改**：在 `showcase.css` 中增加自定义样式或调整 CSS 变量（如颜色、间距、圆角、阴影）。

---

## 5. 重构后的回填工作流

当您完成 HTML/CSS 的重构后，请直接在会话中告诉我：
> “我已经重构好了 showcase 中的 HTML/CSS，请帮我把新的样式和结构同步回应用前端组件。”

我将：
1. 精准提取您调整过的 DOM 结构与样式类名；
2. 映射更新到 `code/apps/desktop/src/app/` 对应的 React 组件中；
3. 恢复动态数据绑定与 Tauri 桥接，保留全部现有业务逻辑与测试契约；
4. 运行 `npm run test:run`、`npx tsc --noEmit`、`npm run build` 和 Playwright 自动化验证，确保端到端无损回填。
