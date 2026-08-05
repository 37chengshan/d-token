# d-token 前后端性能研究报告多维度审查

> 日期：2026-07-15
> 审查对象：
> - `FRONTEND_UI_PERFORMANCE_AUDIT_2026-07-15.md`
> - `BACKEND_PERFORMANCE_AUDIT_2026-07-15.md`
>
> 审查目标：校验事实、优先级、前后端依赖、可实施性、安全性、测试闭环与跨平台一致性；本文件是报告质量审查，不宣称代码性能已经达标。

---

## 0. 最终审查结论

两份报告的主结论成立，且能够共同解释当前性能风险：前端的广泛重渲染、详情派生和多调度器只是表层放大器；后端 summary O(n) 聚合、多次 durable sync、全量 revision 序列化与智能学习锁竞争是更深层成本。任何单边优化都不足以完成 PRD 性能门禁。

审查后的实施顺序：

```text
Wave 0 统一测量与候选基线
  ↓
Wave 1 后端三个根因
  summary 独立读模型
  learning 锁外扫描 + 原子取消
  terminal persistence 分段 timing / canonical commit 设计
  ↓
Wave 2 前端切片订阅 + 单一调度器
  同步引入 Native slice revision / single-flight
  ↓
Wave 3 详情 cursor + 虚拟列表 + learning delta events
  ↓
Wave 4 有界内存、持久化收敛、启动恢复与 soak
  ↓
Wave 5 包体/CSS/动画的证据驱动收敛
```

**报告质量结论：批准作为实施规划输入。** 仍需补充真实 Tauri release、Windows/macOS、磁盘与压力基准后，才能把建议阈值转化为发布通过证据。

---

## 1. 审查维度与结果

| 维度 | 前端报告 | 后端报告 | 审查结论 |
|---|---:|---:|---|
| 源码事实准确性 | 91/100 | 94/100 | 核心证据与当前工作树一致 |
| 问题根因深度 | 90/100 | 95/100 | 后端报告定位到同步持久化、读模型与锁粒度根因 |
| 优先级合理性 | 88/100 | 93/100 | 需坚持先测量和后端热路径，再做包体微调 |
| 前后端契约一致性 | 92/100 | 92/100 | summary/detail/revision/cursor/delta 方向一致 |
| 安全与真实性 | 94/100 | 97/100 | 明确禁止牺牲 durable、provenance、freshness 与 fail-closed |
| 可实施性 | 89/100 | 88/100 | Gate/Wave 清晰；canonical commit 属高风险架构改造 |
| 可验证性 | 95/100 | 95/100 | 均定义了 fixture、p95/p99、soak 与故障注入 |
| 跨平台完整性 | 87/100 | 90/100 | 已纳入 Windows/macOS；仍缺真实双端数据 |
| 文档边界 | 96/100 | 96/100 | 未替代 PRD/PLAN，区分审计与发布证据 |

综合评分：

- 前端报告：**91/100**
- 后端报告：**93/100**
- 交叉一致性：**94/100**

---

## 2. 源码事实复核

### 2.1 已确认的前端事实

1. 一级页面通过 `React.lazy` 拆分，charts vendor 保持懒加载。
2. `PageStage` 当前只有进入动画，没有旧式 `mode="wait"` 串行退出等待。
3. `RuntimeEvidenceProvider` 已实现 8 秒 summary fallback、in-flight 合并、隐藏暂停、可见恢复与失败退避。
4. detail 由 consumer 激活，不是所有页面启动时拉取。
5. 运行证据仍通过较大的 Context value 广播，selector 订阅尚未建立。
6. Connection 60 秒、Optical 20 秒、Learning 1.2 秒 fallback 与 Runtime 8 秒调度并存。
7. Smart Learning 同时使用 Tauri event 与完整 status fallback，并在页面卸载时取消运行任务。
8. Logs/Ledger 展示会先构建和排序数据，再截取有限行；缺少 cursor/virtualization 统一基元。
9. 生产构建命令成功：2,815 modules，2.90 秒；构建产物数字已在前端报告记录。
10. 源码未发现统一 PerformanceObserver、Long Task 或 React Profiler 基线。

### 2.2 已确认的后端事实

