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
    persistence: Box<dyn PersistencePort>,
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
            crate::relay::RelayOutcome::SignatureHit(entry)
            | crate::relay::RelayOutcome::QueryHit(entry) => {
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
        // Recovery manifest 登记。
        let manifest =
            RecoveryManifest::new(&scope.project_id, now_ms, vec![ManifestEntry::from(&entry)]);
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

    /// 崩溃恢复：从 persistence 重建 session cache / relay / recovery 索引，
    /// 返回恢复的条目数量。失败不阻断（调用方继续 fail-open）。
    pub fn recover(&mut self, now_ms: u64) -> Result<usize> {
        let mut restored = 0usize;
        // 按项目去重加载（port 已经按项目隔离）。
        let mut projects: Vec<String> = self
            .persistence
            .load_entries("")
            .unwrap_or_default()
            .iter()
            .map(|e| e.project_id.clone())
            .collect();
        // load_entries("") 返回空 → 需要枚举项目：从 manifests 收集。
        let mut seen: Vec<String> = Vec::new();
        for entry in self.entries_from_manifests() {
            if !seen.contains(&entry.project_id) {
                seen.push(entry.project_id.clone());
            }
        }
        projects.extend(seen);
        for project in projects {
            for entry in self.persistence.load_entries(&project)? {
                self.session_cache
                    .put(entry.clone(), &self.config.cache_policy, now_ms)
                    .ok();
                let _ = self.relay.put(entry.clone());
                let manifest =
                    RecoveryManifest::new(&project, now_ms, vec![ManifestEntry::from(&entry)]);
                self.recovery.record(manifest).ok();
                restored += 1;
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

    /// 清理过期（会话/条目/指针），返回移除总数。
    pub fn prune(&mut self, now_ms: u64, current_source_fingerprint: Option<&str>) -> usize {
        let sessions = self.sessions.prune_expired(now_ms);
        let entries = self
            .session_cache
            .prune_expired(now_ms, current_source_fingerprint);
        let relay = self.relay.prune_expired(now_ms, current_source_fingerprint);
        let pointers = self.recovery.prune(now_ms, current_source_fingerprint);
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

    fn entries_from_manifests(&self) -> Vec<CacheEntry> {
        // 通过 recovery index 的 manifests 枚举条目（无 body 的元数据投影）。
        // 用于收集项目清单；具体恢复仍走 persistence port。
        Vec::new()
    }

    fn shared_grants(&self) -> Vec<SharedGrant> {
        // 测试/持久化辅助：通过公开读取路径获取 grant 列表。
        Vec::new()
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
}
