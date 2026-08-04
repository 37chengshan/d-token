# d-token 与 AiMaMi 差异、缺陷与竞争力分析

## 1. 结论

两者不是同类产品的简单高低关系：

- AiMaMi 更接近成熟的 Codex 桌面伴侣，优势在用户闭环、系统集成和日常可用性。
- d-token 更接近多 Agent 本地运行时和安全控制面，优势在路由、协议、压缩、事务与证据。

当前 d-token 的主要问题不是“功能比 AiMaMi 少”，而是已有深层能力尚未被稳定、易懂、可发布的桌面产品完整承载。

## 2. 多维度对比

| 维度 | AiMaMi | d-token 当前 | 判断 |
|---|---|---|---|
| 产品聚焦 | Codex 单生态深耕 | 多 Agent、多协议 | d-token 范围更大，落地成本更高 |
| 用户旅程 | 账号、额度、路由、会话、MCP、Skills 完整 | 六页控制面，连接路径正在形成 | AiMaMi 更成熟 |
| 桌面系统能力 | 托盘、快捷键、更新、进程等更完整 | Tauri 壳、sidecar、打包存在，系统闭环不足 | AiMaMi 领先 |
| 路由正确性 | 以 Codex 管理为主 | 显式 route/provider/instance，歧义 fail closed | d-token 领先 |
| 配置安全 | 需重点审计完整事务 | hash-bound review、backup、verify、rollback | d-token 明显领先 |
| 凭据 | 本地管理 | OS credential store + opaque ref | d-token 边界更严格 |
| 协议支持 | Codex 相关 | OpenAI Chat/Responses、Anthropic Messages | d-token 更广 |
| 上下文优化 | 非核心 | Hybrid/Optical 深层算法 | d-token 独有 |
| 可观测性 | 状态与管理体验强 | Ledger、Evidence、Doctor、route decision | d-token 技术更强，表达仍需优化 |
| 前端体验 | 成熟统一 | 正在大规模重构并已吸收部分 AiMaMi 思路 | AiMaMi 当前领先 |
| 自动化测试 | 需按源码实际评估 | Rust workspace + 108 前端测试 + Clippy | d-token 当前证据较强 |
| 发布成熟度 | 已形成桌面产品路径 | 签名、真机、同 commit 证据未闭环 | AiMaMi 领先 |
| 维护性 | 部分中心模块较大 | 多 crate 较好，但 Tauri main/核心文件膨胀 | 各有中心模块债务 |

## 3. d-token 相比 AiMaMi 的核心优势

### 3.1 不只是管理工具，而是请求执行层

AiMaMi 管理 Codex 使用环境；d-token 直接处于 Agent 与 upstream 之间，可控制路由、协议、压缩、错误和证据。这意味着 d-token 能解决更底层的问题，也意味着任何错误影响更大。

### 3.2 配置写入的安全性

OpenCode adapter 和 ConfigPatcher 已具备：

- 严格目标发现。
- review revision。
- before/after hash。
- 脱敏 diff。
- durable backup。
- 原子写入。
- post-write verify。
- 精确 rollback。
- 并发修改保护。

这是可推广为平台能力的技术资产。

### 3.3 路由与协议证据

d-token 能记录请求入口协议、目标 Provider、适配模型、route decision、upstream 结果和脱敏 usage。它不仅显示“代理在运行”，还能证明“这次请求走了哪条链”。

### 3.4 Hybrid Context Optimization

d-token 已实现从 Context IR、依赖图、风险分类、经济门到 renderer、patcher、post-validation 的完整链路。即使 active 默认关闭，这也是长期差异化。

### 3.5 Evidence-first

verified/partial/stale/unavailable 的状态模型比普通绿色/红色 Dashboard 更可信，适合高风险本地工具。

## 4. d-token 相比 AiMaMi 的核心缺陷

### 4.1 用户任务不够直接

用户需要理解 runtime、Provider、Route、Instance、Capability、Ledger 等概念，才能完成简单连接。AiMaMi 更擅长把技术能力包装成“切换账号”“查看额度”“管理 MCP”等任务。

**影响**：学习成本高，功能看起来复杂且不成熟。

**改进**：连接工作台使用步骤向导；证据默认折叠；高级字段按需展开。

### 4.2 桌面能力闭环不足

缺少或未完成：

- 发布级 updater。
- 单实例。
- 托盘状态与安全动作。
- 全局快捷键策略。
- 安装后自动启动/恢复策略。
- 完整 macOS/Windows 签名证据。

**影响**：即使核心能力强，用户仍感觉像开发工具而非成品。

### 4.3 Tauri main.rs 过度集中

**影响**：多个开发任务冲突，安全审查困难，平台差异与业务逻辑混合。

