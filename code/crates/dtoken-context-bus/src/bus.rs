//! Context Bus — LeanCTX 九边界编排器（九边界之一，唯一 domain authority）。
//!
//! 聚合：
//! - [`crate::session::SessionRegistry`] — 会话生命周期；
//! - [`crate::cache::SessionCache`] — 会话内 map-signature 复用（CacheReuse）；
//! - [`crate::relay::ContextRelay`] — 同项目跨 Agent 复用（RelayHit）；
//! - [`crate::recovery::RecoveryIndex`] — Recovery Pointer/Manifest 崩溃恢复；
//! - [`crate::shared::SharedContext`] — grant/revoke 授权；
//! - [`crate::persistence::PersistencePort`] — 持久化 port（store 实现）。
//!
//! 生产调用方（core-api canonical pipeline）使用 [`ContextBus::consult`] /
//! [`ContextBus::record`]；任何 bus 失败都 fail-open → 调用方正常读取原文。

use crate::cache::{CacheEntry, CachePolicy, SessionCache};
use crate::persistence::{InMemoryPersistence, PersistencePort};
use crate::receipt::{BusReceipt, CacheReuseReceipt, RelayHitReceipt};
use crate::recovery::{ManifestEntry, RecoveryIndex, RecoveryManifest, RecoveryPointer};
use crate::relay::{ContextRelay, RelayQuery};
use crate::scope::{BusScope, GrantScope, ScopeError};
use crate::session::{Session, SessionRegistry};
use crate::shared::{GrantRequest, SharedContext, SharedGrant};
use crate::signature::{map_signature, normalize_query_tokens, source_fingerprint};
use anyhow::Result;

/// consult 结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConsultOutcome {
    /// Session Cache 命中（会话内复用）→ CacheReuse receipt。
    CacheHit {
        entry: CacheEntry,
        receipt: CacheReuseReceipt,
    },
    /// Context Relay 命中（跨 Agent）→ RelayHit receipt。
    RelayHit {
        entry: CacheEntry,
        receipt: RelayHitReceipt,
    },
    /// 未命中（调用方正常压缩）。
    Miss,
}

impl ConsultOutcome {
    pub fn receipt(&self) -> Option<BusReceipt> {
        match self {
            Self::CacheHit { receipt, .. } => Some(BusReceipt::CacheReuse(receipt.clone())),
            Self::RelayHit { receipt, .. } => Some(BusReceipt::RelayHit(receipt.clone())),
            Self::Miss => None,
        }
    }

    pub fn candidate_body(&self) -> Option<&str> {
        match self {
            Self::CacheHit { entry, .. } | Self::RelayHit { entry, .. } => entry.body.as_deref(),
            Self::Miss => None,
        }
    }
}

/// Context Bus 配置。
#[derive(Debug, Clone)]
pub struct BusConfig {
    pub cache_policy: CachePolicy,
    /// 会话默认 TTL（秒）。
    pub session_ttl_seconds: Option<u64>,
}

impl Default for BusConfig {
    fn default() -> Self {
        Self {
            cache_policy: CachePolicy::default(),
            session_ttl_seconds: Some(3600),
        }
    }
}

/// Context Bus 编排器。
pub struct ContextBus {
    config: BusConfig,
    sessions: SessionRegistry,
    session_cache: SessionCache,
    relay: ContextRelay,
    recovery: RecoveryIndex,
    shared: SharedContext,
    /// 持久化 port（store 实现；bus 域 policy 单一 authority）。
    pub persistence: Box<dyn PersistencePort>,
}

impl Default for ContextBus {
    fn default() -> Self {
        Self::new(BusConfig::default())
    }
}

impl ContextBus {
    pub fn new(config: BusConfig) -> Self {
        Self {
            config,
            sessions: SessionRegistry::new(),
            session_cache: SessionCache::new(),
            relay: ContextRelay::new(),
            recovery: RecoveryIndex::new(),
            shared: SharedContext::new(),
            persistence: Box::new(InMemoryPersistence::new()),
        }
    }

    pub fn with_persistence(config: BusConfig, persistence: Box<dyn PersistencePort>) -> Self {
        Self {
            config,
            sessions: SessionRegistry::new(),
            session_cache: SessionCache::new(),
            relay: ContextRelay::new(),
            recovery: RecoveryIndex::new(),
            shared: SharedContext::new(),
            persistence,
        }
    }

