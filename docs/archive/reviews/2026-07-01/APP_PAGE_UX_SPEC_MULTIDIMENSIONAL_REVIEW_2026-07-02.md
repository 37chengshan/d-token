# APP_PAGE_UX_SPEC 多维度审查报告

日期：2026-07-02  
对象：`docs/APP_PAGE_UX_SPEC.md`  
参考：`docs/PRD.md`、`docs/PLAN.md`、`docs/OPENCODE_LOCAL_TEST_PLAN.md`、`borawong/AiMaMi`

## 1. 总结结论

当前 `APP_PAGE_UX_SPEC.md` 已经抓住了 d-token App 的核心：不是把命令行照搬成 GUI，而是把 Headroom-derived runtime、Integrated RTK、Provider、cc-switch、自定义转发端口和 OpenCode 测试链路变成用户可理解的可视化流程。

整体方向正确，尤其是：

1. 左侧导航和顶部全局状态条已经具备桌面控制台雏形。
2. Port Discovery 独立成页是正确决策。
3. OpenCode Test Runner 已经成为真实链路验证入口。
4. Dashboard、Doctor、Config Diff、Backup/Rollback 的安全意识较强。

但与 AiMaMi 这类成熟桌面伴侣工具相比，当前 UX spec 仍有三个明显短板：

1. **安装体验不够产品化**：Setup Wizard 仍偏概念说明，缺少 first-run scan、install plan、apply、verify、rollback 的完整 stepper。
2. **本地配置所有权不够清晰**：还没有像 AiMaMi 那样明确区分“原生配置目录”和“App 自有数据目录”，用户不知道 d-token 会改哪些文件、哪些区块、哪些端口。
3. **页面-痛点-动作-结果没有完全闭环**：页面清单已经全，但每个页面还需要补“解决什么痛点、用户点什么、成功后看到什么、失败后怎么恢复”。

推荐评分：

```text
产品方向：8.8 / 10
页面覆盖：8.4 / 10
安装逻辑：6.8 / 10
端口/路由逻辑：8.2 / 10
普通用户理解成本：7.0 / 10
安全/回滚意识：8.3 / 10
可实现性：7.8 / 10
综合：7.9 / 10
```

## 2. AiMaMi 可参考点

AiMaMi 的价值不是简单功能照搬，而是产品叙事和本地工作流组织方式值得学习。

AiMaMi 把自己定义为 OpenAI Codex 的原生桌面伴侣，统一管理账号、路由、会话与本地配置。它的 README 明确指出 Codex 的账号、会话、MCP、Skills、智能路由与中转配置分散在 `~/.codex` 下，用户会被迫手改 TOML / JSON / SQLite；AiMaMi 用 Tauri 2 + React + Rust 把这些高频操作收敛进桌面应用，减少手动改文件风险。

这对 d-token 的启发是：

```text
d-token 不应只说“我们有压缩、路由、端口发现”。
d-token 应明确说“我们把 AI coding agent 的压缩、路由、端口、配置、诊断、回滚，从手改文件变成可视化本地工作流”。
```

AiMaMi README 的核心能力表也很值得借鉴：每个模块都对应一个痛点，例如账号管理解决手改 `auth.json`，智能路由解决中转模型使用，Relay management 解决 Provider 配置、连通性测试、导入导出和路由诊断，自定义指令只管理受控区块并支持预览与回滚。

对 d-token 来说，页面文档应从“页面列表”升级为“页面-痛点-动作-结果”表。

## 3. 当前 UX Spec 做得好的地方

### 3.1 产品目标明确

文档开头已经说明：App 的核心目标不是把命令行搬到 GUI，而是把复杂 runtime、RTK、Provider、cc-switch、自定义转发端口、OpenCode 测试链路变成用户能理解的流程。

这句话是正确的，应保留为 UX spec 的最高原则。

### 3.2 信息架构基本完整

当前左侧导航覆盖：Setup、Agents、Runtime、Providers & Routes、Port Discovery、OpenCode Test Runner、Dashboard、Doctor、Logs、Config Diff、Backup & Rollback、Compatibility Matrix、Settings。

这套导航已经覆盖 d-token 的主要工作流。

### 3.3 Port Discovery 单独成页是正确方向

Port Discovery 页面明确用于识别并展示：

1. d-token runtime 端口。
2. OpenCode server 端口。
3. cc-switch candidate 端口。
4. custom provider 端口。
5. test route 端口。

而且它规定只能默认扫描 loopback，不扫描公网/局域网，不读取完整 prompt/response，不打印 API key，candidate 不自动接入生产 route。

这几个安全边界非常重要，应作为实现红线。

