//! Session Cache — 会话内 map-signature 复用的状态机（九边界之二）。
//!
//! - 条目以 map-signature 为 key（等价内容同签名 → 可复用）；
//! - self-hit 判定：同一 agent 的条目不产生 relay 命中（relay 只跨 agent）；
//! - scope：条目严格绑定 project/agent/session；
//! - size：条目 body 字节上限与缓存总量上限（超限即拒写/淘汰）；
//! - freshness：TTL 过期即 stale，source 变化即 superseded（miss）。

use crate::scope::{validate_scope_identifier, ScopeError};
use crate::policy::is_fresh;

/// 条目可见状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryState {
    Fresh,
    Stale,
    Superseded,
}

/// 一个可复用的 cache/relay 条目。
/// `body` 只允许承载**脱敏后**的候选内容：写入前必须通过 secret-shape gate；
/// 若 gate 拒绝，调用方应丢弃条目而不是降级存储（AGENTS.md §4.6）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheEntry {
    pub map_signature: String,
    pub project_id: String,
    pub agent_id: String,
    pub session_id: String,
    /// 64-hex source revision + 可选 file digest 的 fingerprint。
    pub source_fingerprint: String,
    pub recorded_at_ms: u64,
    pub ttl_seconds: Option<u64>,
    pub body_bytes: usize,
    pub query_tokens: Vec<String>,
    pub body: Option<String>,
}

impl CacheEntry {
    pub fn is_fresh(&self, now_ms: u64) -> bool {
        is_fresh(self.recorded_at_ms, self.ttl_seconds, now_ms)
    }

    pub fn state_at(&self, now_ms: u64, current_source_fingerprint: Option<&str>) -> EntryState {
        if let Some(current) = current_source_fingerprint {
            if current != self.source_fingerprint {
                return EntryState::Superseded;
            }
        }
        if self.is_fresh(now_ms) {
            EntryState::Fresh
        } else {
            EntryState::Stale
        }
    }

    /// self-hit 判定：条目属于同一 agent 且同一 session 时，对同一 agent 是
    /// 自命中（不产生 relay 复用）；跨 agent 才可能 relay。
    pub fn is_self_hit(&self, agent_id: &str, session_id: &str) -> bool {
        self.agent_id == agent_id && self.session_id == session_id
    }

    pub fn is_visible_to(&self, project_id: &str, agent_id: &str) -> bool {
        self.project_id == project_id
            && (self.agent_id == agent_id
                || self.agent_id.is_empty()
                || self.project_id == project_id && !self.agent_id.is_empty())
    }
}

/// Session Cache 配置（size/freshness policy，九边界之 self-hit-size）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CachePolicy {
    /// 单条目 body 字节上限。
    pub max_entry_bytes: usize,
    /// 缓存总字节上限（超出后淘汰最旧条目）。
    pub max_total_bytes: usize,
    /// 会话级 TTL（秒）。
    pub ttl_seconds: Option<u64>,
}

impl Default for CachePolicy {
    fn default() -> Self {
        Self {
            max_entry_bytes: 256 * 1024,
            max_total_bytes: 4 * 1024 * 1024,
            ttl_seconds: Some(3600),
        }
    }
}

/// Session Cache：按 (project, agent, session) 分桶，key 为 map-signature。
#[derive(Debug, Clone, Default)]
pub struct SessionCache {
    entries: Vec<CacheEntry>,
}

impl SessionCache {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// 写入前校验：scope 合法、签名完整、size 合规、内容非 secret-shape。
    pub fn validate_entry(
        entry: &CacheEntry,
        policy: &CachePolicy,
        now_ms: u64,
    ) -> Result<(), ScopeError> {
        validate_scope_identifier("project_id", &entry.project_id)?;
        validate_scope_identifier("agent_id", &entry.agent_id)?;
        if !entry.session_id.is_empty() {
            validate_scope_identifier("session_id", &entry.session_id)?;
        }
        if entry.map_signature.len() != 64 || entry.source_fingerprint.len() != 64 {
            return Err(ScopeError("cache entry signatures must be 64 hex".to_string()));
        }
        if entry.body_bytes > policy.max_entry_bytes {
            return Err(ScopeError("cache entry exceeds max entry bytes".to_string()));
        }
        if let Some(body) = entry.body.as_deref() {
            if crate::signature::is_secret_shaped(body) {
                return Err(ScopeError("cache entry body is secret-shaped".to_string()));
            }
        }
        // 未来时间戳视为非法（时钟回拨防护与 store 一致：拒绝未来写入）。
        if entry.recorded_at_ms > now_ms {
            return Err(ScopeError("cache entry recorded in the future".to_string()));
        }
        Ok(())
    }

