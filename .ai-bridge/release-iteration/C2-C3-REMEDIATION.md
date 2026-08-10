# C2/C3 Remediation Report — codex/final/c2-c3-remediation

- 日期：2026-08-10
- 候选基线：`15f98cf`（`release: bind registry to tracked evidence`）
- 切片：T-C2A（bounded codec fuzz）、T-C2B（cache live-zone）、T-C3A（context bus core）、T-C3B（relay retrieval）、T-C3C（recovery shared）
- 工作树：`/Users/cc/Documents/d-token-worktrees/final-c2-c3-remediation`（隔离 worktree，独立提交，未 push / 未建 PR）

## 交付清单

### 1. Bounded codec fuzz runner（T-C2A）
- `code/crates/dtoken-compress/tests/bounded_fuzz.rs`（untracked，既有用户文件；审计确认其完整性：4 个确定性 fuzz 家族 + code codec 专项，固定种子，lossless/secret-exact/malformed/exclusive-ownership 全覆盖，frozen-prefix 字节恒等断言）。**不属本切片新增**，本 commit 不含该文件。

### 2. SQLite PersistencePort（T-C3A/C3C，生产持久化）
- 新增 `code/crates/dtoken-context-store/src/bus_persistence.rs`：`SqlitePersistence` 实现 `dtoken_context_bus::persistence::PersistencePort`。
  - 表：`bus_entry` / `bus_manifest` / `bus_grant`（WAL + foreign_keys + synchronous=NORMAL；私有权限 0600/0700 + Windows DACL 同 `ContextStore` 基线；拒绝 symlink 路径）。
  - 语义：按 project 隔离读取；`delete_entry/delete_manifest/delete_grant` 提供 TTL/stale/superseded 的持久层删除（重启不复活）；`project_ids()` 供启动恢复枚举。
  - 不持久化 raw/secret：body 写入前必经 bus secret-shape gate（结构性禁止；无明文列）。
- `dtoken-context-bus/src/persistence.rs`：`PersistencePort` trait 扩展（delete ×3 + project_ids），`InMemoryPersistence` 同步实现。
- `dtoken-context-bus/src/cache.rs` / `recovery.rs` / `shared.rs`：`CacheEntry` / `ManifestEntry` / `SharedGrant` 增加 serde derive（SQLite JSON 列 roundtrip）。
- `dtoken-context-bus/src/signature.rs`：新增 `sha256_hex(bytes)`（绑定 digest 助手）。

### 3. 启动恢复 + TTL/stale/superseded/delete 语义（T-C3C）
- `dtoken-context-bus/src/bus.rs`：
  - `recover(now_ms)` 重写：按 `project_ids()` 枚举；stale 条目跳过并从持久层删除；manifest 全 fresh 才登记否则删除；grant/revoke 按四元组还原（revoked 保持 revoked）。
  - `prune(now_ms, fingerprint)` 扩展：内存清理后同步删除持久层非 fresh 条目与失效 manifest。
  - `record_bound(...)`：manifest 携带 request_id + source_digest + decision_digest；`RecoveryManifest::new_bound` / `binding_matches`（与 C0C1 `RecoveryReceipt::mint` 交叉绑定，C0C1 已确认）。
  - `shared_grants()` 真实实现（revoke 后持久化撤销状态）。
  - `persistence()` 只读访问（测试/审计）。
- `dtoken-context-bus/src/recovery.rs`：`RecoveryManifest` 绑定字段 + `binding_matches` + `new_bound`。
- `dtoken-core-api/src/lib.rs` 生产启动（`new_persistent_with_private_store`）：`context_bus` 从 InMemory 改为 `SqlitePersistence::open(runtime_dir/context-bus.sqlite3)` + 启动 `recover`（失败 fail-open 不阻断）。**生产恢复不再是 InMemory-only。**

### 4. 每个 HTTP ingress（含 direct Anthropic）经 context bus（T-C3A/T-C3B）
- `compress_for_route_with_relay` 新增 `baseline_digest` 参数；miss 路径改 `record_bound`（绑定 request_id + 验证基线 digest + route/provider decision digest）。
- `anthropic_route_payload`：Direct Anthropic 分支从 `compress_for_route(Off)` 改为经 `compress_for_route_with_relay`（bus 命中即复用候选、miss 走 record_bound），在 hybrid 规划之前。
- `anthropic_messages` handler：仅当真实替换（任一 receipt `avoided_bytes > 0`）时跳过 hybrid 规划 / raw-wire 保持，append `context_bus` stage event（transform_applied=false）；query-aware 信号（0 字节）不改变 wire body，继续走 hybrid/raw-wire。
- chat / responses / native-responses 三处 callsite 全部迁移（baseline_digest = 精确 pre-plan 规范化 body 的 sha256）。
- RequestScope 身份：`resolve_configured_request_scope` 增加 project_id/session_id 参数；`header_evidence` 读取 `x-dtoken-project-id` / `x-dtoken-session-id`（bus scope 未投影时 miss，绝不 unsafe 投影）。

