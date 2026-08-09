//! Context Relay — 同项目跨 Agent 的结构化上下文复用（九边界之三）。
//!
//! 规则（搬移自 store memory share 授权语义 + compress map-signature policy）：
//! - 两 Agent 同项目可命中：条目 project 相同、grantee 有未撤销的 grant；
//! - 跨项目污染 = 0：project 不同直接 miss；
//! - self-hit = 0：条目 owner 与请求 agent 相同 → 不命中 relay；
//! - 文件/TTL/权限/source 变化即 miss；
//! - query-aware retrieval：按 query tokens 检索可复用条目（九边界之五）。
//!
//! relay 只承载脱敏后的候选内容；secret/raw 不得进入 relay 条目。

use crate::cache::{CacheEntry, EntryState};
use crate::scope::{validate_scope_identifier, ScopeError};
use crate::signature::normalize_query_tokens;

/// Relay 查询请求。
#[derive(Debug, Clone)]
pub struct RelayQuery<'a> {
    pub project_id: &'a str,
    pub agent_id: &'a str,
    pub session_id: &'a str,
    pub map_signature: Option<&'a str>,
    pub source_fingerprint: &'a str,
    pub query_tokens: &'a [String],
    pub now_ms: u64,
}

/// Relay 查询结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RelayOutcome {
    /// 精确 map-signature 命中。
    SignatureHit(CacheEntry),
    /// query-aware retrieval 命中（token 匹配）。
    QueryHit(CacheEntry),
    Miss,
}

/// 判断 grant 是否有效（无 grant 语义：owner=request 的条目除外，但 relay 禁止 self-hit）。
pub fn relay_authorized(
    project_id: &str,
    owner_agent_id: &str,
    grantee_agent_id: &str,
    session_id: &str,
    is_granted: impl FnOnce(&str, &str, &str, &str) -> bool,
) -> bool {
    if owner_agent_id == grantee_agent_id {
        // self-hit = 0：owner 不能 relay 自己的条目。
        return false;
    }
    is_granted(project_id, owner_agent_id, grantee_agent_id, session_id)
}

/// Context Relay 注册表：跨 Agent 可复用条目池。
#[derive(Debug, Clone, Default)]
pub struct ContextRelay {
    entries: Vec<CacheEntry>,
}

impl ContextRelay {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// 写入 relay 条目（须已通过 secret/size gate）。
    pub fn put(&mut self, entry: CacheEntry) -> Result<(), ScopeError> {
        validate_scope_identifier("project_id", &entry.project_id)?;
        validate_scope_identifier("agent_id", &entry.agent_id)?;
        if !entry.session_id.is_empty() {
            validate_scope_identifier("session_id", &entry.session_id)?;
        }
        if entry.map_signature.len() != 64 || entry.source_fingerprint.len() != 64 {
            return Err(ScopeError("relay entry signatures must be 64 hex".to_string()));
        }
        if let Some(body) = entry.body.as_deref() {
            if crate::signature::is_secret_shaped(body) {
                return Err(ScopeError("relay entry body is secret-shaped".to_string()));
            }
        }
        if let Some(existing) = self
            .entries
            .iter_mut()
            .find(|e| e.map_signature == entry.map_signature)
        {
            *existing = entry;
        } else {
            self.entries.push(entry);
        }
        Ok(())
    }

