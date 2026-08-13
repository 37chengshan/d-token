//! Production cc-switch read-only snapshot, hash-bound follow decision, and
//! change watcher.
//!
//! Boundary (kept deliberately small and honest):
//!
//! * `read_cc_switch_snapshot` opens the cc-switch SQLite database with
//!   read-only flags, never writes, and only derives a metadata snapshot from
//!   schema-12 columns. Payloads (`settings_config`), `private_metadata` and
//!   credential *values* are never read into the snapshot; only a
//!   credential-present boolean and a normalized endpoint survive.
//! * The watcher is **not** a SQLite watcher: it is a byte-level polling
//!   watcher over the located DB/WAL/SHM paths and their parent directory. It
//!   never opens or parses the database; it only hashes raw file bytes and
//!   directory listings, so it cannot read credentials either.
//! * Resolution is fail-closed: an unknown schema version resolves to
//!   `blocked`, multiple active candidates for one app type resolve to
//!   `needs_selection`, and an active provider missing a required field is
//!   never silently defaulted — it blocks the selection state.

use anyhow::{Context, Result};
use rusqlite::{Connection, OpenFlags, OptionalExtension};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use crate::stable_hash;

/// Production watcher debounce (ms): a settled change is reported only after
/// the watched paths stay quiet for this long.
pub const CC_SWITCH_WATCH_DEBOUNCE_MS: u64 = 750;

/// The only schema version this crate understands. Anything else resolves
/// fail-closed to `blocked` without touching any table.
pub const CC_SWITCH_SUPPORTED_SCHEMA_VERSION: i64 = 12;

/// Read-only snapshot of an external cc-switch selection.
///
/// Serialization of this type is the leak test surface: it must never contain
/// `settings_config` payloads, `private_metadata`, env keys, or credential
/// values — only the derived metadata fields below.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CcSwitchSnapshot {
    /// SQLite `PRAGMA user_version` of the source database.
    pub schema_version: i64,
    /// `loaded` when the schema is supported, `unknown` otherwise.
    pub schema_kind: String,
    /// `loaded` when the source was parsed, `blocked` otherwise.
    pub source_state: String,
    /// `resolved` | `needs_selection` | `blocked`.
    pub selection_state: String,
    /// Every active candidate (never a silent subset), with redacted fields.
    pub active_upstreams: Vec<CcSwitchUpstream>,
    /// Profile names, listed read-only for the selection decision.
    pub profiles: Vec<String>,
    /// SHA-256 of the raw source database bytes (metadata only).
    pub source_fingerprint: String,
    /// Content-derived revision: first 8 bytes of the source fingerprint, so
    /// route sync can order snapshots by content without reading payloads.
    pub revision: u64,
}

/// One active upstream candidate. Credential values and payload fields are
/// deliberately absent; only a presence boolean and a normalized endpoint are
/// surfaced.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CcSwitchUpstream {
    pub app_type: String,
    pub provider_id: String,
    /// Missing model/protocol on an active provider fails closed (the
    /// candidate is dropped and the snapshot blocks) — never defaulted here.
    pub model: String,
    pub protocol: String,
    /// Endpoint normalized by stripping user-info, query, fragment and a
    /// trailing slash; the value itself is metadata, not a credential.
    pub normalized_endpoint: String,
    /// True when the provider's config carries a credential-shaped field with
    /// a non-empty value. The value is never read or returned.
    pub credential_present: bool,
}

/// Outcome of a hash-bound follow decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FollowDecision {
    /// The synced route already matches the current route.
    Unchanged,
    /// The upstream identity is authorized and the model changed: sync.
    SyncRequired,
    /// A new provider identity / endpoint / protocol appeared: reauthorize.
    ReauthorizationRequired,
}

/// Hash of an upstream identity: provider id, source, protocol and normalized
/// endpoint. Format is pinned by the fixture contract and must not change.
pub fn cc_switch_upstream_signature(
    provider_id: &str,
    source: &str,
    protocol: &str,
    endpoint: &str,
) -> String {
    stable_hash(&format!(
        "cc-switch-upstream-v1\n{provider_id}\n{source}\n{protocol}\n{endpoint}"
    ))
}

/// Hash of a specific route (upstream identity + model). Model changes within
/// an authorized identity are followable; identity changes are not.
pub fn cc_switch_route_signature(upstream_signature: &str, model: &str) -> String {
    stable_hash(&format!(
        "cc-switch-route-v1\n{upstream_signature}\n{model}"
    ))
}