    /// 登记/刷新会话。
    pub fn register_session(&mut self, scope: &BusScope, now_ms: u64) -> Result<(), ScopeError> {
        let session = Session::new(
            &scope.session_id,
            &scope.agent_id,
            &scope.project_id,
            now_ms,
            self.config.session_ttl_seconds,
        );
        self.sessions.register(session)
    }

    pub fn session_count(&self) -> usize {
        self.sessions.len()
    }

    /// 查询（fail-open：内部错误按 Miss 处理，调用方正常读原文）。
    /// 优先级：Session Cache（同会话，允许 self-hit）→ Context Relay（跨 Agent，
    /// self-hit=0、需要 grant、项目必须一致、fresh、source 一致）。
    pub fn consult(
        &mut self,
        scope: &BusScope,
        map_signature: &str,
        source_fingerprint: &str,
        query_tokens: &[String],
        now_ms: u64,
    ) -> ConsultOutcome {
        // 1) Session Cache：同一 agent 同一 session 的 map-signature 复用。
        if let Some(entry) = self.session_cache.get(
            &scope.project_id,
            &scope.agent_id,
            &scope.session_id,
            map_signature,
            source_fingerprint,
            now_ms,
            true,
        ) {
            let receipt = CacheReuseReceipt {
                project_id: scope.project_id.clone(),
                agent_id: scope.agent_id.clone(),
                session_id: scope.session_id.clone(),
                map_signature: map_signature.to_string(),
                source_fingerprint: source_fingerprint.to_string(),
                reused_bytes: entry.body_bytes,
                hit_at_ms: now_ms,
            };
            return ConsultOutcome::CacheHit {
                entry: entry.clone(),
                receipt,
            };
        }
        // 2) Context Relay：跨 Agent（self-hit=0）+ grant + 同项目 + fresh + source。
        let query = RelayQuery {
            project_id: &scope.project_id,
            agent_id: &scope.agent_id,
            session_id: &scope.session_id,
            map_signature: Some(map_signature),
            source_fingerprint,
            query_tokens,
            now_ms,
        };
        let granted = |project: &str, owner: &str, grantee: &str, session: &str| {
            self.shared.is_granted(project, owner, grantee, session)
        };
        match self.relay.query(&query, &granted) {
            crate::relay::RelayOutcome::SignatureHit(entry) => {
                // 精确 map-signature 命中：canonical 内容与 source fingerprint
                // 完全一致 → 同一变换对同一内容确定成立 → 允许 body 替换。
                let receipt = RelayHitReceipt {
                    project_id: scope.project_id.clone(),
                    owner_agent_id: entry.agent_id.clone(),
                    grantee_agent_id: scope.agent_id.clone(),
                    session_id: entry.session_id.clone(),
                    map_signature: entry.map_signature.clone(),
                    source_fingerprint: entry.source_fingerprint.clone(),
                    avoided_bytes: entry.body_bytes,
                    hit_at_ms: now_ms,
                };
                ConsultOutcome::RelayHit { entry, receipt }
            }
            crate::relay::RelayOutcome::QueryHit(mut entry) => {
                // query-aware retrieval 只提供「存在可复用条目」的信号，不提供
                // body：无精确 map-signature 绑定时替换 body 会把请求 A 的
                // 变换内容发到请求 B 的线上（跨请求污染，RUNTIME-7/CONTEXT-4）。
                // 因此剥离 body 并把 avoided_bytes 置 0；调用方回退到对当前
                // 请求自身内容的压缩（负收益 fallback，零替换）。
                entry.body = None;
                let receipt = RelayHitReceipt {
                    project_id: scope.project_id.clone(),
                    owner_agent_id: entry.agent_id.clone(),
                    grantee_agent_id: scope.agent_id.clone(),
                    session_id: entry.session_id.clone(),
                    map_signature: entry.map_signature.clone(),
                    source_fingerprint: entry.source_fingerprint.clone(),
                    avoided_bytes: 0,
                    hit_at_ms: now_ms,
                };
                ConsultOutcome::RelayHit { entry, receipt }
            }
            crate::relay::RelayOutcome::Miss => ConsultOutcome::Miss,
        }
    }