1. `/dashboard/summary` 由完整 dashboard 构建后再删除/压缩字段，期间克隆并聚合最多 10,000 条 request logs。
2. terminal request evidence 顺序写 event、route decision、request log、CCR metadata；每个 JSONL append 都 flush + `sync_data`。
3. durable 成功后还会同步更新内存和 SQLite ContextStore projections。
4. `JsonlStore::recent(limit)` 从文件头解析全部完整行，只保留最后 N 条。
5. 启动会从多类持久数据各加载最多 10,000 条到无界 Vec/Ledger。
6. Tauri local service 调用使用 blocking `TcpStream`、`Connection: close` 和完整 String 读取。
7. desktop revision 对选定大字段 `to_string()` 后哈希。
8. 300 ms summary/detail cache 在过期后无 single-flight，可能并发重复采集。
9. core-api 上游路径复用 `reqwest::Client`，有 provider timeout，成功 streaming 走 `bytes_stream`。
10. Learning 的最终 `index_project` 在 `update_job` closure 内执行，而 closure 持有全局 jobs Mutex；cancel/status 依赖同一锁。
11. Learning 每次 update clone 并持久化完整 snapshot，SQLite 对 sources/candidates 执行 delete-all + reinsert-all。
12. 每次 learning start 使用 `std::thread::spawn`，缺少统一 bounded worker 管理。

以上三项证据最强、应作为首批实现任务：

- Dashboard summary 的 O(n) 完整聚合。
- Learning 全局锁内项目扫描。
- Terminal evidence 多次强同步。

---

## 3. 需要谨慎解释的内容

### 3.1 前端 172.90 kB gzip 是构建图推断，不是最终首屏网络证据

报告已标注该限制。最终应以 Vite manifest、WebView resource trace 和 parse/evaluate 时间为准。不能仅因推断超 160 kB 就立即移除 Motion/Radix。

审查决定：**保留 P1，不升级为 P0。** 包体优化必须排在测量、summary、锁竞争和状态切片之后。

### 3.2 当前 core runtime 的 dashboard payload 只带 recent window，但详情模型仍是跨 slice 整包

core runtime 当前将 request logs/events/route decisions限制在 recent window；因此“前端正在渲染 10,000 条 runtime logs”不是当前事实。报告提出 10,000/20,000 fixture 是 PRD 压力目标和未来查询扩展门禁。

审查决定：

- 保留 cursor/virtualization 建议。
- 实施时先测当前 bounded payload，再设计历史查询 API。
- 不为尚未存在的 10,000 行 DOM 做过早复杂化；超过约 200 可见候选/记录后再启用虚拟化阈值。

### 3.3 `TerminalEvidenceCommit` 是目标设计，不是可直接套用的补丁

它改变持久化主模型，涉及：

- schema 与迁移
- stream terminal 语义
- crash recovery
- projection replay
- export 与历史兼容
- CCR metadata 与原始恢复边界

审查决定：**保持 P0 研究/原型优先级，但生产切换必须晚于 fault injection 与双写/重放验证。** 在此之前先增加 timing，确认各 sync 的真实占比。

### 3.4 SQLite 不是被笼统判定为慢

ContextStore 已使用 WAL、NORMAL 和 busy timeout。问题是当前请求终态中持有单个 ContextStore Mutex 并同步执行 projection，以及 Learning 的完整快照重写。优化方向是事务边界、增量写和所有权，不是简单替换数据库。

### 3.5 `spawn_blocking` 只能是过渡隔离

Tauri 已正确使用 `spawn_blocking` 包裹同步 snapshot，但 core dashboard async handler仍做同步聚合。把 O(n) 工作移进 blocking pool可避免 Tokio worker 直接被占，但不能减少总 CPU/clone；最终仍需增量 read model。

---

## 4. 前后端依赖关系复核

| 前端目标 | 必需后端能力 | 单边实施风险 |
|---|---|---|
| selector store | slice revision、稳定 summary schema | 后端每次重建对象时，前端 selector 仍频繁变化 |
| 单一 EvidenceScheduler | keyed Native commands、single-flight、event invalidation | 只合并前端 timer，Native 过期 cache仍可能重复采集 |
| Logs cursor/virtualization | cursor API、稳定 ID、revision/next cursor | 前端仍接整包后切片，IPC/序列化成本不降 |
| Learning delta UI | progress/candidate/terminal delta events | 后端继续 emit 完整 snapshot，前端仅做局部 patch 仍需 clone 大包 |
| 轻量 Overview | O(1) aggregate summary | 前端组件更轻，但后端每 8 秒仍扫描 10k logs |
| 快速 stale/fresh 状态 | collected_at 与 content revision 分离 | 时间戳变化导致无意义重渲染 |
| 隐藏窗口零轮询 | Native task interest/priority | 后端后台线程仍可能继续高强度工作 |

反向依赖：

