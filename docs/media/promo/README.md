# d-token 宣传图需求说明（v2.1 README 配套）

更新时间：2026-08-06

本文件记录 v2.1 README 宣传图的现状与生成提示词。**已生成并入库**：GitHub 社交预览图（og:image）与 README 顶部 hero 横幅；3 张产品实拍截图已取消。其余横幅为程序化 SVG，已在仓库内。

## 1. 素材现状

| 素材 | 位置 | 状态 |
|---|---|---|
| README 顶部 hero 横幅 | `assets/marketing/hero-banner.png` | 已生成，README 最顶部引用 |
| GitHub 社交预览图（og:image） | `assets/marketing/og-social-preview.png` | 已生成，待上传 GitHub Settings |
| 横幅 01 本地优先控制面 | `assets/marketing/01-local-first-control-plane.svg` | 已存在，保留 |
| 横幅 02 显式路由链路 | `assets/marketing/02-explicit-routing-pipeline.svg` | 已存在，保留 |
| 横幅 03 可观测性与代码智能 | `assets/marketing/03-observability-and-code-intelligence.svg` | 已存在，保留 |
| 横幅 04 安全配置与回退 | `assets/marketing/04-safe-config-and-rollback.svg` | 已存在，保留 |
| 横幅 05 Context Relay（v0.3 规划） | `assets/marketing/05-context-relay.svg` | 本轮新增（SVG） |
| 横幅 06 Optimization Receipt | `assets/marketing/06-optimization-receipt.svg` | 本轮新增（SVG） |
| 旧 ChatGPT PNG ×8 | `docs/media/promo/` | 历史素材，README 未引用，暂保留 |

## 2. GitHub 社交预览图（og:image）——已生成

用途：GitHub 仓库设置 → Social preview（尺寸 1280×640 显示）与 Open Graph 分享。

**文件已就位**：`assets/marketing/og-social-preview.png`。下一步：在 GitHub 仓库 Settings → Social preview 上传该文件。

生成提示词（如需重新生成时使用）：

> （提示词：一张 d-token 的 GitHub 社交预览图，16:9 横幅，深色科技风。背景使用从 #0b1220 到 #1e293b 的深蓝黑渐变，带极淡的细网格纹理。左侧放置品牌 logo：一个珊瑚橙到橙色渐变圆角方块，内含白色圆环加字母 "d" 的 token 图标。大标题用白色粗体无衬线字体写 "Less context. Same intent. Full control."，副标题用浅灰色写 "A local context control plane for AI coding agents"。右侧留出三分之一的构图空间放一个模糊化的深色桌面应用界面卡片（仅示意，不出现真实数据或文字细节）。整体风格克制：不使用光晕、渐变流场、星座或多余装饰，仅使用珊瑚橙 #ea580c、天蓝 #2563eb 与白/灰文字。无人物，无文字水印。）

## 3. README 顶部 hero 横幅——已生成

**文件已就位**：`assets/marketing/hero-banner.png`，已在 `README.md` / `README-cn.md` 最顶部引用（logo 与标题上方）。横幅内容包含品牌 logo、"Less context. Same intent. Full control." 口号、connect / optimize / route / observe 四项能力与右侧控制面界面示意。

1. 深色/浅色均可，但全 README 内建议统一深色主视觉。
2. 主强调色只用珊瑚橙（#ea580c 系），成功/警告/错误色仅表达状态。
3. 禁止大面积渐变、厚阴影、发光、玻璃卡片嵌套；保留克制的面板与细边界。
4. 截图若含敏感内容（token、路径、密钥、完整 prompt），先脱敏再使用；README 只展示 UI 预览。

## 5. 生成后收尾

1. 图片放入 `assets/marketing/` 后，如需在 README 正文引用，告诉我图片文件名，我更新 `README.md` 与 `README-cn.md` 的图注。
2. 若不需要实拍图，README 保持当前纯 SVG 横幅方案即可，本文件第 3 节可忽略。