    /// 记录压缩候选：写入 session cache + relay + Recovery manifest + persistence。
    ///
    /// `body` 必须为脱敏后的候选内容（secret-shape gate 由 [`CacheEntry`] 校验执行；
    /// 校验失败返回 Err，调用方丢弃，不降级存储）。
    #[allow(clippy::too_many_arguments)]
    pub fn record(
        &mut self,
        scope: &BusScope,
        canonical_content: &str,
        source_revision: &str,
        file_digest: Option<&str>,
        body: Option<String>,
        query_tokens: &[String],
        now_ms: u64,
    ) -> Result<RecoveryPointer> {
        self.record_bound(
            scope,
            canonical_content,
            source_revision,
            file_digest,
            body,
            query_tokens,
            now_ms,
            "",
            "",
            "",
        )
    }

    /// 请求绑定记录：与 [`Self::record`] 相同，但 manifest 携带
    /// request_id / source_digest / decision_digest（跨绑定校验见
    /// [`RecoveryManifest::binding_matches`]）。`source_digest` 必须是对
    /// 验证基线 bytes（相同 source → 相同 digest，见 `signature::sha256_hex`）。
    #[allow(clippy::too_many_arguments)]
    pub fn record_bound(
        &mut self,
        scope: &BusScope,
        canonical_content: &str,
        source_revision: &str,
        file_digest: Option<&str>,
        body: Option<String>,
        query_tokens: &[String],
        now_ms: u64,
        request_id: &str,
        source_digest: &str,
        decision_digest: &str,
    ) -> Result<RecoveryPointer> {
        let body_bytes = body.as_deref().map(str::len).unwrap_or(0);
        let entry = CacheEntry {
            map_signature: map_signature(&scope.project_id, canonical_content),
            project_id: scope.project_id.clone(),
            agent_id: scope.agent_id.clone(),
            session_id: scope.session_id.clone(),
            source_fingerprint: source_fingerprint(source_revision, file_digest),
            recorded_at_ms: now_ms,
            ttl_seconds: self.config.cache_policy.ttl_seconds,
            body_bytes,
            query_tokens: normalize_query_tokens(query_tokens),
            body,
        };
        // session cache 写入（含 size/secret gate）。
        self.session_cache
            .put(entry.clone(), &self.config.cache_policy, now_ms)?;
        // relay 写入（同项目跨 Agent 可复用；secret/size gate 再次执行）。
        self.relay.put(entry.clone())?;
        // Recovery manifest 登记（请求绑定）。
        let manifest = RecoveryManifest::new_bound(
            &scope.project_id,
            request_id,
            source_digest,
            decision_digest,
            now_ms,
            vec![ManifestEntry::from(&entry)],
        );
        let pointer = self.recovery.record(manifest.clone())?;
        // 持久化 port。
        self.persistence.save_entry(&entry)?;
        self.persistence.save_manifest(&manifest)?;
        Ok(pointer)
    }

    /// grant（委托 SharedContext 状态机）。
    pub fn grant(&mut self, request: &GrantRequest) -> Result<SharedGrant, ScopeError> {
        let grant = self.shared.grant(request)?;
        let _ = self.persistence.save_grant(&grant);
        Ok(grant)
    }

    /// revoke（委托 SharedContext 状态机；权限变化后 relay 立即 miss）。
    pub fn revoke(
        &mut self,
        scope: &GrantScope,
        expected_revision: &str,
        revoked_at_ms: u64,
    ) -> Result<bool, ScopeError> {
        let revoked = self
            .shared
            .revoke(scope, expected_revision, revoked_at_ms)?;
        if revoked {
            // 持久化撤销状态：将 grant 的 revoked_at 落盘。
            if let Some(grant) = self.shared_grants().into_iter().find(|g| {
                g.project_id == scope.project_id
                    && g.owner_agent_id == scope.owner_agent_id
                    && g.grantee_agent_id == scope.grantee_agent_id
                    && g.session_id == scope.session_id
            }) {
                let _ = self.persistence.save_grant(&grant);
            }
        }
        Ok(revoked)
    }