| 后端目标 | 必需前端配合 | 单边实施风险 |
|---|---|---|
| summary 字段瘦身 | 页面停止依赖 detail 字段 | 前端功能回归或被迫再次扩充 summary |
| bounded recent ring | UI 使用 cursor 查历史 | 用户误认为历史丢失 |
| delta events | 前端按 revision 合并与缺口重拉 | 丢事件后 UI 永久不一致 |
| stale-while-revalidate | UI 明确显示 stale/freshness | 缓存数据被误呈现为实时 |
| projection degraded | UI 暴露 partial/error | 性能改造后失败被隐藏 |

**结论：必须以“数据合同 PR”组织实现，不应把前端和后端拆成互不沟通的两条优化线。**

---

## 5. 优先级交叉校准

### P0：必须先解决

1. 建立同一套前端/Native/core correlation ID 与阶段 duration。
2. 新建独立 dashboard summary read model或至少消除 full payload 后处理。
3. 将 Learning `index_project` 移到全局锁外，取消改原子/Token。
4. 为 JSONL/SQLite terminal persistence 增加阶段 p95/p99 与 fsync 计数。
5. 建立 fixed fixture 和同候选 commit 的双平台 release 基线。

### P1：紧接 P0

1. Runtime selector store 与 slice revision。
2. 前端单一 scheduler + Native keyed single-flight。
3. Learning progress/delta/terminal 事件与增量 persistence。
4. bounded recent rings + cursor projection API。
5. Tauri shared loopback client、response body cap与分段 timing。
6. desktop revision 改为 revision tuple。

### P2：在真实数据证明后实施

1. VirtualEvidenceList 的全面铺开。
2. Charts 可见性挂载与 Worker 聚合。
3. Shell Motion/Radix 静态路径收敛。
4. CSS selector 清理。
5. core/Tauri 大文件按性能所有权拆分。
6. JSONL rotation/index 或完整 canonical journal 迁移。

### 不建议现在做

- 重写全部 React 页面。
- 更换 UI 框架。
- 更换 SQLite。
- 删除所有动画。
- 直接删除 `sync_data`。
- 用长 TTL 掩盖 dashboard 成本。
- 无界并行项目扫描。

---

## 6. 统一实施路线

### Wave 0：两端共同测量

交付：

- `perf_run_id` / `correlation_id`
- WebView marks + Long Task
- Native command span
- core request/dashboard/persistence/learning span
- build manifest 与 fixture 版本
- macOS/Windows release baseline 文档

退出条件：任一慢场景能回答“时间花在 WebView、IPC、local HTTP、aggregate、磁盘还是 provider”。

### Wave 1：后端最大收益根因

交付：

- summary 不再从 full payload 后处理。
- learning global lock只保护短状态变更。
- cancellation 不等待 jobs map Mutex。
- terminal persistence阶段数据与 fault injection harness。

退出条件：10k history summary 近似常数；learning cancel p95 ≤250 ms；terminal overhead p95/p99 可见。

### Wave 2：状态与刷新合同

交付：

- summary schema v2。
- slice revision。
- Native single-flight。
- FrontendEvidenceScheduler。
- selector store。

退出条件：单 slice变化只触发目标 React consumers；空闲/隐藏 IPC符合预算；同 key无重复采集。

### Wave 3：详情扩展性

交付：

- logs/events/routes/backups cursor API。
- Learning delta event与缺口恢复。
- VirtualEvidenceList。
- Worker/Native 聚合边界。

退出条件：10k logs/20k metadata fixture 无 >100 ms Long Task，DOM保持有界。

### Wave 4：存储与生命周期

交付：

- bounded rings。
- startup aggregate/recent window。
- canonical commit 原型、双写/重放与 crash tests。
- projection degraded/recovery。
- background task manager。

退出条件：长历史不使启动线性退化；24 小时 RSS进入稳定区；故障恢复无成功证据丢失。

### Wave 5：启动资产与维护性

交付：

- 确认首屏 import graph。
- 页面 intent prefetch。
- Motion/Radix/CSS 证据驱动收敛。
- 大文件按所有权拆分。

退出条件：初始 gzip/parse/interactive 满足预算，且无无障碍、视觉或双平台回归。

---

## 7. 统一验收矩阵

