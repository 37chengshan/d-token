//! SharedContext authorization — grant/revoke 状态机（九边界之八）。
//!
//! 搬移自 `dtoken-context-store` 的 memory share grant 语义（`grant_memory_share_for_scope`
//! / `revoke_memory_share_for_scope` / `MemoryShareGrant`）：grant 绑定
//! project/owner/grantee/session 四元组，带 grant_revision 与 revoked_at；
//! 权限变化（revoke）后 relay 立即 miss。

use crate::scope::{validate_revision, GrantScope, ScopeError};
use uuid::Uuid;

/// 一条 SharedContext grant。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SharedGrant {
    pub grant_id: String,
    pub project_id: String,
    pub owner_agent_id: String,
    pub grantee_agent_id: String,
    pub session_id: String,
    pub grant_revision: String,
    pub granted_at_ms: u64,
    pub revoked_at_ms: Option<u64>,
}

/// grant 请求（等价 store `ScopedMemoryShareGrantRequest`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrantRequest {
    pub scope: GrantScope,
    pub grant_revision: String,
    pub granted_at_ms: u64,
}

/// SharedContext 注册表：按四元组管理 grant 生命周期。
#[derive(Debug, Clone, Default)]
pub struct SharedContext {
    grants: Vec<SharedGrant>,
}

