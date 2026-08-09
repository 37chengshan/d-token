//! Persistence port — store 只保留 persistence（九边界架构边界，RISKS_DECISIONS D5）。
//!
//! 本 crate 定义 bus 所需的持久化契约；`dtoken-context-store` 实现该 port
//! 提供 SQLite 持久化。policy 全部留在本 crate，store 不再拥有校验规则。
//! `InMemoryPersistence` 是默认实现（测试与无 store 环境）。

use crate::cache::CacheEntry;
use crate::recovery::RecoveryManifest;
use crate::shared::SharedGrant;
use anyhow::Result;

/// bus 的持久化契约。实现必须保证：
/// - 不持久化 raw/secret（条目 body 必须已经过 secret-shape gate）；
/// - 按 project 隔离读取（跨项目污染 = 0）。
pub trait PersistencePort: Send + Sync {
    fn save_entry(&mut self, entry: &CacheEntry) -> Result<()>;
    fn load_entries(&self, project_id: &str) -> Result<Vec<CacheEntry>>;
    fn save_manifest(&mut self, manifest: &RecoveryManifest) -> Result<()>;
    fn load_manifests(&self, project_id: &str) -> Result<Vec<RecoveryManifest>>;
    fn save_grant(&mut self, grant: &SharedGrant) -> Result<()>;
    fn load_grants(&self, project_id: &str) -> Result<Vec<SharedGrant>>;
}

/// 内存实现（默认；进程内生效，重启后清空）。
#[derive(Debug, Clone, Default)]
pub struct InMemoryPersistence {
    entries: Vec<CacheEntry>,
    manifests: Vec<RecoveryManifest>,
    grants: Vec<SharedGrant>,
}

impl InMemoryPersistence {
    pub fn new() -> Self {
        Self::default()
    }
}

impl PersistencePort for InMemoryPersistence {
    fn save_entry(&mut self, entry: &CacheEntry) -> Result<()> {
        if let Some(existing) = self
            .entries
            .iter_mut()
            .find(|e| e.map_signature == entry.map_signature)
        {
            *existing = entry.clone();
        } else {
            self.entries.push(entry.clone());
        }
        Ok(())
    }

    fn load_entries(&self, project_id: &str) -> Result<Vec<CacheEntry>> {
        Ok(self
            .entries
            .iter()
            .filter(|e| e.project_id == project_id)
            .cloned()
            .collect())
    }

    fn save_manifest(&mut self, manifest: &RecoveryManifest) -> Result<()> {
        if let Some(existing) = self
            .manifests
            .iter_mut()
            .find(|m| m.manifest_id == manifest.manifest_id)
        {
            *existing = manifest.clone();
        } else {
            self.manifests.push(manifest.clone());
        }
        Ok(())
    }

    fn load_manifests(&self, project_id: &str) -> Result<Vec<RecoveryManifest>> {
        Ok(self
            .manifests
            .iter()
            .filter(|m| m.project_id == project_id)
            .cloned()
            .collect())
    }

    fn save_grant(&mut self, grant: &SharedGrant) -> Result<()> {
        if let Some(existing) = self
            .grants
            .iter_mut()
            .find(|g| g.grant_id == grant.grant_id)
        {
            *existing = grant.clone();
        } else {
            self.grants.push(grant.clone());
        }
        Ok(())
    }

    fn load_grants(&self, project_id: &str) -> Result<Vec<SharedGrant>> {
        Ok(self
            .grants
            .iter()
            .filter(|g| g.project_id == project_id)
            .cloned()
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::signature::{map_signature, source_fingerprint};

    fn entry(project: &str, content: &str) -> CacheEntry {
        CacheEntry {
            map_signature: map_signature(project, content),
            project_id: project.to_string(),
            agent_id: "agent-a".to_string(),
            session_id: "sess-1".to_string(),
            source_fingerprint: source_fingerprint(&"a".repeat(64), None),
            recorded_at_ms: 1_000,
            ttl_seconds: Some(60),
            body_bytes: content.len(),
            query_tokens: vec!["route".to_string()],
            body: Some(format!("candidate:{content}")),
        }
    }

    #[test]
    fn in_memory_port_isolates_by_project() {
        let mut port = InMemoryPersistence::new();
        port.save_entry(&entry("proj-a", "one")).unwrap();
        port.save_entry(&entry("proj-b", "two")).unwrap();
        let a = port.load_entries("proj-a").unwrap();
        let b = port.load_entries("proj-b").unwrap();
        assert_eq!(a.len(), 1);
        assert_eq!(b.len(), 1);
        assert_eq!(port.load_entries("proj-c").unwrap().len(), 0);
        assert_eq!(a[0].project_id, "proj-a");
        assert_eq!(b[0].project_id, "proj-b");
    }

    #[test]
    fn in_memory_port_roundtrips_manifests_and_grants() {
        let mut port = InMemoryPersistence::new();
        let manifest = RecoveryManifest::new("proj", 1_000, vec![]);
        port.save_manifest(&manifest).unwrap();
        let loaded = port.load_manifests("proj").unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].manifest_id, manifest.manifest_id);
        let grant = SharedGrant {
            grant_id: "grant-1".to_string(),
            project_id: "proj".to_string(),
            owner_agent_id: "a".to_string(),
            grantee_agent_id: "b".to_string(),
            session_id: "s".to_string(),
            grant_revision: "rev-1".to_string(),
            granted_at_ms: 1_000,
            revoked_at_ms: None,
        };
        port.save_grant(&grant).unwrap();
        let grants = port.load_grants("proj").unwrap();
        assert_eq!(grants.len(), 1);
        assert_eq!(grants[0].grant_id, "grant-1");
    }
}