/// Fail-closed follow decision (upstream-signature-bound legacy contract).
/// A new upstream identity is never followed without reauthorization.
pub fn follow_decision(
    authorized_upstream_signatures: &[String],
    last_synced_route_signature: Option<&str>,
    upstream_signature: &str,
    route_signature: &str,
) -> FollowDecision {
    if !authorized_upstream_signatures
        .iter()
        .any(|authorized| authorized == upstream_signature)
    {
        FollowDecision::ReauthorizationRequired
    } else if last_synced_route_signature == Some(route_signature) {
        FollowDecision::Unchanged
    } else {
        FollowDecision::SyncRequired
    }
}

/// App-level authorized follow decision (PRD P0.0.10.6 / PRODUCT_CONTRACT §6).
///
/// Once an app (agent_id) is authorized, every provider/model/protocol/
/// endpoint/profile change within that app follows automatically — d-token
/// updates its own route/runtime without a second confirmation. Only an app
/// that was never authorized still requires (re)authorization and fails
/// closed. This is the contract the R2.C auto-follow path must use; the
/// legacy signature-bound `follow_decision` remains for the pre-R2.C path.
pub fn follow_decision_for_app(
    authorized_app_ids: &[String],
    app_id: &str,
    last_synced_route_signature: Option<&str>,
    _upstream_signature: &str,
    route_signature: &str,
) -> FollowDecision {
    if !authorized_app_ids.iter().any(|authorized| authorized == app_id) {
        return FollowDecision::ReauthorizationRequired;
    }
    if last_synced_route_signature == Some(route_signature) {
        FollowDecision::Unchanged
    } else {
        FollowDecision::SyncRequired
    }
}

/// Parses a cc-switch database read-only into a redacted snapshot.
///
/// Errors are reserved for unreadable / non-SQLite sources. An unknown but
/// valid schema resolves to a `blocked` snapshot, never an error.
pub fn read_cc_switch_snapshot(path: &Path) -> Result<CcSwitchSnapshot> {
    let raw = fs::read(path).context("failed to read cc-switch database for fingerprint")?;
    let digest = sha256_bytes(&raw);
    let source_fingerprint = hex(&digest);
    let revision = u64::from_be_bytes(digest[..8].try_into().expect("8 bytes"));

    let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .context("failed to open cc-switch database read-only")?;
    let schema_version: i64 = connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap_or(0);

    if schema_version != CC_SWITCH_SUPPORTED_SCHEMA_VERSION {
        // Fail closed: unknown schema never fabricates upstreams.
        return Ok(CcSwitchSnapshot {
            schema_version,
            schema_kind: "unknown".into(),
            source_state: "blocked".into(),
            selection_state: "blocked".into(),
            active_upstreams: Vec::new(),
            profiles: Vec::new(),
            source_fingerprint,
            revision,
        });
    }

    let profiles = load_profiles(&connection)?;
    let mut candidates = Vec::new();
    let mut parse_failure = false;

    let mut statement = connection
        .prepare(
            "SELECT id, app_type, settings_config FROM providers \
             WHERE is_current = 1 ORDER BY sort_index, id",
        )
        .context("failed to prepare active provider query")?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .context("failed to read active providers")?;
    for row in rows {
        let (provider_id, app_type, settings_config) =
            row.context("failed to read active provider row")?;
        match parse_active_upstream(&connection, &provider_id, &app_type, &settings_config) {
            Ok(Some(upstream)) => candidates.push(upstream),
            // An active provider that cannot be resolved is never silently
            // skipped: the whole selection blocks.
            Ok(None) => parse_failure = true,
            Err(_) => parse_failure = true,
        }
    }

    let selection_state = if parse_failure {
        "blocked".to_string()
    } else if candidates.is_empty() {
        "resolved".to_string()
    } else if has_multiple_active_per_app_type(&candidates) {
        "needs_selection".to_string()
    } else {
        "resolved".to_string()
    };

    Ok(CcSwitchSnapshot {
        schema_version,
        schema_kind: "loaded".into(),
        source_state: "loaded".into(),
        selection_state,
        active_upstreams: candidates,
        profiles,
        source_fingerprint,
        revision,
    })
}

