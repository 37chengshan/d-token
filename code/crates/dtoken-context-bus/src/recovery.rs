//! Recovery Pointer/Manifest — 崩溃恢复与 Stale/Superseded 生命周期（九边界之六/七）。
//!
//! 每次成功写入 cache/relay 条目都会产生一个 Recovery Pointer，指向一个
//! Recovery Manifest（条目元数据集合，不含 raw/secret body）。崩溃后可从
//! manifest 恢复条目索引；manifest 缺失或校验失败 → 指针不可解析 → miss
//! （生产调用失败时正常读取原文，见 bus 层 fail-open 语义）。

use crate::cache::{CacheEntry, EntryState};
use crate::scope::{validate_scope_identifier, ScopeError};
use sha2::{Digest, Sha256};
use uuid::Uuid;

/// Recovery Pointer：指向一条可恢复的条目。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveryPointer {
    pub pointer: String,
    pub manifest_id: String,
    pub entry_digest: String,
    pub created_at_ms: u64,
}

/// Manifest 条目（元数据投影，不含 body 原文）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestEntry {
    pub map_signature: String,
    pub project_id: String,
    pub agent_id: String,
    pub session_id: String,
    pub source_fingerprint: String,
    pub recorded_at_ms: u64,
    pub ttl_seconds: Option<u64>,
    pub body_bytes: usize,
    pub query_tokens: Vec<String>,
}

impl From<&CacheEntry> for ManifestEntry {
    fn from(entry: &CacheEntry) -> Self {
        Self {
            map_signature: entry.map_signature.clone(),
            project_id: entry.project_id.clone(),
            agent_id: entry.agent_id.clone(),
            session_id: entry.session_id.clone(),
            source_fingerprint: entry.source_fingerprint.clone(),
            recorded_at_ms: entry.recorded_at_ms,
            ttl_seconds: entry.ttl_seconds,
            body_bytes: entry.body_bytes,
            query_tokens: entry.query_tokens.clone(),
        }
    }
}

impl ManifestEntry {
    /// manifest 条目的内容摘要（不含 body，仅元数据）。
    pub fn digest(&self) -> String {
        let mut digest = Sha256::new();
        for value in [
            self.map_signature.as_str(),
            self.project_id.as_str(),
            self.agent_id.as_str(),
            self.session_id.as_str(),
            self.source_fingerprint.as_str(),
        ] {
            digest.update(value.as_bytes());
            digest.update([0]);
        }
        digest.update(self.recorded_at_ms.to_le_bytes());
        digest.update(self.body_bytes.to_le_bytes());
        format!("{:x}", digest.finalize())
    }
}

/// Recovery Manifest：一次写入事务的条目集合。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveryManifest {
    pub manifest_id: String,
    pub project_id: String,
    pub created_at_ms: u64,
    pub entries: Vec<ManifestEntry>,
}

impl RecoveryManifest {
    pub fn new(
        project_id: impl Into<String>,
        created_at_ms: u64,
        entries: Vec<ManifestEntry>,
    ) -> Self {
        Self {
            manifest_id: format!("manifest-{}", Uuid::new_v4().simple()),
            project_id: project_id.into(),
            created_at_ms,
            entries,
        }
    }

    pub fn is_valid(&self) -> bool {
        validate_scope_identifier("project_id", &self.project_id).is_ok()
            && self
                .entries
                .iter()
                .all(|e| validate_scope_identifier("project_id", &e.project_id).is_ok())
    }
}

/// Recovery Index：指针 ↔ manifest 的解析与恢复。
#[derive(Debug, Clone, Default)]
pub struct RecoveryIndex {
    pointers: Vec<RecoveryPointer>,
    manifests: Vec<RecoveryManifest>,
}

impl RecoveryIndex {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn pointer_count(&self) -> usize {
        self.pointers.len()
    }

    pub fn manifest_count(&self) -> usize {
        self.manifests.len()
    }

    /// 登记 manifest 并返回其指针。
    pub fn record(&mut self, manifest: RecoveryManifest) -> Result<RecoveryPointer, ScopeError> {
        if !manifest.is_valid() {
            return Err(ScopeError(
                "recovery manifest must reference valid scope identifiers".to_string(),
            ));
        }
        let manifest_id = manifest.manifest_id.clone();
        let project_id = manifest.project_id.clone();
        let created_at_ms = manifest.created_at_ms;
        let mut pointers = Vec::with_capacity(manifest.entries.len());
        for entry in &manifest.entries {
            pointers.push(RecoveryPointer {
                pointer: format!("ptr-{}", Uuid::new_v4().simple()),
                manifest_id: manifest_id.clone(),
                entry_digest: entry.digest(),
                created_at_ms: entry.recorded_at_ms,
            });
        }
        self.manifests.push(manifest);
        // 项目级兜底指针：即使 entries 为空也保留可恢复性。
        let fallback = RecoveryPointer {
            pointer: format!("ptr-{}", Uuid::new_v4().simple()),
            manifest_id,
            entry_digest: String::new(),
            created_at_ms,
        };
        if pointers.is_empty() {
            pointers.push(fallback);
        }
        let _ = project_id;
        self.pointers.extend(pointers);
        Ok(self
            .pointers
            .last()
            .cloned()
            .expect("pointer list is non-empty"))
    }