### 3.4 OpenCode Test Runner 方向正确

OpenCode Test Runner 已经把真实链路拆为两条：

```text
OpenCode → d-token test port → custom provider URL
OpenCode → d-token test port → cc-switch → final provider
```

这与 PRD、PLAN、OpenCode Local Test Plan 是一致的。

## 4. 主要缺口

### 4.1 缺少 Home / Overview 页面定义

AiMaMi 实际产品结构中有 Overview 路由，并通过侧边栏组织 Overview、Custom Instructions、MCP、Skills、Maintenance、Settings 等高频页面。

当前 d-token UX spec 有 Setup，但缺少一个安装后用户每天打开都会看到的 Home / Overview。

建议新增：

```text
Home / Overview
  - 当前安全模式：Fixture / Shadow / Managed
  - Runtime 状态
  - 当前 active route
  - 监听端口摘要
  - 今日请求数 / savings / 错误
  - Doctor 总状态
  - 快捷操作：Start runtime / Run OpenCode test / Add provider / View ports
```

原因：Setup 是首次使用流程，Home 是日常控制台。两者不能混用。

### 4.2 Setup Wizard 太轻，安装体验不够闭环

当前 Setup Wizard 只写了模式选择、是否写入真实配置、是否创建备份和回滚点、是否启动本地端口。

但 d-token 的真实安装复杂度远高于普通设置页，需要做成 stepper：

```text
1. Welcome
2. Scan Environment
3. Choose Mode
4. Choose Agent
5. Choose Runtime
6. Choose Provider / cc-switch
7. Port Discovery
8. Install Plan
9. Config Diff
10. Apply
11. Verify
12. Doctor Result
13. Rollback Point
```

每一步都要有：

```text
输入 → 检测 → 风险提示 → 用户确认 → 结果 → 下一步
```

### 4.3 缺少“配置所有权 / Managed Files”页面或区域

AiMaMi 的 README 明确把 Codex 原生目录和 AiMaMi app data 分开：

```text
~/.codex              Codex native
~/.codex/codexmate/   AiMaMi app data
```

这非常值得 d-token 学习。d-token 也应该明确：

```text
Agent native config       原生配置，d-token 只读或受控修改
D-token app data          d-token 自己的数据目录
D-token managed blocks    d-token 受控区块，可 diff / rollback
External configs          cc-switch / OpenCode / Claude / Codex 外部配置
```

建议新增页面或 Settings 子页：

```text
Config Ownership
  - d-token app data path
  - runtime data path
  - agent native config paths
  - route registry path
  - backup path
  - managed block list
  - last patch / last rollback
```

没有这个页面，用户会担心 d-token 到底改了什么。

### 4.4 Port Discovery 需要从“端口列表”升级为“端口关系图”

现在 Port Discovery 的端口卡片设计很好，但还缺少图形化关系：

```text
OpenCode server :4096
  ↓
d-token test route :18180
  ↓
compression runtime
  ↓
cc-switch candidate :xxxx
  ↓
final provider
```

建议 Port Discovery 页面分三栏：

```text
左：Detected Endpoints
中：Route Graph
右：Evidence / Actions
```

每个 candidate 要显示为什么被识别：

```text
Evidence:
  - found in OpenCode config baseURL
  - /v1/models responded 200
  - process name matched cc-switch
  - seen in Event Ledger 12 minutes ago
```

### 4.5 缺少 “Where will my request go?” 路由模拟器

你之前非常重视“用户必须知道请求最终去了哪里”。当前 Dashboard 和 Route Builder 都提到链路，但 UX spec 里没有一个明确的模拟器。

建议在 Providers & Routes 或 Port Discovery 中新增按钮：

```text
Simulate request path
```

用户选择：

```text
Agent: OpenCode
Project: 当前项目
Route mode: custom provider / cc-switch
```

输出：

```text
OpenCode → d-token :18180 → compression → cc-switch :xxxx → final provider
Fallback: disabled
Credential: d-token test credential
Risk: low / medium / high
```

这是降低配置焦虑的关键功能。

### 4.6 Secrets / Credentials 不应只藏在 Settings

当前 Settings 里写了“密钥存储方式”，但 d-token 涉及 official provider、custom provider、cc-switch、local chain，密钥和 header 配置是高风险区域。

建议增加独立页面或 Providers & Routes 子页：

```text
Secrets & Credentials
  - provider key status
  - storage location: keychain / credential manager / local encrypted store
  - last used route
  - redaction preview
  - rotate / remove / test connectivity
```

红线：永远不在 WebView 明文状态里显示完整 key。

### 4.7 Maintenance 页面缺失

