# d-token 后端性能系统性研究报告

> 日期：2026-07-15
> 对象：Rust workspace、Tauri Native control plane、本地 Axum runtime、JSONL/SQLite 持久化、智能学习任务与上游转发链路
> 方法：生产代码静态审查、关键热路径对账与现有测试合同分析；未将单元测试耗时冒充真实吞吐/尾延迟
> 文档性质：性能改造研究输入，不替代 PRD、安全边界与发布门禁

---

## 0. 结论

后端已经具备较强的正确性与安全基础：loopback 限制、请求体上限、上游 timeout、流式转发、凭据边界、fail-closed 路由、CCR 预发送准备、JSONL 尾部修复、SQLite WAL 与 Tauri `spawn_blocking` 均已实现。

当前性能风险主要来自“正确但昂贵”的同步路径，而不是算法库本身：

1. **Dashboard summary 实际仍执行完整聚合**：先克隆并扫描最多 10,000 条请求日志、生成完整统计/历史/指标，再删除或压缩字段。桌面 8 秒 summary 刷新会持续触发这条 O(n) 路径。
2. **一次请求终态可能进行多次强同步持久化**：event、route decision、request log、CCR metadata 分别打开文件、序列化、flush、`sync_data`；之后还同步写 SQLite projection。磁盘尾延迟直接进入请求完成路径。
3. **智能学习在全局任务锁内扫描项目**：最终候选评分在 `update_job` 持有 `LEARNING_JOBS` Mutex 时调用 `index_project`，单项目最多扫描 5,000 条目；status、cancel 与 event snapshot 都可能长时间等待。
4. **JSONL recent 与启动恢复是全文件扫描**：读取“最近 N 条”仍从头解析整文件；启动又对五类数据各读取最多 10,000 条，日志增长后启动成本和内存常驻量线性增加。
5. **Tauri snapshot 层重复全量序列化与短连接**：Native 使用 blocking `TcpStream + Connection: close` 读取完整响应；revision 又将多个大数组 `to_string()` 后哈希。300 ms cache 不能解决 8 秒周期的重复工作，并存在过期并发重复采集。

综合判断：

| 维度 | 当前成熟度 | 结论 |
|---|---:|---|
| 路由/上游异步 I/O | 75/100 | `reqwest::Client` 复用、timeout、streaming 基础正确 |
| 请求终态持久化 | 32/100 | 多文件同步与 SQLite projection 进入关键路径 |
| Dashboard/控制面 | 35/100 | summary 仍是完整聚合，revision 重复序列化 |
| 内存与数据保留 | 35/100 | 多个无界 Vec，启动恢复 10,000×多类数据 |
| 智能学习任务模型 | 38/100 | 有缓存/取消/event，但锁粒度与写放大严重 |
| 可观测与基准 | 28/100 | 有业务平均延迟指标，缺 queue/fsync/command/p95 分段 |
| 安全与性能平衡 | 72/100 | 安全边界明确，但优化必须保持 durable/fail-closed |

**总体成熟度：45/100。** 当前适合先做测量、summary 热路径、智能学习锁粒度和持久化事务模型四项根因治理，而不是先做微观 Rust 优化。

---

## 1. 架构与热路径

### 1.1 运行路径

```text
Agent HTTP request
  → Axum loopback guard + body limit
  → route / preprocess / compression / adapter
  → reqwest shared client → provider
  → streaming or JSON response
  → terminal evidence
      JSONL event
      JSONL route decision
      JSONL request log
      JSONL CCR metadata
      SQLite ContextStore projections
  → in-memory ledger/vectors
```

### 1.2 桌面刷新路径

```text
React scheduler
  → Tauri invoke get_desktop_summary
  → spawn_blocking
  → service_health_payload_cached
  → blocking TcpStream /dashboard/summary
  → core-api dashboard_summary_for_state
      clone recent data
      clone/scan up to 10,000 request logs
      compute stats/history/metrics/cache
      construct full payload
      remove/compact fields
  → Tauri complete summary
  → finalize_desktop_snapshot
      serialize selected payload to JSON string
      stable hash revision
  → IPC JSON clone to WebView
```

完整 detail 还会同步读取 route drafts、rollback points，并并行打开四个本地 HTTP 短连接。