### 5. 安全修复：query-aware relay 不得跨请求替换 body（PipelineSecurityReview）
- 根因：无 provider cache evidence 时 source fingerprint 为 sha256(empty)，查询 token 仅 route/model → query-aware retrieval 命中会把请求 A 的变换 body 发到请求 B 的线上。
- 修复（bus.rs `consult`）：`RelayOutcome::QueryHit` 仅作为「存在可复用条目」信号——剥离 body、`avoided_bytes=0`，调用方回退到对当前请求自身内容压缩（负收益 fallback，零替换）；只有精确 `map_signature` 命中的 `SignatureHit`（canonical 内容 + source fingerprint 完全一致）才替换 body。
- 回归测试：`bus::tests::query_hit_never_substitutes_another_requests_body`（bus 单测）+ `context_bus_same_route_model_different_body_no_cross_body_reuse`（core-api 集成）——同 route/model、不同 body、无 cache evidence，证明无跨 body 复用。

### 6. 独立 relay/session-cache receipts 分账（CONTEXT-3）
- `BusReceipt` / `RelayHitReceipt` / `CacheReuseReceipt` 保持「avoided-retransmission」语义；`build_ledger_event` 的 `bus_receipts` metadata 独立于 `saved_tokens`（compression savings 永不进入 receipt，receipt 永不计入 `saved_tokens`；无 double count）。本切片不触碰 `RequestLog.tokens_saved` 求和路径（C4 范围）。

### 7. Registry 指针（C2/C3 owned，最小 diff）
- `support/fixtures/unified-capability-registry-v1.json`：8 条 `aggregate.relay` / `c3.*` / `context-store.persistence` 从指向不存在的 `dtoken_context_store::{SharedContext, SessionCache, ContextRelay, retrieve, RelayRegistry}` 修正为真实符号：
  - `aggregate.relay` / `c3.context-relay` → `dtoken_context_bus::relay::ContextRelay`
  - `c3.context-bus` → `dtoken_context_bus::bus::ContextBus`
  - `c3.session-cache` → `dtoken_context_bus::cache::SessionCache`
  - `c3.query-aware-retrieval` → `dtoken_context_bus::relay::ContextRelay::query`
  - `c3.stale-superseded` → `dtoken_context_bus::cache::CacheEntry::state_at`
  - `c3.shared-context-authorization` → `dtoken_context_bus::shared::SharedContext`
  - `context-store.persistence` → `dtoken_context_store::bus_persistence::SqlitePersistence`
- 仅 8 条目 × 2 字段（owner_crate + dispatch_symbol）的最小文本 diff（30 行）；`status` 保持原样（C8 拥有 N0 密封权，本切片不翻转 verified）。

## 验证证据（本切片定向运行，全部通过）

```
cargo check -p dtoken-context-bus                                          ok
cargo check -p dtoken-context-store                                       ok
cargo check -p dtoken-core-api -p dtoken-context-store -p dtoken-context-bus \
             -p dtoken-compress -p dtoken-proxy -p dtoken-agent-bridge     ok（无 error）
cargo test -p dtoken-context-bus       → 48 passed（含新 record_bound/recover/prune/revocation/query-hit 回归）
cargo test -p dtoken-context-store --lib → 7 passed（含 bus_persistence 4 项）
cargo test -p dtoken-core-api --lib context_bus → 3 passed
  （context_bus_relay_reuses_across_two_app_states_without_cross_body_pollution
     + context_bus_same_route_model_different_body_no_cross_body_reuse + 既有）
```

`cargo check --workspace` 仅因 `apps/desktop/src-tauri` 的 sidecar build-script（环境缺 macOS 二进制）失败——非本切片引入，未触碰 desktop。

## 边界与未触碰
- 未触碰 `dtoken-runtime-pipeline`（C0C1）、`RequestLog` 类型、RTK stats（C4）、`ccr_store.rs`、`mcp.rs`；registry 聚合扇入归 C8。
- 未触碰 desktop/React、support acceptance、CBM。
- loopback / secret-redaction / exact-restore / fail-closed 契约保持：bus 全部 consult/record 失败 fail-open；secret-shape gate 拒绝写入（Err 丢弃不降级）；manifest/entries 从不含 raw/secret body。

## 审计与提交
- `support/scripts/git_operation_audit.py record --operation commit --summary ... --branch codex/final/c2-c3-remediation --head-before 15f98cf`
- Commit 仅含本切片 owned 文件（详见 git status / diff 与提交 message）。