    /// 解析指针 → manifest 条目元数据。
    /// manifest 缺失 / 校验失败 → None（不可解析 → miss）。
    pub fn resolve(&self, pointer: &str) -> Option<&ManifestEntry> {
        let pointer = self.pointers.iter().find(|p| p.pointer == pointer)?;
        let manifest = self
            .manifests
            .iter()
            .find(|m| m.manifest_id == pointer.manifest_id)?;
        manifest
            .entries
            .iter()
            .find(|e| e.digest() == pointer.entry_digest)
    }

    /// 由 manifest 元数据重建 CacheEntry（body 由调用方按需回读，此处保持 None）。
    pub fn rehydrate_entry(manifest_entry: &ManifestEntry) -> CacheEntry {
        CacheEntry {
            map_signature: manifest_entry.map_signature.clone(),
            project_id: manifest_entry.project_id.clone(),
            agent_id: manifest_entry.agent_id.clone(),
            session_id: manifest_entry.session_id.clone(),
            source_fingerprint: manifest_entry.source_fingerprint.clone(),
            recorded_at_ms: manifest_entry.recorded_at_ms,
            ttl_seconds: manifest_entry.ttl_seconds,
            body_bytes: manifest_entry.body_bytes,
            query_tokens: manifest_entry.query_tokens.clone(),
            body: None,
        }
    }

    /// 清理过期/被取代的 manifest 条目（Stale/Superseded），返回移除的指针数。
    pub fn prune(&mut self, now_ms: u64, source_fingerprint: Option<&str>) -> usize {
        let before = self.pointers.len();
        let mut kept_pointers = Vec::with_capacity(self.pointers.len());
        for pointer in self.pointers.drain(..) {
            let keep = self
                .manifests
                .iter()
                .find(|m| m.manifest_id == pointer.manifest_id)
                .and_then(|m| {
                    m.entries
                        .iter()
                        .find(|e| e.digest() == pointer.entry_digest)
                })
                .is_some_and(|e| {
                    let hydrated = Self::rehydrate_entry(e);
                    matches!(
                        hydrated.state_at(now_ms, source_fingerprint),
                        EntryState::Fresh
                    )
                });
            if keep {
                kept_pointers.push(pointer);
            }
        }
        self.pointers = kept_pointers;
        before - self.pointers.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::CacheEntry;
    use crate::signature::{map_signature, source_fingerprint};

    fn entry(project: &str, agent: &str, session: &str, content: &str, at_ms: u64) -> CacheEntry {
        CacheEntry {
            map_signature: map_signature(project, content),
            project_id: project.to_string(),
            agent_id: agent.to_string(),
            session_id: session.to_string(),
            source_fingerprint: source_fingerprint(&"a".repeat(64), None),
            recorded_at_ms: at_ms,
            ttl_seconds: Some(60),
            body_bytes: content.len(),
            query_tokens: vec!["route".to_string()],
            body: Some(format!("candidate:{content}")),
        }
    }

    #[test]
    fn pointer_resolves_to_manifest_entry() {
        let mut index = RecoveryIndex::new();
        let e = entry("proj", "agent-a", "sess-1", "content", 1_000);
        let manifest = RecoveryManifest::new("proj", 1_000, vec![ManifestEntry::from(&e)]);
        let pointer = index.record(manifest).unwrap();
        let resolved = index.resolve(&pointer.pointer).expect("pointer resolves");
        assert_eq!(resolved.map_signature, e.map_signature);
        assert_eq!(resolved.project_id, "proj");
        // 不存在的指针 → None。
        assert!(index.resolve("ptr-does-not-exist").is_none());
    }

    #[test]
    fn missing_manifest_fails_resolution() {
        let mut index = RecoveryIndex::new();
        let e = entry("proj", "agent-a", "sess-1", "content", 1_000);
        let manifest = RecoveryManifest::new("proj", 1_000, vec![ManifestEntry::from(&e)]);
        let pointer = index.record(manifest).unwrap();
        // 模拟 manifest 丢失（清空）。
        index.manifests.clear();
        assert!(index.resolve(&pointer.pointer).is_none());
    }

    #[test]
    fn rehydrate_produces_equivalent_metadata_entry() {
        let e = entry("proj", "agent-a", "sess-1", "content", 1_000);
        let hydrated = RecoveryIndex::rehydrate_entry(&ManifestEntry::from(&e));
        assert_eq!(hydrated.map_signature, e.map_signature);
        assert_eq!(hydrated.source_fingerprint, e.source_fingerprint);
        assert_eq!(hydrated.body_bytes, e.body_bytes);
        // rehydrate 后 body 为空（无 raw 内容随 manifest 恢复）。
        assert!(hydrated.body.is_none());
    }

    #[test]
    fn prune_removes_stale_and_superseded_pointers() {
        let mut index = RecoveryIndex::new();
        let e = entry("proj", "agent-a", "sess-1", "content", 1_000);
        let manifest = RecoveryManifest::new("proj", 1_000, vec![ManifestEntry::from(&e)]);
        index.record(manifest).unwrap();
        assert_eq!(index.pointer_count(), 1);
        // TTL 过期 → prune 移除。
        assert_eq!(index.prune(70_000, None), 1);
        assert_eq!(index.pointer_count(), 0);
        // source 变化 → superseded → prune 移除。
        index
            .record(RecoveryManifest::new(
                "proj",
                1_000,
                vec![ManifestEntry::from(&e)],
            ))
            .unwrap();
        let changed = source_fingerprint(&"a".repeat(64), Some("new-digest"));
        assert_eq!(index.prune(1_000, Some(&changed)), 1);
    }
}