| 场景 | 前端指标 | Native/core 指标 | 共同目标 |
|---|---|---|---|
| 冷启动 | shell interactive | service first health / snapshot | p95 ≤1.5 s |
| 暖启动 | shell interactive | cached config/read model ready | p95 ≤800 ms |
| Overview 刷新 | React commits / feedback | summary aggregate + local HTTP | online p95 ≤250 ms |
| runtime 离线 | stale/error feedback | connect timeout/fail-fast | p95 ≤500 ms |
| Logs 10k | Long Task/DOM/filter | cursor query/payload bytes | UI反馈 ≤100 ms |
| Learning stress | progress/cancel feedback | lock wait/index/persist | cancel ≤250 ms |
| Provider request | UI status | route/compress/provider/evidence p95/p99 | 无持久化尾延迟失控 |
| Streaming disconnect | UI terminal state | task cleanup/evidence terminal | 无泄漏、状态真实 |
| 30 min/24 h soak | WebView heap | RSS/thread/fd/disk queue | 稳定区漂移 <5% |
| 双平台 | 组件/布局一致 | APFS/Windows Defender 差异 | 同合同、同预算或有证据例外 |

---

## 8. 安全与产品合同审查

### 通过项

- 两份报告都保留 d-token-owned runtime provenance。
- 前端不使用假数据替代离线/partial。
- 后端不建议以异步丢队列替代 durable evidence。
- CCR pre-send recovery 边界不后移。
- cursor/delta 建议包含 revision 与缺口恢复。
- stale-while-revalidate 仅在 UI 明确 freshness 时允许。
- 双平台不允许前端结构分叉。

### 实施时必须新增的安全测试

1. canonical commit 每个阶段 crash/failure injection。
2. projection lag/replay 与重复 commit idempotency。
3. delta event 丢失、乱序、重复与 revision gap。
4. cursor 越界、过期和数据被 rotation 后的行为。
5. shared local HTTP client仍只能连接 `127.0.0.1`，有响应体上限。
6. performance trace 不含 prompt、response、credential、auth file和原始路径内容。
7. Learning 锁外扫描仍保持 allowed roots、敏感目录跳过与 entry/depth caps。

---

## 9. 报告缺口与后续证据

本轮报告没有完成、也不应伪装完成以下证据：

- Tauri release 冷/暖启动真实 profile。
- Windows 机器性能与 Windows Defender 磁盘尾延迟。
- macOS APFS 实际 `sync_data` p99。
- 10k/100k dashboard microbenchmark。
- 10k logs/20k metadata WebView压力结果。
- 30 分钟/24 小时 soak。
- 真实 provider streaming并发和断连资源审计。
- Vite manifest对应的精确首屏 waterfall。

这些不是报告错误，而是 Gate 0 必须产生的下一阶段证据。任何实施 PR 应将基线与优化后结果绑定到同一候选 commit/fixture/platform。

---

## 10. 交付验证结果

本轮只新增/更新研究文档与文档索引，没有修改产品功能代码。验证结果：

| 验证 | 结果 |
|---|---|
| `npm run build` | 通过；2,815 modules，Vite 构建 2.90 秒 |
| `npm run check:structure` | 通过；canonical roots、单一 PRD、archive 边界和活动 Markdown 链接有效 |
| `npm run test:run` | 通过；36 个测试文件、191 个测试全部成功 |
| `cargo test -p dtoken-core-api -p dtoken-event-ledger -p dtoken-learning` | 通过；相关 unit/integration/doc tests 共 96 个成功 |
| `cargo test -p d-token-desktop` | 通过；105 个 Tauri/Native 测试成功 |

独立 TypeScript `tsc --noEmit` 未执行：当前 `package.json` 没有 `typecheck` script，CodexPro 安全命令白名单禁止临时 `npm exec tsc`。`vite build` 成功不能替代完整 TypeScript 类型检查，因此此项明确保留为未验证，而不是视为通过。

---

## 11. 最终批准意见

### 前端报告

批准。重点应放在性能观测、selector store、统一 scheduler、轻量 summary合同和详情扩展性；包体、CSS和动画保持 P1/P2，避免过早优化。

### 后端报告

批准。summary O(n)、Learning 全局锁与 terminal multi-sync 是最高价值问题；canonical commit必须以故障注入和迁移计划推进，不能直接删除当前 durable 路径。

### 联合执行

批准按 Wave 0–5 推进。建议第一轮实现任务严格限制为：

```text
1. 前后端统一 performance spans/marks
2. dashboard summary 独立构建 benchmark + 实现
3. learning index 移出全局锁 + cancellation benchmark
4. terminal persistence阶段 timing/fault harness
5. Runtime slice schema/revision 设计文档
```

完成以上五项后再决定状态库、虚拟列表、Motion/Radix、JSONL迁移的具体技术方案。

**最终状态：两份报告经多维度复核后可用，优先级与安全约束一致；尚不构成性能发布通过证明。**