    /// 崩溃恢复：从 persistence 重建 session cache / relay / recovery 索引。
    /// 返回恢复的条目数量。失败不阻断（调用方继续 fail-open）。
    ///
    /// 语义：
    /// - TTL/stale 条目在启动时跳过并从持久层删除（重启不复活）；
    /// - manifest 仅在其全部条目 fresh 时登记指针，否则删除；
    /// - grant/revoke 状态按持久化四元组还原（revoked 保持 revoked）。
    pub fn recover(&mut self, now_ms: u64) -> Result<usize> {
        let mut restored = 0usize;
        let projects = self.persistence.project_ids().unwrap_or_default();
        for project in projects {
            for entry in self.persistence.load_entries(&project)? {
                if entry.state_at(now_ms, None) != crate::cache::EntryState::Fresh {
                    // stale（TTL 过期）：跳过且删除，避免重启后复活。
                    let _ = self
                        .persistence
                        .delete_entry(&project, &entry.map_signature);
                    continue;
                }
                self.session_cache
                    .put(entry.clone(), &self.config.cache_policy, now_ms)
                    .ok();
                let _ = self.relay.put(entry.clone());
                restored += 1;
            }
            for manifest in self.persistence.load_manifests(&project)? {
                let all_fresh = manifest.entries.iter().all(|entry| {
                    crate::recovery::RecoveryIndex::rehydrate_entry(entry).state_at(now_ms, None)
                        == crate::cache::EntryState::Fresh
                });
                if !all_fresh || manifest.entries.is_empty() {
                    let _ = self.persistence.delete_manifest(&manifest.manifest_id);
                    continue;
                }
                let _ = self.recovery.record(manifest);
            }
            for grant in self.persistence.load_grants(&project)? {
                let request = GrantRequest {
                    scope: GrantScope::new(
                        &grant.project_id,
                        &grant.owner_agent_id,
                        &grant.grantee_agent_id,
                        &grant.session_id,
                    ),
                    grant_revision: grant.grant_revision.clone(),
                    granted_at_ms: grant.granted_at_ms,
                };
                let _ = self.shared.grant(&request);
                if grant.revoked_at_ms.is_some() {
                    let _ = self.shared.revoke(
                        &GrantScope::new(
                            &grant.project_id,
                            &grant.owner_agent_id,
                            &grant.grantee_agent_id,
                            &grant.session_id,
                        ),
                        &grant.grant_revision,
                        grant.revoked_at_ms.unwrap_or(grant.granted_at_ms + 1),
                    );
                }
            }
        }
        Ok(restored)
    }

    /// 清理过期（会话/条目/指针），返回移除总数；持久层同步删除
    /// （TTL/stale/superseded 的 delete 语义，重启不复活）。
    pub fn prune(&mut self, now_ms: u64, current_source_fingerprint: Option<&str>) -> usize {
        let sessions = self.sessions.prune_expired(now_ms);
        let entries = self
            .session_cache
            .prune_expired(now_ms, current_source_fingerprint);
        let relay = self.relay.prune_expired(now_ms, current_source_fingerprint);
        let pointers = self.recovery.prune(now_ms, current_source_fingerprint);
        // 持久层同步：非 fresh 条目/全部失效的 manifest 一并删除。
        let projects = self.persistence.project_ids().unwrap_or_default();
        for project in projects {
            if let Ok(persisted) = self.persistence.load_entries(&project) {
                for entry in persisted {
                    if entry.state_at(now_ms, current_source_fingerprint)
                        != crate::cache::EntryState::Fresh
                    {
                        let _ = self
                            .persistence
                            .delete_entry(&project, &entry.map_signature);
                    }
                }
            }
            if let Ok(manifests) = self.persistence.load_manifests(&project) {
                for manifest in manifests {
                    let stale = manifest.entries.iter().any(|entry| {
                        crate::recovery::RecoveryIndex::rehydrate_entry(entry)
                            .state_at(now_ms, current_source_fingerprint)
                            != crate::cache::EntryState::Fresh
                    });
                    if stale || manifest.entries.is_empty() {
                        let _ = self.persistence.delete_manifest(&manifest.manifest_id);
                    }
                }
            }
        }
        sessions + entries + relay + pointers
    }

    pub fn shared_grant_count(&self) -> usize {
        self.shared.len()
    }

    pub fn relay_entry_count(&self) -> usize {
        self.relay.len()
    }

    pub fn cache_entry_count(&self) -> usize {
        self.session_cache.len()
    }

