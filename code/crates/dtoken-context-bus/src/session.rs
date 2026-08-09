//! Session 模型 — 会话生命周期与 session-scope 判定。
//!
//! 九边界之一：Session Cache。会话由 project/agent/session 三元组唯一标识，
//! 携带创建时间与 TTL；会话过期后其 cache 条目全部变为 stale（miss）。

use crate::scope::{validate_scope_identifier, ScopeError};
use crate::policy::is_fresh;

/// 一个 Context Bus 会话。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    pub session_id: String,
    pub agent_id: String,
    pub project_id: String,
    pub created_at_ms: u64,
    pub ttl_seconds: Option<u64>,
}

impl Session {
    pub fn new(
        session_id: impl Into<String>,
        agent_id: impl Into<String>,
        project_id: impl Into<String>,
        created_at_ms: u64,
        ttl_seconds: Option<u64>,
    ) -> Self {
        Self {
            session_id: session_id.into(),
            agent_id: agent_id.into(),
            project_id: project_id.into(),
            created_at_ms,
            ttl_seconds,
        }
    }

    pub fn is_valid(&self) -> bool {
        validate_scope_identifier("session_id", &self.session_id).is_ok()
            && validate_scope_identifier("agent_id", &self.agent_id).is_ok()
            && validate_scope_identifier("project_id", &self.project_id).is_ok()
    }

    /// 会话是否仍然 fresh（TTL 未过期）。
    pub fn is_fresh(&self, now_ms: u64) -> bool {
        is_fresh(self.created_at_ms, self.ttl_seconds, now_ms)
    }
}

/// Session 注册表：按 session_id 登记会话，提供过期判定与清理。
#[derive(Debug, Clone, Default)]
pub struct SessionRegistry {
    sessions: Vec<Session>,
}

impl SessionRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// 登记会话（幂等：同 session_id 覆盖）。
    pub fn register(&mut self, session: Session) -> Result<(), ScopeError> {
        if !session.is_valid() {
            return Err(ScopeError("session must be a bounded opaque identifier".to_string()));
        }
        if let Some(existing) = self.sessions.iter_mut().find(|s| s.session_id == session.session_id)
        {
            *existing = session;
        } else {
            self.sessions.push(session);
        }
        Ok(())
    }

    pub fn get(&self, session_id: &str) -> Option<&Session> {
        self.sessions.iter().find(|s| s.session_id == session_id)
    }

    /// 移除过期会话，返回移除数量（stale 清理）。
    pub fn prune_expired(&mut self, now_ms: u64) -> usize {
        let before = self.sessions.len();
        self.sessions.retain(|s| s.is_fresh(now_ms));
        before - self.sessions.len()
    }

    pub fn len(&self) -> usize {
        self.sessions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.sessions.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_freshness_and_ttl() {
        let session = Session::new("sess-1", "agent-a", "proj", 1_000, Some(1));
        assert!(session.is_fresh(1_000));
        assert!(session.is_fresh(2_000));
        assert!(!session.is_fresh(2_001));
        assert!(!session.is_fresh(999));
    }

    #[test]
    fn registry_register_get_and_prune() {
        let mut registry = SessionRegistry::new();
        registry.register(Session::new("sess-1", "agent-a", "proj", 1_000, Some(1))).unwrap();
        registry.register(Session::new("sess-2", "agent-b", "proj", 1_000, None)).unwrap();
        assert_eq!(registry.len(), 2);
        // 幂等覆盖（保持相同 TTL）。
        registry.register(Session::new("sess-1", "agent-a", "proj", 1_000, Some(1))).unwrap();
        assert_eq!(registry.len(), 2);
        assert!(registry.get("sess-1").unwrap().is_fresh(1_000));
        // 过期清理：sess-1（ttl=1s）过期，sess-2（无 ttl）保留。
        assert_eq!(registry.prune_expired(2_001), 1);
        assert_eq!(registry.len(), 1);
        assert!(registry.get("sess-2").is_some());
        // 非法 session 拒绝。
        assert!(registry.register(Session::new("bad session", "agent-a", "proj", 1, None)).is_err());
    }
}