    /// 写入条目（幂等：同 map-signature 覆盖）。失败返回 Err 时不做任何变更。
    pub fn put(&mut self, entry: CacheEntry, policy: &CachePolicy, now_ms: u64) -> Result<(), ScopeError> {
        Self::validate_entry(&entry, policy, now_ms)?;
        if let Some(existing) = self
            .entries
            .iter_mut()
            .find(|e| e.map_signature == entry.map_signature)
        {
            *existing = entry;
        } else {
            self.entries.push(entry);
        }
        self.enforce_total_budget(policy);
        Ok(())
    }

    fn enforce_total_budget(&mut self, policy: &CachePolicy) {
        let mut total: usize = self.entries.iter().map(|e| e.body_bytes).sum();
        // 最旧优先淘汰，直到满足总预算。
        while total > policy.max_total_bytes && self.entries.len() > 1 {
            if let Some((index, _)) = self
                .entries
                .iter()
                .enumerate()
                .min_by_key(|(_, e)| e.recorded_at_ms)
            {
                total = total.saturating_sub(self.entries[index].body_bytes);
                self.entries.remove(index);
            } else {
                break;
            }
        }
    }

    /// 会话内 map-signature 查询：命中 fresh 条目返回候选 body。
    /// `self_hit` 为 true 时拒绝命中（自命中=0）。
    pub fn get(
        &self,
        project_id: &str,
        agent_id: &str,
        session_id: &str,
        map_signature: &str,
        source_fingerprint: &str,
        now_ms: u64,
        allow_self_hit: bool,
    ) -> Option<&CacheEntry> {
        self.entries.iter().find(|e| {
            e.project_id == project_id
                && e.session_id == session_id
                && e.map_signature == map_signature
                && e.state_at(now_ms, Some(source_fingerprint)) == EntryState::Fresh
                && (allow_self_hit || !e.is_self_hit(agent_id, session_id))
        })
    }

