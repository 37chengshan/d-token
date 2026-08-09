//! dtoken-context-bus — LeanCTX 九边界 Rust-owned domain（单一 domain authority）。
//!
//! 本 crate 是 CONTEXT-1（docs/releases/v0.0.1/REQUIREMENTS.md）中 LeanCTX 九边界的唯一
//! 归属实现，policy 从 `dtoken-compress` / `dtoken-context-store` 搬移而来而非复制：
//!
//! 1. Context Bus — 编排 Session Cache / Context Relay / Recovery Index / SharedContext；
//! 2. Session Cache — 会话内 map-signature 复用（CacheReuse receipt）；
//! 3. Context Relay — 同项目跨 Agent 复用（RelayHit receipt）；
//! 4. map-signature reuse — 规范化内容签名，等价内容复用同一签名；
//! 5. query-aware retrieval — 按 query tokens 检索可复用条目；
//! 6. Recovery Pointer/Manifest — 崩溃后从 manifest 恢复条目，指针可解析；
//! 7. Stale/Superseded — TTL 过期或 source 变化即 miss；
//! 8. SharedContext authorization — grant/revoke，权限变化即 miss；
//! 9. self-hit-size boundary — 自命中=0、跨项目污染=0、条目 size 上限。
//!
//! 安全边界：本 crate 不持久化 raw/secret（条目写入前经 secret-shape gate 与 size gate），
//! 不计算 compression savings（RelayHit/CacheReuse receipt 与压缩分账严格分离，
//! 见 [`receipt`]）。
//!
//! store 只保留 persistence port：本 crate 定义 [`persistence::PersistencePort`]，
//! `dtoken-context-store` 实现该 port 提供 SQLite 持久化，policy 不再进入 store。

pub mod bus;
pub mod cache;
pub mod policy;
pub mod persistence;
pub mod receipt;
pub mod recovery;
pub mod relay;
pub mod scope;
pub mod session;
pub mod signature;
pub mod shared;

pub use bus::{BusConfig, ConsultOutcome, ContextBus};
pub use policy::{CacheEvidenceStatus, ProviderCacheEvidence, ProviderCacheStrategy};
pub use receipt::{BusReceipt, BusSavingsKind, CacheReuseReceipt, RelayHitReceipt};
pub use scope::{BusScope, GrantScope, ScopeError};
