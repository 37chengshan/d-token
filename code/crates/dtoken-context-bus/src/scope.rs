//! Scope model — 从 `dtoken-context-store` 搬移的 scope 校验 policy（单一 authority）。
//!
//! 原实现位于 `dtoken-context-store/src/lib.rs`（`validate_scope_identifier` /
//! `validate_revision` / `normalized_optional_scope`）。policy 搬移到本 crate，
//! store 只保留 persistence port，不再拥有校验规则。

use std::fmt;

/// 一个 Context Bus 访问范围：project / agent / session 三元组。
/// session 可为空（空串表示会话无关的项目级范围）。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BusScope {
    pub project_id: String,
    pub agent_id: String,
    pub session_id: String,
}

impl BusScope {
    pub fn new(project_id: impl Into<String>, agent_id: impl Into<String>, session_id: impl Into<String>) -> Self {
        Self {
            project_id: project_id.into(),
            agent_id: agent_id.into(),
            session_id: session_id.into(),
        }
    }

    /// 仅项目范围（relay 命中检查用）：忽略 agent/session。
    pub fn project_key(&self) -> &str {
        &self.project_id
    }

    pub fn is_valid(&self) -> bool {
        validate_scope_identifier("project_id", &self.project_id).is_ok()
            && validate_scope_identifier("agent_id", &self.agent_id).is_ok()
            && (self.session_id.is_empty()
                || validate_scope_identifier("session_id", &self.session_id).is_ok())
    }
}

impl fmt::Display for BusScope {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{}/{}/{}",
            self.project_id, self.agent_id, self.session_id
        )
    }
}

/// SharedContext grant 的范围：project / owner / grantee / session。
/// grant 绑定到单一会话，不是项目级 Agent 权限。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct GrantScope {
    pub project_id: String,
    pub owner_agent_id: String,
    pub grantee_agent_id: String,
    pub session_id: String,
}

impl GrantScope {
    pub fn new(
        project_id: impl Into<String>,
        owner_agent_id: impl Into<String>,
        grantee_agent_id: impl Into<String>,
        session_id: impl Into<String>,
    ) -> Self {
        Self {
            project_id: project_id.into(),
            owner_agent_id: owner_agent_id.into(),
            grantee_agent_id: grantee_agent_id.into(),
            session_id: session_id.into(),
        }
    }

    pub fn is_valid(&self) -> bool {
        validate_scope_identifier("project_id", &self.project_id).is_ok()
            && validate_scope_identifier("owner_agent_id", &self.owner_agent_id).is_ok()
            && validate_scope_identifier("grantee_agent_id", &self.grantee_agent_id).is_ok()
            && validate_scope_identifier("session_id", &self.session_id).is_ok()
    }
}

/// Scope 校验错误（原 store `bail!` 的字符串化，保留原 message 语义）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopeError(pub String);

impl fmt::Display for ScopeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for ScopeError {}

/// 搬移自 `dtoken-context-store::validate_scope_identifier`。
pub fn validate_scope_identifier(field: &str, value: &str) -> Result<(), ScopeError> {
    let value = value.trim();
    if value.is_empty()
        || value.len() > 128
        || value.chars().any(|character| {
            !(character.is_ascii_alphanumeric()
                || matches!(character, '-' | '_' | '.' | ':' | '/' | '@'))
        })
    {
        return Err(ScopeError(format!(
            "{field} must be a bounded opaque identifier"
        )));
    }
    Ok(())
}

/// 搬移自 `dtoken-context-store::validate_revision`。
pub fn validate_revision(field: &str, value: &str) -> Result<(), ScopeError> {
    validate_scope_identifier(field, value)
}

/// 搬移自 `dtoken-context-store::normalized_optional_scope`。
pub fn normalized_optional_scope(value: Option<&str>) -> String {
    value.map(str::trim).unwrap_or_default().to_string()
}

/// 非空 scope 才保留（与 store `nonempty_scope` 语义一致）。
pub fn nonempty_scope(value: String) -> Option<String> {
    (!value.is_empty()).then_some(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scope_identifier_validation_matches_store_semantics() {
        assert!(validate_scope_identifier("project_id", "my-project/org@team").is_ok());
        assert!(validate_scope_identifier("agent_id", "agent-01").is_ok());
        // 空 / 超长 / 非法字符全部拒绝。
        assert!(validate_scope_identifier("project_id", "  ").is_err());
        assert!(validate_scope_identifier("project_id", &"x".repeat(129)).is_err());
        assert!(validate_scope_identifier("project_id", "has space").is_err());
        assert!(validate_scope_identifier("project_id", "has;sql").is_err());
        // revision 复用同一规则。
        assert!(validate_revision("revision", "abc-123").is_ok());
        assert!(validate_revision("revision", "bad revision").is_err());
    }

    #[test]
    fn bus_scope_validity_and_display() {
        let scope = BusScope::new("proj", "agent-a", "sess-1");
        assert!(scope.is_valid());
        assert_eq!(scope.to_string(), "proj/agent-a/sess-1");
        // session 为空时仍有效（项目级范围）。
        assert!(BusScope::new("proj", "agent-a", "").is_valid());
        // 非法 project 直接判 invalid。
        assert!(!BusScope::new("bad project", "agent-a", "sess-1").is_valid());
    }

    #[test]
    fn grant_scope_requires_all_four_identifiers() {
        let scope = GrantScope::new("proj", "owner", "grantee", "sess-1");
        assert!(scope.is_valid());
        assert!(!GrantScope::new("proj", "owner", "grantee", "").is_valid());
        assert!(!GrantScope::new("", "owner", "grantee", "sess-1").is_valid());
    }

    #[test]
    fn optional_scope_normalization_matches_store() {
        assert_eq!(normalized_optional_scope(None), "");
        assert_eq!(normalized_optional_scope(Some("  sess ")), "sess");
        assert_eq!(nonempty_scope("".to_string()), None);
        assert_eq!(nonempty_scope("sess".to_string()), Some("sess".to_string()));
    }
}
