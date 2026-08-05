//! d-token-owned Context Bus / Relay.
//!
//! The relay is a bounded, scoped cache that lets one Agent's prepared
//! context be reused by another Agent within the same project/session,
//! with explicit provenance and fail-closed freshness.  It is deliberately
//! NOT a general-purpose cache: every stored entry must carry a scope
//! (project + session partition, agent identity), a producer identity, a
//! source revision, a content digest and a bounded size.
//!
//! Behavior contract (fail-closed):
//!
//! 1. **scope** — relay entries are partitioned by `(project_id,
//!    session_id)` only.  `agent_id` is the caller's declared identity for
//!    provenance, self-hit and identity checks, never a relay partition
//!    dimension: different Agents in the same project/session share one
//!    partition, while a different session or project stays fully isolated.
//!    The plain session cache remains partitioned per `(project_id,
//!    agent_id, session_id)`.
//! 2. **freshness / provenance** — every entry records producer agent,
//!    source revision, content digest and observed-at time; expired entries
//!    are never served.
//! 3. **same-Agent self-hit protection + identity validation** — a relay
//!    lookup validates that `requester_agent` matches the declared
//!    `scope.agent_id` and never serves an entry back to the Agent that
//!    produced it; a self-hit is a miss so the caller falls back to a
//!    baseline instead of re-consuming its own relayed context.
//! 4. **content size bounds** — entries larger than the per-entry cap are
//!    rejected; the bus enforces per-partition entry and total-byte quotas.
//! 5. **plain Session Cache isolation** — the relay namespace is disjoint
//!    from the plain session-cache namespace; content never leaks between
//!    them and the relay never serves plain session-cache data.
//! 6. **file-change invalidation** — entries bound to observed file states
//!    are only served when the current file state matches; an unverifiable
//!    state is a miss, never a blind hit.
//! 7. **relay failure fallback** — `relay_or_fallback` never propagates an
//!    error to the caller; internal failures degrade to an explicit
//!    fallback so the caller always has a baseline path.
//!
//! This module is provenance boundary `dtoken-context-bus-v1`: it is a
//! d-token-owned implementation and does not import external LeanCTX code.
//! Any future upstream-derived behavior must record source/license/commit
//! before it is enabled here.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::UNIX_EPOCH;

use sha2::{Digest, Sha256};

/// Evidence revision for the whole Context Bus implementation boundary.
pub const CONTEXT_BUS_EVIDENCE_REVISION: &str = "dtoken-context-bus-v1";

/// Hard per-entry content cap (bytes).  Anything larger is rejected, never
/// truncated or spilled.
pub const MAX_RELAY_ENTRY_BYTES: usize = 256 * 1024;

/// Maximum entries retained per (project, session) relay partition.
pub const MAX_RELAY_ENTRIES_PER_SCOPE: usize = 128;

/// Total relayed bytes across all partitions.
pub const MAX_RELAY_TOTAL_BYTES: usize = 16 * 1024 * 1024;

/// Default relay freshness window (milliseconds).
pub const DEFAULT_RELAY_TTL_MS: u64 = 30 * 60 * 1000;

/// Per-entry cap for the plain session cache.
pub const MAX_SESSION_CACHE_ENTRY_BYTES: usize = 256 * 1024;

/// Total session-cache bytes across all partitions.
pub const MAX_SESSION_CACHE_TOTAL_BYTES: usize = 16 * 1024 * 1024;

/// Default session-cache TTL (milliseconds).
pub const DEFAULT_SESSION_CACHE_TTL_MS: u64 = 10 * 60 * 1000;

/// A cache scope: project + agent identity + session.  The relay partitions
/// by (project, session); `agent_id` is the caller's declared identity used
/// for provenance and self-hit/identity checks.  The plain session cache
/// partitions by (project, agent, session).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ContextBusScope {
    pub project_id: String,
    pub agent_id: String,
    pub session_id: Option<String>,
}

impl ContextBusScope {
    pub fn new(
        project_id: impl Into<String>,
        agent_id: impl Into<String>,
        session_id: Option<String>,
    ) -> Result<Self, ContextBusError> {
        let scope = Self {
            project_id: project_id.into(),
            agent_id: agent_id.into(),
            session_id: session_id.filter(|value| !value.trim().is_empty()),
        };
        scope.validate()?;
        Ok(scope)
    }

    fn validate(&self) -> Result<(), ContextBusError> {
        if self.project_id.trim().is_empty() {
            return Err(ContextBusError::InvalidScope(
                "project_id must be non-empty".to_string(),
            ));
        }
        if self.agent_id.trim().is_empty() {
            return Err(ContextBusError::InvalidScope(
                "agent_id must be non-empty".to_string(),
            ));
        }
        if self
            .session_id
            .as_deref()
            .is_some_and(|value| value.trim().is_empty())
        {
            return Err(ContextBusError::InvalidScope(
                "session_id must be non-empty when present".to_string(),
            ));
        }
        Ok(())
    }

    /// Typed relay partition: (project, session).  Deliberately excludes
    /// `agent_id`: the relay is shared across Agents within the same
    /// project/session, so `agent_id` participates in identity validation
    /// but never in partitioning.  Typed keys (instead of string
    /// concatenation with ':' / '*' separators) make cross-scope collisions
    /// impossible even when user-controlled ids contain those characters.
    fn relay_partition(&self) -> RelayPartitionKey {
        RelayPartitionKey {
            project_id: self.project_id.clone(),
            session_id: self.session_id.clone(),
        }
    }

    /// Full typed key of a relay entry for this scope.
    fn relay_entry_key(&self, key: &str) -> RelayEntryKey {
        RelayEntryKey {
            partition: self.relay_partition(),
            key: key.to_string(),
        }
    }

    /// Typed partition for the plain agent-local session cache.  Unlike the
    /// relay, this stays partitioned per (project, agent, session): it is an
    /// ordinary agent-local cache with no cross-agent sharing.
    fn session_partition(&self) -> SessionPartitionKey {
        SessionPartitionKey {
            project_id: self.project_id.clone(),
            agent_id: self.agent_id.clone(),
            session_id: self.session_id.clone(),
        }
    }

    /// Full typed key of a session-cache entry for this scope.
    fn session_entry_key(&self, key: &str) -> SessionEntryKey {
        SessionEntryKey {
            partition: self.session_partition(),
            key: key.to_string(),
        }
    }
}

/// Typed relay partition key (project, session).  Structurally distinct
/// values, so user-controlled ids containing ':' or '*' can never alias each
/// other the way a string-concatenated namespace could.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct RelayPartitionKey {
    project_id: String,
    session_id: Option<String>,
}