/// Parses one active provider's config into a redacted upstream candidate.
///
/// Returns `Ok(None)` when a required field is missing (fail closed) and
/// `Err` when the payload is not parseable — both block the snapshot.
fn parse_active_upstream(
    connection: &Connection,
    provider_id: &str,
    app_type: &str,
    settings_config: &str,
) -> Result<Option<CcSwitchUpstream>> {
    let settings: serde_json::Value = match serde_json::from_str(settings_config) {
        Ok(value) => value,
        Err(_) => return Ok(None),
    };
    let endpoint_row: Option<String> = connection
        .query_row(
            "SELECT endpoint FROM provider_endpoints WHERE provider_id = ?1",
            [provider_id],
            |row| row.get(0),
        )
        .optional()
        .context("failed to read provider endpoint")?;

    // env-shaped config (Claude Code style): ANTHROPIC_* fields inside `env`.
    if let Some(env) = settings.get("env").and_then(serde_json::Value::as_object) {
        let Some(endpoint) = env
            .get("ANTHROPIC_BASE_URL")
            .and_then(serde_json::Value::as_str)
        else {
            return Ok(None);
        };
        let Some(model) = env
            .get("ANTHROPIC_MODEL")
            .and_then(serde_json::Value::as_str)
        else {
            return Ok(None);
        };
        let credential_present = env
            .iter()
            .any(|(key, value)| is_credential_key(key) && has_value(value));
        return Ok(Some(CcSwitchUpstream {
            app_type: app_type.to_string(),
            provider_id: provider_id.to_string(),
            model: model.to_string(),
            // The env-shaped transport is by definition the anthropic
            // protocol family; this is a semantic fact of the shape, not a
            // silent guess.
            protocol: "anthropic".to_string(),
            normalized_endpoint: normalize_endpoint(endpoint),
            credential_present,
        }));
    }

    // baseUrl-shaped config (open_ai / custom providers). The endpoint table
    // row, when present, is authoritative over the embedded baseUrl.
    let endpoint = match endpoint_row.or_else(|| {
        settings
            .get("baseUrl")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string)
    }) {
        Some(endpoint) => endpoint,
        None => return Ok(None),
    };
    let Some(protocol) = settings.get("protocol").and_then(serde_json::Value::as_str) else {
        return Ok(None);
    };
    let Some(model) = settings.get("model").and_then(serde_json::Value::as_str) else {
        return Ok(None);
    };
    let credential_present = settings
        .as_object()
        .into_iter()
        .flat_map(serde_json::Map::iter)
        .any(|(key, value)| is_credential_key(key) && has_value(value));

    Ok(Some(CcSwitchUpstream {
        app_type: app_type.to_string(),
        provider_id: provider_id.to_string(),
        model: model.to_string(),
        protocol: protocol.to_string(),
        normalized_endpoint: normalize_endpoint(&endpoint),
        credential_present,
    }))
}

fn load_profiles(connection: &Connection) -> Result<Vec<String>> {
    let mut statement = connection
        .prepare("SELECT name FROM profiles ORDER BY id")
        .context("failed to prepare profile query")?;
    let names = statement
        .query_map([], |row| row.get::<_, String>(0))
        .context("failed to read profiles")?
        .collect::<std::result::Result<Vec<_>, _>>()
        .context("failed to collect profiles")?;
    Ok(names)
}

fn has_multiple_active_per_app_type(candidates: &[CcSwitchUpstream]) -> bool {
    candidates.iter().any(|candidate| {
        candidates
            .iter()
            .filter(|other| other.app_type == candidate.app_type)
            .count()
            > 1
    })
}

/// Endpoint normalization that never touches credentials: strips user-info,
/// query, fragment and a trailing slash.
fn normalize_endpoint(raw: &str) -> String {
    let mut normalized = raw.trim().to_string();
    while normalized.ends_with('/') {
        normalized.pop();
    }
    if let Some(hash) = normalized.find('#') {
        normalized.truncate(hash);
    }
    if let Some(query) = normalized.find('?') {
        normalized.truncate(query);
    }
    if let Some(scheme_end) = normalized.find("://") {
        let host_start = scheme_end + 3;
        if let Some(at) = normalized[host_start..].find('@') {
            normalized.replace_range(host_start..host_start + at + 1, "");
        }
    }
    normalized
}

/// Credential-shaped key heuristic. Presence of such a key with a non-empty
/// value flips `credential_present`; the value itself is never read.
fn is_credential_key(key: &str) -> bool {
    let lowered = key.to_ascii_lowercase();
    lowered.contains("token")
        || lowered.contains("api_key")
        || lowered.contains("apikey")
        || lowered.contains("secret")
        || lowered.contains("password")
        || lowered.contains("authorization")
        || lowered.contains("auth")
        || lowered == "key"
}