    /// 清理过期条目，返回移除数量。
    pub fn prune_expired(&mut self, now_ms: u64, current_source_fingerprint: Option<&str>) -> usize {
        let before = self.entries.len();
        self.entries.retain(|e| {
            matches!(e.state_at(now_ms, current_source_fingerprint), EntryState::Fresh)
        });
        before - self.entries.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::signature::{map_signature, source_fingerprint};

    fn entry(
        project: &str,
        agent: &str,
        session: &str,
        content: &str,
        at_ms: u64,
    ) -> CacheEntry {
        CacheEntry {
            map_signature: map_signature(project, content),
            project_id: project.to_string(),
            agent_id: agent.to_string(),
            session_id: session.to_string(),
            source_fingerprint: fp(),
            recorded_at_ms: at_ms,
            ttl_seconds: Some(60),
            body_bytes: content.len(),
            query_tokens: vec!["route".to_string()],
            body: Some(format!("candidate:{content}")),
        }
    }

    /// 与 `entry` helper 写入的 fingerprint 一致的当前 source fingerprint。
    fn fp() -> String {
        source_fingerprint(&"a".repeat(64), None)
    }

    #[test]
    fn session_cache_put_and_same_session_hit() {
        let policy = CachePolicy::default();
        let mut cache = SessionCache::new();
        let at = 1_000;
        cache.put(entry("proj", "agent-a", "sess-1", "hello", at), &policy, at).unwrap();
        // 同 session 同签名 → 命中（允许 self-hit：session cache 本会话复用）。
        let hit = cache
            .get("proj", "agent-a", "sess-1", &entry("proj", "agent-a", "sess-1", "hello", at).map_signature, &fp(), 1_000, true)
            .expect("same-session hit expected");
        assert_eq!(hit.body.as_deref(), Some("candidate:hello"));
    }

    #[test]
    fn self_hit_is_zero_for_relay_path() {
        let policy = CachePolicy::default();
        let mut cache = SessionCache::new();
        let at = 1_000;
        let e = entry("proj", "agent-a", "sess-1", "shared", at);
        let signature = e.map_signature.clone();
        cache.put(e, &policy, at).unwrap();
        // 同一 agent 查询（allow_self_hit=false，relay 路径）→ 不命中。
        assert!(cache
            .get("proj", "agent-a", "sess-1", &signature, &fp(), 1_000, false)
            .is_none());
        // 不同 agent（跨 agent relay）→ 命中。
        assert!(cache
            .get("proj", "agent-b", "sess-1", &signature, &fp(), 1_000, false)
            .is_some());
    }

    #[test]
    fn cross_project_pollution_is_zero() {
        let policy = CachePolicy::default();
        let mut cache = SessionCache::new();
        let at = 1_000;
        let e = entry("proj-a", "agent-a", "sess-1", "same content", at);
        let signature = e.map_signature.clone();
        cache.put(e, &policy, at).unwrap();
        // 不同项目即使内容/签名相同也查不到；签名本身含 project，双保险。
        assert!(cache
            .get("proj-b", "agent-b", "sess-1", &signature, &fp(), 1_000, true)
            .is_none());
    }

    #[test]
    fn ttl_and_source_change_make_entry_miss() {
        let policy = CachePolicy::default();
        let mut cache = SessionCache::new();
        let at = 1_000;
        let e = entry("proj", "agent-a", "sess-1", "content", at);
        let signature = e.map_signature.clone();
        cache.put(e, &policy, at).unwrap();
        // TTL 过期 → miss。
        assert!(cache
            .get("proj", "agent-a", "sess-1", &signature, &fp(), 70_000, true)
            .is_none());
        // source 变化（file digest 变化）→ superseded → miss。
        let changed_fingerprint = source_fingerprint(&"a".repeat(64), Some("new-file-digest"));
        assert!(cache
            .get("proj", "agent-a", "sess-1", &signature, &changed_fingerprint, 1_000, true)
            .is_none());
        // prune 清理过期。
        cache.put(entry("proj", "agent-a", "sess-1", "content", at), &policy, at).unwrap();
        assert_eq!(cache.prune_expired(70_000, Some(&fp())), 1);
        assert!(cache.is_empty());
    }

    #[test]
    fn size_limits_reject_oversized_and_evict_oldest() {
        let mut policy = CachePolicy::default();
        policy.max_entry_bytes = 100;
        policy.max_total_bytes = 150;
        let mut cache = SessionCache::new();
        let at = 1_000;
        let big = entry("proj", "agent-a", "sess-1", &"x".repeat(200), at);
        // 单条目超限 → 拒写。
        assert!(cache.put(big, &policy, at).is_err());
        // 两条各 90 字节，合计 180 > 150 → 淘汰最旧的一条。
        let e1 = entry("proj", "agent-a", "sess-1", &"x".repeat(90), at);
        let e2 = entry("proj", "agent-a", "sess-1", &"y".repeat(90), at + 1);
        cache.put(e1, &policy, at).unwrap();
        cache.put(e2, &policy, at + 1).unwrap();
        // 总量预算 150 < 两条之和 → 淘汰最旧。
        assert_eq!(cache.len(), 1);
        assert!(cache
            .get("proj", "agent-a", "sess-1", &entry("proj", "agent-a", "sess-1", &"x".repeat(90), at).map_signature, &fp(), at, true)
            .is_none());
    }

    #[test]
    fn secret_shaped_body_is_rejected() {
        let policy = CachePolicy::default();
        let mut cache = SessionCache::new();
        let at = 1_000;
        let mut bad = entry("proj", "agent-a", "sess-1", "leak", at);
        bad.body = Some("Authorization: Bearer sk-abc123".to_string());
        assert!(cache.put(bad, &policy, at).is_err());
        assert!(cache.is_empty());
    }
}