/// Typed session-cache partition key (project, agent, session).  Agent-local,
/// so unlike the relay partition it always includes the agent identity.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct SessionPartitionKey {
    project_id: String,
    agent_id: String,
    session_id: Option<String>,
}

/// Full typed key of a stored relay entry: partition + entry key.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct RelayEntryKey {
    partition: RelayPartitionKey,
    key: String,
}

/// Full typed key of a stored session-cache entry: partition + entry key.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct SessionEntryKey {
    partition: SessionPartitionKey,
    key: String,
}

/// An observed source-file state used for file-change invalidation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservedFileState {
    pub path: String,
    pub size: u64,
    pub modified_ms: u64,
}

impl ObservedFileState {
    pub fn new(
        path: impl Into<String>,
        size: u64,
        modified_ms: u64,
    ) -> Result<Self, ContextBusError> {
        let state = Self {
            path: path.into(),
            size,
            modified_ms,
        };
        if state.path.trim().is_empty() {
            return Err(ContextBusError::InvalidFileState(
                "path must be non-empty".to_string(),
            ));
        }
        Ok(state)
    }
}

/// Failure modes of the Context Bus.  Store-level APIs return these as
/// errors; the runtime-facing `relay_or_fallback` converts every one of
/// them into an explicit fallback instead of propagating.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContextBusError {
    InvalidScope(String),
    InvalidKey(String),
    InvalidFileState(String),
    EmptyContent,
    OversizeEntry,
    ScopeEntryLimitExceeded,
    TotalBytesExceeded,
    LockPoisoned,
}

impl std::fmt::Display for ContextBusError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidScope(detail) => write!(formatter, "invalid context bus scope: {detail}"),
            Self::InvalidKey(detail) => write!(formatter, "invalid context bus key: {detail}"),
            Self::InvalidFileState(detail) => {
                write!(formatter, "invalid observed file state: {detail}")
            }
            Self::EmptyContent => write!(formatter, "context bus content is empty"),
            Self::OversizeEntry => write!(formatter, "context bus entry exceeds the size cap"),
            Self::ScopeEntryLimitExceeded => {
                write!(formatter, "context bus partition entry limit exceeded")
            }
            Self::TotalBytesExceeded => write!(formatter, "context bus total byte quota exceeded"),
            Self::LockPoisoned => write!(formatter, "context bus internal lock poisoned"),
        }
    }
}

impl std::error::Error for ContextBusError {}

/// Why a relay lookup did not produce a served hit.  Every miss reason is a
/// fallback reason for the caller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RelayMissReason {
    /// No entry exists for the requested scope + key.
    NotPresent,
    /// The entry exists but its freshness window has passed.
    Expired,
    /// The entry was produced by the requesting Agent itself.
    SelfHit,
    /// The entry is bound to file states that no longer match the current
    /// observed state.
    FileChanged,
    /// The entry is bound to file states but the caller supplied none, so
    /// freshness cannot be verified.
    FileStateUnverifiable,
}

impl RelayMissReason {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::NotPresent => "relay_miss_not_present",
            Self::Expired => "relay_miss_expired",
            Self::SelfHit => "relay_miss_self_hit",
            Self::FileChanged => "relay_miss_file_changed",
            Self::FileStateUnverifiable => "relay_miss_file_state_unverifiable",
        }
    }
}

/// In-memory store receipt: the entry has been accepted into the bounded
/// in-memory relay.  This is NOT a durable/persisted receipt — the relay is
/// process-local and content never survives a restart — but it lets the
/// producer correlate the accepted write (digest, expiry) with later hits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelayStoreReceipt {
    pub key: String,
    pub producer_agent: String,
    pub produced_at_ms: u64,
    pub expires_at_ms: u64,
    pub stored_bytes: usize,
    /// Hex SHA-256 of the stored content, so the producer can correlate and
    /// later verify what the relay actually holds.
    pub content_digest: String,
}

/// Receipt returned on a served relay hit.  `avoided_transfer_bytes` is the
/// actual number of content bytes the requester did not have to transmit
/// because the relay served them — measured, not estimated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelayHitReceipt {
    pub key: String,
    pub producer_agent: String,
    pub produced_at_ms: u64,
    pub expires_at_ms: u64,
    pub source_revision: String,
    /// Hex SHA-256 of the stored content, so the requester can verify that
    /// the relayed content matches the recorded provenance (producer,
    /// revision).
    pub content_digest: String,
    pub served_at_ms: u64,
    pub avoided_transfer_bytes: usize,
}

/// The explicit basis on which a measured relay saving may become claimable.
/// The Context Bus store itself has no Provider usage or billing attribution
/// and can never claim savings on its own; only an upstream contract that
/// ties a served hit to the requesting Provider's usage record may upgrade
/// claimability.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RelayAttributionBasis {
    /// An upstream attribution contract tied the served hit to the
    /// requesting Provider's usage / chargeable record.
    ProviderUsageAttributed,
}

/// Delivery receipt that keeps the two saving kinds distinct.
///
/// The relay avoids **duplicate transmission**: the requester did not have to
/// re-send bytes the relay already holds.  Compression saves **tokens/bytes
/// inside a transmitted payload** — a different mechanism, produced by the
/// codec pipeline, never by the relay.  `compression_saved_bytes` is carried
/// here only so one evidence record can state both numbers without conflating
/// them; the relay itself always reports zero compression savings.
///
/// `estimated` / `actual` / `claimable` are strictly separated:
/// - `estimated` may be recorded pre-send and is never claimable;
/// - `actual` is the measured value after the relay served the entry;
/// - `claimable` defaults to `false` even for an actually-served hit with a
///   measured `relay_avoided_transfer_bytes`.  The store has no Provider
///   usage/billing attribution, so claimability becomes `true` only through
///   an explicit upstream attribution contract
///   (`with_provider_usage_attribution`); estimates, fallbacks and
///   compression-only numbers are never claimable, and compression savings
///   are never double-counted into relay savings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelayDeliveryReceipt {
    pub served: bool,
    pub relay_avoided_transfer_bytes: Option<u64>,
    pub compression_saved_bytes: Option<u64>,
    pub estimated_relay_avoided_transfer_bytes: Option<u64>,
    /// `true` only after an explicit upstream attribution contract upgraded
    /// the measured saving.  The store never sets this by itself.
    pub claimable: bool,
    /// The basis for a claimable saving; always `None` while `claimable` is
    /// `false`.
    pub attribution_basis: Option<RelayAttributionBasis>,
}

