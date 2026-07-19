# d-token 前端 UI 与性能系统性研究报告

> 日期：2026-07-15
> 对象：`code/apps/desktop` 当前工作树
> 范围：React 生产入口、Shell、页面懒加载、运行证据状态、IPC 调度、长列表、图表、动画、后台任务、CSS/资源与测试门禁
> 方法：源码静态审查 + `npm run build` 生产构建基线；未以浏览器开发模式成绩替代 Tauri release 实测
> 文档性质：优化实施输入，不替代 `docs/PRD.md`、`docs/PLAN.md` 与当前实现状态

---

## 0. 结论

当前前端已经完成了第一阶段正确性治理：一级页面按路由懒加载，页面切换不再等待串行退出动画，全局运行证据刷新具备请求合并、失败退避、窗口隐藏暂停与按需 detail 读取，启动图标也已从旧基线的 281 kB 降到构建产物 16.24 kB。

但前端仍未进入“性能可证明”的阶段。核心瓶颈不是单个组件写法，而是四个系统性问题：

1. **缺少真实性能观测闭环**：代码中没有 `performance.mark/measure`、Long Task 观测、React Profiler 基线或 Tauri 窗口场景数据，因此无法判断用户感知卡顿来自 React、IPC、Native 文件读取还是本地 HTTP。
2. **状态订阅粒度过粗**：运行证据仍通过大 Context 和大对象快照广播；任一状态、时间或 slice 变化都可能让多个无关消费者重新渲染与重新派生数据。
3. **详情数据仍是全量模型**：日志、事件、route、backup、changes 等数组通过 `get_desktop_data` 整包进入前端；页面再进行全量 map/filter/sort，最后才截取可见结果，没有分页、增量合并和虚拟列表。
4. **刷新调度仍然分散**：全局 runtime 8 秒、连接发现 60 秒、视觉压缩 20 秒、智能学习 1.2 秒兜底轮询并存；虽然均有局部保护，但还不是 PRD 要求的单一 event-first 调度器。

综合判断：

| 维度 | 当前成熟度 | 结论 |
|---|---:|---|
| UI 正确性与产品一致性 | 78/100 | Shell、页面 IA、状态语义和 reduced-motion 基础较完整 |
| 感知响应性 | 66/100 | 切页阻塞已消除，但首次加载、长任务与后台更新尚无 p95 证据 |
| React 渲染效率 | 48/100 | 大 Context、大 payload、页面级 O(n) 派生仍是主要风险 |
| 数据与 IPC 效率 | 45/100 | summary/detail 已拆，但 detail 仍全量，调度器仍多套 |
| 启动与包体积 | 62/100 | route chunk 良好，Shell 静态依赖集合预计仍超 160 kB gzip 暂定预算 |
| 长列表与压力可扩展性 | 30/100 | 无统一分页/虚拟化；10,000 条 fixture 尚未证明 |
| 性能可观测与回归门禁 | 20/100 | 尚无前端性能标记、真实窗口 profile 与 CI 预算比较 |

**总体成熟度：52/100。** 可以开始系统优化，但必须先补测量，不能仅凭“看起来更流畅”验收。

---

## 1. 审计边界与事实基线

### 1.1 生产链路

```text
src/main.tsx
  → styles/index.css
  → app/App.tsx
    → AppProviders
    → RuntimeEvidenceProvider
    → ConnectionSetupProvider
    → NotificationProvider
    → RuntimeNotificationBridge
    → AppShell
      → Sidebar / Topbar / StatusBar
      → PageStage + Suspense
      → 7 个一级页面 lazy chunk
```

关键代码：

- `src/app/App.tsx`
- `src/app/components/shell/AppShell.tsx`
- `src/app/components/shell/PageStage.tsx`
- `src/app/lib/useRuntimeDashboard.ts`
- `src/app/lib/connectionSetup.tsx`
- `src/app/components/pages/SmartLearning.tsx`
- `src/app/components/pages/VisualCompression.tsx`
- `vite.config.ts`