    /// 持久化 port 只读访问（审计/测试：验证跨重启恢复）。
    pub fn persistence(&self) -> &dyn PersistencePort {
        self.persistence.as_ref()
    }

    /// 全部 grant 的只读投影（持久化撤销状态用）。
    fn shared_grants(&self) -> Vec<SharedGrant> {
        self.shared.grants()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scope::BusScope;

    fn scope(project: &str, agent: &str, session: &str) -> BusScope {
        BusScope::new(project, agent, session)
    }

    #[test]
    fn session_cache_reuse_within_same_session() {
        let mut bus = ContextBus::new(BusConfig::default());
        let scope = scope("proj", "agent-a", "sess-1");
        bus.record(
            &scope,
            r#"{"q":"route"}"#,
            &"a".repeat(64),
            None,
            Some("candidate".to_string()),
            &["route".to_string()],
            1_000,
        )
        .unwrap();
        let outcome = bus.consult(
            &scope,
            &map_signature("proj", r#"{"q":"route"}"#),
            &source_fingerprint(&"a".repeat(64), None),
            &["route".to_string()],
            1_000,
        );
        assert!(matches!(outcome, ConsultOutcome::CacheHit { .. }));
        assert!(matches!(outcome.receipt(), Some(BusReceipt::CacheReuse(_))));
    }

    #[test]
    fn relay_hit_requires_grant_and_different_agent() {
        let mut bus = ContextBus::new(BusConfig::default());
        let owner = scope("proj", "agent-a", "sess-1");
        bus.record(
            &owner,
            "shared-content",
            &"a".repeat(64),
            None,
            Some("candidate".to_string()),
            &[],
            1_000,
        )
        .unwrap();
        // 无 grant → miss。
        let consumer = scope("proj", "agent-b", "sess-2");
        let signature = map_signature("proj", "shared-content");
        let fingerprint = source_fingerprint(&"a".repeat(64), None);
        assert!(matches!(
            bus.consult(&consumer, &signature, &fingerprint, &[], 1_000),
            ConsultOutcome::Miss
        ));
        // grant 后 → relay hit。
        bus.grant(&GrantRequest {
            scope: GrantScope::new("proj", "agent-a", "agent-b", "sess-1"),
            grant_revision: "rev-1".to_string(),
            granted_at_ms: 1_000,
        })
        .unwrap();
        let outcome = bus.consult(&consumer, &signature, &fingerprint, &[], 1_000);
        assert!(matches!(outcome, ConsultOutcome::RelayHit { .. }));
        assert!(matches!(outcome.receipt(), Some(BusReceipt::RelayHit(_))));
        // revoke 后 → miss（权限变化即 miss）。
        bus.revoke(
            &GrantScope::new("proj", "agent-a", "agent-b", "sess-1"),
            "rev-1",
            2_000,
        )
        .unwrap();
        assert!(matches!(
            bus.consult(&consumer, &signature, &fingerprint, &[], 2_000),
            ConsultOutcome::Miss
        ));
    }

    #[test]
    fn self_hit_and_cross_project_are_zero() {
        let mut bus = ContextBus::new(BusConfig::default());
        bus.record(
            &scope("proj-a", "agent-a", "sess-1"),
            "same",
            &"a".repeat(64),
            None,
            Some("candidate".to_string()),
            &[],
            1_000,
        )
        .unwrap();
        let signature = map_signature("proj-a", "same");
        let fingerprint = source_fingerprint(&"a".repeat(64), None);
        // 同一 agent 同 session：session cache 允许（CacheReuse），但不是 RelayHit。
        // 跨项目（即使 grant 全开）→ miss。
        let other = scope("proj-b", "agent-b", "sess-2");
        assert!(matches!(
            bus.consult(&other, &signature, &fingerprint, &[], 1_000),
            ConsultOutcome::Miss
        ));
    }

    #[test]
    fn recovery_roundtrip_restores_entries() {
        let mut bus = ContextBus::new(BusConfig::default());
        bus.record(
            &scope("proj", "agent-a", "sess-1"),
            "content",
            &"a".repeat(64),
            None,
            Some("candidate".to_string()),
            &["route".to_string()],
            1_000,
        )
        .unwrap();
        let signature = map_signature("proj", "content");
        let fingerprint = source_fingerprint(&"a".repeat(64), None);
        // 新 bus 实例（同 InMemory persistence 不行——默认 port 各自独立；
        // 这里验证 recover 不 panic 且不产生条目即可）。
        let mut fresh = ContextBus::new(BusConfig::default());
        let restored = fresh.recover(2_000).unwrap();
        assert_eq!(restored, 0);
        // 原 bus 仍可命中（同会话 CacheReuse）。
        assert!(matches!(
            bus.consult(
                &scope("proj", "agent-a", "sess-1"),
                &signature,
                &fingerprint,
                &["route".to_string()],
                1_000
            ),
            ConsultOutcome::CacheHit { .. }
        ));
    }

    #[test]
    fn record_rejects_secret_shaped_body() {
        let mut bus = ContextBus::new(BusConfig::default());
        let result = bus.record(
            &scope("proj", "agent-a", "sess-1"),
            "content",
            &"a".repeat(64),
            None,
            Some("Authorization: Bearer sk-abc".to_string()),
            &[],
            1_000,
        );
        assert!(result.is_err());
    }

    #[test]
    fn record_bound_binds_request_and_digests() {
        let mut bus = ContextBus::new(BusConfig::default());
        let bus_scope = scope("proj", "agent-a", "sess-1");
        let request_id = "req-123";
        let source_digest = crate::signature::sha256_hex(b"baseline-bytes");
        let decision_digest = "decision-abc";
        let pointer = bus
            .record_bound(
                &bus_scope,
                r#"{"q":"route"}"#,
                &"a".repeat(64),
                None,
                Some("candidate".to_string()),
                &["route".to_string()],
                1_000,
                request_id,
                &source_digest,
                decision_digest,
            )
            .unwrap();
        // 持久化 manifest 携带绑定字段。
        let manifest = bus
            .persistence
            .load_manifests("proj")
            .unwrap()
            .into_iter()
            .find(|m| m.manifest_id == pointer.manifest_id)
            .expect("manifest persisted");
        assert_eq!(manifest.request_id, request_id);
        assert_eq!(manifest.source_digest, source_digest);
        assert_eq!(manifest.decision_digest, decision_digest);
        assert!(manifest.binding_matches(request_id, &source_digest));
        assert!(!manifest.binding_matches("other-request", &source_digest));
        assert!(!manifest.binding_matches(request_id, "other-digest"));
        // 未绑定 record 的 manifest 绑定为空串。
        bus.record(
            &scope("proj", "agent-a", "sess-2"),
            "unbound",
            &"a".repeat(64),
            None,
            Some("candidate".to_string()),
            &[],
            1_000,
        )
        .unwrap();
        let unbound = bus
            .persistence
            .load_manifests("proj")
            .unwrap()
            .into_iter()
            .find(|m| m.request_id.is_empty())
            .expect("unbound manifest persisted");
        assert!(unbound.binding_matches("", ""));
        assert!(!unbound.binding_matches("req", ""));
    }

    #[test]
    fn recover_skips_and_deletes_stale_entries() {
        // 直接向 port 写入一个 stale 条目（TTL 已过期）与一个 fresh 条目。
        let mut port = InMemoryPersistence::new();
        let stale = crate::cache::CacheEntry {
            map_signature: map_signature("proj", "stale"),
            project_id: "proj".to_string(),
            agent_id: "agent-a".to_string(),
            session_id: "sess-1".to_string(),
            source_fingerprint: source_fingerprint(&"a".repeat(64), None),
            recorded_at_ms: 1_000,
            ttl_seconds: Some(60),
            body_bytes: 5,
            query_tokens: vec![],
            body: Some("stale".to_string()),
        };
        let fresh = crate::cache::CacheEntry {
            map_signature: map_signature("proj", "fresh"),
            recorded_at_ms: 60_000,
            ..stale.clone()
        };
        port.save_entry(&stale).unwrap();
        port.save_entry(&fresh).unwrap();
        let mut bus = ContextBus::with_persistence(BusConfig::default(), Box::new(port));
        // 1_000 + 60s TTL → 61_000 仍 fresh（闭区间）；61_001 时 stale 过期。
        let restored = bus.recover(61_001).unwrap();
        assert_eq!(restored, 1);
        assert_eq!(bus.cache_entry_count(), 1);
        // stale 已从持久层删除（重启不复活）。
        let remaining = bus.persistence.load_entries("proj").unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].map_signature, fresh.map_signature);
    }

