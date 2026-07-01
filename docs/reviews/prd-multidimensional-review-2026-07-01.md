# PRD 多维审查报告：d-token v0.1 -> v0.2

日期：2026-07-01  
审查对象：`docs/prd/d-token-prd-v0.1.md`  
审查结论：v0.1 方向正确，但必须升级为 v0.2。核心修正是：**Headroom 全功能保留，Headroom Dashboard 全量迁移**。

## 1. 总体结论

v0.1 PRD 的优点：

- 抓住了“本地压缩 + provider 路由 + 桌面 App + 多实例”的主线。
- 已经意识到 Windows/macOS 兼容、第三方 provider、cc-switch、Codex 双实例的重要性。
- 已经把 d-token 与单纯 API relay 区分开。

v0.1 PRD 的不足：

- 对 Headroom 的描述偏“借鉴”，没有把 Headroom 功能当成必须保留的底座。
- Dashboard 写得过轻，只是普通监控页，不是 Headroom Dashboard 的完整迁移。
- 没有功能迁移矩阵，后续开发很容易漏功能。
- 没有 Dashboard 迁移矩阵，后续 UI 很容易只做简化版。

最终建议：

> d-token 必须定位为 Headroom-compatible enhanced desktop product，而不是 Headroom-inspired lightweight proxy。

## 2. 多维度审查

### 2.1 战略定位

评分：8/10 -> v0.2 后可达 9.2/10。

问题：v0.1 已经有清晰方向，但没有写死“保留 Headroom 全功能”。如果不修正，产品会变成普通压缩中转。

修正：v0.2 明确 d-token = Headroom 全功能内核 + Dashboard 桌面化 + d-token 路由创新。

### 2.2 功能完整性

评分：5.5/10 -> v0.2 后可达 9/10。

问题：v0.1 只列了部分能力：proxy、MCP、learn、token savings、压缩策略。缺少 SDK、wrap、CCR、memory、SharedContext、output reduction、durable ledger、framework integrations、dashboard panels 等明确要求。

修正：新增 Headroom feature parity matrix。

### 2.3 Dashboard 完整性

评分：4/10 -> v0.2 后可达 9.5/10。

问题：v0.1 Dashboard 只写“请求日志、压缩率、上游、端口、错误”，未覆盖 Headroom 原 Dashboard 的 session/history、hero metrics、agent usage、per-model、per-project、historical export、live feed、waste signals 等。

修正：新增 Dashboard full checklist 和 migration matrix。

### 2.4 技术架构

评分：7.5/10 -> v0.2 后可达 8.8/10。

问题：v0.1 架构层次正确，但 Headroom-compatible core 与 d-token routing layer 边界不够清晰。

修正：拆成三层：

1. Headroom-compatible core。
2. d-token routing / orchestration layer。
3. d-token desktop layer。

### 2.5 Windows/macOS 可落地性

评分：8.5/10。

优点：v0.1 已经明确 UTF-8、Windows 盘符路径、连字符路径、`.cmd` shim、端口占用检测。

补充：Dashboard 全量迁移后，要把这些兼容能力做成 `doctor` 与桌面端可视化诊断。

### 2.6 Provider / Route 设计

评分：8/10。

优点：v0.1 已经区分官方上游、第三方上游和 chain bridge。

补充：v0.2 要求所有 Dashboard 请求记录都增加 provider、route、instance、chain mode 维度，避免“请求工作正常但 Dashboard 无日志”。

### 2.7 多实例方案

评分：8.5/10。

优点：多个本地实例拆开是正确方向。

补充：Dashboard 必须按 instance 拆分请求、节省、错误、provider、route、状态，不得只看总量。

### 2.8 安全与合规边界

评分：7.8/10。

问题：v0.1 有本地密钥与脱敏要求，但缺少功能迁移时的边界说明。

修正：v0.2 明确 d-token 只做本地开发、压缩、路由、观测、合规配置；不做账号共享、账号托管或违反平台条款的自动化用途。

### 2.9 开源迁移风险

评分：7/10。

问题：用户要求“全部移植”，但工程上要区分：功能全量迁移、信息架构迁移、代码是否复用。

修正：建议优先 clean-room reimplementation；如果复用 Apache 2.0 代码，必须保留 license notice。

## 3. 必须改的 PRD 条款

### 3.1 原条款

“借鉴 Headroom 的产品方向”。

### 3.2 修改为

“d-token 必须保留 Headroom 的所有核心功能，并在此基础上增加 d-token 的桌面化、多 provider、多实例和 chain bridge 能力。”

### 3.3 原条款

“Dashboard 展示请求日志、压缩率、上游、端口、错误。”

### 3.4 修改为

“Dashboard 必须全量迁移 Headroom Dashboard 的信息架构和指标体系，并增加 d-token 的 provider、route、instance、chain mode 维度。”

## 4. 优先级调整

### 原优先级

1. Proxy 内核。
2. 压缩 MVP。
3. Desktop MVP。
4. 多实例与 chain bridge。

### 新优先级

1. Headroom feature parity matrix。
2. Dashboard full migration matrix。
3. Headroom-compatible core skeleton。
4. Dashboard full migration skeleton。
5. Provider / Route / chain bridge layer。
6. Compression parity。
7. Windows/macOS hardening。

## 5. 最终审查结论

PRD v0.1 不是不能用，而是不够硬。

它适合作为“方向草案”，但不适合作为开发执行版本。真正可执行的版本必须是 v0.2：

- Headroom 全功能不删减。
- Dashboard 全量迁移。
- d-token 只在此基础上做增强。
- 所有新增能力必须进入 provider/route/instance/chain mode 可观测链路。

一句话：

> d-token 的正确路线不是重新发明一个 Headroom，而是把 Headroom 的能力完整桌面化、产品化、Windows/macOS 化，再加入你需要的多上游和双实例路由体系。