### 1.2 2026-07-15 生产构建基线

执行：

```text
cd code/apps/desktop
npm run build
```

结果：2,815 modules transformed，Vite 构建 2.90 秒，命令成功。

| 产物 | Raw | Gzip | 评价 |
|---|---:|---:|---|
| `index` 主业务 chunk | 117.88 kB | 39.35 kB | 相比旧单块基线显著改善 |
| React vendor | 166.60 kB | 54.83 kB | Shell 必需 |
| Motion vendor | 129.12 kB | 42.45 kB | Shell 当前静态使用，启动成本较高 |
| Radix vendor | 114.29 kB | 29.38 kB | Shell/基础控件静态依赖 |
| Icons vendor | 28.16 kB | 6.19 kB | 可接受，仍需防止无界图标导入 |
| Tauri vendor | 1.48 kB | 0.70 kB | 很小 |
| CSS | 158.00 kB | 25.54 kB | Raw 较旧基线继续增长，需做未使用规则审计 |
| Charts vendor | 374.86 kB | 103.83 kB | 仍保持 route-level lazy，不能进入 Shell |
| Overview | 33.54 kB | 10.99 kB | 良好 |
| InstallWorkbench | 44.31 kB | 14.52 kB | 良好 |
| Analytics | 19.67 kB | 6.46 kB | 页面代码小，但会拉取 charts vendor |
| Advanced | 48.48 kB | 15.10 kB | 页面体积尚可，源码维护复杂度高 |
| VisualCompression | 52.88 kB | 13.95 kB | 页面 chunk 可接受，运行时任务较多 |
| 启动图标 | 16.24 kB | — | 已消除旧 281 kB 大资产风险 |

按 Shell 静态 import 对应的已发射 chunk 粗略相加，`index + React + Motion + Radix + Icons + Tauri` 预计约 **172.90 kB gzip**。这高于 `docs/PLAN.md` 暂定的初始 JS gzip 160 kB 预算。该数字是构建图推断，不是最终网络 waterfall；必须用 Vite manifest 或 Tauri WebView 实际加载记录确认。默认 Overview 首屏还会追加约 10.99 kB gzip。

### 1.3 已经做对的事情

1. `AppShell` 对一级页面使用 `React.lazy`，图表 vendor 未进入 Shell。
2. 页面切换已不再使用 `AnimatePresence mode="wait"`，避免固定退出等待。
3. `RuntimeEvidenceProvider` 对全局刷新做了 in-flight 合并。
4. 全局刷新在窗口隐藏时停止，恢复可见后只触发一次合并刷新。
5. 失败轮询使用指数退避，避免本地服务离线时持续高频重试。
6. summary 与 detail 已拆分；完整日志/历史仅在有 detail consumer 时读取。
7. 智能学习已经有 Tauri event 通知和取消语义，不是纯轮询。
8. reduced-motion 在 Shell 与多个页面存在基本处理。

这些能力应保留，后续优化不能回退到 mount-local 状态、假数据、静默失败或无来源状态。

---

## 2. 多维度发现与优先级

### FE-P0-01：性能没有可定位证据

**证据**

- `code/apps/desktop/src` 未发现 `performance.mark`、`performance.measure`、`PerformanceObserver`、Long Task 或 React `Profiler`。
- 当前构建只提供 chunk 大小，不能说明 Tauri 冷启动、WebView 主线程、IPC、CPU、内存或用户点击后的 p95。

**影响**

- 优化后无法证明改善。
- 任何“卡顿”都可能被错误归因给动画或 React，实际根因可能是 Native HTTP、文件扫描或持久化。
- CI 无法拦截回归。

**建议**

建立统一的前端性能事件：