impl RelayDeliveryReceipt {
    /// Receipt for a relay hit that actually served content.  The avoided
    /// transfer is measured but NOT yet claimable: the store has no Provider
    /// usage/billing attribution, so only an explicit upstream attribution
    /// contract (`with_provider_usage_attribution`) can upgrade it.
    /// Compression is always reported separately and never adds to relay
    /// claimability.
    pub fn served(relay_avoided_transfer_bytes: u64) -> Self {
        Self {
            served: true,
            relay_avoided_transfer_bytes: Some(relay_avoided_transfer_bytes),
            compression_saved_bytes: None,
            estimated_relay_avoided_transfer_bytes: None,
            claimable: false,
            attribution_basis: None,
        }
    }

    /// Receipt for a fallback (miss / unavailable).  Nothing was served, so
    /// nothing is claimable and no actual transfer was avoided.
    pub fn fallback() -> Self {
        Self {
            served: false,
            relay_avoided_transfer_bytes: None,
            compression_saved_bytes: None,
            estimated_relay_avoided_transfer_bytes: None,
            claimable: false,
            attribution_basis: None,
        }
    }

    /// Attach a pre-send estimate.  The estimate is an independent
    /// observability field and never changes claimability: claimable is
    /// decided only by the measured actual served transfer, so the estimate
    /// can never be reported as relay savings.
    pub fn with_estimated_relay_avoided_transfer(mut self, estimated_bytes: u64) -> Self {
        self.estimated_relay_avoided_transfer_bytes = Some(estimated_bytes);
        self
    }

    /// Record codec compression savings for the same request.  This is a
    /// separate mechanism and must never be reported as relay-avoided
    /// transmission.
    pub fn with_compression_saved_bytes(mut self, saved_bytes: u64) -> Self {
        self.compression_saved_bytes = Some(saved_bytes);
        self
    }

    /// Explicitly attribute the served hit to the requesting Provider's usage
    /// record.  This is the ONLY way claimability may become `true`: the
    /// store itself has no Provider usage/billing attribution, so a measured
    /// saving stays unclaimable until an upstream contract calls this.  A
    /// non-served receipt (fallback) is never upgraded.
    pub fn with_provider_usage_attribution(mut self, basis: RelayAttributionBasis) -> Self {
        if self.served {
            self.claimable = true;
            self.attribution_basis = Some(basis);
        }
        self
    }

    /// The savings that may honestly be claimed as relay benefit: only the
    /// measured actual transfer avoided by a served hit that an explicit
    /// upstream attribution contract marked claimable.  Estimates,
    /// compression savings and fallbacks never contribute, and a measured
    /// but unattributed hit is never claimable.
    pub fn claimable_relay_savings(&self) -> Option<u64> {
        (self.served && self.claimable && self.attribution_basis.is_some())
            .then_some(self.relay_avoided_transfer_bytes)
            .flatten()
    }

    pub fn is_valid(&self) -> bool {
        if self.claimable && !self.served {
            return false;
        }
        if self.claimable && self.relay_avoided_transfer_bytes.is_none() {
            return false;
        }
        // Claimable without an explicit attribution basis is a lie.
        if self.claimable && self.attribution_basis.is_none() {
            return false;
        }
        if self.attribution_basis.is_some() && !self.claimable {
            return false;
        }
        if self.served && self.relay_avoided_transfer_bytes.is_none() {
            return false;
        }
        if !self.served && self.relay_avoided_transfer_bytes.is_some() {
            return false;
        }
        true
    }
}

/// A served relay hit: the actual payload the requester can reuse, plus the
/// receipt proving provenance.  The payload is redacted from `Debug` output,
/// never logged and never emitted by `summary()`; only the receipt metadata
/// is observable.
#[derive(Clone, PartialEq, Eq)]
pub struct RelayResolved {
    pub payload: Vec<u8>,
    pub receipt: RelayHitReceipt,
}

impl std::fmt::Debug for RelayResolved {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RelayResolved")
            .field("payload_bytes", &self.payload.len())
            .field("receipt", &self.receipt)
            .finish()
    }
}

/// Result of a store-level relay lookup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RelayLookup {
    Hit(RelayResolved),
    Miss(RelayMissReason),
}

/// Runtime-facing outcome: either the relay served the entry (with the
/// payload) or the caller must fall back to the baseline path.  `Unavailable`
/// is produced by internal failures and by `relay_or_fallback`; it is never
/// an error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RelayOutcome {
    Served(RelayResolved),
    Fallback(RelayMissReason),
    Unavailable,
}

impl RelayOutcome {
    pub fn is_served(&self) -> bool {
        matches!(self, Self::Served(_))
    }
}

#[derive(Clone, PartialEq, Eq)]
struct RelayEntry {
    partition: RelayPartitionKey,
    key: String,
    digest: [u8; 32],
    content: Vec<u8>,
    producer_agent: String,
    source_revision: String,
    produced_at_ms: u64,
    expires_at_ms: u64,
    observed_files: Vec<ObservedFileState>,
}

impl std::fmt::Debug for RelayEntry {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The stored payload must never appear in Debug output.
        formatter
            .debug_struct("RelayEntry")
            .field("partition", &self.partition)
            .field("key", &self.key)
            .field("content_bytes", &self.content.len())
            .field("digest", &hex_digest(self.digest))
            .field("producer_agent", &self.producer_agent)
            .field("source_revision", &self.source_revision)
            .field("produced_at_ms", &self.produced_at_ms)
            .field("expires_at_ms", &self.expires_at_ms)
            .field("observed_files", &self.observed_files)
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SessionCacheEntry {
    partition: SessionPartitionKey,
    content_bytes: usize,
    expires_at_ms: u64,
}

#[derive(Default)]
struct Inner {
    relay_entries: HashMap<RelayEntryKey, RelayEntry>,
    session_entries: HashMap<SessionEntryKey, SessionCacheEntry>,
    relay_total_bytes: usize,
    session_total_bytes: usize,
}

/// The Context Bus / Relay.  In-memory, bounded, mutex-protected.  It is a
/// runtime-side cache: content is never persisted, never leaves the process
/// and never appears in logs.
#[derive(Default)]
pub struct ContextBus {
    inner: Mutex<Inner>,
}

impl ContextBus {
    pub fn new() -> Self {
        Self::default()
    }