    /// relay 查询：两 Agent 同项目命中；跨项目污染=0；self-hit=0；
    /// 文件/TTL/权限/source 变化即 miss。
    ///
    /// `is_granted(project, owner, grantee, session)` 由 SharedContext 提供
    /// （默认实现为无 grant → 仅当条目是「共享条目」时允许，见 [`RelayAccess`]）。
    pub fn query(&self, query: &RelayQuery<'_>, is_granted: &impl Fn(&str, &str, &str, &str) -> bool) -> RelayOutcome {
        if !query.project_id.is_empty()
            && !self.entries.is_empty()
            && self.entries.iter().all(|e| e.project_id != query.project_id)
        {
            // 快速路径：本项目没有任何条目。
            return RelayOutcome::Miss;
        }
        // 精确签名路径。
        if let Some(signature) = query.map_signature {
            for entry in self.entries.iter().filter(|e| e.project_id == query.project_id) {
                if entry.map_signature != signature {
                    continue;
                }
                if entry.state_at(query.now_ms, Some(query.source_fingerprint)) != EntryState::Fresh {
                    continue;
                }
                if entry.is_self_hit(query.agent_id, query.session_id) {
                    continue; // self-hit = 0。
                }
                if !relay_authorized(
                    query.project_id,
                    &entry.agent_id,
                    query.agent_id,
                    &entry.session_id,
                    is_granted,
                ) {
                    continue; // 权限变化即 miss。
                }
                return RelayOutcome::SignatureHit(entry.clone());
            }
        }
        // query-aware retrieval 路径（九边界之五）：token 匹配 + 同项目 + fresh +
        // 非 self-hit + 授权；取最近记录的最优条目。
        let normalized_query = normalize_query_tokens(query.query_tokens);
        if normalized_query.is_empty() {
            return RelayOutcome::Miss;
        }
        let mut best: Option<CacheEntry> = None;
        for entry in self.entries.iter().filter(|e| e.project_id == query.project_id) {
            if entry.state_at(query.now_ms, Some(query.source_fingerprint)) != EntryState::Fresh {
                continue;
            }
            if entry.is_self_hit(query.agent_id, query.session_id) {
                continue;
            }
            if !relay_authorized(
                query.project_id,
                &entry.agent_id,
                query.agent_id,
                &entry.session_id,
                is_granted,
            ) {
                continue;
            }
            let score = entry
                .query_tokens
                .iter()
                .filter(|token| normalized_query.contains(token))
                .count();
            if score == 0 {
                continue;
            }
            let candidate = entry.clone();
            if best
                .as_ref()
                .is_none_or(|current: &CacheEntry| candidate.recorded_at_ms > current.recorded_at_ms)
            {
                best = Some(candidate);
            }
        }
        best.map_or(RelayOutcome::Miss, RelayOutcome::QueryHit)
    }

    /// 清理过期条目，返回移除数量。
    pub fn prune_expired(&mut self, now_ms: u64, current_source_fingerprint: Option<&str>) -> usize {
        let before = self.entries.len();
        self.entries
            .retain(|e| matches!(e.state_at(now_ms, current_source_fingerprint), EntryState::Fresh));
        before - self.entries.len()
    }
}

/// 默认 relay 访问策略：无 SharedContext 时拒绝一切跨 Agent relay（fail-closed）。
#[derive(Debug, Clone, Copy, Default)]
pub struct RelayAccess;

impl RelayAccess {
    pub fn default_grant(_project: &str, _owner: &str, _grantee: &str, _session: &str) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::signature::{map_signature, source_fingerprint};

    fn entry(project: &str, agent: &str, session: &str, content: &str, at_ms: u64, tokens: &[&str]) -> CacheEntry {
        CacheEntry {
            map_signature: map_signature(project, content),
            project_id: project.to_string(),
            agent_id: agent.to_string(),
            session_id: session.to_string(),
            source_fingerprint: fp(),
            recorded_at_ms: at_ms,
            ttl_seconds: Some(60),
            body_bytes: content.len(),
            query_tokens: tokens.iter().map(|t| t.to_string()).collect(),
            body: Some(format!("candidate:{content}")),
        }
    }

    /// 与 `entry` helper 写入的 fingerprint 一致的当前 source fingerprint。
    fn fp() -> String {
        source_fingerprint(&"a".repeat(64), None)
    }

    fn grant_all(_p: &str, _o: &str, _g: &str, _s: &str) -> bool {
        true
    }

    #[test]
    fn two_agents_same_project_can_hit() {
        let mut relay = ContextRelay::new();
        relay.put(entry("proj", "agent-a", "sess-1", "shared plan", 1_000, &["plan"])).unwrap();
        let query = RelayQuery {
            project_id: "proj",
            agent_id: "agent-b",
            session_id: "sess-2",
            map_signature: Some(&entry("proj", "agent-a", "sess-1", "shared plan", 1_000, &["plan"]).map_signature),
            source_fingerprint: &fp(),
            query_tokens: &[],
            now_ms: 1_000,
        };
        assert!(matches!(relay.query(&query, &grant_all), RelayOutcome::SignatureHit(_)));
    }

    #[test]
    fn cross_project_pollution_is_zero() {
        let mut relay = ContextRelay::new();
        relay.put(entry("proj-a", "agent-a", "sess-1", "same content", 1_000, &[])).unwrap();
        let query = RelayQuery {
            project_id: "proj-b",
            agent_id: "agent-b",
            session_id: "sess-2",
            map_signature: Some(&entry("proj-a", "agent-a", "sess-1", "same content", 1_000, &[]).map_signature),
            source_fingerprint: &fp(),
            query_tokens: &[],
            now_ms: 1_000,
        };
        assert_eq!(relay.query(&query, &grant_all), RelayOutcome::Miss);
    }