### 1.3 智能学习路径

```text
Tauri start_learning_discovery
  → std::thread::spawn
  → sequential adapter scans + checkpoint persistence
  → update global job snapshot + full SQLite save
  → emit full snapshot to WebView
  → final candidate aggregation
  → while holding global jobs mutex:
       index_project(candidate path) × candidates
       each may scan up to 5,000 entries
  → persist full snapshot + emit full snapshot
```

---

## 2. 已有正确设计，应保留

1. `AppState` 复用单个 `reqwest::Client`，上游连接可复用。
2. Provider 请求使用配置 timeout，非流式与流式路径均处理网络/超时分类。
3. 流式响应使用 `bytes_stream()` 转发，没有先缓冲完整成功响应。
4. Router 设置 `DefaultBodyLimit::max(MAX_REQUEST_BODY_BYTES)`。
5. Tauri 复杂同步采集通过 `spawn_blocking` 隔离 WebView invoke async executor。
6. JSONL 对相同路径共享锁，append 前可修复损坏尾部，并在写入后 `sync_data`。
7. ContextStore 使用 SQLite WAL、`synchronous=NORMAL` 与 busy timeout。
8. CCR 在需要优化发送时于上游调用前准备，失败时不允许无恢复证据地继续优化发送。
9. Dashboard 与 Tauri 均验证 d-token-owned runtime provenance，不以第三方或离线数据伪装实时状态。
10. Learning 对扫描深度、条目数量、敏感路径、候选与 session metadata 数量设有上限。

性能改造必须在这些约束上演进，不能通过移除 durable evidence、放宽 loopback、缓存假健康或吞掉失败来换取低延迟。

---

## 3. 多维度发现与优先级

### BE-P0-01：`/dashboard/summary` 仍执行完整 10,000 日志聚合

**证据**

`dashboard_payload_for_state`：

- 读取最近 20 条 route decisions。
- 读取最近 20 条 request logs。
- 再克隆 `recent_request_logs(10_000)`。
- 对全部日志分别计算 stats、history、metrics、cache summary。

`dashboard_summary_for_state` 随后调用上述完整函数，再删除 events/memory，并逐项压缩 20 条日志和决策字段。

**复杂度与影响**

- 每 8 秒桌面轮询会做 O(n) clone + 多轮 O(n) 聚合。
- 10,000 条日志可能被多个聚合函数重复遍历。
- handler 是 async function，但内部是同步 Mutex/CPU 工作，会占用 Tokio worker。
- summary 字段虽小，计算成本并不小。

**根因**

summary 是“完整 payload 的后处理视图”，不是独立读模型。

**建议**

建立 `RuntimeAggregateState`：

```text
request_count
success/error count
input/output/saved totals
latency count/sum/buckets
per route/provider/agent counters
recent 20 compact logs
recent 20 compact decisions
time buckets for history
revision per slice
```

每次 terminal commit 后增量更新；summary 只 clone 小型聚合快照。若需要重建，可从持久化 projection 在启动时一次计算并校验 revision，而不是每 8 秒重算。

**验收**

- 0、1k、10k、100k 历史下 summary 时间近似常数。
- online summary p95 ≤250 ms，目标内部 core summary p95 ≤25 ms。
- Tokio worker 无 >100 ms 同步聚合任务。
- summary 响应体不含完整日志、events、memory。

---

### BE-P0-02：请求终态存在四次 JSONL `sync_data` + SQLite 同步写

**证据**

`record_routed_request_inner` 按顺序：

1. `append_event`
2. `append_route_decision`
3. `append_request_log`
4. `append_ccr_metadata`
5. 更新多个内存 Vec/Ledger
6. 锁 `ContextStore`，执行 request projection 与 CCR metadata upsert

每个 `JsonlStore::append` 都会：序列化、获取路径锁、创建目录、打开文件、检查 metadata、必要时扫描修复尾部、seek、write、flush、`sync_data`。

**影响**

- 一个请求完成可能触发四次文件打开与四次强制数据同步。
- fsync/`sync_data` 的 p99 受磁盘、杀毒软件、虚拟化和系统电源状态影响显著。
- 多请求并发时不同文件锁虽分开，但磁盘 flush 仍形成 I/O 队列。
- SQLite projection 又在同一请求终态路径中同步执行。