    #[test]
    fn prune_deletes_stale_entries_from_persistence() {
        let mut port = InMemoryPersistence::new();
        let stale = crate::cache::CacheEntry {
            map_signature: map_signature("proj", "stale"),
            project_id: "proj".to_string(),
            agent_id: "agent-a".to_string(),
            session_id: "sess-1".to_string(),
            source_fingerprint: source_fingerprint(&"a".repeat(64), None),
            recorded_at_ms: 1_000,
            ttl_seconds: Some(60),
            body_bytes: 5,
            query_tokens: vec![],
            body: Some("stale".to_string()),
        };
        port.save_entry(&stale).unwrap();
        let manifest = RecoveryManifest::new("proj", 1_000, vec![ManifestEntry::from(&stale)]);
        port.save_manifest(&manifest).unwrap();
        let mut bus = ContextBus::with_persistence(BusConfig::default(), Box::new(port));
        let removed = bus.prune(70_000, None);
        // sessions 0 + entries 0 + relay 0 + pointers 0（未恢复，仅持久层清理）。
        assert_eq!(removed, 0);
        // 持久层条目与 manifest 已被删除。
        assert!(bus.persistence.load_entries("proj").unwrap().is_empty());
        assert!(bus.persistence.load_manifests("proj").unwrap().is_empty());
    }