    /// Store a relayed context entry under `scope` with explicit producer
    /// provenance.  Rejects empty/oversize content, enforces the per-scope
    /// entry cap and the total byte quota, and returns an in-memory store
    /// receipt (accepted into the process-local relay, not durable).
    #[allow(clippy::too_many_arguments)]
    pub fn relay_store(
        &self,
        scope: &ContextBusScope,
        key: &str,
        content: &[u8],
        producer_agent: &str,
        source_revision: &str,
        observed_files: Vec<ObservedFileState>,
        now_ms: u64,
    ) -> Result<RelayStoreReceipt, ContextBusError> {
        self.relay_store_with_ttl(
            scope,
            key,
            content,
            producer_agent,
            source_revision,
            observed_files,
            now_ms,
            DEFAULT_RELAY_TTL_MS,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn relay_store_with_ttl(
        &self,
        scope: &ContextBusScope,
        key: &str,
        content: &[u8],
        producer_agent: &str,
        source_revision: &str,
        observed_files: Vec<ObservedFileState>,
        now_ms: u64,
        ttl_ms: u64,
    ) -> Result<RelayStoreReceipt, ContextBusError> {
        scope.validate()?;
        if key.trim().is_empty() {
            return Err(ContextBusError::InvalidKey(
                "key must be non-empty".to_string(),
            ));
        }
        // Identity consistency, not a partition: the writer declares its own
        // identity in the scope and must record itself as the producer.  The
        // relay namespace never contains agent_id, so other Agents can still
        // read the entry afterwards.
        if producer_agent.trim().is_empty() || producer_agent != scope.agent_id {
            return Err(ContextBusError::InvalidScope(
                "producer_agent must match the scope agent identity".to_string(),
            ));
        }
        if source_revision.trim().is_empty() {
            return Err(ContextBusError::InvalidKey(
                "source_revision must be non-empty".to_string(),
            ));
        }
        if content.is_empty() {
            return Err(ContextBusError::EmptyContent);
        }
        if content.len() > MAX_RELAY_ENTRY_BYTES {
            return Err(ContextBusError::OversizeEntry);
        }
        let entry_key = scope.relay_entry_key(key);
        let expires_at_ms = now_ms.saturating_add(ttl_ms.max(1));
        let digest = Sha256::digest(content);

        let mut inner = self
            .inner
            .lock()
            .map_err(|_| ContextBusError::LockPoisoned)?;
        // Partition-level cap: reject rather than evict so a full partition
        // is an observable boundary instead of silently dropping old entries.
        let partition_entries = inner
            .relay_entries
            .values()
            .filter(|entry| entry.partition == entry_key.partition)
            .count();
        if partition_entries >= MAX_RELAY_ENTRIES_PER_SCOPE {
            return Err(ContextBusError::ScopeEntryLimitExceeded);
        }
        let next_total = inner
            .relay_total_bytes
            .saturating_add(content.len())
            .saturating_sub(
                inner
                    .relay_entries
                    .get(&entry_key)
                    .map(|entry| entry.content.len())
                    .unwrap_or(0),
            );
        if next_total > MAX_RELAY_TOTAL_BYTES {
            return Err(ContextBusError::TotalBytesExceeded);
        }

        let entry = RelayEntry {
            partition: entry_key.partition.clone(),
            key: key.to_string(),
            digest: digest.into(),
            content: content.to_vec(),
            producer_agent: producer_agent.to_string(),
            source_revision: source_revision.to_string(),
            produced_at_ms: now_ms,
            expires_at_ms,
            observed_files,
        };
        inner.relay_entries.insert(entry_key, entry);
        inner.relay_total_bytes = next_total;
        Ok(RelayStoreReceipt {
            key: key.to_string(),
            producer_agent: producer_agent.to_string(),
            produced_at_ms: now_ms,
            expires_at_ms,
            stored_bytes: content.len(),
            content_digest: hex_digest(digest),
        })
    }

    /// Store-level lookup.  Returns `Hit` (with the stored payload and its
    /// receipt) only when the entry exists, is fresh, was produced by a
    /// different Agent and its observed file state still matches (or it is
    /// not file-bound).  Any other case is an explicit `Miss` reason;
    /// internal failures are errors here and become `Unavailable` through
    /// `relay_or_fallback`.
    pub fn relay_lookup(
        &self,
        scope: &ContextBusScope,
        key: &str,
        requester_agent: &str,
        current_files: Option<&[ObservedFileState]>,
        now_ms: u64,
    ) -> Result<RelayLookup, ContextBusError> {
        scope.validate()?;
        if key.trim().is_empty() {
            return Err(ContextBusError::InvalidKey(
                "key must be non-empty".to_string(),
            ));
        }
        // Identity validation, fail-closed: the requester must declare its
        // own identity consistently in the scope and `requester_agent`.  A
        // mismatch is a programming error, not a miss — it surfaces as an
        // error here and as `Unavailable` through `relay_or_fallback`.
        if requester_agent.trim().is_empty() || requester_agent != scope.agent_id {
            return Err(ContextBusError::InvalidScope(
                "requester_agent must match the scope agent identity".to_string(),
            ));
        }
        let entry_key = scope.relay_entry_key(key);
        let inner = self
            .inner
            .lock()
            .map_err(|_| ContextBusError::LockPoisoned)?;
        let Some(entry) = inner.relay_entries.get(&entry_key) else {
            return Ok(RelayLookup::Miss(RelayMissReason::NotPresent));
        };
        if now_ms > entry.expires_at_ms {
            return Ok(RelayLookup::Miss(RelayMissReason::Expired));
        }
        if entry.producer_agent == requester_agent {
            return Ok(RelayLookup::Miss(RelayMissReason::SelfHit));
        }
        if !entry.observed_files.is_empty() {
            let Some(current) = current_files else {
                return Ok(RelayLookup::Miss(RelayMissReason::FileStateUnverifiable));
            };
            for observed in &entry.observed_files {
                let matches = current.iter().any(|candidate| {
                    candidate.path == observed.path
                        && candidate.size == observed.size
                        && candidate.modified_ms == observed.modified_ms
                });
                if !matches {
                    return Ok(RelayLookup::Miss(RelayMissReason::FileChanged));
                }
            }
        }
        Ok(RelayLookup::Hit(RelayResolved {
            payload: entry.content.clone(),
            receipt: RelayHitReceipt {
                key: key.to_string(),
                producer_agent: entry.producer_agent.clone(),
                produced_at_ms: entry.produced_at_ms,
                expires_at_ms: entry.expires_at_ms,
                source_revision: entry.source_revision.clone(),
                content_digest: hex_digest(entry.digest),
                served_at_ms: now_ms,
                avoided_transfer_bytes: entry.content.len(),
            },
        }))
    }

    /// Runtime-facing relay decision with failure fallback.  Never returns
    /// an error: every miss reason and every internal failure maps to a
    /// `Fallback` / `Unavailable` outcome so the caller always has a
    /// baseline path.
    pub fn relay_or_fallback(
        &self,
        scope: &ContextBusScope,
        key: &str,
        requester_agent: &str,
        current_files: Option<&[ObservedFileState]>,
        now_ms: u64,
    ) -> RelayOutcome {
        match self.relay_lookup(scope, key, requester_agent, current_files, now_ms) {
            Ok(RelayLookup::Hit(resolved)) => RelayOutcome::Served(resolved),
            Ok(RelayLookup::Miss(reason)) => RelayOutcome::Fallback(reason),
            Err(_) => RelayOutcome::Unavailable,
        }
    }

    /// Plain per-session cache, isolated from the relay namespace.  No
    /// provenance, no self-hit protection, no file binding: this is the
    /// ordinary agent-local session cache, and the relay never reads it.
    pub fn session_cache_put(
        &self,
        scope: &ContextBusScope,
        key: &str,
        content: &[u8],
        now_ms: u64,
    ) -> Result<(), ContextBusError> {
        scope.validate()?;
        if key.trim().is_empty() {
            return Err(ContextBusError::InvalidKey(
                "key must be non-empty".to_string(),
            ));
        }
        if content.is_empty() {
            return Err(ContextBusError::EmptyContent);
        }
        if content.len() > MAX_SESSION_CACHE_ENTRY_BYTES {
            return Err(ContextBusError::OversizeEntry);
        }
        let entry_key = scope.session_entry_key(key);
        let expires_at_ms = now_ms.saturating_add(DEFAULT_SESSION_CACHE_TTL_MS);
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| ContextBusError::LockPoisoned)?;
        let next_total = inner
            .session_total_bytes
            .saturating_add(content.len())
            .saturating_sub(
                inner
                    .session_entries
                    .get(&entry_key)
                    .map(|entry| entry.content_bytes)
                    .unwrap_or(0),
            );
        if next_total > MAX_SESSION_CACHE_TOTAL_BYTES {
            return Err(ContextBusError::TotalBytesExceeded);
        }
        inner.session_entries.insert(
            entry_key,
            SessionCacheEntry {
                partition: scope.session_partition(),
                content_bytes: content.len(),
                expires_at_ms,
            },
        );
        inner.session_total_bytes = next_total;
        Ok(())
    }

    pub fn session_cache_get(
        &self,
        scope: &ContextBusScope,
        key: &str,
        now_ms: u64,
    ) -> Result<bool, ContextBusError> {
        scope.validate()?;
        if key.trim().is_empty() {
            return Err(ContextBusError::InvalidKey(
                "key must be non-empty".to_string(),
            ));
        }
        let inner = self
            .inner
            .lock()
            .map_err(|_| ContextBusError::LockPoisoned)?;
        let Some(entry) = inner.session_entries.get(&scope.session_entry_key(key)) else {
            return Ok(false);
        };
        Ok(now_ms <= entry.expires_at_ms)
    }

    /// Bounded summary for observability/evidence.  Never exposes content.
    pub fn summary(&self) -> Result<ContextBusSummary, ContextBusError> {
        let inner = self
            .inner
            .lock()
            .map_err(|_| ContextBusError::LockPoisoned)?;
        let mut relay_expired = 0_usize;
        let now_ms = now_ms();
        for entry in inner.relay_entries.values() {
            if now_ms > entry.expires_at_ms {
                relay_expired += 1;
            }
        }
        Ok(ContextBusSummary {
            relay_entries: inner.relay_entries.len(),
            relay_expired,
            relay_bytes: inner.relay_total_bytes,
            session_entries: inner.session_entries.len(),
            session_bytes: inner.session_total_bytes,
            evidence_revision: CONTEXT_BUS_EVIDENCE_REVISION,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextBusSummary {
    pub relay_entries: usize,
    pub relay_expired: usize,
    pub relay_bytes: usize,
    pub session_entries: usize,
    pub session_bytes: usize,
    pub evidence_revision: &'static str,
}

/// Hex-encode a SHA-256 digest for receipts (stable, dependency-free).
fn hex_digest(digest: impl AsRef<[u8]>) -> String {
    use std::fmt::Write as _;
    let mut out = String::with_capacity(64);
    for byte in digest.as_ref() {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scope(project: &str, agent: &str, session: Option<&str>) -> ContextBusScope {
        ContextBusScope::new(project, agent, session.map(str::to_string)).expect("scope")
    }

    fn file(path: &str, size: u64, modified_ms: u64) -> ObservedFileState {
        ObservedFileState::new(path, size, modified_ms).expect("file state")
    }

    #[test]
    fn relay_is_project_and_session_scoped_across_agents() {
        let bus = ContextBus::new();
        bus.relay_store(
            &scope("p1", "agent-a", Some("s1")),
            "worktree",
            b"context-payload",
            "agent-a",
            "rev-1",
            vec![],
            1_000,
        )
        .expect("store");

        // Cross-agent reuse: a different Agent in the same project/session
        // must hit the entry Agent A produced (the relay is NOT partitioned
        // per agent).
        match bus
            .relay_lookup(
                &scope("p1", "agent-b", Some("s1")),
                "worktree",
                "agent-b",
                None,
                2_000,
            )
            .expect("lookup")
        {
            RelayLookup::Hit(resolved) => {
                let receipt = &resolved.receipt;
                assert_eq!(receipt.key, "worktree");
                assert_eq!(receipt.producer_agent, "agent-a");
                assert_eq!(receipt.source_revision, "rev-1");
                assert_eq!(receipt.avoided_transfer_bytes, b"context-payload".len());
                let expected_digest: [u8; 32] = Sha256::digest(b"context-payload").into();
                assert_eq!(receipt.content_digest, hex_digest(expected_digest));
                // The served payload is the exact stored content, reusable.
                assert_eq!(resolved.payload, b"context-payload");
            }
            RelayLookup::Miss(reason) => panic!("expected a served hit, got {reason:?}"),
        }
        // A third Agent in the same project/session is served too.
        match bus
            .relay_lookup(
                &scope("p1", "agent-c", Some("s1")),
                "worktree",
                "agent-c",
                None,
                2_000,
            )
            .expect("lookup")
        {
            RelayLookup::Hit(resolved) => assert_eq!(resolved.receipt.producer_agent, "agent-a"),
            RelayLookup::Miss(reason) => panic!("expected a served hit, got {reason:?}"),
        }

        // Same project, different session -> isolated.
        assert_eq!(
            bus.relay_lookup(
                &scope("p1", "agent-b", Some("s2")),
                "worktree",
                "agent-b",
                None,
                2_000,
            )
            .expect("lookup"),
            RelayLookup::Miss(RelayMissReason::NotPresent)
        );
        // Different project -> isolated.
        assert_eq!(
            bus.relay_lookup(
                &scope("p2", "agent-b", Some("s1")),
                "worktree",
                "agent-b",
                None,
                2_000,
            )
            .expect("lookup"),
            RelayLookup::Miss(RelayMissReason::NotPresent)
        );
    }

    #[test]
    fn same_agent_self_hit_is_never_served() {
        let bus = ContextBus::new();
        bus.relay_store(
            &scope("p1", "agent-a", Some("s1")),
            "worktree",
            b"context-payload",
            "agent-a",
            "rev-1",
            vec![],
            1_000,
        )
        .expect("store");

        // The producer asking for its own entry must never get a hit.
        assert_eq!(
            bus.relay_lookup(
                &scope("p1", "agent-a", Some("s1")),
                "worktree",
                "agent-a",
                None,
                2_000,
            )
            .expect("lookup"),
            RelayLookup::Miss(RelayMissReason::SelfHit)
        );
        assert_eq!(
            bus.relay_or_fallback(
                &scope("p1", "agent-a", Some("s1")),
                "worktree",
                "agent-a",
                None,
                2_000,
            ),
            RelayOutcome::Fallback(RelayMissReason::SelfHit)
        );
    }

    #[test]
    fn freshness_expiry_and_provenance_are_enforced() {
        let bus = ContextBus::new();
        let store = bus
            .relay_store_with_ttl(
                &scope("p1", "agent-a", Some("s1")),
                "worktree",
                b"context-payload",
                "agent-a",
                "rev-1",
                vec![],
                1_000,
                500,
            )
            .expect("store");
        assert_eq!(store.expires_at_ms, 1_500);
        assert_eq!(store.produced_at_ms, 1_000);
        // The store receipt exposes the content digest so the producer can
        // correlate it with later hits.
        let expected_digest: [u8; 32] = Sha256::digest(b"context-payload").into();
        assert_eq!(store.content_digest, hex_digest(expected_digest));

        // Fresh window -> served to another agent.
        assert!(matches!(
            bus.relay_lookup(
                &scope("p1", "agent-b", Some("s1")),
                "worktree",
                "agent-b",
                None,
                1_400,
            )
            .expect("lookup"),
            RelayLookup::Hit(_)
        ));
        // Past freshness -> miss.
        assert_eq!(
            bus.relay_lookup(
                &scope("p1", "agent-b", Some("s1")),
                "worktree",
                "agent-b",
                None,
                1_501,
            )
            .expect("lookup"),
            RelayLookup::Miss(RelayMissReason::Expired)
        );
    }

    #[test]
    fn content_size_bounds_reject_oversize_and_quota_excess() {
        let bus = ContextBus::new();
        let oversize = vec![0_u8; MAX_RELAY_ENTRY_BYTES + 1];
        assert_eq!(
            bus.relay_store(
                &scope("p1", "agent-a", Some("s1")),
                "big",
                &oversize,
                "agent-a",
                "rev-1",
                vec![],
                1_000,
            )
            .expect_err("must reject"),
            ContextBusError::OversizeEntry
        );
        assert_eq!(
            bus.relay_store(
                &scope("p1", "agent-a", Some("s1")),
                "empty",
                b"",
                "agent-a",
                "rev-1",
                vec![],
                1_000,
            )
            .expect_err("must reject"),
            ContextBusError::EmptyContent
        );

        // Partition entry cap: fill the partition to its limit, then the
        // next store must fail with an explicit boundary instead of evicting.
        for index in 0..MAX_RELAY_ENTRIES_PER_SCOPE {
            bus.relay_store(
                &scope("p1", "agent-a", Some("s1")),
                &format!("key-{index}"),
                b"payload",
                "agent-a",
                "rev-1",
                vec![],
                1_000,
            )
            .expect("fill partition");
        }
        assert_eq!(
            bus.relay_store(
                &scope("p1", "agent-a", Some("s1")),
                "overflow",
                b"payload",
                "agent-a",
                "rev-1",
                vec![],
                1_000,
            )
            .expect_err("must reject"),
            ContextBusError::ScopeEntryLimitExceeded
        );
    }

    #[test]
    fn relay_namespace_is_isolated_from_plain_session_cache() {
        let bus = ContextBus::new();
        let scoped = scope("p1", "agent-a", Some("s1"));
        bus.session_cache_put(&scoped, "draft", b"session-local", 1_000)
            .expect("session cache put");

        // The relay must not serve plain session-cache data...
        assert_eq!(
            bus.relay_lookup(
                &scope("p1", "agent-b", Some("s1")),
                "draft",
                "agent-b",
                None,
                2_000,
            )
            .expect("lookup"),
            RelayLookup::Miss(RelayMissReason::NotPresent)
        );
        // ...and the session cache must not observe relay entries.
        bus.relay_store(
            &scoped,
            "shared",
            b"relay-shared",
            "agent-a",
            "rev-1",
            vec![],
            1_000,
        )
        .expect("relay store");
        assert!(!bus
            .session_cache_get(&scoped, "shared", 2_000)
            .expect("session cache get"));
        assert!(bus
            .session_cache_get(&scoped, "draft", 2_000)
            .expect("session cache get"));
    }

    #[test]
    fn relay_and_session_keys_are_collision_free_for_colon_and_star() {
        let bus = ContextBus::new();
        // Under the old string-concatenated namespace both of these mapped
        // to the same map key "relay:a:*:b:c".  Typed keys keep them
        // isolated: (project "a", no session, key "b:c") vs
        // (project "a:*", session "b", key "c").
        bus.relay_store(
            &scope("a", "agent-a", None),
            "b:c",
            b"payload-one",
            "agent-a",
            "rev-1",
            vec![],
            1_000,
        )
        .expect("store");
        assert_eq!(
            bus.relay_lookup(
                &scope("a:*", "agent-b", Some("b")),
                "c",
                "agent-b",
                None,
                2_000,
            )
            .expect("lookup"),
            RelayLookup::Miss(RelayMissReason::NotPresent)
        );
        // The payload is still served through its real scope, exact bytes.
        match bus
            .relay_lookup(&scope("a", "agent-b", None), "b:c", "agent-b", None, 2_000)
            .expect("lookup")
        {
            RelayLookup::Hit(resolved) => assert_eq!(resolved.payload, b"payload-one"),
            RelayLookup::Miss(reason) => panic!("expected a hit, got {reason:?}"),
        }

        // '*' as a literal session value is distinct from no session.
        bus.relay_store(
            &scope("a", "agent-a", Some("*")),
            "k",
            b"star-session",
            "agent-a",
            "rev-1",
            vec![],
            1_000,
        )
        .expect("store");
        assert_eq!(
            bus.relay_lookup(&scope("a", "agent-b", None), "k", "agent-b", None, 2_000)
                .expect("lookup"),
            RelayLookup::Miss(RelayMissReason::NotPresent)
        );
        assert!(matches!(
            bus.relay_lookup(
                &scope("a", "agent-b", Some("*")),
                "k",
                "agent-b",
                None,
                2_000,
            )
            .expect("lookup"),
            RelayLookup::Hit(_)
        ));

        // Session cache: under the old encoding both of these mapped to the
        // same key "session:a:x:*:b:c:k".  Typed keys keep them isolated:
        // (project "a", agent "x", no session, key "b:c:k") vs
        // (project "a:x:*", agent "b", session "c", key "k").
        bus.session_cache_put(&scope("a", "x", None), "b:c:k", b"sc-one", 1_000)
            .expect("put one");
        bus.session_cache_put(&scope("a:x:*", "b", Some("c")), "k", b"sc-two", 1_000)
            .expect("put two");
        assert!(bus
            .session_cache_get(&scope("a", "x", None), "b:c:k", 2_000)
            .expect("get one"));
        assert!(bus
            .session_cache_get(&scope("a:x:*", "b", Some("c")), "k", 2_000)
            .expect("get two"));
    }

    #[test]
    fn relay_hit_returns_exact_payload_and_fallback_never_exposes_content() {
        let bus = ContextBus::new();
        let payload = b"exact-relayable-context".to_vec();
        bus.relay_store(
            &scope("p1", "agent-a", Some("s1")),
            "worktree",
            &payload,
            "agent-a",
            "rev-1",
            vec![],
            1_000,
        )
        .expect("store");

        // Agent B reuses the exact bytes the relay stored.
        let reader = scope("p1", "agent-b", Some("s1"));
        let resolved = match bus
            .relay_lookup(&reader, "worktree", "agent-b", None, 2_000)
            .expect("lookup")
        {
            RelayLookup::Hit(resolved) => resolved,
            RelayLookup::Miss(reason) => panic!("expected a hit, got {reason:?}"),
        };
        assert_eq!(resolved.payload, payload);
        let expected_digest: [u8; 32] = Sha256::digest(&payload).into();
        assert_eq!(resolved.receipt.content_digest, hex_digest(expected_digest));
        assert_eq!(resolved.receipt.avoided_transfer_bytes, payload.len());

        // Debug output redacts the payload: only metadata is observable.
        let debug = format!("{resolved:?}");
        assert!(!debug.contains("exact-relayable-context"));
        assert!(debug.contains("payload_bytes"));

        // Fallback outcomes carry no payload and never expose content.
        let fallback = bus.relay_or_fallback(
            &scope("p2", "agent-b", Some("s1")),
            "worktree",
            "agent-b",
            None,
            2_000,
        );
        assert!(matches!(
            fallback,
            RelayOutcome::Fallback(RelayMissReason::NotPresent)
        ));
        assert!(!format!("{fallback:?}").contains("exact-relayable-context"));

        // The runtime-facing served path also returns the exact bytes.
        match bus.relay_or_fallback(&reader, "worktree", "agent-b", None, 3_000) {
            RelayOutcome::Served(resolved) => assert_eq!(resolved.payload, payload),
            other => panic!("expected served, got {other:?}"),
        }
    }

    #[test]
    fn file_change_invalidation_never_serves_stale_content() {
        let bus = ContextBus::new();
        let scoped = scope("p1", "agent-a", Some("s1"));
        let observed = vec![file("/repo/src/main.rs", 1024, 100)];
        bus.relay_store(
            &scoped,
            "worktree",
            b"context-payload",
            "agent-a",
            "rev-1",
            observed.clone(),
            1_000,
        )
        .expect("store");

        // Matching file state -> served (to a different Agent in the same
        // project/session).
        let reader = scope("p1", "agent-b", Some("s1"));
        assert!(matches!(
            bus.relay_lookup(&reader, "worktree", "agent-b", Some(&observed), 2_000)
                .expect("lookup"),
            RelayLookup::Hit(_)
        ));
        // Size changed -> invalidated.
        let changed = vec![file("/repo/src/main.rs", 2048, 100)];
        assert_eq!(
            bus.relay_lookup(&reader, "worktree", "agent-b", Some(&changed), 2_000)
                .expect("lookup"),
            RelayLookup::Miss(RelayMissReason::FileChanged)
        );
        // mtime changed -> invalidated.
        let touched = vec![file("/repo/src/main.rs", 1024, 999)];
        assert_eq!(
            bus.relay_lookup(&reader, "worktree", "agent-b", Some(&touched), 2_000)
                .expect("lookup"),
            RelayLookup::Miss(RelayMissReason::FileChanged)
        );
        // No file state supplied for a file-bound entry -> unverifiable.
        assert_eq!(
            bus.relay_lookup(&reader, "worktree", "agent-b", None, 2_000)
                .expect("lookup"),
            RelayLookup::Miss(RelayMissReason::FileStateUnverifiable)
        );
        // Served bytes reflect the actual stored payload.
        if let RelayLookup::Hit(resolved) = bus
            .relay_lookup(&reader, "worktree", "agent-b", Some(&observed), 3_000)
            .expect("lookup")
        {
            assert_eq!(
                resolved.receipt.avoided_transfer_bytes,
                b"context-payload".len()
            );
        } else {
            panic!("expected a served hit with matching file state");
        }
    }

    #[test]
    fn relay_or_fallback_never_propagates_errors() {
        let bus = ContextBus::new();
        let scoped = scope("p1", "agent-a", Some("s1"));

        // Invalid scope/key are converted into an explicit fallback, not an
        // error, so the caller always keeps a baseline path.
        let invalid = ContextBusScope::new("", "agent-a", None).expect_err("invalid scope");
        assert!(matches!(invalid, ContextBusError::InvalidScope(_)));
        assert_eq!(
            bus.relay_or_fallback(&scoped, "", "agent-b", None, 2_000),
            RelayOutcome::Unavailable
        );
        // Missing entries fall back; a served hit is the only served outcome.
        assert_eq!(
            bus.relay_or_fallback(
                &scope("p1", "agent-b", Some("s1")),
                "missing",
                "agent-b",
                None,
                2_000,
            ),
            RelayOutcome::Fallback(RelayMissReason::NotPresent)
        );
        assert_eq!(
            bus.relay_or_fallback(
                &ContextBusScope {
                    project_id: String::new(),
                    agent_id: "agent-a".to_string(),
                    session_id: None,
                },
                "key",
                "agent-b",
                None,
                2_000,
            ),
            RelayOutcome::Unavailable
        );
        // Requester identity must match the declared scope agent: a mismatch
        // fails closed as Unavailable, never as a served hit.
        assert_eq!(
            bus.relay_or_fallback(&scoped, "missing", "other-agent", None, 2_000),
            RelayOutcome::Unavailable
        );
    }

    #[test]
    fn relay_identity_checks_are_enforced_on_both_sides() {
        let bus = ContextBus::new();
        let scoped = scope("p1", "agent-a", Some("s1"));

        // Store: the writer must declare itself as the producer.
        let store_err = bus
            .relay_store(
                &scoped,
                "worktree",
                b"context-payload",
                "agent-b",
                "rev-1",
                vec![],
                1_000,
            )
            .expect_err("producer mismatch must be rejected");
        assert!(matches!(store_err, ContextBusError::InvalidScope(_)));

        // Lookup: the requester must match the declared scope agent.
        let lookup_err = bus
            .relay_lookup(&scoped, "worktree", "other-agent", None, 2_000)
            .expect_err("requester mismatch must be rejected");
        assert!(matches!(lookup_err, ContextBusError::InvalidScope(_)));

        // Runtime-facing path maps the identity mismatch to Unavailable.
        assert_eq!(
            bus.relay_or_fallback(&scoped, "worktree", "other-agent", None, 2_000),
            RelayOutcome::Unavailable
        );
    }

    #[test]
    fn summary_is_bounded_and_never_exposes_content() {
        let bus = ContextBus::new();
        let scoped = scope("p1", "agent-a", Some("s1"));
        bus.relay_store(
            &scoped,
            "worktree",
            b"context-payload",
            "agent-a",
            "rev-1",
            vec![],
            1_000,
        )
        .expect("relay store");
        bus.session_cache_put(&scoped, "draft", b"session-local", 1_000)
            .expect("session cache put");

        let summary = bus.summary().expect("summary");
        assert_eq!(summary.relay_entries, 1);
        assert_eq!(summary.relay_bytes, b"context-payload".len());
        assert_eq!(summary.session_entries, 1);
        assert_eq!(summary.session_bytes, b"session-local".len());
        assert_eq!(summary.evidence_revision, CONTEXT_BUS_EVIDENCE_REVISION);
    }

    #[test]
    fn delivery_receipt_keeps_compression_and_relay_transfer_distinct() {
        // A served relay hit carries the measured avoided transfer, and
        // compression savings ride separately — but neither is claimable
        // without an explicit upstream attribution contract.
        let served = RelayDeliveryReceipt::served(4_096).with_compression_saved_bytes(1_024);
        assert!(served.is_valid());
        assert!(served.served);
        assert!(!served.claimable);
        assert_eq!(served.claimable_relay_savings(), None);
        // Compression savings are carried separately and never counted as
        // relay-avoided transmission.
        assert_eq!(served.compression_saved_bytes, Some(1_024));

        // A fallback never claims anything, even with a pre-send estimate.
        let fallback = RelayDeliveryReceipt::fallback()
            .with_estimated_relay_avoided_transfer(8_192)
            .with_compression_saved_bytes(512);
        assert!(fallback.is_valid());
        assert!(!fallback.served);
        assert_eq!(fallback.claimable_relay_savings(), None);
        assert!(!fallback.claimable);
        assert!(fallback.attribution_basis.is_none());
        // The estimate is recorded for observability but is not claimable.
        assert_eq!(fallback.estimated_relay_avoided_transfer_bytes, Some(8_192));

        // Estimates attached to a served hit never upgrade claimability.
        let estimated =
            RelayDeliveryReceipt::served(2_048).with_estimated_relay_avoided_transfer(16_384);
        assert!(estimated.is_valid());
        assert!(!estimated.claimable);
        assert_eq!(estimated.claimable_relay_savings(), None);
        assert!(
            estimated.claimable_relay_savings() != estimated.estimated_relay_avoided_transfer_bytes
        );

        // Invalid shapes fail the invariant check: served without a measured
        // transfer, claimable without an attribution basis, or unsupported
        // claimable without serving.
        let broken = RelayDeliveryReceipt {
            served: true,
            relay_avoided_transfer_bytes: None,
            compression_saved_bytes: None,
            estimated_relay_avoided_transfer_bytes: None,
            claimable: false,
            attribution_basis: None,
        };
        assert!(!broken.is_valid());
        let broken_claim = RelayDeliveryReceipt {
            served: false,
            relay_avoided_transfer_bytes: Some(64),
            compression_saved_bytes: None,
            estimated_relay_avoided_transfer_bytes: None,
            claimable: true,
            attribution_basis: Some(RelayAttributionBasis::ProviderUsageAttributed),
        };
        assert!(!broken_claim.is_valid());
        let unattributed_claim = RelayDeliveryReceipt {
            served: true,
            relay_avoided_transfer_bytes: Some(64),
            compression_saved_bytes: None,
            estimated_relay_avoided_transfer_bytes: None,
            claimable: true,
            attribution_basis: None,
        };
        assert!(!unattributed_claim.is_valid());
    }

    #[test]
    fn claimable_requires_explicit_provider_usage_attribution() {
        // Measured but unattributed: never claimable.
        let served = RelayDeliveryReceipt::served(4_096);
        assert!(served.is_valid());
        assert!(served.served);
        assert_eq!(served.relay_avoided_transfer_bytes, Some(4_096));
        assert!(!served.claimable);
        assert!(served.attribution_basis.is_none());
        assert_eq!(served.claimable_relay_savings(), None);

        // The explicit upstream attribution contract upgrades claimability.
        let attributed = served
            .with_compression_saved_bytes(1_024)
            .with_provider_usage_attribution(RelayAttributionBasis::ProviderUsageAttributed);
        assert!(attributed.is_valid());
        assert!(attributed.claimable);
        assert_eq!(
            attributed.attribution_basis,
            Some(RelayAttributionBasis::ProviderUsageAttributed)
        );
        // Compression savings are never double-counted into relay savings.
        assert_eq!(attributed.claimable_relay_savings(), Some(4_096));
        assert_ne!(
            attributed.claimable_relay_savings(),
            attributed.compression_saved_bytes
        );

        // A fallback can never be upgraded to claimable.
        let fallback = RelayDeliveryReceipt::fallback()
            .with_provider_usage_attribution(RelayAttributionBasis::ProviderUsageAttributed);
        assert!(fallback.is_valid());
        assert!(!fallback.claimable);
        assert!(fallback.attribution_basis.is_none());
        assert_eq!(fallback.claimable_relay_savings(), None);
    }
}
