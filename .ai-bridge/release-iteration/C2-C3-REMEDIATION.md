# C2/C3 Remediation Report — codex/final/c2-c3-remediation

- 日期：2026-08-10
- 候选基线：`15f98cf`（`release: bind registry to tracked evidence`）
- 切片：T-C2A（bounded codec fuzz）、T-C2B（cache live-zone）、T-C3A（context bus core）、T-C3B（relay retrieval）、T-C3C（recovery shared）
- 工作树：`/Users/cc/Documents/d-token-worktrees/final-c2-c3-remediation`（隔离 worktree，独立提交）

## 交付清单

### 1. Bounded codec fuzz runner（T-C2A）
- `code/crates/dtoken-compress/tests/bounded_fuzz.rs`（untracked，既有用户文件；本次审计确认其完整性：4 个确定性 fuzz 家族 + code codec 专项，固定种子，lossless/secret-exact/malformed/exclusive-ownership 全覆盖，frozen-prefix 字节恒等断言）。**不属本切片新增**，但属 C2A 范围；已保留未动。

### 2. SQLite PersistencePort（T-C3A/C3C，生产持久化）
- 新增 `code/crates/dtoken-context-store/src/bus_persistence.rs`：`SqlitePersistence` 实现 `dtoken_context_bus::persistence::PersistencePort`。
  - 表：`bus_entry` / `bus_manifest` / `bus_grant`（WAL + foreign_keys + synchronous=NORMAL；私有权限 0600/0700 + Windows DACL 同 `ContextStore` 基线；拒绝 symlink 路径）。
  - 语义：按 project 隔离读取；`delete_entry/delete_manifest/delete_grant` 提供 TTL/stale/superseded 的持久层删除（重启不复活）；`project_ids()` 供启动恢复枚举。
  - 不持久化 raw/secret：body 写入前必经 bus secret-shape gate（结构性禁止；无额外明文列）。
- `dtoken-context-bus/src/persistence.rs`：`PersistencePort` trait 扩展（delete ×3 + project_ids），`InMemoryPersistence` 同步实现。
- `dtoken-context-bus/src/cache.rs` / `recovery.rs` / `shared.rs`：`CacheEntry` / `ManifestEntry` / `SharedGrant` 增加 serde derive（SQLite JSON 列 roundtrip）。
- `dtoken-context-bus/src/signature.rs`：新增 `sha256_hex(bytes)`（绑定 digest 助手）。

### 3. 启动恢复 + TTL/stale/superseded/delete 语义（T-C3C）
- `dtoken-context-bus/src/bus.rs`：
  - `recover(now_ms)` 重写：按 `project_ids()` 枚举；stale 条目跳过并从持久层删除；manifest 全 fresh 才登记否则删除；grant/revoke 按四元组还原（revoked 保持 revoked）。
  - `prune(now_ms, fingerprint)` 扩展：内存清理后同步删除持久层非 fresh 条目与失效 manifest。
  - `record_bound(...)`：manifest 携带 request_id + source_digest + decision_digest；`RecoveryManifest::new_bound` / `binding_matches`（与 C0C1 `RecoveryReceipt::mint` 交叉绑定，C0C1 已确认）。
  - `shared_grants()` 真实实现（revoke 后持久化撤销状态）。
  - `persistence` 字段公开 + `persistence()` 只读访问（消费者/测试）。
- `dtoken-context-bus/src/recovery.rs`：`RecoveryManifest` 绑定字段 + `binding_matches`。
- `dtoken-core-api/src/lib.rs` 生产启动（`new_persistent_with_private_store`）：`context_bus` 从 InMemory 改为 `SqlitePersistence::open(runtime_dir/context-bus.sqlite3)` + 启动 `recover`（失败 fail-open 不阻断）。**生产恢复不再是 InMemory-only。**

### 4. 每个 HTTP ingress（含 direct Anthropic）经 context bus（T-C3A/T-C3B）
- `compress_for_route_with_relay` 新增 `baseline_digest` 参数；miss 路径改 `record_bound`（绑定 request_id + 验证基线 digest + route/provider decision digest）。
- `anthropic_route_payload`：Direct Anthropic 分支从 `compress_for_route(Off)` 改为经 `compress_for_route_with_relay`（bus 命中即复用，miss 记录）。
- `anthropic_messages` handler：`bus_receipts` 非空时跳过 hybrid 规划/raw-wire 保持（append `context_bus` stage event；transform_applied=false）。
- chat / responses / native-responses 三处 callsite 全部迁移（baseline_digest = 精确 pre-plan 规范化 body 的 sha256）。
- RequestScope 身份：`resolve_configured_request_scope` 增加 project_id/session_id 参数；`header_evidence` 读取 `x-dtoken-project-id` / `x-dtoken-session-id`（bus scope 未投影时 miss，绝不 unsafe 投影）。

### 5. 独立 relay/session-cache receipts 分账（CONTEXT-3）
- 既有 `BusReceipt` / `RelayHitReceipt` / `CacheReuseReceipt` 保持「avoided-retransmission」语义；`build_ledger_event` 的 `bus_receipts` metadata 独立于 `saved_tokens`（无 double count）。本切片不触碰 `RequestLog.tokens_saved` 求和路径（C4 范围）。

### 6. 测试
- `dtoken-context-bus`（48 pass）：新增 record_bound 绑定、recover stale 跳过+删除、prune 持久层删除、revocation 持久化恢复；既有 relay/cache/self-hit/cross-project/secret gate 全绿。
- `dtoken-context-store`（30 pass）：`bus_persistence` 4 项（roundtrip/隔离/delete/binding+revocation/symlink 拒绝）+ 既有 durability/migrations/permission（**修复了测试清理误删临时目录的根因**：`remove_dir_all(parent)` → 仅删 db 文件 + -wal/-shm sidecars）。
- `dtoken-core-api`：`context_bus_relay_survives_restart_across_two_app_states`（两个 AppState 实例：写 → 重启 → SQLite 恢复 → 跨 Agent RelayHit + 跨项目 Miss）+ `context_bus_relay_reuses_across_two_app_states_without_cross_body_pollution`（安全回归）2 pass；direct Anthropic 既有 cache breakpoint 测试保持通过（全 lib 177 过滤运行验证）。

## 边界与未触碰
- 未触碰 `dtoken-runtime-pipeline`（C0C1 所有）、`RequestLog` 类型、RTK stats（C4）、`ccr_store.rs`、`mcp.rs`、registry 聚合（C8 在收；c3.* 符号口径已与 C8 对齐：`dtoken_context_bus::{bus::ContextBus, cache::SessionCache, relay::ContextRelay, shared::SharedContext}`，persistence 为 `dtoken_context_store::bus_persistence::SqlitePersistence`）。
- 未触碰 desktop/React、support acceptance、CBM。
- loopback/secret-redaction/exact-restore/fail-closed 契约保持：bus 全部 consult/record 失败 fail-open；secret-shape gate 拒绝写入（Err 丢弃不降级）。

## 验证命令（全部通过）
```
cargo check -p dtoken-context-bus
cargo check -p dtoken-context-store
cargo check -p dtoken-core-api --all-targets
cargo test -p dtoken-context-bus        # 48 passed
cargo test -p dtoken-context-store      # 30 passed
cargo test -p dtoken-core-api --lib context_bus_relay  # 2 passed
```