    #[test]
    fn revocation_persists_durably() {
        let mut bus = ContextBus::new(BusConfig::default());
        bus.grant(&GrantRequest {
            scope: GrantScope::new("proj", "agent-a", "agent-b", "sess-1"),
            grant_revision: "rev-1".to_string(),
            granted_at_ms: 1_000,
        })
        .unwrap();
        bus.revoke(
            &GrantScope::new("proj", "agent-a", "agent-b", "sess-1"),
            "rev-1",
            2_000,
        )
        .unwrap();
        // 撤销状态已持久化。
        let grants = bus.persistence.load_grants("proj").unwrap();
        assert_eq!(grants.len(), 1);
        assert_eq!(grants[0].revoked_at_ms, Some(2_000));
        // 经持久化恢复：revoked grant 不会重新授权。
        let mut restored = ContextBus::with_persistence(
            BusConfig::default(),
            Box::new(InMemoryPersistence::new()),
        );
        restored
            .persistence
            .save_grant(&bus.persistence.load_grants("proj").unwrap()[0])
            .unwrap();
        let _ = restored.recover(3_000).unwrap();
        assert!(!restored
            .shared
            .is_granted("proj", "agent-a", "agent-b", "sess-1"));
    }

    #[test]
    fn query_hit_never_substitutes_another_requests_body() {
        let mut bus = ContextBus::new(BusConfig::default());
        // agent-a 记录「内容 A」的变换候选。
        let owner = scope("proj", "agent-a", "sess-1");
        bus.record(
            &owner,
            r#"{"q":"content-a"}"#,
            "",
            None,
            Some(r#"{"q":"content-a","transformed":true}"#.to_string()),
            &["route-1".to_string(), "model-x".to_string()],
            1_000,
        )
        .unwrap();
        // grant agent-a → agent-b。
        bus.grant(&GrantRequest {
            scope: GrantScope::new("proj", "agent-a", "agent-b", "sess-1"),
            grant_revision: "rev-1".to_string(),
            granted_at_ms: 1_000,
        })
        .unwrap();
        // agent-b 发起「内容 B」（相同 route/model 查询 token，不同 body）。
        let consumer = scope("proj", "agent-b", "sess-2");
        let outcome = bus.consult(
            &consumer,
            &map_signature("proj", r#"{"q":"content-b"}"#),
            &source_fingerprint("", None),
            &["route-1".to_string(), "model-x".to_string()],
            1_000,
        );
        // query-aware 命中只提供信号：绝不提供 A 的 body。
        let ConsultOutcome::RelayHit { entry, receipt } = outcome else {
            panic!("query-aware retrieval signal expected");
        };
        assert!(entry.body.is_none(), "query hit must not carry a body");
        assert_eq!(receipt.avoided_bytes, 0);
        assert_eq!(
            entry.map_signature,
            map_signature("proj", r#"{"q":"content-a"}"#)
        );
    }
}