**安全约束**

不能简单改成“后台异步写后直接返回”，因为项目要求 durable evidence authoritative；持久化失败不能暴露成功终态。CCR 原始恢复证据的预发送边界也不能后移。

**建议：单次 durable terminal commit**

定义一个规范化 `TerminalEvidenceCommit`：

```text
request_id
route_decision
request_log
terminal_event
ccr_metadata
schema_version
commit_revision
```

优先方案：

1. 将整份 commit 作为单条记录写入一个 append-only evidence journal。
2. 单次 serialize、write、flush、`sync_data`。
3. durable 成功后更新内存 ring 与增量 aggregates。
4. JSONL 兼容视图、SQLite projections 和导出可由单 writer actor/恢复过程派生。
5. projection 失败必须暴露为 partial/degraded，并可从 canonical journal 重放修复；不能伪装完整成功。

若短期不能重构存储格式，至少将四项写入同一 SQLite transaction/WAL，并做一次 durability 边界，或复用长期打开的 writer、合并 flush；必须先通过故障注入证明断电/进程终止语义。

**验收**

- 每请求 terminal durable sync 次数从 4+ 降到 1 个规范化 commit 边界。
- terminal overhead p95/p99 有明确数据。
- 在每个写入阶段注入失败时，不产生“内存成功但持久化缺失”。
- 重启可从 journal 恢复 projection，revision 一致。

---

### BE-P0-03：智能学习在全局 Mutex 内执行项目文件扫描

**证据**

`learning_discovery.rs` 的 `update_job`：

- 获取全局 `LEARNING_JOBS: Mutex<HashMap<...>>`。
- 在锁内调用传入 closure。
- closure 完成后 clone 完整 snapshot。

最终 scoring closure 遍历所有 candidates，并在锁内调用 `dtoken_learning::index_project`。该函数受 `MAX_ENTRIES=5_000`、`MAX_DEPTH=4` 约束，但仍是文件系统重任务。

`status`、`cancel`、`snapshot_for`、`is_cancelled` 都需要同一 Mutex。

**影响**

- 取消可能直到当前或全部候选扫描结束才生效，违反 ≤250 ms 取消反馈目标。
- Tauri status invoke 与 event emit 获取不到 snapshot。
- 多 job 状态互相阻塞。
- 前端看到进度停止，误以为应用卡死。

**建议**

1. 锁内只读取/写入小状态；取出 candidates 后立即释放锁。
2. 对每个 candidate 在锁外执行 index。
3. 每个项目完成后用短锁提交 delta，并检查 cancellation token。
4. 将 `bool cancelled` 改为 `Arc<AtomicBool>` 或 CancellationToken，取消无需等待 jobs map 锁。
5. 用受限 worker pool / Semaphore 控制并发；禁止每次 start 无界 `std::thread::spawn`。
6. job map 存 `Arc<JobState>`，每任务独立锁；全局 map 只管理注册表。

**验收**

- 5,000-entry 项目扫描中 cancel acknowledgement p95 ≤250 ms。
- status 查询 p95 ≤50 ms。
- 两个 job 不因其中一个文件系统慢而互相阻塞。
- 线程数有上限，App 退出可收敛。

---

### BE-P0-04：缺少分段 p95/p99 与队列观测

**证据**

当前已有 request log `latency_ms`、压缩 `elapsed_us`、平均 latency metrics 和部分业务聚合，但未发现统一的：

- Native command duration
- dashboard build duration
- JSONL serialize/open/write/flush/sync duration
- ContextStore lock wait/write duration
- learning queue/scan/index/persist/emit duration
- IPC payload bytes
- request in-flight/queue depth
- p50/p95/p99 histogram

**影响**

- 平均延迟掩盖磁盘 p99 与锁竞争。
- 无法判断优化是否把成本从请求路径转移到后台积压。
- 无法为不同平台建立真实预算。

**建议**

以 `tracing` span 或轻量内部 histogram 记录阶段：

```text
request.route
request.compress
request.ccr_prepare
request.provider_connect / first_byte / complete
request.evidence_commit
request.projection
summary.aggregate / serialize
tauri.local_http / snapshot_finalize / ipc_payload
learning.enumerate / index / persist / emit
```

