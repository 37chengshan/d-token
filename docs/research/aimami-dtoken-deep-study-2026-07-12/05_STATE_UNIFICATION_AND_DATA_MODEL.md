# 状态统一与数据模型

## 1. 为什么状态统一是这类应用的核心

AiMaMi 类桌面应用同时读取：

- 本地配置文件。
- 运行中的进程和端口。
- Provider/账号/额度信息。
- 代理或本地服务状态。
- 用户刚刚执行的操作结果。
- 历史会话、日志和统计。

这些来源具有不同刷新频率、可信度和失败模式。若每个页面自行读取并推断状态，会出现：

- 同一服务在 Overview 显示运行，在 Settings 显示停止。
- 页面卸载后重新挂载，状态回到默认假值。
- 请求失败时旧数据被清空，用户无法判断是“零”还是“不可用”。
- Browser preview 被误当成真实运行状态。
- 用户刚完成 apply，但另一个页面仍显示旧 revision。

所以状态统一不只是前端工程问题，而是产品真实性和安全问题。

## 2. AiMaMi 的状态思路

AiMaMi 的前端使用集中状态和 Hooks 组织账号、配额、代理、配置和页面数据。优势是任务体验连贯，缺点是当后端返回结构变化或多个异步请求竞争时，容易形成隐式状态和页面特定逻辑。

其值得借鉴的点：

- 按业务域拆 store/hook，而不是所有状态放进单一全局对象。
- 把耗时操作状态和业务数据分开。
- 页面懒加载但共享全局关键状态。
- 系统托盘、窗口和页面使用同一运行状态来源。

## 3. d-token 当前状态模型

`useRuntimeDashboard.ts` 已引入应用级 `RuntimeEvidenceProvider`，统一读取：

- `get_desktop_data`。
- `get_dtoken_service_status`。

并拆分为 operational slices：

- service。
- dashboard。
- traffic。
- routes。
- transactions。
- doctor。

状态包括：

```text
loading
runtime
browser-preview
unavailable
stale
partial
```

每个 slice 包含：

- data。
- source。
- state。
- freshness。
- collectedAt。
- error。

这是 d-token 很重要的正确方向，因为它不再让各页面根据 mount-local 默认值推断服务状态。

## 4. 当前问题

### 4.1 动态类型过多

`Record<string, any>` 和 `any[]` 仍大量存在。风险：

- Rust 字段改名后 TypeScript 不报错。
- `undefined` 被当成空数组或零值。
- 页面自行 fallback 到不同旧字段。
- 测试 fixture 无法证明真实 DTO 完整。

### 4.2 状态分类与数据 DTO 耦合

`classifyRuntimePayload` 通过特定字符串判断 runtime identity。建议把分类结果由后端或共享 schema 明确返回，而不是前端重复推断。

### 4.3 错误字符串不稳定

前端仍可能依据字符串展示错误。应使用稳定 code。

### 4.4 freshness 语义不统一

当前 freshness 可能是 `fresh`、`local`、`unknown` 或时间戳。应拆分为：

- observed_at。
- expires_at 或 max_age_ms。
- freshness_state。

### 4.5 operation 与 snapshot 的关系不明确

用户执行 apply 后，需要立即失效相关 snapshot。当前 `invalidate` 等同于 refresh，但建议引入 operation receipt 驱动的精确 invalidation。

## 5. 推荐统一状态信封

所有 Tauri command 和本地 API 面向 UI 的数据建议使用统一 envelope：

```ts
interface EvidenceEnvelope<T> {
  schemaVersion: number;
  state: "verified" | "partial" | "stale" | "unavailable" | "failed";
  source: string;
  observedAt: string;
  maxAgeMs: number | null;
  data: T | null;
  error: EvidenceError | null;
  provenance: EvidenceProvenance;
}
```

```ts
interface EvidenceProvenance {
  runtimeId?: string;
  runtimeVersion?: string;
  routeId?: string;
  configRevision?: string;
  candidateCommit?: string;
  ledgerEventIds?: string[];
}
```

错误模型：

```ts
interface EvidenceError {
  code: string;
  userMessage: string;
  technicalSummary?: string;
  recoverability: "retry" | "review" | "rollback" | "manual" | "none";
  suggestedAction?: string;
}
```

## 6. 领域 Store 设计

不建议把所有状态塞进一个巨大 store。推荐：

```text
RuntimeStore
RouteStore
TransactionStore
DoctorStore
SettingsStore
UpdateStore
```