    #[test]
    fn self_hit_is_zero() {
        let mut relay = ContextRelay::new();
        relay.put(entry("proj", "agent-a", "sess-1", "content", 1_000, &[])).unwrap();
        // 同一 agent 查询 → self-hit → miss（即使有 grant）。
        let query = RelayQuery {
            project_id: "proj",
            agent_id: "agent-a",
            session_id: "sess-1",
            map_signature: Some(&entry("proj", "agent-a", "sess-1", "content", 1_000, &[]).map_signature),
            source_fingerprint: &fp(),
            query_tokens: &[],
            now_ms: 1_000,
        };
        assert_eq!(relay.query(&query, &grant_all), RelayOutcome::Miss);
    }

    #[test]
    fn permission_change_makes_miss() {
        let mut relay = ContextRelay::new();
        relay.put(entry("proj", "agent-a", "sess-1", "content", 1_000, &[])).unwrap();
        let query = RelayQuery {
            project_id: "proj",
            agent_id: "agent-b",
            session_id: "sess-2",
            map_signature: Some(&entry("proj", "agent-a", "sess-1", "content", 1_000, &[]).map_signature),
            source_fingerprint: &fp(),
            query_tokens: &[],
            now_ms: 1_000,
        };
        // 无 grant（fail-closed 默认）→ miss。
        assert_eq!(relay.query(&query, &RelayAccess::default_grant), RelayOutcome::Miss);
        // 有 grant → hit。
        assert!(matches!(relay.query(&query, &grant_all), RelayOutcome::SignatureHit(_)));
    }

    #[test]
    fn ttl_and_source_change_make_miss() {
        let mut relay = ContextRelay::new();
        relay.put(entry("proj", "agent-a", "sess-1", "content", 1_000, &[])).unwrap();
        let signature = entry("proj", "agent-a", "sess-1", "content", 1_000, &[]).map_signature;
        // TTL 过期。
        let stale_query = RelayQuery {
            project_id: "proj",
            agent_id: "agent-b",
            session_id: "sess-2",
            map_signature: Some(&signature),
            source_fingerprint: &fp(),
            query_tokens: &[],
            now_ms: 70_000,
        };
        assert_eq!(relay.query(&stale_query, &grant_all), RelayOutcome::Miss);
        // source 变化。
        let source_query = RelayQuery {
            project_id: "proj",
            agent_id: "agent-b",
            session_id: "sess-2",
            map_signature: Some(&signature),
            source_fingerprint: &source_fingerprint(&"a".repeat(64), Some("new-digest")),
            query_tokens: &[],
            now_ms: 1_000,
        };
        assert_eq!(relay.query(&source_query, &grant_all), RelayOutcome::Miss);
    }

    #[test]
    fn query_aware_retrieval_hits_by_tokens() {
        let mut relay = ContextRelay::new();
        relay.put(entry("proj", "agent-a", "sess-1", "a", 1_000, &["route", "auth"])).unwrap();
        relay.put(entry("proj", "agent-a", "sess-1", "b", 2_000, &["route", "logging"])).unwrap();
        let query = RelayQuery {
            project_id: "proj",
            agent_id: "agent-b",
            session_id: "sess-2",
            map_signature: None,
            source_fingerprint: &fp(),
            query_tokens: &["route".to_string(), "logging".to_string()],
            now_ms: 2_000,
        };
        match relay.query(&query, &grant_all) {
            RelayOutcome::QueryHit(entry) => assert_eq!(entry.body.as_deref(), Some("candidate:b")),
            other => panic!("expected QueryHit, got {other:?}"),
        }
        // 无匹配 token → miss。
        let miss_query = RelayQuery {
            project_id: "proj",
            agent_id: "agent-b",
            session_id: "sess-2",
            map_signature: None,
            source_fingerprint: &fp(),
            query_tokens: &["unrelated".to_string()],
            now_ms: 2_000,
        };
        assert_eq!(relay.query(&miss_query, &grant_all), RelayOutcome::Miss);
    }

    #[test]
    fn secret_shaped_relay_body_is_rejected() {
        let mut relay = ContextRelay::new();
        let mut bad = entry("proj", "agent-a", "sess-1", "leak", 1_000, &[]);
        bad.body = Some("api_key=sk-abc".to_string());
        assert!(relay.put(bad).is_err());
        assert!(relay.is_empty());
    }
}