字段只保留 ID、类别、计数、字节桶与时长；不得输出 prompt/response/credential/path 内容。

---

### BE-P1-01：内存 Vec 无界增长，启动恢复固定加载多组 10,000 条

**证据**

`AppState` 包含：

- `EventLedger.events: Vec<DtokenEvent>`
- `route_decisions: Mutex<Vec<Value>>`
- `request_logs: Mutex<Vec<RequestLog>>`
- `ccr_metadata: Mutex<Vec<Value>>`
- `memory_records: Mutex<Vec<Value>>`

运行时只 push，未发现统一 retention。持久化启动时对上述五类数据分别读取最多 10,000 条并放入内存。

**影响**

- 长期运行内存随请求数增长。
- 每次 recent 需要锁并 clone。
- 大 Vec 扩容和 clone 进入 dashboard/导出路径。
- 启动会读取并解析多份历史，低端磁盘明显放大。

**建议**

- recent 数据改为容量固定的 `VecDeque`/ring buffer，例如 UI recent 200–1,000，具体由 fixture 决定。
- 长期历史由 SQLite projection/cursor 查询，不常驻全部对象。
- EventLedger 也设置明确 retention；审计历史以持久化 canonical journal 为准。
- 启动只加载 aggregates、revision 和小型 recent window。

---

### BE-P1-02：`JsonlStore::recent` 是 O(file size) 全文件扫描

**证据**

`recent(limit)` 从文件开头 `read_until('\n')`，逐条 `serde_json::from_slice`，只用 VecDeque 保留最后 N 条。即使 limit=20，也解析全文件。

**影响**

- 文件越大，启动、导出、恢复和任何 recent 查询越慢。
- 五个文件依次全扫，放大 I/O 与 JSON parse。
- 文件没有 rotation/segment/index 合同。

**建议**

中期以 canonical journal + SQLite projection 替代频繁 JSONL recent。若保留 JSONL：

1. 分段 rotation，例如按大小/日期。
2. 维护安全的 offset index/checkpoint。
3. 从尾部读取完整行或从最后若干 segment 扫描。
4. compaction 必须原子、可恢复、保留审计链 revision。
5. `known_good_len` 之外增加 segment metadata，避免每次不必要检查。

---

### BE-P1-03：Tauri local HTTP 使用短连接并完整读取响应

**证据**

`simple_http_get` 每次：

- 新建 blocking `TcpStream`
- `Connection: close`
- 将完整 HTTP response 读入 String
- 再从 header/body 分割并解析 JSON

完整 detail 一次用 `thread::scope` 创建四个短连接，summary 也至少 health + dashboard。

**影响**

loopback TCP 很快，但频繁 connect、线程、字符串复制和完整响应读取仍是固定成本；大 payload 时额外分配明显。手写 HTTP 还无法自然使用 keep-alive、压缩、body size guard 和结构化 timing。

**建议**

- Tauri Native 使用复用的 HTTP client，限定 `127.0.0.1`，配置 connect/read/total timeout 与最大响应体。
- 使用 keep-alive；summary 与 detail 可并发但必须有 keyed coalescing。
- 逐层记录 connect/TTFB/read/parse bytes。
- 若 Tauri 与 runtime 同进程架构允许，长期可通过 typed in-process service handle 读取 summary，减少 JSON 往返；但不能绕过 provenance/evidence 合同。

---

### BE-P1-04：snapshot revision 对大数组重复 JSON 序列化

**证据**

`finalize_desktop_snapshot` 构造包含 providers、routes、instances、request_logs、route_decisions、changes、drafts、backups 等字段的 `revision_input`，执行 `revision_input.to_string()` 再 `stable_hash`。

**影响**

- 为了判断“是否变化”先完整复制/序列化待判断数据。
- detail 越大，revision 计算越贵。
- 前端 fallback 仍可能在 revision 缺失时 `JSON.stringify(desktop)`，形成双端重复。

**建议**

每个数据源在写入/变更时维护 revision：

```text
runtime_summary_revision
request_log_revision
route_revision
transaction_revision
learning_revision
optical_revision
```

