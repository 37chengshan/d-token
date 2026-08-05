# 前端 UI 设计研究

## 1. 设计目标

AiMaMi 类应用不是普通数据后台。它同时承担桌面工具、系统状态观察器、配置编辑器和恢复入口，因此 UI 需要同时满足：

- 高频操作足够快。
- 危险操作足够慢、足够明确。
- 状态来源可解释。
- 页面在窗口较窄时仍可使用。
- macOS 与 Windows 视觉一致，但窗口控制和系统能力符合平台习惯。
- 普通用户看任务，高级用户看证据。

## 2. AiMaMi 的 UI 优势

### 2.1 任务化信息架构

AiMaMi 把账号、额度、路由、会话、MCP、Skills、自定义指令和维护作为用户任务，而不是暴露 Repository、ProcessManager、ProxyConfig 等内部模块。这样能够降低理解门槛。

### 2.2 桌面壳层完整

典型结构为：

```text
窗口壳层
├── 侧边导航
├── 顶部窗口/页面控制
├── 页面内容区
├── 弹窗、抽屉与通知
└── 托盘与系统入口
```

桌面壳层的价值是保持状态、导航、更新提示和系统动作长期可见，而不是每个页面自行实现。

### 2.3 页面按功能懒加载

复杂桌面管理应用页面数量多，图表、编辑器和数据组件体积大。懒加载可以减少首次启动负担，也能让某个页面失败时不影响全部页面。

### 2.4 强调状态与快捷动作

账号、代理、路由和进程类应用需要明确状态徽标和就地操作。优秀设计不是只显示一个绿色圆点，而是同时提供：状态、来源、更新时间、原因和下一步动作。

## 3. d-token 当前 UI 结构

当前生产入口已收敛为六个一级页面：

1. Overview。
2. Install Workbench。
3. Analytics。
4. Doctor。
5. Changes。
6. Settings。

`AppShell.tsx` 已采用页面懒加载、统一 Sidebar、Topbar、StatusBar、PageStage 与 Reduce Motion。近期还引入了：

- `Surface.tsx`：统一内容表面。
- `PageStage.tsx`：页面切换容器。
- `AnimatedSegmentedControl.tsx`：分段选择器。
- `AnimatedNavIcon.tsx`：导航图标动画。
- `shell.css`、`surface.css`：壳层和表面样式。

这说明 d-token 已经从“页面集合”进入“设计系统收敛”阶段。

## 4. 推荐设计系统

### 4.1 三层表面

不要让所有内容都使用同样的玻璃卡片。建议分为：

| 层级 | 用途 | 视觉特点 |
|---|---|---|
| Shell | 窗口、侧栏、顶部栏、状态栏 | 稳定、低对比、固定位置 |
| Surface | 页面分组、数据区、工作台步骤 | 清晰边界、中等层次 |
| Action Surface | 危险确认、diff、Doctor 修复 | 高对比、明确状态色、不可忽略 |

删除到处复用 `GlassCard` 是正确方向。统一的 `Surface` 应通过 variant 表达语义，而不是每页自行堆阴影、透明度和边框。

### 4.2 状态颜色

颜色只能作为辅助，必须配合文字和图标。

- verified：成功/已验证。
- partial：部分数据可用。
- stale：数据过期。
- unavailable：无法取得证据。
- review required：等待用户审查。
- destructive：可能破坏配置或中断服务。

不要把“未检测”“未安装”“未启动”“请求失败”全部显示为灰色，这些状态对应完全不同的用户动作。

### 4.3 字体与密度

桌面开发工具应采用紧凑但不拥挤的密度：

- 页面标题：20–24px。
- 区块标题：14–16px。
- 正文：13–14px。
- 元数据：11–12px。
- 行高不低于 1.4。
- 可点击目标至少 32px，高频按钮建议 36–40px。

代码、路径、hash 和端口使用等宽字体，但不能让整个页面都等宽。

## 5. 页面级建议

### 5.1 Overview

首页只回答核心问题：

- 服务是否正常。
- 当前活动路由是什么。
- 最近请求是否真正经过 d-token。
- 过去一段时间的实际节省和失败率。
- 是否存在必须处理的问题。

推荐结构：

```text
状态摘要
→ 关键 KPI
→ 当前链路
→ 最近事件
→ 待处理问题
```

