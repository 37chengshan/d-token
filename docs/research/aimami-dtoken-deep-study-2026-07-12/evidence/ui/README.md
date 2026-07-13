# AiMaMi UI 截图证据

更新时间：2026-07-12  
参考项目：[`borawong/AiMaMi`](https://github.com/borawong/AiMaMi)  
研究源码基线：`297c7af56f10fb371b77bc9b6b65aa320afcbe7e`

本目录保存用户提供的 AiMaMi 真实界面截图，用于 d-token 的逐页视觉对照。截图只证明参考界面的布局、密度、表面、控件和交互语言，不证明 AiMaMi 或 d-token 的运行功能，也不得将截图中的示例数字复制为 d-token 数据。

## 页面索引

| 文件 | 页面/状态 | 重点观察 |
|---|---|---|
| `01-dashboard-activity-year.png` | 仪表盘 / 活跃趋势 / 年 | 连续 Shell、四格 KPI、主内容分区、全年热力图 |
| `02-dashboard-tools-week.png` | 仪表盘 / 工具 / 本周 | 分段控件、条形排名、圆环占比、纯色画布 |
| `03-dashboard-token-week-detail.png` | Token 趋势细节 | 图例、柱线组合、tooltip、汇总指标 |
| `04-account-management-redacted.png` | 账号管理 | 工具栏、搜索/筛选、单实体卡片、操作密度；真实邮箱已遮盖 |
| `05-maintenance.png` | 维护工具 | grouped action rows、语义图标、危险/禁用动作 |
| `06-skills-management.png` | Skills 管理 | 汇总指标条、长列表、行级操作、页面滚动；本机根目录已遮盖 |
| `07-mcp-management.png` | MCP 管理 | 概览指标、协议/状态标签、配置列表；本机 MCP 名称、命令和地址区域已像素化 |
| `08-settings-primary.png` | 系统设置 | grouped settings rows、分段控件、色板、开关 |
| `09-settings-secondary.png` | 系统设置重复证据 | 与 08 内容一致，保留作为用户提供证据完整性核对 |

## 采用边界

- 采用：纯色画布、连续侧栏与 workspace、间距/字号/边界关系、grouped rows、分段控件、状态标签、图表与二级 UI 的完成度。
- 不采用：AiMaMi 品牌、Logo、人物、顶部推广入口、Codex 单产品信息架构、账号数据、示例数值、用户路径和任何敏感配置。
- d-token 仍以 `docs/PRD.md` 为唯一产品契约，真实数据只能来自 d-token-owned runtime、Tauri command 或 Event Ledger。
- 原始截图包含真实邮箱、本机路径和 MCP 配置；仓库内只保存已遮盖或像素化版本。