聚合 snapshot revision 只哈希固定长度 revision tuple，而不哈希内容。collection timestamp 与 revision 分离：采集时间变化不代表内容变化。

---

### BE-P1-05：300 ms cache 存在过期并发重复采集

**证据**

summary/detail cache 读取时加锁，但在 cache miss/expired 后释放锁再执行采集。多个并发调用可同时发现过期并重复运行本地 HTTP、文件读取和序列化。

**建议**

使用 single-flight cache：

- cache state：`Ready(value, revision, expires)` / `Collecting(waiters)`。
- 过期时只有一个 collector，其他调用等待或拿 stale-while-revalidate。
- manual force refresh 也进入同一 keyed collector。
- TTL 不作为真实性替代；health/provenance failure必须更新状态。

---

### BE-P1-06：同步 CPU/Mutex 工作运行在 Axum async handler 中

**证据**

`dashboard_payload` 和 `dashboard_summary` 是 async handler，但直接执行同步 clone、Mutex lock、聚合与 JSON construction。其它 stats/history/metrics endpoint 也可读取 10,000 logs 后同步计算。

**建议**

根因方案是增量 read model。过渡期可将昂贵重建放入 `spawn_blocking`，但不能把每 8 秒 O(n) 任务简单搬到 blocking pool 后视为解决；还需限制并发与 single-flight。

---

### BE-P1-07：智能学习快照持久化写放大

**证据**

每次 `update_job`：

1. clone 完整 snapshot。
2. aggregate candidates。
3. `open_learning_cache()`，初始化/检查 SQLite。
4. `save_job()` 开启 transaction。
5. upsert scan job。
6. 删除该 job 全部 session evidence 和 project candidates。
7. 重新插入所有 sources/candidates。
8. prune 到 16 jobs。
9. commit。

adapter 每完成一次、阶段变化和终态均可能触发完整重写；另外 session records 在循环中也可能全量 replace。

**影响**

- candidates/session records 越多，每次进度持久化越慢。
- event emit 频率与 SQLite 写放大绑定。
- cache open、权限检查和 migration path 被反复进入。

**建议**

- job 生命周期复用一个受控 cache handle/connection actor。
- progress counters 与 phase 做轻量 upsert。
- source/candidate 使用 delta upsert，不做 delete-all/reinsert-all。
- session metadata 分批 transaction。
- terminal 时生成完整一致性 snapshot；中间 checkpoint 使用节流，例如时间或记录数阈值。
- event emit 与 persistence revision 解耦，但 terminal 需等待持久 checkpoint。

---

### BE-P1-08：后台任务线程模型缺少全局上限

**证据**

每次 learning start 使用 `std::thread::spawn`。旧 job 被标记 cancelled，但线程不 join；如果扫描点未及时检查取消，短时间内可存在多个文件扫描线程。

**建议**

统一 `BackgroundTaskManager`：

- bounded worker pool / Tokio blocking semaphore。
- job priority：interactive、foreground、background、maintenance。
- per-job cancellation token。
- App shutdown drain/abort policy。
- CPU、I/O concurrency 分开限制。
- 任务状态通过 delta event 暴露。

---

### BE-P2-01：核心文件体积使性能边界难以维护

当前文件规模约为：

- Tauri `main.rs`：10,373 行，约 390 kB。
- core API `lib.rs`：9,888 行，约 375 kB。
- learning discovery：3,549 行，约 130 kB。

这不是直接运行时瓶颈，但 command、持久化、HTTP、进程、配置与聚合耦合在大文件中，容易让 blocking 工作误入 async/锁内路径。建议按性能所有权拆分：

```text
desktop_snapshot.rs
local_http_client.rs
runtime_aggregates.rs
evidence_commit.rs
retention.rs
learning/task_manager.rs
learning/persistence.rs
perf_observability.rs
```

拆分必须伴随测试和公开接口收敛，不能仅移动代码。

---

## 4. 目标后端架构

### 4.1 Canonical evidence commit

```text
request pipeline
  → CCR prepare before optimized send when required
  → provider response/stream terminal
  → build TerminalEvidenceCommit
  → single durable journal/transaction commit
  → commit acknowledged
  → update bounded recent rings
  → update incremental aggregates
  → enqueue projection/export maintenance
```