不应在首页直接展示所有 Capability、Provider 字段和底层配置详情。

### 5.2 Install Workbench

这是 d-token 最重要的页面，应使用步骤式工作台：

1. 选择 Agent。
2. 扫描本机状态。
3. 选择 d-token listener 与上游。
4. 生成配置计划。
5. 审查摘要与 diff。
6. 明确确认。
7. 应用并验证。
8. 展示 rollback point 和 Doctor 结果。

关键要求：

- 应用按钮必须绑定 review revision、before hash 和 after hash。
- 配置发生变化后自动使旧审查失效。
- 未验证时不能用“已连接”文案。
- 失败后直接展示是否已自动恢复。

### 5.3 Analytics

分析页面需要区分：

- 实际请求数据。
- 上游返回 usage。
- d-token 本地估算。
- shadow 预测。
- active 实际节省。

所有图表都应在标题或图例中标明 evidence kind。不能把估算和实际值绘制为同一种线而不说明来源。

### 5.4 Doctor

Doctor 应使用“检查项 → 证据 → 影响 → 动作”结构：

| 检查 | 状态 | 证据 | 建议动作 |
|---|---|---|---|
| listener | verified | PID/port/exe identity | 无需处理 |
| OpenCode config | stale | hash changed after review | 重新生成 preview |
| credential | unavailable | OS store lookup failed | 重新写入凭据 |

自动修复必须单独确认，并进入 Changes。

### 5.5 Changes

Changes 是信任中心，建议提供：

- 变更类型和目标。
- before/after hash。
- 人类摘要。
- 脱敏 diff。
- apply/verify 状态。
- rollback id。
- 当前文件是否仍处于 apply 后状态。
- 恢复按钮及风险说明。

### 5.6 Settings

Settings 不应成为未分类功能堆积区。推荐分为：

- 外观与语言。
- 桌面集成：托盘、单实例、启动时运行、快捷键。
- 数据与隐私：保留期限、日志级别、导出策略。
- 更新与版本。
- 高级：端口、实验能力、开发诊断。

## 6. 响应式与窗口设计

桌面应用也需要响应式：

- 大于 1180px：展开侧栏，多列 Dashboard。
- 980–1180px：紧凑侧栏，两列布局。
- 小于 980px：默认折叠侧栏，单列工作台。
- 最小窗口宽度必须经过真实 Tauri 验收。

避免依赖浏览器全屏假设。顶部拖拽区域、窗口按钮、安全区域和滚动容器需在 macOS/Windows 分别检查。

## 7. 可访问性

- 所有图标按钮必须有可读名称。
- 分段控制器需支持方向键、Home、End 和清晰焦点。
- 动画遵守 `prefers-reduced-motion`。
- 状态不能只依赖颜色。
- Dialog、Popover、Tooltip 必须正确转发 ref。
- 异步操作使用 `aria-live` 或状态文本宣布结果。
- 错误信息应指向具体字段或操作，而不是只弹 toast。

## 8. d-token 当前 UI 缺陷

1. 部分页面仍使用动态对象读取，容易因为字段缺失产生隐式空态。
2. 前端测试存在 DialogOverlay ref 警告，应视为质量缺陷而不是无害噪声。
3. Analytics 和 InstallWorkbench 构建 chunk 较大，可能影响首次打开页面的体验。
4. 高级能力和普通用户任务之间仍需更清晰的渐进式披露。
5. 真实窗口、最小尺寸、系统字体和高 DPI 证据不足。
6. 浏览器预览虽已明确区分，但仍要防止截图被误当成真实功能证据。

## 9. UI 验收清单

- 六个一级页面在 browser preview 和 Tauri runtime 下状态语义一致。
- 断开 runtime 后不显示伪造 Provider、Route、Agent 或请求数据。
- 所有危险操作必须经过 review 和确认。
- 主题切换、语言切换、折叠侧栏不会丢失焦点。
- Reduce Motion 下无不必要位移和弹性动画。
- macOS/Windows 最小窗口无横向溢出。
- 键盘可以完成导航、审查、取消和关闭弹窗。
- Vitest stderr 零 React 警告。
- Playwright 覆盖核心工作台和回退流程。
- 真 Tauri 窗口截图与候选 commit、平台和 runtime evidence 绑定。