### RuntimeStore

- 服务生命周期。
- listener identity。
- runtime metadata。
- health。

### RouteStore

- Provider/Route/Instance。
- 当前选择。
- 路由决策。
- upstream discovery。

### TransactionStore

- preview。
- review revision。
- apply receipt。
- verify。
- rollback points。

### DoctorStore

- 检查矩阵。
- 当前问题。
- 修复动作。

### SettingsStore

- 主题、语言、数据保留。
- 桌面集成。
- 实验能力。

## 7. 状态机

### 7.1 服务状态机

```text
unknown
→ stopped
→ starting
→ verifying_identity
→ verifying_listener
→ running_verified
→ stopping
→ stopped
```

异常分支：

```text
identity_mismatch
port_conflict
unmanaged_running
start_failed
stop_refused
```

### 7.2 配置事务状态机

```text
idle
→ planning
→ review_required
→ approved
→ backing_up
→ applying
→ verifying
→ doctor_running
→ applied_verified
```

失败状态：

```text
review_stale
backup_failed
apply_failed_restored
apply_failed_restore_incomplete
verify_failed_restored
rollback_refused_external_change
```

### 7.3 路由状态机

```text
unconfigured
→ candidates_discovered
→ selection_required
→ draft_ready
→ reviewed
→ active_verified
```

异常状态：

```text
ambiguous
provider_mismatch
credential_unavailable
upstream_unreachable
loop_detected
```

## 8. 数据一致性策略

### 8.1 单写入者

- d-token 自有 active config 由 d-token 写。
- 第三方 Agent config 通过 adapter + ConfigPatcher 写。
- UI 绝不直接写文件。

### 8.2 乐观并发控制

- preview 记录 before hash。
- apply 前重新读取并比较。
- 不一致时返回 `review_stale`。
- 用户必须重新 preview。

### 8.3 Receipt 驱动更新

操作成功后，前端先保存 receipt，再刷新相关 slice。不能先乐观改成成功状态。

### 8.4 保留旧数据

刷新失败时：

- 保留上次数据。
- 将状态改为 stale/partial。
- 显示最后更新时间。
- 不把旧值当成当前 verified。

## 9. Rust 与 TypeScript 合同

建议使用以下任一方式生成类型：

- `specta` / `tauri-specta`。
- `ts-rs`。
- JSON Schema + TypeScript generator。
- OpenAPI 仅用于本地 HTTP API。

要求：

- DTO 使用 `deny_unknown_fields` 或明确兼容策略。
- schema_version 必填。
- 枚举在 Rust/TypeScript 一致。
- 测试读取真实序列化样本。
- 禁止页面继续添加多个旧字段 fallback。

## 10. 轮询与事件

当前 2.5 秒轮询 + 指数退避是合理基础。后续可采用混合模式：

- 低频 snapshot 轮询保证最终一致。
- Tauri event 推送 operation progress、service state 和重大事件。
- 窗口隐藏时暂停高频轮询。
- 回到前台立即刷新。

避免完全依赖事件，因为应用重启或事件丢失后需要 snapshot 恢复。

## 11. 状态真实性规则

1. 未读到数据不等于零。
2. 未验证不等于失败，也不等于成功。
3. Browser preview 只能显示 preview 标签。
4. shadow estimate 不等于实际节省。
5. process exists 不等于 runtime identity verified。
6. port open 不等于由 d-token 进程持有。
7. config write success 不等于 route 工作。
8. routed request success 必须有 route decision 和 Ledger 证据。

## 12. 测试要求

- Rust DTO serialization snapshot。
- TypeScript schema compile test。
- stale 数据保留测试。
- browser preview 不渲染 runtime claim。
- 两个并发 refresh 不重复覆盖新数据。
- review 后外部修改触发 stale。
- apply receipt 驱动正确 slice 刷新。
- app 隐藏后暂停轮询，恢复后立即刷新。
- runtime offline 时保留 transaction 和 rollback evidence。

## 13. 实施步骤

1. 定义 `EvidenceEnvelope` 和稳定错误 code。
2. 为当前 Dashboard DTO 增加 schema version。
3. 自动生成 TypeScript 类型。
4. 迁移 `useRuntimeDashboard`，移除 `any`。
5. 按领域拆 store，但保持一个 app-wide runtime provider。
6. 引入 operation progress event。
7. 删除页面级旧字段 fallback。
8. 用 E2E 验证跨页面一致性。