关键原则：

- 只有 canonical commit durable 后才形成可公开成功终态。
- projection 可恢复、可重放、可标记 partial，但不能成为静默失败。
- streaming terminal guard 必须在流结束/中断时提交正确 terminal 状态。
- fsync 合并不能跨越安全所需的请求边界，除非 PRD 明确允许批量 durability window。

### 4.2 Runtime read model

```text
RuntimeAggregateState (small, bounded, revisioned)
  ├─ summary counters
  ├─ latency histogram
  ├─ time buckets
  ├─ recent compact logs
  ├─ route/provider summaries
  └─ slice revisions

SQLite/Projection Store
  ├─ cursor logs
  ├─ events
  ├─ route decisions
  ├─ CCR metadata
  ├─ memory metadata
  └─ searchable history
```

Summary 只读 aggregate；详情从 projection cursor 读取；导出可流式读取，避免把全部记录复制进内存。

### 4.3 Native snapshot service

```text
SnapshotService
  keyed single-flight
  stale-while-revalidate where permitted
  typed slice revisions
  shared loopback client / in-process handle
  payload byte caps
  duration spans
```

Tauri 不再通过完整 JSON 内容计算 revision，也不为同一 slice 建多套缓存。

### 4.4 Learning task service

```text
Task registry (short global lock)
  → Arc<JobState>
      atomic cancellation
      short snapshot lock
      bounded progress channel

Worker pool
  enumerate → checkpoint delta
  index project outside lock → candidate delta
  terminal persist → terminal event
```

---

## 5. 分阶段实施计划

### Gate B0：建立真实性能基线

必须新增 benchmark/trace：

1. Dashboard summary：0/1k/10k/100k logs。
2. Terminal evidence：临时磁盘与真实用户盘，p50/p95/p99、fsync 次数。
3. Runtime 启动：10k/100k JSONL，解析时间、RSS、首次 health。
4. Tauri summary/detail：online/offline、payload bytes、local HTTP 分段。
5. Learning：1/100/1,000 candidates、5,000-entry project、cancel latency、SQLite bytes written。
6. 并发请求：1/8/32 in-flight，吞吐、queue、p99、内存。
7. Streaming：first byte、complete、client disconnect、terminal evidence delay。

### Gate B1：先修三个根因

1. 独立实现增量 dashboard summary，禁止从 full payload 后处理。
2. 将 learning project index 移出全局 lock，引入原子取消与 bounded worker。
3. 为 evidence persistence 增加阶段 timing/fsync 计数，并设计 single durable commit 原型。

### Gate B2：有界内存与查询模型

1. recent Vec 改有界 ring。
2. 历史查询迁移到 SQLite/cursor。
3. 启动只加载 aggregates + recent window。
4. JSONL 增加 rotation/index，或由 canonical journal 取代多文件 recent。

### Gate B3：Native snapshot 与 IPC

1. shared loopback client 或 typed in-process read handle。
2. single-flight cache。
3. slice revisions，不对大 payload `to_string`。
4. summary/detail/query commands 统一 payload limit 和 duration span。
5. 向前端提供 cursor 与 delta event。

### Gate B4：持久化事务收敛

1. `TerminalEvidenceCommit` schema + migration。
2. 单次 durable commit。
3. projection replay 与故障恢复。
4. streaming/非 streaming/失败/timeout/cancel 全路径 fault injection。
5. 证明 durability 与审计能力不低于当前实现后，才删除旧多文件同步路径。

### Gate B5：发布与 soak

1. 30 分钟、2 小时、24 小时 soak。
2. 长日志增长、rotation、restart、crash recovery。
3. Windows Defender/普通 HDD 场景与 macOS APFS 场景。
4. p95/p99、RSS、disk bytes、queue depth 与 CPU 门禁。
5. 同一候选 commit 绑定 release artifact。

---

## 6. 基准与验收目标