```text
app_boot_start / shell_interactive
page_navigation_start / page_content_stable
runtime_refresh_start / runtime_refresh_commit
first_user_feedback
learning_start / learning_first_result / learning_stopped
visual_snapshot_refresh
dialog_open / large_list_filter
```

每个事件至少记录：`name`、`duration_ms`、`page`、`result_state`、`payload_count_bucket`、`revision_changed`、`native_command`。禁止记录路径正文、提示词、模型响应、密钥或原始日志内容。

**验收**

- release WebView 能导出 p50/p95。
- React Profiler 可显示 runtime revision 变化后的 commit 数。
- 能将一次慢刷新拆成前端等待、IPC、Native 子阶段与 React commit。

---

### FE-P0-02：大 Context 广播导致无关重渲染

**证据**

`useRuntimeDashboard.ts` 中 `RuntimeEvidenceContext` 的单个 value 同时包含：

- `payload`
- `status`
- `operational`
- `updatedAt`
- `error`
- `refresh/invalidate`

`StatusBar`、Overview、Install、Settings、RuntimeNotificationBridge 等均订阅同一 Context。即使只有 service 状态或 `updatedAt` 变化，所有消费者仍可能重新渲染。`operational` 又包含 dashboard、traffic、routes、transactions、doctor 多个 slice。

**影响**

- 8 秒刷新或 invalidation 会放大为多组件 commit。
- 页面派生函数即使使用 `useMemo`，只要依赖整个 payload object，revision 变化时仍会重算。
- 大数组引用变化时，React memo 边界失效。

**根因**

状态模型以“请求返回对象”为中心，而不是以“产品 slice + revision”为中心。

**建议**

实现 `RuntimeSnapshotStore`，使用 `useSyncExternalStore` 或带 selector 的轻量 store：

```text
serviceSummary     revision_service
trafficSummary     revision_traffic_summary
routeSummary       revision_routes
transactionSummary revision_transactions
doctorSummary      revision_doctor
```

页面通过 selector 只订阅所需字段；数组详情不进入全局 summary store。selector 比较使用稳定 primitive/revision，不对完整 payload 做深比较或 `JSON.stringify`。

**验收**

- service 心跳变化时，Overview 指标区和无关页面不 commit。
- StatusBar 展开状态变化不触发当前业务页重算。
- React Profiler fixture 中，单 slice 更新只提交目标消费者。

---

### FE-P0-03：详情页仍接收整包数据并在前端全量派生

**证据**

- `useRuntimeDashboardDetail()` 调用 `get_desktop_data`，返回日志、events、route decisions、backups、changes 等详情。
- `LogsSecurity.tsx` 将多个数组转换为展示行，对结果全量排序后才 `.slice(0, 80)`。
- 同一 payload 同时生成 runtime logs、ledger rows、summary count 和 security rows。
- `shown = activeLogs.filter(...)` 在每次渲染执行；数据增长后与 DOM/派生工作线性相关。

**影响**

- 即使只显示 80 行，也会先克隆、格式化、拼接字符串和排序所有记录。
- detail revision 一变，多页面可能重新派生全部历史。
- 10,000 条日志和 20,000 条 session metadata 的 PRD 压力目标无法成立。

**建议**

将详情 API 改为显式查询：

```text
get_runtime_logs({ cursor, limit, filter, sort })
get_runtime_events({ cursor, limit, kind })
get_route_history({ cursor, limit })
get_backups({ cursor, limit })
```

Native 返回：`items`、`next_cursor`、`total_estimate`、`revision`、`source`、`freshness`。前端以稳定 ID 增量合并，只有可见窗口进入 DOM。搜索和排序优先下推 Native/SQLite；纯前端过滤使用 `useDeferredValue` 或 Worker，并设置取消/过期结果丢弃。

**验收**

- 10,000 条 fixture 下 DOM 行数近似可见行数，而非数据总量。
- filter p95 ≤ 100 ms，滚动无 >100 ms Long Task。
- 新日志到达时只增量插入，不重建全部展示对象。

