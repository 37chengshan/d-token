# 优秀思路与借鉴矩阵

## 1. 借鉴原则

借鉴 AiMaMi 不等于复制 AiMaMi。应把每个思路拆成四个问题：

1. 它解决了什么用户问题。
2. 它依赖什么产品边界和数据来源。
3. d-token 是否有同样的问题。
4. 如何在 d-token 的安全模型中重新实现。

## 2. 推荐直接吸收的思路

### 2.1 任务化导航

**AiMaMi 优点**：用户按账号、额度、路由、会话、MCP、Skills 和维护进入，而不是按代码模块进入。

**d-token 落地**：保留六页一级导航，在每页内部按任务组织。连接工作台成为 Agent 接入的唯一入口，Changes 成为所有写操作的统一入口。

**验收**：新用户不阅读架构文档也能完成 OpenCode 连接、验证和回退。

### 2.2 页面懒加载

**AiMaMi 优点**：复杂页面不影响首次启动。

**d-token 现状**：`AppShell.tsx` 已使用 `React.lazy` 和 Suspense。

**继续优化**：拆分 Recharts、InstallWorkbench 和 review 逻辑；设置 chunk budget。

### 2.3 完整桌面壳

**AiMaMi 优点**：侧栏、顶部、托盘、快捷键、更新和系统状态形成一致体验。

**d-token 落地**：继续完善 Sidebar、Topbar、StatusBar、Tray 和 updater；所有入口共享同一 Runtime Evidence。

### 2.4 单实例

**价值**：避免两个应用窗口同时修改配置或管理同一 sidecar。

**d-token 落地**：第二实例将焦点交给第一实例；任何写操作只由主实例执行。仍需后端文件级并发保护，单实例不是唯一防线。

### 2.5 托盘状态

**价值**：本地 proxy 和 Agent 连接是长期运行状态，用户不必保持窗口打开。

**d-token 落地**：托盘只显示经过验证的 service/route 状态，并提供打开应用、启动/停止服务和退出。停止动作仍验证身份。

### 2.6 更新体验

**价值**：桌面应用需要安全升级，用户不应手工替换 app 和 sidecar。

**d-token 落地**：签名 updater、版本说明、sidecar hash、失败恢复和禁止未签名降级。

### 2.7 账号/Provider 状态的卡片表达

**价值**：复杂来源需要统一摘要。

**d-token 落地**：把 Provider、Route、Instance 和 Agent connection 统一为状态卡，但必须显示 source/freshness/state，不做账号运营工具。

### 2.8 MCP/Skills 管理的产品表达

**价值**：用户理解的是“为 Agent 增加能力”，不是编辑 JSON。

**d-token 落地**：未来可在 Compatibility/Install Workbench 中显示检测、兼容性和受控配置，但不接管第三方生态全部生命周期。

## 3. 应改造后吸收的思路

### 3.1 Repository 模式

**可取**：集中路径发现、读取和领域数据访问。

**风险**：容易膨胀成上帝模块。

**d-token 方案**：按 Agent、transaction、runtime、evidence 拆 repository/service，不设全局万能 Repository。

### 3.2 Process 管理

**可取**：把终端操作转为可视化启动、停止和日志。

**风险**：仅凭 PID/名称误杀进程。

**d-token 方案**：PID + executable + port + manifest + runtime identity 五重验证。

### 3.3 Proxy 控制

**可取**：用户在应用中看到代理是否运行、监听在哪、流量是否经过。

**风险**：代理和路由规则混在 UI 状态中，出现伪运行。

**d-token 方案**：Proxy lifecycle 与 RouteDecision 分开，只有真实 routed request 才能证明链路。

### 3.4 配置编辑器

**可取**：减少用户手工编辑。

**风险**：覆盖未知字段、格式和注释。

**d-token 方案**：Agent Adapter 生成最小 patch，严格 preview/review/hash/backup/apply/verify/rollback。

### 3.5 配额与成本图表

**可取**：让用户理解用量和价值。

**风险**：把估算、shadow 和 actual 混合。

**d-token 方案**：所有指标带 evidence kind；实际节省只来自满足归因条件的请求。