| 场景 | 指标 | 目标 |
|---|---|---:|
| core dashboard summary 10k/100k history | p95 | ≤25 ms，且随历史规模近似常数 |
| Tauri online summary | p95 | ≤250 ms |
| Tauri offline fail-fast | p95 | ≤500 ms |
| terminal evidence overhead | p95 | 基线后设门槛，优化后至少下降 50% |
| terminal durable sync | 每请求 | 1 个 canonical commit 边界 |
| status/cancel during learning index | p95 | status ≤50 ms；cancel acknowledgement ≤250 ms |
| restart with long history | 首次 health | ≤1 s 目标；不得随全部历史线性增长 |
| bounded recent memory | 30 分钟/24 小时 | 达稳定区，漂移 <5% |
| 32 并发 provider requests | queue/timeout | 无无界队列，无 runtime starvation |
| stream client disconnect | terminal evidence | 必须记录 cancelled/failed，且不泄漏 task |
| projection failure | evidence state | 显式 partial/degraded，可重放恢复 |

“下降 50%”必须以 Gate B0 的同硬件、同 fixture、同 release profile 基线为分母；不能跨机器比较。

---

## 7. 建议任务矩阵

| 任务 | 优先级 | 收益 | 风险 | 安全门禁 |
|---|---:|---:|---:|---|
| Performance spans/histograms | P0 | 极高 | 低 | 禁止敏感字段 |
| Incremental dashboard summary | P0 | 极高 | 中 | source/freshness/revision 保留 |
| Learning lock/cancellation 重构 | P0 | 极高 | 中 | allowed roots/limits 不变 |
| TerminalEvidenceCommit 原型 | P0 | 极高 | 高 | durable/fail-closed fault injection |
| Bounded recent rings | P1 | 高 | 中 | 历史仍可持久查询 |
| Cursor projection API | P1 | 高 | 中 | redaction 与权限边界 |
| Tauri shared local client | P1 | 中高 | 中 | 固定 loopback + body cap |
| Slice revision/single-flight | P1 | 高 | 中 | 不缓存假健康 |
| Learning delta persistence/events | P1 | 高 | 中 | terminal checkpoint durable |
| JSONL rotation/index | P2 | 中高 | 高 | crash-safe migration |
| 大文件按所有权拆分 | P2 | 中 | 中 | 行为测试先行 |

---

## 8. 风险与反模式

以下做法不应采用：

1. **直接删除 `sync_data`**：会改变 durable evidence 合同。
2. **把所有写入丢进无限 channel**：会把延迟变成内存/丢失风险。
3. **仅给 summary 加更长 TTL**：会隐藏根因并削弱 freshness。
4. **只用 `spawn_blocking` 包住 O(n) 聚合**：只是转移阻塞池，未降低总成本。
5. **为速度跳过 provenance/health 验证**：会产生假在线状态。
6. **无界并行扫描项目**：会造成磁盘争用和线程膨胀。
7. **每个请求创建新 HTTP client**：会丢失连接池收益；当前 shared reqwest 设计应保留。
8. **用平均延迟代替 p95/p99**：无法发现磁盘与锁尾延迟。
9. **把 raw prompt/response 加入性能 trace**：违反隐私与安全边界。
10. **一次性替换全部存储格式**：应先双写/重放验证，再迁移读取路径。

---

## 9. 报告自审

### 准确性

- 已确认上游成功流式路径使用 `bytes_stream`，未错误声称所有 provider 响应均全量缓冲。
- 已区分 Tauri 手写 loopback HTTP 与 core-api 的 shared `reqwest::Client`，没有把两者混为一谈。
- 已指出 ContextStore 使用 WAL/NORMAL，不将 SQLite 本身笼统定性为瓶颈；问题在锁内同步投影与频率。
- 未用测试耗时推断真实 p95/p99。

### 完整性

覆盖请求热路径、流式、持久化、Dashboard、Tauri IPC、缓存、revision、内存、启动、Learning、线程模型、可观测、故障恢复和跨平台磁盘差异。

### 可执行性

P0 项均有明确代码根因、目标架构、故障门禁和 benchmark；没有提出脱离安全合同的“全部异步化”。

### 安全性

`TerminalEvidenceCommit` 明确要求 durable 后才公开成功终态；CCR 预发送恢复边界不后移；projection 失败必须显式 degraded 并可重放。

**结论：本报告可作为后端性能重构输入。优先实施 Gate B0/B1，不应在没有 fault injection 和真实磁盘基线前直接删除现有 durable 写入。**