---

### FE-P1-01：存在四套独立刷新调度器

**证据**

| 调度器 | 周期 | 位置 |
|---|---:|---|
| Runtime summary | 8 s，失败退避 | `useRuntimeDashboard.ts` |
| Connection discovery | 60 s | `connectionSetup.tsx` |
| Visual Compression | 20 s + window focus | `VisualCompression.tsx` |
| Smart Learning fallback | 1.2 s，且同时监听 Tauri event | `SmartLearning.tsx` |

**影响**

- 同一 invalidation 可能触发多套 Native 工作。
- 各页面自行决定 visible/focus/重试，难以统一限流和观测。
- 用户点击刷新、Tauri event 和周期 timer 可能在短时间内重复运行。

**建议**

建立唯一 `FrontendEvidenceScheduler`：

1. Tauri event 首先按 slice invalidation。
2. 所有手动刷新与周期刷新进入同一 keyed in-flight map。
3. 页面注册 interest，而不是自建固定 timer。
4. 页面不可见时取消低优先级刷新；窗口恢复后合并一次。
5. refresh reason 明确为 `boot | event | manual | visibility | fallback`。
6. Smart Learning event 正常时不做 1.2 秒轮询；仅在连续若干秒无事件后做低频状态核验。

**验收**

- 任一 slice 同时最多一个 Native 调用。
- 30 分钟空闲 IPC 次数可预测且可解释。
- 失去焦点、隐藏、恢复、手动刷新均无重复请求风暴。

---

### FE-P1-02：Shell 启动依赖预计超过当前 gzip 预算

**证据**

Shell 静态依赖预计约 172.90 kB gzip，高于 160 kB 暂定预算；主要增量来自 Motion 42.45 kB 和 Radix 29.38 kB。

**判断**

这不等于应立即移除 Motion 或 Radix。它说明必须确定这些依赖在冷启动关键路径上是否值得，以及是否可以把页面级能力从 Shell 静态路径拆出。

**建议顺序**

1. 生成并保存 Vite manifest，确认实际静态 import 图。
2. 记录 Tauri release 的 JS parse/evaluate 时间，而非只看 gzip。
3. 检查 Shell 中是否只为简单旋转、淡入和 tooltip 拉入整包能力。
4. 优先把页面专用 Motion/复杂 Radix primitive 留在 lazy page；Shell 简单过渡可使用 CSS。
5. 不要为了数字强行自研所有无障碍 primitive；若替换 Radix，必须先证明 parse/CPU 是真实瓶颈。
6. 对导航 hover/focus 做 route intent prefetch，减少首次进入页面的等待，但禁止启动时预取全部页面。

**验收**

- 初始加载图由 manifest/trace 证明。
- 冷启动 Shell 可交互 p95 ≤ 1.5 秒，暖启动 ≤ 800 ms。
- 初始 JS gzip回到 ≤160 kB，或以真实 parse/interactive 证据批准预算例外。

---

### FE-P1-03：长列表没有统一虚拟化和分页策略

**证据**

Logs、Changes、Doctor、Smart Learning candidates、route/history 等组件均使用普通数组 map；部分页面只截取前若干行，但仍先构建全量数据。未发现统一 virtual list 基元。

**建议**

创建共享 `VirtualEvidenceList`：

- 固定/可测行高优先。
- overscan 有上限。
- 以稳定 ID 作为 key。
- 空、loading、partial、stale、error 状态沿用产品 evidence 语义。
- 支持键盘焦点恢复和屏幕阅读器总数提示。
- 小数据量阈值以下可直接 map，避免无意义复杂化。

建议阈值先设 200 行，使用 fixture 再调整。

---

### FE-P1-04：智能学习使用完整快照事件，且离页即取消

**证据**