impl SharedContext {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.grants.len()
    }

    pub fn is_empty(&self) -> bool {
        self.grants.is_empty()
    }

    /// 全部 grant 的只读投影（持久化撤销状态 / 审计用）。
    pub fn grants(&self) -> Vec<SharedGrant> {
        self.grants.clone()
    }

    /// grant（幂等 upsert：同 revision 且未撤销 → 原样返回；否则覆盖/新建）。
    pub fn grant(&mut self, request: &GrantRequest) -> Result<SharedGrant, ScopeError> {
        if !request.scope.is_valid() {
            return Err(ScopeError(
                "memory share grant requires a valid scope".to_string(),
            ));
        }
        validate_revision("memory share grant revision", &request.grant_revision)?;
        if request.granted_at_ms == 0 {
            return Err(ScopeError(
                "memory share grant time must be positive".to_string(),
            ));
        }
        let scope = &request.scope;
        if let Some(existing) = self.grant_for_scope(
            &scope.project_id,
            &scope.owner_agent_id,
            &scope.grantee_agent_id,
            &scope.session_id,
        ) {
            if existing.grant_revision == request.grant_revision && existing.revoked_at_ms.is_none()
            {
                return Ok(existing.clone());
            }
            let updated = SharedGrant {
                grant_id: existing.grant_id.clone(),
                project_id: scope.project_id.clone(),
                owner_agent_id: scope.owner_agent_id.clone(),
                grantee_agent_id: scope.grantee_agent_id.clone(),
                session_id: scope.session_id.clone(),
                grant_revision: request.grant_revision.clone(),
                granted_at_ms: request.granted_at_ms,
                revoked_at_ms: None,
            };
            if let Some(slot) = self
                .grants
                .iter_mut()
                .find(|g| g.grant_id == updated.grant_id)
            {
                *slot = updated.clone();
            }
            return Ok(updated);
        }
        let grant = SharedGrant {
            grant_id: format!("grant-{}", Uuid::new_v4().simple()),
            project_id: scope.project_id.clone(),
            owner_agent_id: scope.owner_agent_id.clone(),
            grantee_agent_id: scope.grantee_agent_id.clone(),
            session_id: scope.session_id.clone(),
            grant_revision: request.grant_revision.clone(),
            granted_at_ms: request.granted_at_ms,
            revoked_at_ms: None,
        };
        self.grants.push(grant.clone());
        Ok(grant)
    }

    /// revoke：仅撤销匹配四元组 + revision + 未撤销的 grant。
    pub fn revoke(
        &mut self,
        scope: &GrantScope,
        expected_revision: &str,
        revoked_at_ms: u64,
    ) -> Result<bool, ScopeError> {
        if !scope.is_valid() {
            return Err(ScopeError(
                "memory share revoke requires a valid scope".to_string(),
            ));
        }
        validate_revision("expected grant revision", expected_revision)?;
        if revoked_at_ms == 0 {
            return Err(ScopeError(
                "memory share revoke time must be positive".to_string(),
            ));
        }
        let Some(grant) = self.grant_for_scope_mut(
            &scope.project_id,
            &scope.owner_agent_id,
            &scope.grantee_agent_id,
            &scope.session_id,
        ) else {
            return Ok(false);
        };
        if grant.grant_revision != expected_revision || grant.revoked_at_ms.is_some() {
            return Ok(false);
        }
        grant.revoked_at_ms = Some(revoked_at_ms);
        Ok(true)
    }

    /// grant 是否有效（未撤销 + revision 匹配可省略：见 store 语义）。
    pub fn is_granted(
        &self,
        project_id: &str,
        owner: &str,
        grantee: &str,
        session_id: &str,
    ) -> bool {
        self.grant_for_scope(project_id, owner, grantee, session_id)
            .is_some_and(|g| g.revoked_at_ms.is_none())
    }

    fn grant_for_scope(
        &self,
        project_id: &str,
        owner: &str,
        grantee: &str,
        session_id: &str,
    ) -> Option<&SharedGrant> {
        self.grants.iter().find(|g| {
            g.project_id == project_id
                && g.owner_agent_id == owner
                && g.grantee_agent_id == grantee
                && g.session_id == session_id
        })
    }

    fn grant_for_scope_mut(
        &mut self,
        project_id: &str,
        owner: &str,
        grantee: &str,
        session_id: &str,
    ) -> Option<&mut SharedGrant> {
        self.grants.iter_mut().find(|g| {
            g.project_id == project_id
                && g.owner_agent_id == owner
                && g.grantee_agent_id == grantee
                && g.session_id == session_id
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scope() -> GrantScope {
        GrantScope::new("proj", "agent-a", "agent-b", "sess-1")
    }

    #[test]
    fn grant_then_revoke_then_is_granted_flips() {
        let mut shared = SharedContext::new();
        let request = GrantRequest {
            scope: scope(),
            grant_revision: "rev-1".to_string(),
            granted_at_ms: 1_000,
        };
        let grant = shared.grant(&request).unwrap();
        assert_eq!(grant.grant_revision, "rev-1");
        assert!(shared.is_granted("proj", "agent-a", "agent-b", "sess-1"));
        // 幂等：同 revision 原样返回，不重复。
        let again = shared.grant(&request).unwrap();
        assert_eq!(again.grant_id, grant.grant_id);
        assert_eq!(shared.len(), 1);
        // revoke。
        assert!(shared.revoke(&scope(), "rev-1", 2_000).unwrap());
        assert!(!shared.is_granted("proj", "agent-a", "agent-b", "sess-1"));
        // 重复 revoke → false。
        assert!(!shared.revoke(&scope(), "rev-1", 3_000).unwrap());
    }

    #[test]
    fn grant_update_replaces_revision_and_clears_revocation() {
        let mut shared = SharedContext::new();
        shared
            .grant(&GrantRequest {
                scope: scope(),
                grant_revision: "rev-1".to_string(),
                granted_at_ms: 1_000,
            })
            .unwrap();
        shared.revoke(&scope(), "rev-1", 2_000).unwrap();
        // 新 revision 重新授权 → 撤销被清除。
        let renewed = shared
            .grant(&GrantRequest {
                scope: scope(),
                grant_revision: "rev-2".to_string(),
                granted_at_ms: 3_000,
            })
            .unwrap();
        assert_eq!(renewed.grant_revision, "rev-2");
        assert!(renewed.revoked_at_ms.is_none());
        assert!(shared.is_granted("proj", "agent-a", "agent-b", "sess-1"));
        assert_eq!(shared.len(), 1);
    }

    #[test]
    fn invalid_scope_or_revision_is_rejected() {
        let mut shared = SharedContext::new();
        let bad_scope = GrantRequest {
            scope: GrantScope::new("proj", "owner", "grantee", ""),
            grant_revision: "rev-1".to_string(),
            granted_at_ms: 1_000,
        };
        assert!(shared.grant(&bad_scope).is_err());
        let bad_revision = GrantRequest {
            scope: scope(),
            grant_revision: "bad revision".to_string(),
            granted_at_ms: 1_000,
        };
        assert!(shared.grant(&bad_revision).is_err());
        assert!(shared.revoke(&scope(), "rev-1", 0).is_err());
        assert!(shared.is_empty());
    }
}