fn has_value(value: &serde_json::Value) -> bool {
    value.as_str().is_some_and(|value| !value.is_empty())
}

fn sha256_bytes(data: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hasher.finalize().into()
}

fn hex(bytes: &[u8; 32]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Outcome of a single `CcSwitchWatcher::poll()`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WatchEvent {
    /// No change since the stable snapshot.
    NoChange,
    /// A change started a debounce window that has not settled yet.
    PendingChange,
    /// The change stayed quiet for the full debounce; handle it exactly once.
    Settled,
    /// A pending change rolled back to the stable snapshot before settling.
    Reverted,
    /// A settled change is still being handled; new changes are single-flighted.
    InFlight,
}

/// Byte-level polling watcher over located cc-switch DB/WAL/SHM paths and
/// their parent directory.
///
/// This is deliberately **not** a SQLite watcher: it hashes raw file bytes
/// and directory listings and never opens or parses the database, so it can
/// never read credentials. The mode is always reported explicitly as
/// `polling_fallback`; there is no silent inotify/FSEvents path.
pub struct CcSwitchWatcher {
    paths: Vec<PathBuf>,
    debounce: Duration,
    baseline: Vec<Option<String>>,
    pending_since: Option<Instant>,
    pending_hash: Vec<Option<String>>,
    settled: bool,
}

impl CcSwitchWatcher {
    pub fn new(paths: Vec<PathBuf>, debounce: Duration) -> Result<Self> {
        anyhow::ensure!(
            !paths.is_empty(),
            "cc-switch watcher needs at least one path"
        );
        let baseline = paths.iter().map(|path| entry_hash(path)).collect();
        Ok(Self {
            paths,
            debounce,
            baseline,
            pending_since: None,
            pending_hash: Vec::new(),
            settled: false,
        })
    }

    /// Reports the watcher mode explicitly; this implementation always uses
    /// polling fallback.
    pub fn watch_mode(&self) -> &'static str {
        "polling_fallback"
    }

    /// Advances the watcher state machine by one poll.
    pub fn poll(&mut self) -> WatchEvent {
        let current = self.hash_all();
        if self.settled {
            if current == self.baseline {
                WatchEvent::NoChange
            } else {
                WatchEvent::InFlight
            }
        } else if let Some(since) = self.pending_since {
            if current == self.baseline {
                // The pending change rolled back before settling.
                self.pending_since = None;
                WatchEvent::Reverted
            } else if current == self.pending_hash {
                if since.elapsed() >= self.debounce {
                    // Settles exactly once; further changes are single-flighted
                    // until `mark_handled`.
                    self.baseline = current;
                    self.pending_since = None;
                    self.settled = true;
                    WatchEvent::Settled
                } else {
                    WatchEvent::NoChange
                }
            } else {
                // The change evolved during the debounce window: restart it.
                self.pending_hash = current;
                self.pending_since = Some(Instant::now());
                WatchEvent::PendingChange
            }
        } else if current == self.baseline {
            WatchEvent::NoChange
        } else {
            self.pending_hash = current;
            self.pending_since = Some(Instant::now());
            WatchEvent::PendingChange
        }
    }

    /// Acknowledges that the settled change has been handled, ending
    /// single-flighting. The settled state becomes the new stable baseline.
    pub fn mark_handled(&mut self) {
        self.settled = false;
    }

    fn hash_all(&self) -> Vec<Option<String>> {
        self.paths.iter().map(|path| entry_hash(path)).collect()
    }
}

/// Hash of one watched entry: directory listings are hashed by sorted entry
/// names, files by raw content bytes. `None` when the entry is absent.
fn entry_hash(path: &Path) -> Option<String> {
    let metadata = fs::metadata(path).ok()?;
    if metadata.is_dir() {
        let mut names: Vec<String> = fs::read_dir(path)
            .ok()?
            .filter_map(|entry| {
                entry
                    .ok()
                    .map(|entry| entry.file_name().to_string_lossy().into_owned())
            })
            .collect();
        names.sort();
        Some(stable_hash(&names.join("\n")))
    } else {
        let bytes = fs::read(path).ok()?;
        Some(hex(&sha256_bytes(&bytes)))
    }
}