- `dtoken:learning-discovery` 每次 event payload 是完整 snapshot，含所有 sources 与 candidates。
- 前端收到后直接 `setSnapshot(next)`。
- 运行期间还每 1.2 秒请求完整 status。
- 组件卸载时，如任务仍运行则调用 `cancel_learning_discovery`。

**影响**

- 候选增长时 IPC 序列化、WebView clone 和 React state 替换持续放大。
- 用户切页会取消本可低优先级继续的轻量扫描，导致重复启动与重复磁盘工作。
- 进度 event 和 fallback poll 可能重复提交同一 snapshot。

**建议**

事件拆为：

```text
learning:progress { job_id, revision, counters, phase }
learning:candidates-delta { job_id, revision, added, updated_ids }
learning:terminal { job_id, revision, state, summary }
```

页面首次进入或 revision 缺口时再拉取分页 candidates。轻量元数据扫描允许离页后低优先级继续；深度扫描/外部委派由全局任务中心管理并显式显示。用户主动停止与 App 退出才取消。

---

### FE-P1-05：视觉压缩页面重复获取独立状态

**证据**

`VisualCompression.tsx` 挂载时同时执行 `refresh("initial")`、`loadEditor()`、`loadAgentIntegration(true)`，并每 20 秒刷新、窗口 focus 再刷新。其运行证据与全局 runtime/connection 状态存在交叉。

**建议**

- 将 optical status/capabilities/lifecycle 作为独立 slice 注册到统一 scheduler。
- editor draft 只在进入编辑页或 revision 变化时读取。
- agent integration 使用 connection store 的 revision，而不是页面自建长期轮询。
- 多个 Native command 可由 Native 聚合为一个轻量 summary，但详情保持按需。

---

### FE-P2-01：页面源码过大，性能边界难以独立验证

当前典型文件：

- `VisualCompression.tsx` 约 1,553 行。
- `Advanced.tsx` 约 1,300 行。
- `Overview.tsx` 超过 800 行。
- `LogsSecurity.tsx` 超过 700 行。

大文件本身不必然导致运行慢，但会让状态依赖、memo 边界、effect 生命周期和 code splitting 难以审计。应按“数据控制器 / 纯展示 / 重型详情”拆分，而不是为了行数机械拆组件。

推荐边界：

```text
PageController: interest + command + state machine
PageSummary: 首屏轻量展示
PageDetail: lazy / pagination
PageDialogs: 用户触发后加载
selectors.ts: 纯函数，可 fixture benchmark
```

---

### FE-P2-02：动画数量需要以合成与长任务证据约束

Motion 已用于 PageStage、Sidebar、StatusBar、StatCard、Analytics 指标、Heatmap、Logs 等。当前页面切换只剩单次 180 ms 入场，这是合理基础，但列表逐项动画和图表动画在数据增长时会放大布局与合成成本。

规则建议：

1. Shell 页面切换只允许一次容器级 opacity/transform。
2. 实时/长列表禁止逐行 stagger。
3. 图表只在可见容器创建，离页释放 observer/tooltip/animation。
4. `prefers-reduced-motion` 必须关闭非必要 movement，而不只是把时长缩短。
5. 动画不得延迟状态选中、pending 或焦点移动。

---

### FE-P2-03：CSS 需要建立增长预算而不是一次性大清理

CSS 158.00 kB raw / 25.54 kB gzip，较 2026-07-13 raw 基线 152.82 kB 增长约 5.18 kB。当前 `index.css` 汇总 fonts、tailwind、theme、shell、surface。

建议：

- 先生成 selector 使用报告，识别旧入口、重复 token 和不可达 class。
- 每次 PR 比较 raw/gzip 与选择器数量。
- 不做高风险整文件重写；按组件迁移并配视觉回归。
- 保持 Windows/macOS 共用 token 和布局，不引入平台 UI 分叉。

---

## 3. 目标架构

### 3.1 数据流

