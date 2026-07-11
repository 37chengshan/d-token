# d-token Desktop Design System

状态：工作规范  
适用范围：Windows / macOS 共享 React UI  
更新日期：2026-07-11

## 1. 设计目标

d-token 是本地 AI Coding 路由与压缩控制面，不是营销站、测试面板或命令行外壳。界面必须同时具备：

- **简洁的科技感**：信息结构清楚，状态与数据优先，界面元素克制。
- **亲和力**：暖色画布、自然语言、舒展排版，避免冷硬的运维后台观感。
- **神话感**：以低对比星雾、微弱余辉和空间景深表达“观测台 / 神谕台”意象，不使用浮夸符号、厚重光效或装饰性文案。
- **事实一致性**：没有运行证据时显示 `—`、原因与数据更新时间；禁止用 mock、fixture、硬编码曲线或零值冒充真实数据。

## 2. 研究依据与取舍

本规范参考公开且可核验的资料，但不复制 Anthropic 的商标、专有字体或受保护资产。

1. Anthropic 官方介绍强调：设计系统应从代码和设计文件中提取，并一致应用颜色、排版与组件；间距、颜色和布局应可被细粒度调整。[Introducing Claude Design](https://www.anthropic.com/news/claude-design-anthropic-labs)
2. assistant-ui 的开源 Claude 示例总结了产品界面的公开视觉特征：暖米色画布、编辑感字体、细边框、极少阴影、hover 才出现的次级动作，以及稀缺的珊瑚色主强调。[Claude Clone](https://www.assistant-ui.com/examples/claude) / [source](https://github.com/assistant-ui/assistant-ui/blob/main/apps/docs/components/examples/claude.tsx)
3. 社区维护的 Claude 设计映射提供了颜色、排版、表面与交互状态的参考，但它不是 Anthropic 官方规范，只作为方向交叉验证。[Community DESIGN.md](https://github.com/voltagent/awesome-design-md/blob/main/design-md/claude/DESIGN.md)
4. `prefers-reduced-motion` 是跨 Safari、Chromium 与 WebView 的标准用户偏好；非必要运动必须被移除、减弱或替换。[MDN](https://developer.mozilla.org/en-US/docs/Web/CSS/@media/prefers-reduced-motion)

对 d-token 的关键取舍：

- 保留“暖画布、编辑感排版、稀缺暖色强调、低阴影”的通用语言。
- 不照搬 Claude 的聊天布局；d-token 仍是桌面数据与任务控制面。
- 不引入 Copernicus、Styrene 等专有字体；使用跨平台合法字体栈。
- 不使用 Anthropic 黑色放射标志、Claude 名称或一比一品牌组件。

## 3. 信息层级

### 3.1 总体概览

概览是**总体数据驾驶舱**，不是功能入口页。

首屏顺序：

1. 数据来源、完整性与更新时间。
2. 总请求量、节省 Token、压缩率、失败率或总体健康度。
3. 本地服务、Agent、路由、上游的汇总状态。
4. 趋势、近期活动与异常。

大型“前往安装”“开始配置”入口不得主导概览。必要的上下文链接只能作为次级文本动作。无真实数据时整块展示任务型空态，不绘制假曲线、假排行或 `0` 值墙。

### 3.2 任务页面

安装、配置、诊断、变更、回退、日志与导出分别落在对应页面。每页必须回答：

- 我在哪里？
- 当前事实是什么？
- 我现在能完成什么任务？
- 完成或失败后如何验证？

页面默认只有一个主要动作。危险动作必须有精确目标、差异、备份与恢复信息。

## 4. 排版

采用“编辑感标题 + 人文系统正文”的混合体系：

```css
--font-display: ui-serif, "Iowan Old Style", "Palatino Linotype",
  "Songti SC", STSong, serif;
--font-ui: -apple-system, BlinkMacSystemFont, "Segoe UI", "PingFang SC",
  "Microsoft YaHei", sans-serif;
--font-mono: "SFMono-Regular", Consolas, "Liberation Mono", monospace;
```

- 页面标题：`--font-display`，24–28px，500，紧凑但不挤压；中文不得使用夸张粗宋体。
- 区块标题：`--font-ui`，16–18px，600。
- 正文：`--font-ui`，14–15px，400，行高 1.5–1.6。
- 标签与按钮：13–14px，500；不得用 700 作为默认强调。
- KPI 数字：`--font-ui`，600，启用 tabular numerals；只有代码、真实路径和日志使用等宽字体。
- 工程词不得靠全大写和超宽字距制造“科技感”。

macOS 与 Windows 字形会由系统字体决定，但字号、权重、行高、字距和布局语义必须一致。

## 5. 色彩与表面

### 5.1 色彩角色

- 画布：暖米白 / 暖灰黑，不使用纯白或冷蓝灰作为全局基底。
- 主文本：接近黑的暖墨色；深色模式使用柔和米白。
- 品牌强调：低饱和珊瑚橙 / 琥珀，仅用于主要动作、焦点、选中指示与少量关键数据。
- 成功绿：仅表示已验证成功；不得作为导航选中或普通 hover 色。
- 警告黄、错误红：只表达语义状态，并同时提供图标或文字，不能只靠颜色。

### 5.2 表面规则

- 首选细边界和明度差，阴影只用于窗口层级、浮层和少量关键卡片。
- 禁止在玻璃卡片内连续嵌套多个玻璃卡片。
- 同一屏不同时出现大面积渐变、厚阴影、彩色描边和背景流动。
- 选中态只使用“一层轻染色 + 一个清晰指示”；不得同时使用整块高饱和填充、底线、发光和阴影。

## 6. 交互状态

每个交互组件必须区分：default、hover、focus-visible、pressed、disabled、selected。

- Hover：背景或边界轻微升温；不放大、不漂浮。
- Focus visible：3px 低透明暖橙外环，必须在浅色和深色中可见。
- Pressed：颜色略加深，可选最多 1px 位移；不得弹跳。
- Selected：柔和暖色表面 + 高对比文字 + 单一指示。
- Disabled：降低对比度但仍可读，并说明不可用原因。
- 次级动作可在上下文 hover/focus 时出现，但键盘用户必须同样能访问。

## 7. 动效

```text
instant    100–120ms  按下、图标反馈
fast       140–180ms  hover、focus、短淡入
standard   180–220ms  页签、折叠、对话框
ambient    64–96s     低对比背景星雾循环
```

- 只为状态变化、空间关系和注意力转移使用动画。
- 常规交互只允许颜色、边界、透明度和 1–2px 位移。
- 禁止大面积背景 position 重绘、持续高对比渐变横扫、弹簧反复回弹。
- 星雾层使用 GPU transform，速度不同但幅度很小；不得经过正文形成明显色带。
- `prefers-reduced-motion: reduce` 下，星雾与非必要 transform 必须完全静止；页面切换只保留即时显示或极短淡入。

## 8. 背景与“神话感”

神话感来自空间氛围，不来自题材堆砌：

- 一层暖中性画布。
- 两到三层低透明星雾 / 极光，分别承担暖余辉、冷景深和微弱高光。
- 不同层使用 64–96 秒低频运动，移动范围小于自身尺寸的 8%。
- 内容区域始终保持稳定对比度；背景不得让卡片边界忽明忽暗。
- 不使用星座、符文、神明名称等与任务无关的产品文案。

## 9. 空态与真实性

空态必须包含：事实、影响、唯一下一步。

正确示例：

```text
暂无真实请求记录
连接本地服务并发送请求后，这里会显示趋势。
```

禁止：

- 生成随机或硬编码日期曲线。
- 将缺失值渲染为 `0 Token`、`0%` 或 `$0.000`。
- 显示虚构工具排行、日志、终端命令或配置变更。
- 使用“示例数据”填充正式 App；预览也应展示诚实空态。

## 10. 验收清单

- 1180×780、920×640 和全屏下无内容被状态栏覆盖、无卡片残片、无横向滚动。
- 顶部窗口栏与页面内容形成一个连续层，页面标题只出现一次。
- 概览首屏为真实总体数据，不被功能入口占据。
- 浅色、深色和跟随系统均保持对比与层级。
- Tab/Shift+Tab、方向键、Home/End、Enter/Space、Esc 可完成对应任务。
- Reduce Motion 下无背景漂移、页面位移或弹跳。
- 生产窗口和构建产物不含 mock、fixture、benchmark、smoke、fake-bin、testing center 等测试语义。
- 所有数据卡、图表、排行和状态均可追溯到 d-token-owned runtime 或桌面控制面证据。
