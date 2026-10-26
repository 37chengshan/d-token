# d-token 完整应用页面 Figma 导入指南

本指南指导您将 `d-token` 现存应用的全部 10 大页面（8 大主页面 + 首次引导 + 系统托盘浮窗）完整、无损地导入到 Figma 画布中。

- **关联 Figma 文件地址**：
  `https://www.figma.com/design/oIccsjtjaHJg86RlrGDY4P/d-token-%C2%B7-%E5%BD%93%E5%89%8D%E7%95%8C%E9%9D%A2%E4%B8%8E%E4%BA%94%E9%A1%B5%E9%87%8D%E6%9E%84-%C2%B7-2026-09-19?node-id=12-6&m=dev`

---

## 方案一：1:1 像素级高清画板拖入（最快捷，推荐用于视觉参照与走查）

在导出目录（`support/artifacts/figma-export/` 或 `code/apps/desktop/showcase/exports/`）中，已包含由 Playwright 以 Retina 2x 自动化截取的 20 张高清画板（1280x800）：

### 浅色模式（Warm Minimalist，底色与 Figma 节点 12:6/12:2 严格一致 #e9e5dc）：
1. `01-overview-light.png`：总体概览（4 组 KPI、折线趋势图、活动热力图、上游代理状态）
2. `02-install-light.png`：安装与连接工作台（一键事务配置、智能学习、高级端点）
3. `03-codebase-memory-light.png`：代码图谱（拓扑图示、实体关系树、Graph Trace 控制台）
4. `04-analytics-light.png`：分析（削减走势、模型计费效益表、延迟网络分解）
5. `05-visual-light.png`：视觉压缩 (Beta)（Optical 引擎状态、图像对比检视器）
6. `06-doctor-light.png`：Doctor（健康自检仪表盘、诊断项清单、诊断终端输出）
7. `07-changes-light.png`：变更与回退（还原点清单、前后配置 Diff 视窗、回退确认）
8. `08-settings-light.png`：系统设置（外观主题、强调色、端口安全、凭据库）
9. `09-onboarding-light.png`：首次引导向导（环境检测、Agent 扫描、一键进入）
10. `10-tray-panel-light.png`：系统托盘面板（360px 紧凑悬浮窗、服务启停、今日节省）

### 深色模式：
- `01-overview-dark.png` 至 `10-tray-panel-dark.png`（对应深色青灰/碳黑质感）

**导入步骤**：
直接将上述 PNG 文件全选，拖入到您的 Figma 画布中，排列在 `12:2` 画板旁即可直接作为 1:1 精确对照底图！

---

## 方案二：通过 Figma "html.to.design" 插件导入 100% 全矢量可编辑图层（推荐用于重构与组件提取）

如果您希望在 Figma 中获得**可编辑的文字图层、矢量矩形、Auto Layout 布局结构、CSS 变量色彩**：

### 步骤：
1. **在 Figma 中启动插件**：
   在 Figma 菜单中选择 `Plugins` -> 搜索并打开 `html.to.design`。
2. **选择导入方式**：
   - **方式 A (直接上传文件)**：
     点击插件界面的 `Import HTML file`，选择本导出目录下的 `*-light.html` 或 `*-dark.html` 单页文件，插件将自动解析 DOM 并生成全矢量 Figma Frame。
   - **方式 B (本地预览实时导入)**：
     在终端启动本地静态服务：
     ```bash
     cd code/apps/desktop/showcase
     npx serve .
     ```
     然后在 `html.to.design` 插件中输入 `http://localhost:3000`，输入视口宽度 `1280`，高度 `800`，点击 `Import` 即可将当前页面一键转换为 Figma 全矢量画板！