```text
Tauri events / fallback tick / manual action
                ↓
FrontendEvidenceScheduler
  keyed in-flight + priority + visibility + reason
                ↓
RuntimeSnapshotStore
  service / trafficSummary / routes / transactions / doctor / optical
  each slice: data + revision + source + freshness + state + error
                ↓
selector subscriptions
                ↓
page summary
                ↓ user opens detail
cursor API + incremental cache + virtual list
```

### 3.2 状态原则

- Summary 不含全量日志和历史。
- 详情数组以独立 cache 保存，不嵌入 Context value。
- 每个 slice 用 revision 判定变化；`collected_at` 变化不能单独导致全树更新。
- stale/partial/error 保留上一份可验证数据，但必须显示 freshness 与错误。
- manual refresh、event refresh 和 fallback refresh 共用一个请求。
- 页面卸载只释放订阅，不随意取消全局低优先级任务。

### 3.3 UI 响应原则

- 点击 100 ms 内出现 pending/选中/骨架。
- 页面内容不可因退出动画延迟替换。
- 首次 lazy load 显示与页面结构匹配的 skeleton，而非仅居中文本。
- 长任务在状态栏或全局任务中心持续可见。
- 不通过假数据、假曲线或永久绿色状态制造“流畅”。

---

## 4. 分阶段实施计划

### Gate F0：测量与固定基线

1. 增加前端 Performance marks、Long Task observer 与可禁用的 React Profiler harness。
2. 保存构建 manifest、chunk raw/gzip、CSS 与资产清单。
3. 建立 small/medium/stress fixture。
4. 在 macOS 与 Windows 同一候选 commit、release build 下记录：冷/暖启动、连续切页、runtime 刷新、日志过滤、Smart Learning 首结果、Visual Compression 刷新。
5. 输出 CPU、内存、IPC 次数、React commit 和 Long Task 基线。

**没有 Gate F0 数据，不批准大规模依赖替换。**

### Gate F1：消除高频无效工作

1. 建立统一 scheduler，先接管 runtime、connection、optical。
2. Smart Learning 正常 event 流下关闭 1.2 秒轮询；超时后才 fallback。
3. 所有刷新统一 in-flight、reason、visibility 与错误退避。
4. 删除长列表逐项动画。
5. 确保每个 CTA 100 ms 内进入 pending。

### Gate F2：切片订阅与轻量 summary

1. 引入 selector store / `useSyncExternalStore`。
2. Context 只保留稳定 store 引用，不广播大 snapshot。
3. Native summary 明确字段合同，移除全量数组。
4. 每个 slice 使用 revision 和结构共享。
5. 页面只订阅自己使用的 slice。

### Gate F3：详情分页、虚拟化与增量事件

1. Logs/events/routes/backups/changes 使用 cursor API。
2. 建立共享 virtual list。
3. 搜索、排序、聚合下推 Native 或 Worker。
4. Learning 使用 progress/delta/terminal 事件。
5. 图表按可见性创建并释放。

### Gate F4：启动路径与资源收敛

1. 用 manifest 和 Tauri trace确认 Shell 静态依赖。
2. 将页面专用 Motion/Radix 能力留在 lazy chunk。
3. 导航 hover/focus intent prefetch。
4. 清理不可达 CSS 和重复 token。
5. 预算例外必须记录真实 parse/interactive 证据。

### Gate F5：CI 与发布门禁

1. 初始 gzip、页面 chunk、CSS、资产大小回归检查。
2. React commit/Long Task fixture 回归。
3. Playwright/Tauri 关键场景 p95。
4. 10,000 日志、20,000 session metadata 压力测试。
5. 30 分钟 soak：内存漂移 <5%，隐藏窗口无非必要轮询。

---

## 5. 建议任务拆分