**改进**：commands/services/agents/platform/dto 分层。

### 4.4 Agent 支持“广而未闭环”

PRD 覆盖多个 Agent，但真正达到 OpenCode adapter 同等事务质量的实现尚未形成矩阵。

**影响**：多 Agent 宣称与真实可用性存在差距。

**改进**：先定义 Adapter SPI，OpenCode 作为 golden adapter；其他 Agent 未通过同一 Gate 前标记 untested/unavailable。

### 4.5 状态类型仍不够强

`Record<string, any>` 使前端可能在运行时才发现后端字段变化。

**影响**：页面显示错误、空值冒充零值、旧字段 fallback 长期存在。

**改进**：自动生成 TypeScript DTO，统一 EvidenceEnvelope。

### 4.6 前端质量门禁不完整

本轮发现：

- `npm run typecheck` 脚本不存在。
- Vite build 通过，但不能替代完整 `tsc --noEmit`。
- Vitest 108 项通过，但 stderr 有 Radix ref 警告。

**影响**：测试绿色但仍可能存在类型和可访问性问题。

### 4.7 Bundle 体积和首屏性能

生产构建中主 chunk、Recharts 相关 chunk 和 InstallWorkbench chunk 较大。

**影响**：低性能机器首次打开页面可能延迟，动画可能掩盖 loading。

**改进**：继续 route/feature split，按预算在 CI 报警。

### 4.8 第三方 schema 耦合

cc-switch discovery 读取内部 SQLite schema。

**影响**：上游升级后功能突然失效或误解析。

**改进**：版本化 adapter、schema capability detection、只读 fail closed。

### 4.9 发布证据不足

实现存在不等于可发布。当前仍缺：

- 同候选 commit 的 app/DMG/NSIS。
- artifact hash。
- 签名/notarization。
- 安装后 runtime identity。
- Keychain/Windows Credential Manager 实机。
- 真实 OpenCode routed request。
- route decision 与 Ledger 证据。
- 回退和覆盖升级。

### 4.10 Optical 产品风险

代码路径越来越完整，但真实 Provider fidelity/cost/latency 未闭环。

**风险**：研发进度被误解为生产可用，或者过早宣传节省。

**要求**：active 默认关闭；UI 显示缺失 Gate；shadow 与实际节省严格区分。

## 5. AiMaMi 相比 d-token 的缺陷

### 5.1 中心模块维护风险

Repository、Process、Proxy 大文件会提高变更耦合。d-token 不应复制这种结构。

### 5.2 权限与攻击面

桌面系统能力越完整，Tauri capability、CSP、Shell 参数和 updater 越需要严格审计。

### 5.3 配置事务深度

若配置修改没有 d-token 式 hash-bound review 和精确 rollback，在多配置、多账号场景下风险较高。

### 5.4 单生态绑定

深度绑定 Codex 带来优秀体验，但也承受 Codex 配置和内部结构变化风险，且难以直接迁移到其他 Agent。

### 5.5 状态来源可信度

账号额度、代理和本地缓存应明确来源和 freshness；否则“显示得很完整”不等于“证据可靠”。

## 6. d-token 应达到的目标形态

```text
AiMaMi 级桌面产品体验
+
d-token 级事务安全与路由证据
+
多 Agent Adapter SPI
+
跨平台签名发布闭环
+
Optical 的证据门控创新
```

它不应变成 AiMaMi 的多 Agent 复制版，而应成为“安全连接和优化多个 AI coding Agent 的本地控制面”。

## 7. 缺陷优先级

### P0

1. 拆 Tauri main。
2. Agent Adapter SPI + OpenCode 迁移。
3. macOS/Windows 同 commit 发布证据。
4. 真实 routed Agent 验收。
5. release CSP/capability 审计。
6. Optical active 保持关闭。

### P1

1. Rust → TypeScript 强类型。
2. 独立 typecheck 和统一 check。
3. 零 warning 前端测试。
4. ConfigPatcher/Optical Engine 拆分。
5. JSONL 锁、轮转、SQLite projection。
6. chunk budget。
7. cc-switch versioned adapter。

### P2

1. updater、single-instance、tray、shortcut。
2. 会话/MCP/Skills 的只读兼容视图。
3. 历史 UI 与旧代码清理。
4. 长期运行和性能评估。

## 8. 判断是否真正超过 AiMaMi

不是功能列表更长，而是同时满足：

- 新用户可以更快完成首个 verified connection。
- 所有写入可审查并可精确回退。
- 多 Agent 支持有真实测试，不是图标矩阵。
- 路由和节省有证据。
- macOS/Windows 都可签名安装。
- 桌面体验达到稳定工具标准。
- 核心模块可以独立演进。
- 外部项目借鉴合法且不复制品牌。