## 4. 不应照搬的思路

### 4.1 宽泛 Shell 权限

不应提供通用 `execute_command` 给前端。命令必须固定模板、白名单和后端参数验证。

### 4.2 直接覆盖第三方完整配置

不应将整个配置解析后重新序列化覆盖，尤其是 JSONC/TOML/带注释格式。只修改托管块或最小字段。

### 4.3 账号池和自动轮换作为核心

d-token 的产品定位是压缩、路由和安全控制面，不应转向账号运营工具。可支持多个 Provider/Instance，但必须显式选择和可解释决策。

### 4.4 只用 PID 控制进程

PID 可复用，端口也可能被其他进程占用。必须验证完整身份。

### 4.5 CSP 长期关闭

开发方便不能成为 release 默认。d-token 应有 release CSP gate。

### 4.6 把动画当成成功证据

视觉上的绿色、完成图标和 toast 不能先于 verify receipt。

## 5. d-token 自身值得保留并强化的优秀思路

### 5.1 Evidence-first UI

任何状态都带 source、freshness 和验证等级。这是 d-token 区别于普通 Dashboard 的信任资产。

### 5.2 Hash-bound Review

应用内容必须与用户审查内容一致。OpenCode adapter 的 revision + before/after hash 是高价值设计，应推广到所有 Agent。

### 5.3 精确回退

恢复原始 bytes、存在性和权限，而不是“重新生成看起来相同的配置”。

### 5.4 无静默 fallback

路由不明确时拒绝，避免请求误发官方、错误 Provider 或自环。

### 5.5 Browser Preview 诚实边界

浏览器模式只证明 UI，不渲染为本机真实状态。

### 5.6 Optical Capability Gate

有损优化不是开关即生效，而是由 route/model capability、签名、eval、经济、资源和 post-validation 共同授权。

### 5.7 Signed Savings Attribution

允许负节省，区分估算与实际，不重复计算多个 transform 的贡献。

## 6. 借鉴矩阵

| 思路 | AiMaMi 价值 | d-token 决策 | 优先级 | 关键改造 |
|---|---|---|---|---|
| 任务化导航 | 高 | 采用 | P0 | 保留六页，内部改为步骤任务 |
| 页面懒加载 | 高 | 已采用并继续 | P1 | chunk budget |
| 托盘 | 高 | 采用 | P2 | 只显示 verified 状态 |
| 单实例 | 高 | 采用 | P1/P2 | 主实例写入 + 文件事务 |
| Updater | 高 | 采用 | P0/P2 | 签名、sidecar hash、回退 |
| 全局快捷键 | 中 | 可选 | P2 | 默认关闭、可撤销 |
| 账号管理 | 高但边界不同 | 不做核心 | — | 只做 Provider/Instance |
| 配额卡片 | 中 | 改造采用 | P2 | evidence kind |
| Repository 中心层 | 开发快 | 不照搬 | P0 | 拆 service/adapter |
| Shell/Process 宽权限 | 功能强 | 不照搬 | P0 | 最小 capability |
| 配置 GUI | 高 | 强化采用 | P0 | transaction contract |
| 会话索引 | 中 | 选择性采用 | P2 | metadata-only、上游所有权 |
| MCP/Skills 管理 | 高 | 选择性采用 | P2 | Compatibility/Adapter |
| 动画壳层 | 高 | 已适配 | P1 | Reduce Motion、真窗口测试 |

## 7. 优秀思路的实施模板

每次吸收外部思路，新增一份 design note，包含：

```text
Problem
User journey
Source project and fixed commit
What is copied / adapted / independently implemented
License and trademark boundary
d-token security constraints
Data source and evidence
Failure semantics
Tests
Release gate
```

## 8. 评估标准

一个借鉴是否成功，不看“做得像不像 AiMaMi”，而看：

- 是否减少用户完成任务的步骤。
- 是否没有降低安全边界。
- 是否使用真实数据合同。
- 是否跨平台可用。
- 是否可测试、可回退。
- 是否保留来源与许可证。
- 是否仍符合 d-token PRD。