| 任务 | 优先级 | 预期收益 | 风险 | 前置证据 |
|---|---:|---:|---:|---|
| Performance marks + command correlation | P0 | 极高 | 低 | 无 |
| 单一 EvidenceScheduler | P0 | 高 | 中 | IPC 基线 |
| Runtime selector store | P0 | 极高 | 中 | React commit 基线 |
| Dashboard summary 字段瘦身 | P0 | 极高 | 中 | 后端合同同步 |
| Logs/events cursor API | P1 | 极高 | 中 | fixture |
| VirtualEvidenceList | P1 | 高 | 中 | 行高/可访问性设计 |
| Learning delta events | P1 | 高 | 中 | 后端任务模型调整 |
| Optical/connection 合并调度 | P1 | 中高 | 中 | command duration |
| Shell Motion 静态路径评估 | P1 | 中 | 中高 | Tauri parse trace |
| CSS selector 清理 | P2 | 中 | 中 | 视觉回归 |
| 大页面按性能边界拆分 | P2 | 中 | 中 | Profiler 热点 |

---

## 6. 性能验收矩阵

| 场景 | 数据规模 | 指标 | 目标 |
|---|---:|---|---:|
| 冷启动 | 默认数据 | Shell 可交互 p95 | ≤1.5 s |
| 暖启动 | 默认数据 | Shell 可交互 p95 | ≤800 ms |
| 已加载页面切换 | 默认数据 | 内容稳定 p95 | ≤250 ms |
| 任意 CTA | 默认数据 | 首次视觉反馈 | ≤100 ms |
| Runtime summary 在线 | small/medium | 刷新 p95 | ≤250 ms |
| Runtime 离线 | — | fail-fast p95 | ≤500 ms |
| Logs 滚动 | 10,000 | Long Task | 无 >100 ms |
| Logs 过滤 | 10,000 | 结果反馈 p95 | ≤100 ms |
| Learning 首结果 | 20,000 metadata | 首批候选 | ≤1.5 s |
| Learning 取消 | stress | UI 进入 stopping/stopped | ≤250 ms |
| 空闲可见窗口 | 默认数据 | 平均 CPU | ≤1% |
| 隐藏窗口 | 无后台任务 | 非必要 IPC/timer | 0 |
| 30 分钟 soak | 页面往返 | 内存稳定区漂移 | <5% |

所有数据必须绑定：平台、硬件、OS、release/debug、候选 commit、fixture 版本与重复次数。浏览器 dev server 结果只能用于定位，不能作为发布结论。

---

## 7. 风险与不可退让约束

1. 性能优化不得删除 source、freshness、partial/stale/error 状态。
2. 不得将全量 detail 重新塞回 summary 以换取开发便利。
3. 不得通过长期缓存隐藏服务停止、配置损坏或写入失败。
4. 不得为了减包自研退化的无障碍组件。
5. Windows/macOS 必须共享同一 React 组件、token、动画语义与性能预算；平台差异只留在系统适配层。
6. 不得用假 fixture 数据生成生产 UI 成功状态。
7. 任何 Worker、observer、timer、Tauri listener 和对象 URL 都必须在卸载/取消时释放。

---

## 8. 报告自审

### 准确性

- 已区分“实际生产构建数字”和“根据静态 chunk 图推断的启动集合”。
- 未声称已完成 Tauri release CPU、内存、冷启动或 Windows 实测。
- 已识别当前实现中已经完成的 summary/detail 拆分、hidden pause、backoff 与 in-flight 合并，未重复使用过期结论。

### 完整性

覆盖启动、包体积、React 状态、IPC、刷新、列表、图表、动画、后台任务、资源生命周期、可观测、跨平台与 CI。

### 可执行性

所有 P0/P1 发现均给出目标结构、验收指标和实施顺序；没有把“重写前端”作为泛化答案。

### 风险控制

明确禁止牺牲证据真实性、错误可见性、无障碍和双端 1:1；依赖替换必须由真实 trace 驱动。

**结论：本报告可作为前端性能实施输入，但在 Gate F0 真实窗口基线完成前，不可作为“性能已达标”的证据。**