AiMaMi 有 System maintenance：诊断、清理、重建 registry、force-quit Codex、修复常见配置问题。

d-token 应该有类似 Maintenance，但要更保守：

```text
Maintenance
  - restart d-token-owned runtime
  - release d-token-owned test ports
  - rebuild route registry
  - clean stale test routes
  - repair managed config blocks
  - export redacted diagnostic package
```

注意：不能默认 force kill 未验证进程。只能 stop d-token-owned process，或用户确认后处理。

### 4.8 安装后更新和发布体验不足

AiMaMi 是真实桌面应用，README 写了 Windows NSIS installer、macOS Universal、Linux best-effort，项目也包含 Tauri bundle 配置。

d-token UX spec 目前 Settings 里有更新通道，但安装/更新页面还不够具体。

建议新增：

```text
Updates & Runtime Version
  - app version
  - runtime version
  - Headroom-derived base commit
  - integrated RTK version
  - update channel
  - release notes
  - rollback previous version
```

## 5. 结合安装流程的推荐页面重构

建议最终导航改成：

```text
Home
Setup Wizard
Agents
Runtime
Providers & Routes
Port Discovery
OpenCode Test Runner
Secrets & Credentials
Dashboard
Doctor
Config Ownership
Config Diff
Backup & Rollback
Maintenance
Compatibility Matrix
Settings
```

其中 P0 页面：

```text
Home
Setup Wizard
Providers & Routes
Port Discovery
OpenCode Test Runner
Dashboard
Doctor
Config Diff
Backup & Rollback
```

P1 页面：

```text
Agents
Runtime
Secrets & Credentials
Config Ownership
Maintenance
Compatibility Matrix
Settings
```

P2 页面：

```text
Historical Analytics
Advanced Logs
Plugin/Extension Center
Theme/Appearance polish
```

## 6. 推荐补充的用户体验路径

### 6.1 第一次安装路径

```text
打开 d-token
  → Home 显示未安装 / 未配置
  → Setup Wizard
  → Scan local environment
  → 选择傻瓜模式
  → 选择 OpenCode / Claude Code / Codex
  → 选择 provider 或 cc-switch
  → Port Discovery 识别现有端口
  → Install Plan
  → Config Diff
  → Apply
  → Verify
  → Doctor green
  → Dashboard 显示 active route
```

### 6.2 自定义 provider 路径

```text
Providers & Routes
  → Add custom provider
  → 填 base URL
  → Health check
  → Port Discovery 标记为 custom provider
  → Route Simulator 显示请求会去哪
  → OpenCode Test Runner 新端口测试
  → Promote test route to production route
```

### 6.3 cc-switch 路径

```text
Port Discovery
  → Auto Detect
  → 发现 cc-switch candidate
  → 查看 evidence
  → Confirm as cc-switch
  → Route Builder 创建 compress_then_cc_switch
  → OpenCode Test Runner 新端口测试
  → Dashboard 显示 final provider
```

### 6.4 失败恢复路径

```text
Doctor red
  → 显示失败阶段：port / route / provider / cc-switch / runtime
  → 展示修复建议
  → 一键打开 Config Diff 或 Port Discovery
  → 修复后 Run Verify
  → 仍失败则 Rollback
```

## 7. 文档结构建议

当前 `APP_PAGE_UX_SPEC.md` 是一份好骨架，但建议下一轮重构成以下结构：

```text
1. Design Principles
2. Product Narrative
3. Global Layout
4. User Personas
5. Page Map
6. First-run Installation Flow
7. Daily Use Flow
8. Page Specifications
9. Port Discovery Design
10. OpenCode Test Runner Design
11. Route Simulator Design
12. Config Ownership Design
13. Error / Empty / Loading States
14. Security & Privacy UX
15. Acceptance Criteria
```

## 8. 最终建议

短期不要推翻 `APP_PAGE_UX_SPEC.md`，而是做一次增强版迭代。

优先级如下：

```text
P0:
  - 增加 Home / Overview
  - 把 Setup Wizard 改成完整安装 stepper
  - 增加 Config Ownership
  - 增加 Route Simulator
  - 强化 Port Discovery 的 route graph 和 evidence drawer
  - 强化 OpenCode Test Runner 的 test report

P1:
  - 增加 Secrets & Credentials
  - 增加 Maintenance
  - 增加 Updates & Runtime Version
  - 补 Error / Empty / Loading 状态

P2:
  - 增加图表、历史分析、主题打磨
```

一句话结论：

```text
当前 APP_PAGE_UX_SPEC 已经像“页面规划”。
下一步要把它升级成“安装 + 配置 + 端口 + 路由 + 诊断 + 回滚”的完整用户工作流蓝图。
```
