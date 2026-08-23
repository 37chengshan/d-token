#[cfg(test)]
use crate::agent_registry::AGENT_REGISTRY;
use crate::agent_registry::{
    agent_descriptor, scan_current_process_agents, AgentDescriptor, AgentEvidenceKind,
    AgentInstallCapability,
};
use dtoken_config_patcher::agent::{allowed_agent_config_paths, current_process_scan_context};
use dtoken_config_patcher::agent_mcp::{
    is_verified_hook_agent, resolve_hook_config_path, resolve_mcp_config_path_for,
};
use dtoken_config_patcher::agent_transport::{agent_transport_adapter, can_clear_agent_transport};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use rusqlite::{Connection, OpenFlags};
use serde::{
    de::{IgnoredAny, MapAccess, SeqAccess, Visitor},
    Deserialize, Deserializer, Serialize,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    env, fmt, fs,
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{Arc, Mutex, OnceLock},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

/// Earliest cc-switch schema version that shares the provider/proxy_config table
/// shape we read.  Newer versions are probed structurally (try prepare) before
/// being accepted or rejected, so the integer alone is not a hard gate.
const CC_SWITCH_MIN_SCHEMA_VERSION: i64 = 12;
/// Schema versions above this threshold are **structurally probed** (must
/// pass a table-column existence check) before being accepted.  Versions
/// reasonably close to our known range that pass the structural probe are
/// marked `loaded_compatible_newer`; versions far beyond this threshold that
/// nevertheless pass the probe are accepted but flagged `loaded_future_schema`
/// so the UI can show a compatibility note.
const CC_SWITCH_PROBE_MAX_SCHEMA_VERSION: i64 = 20;
const CC_SWITCH_COMMIT: &str = "f39d463c442e705727531b85f2db98e00ccaf11e";
const EXTERNAL_CONFIG_SNAPSHOT_VERSION: u8 = 1;
const EXTERNAL_CONFIG_POLL_INTERVAL_MS: u64 = 10_000;
const EXTERNAL_CONFIG_DEBOUNCE_MS: u64 = 750;
const MIN_EXTERNAL_CONFIG_POLL_INTERVAL_MS: u64 = 1_000;
const MAX_EXTERNAL_CONFIG_POLL_INTERVAL_MS: u64 = 300_000;
const MIN_EXTERNAL_CONFIG_DEBOUNCE_MS: u64 = 100;
const MAX_EXTERNAL_CONFIG_DEBOUNCE_MS: u64 = 10_000;
static EXTERNAL_CONFIG_REFRESH_COORDINATOR: OnceLock<Mutex<ExternalConfigRefreshCoordinator>> =
    OnceLock::new();
static EXTERNAL_CONFIG_WATCHER: OnceLock<Mutex<Option<RecommendedWatcher>>> = OnceLock::new();

/// A source-state contract for cc-switch metadata. Values are deliberately
/// independent from provider data so callers can distinguish an absent
/// installation from a database that could not be safely inspected.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CcSwitchSourceState {
    Missing,
    MetadataUnavailable,
    LockedOrCorrupt,
    UnsupportedSchema,
    Loaded,
}

impl CcSwitchSourceState {
    fn from_source(value: Option<&str>) -> Self {
        match value {
            Some("loaded") => Self::Loaded,
            Some("locked_or_corrupt") => Self::LockedOrCorrupt,
            Some("unsupported_schema") | Some("invalid_schema") => Self::UnsupportedSchema,
            Some("missing") | None => Self::Missing,
            _ => Self::MetadataUnavailable,
        }
    }
}

/// Metadata only: this type never contains a configuration value, credential,
/// SQLite row payload, or an unredacted external path.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ExternalFileMetadata {
    pub name: String,
    pub state: String,
    pub bytes: Option<u64>,
    pub modified_unix_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CcSwitchSnapshot {
    pub source_state: CcSwitchSourceState,
    pub schema_version: Option<i64>,
    pub schema_kind: Option<String>,
    pub path: Option<String>,
    pub files: Vec<ExternalFileMetadata>,
    pub app_types: Vec<String>,
    pub active_upstreams: Vec<CcSwitchActiveUpstreamSnapshot>,
    /// `resolved`, `needs_selection`, or an explicit source failure. A
    /// provider name is never used as a stand-in for an unknown profile.
    pub selection_state: String,
    pub fingerprint: String,
    pub revision: String,
    pub observed_at: String,
    pub mode: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CcSwitchActiveUpstreamSnapshot {
    pub app_type: String,
    pub provider_id: String,
    pub profile_scope: String,
    pub model: Option<String>,
    pub protocol: String,
    pub endpoint: Option<String>,
    pub credential_present: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AgentConfigSnapshot {
    pub agent_id: String,
    pub state: String,
    pub path: String,
    pub fingerprint: String,
}

/// The stable, safe basis for deciding whether externally-owned configuration
/// may need a human review. It intentionally has no apply instruction: a
/// changed snapshot is evidence only, never route authorization.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ExternalConfigSnapshot {
    pub version: u8,
    pub mode: String,
    pub poll_interval_ms: u64,
    pub debounce_ms: u64,
    pub observed_at: String,
    pub cc_switch: CcSwitchSnapshot,
    pub agent_configs: Vec<AgentConfigSnapshot>,
    pub fingerprint: String,
    pub automatic_apply_allowed: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ExternalConfigRefreshEvent {
    /// `external_config_snapshot`, `change_pending_debounce`,
    /// `external_config_review_required`, or `unchanged`.
    pub state: String,
    pub mode: String,
    pub snapshot_fingerprint: String,
    pub previous_fingerprint: Option<String>,
    pub changed_sources: Vec<String>,
    pub review_required: bool,
    pub automatic_apply_allowed: bool,
    pub next_poll_not_before_ms: u64,
}

/// Injectable bounded metadata-polling coordinator. A runtime owner collects
/// snapshots on its bounded cadence and calls `observe`; this module
/// deliberately does not create a watcher thread or emit Tauri events itself.
#[derive(Debug, Clone)]
pub struct ExternalConfigRefreshCoordinator {
    poll_interval_ms: u64,
    debounce_ms: u64,
    next_poll_not_before_ms: u64,
    last_snapshot: Option<ExternalConfigSnapshot>,
    pending_snapshot: Option<(u64, ExternalConfigSnapshot, Vec<String>)>,
}

impl Default for ExternalConfigRefreshCoordinator {
    fn default() -> Self {
        Self::with_intervals(
            EXTERNAL_CONFIG_POLL_INTERVAL_MS,
            EXTERNAL_CONFIG_DEBOUNCE_MS,
        )
    }
}

impl ExternalConfigRefreshCoordinator {
    pub fn with_intervals(poll_interval_ms: u64, debounce_ms: u64) -> Self {
        Self {
            poll_interval_ms: poll_interval_ms.clamp(
                MIN_EXTERNAL_CONFIG_POLL_INTERVAL_MS,
                MAX_EXTERNAL_CONFIG_POLL_INTERVAL_MS,
            ),
            debounce_ms: debounce_ms.clamp(
                MIN_EXTERNAL_CONFIG_DEBOUNCE_MS,
                MAX_EXTERNAL_CONFIG_DEBOUNCE_MS,
            ),
            next_poll_not_before_ms: 0,
            last_snapshot: None,
            pending_snapshot: None,
        }
    }

    /// Exposes the next preferred metadata refresh deadline to callers that
    /// own a file watcher. Direct watcher notifications still pass through
    /// `observe` immediately so a settled external write is never missed.
    pub fn poll_due(&self, now_ms: u64) -> bool {
        now_ms >= self.next_poll_not_before_ms
    }

    pub fn observe(
        &mut self,
        now_ms: u64,
        snapshot: ExternalConfigSnapshot,
    ) -> ExternalConfigRefreshEvent {
        self.next_poll_not_before_ms = now_ms.saturating_add(self.poll_interval_ms);
        let previous_fingerprint = self
            .last_snapshot
            .as_ref()
            .map(|previous| previous.fingerprint.clone());
        let unchanged = previous_fingerprint
            .as_deref()
            .is_some_and(|fingerprint| fingerprint == snapshot.fingerprint);

        // A changed fingerprint must remain stable for the full debounce
        // window before review is required. The pending snapshot is separate
        // from `last_snapshot`, which remains the last-known-good snapshot
        // until a review is required.
        if let Some((changed_at_ms, pending, changed_sources)) = &self.pending_snapshot {
            if pending.fingerprint == snapshot.fingerprint {
                if now_ms.saturating_sub(*changed_at_ms) < self.debounce_ms {
                    return self.event(
                        "change_pending_debounce",
                        snapshot.fingerprint,
                        previous_fingerprint,
                        changed_sources.clone(),
                        false,
                    );
                }
                let changed_sources = changed_sources.clone();
                self.pending_snapshot = None;
                self.last_snapshot = Some(snapshot.clone());
                return self.event(
                    "external_config_review_required",
                    snapshot.fingerprint,
                    previous_fingerprint,
                    changed_sources,
                    true,
                );
            }
        }

        if self.last_snapshot.is_none() {
            self.last_snapshot = Some(snapshot.clone());
            return self.event(
                "external_config_snapshot",
                snapshot.fingerprint,
                None,
                Vec::new(),
                false,
            );
        }

        if unchanged {
            // The external config returned to the last stable fingerprint.
            // Discard the aborted change so a later recurrence must complete
            // a fresh full debounce window.
            self.pending_snapshot = None;
            return self.event(
                "unchanged",
                snapshot.fingerprint,
                previous_fingerprint,
                Vec::new(),
                false,
            );
        }

        let changed_sources = self
            .last_snapshot
            .as_ref()
            .map(|previous| changed_external_sources(previous, &snapshot))
            .unwrap_or_default();
        self.pending_snapshot = Some((now_ms, snapshot.clone(), changed_sources.clone()));
        self.event(
            "change_pending_debounce",
            snapshot.fingerprint,
            previous_fingerprint,
            changed_sources,
            false,
        )
    }

    fn event(
        &self,
        state: &str,
        snapshot_fingerprint: String,
        previous_fingerprint: Option<String>,
        changed_sources: Vec<String>,
        review_required: bool,
    ) -> ExternalConfigRefreshEvent {
        ExternalConfigRefreshEvent {
            state: state.to_string(),
            mode: "metadata_polling".to_string(),
            snapshot_fingerprint,
            previous_fingerprint,
            changed_sources,
            review_required,
            automatic_apply_allowed: false,
            next_poll_not_before_ms: self.next_poll_not_before_ms,
        }
    }
}

/// The watcher never opens or writes Agent or cc-switch files. It observes only
/// confirmed Agent config parents plus the cc-switch database parents so
/// atomic replacement, SQLite WAL/SHM changes, and ordinary file writes all
/// trigger the same debounced read-only rediscovery. A polling fallback remains
/// mandatory because filesystem notifications can be coalesced or unavailable.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ExternalConfigWatcherStart {
    pub state: String,
    pub mode: String,
    pub watched_paths: Vec<String>,
    pub polling_fallback: bool,
}

pub fn start_external_config_watcher(
    on_change: Arc<dyn Fn() + Send + Sync + 'static>,
) -> Result<ExternalConfigWatcherStart, String> {
    let watcher_slot = EXTERNAL_CONFIG_WATCHER.get_or_init(|| Mutex::new(None));
    let mut watcher_slot = watcher_slot
        .lock()
        .map_err(|_| "external config watcher coordination is unavailable".to_string())?;
    let agent_configs = agent_config_watch_files();
    let cc_switch_databases = cc_switch_db_paths();
    let paths = external_config_watch_paths(&agent_configs, &cc_switch_databases);
    if watcher_slot.is_some() {
        return Ok(ExternalConfigWatcherStart {
            state: "already_running".to_string(),
            mode: "read_only_notify_with_polling_fallback".to_string(),
            watched_paths: paths.iter().map(|path| display_path(path)).collect(),
            polling_fallback: true,
        });
    }

    let callback = Arc::clone(&on_change);
    let mut watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
        if let Ok(event) = event {
            if event.paths.iter().any(|path| {
                is_external_config_watch_path_for_targets(
                    path,
                    &agent_configs,
                    &cc_switch_databases,
                )
            }) {
                callback();
            }
        }
    })
    .map_err(|error| format!("external read-only watcher initialization failed: {error}"))?;

    if paths.is_empty() {
        return Err("external config watcher has no confirmed directory to observe".to_string());
    }
    for path in &paths {
        watcher
            .watch(path, RecursiveMode::NonRecursive)
            .map_err(|error| format!("external read-only watcher setup failed: {error}"))?;
    }
    let watched_paths = paths.iter().map(|path| display_path(path)).collect();
    *watcher_slot = Some(watcher);
    Ok(ExternalConfigWatcherStart {
        state: "running".to_string(),
        mode: "read_only_notify_with_polling_fallback".to_string(),
        watched_paths,
        polling_fallback: true,
    })
}

fn agent_config_watch_files() -> Vec<PathBuf> {
    let mut paths = scan_current_process_agents()
        .ok()
        .into_iter()
        .flat_map(|snapshot| snapshot.agents)
        .flat_map(|agent| {
            let agent_id = agent.id;
            agent
                .evidence
                .into_iter()
                .filter(|evidence| evidence.kind == AgentEvidenceKind::Config)
                .flat_map(move |evidence| {
                    agent_config_files_for_snapshot(&agent_id, &evidence.path)
                })
        })
        .collect::<Vec<_>>();
    paths.sort();
    paths.dedup();
    paths
}

fn external_config_watch_paths(
    agent_configs: &[PathBuf],
    cc_switch_databases: &[PathBuf],
) -> Vec<PathBuf> {
    let mut targets = agent_configs
        .iter()
        .cloned()
        .chain(cc_switch_databases.iter().cloned())
        .collect::<Vec<_>>();
    let mut paths = targets
        .drain(..)
        .filter_map(|target| {
            let preferred = target.parent()?.to_path_buf();
            let mut current = preferred.as_path();
            loop {
                if current.is_dir() {
                    return Some(current.to_path_buf());
                }
                current = current.parent()?;
            }
        })
        .collect::<Vec<_>>();
    paths.sort();
    paths.dedup();
    paths
}

fn is_external_config_watch_path_for_targets(
    path: &Path,
    agent_configs: &[PathBuf],
    cc_switch_databases: &[PathBuf],
) -> bool {
    agent_configs.iter().any(|config| path == config)
        || is_cc_switch_watch_path_for_databases(path, cc_switch_databases)
}

fn is_cc_switch_watch_path_for_databases(path: &Path, databases: &[PathBuf]) -> bool {
    databases.iter().any(|database| {
        let wal = PathBuf::from(format!("{}-wal", database.display()));
        let shm = PathBuf::from(format!("{}-shm", database.display()));
        path == database
            || path == wal
            || path == shm
            || database.parent().is_some_and(|parent| path == parent)
    })
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct UpstreamCandidate {
    pub id: String,
    pub name: String,
    pub protocol: String,
    pub base_url: Option<String>,
    pub models: Vec<String>,
    pub source: String,
    pub freshness: String,
    pub auth_mode: String,
    pub active: bool,
    pub port: Option<u16>,
    pub state: String,
    pub agent_ids: Vec<String>,
    pub confidence: u8,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AgentUpstreamBinding {
    pub agent_id: String,
    pub agent_name: String,
    pub installed: bool,
    pub binary_path: Option<String>,
    pub config_path: Option<String>,
    pub config_paths: Vec<String>,
    pub current_config_source: String,
    pub candidates: Vec<UpstreamCandidate>,
    pub resolved_candidate: Option<UpstreamCandidate>,
    pub resolution: String,
    pub confidence: u8,
    pub reasons: Vec<String>,
    pub capability: String,
    pub can_configure: bool,
    pub can_clear: bool,
    pub transport_supported: bool,
    pub transport_status: String,
    pub transport_reason: Option<String>,
    pub already_connected: bool,
    /// The kind of integration this adapter provides: "route", "mcp", or "hook".
    /// Populated from the transport adapter descriptor. Agents without a
    /// verified transport adapter get "unsupported".
    pub integration_kind: String,
    /// Truthful, credential-free description of limitations, or null for Route
    /// adapters that provide full route evidence.
    pub truthful_limitation: Option<String>,
    /// Whether this integration provides transparent compression and route
    /// evidence. Always false for MCP and Hook integrations.
    pub provides_route_evidence: bool,
    /// Whether a verified MCP config path exists for this agent. When true,
    /// the agent supports MCP-level configure/clear through the d-token-agent-bridge
    /// even if the transport adapter cannot configure a Route integration.
    pub mcp_available: bool,
    /// Whether a verified Hook config path exists for this agent (e.g.
    /// cortex-code hooks.json). Distinct from `mcp_available`: a Hook agent
    /// has no MCP config target.
    pub hook_available: bool,
    /// The resolved absolute path to the MCP configuration file, if known.
    pub mcp_config_path: Option<String>,
}

pub fn discover() -> Value {
    let mut candidates = Vec::new();
    let mut sources = Vec::new();
    let agent_discovery = scan_current_process_agents();
    discover_opencode_catalog(&mut candidates, &mut sources);
    if let Ok(snapshot) = &agent_discovery {
        discover_agent_configs(&snapshot.agents, &mut candidates, &mut sources);
    }
    discover_cc_switch(&mut candidates, &mut sources);
    normalize_cc_switch_proxy_candidates(&mut candidates);
    candidates.sort_by(|left, right| left.source.cmp(&right.source).then(left.id.cmp(&right.id)));
    candidates.dedup_by(|left, right| {
        left.source == right.source && left.id == right.id && left.models == right.models
    });
    let external_config_snapshot =
        external_config_snapshot(&sources, &candidates, agent_discovery.as_ref().ok());
    let (agent_bindings, agent_scan) = match agent_discovery {
        Ok(snapshot) => {
            let status = json!({
                "state": "loaded",
                "freshness": format!("unix-ms:{}", snapshot.observed_at_ms),
                "evidence_revision": snapshot.evidence_revision,
                "detected_count": snapshot.agents.len(),
            });
            (
                resolve_agent_bindings(&candidates, &snapshot.agents),
                status,
            )
        }
        Err(error) => (
            Vec::new(),
            json!({
                "state": "unavailable",
                "freshness": now_timestamp(),
                "reason": error.to_string(),
            }),
        ),
    };
    let selection_required = agent_bindings.iter().any(|binding| {
        binding.installed
            && matches!(
                binding.resolution.as_str(),
                "needs-choice" | "needs-credential"
            )
    });
    let cc_switch_sync = cc_switch_sync_state(&sources, &candidates);
    let revision = discovery_revision(&agent_bindings, &cc_switch_sync);
    let external_config_refresh = observe_external_config_snapshot(&external_config_snapshot);
    json!({
        "source": "dtoken-desktop-read-only-discovery",
        "freshness": now_timestamp(),
        "revision": revision,
        "state": if candidates.is_empty() && agent_bindings.is_empty() { "unavailable" } else { "loaded" },
        "candidates": candidates,
        "agent_bindings": agent_bindings,
        "agent_scan": agent_scan,
        "sources": sources,
        "selection_required": selection_required,
        "cc_switch_sync": cc_switch_sync,
        "external_config_snapshot": external_config_snapshot,
        "external_config_refresh": external_config_refresh,
        "writes_performed": false,
        "cc_switch_reference": {
            "repository": "https://github.com/farion1231/cc-switch",
            "commit": CC_SWITCH_COMMIT,
            "license": "MIT",
            "schema_version": CC_SWITCH_MIN_SCHEMA_VERSION
        },
        "privacy": "Reads configuration metadata only; auth.json and credential/api_key/token/auth/cookie values are never read or returned"
    })
}

/// Retains only safe file-metadata fingerprints between discovery runs. It
/// never authorizes a route/configuration write: after the debounce period a
/// caller receives `review_required`, and must explicitly produce a fresh
/// d-token-owned plan before it can apply anything.
fn observe_external_config_snapshot(
    snapshot: &ExternalConfigSnapshot,
) -> ExternalConfigRefreshEvent {
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX);
    let coordinator = EXTERNAL_CONFIG_REFRESH_COORDINATOR
        .get_or_init(|| Mutex::new(ExternalConfigRefreshCoordinator::default()));
    match coordinator.lock() {
        Ok(mut coordinator) => {
            let _poll_deadline_elapsed = coordinator.poll_due(now_ms);
            coordinator.observe(now_ms, snapshot.clone())
        }
        Err(_) => ExternalConfigRefreshEvent {
            state: "metadata_unavailable".to_string(),
            mode: "metadata_polling".to_string(),
            snapshot_fingerprint: snapshot.fingerprint.clone(),
            previous_fingerprint: None,
            changed_sources: Vec::new(),
            review_required: true,
            automatic_apply_allowed: false,
            next_poll_not_before_ms: now_ms,
        },
    }
}

fn external_config_snapshot(
    sources: &[Value],
    candidates: &[UpstreamCandidate],
    agent_discovery: Option<&dtoken_config_patcher::agent::AgentDiscoverySnapshot>,
) -> ExternalConfigSnapshot {
    let cc_switch_source = sources
        .iter()
        .find(|source| source.get("id").and_then(Value::as_str) == Some("cc-switch-sqlite"));
    let cc_switch = cc_switch_snapshot(cc_switch_source, candidates);
    let mut agent_configs = agent_discovery
        .into_iter()
        .flat_map(|snapshot| snapshot.agents.iter())
        .flat_map(|agent| {
            agent
                .evidence
                .iter()
                .filter(|evidence| evidence.kind == AgentEvidenceKind::Config)
                .flat_map(move |evidence| {
                    agent_config_files_for_snapshot(&agent.id, &evidence.path)
                        .into_iter()
                        .map(|path| agent_config_snapshot(&agent.id, &path))
                })
        })
        .collect::<Vec<_>>();
    agent_configs.sort_by(|left, right| {
        left.agent_id
            .cmp(&right.agent_id)
            .then(left.path.cmp(&right.path))
    });
    agent_configs.dedup_by(|left, right| {
        left.agent_id == right.agent_id
            && left.path == right.path
            && left.fingerprint == right.fingerprint
    });
    let observed_at = now_timestamp();
    let stable = json!({
        "version": EXTERNAL_CONFIG_SNAPSHOT_VERSION,
        "cc_switch": &cc_switch,
        "agent_configs": &agent_configs,
    });
    ExternalConfigSnapshot {
        version: EXTERNAL_CONFIG_SNAPSHOT_VERSION,
        mode: "metadata_polling".to_string(),
        poll_interval_ms: EXTERNAL_CONFIG_POLL_INTERVAL_MS,
        debounce_ms: EXTERNAL_CONFIG_DEBOUNCE_MS,
        observed_at,
        cc_switch,
        agent_configs,
        fingerprint: metadata_fingerprint("external-config", &stable),
        automatic_apply_allowed: false,
    }
}

fn cc_switch_snapshot(
    source: Option<&Value>,
    candidates: &[UpstreamCandidate],
) -> CcSwitchSnapshot {
    let source_state = CcSwitchSourceState::from_source(
        source
            .and_then(|source| source.get("state"))
            .and_then(Value::as_str),
    );
    let path = cc_switch_db_paths()
        .into_iter()
        .find(|candidate| candidate.is_file())
        .or_else(|| cc_switch_db_paths().into_iter().next());
    let files = path
        .as_deref()
        .map(cc_switch_file_metadata)
        .unwrap_or_default();
    let schema_version = source
        .and_then(|source| source.get("schema_version"))
        .and_then(Value::as_i64);
    let schema_kind = source
        .and_then(|source| source.get("schema_kind"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let mut app_types = candidates
        .iter()
        .filter_map(|candidate| candidate.source.strip_prefix("cc-switch:"))
        .filter(|source| *source != "proxy_config")
        .map(str::to_string)
        .collect::<Vec<_>>();
    app_types.sort();
    app_types.dedup();
    let mut active_upstreams = candidates
        .iter()
        .filter(|candidate| candidate.source.starts_with("cc-switch:") && candidate.active)
        .map(|candidate| CcSwitchActiveUpstreamSnapshot {
            app_type: candidate
                .source
                .strip_prefix("cc-switch:")
                .and_then(|source| {
                    (source == "proxy_config")
                        .then(|| candidate.agent_ids.first().cloned())
                        .flatten()
                        .or_else(|| Some(source.to_string()))
                })
                .unwrap_or_else(|| "unknown".to_string()),
            provider_id: candidate.id.clone(),
            profile_scope: "unknown".to_string(),
            model: (candidate.models.len() == 1).then(|| candidate.models[0].clone()),
            protocol: candidate.protocol.clone(),
            endpoint: candidate.base_url.as_deref().and_then(redact_base_url),
            credential_present: candidate.auth_mode != "none",
        })
        .collect::<Vec<_>>();
    active_upstreams.sort_by(|left, right| left.provider_id.cmp(&right.provider_id));
    let selection_state = if !matches!(source_state, CcSwitchSourceState::Loaded) {
        "unavailable".to_string()
    } else if active_upstreams.len() == 1 {
        "resolved".to_string()
    } else if active_upstreams.len() > 1 {
        "needs_selection".to_string()
    } else {
        "no_active_upstream".to_string()
    };
    let stable = json!({
        "source_state": &source_state,
        "schema_version": schema_version,
        "schema_kind": &schema_kind,
        "files": &files,
        "app_types": &app_types,
        "active_upstreams": &active_upstreams,
        "selection_state": &selection_state,
    });
    let fingerprint = metadata_fingerprint("cc-switch", &stable);
    CcSwitchSnapshot {
        source_state,
        schema_version,
        schema_kind,
        path: path.as_deref().map(display_path),
        files,
        app_types,
        active_upstreams,
        selection_state,
        revision: format!("cc-switch-{fingerprint}"),
        fingerprint,
        observed_at: now_timestamp(),
        mode: "metadata_polling".to_string(),
    }
}

fn agent_config_snapshot(agent_id: &str, path: &Path) -> AgentConfigSnapshot {
    agent_config_snapshot_with_reader(agent_id, path, |path| fs::read(path))
}

fn agent_config_snapshot_with_reader<F>(agent_id: &str, path: &Path, read: F) -> AgentConfigSnapshot
where
    F: FnOnce(&Path) -> std::io::Result<Vec<u8>>,
{
    let metadata = external_file_metadata("config", path);
    let content_digest = if agent_id == "cortex-code" {
        // Cortex connections.toml may contain credentials. Its watcher and
        // fingerprint contract is metadata-only: size/mtime/state are enough
        // to schedule a fresh, user-reviewed discovery without reading bytes.
        "metadata_only".to_string()
    } else {
        match read(path) {
            Ok(raw) if raw.len() <= 8 * 1024 * 1024 => {
                format!("sha256:{:x}", Sha256::digest(&raw))
            }
            Ok(_) => "too_large".to_string(),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => "missing".to_string(),
            Err(_) => "unavailable".to_string(),
        }
    };
    let stable = json!({
        "agent_id": agent_id,
        "metadata": &metadata,
        "content_digest": content_digest,
    });
    AgentConfigSnapshot {
        agent_id: agent_id.to_string(),
        state: metadata.state.clone(),
        path: display_path(path),
        fingerprint: metadata_fingerprint("agent-config", &stable),
    }
}

fn agent_config_files_for_snapshot(agent_id: &str, primary: &Path) -> Vec<PathBuf> {
    let mut files = vec![primary.to_path_buf()];
    if agent_id == "goose" {
        if let Ok(raw) = fs::read_to_string(primary) {
            if raw.len() <= 2 * 1024 * 1024 {
                if let Ok((Some(provider_id), _)) = parse_goose_selection_metadata(&raw) {
                    if safe_local_provider_id(&provider_id) {
                        if let Some(parent) = primary.parent() {
                            files.push(
                                parent
                                    .join("custom_providers")
                                    .join(format!("{provider_id}.json")),
                            );
                        }
                    }
                }
            }
        }
    }
    files.sort();
    files.dedup();
    files
}

fn cc_switch_file_metadata(path: &Path) -> Vec<ExternalFileMetadata> {
    let wal = PathBuf::from(format!("{}-wal", path.display()));
    let shm = PathBuf::from(format!("{}-shm", path.display()));
    vec![
        external_file_metadata("database", path),
        external_file_metadata("database-wal", &wal),
        external_file_metadata("database-shm", &shm),
    ]
}

fn external_file_metadata(name: &str, path: &Path) -> ExternalFileMetadata {
    match fs::metadata(path) {
        Ok(metadata) if metadata.is_file() => ExternalFileMetadata {
            name: name.to_string(),
            state: "present".to_string(),
            bytes: Some(metadata.len()),
            modified_unix_ms: metadata
                .modified()
                .ok()
                .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
                .and_then(|duration| u64::try_from(duration.as_millis()).ok()),
        },
        Ok(_) => ExternalFileMetadata {
            name: name.to_string(),
            state: "not_a_file".to_string(),
            bytes: None,
            modified_unix_ms: None,
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => ExternalFileMetadata {
            name: name.to_string(),
            state: "missing".to_string(),
            bytes: None,
            modified_unix_ms: None,
        },
        Err(_) => ExternalFileMetadata {
            name: name.to_string(),
            state: "metadata_unavailable".to_string(),
            bytes: None,
            modified_unix_ms: None,
        },
    }
}

fn metadata_fingerprint(prefix: &str, stable: &Value) -> String {
    format!(
        "{prefix}-{:x}",
        Sha256::digest(stable.to_string().as_bytes())
    )
}

fn changed_external_sources(
    previous: &ExternalConfigSnapshot,
    current: &ExternalConfigSnapshot,
) -> Vec<String> {
    let mut changed = Vec::new();
    if previous.cc_switch.fingerprint != current.cc_switch.fingerprint {
        changed.push("cc_switch".to_string());
    }
    if previous.agent_configs != current.agent_configs {
        changed.push("agent_configs".to_string());
    }
    changed
}

fn discover_opencode_catalog(candidates: &mut Vec<UpstreamCandidate>, sources: &mut Vec<Value>) {
    if let Some(path) = opencode_models_cache_path() {
        if let Ok(raw) = fs::read(&path) {
            let models = parse_opencode_free_models(&raw);
            if !models.is_empty() {
                candidates.push(UpstreamCandidate {
                    id: "opencode".to_string(),
                    name: "OpenCode official free catalog".to_string(),
                    protocol: "open_ai".to_string(),
                    base_url: Some("https://opencode.ai/zen/v1".to_string()),
                    models,
                    source: "opencode-provider-catalog-cache".to_string(),
                    freshness: now_timestamp(),
                    auth_mode: "none".to_string(),
                    active: false,
                    port: Some(443),
                    state: "available".to_string(),
                    agent_ids: vec!["opencode".to_string()],
                    confidence: 85,
                    reasons: vec![
                        "OpenCode's local official catalog reports active zero-cost models"
                            .to_string(),
                    ],
                });
                sources.push(json!({
                    "id":"opencode-provider-catalog",
                    "state":"loaded",
                    "freshness":now_timestamp(),
                    "source":"opencode-local-cache",
                    "path":display_path(&path)
                }));
                return;
            }
        }
    }
    let output =
        run_capture_with_timeout("opencode", &["models", "opencode"], Duration::from_secs(6));
    match output {
        Ok((true, stdout)) => {
            let models = stdout
                .lines()
                .map(str::trim)
                .filter_map(|line| line.strip_prefix("opencode/"))
                .filter(|model| !model.is_empty())
                .map(str::to_string)
                .collect::<Vec<_>>();
            if !models.is_empty() {
                candidates.push(UpstreamCandidate {
                    id: "opencode".to_string(),
                    name: "OpenCode official free catalog".to_string(),
                    protocol: "open_ai".to_string(),
                    base_url: Some("https://opencode.ai/zen/v1".to_string()),
                    models,
                    source: "opencode-provider-catalog".to_string(),
                    freshness: now_timestamp(),
                    auth_mode: "none".to_string(),
                    active: false,
                    port: Some(443),
                    state: "available".to_string(),
                    agent_ids: vec!["opencode".to_string()],
                    confidence: 70,
                    reasons: vec!["Official OpenCode free catalog reported this model".to_string()],
                });
            }
            sources.push(json!({"id":"opencode-provider-catalog","state":"loaded","freshness":now_timestamp()}));
        }
        Ok((false, _)) => sources.push(json!({"id":"opencode-provider-catalog","state":"unavailable","freshness":now_timestamp()})),
        Err(error) => sources.push(json!({"id":"opencode-provider-catalog","state":if error == "timeout" { "timeout" } else { "missing" },"freshness":now_timestamp()})),
    }
}

fn opencode_models_cache_path() -> Option<PathBuf> {
    if let Some(path) = env::var_os("DTOKEN_OPENCODE_MODELS_CACHE") {
        return Some(PathBuf::from(path));
    }
    #[cfg(windows)]
    {
        env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .map(|root| root.join("opencode").join("models.json"))
    }
    #[cfg(not(windows))]
    {
        env::var_os("XDG_CACHE_HOME")
            .map(PathBuf::from)
            .or_else(|| {
                env::var_os("HOME")
                    .map(PathBuf::from)
                    .map(|home| home.join(".cache"))
            })
            .map(|root| root.join("opencode").join("models.json"))
    }
}

fn parse_opencode_free_models(raw: &[u8]) -> Vec<String> {
    if raw.len() > 8 * 1024 * 1024 {
        return vec![];
    }
    let Ok(value) = serde_json::from_slice::<Value>(raw) else {
        return vec![];
    };
    let Some(models) = value
        .get("opencode")
        .and_then(|provider| provider.get("models"))
        .and_then(Value::as_object)
    else {
        return vec![];
    };
    let mut free = models
        .iter()
        .filter_map(|(id, model)| {
            let active = model.get("status").and_then(Value::as_str) != Some("deprecated");
            let input_free = model
                .pointer("/cost/input")
                .and_then(Value::as_f64)
                .is_some_and(|cost| cost == 0.0);
            let output_free = model
                .pointer("/cost/output")
                .and_then(Value::as_f64)
                .is_some_and(|cost| cost == 0.0);
            (active && input_free && output_free).then(|| id.clone())
        })
        .collect::<Vec<_>>();
    free.sort();
    free
}

fn candidates_from_opencode_config(path: &Path) -> Result<Vec<UpstreamCandidate>, &'static str> {
    let metadata = fs::metadata(path).map_err(|_| "unavailable")?;
    if !metadata.is_file() || metadata.len() > 2 * 1024 * 1024 {
        return Err("too_large");
    }
    let file = fs::File::open(path).map_err(|_| "unavailable")?;
    let value: OpenCodeConfigMetadata =
        serde_json::from_reader(file).map_err(|_| "unsupported_schema")?;
    let selected_model = value.model;
    let mut candidates = Vec::new();
    for (id, provider) in value.provider {
        if !safe_local_provider_id(&id) {
            continue;
        }
        let active_model = selected_model
            .as_deref()
            .and_then(|model| model.strip_prefix(&format!("{id}/")));
        let active = active_model.is_some();
        let base_url = provider.base_url().as_deref().and_then(redact_base_url);
        let mut models = provider.models.keys().cloned().collect::<Vec<_>>();
        if let Some(model) = active_model.filter(|model| safe_model_id(model)) {
            models = vec![model.to_string()];
        }
        models.sort();
        models.dedup();
        if base_url.is_none() && models.is_empty() {
            continue;
        }
        let protocol = provider.protocol();
        let credential_present = provider.credential_present();
        candidates.push(UpstreamCandidate {
            id: id.clone(),
            name: format!("OpenCode {id}"),
            protocol: protocol.to_string(),
            base_url,
            models,
            source: "opencode-normal-config".to_string(),
            freshness: now_timestamp(),
            auth_mode: if credential_present {
                "unavailable"
            } else {
                "none"
            }
            .to_string(),
            active,
            port: None,
            state: "discovered".to_string(),
            agent_ids: vec!["opencode".to_string()],
            confidence: if active { 98 } else { 80 },
            reasons: vec![if active {
                "OpenCode currently selects this explicit provider/model block".to_string()
            } else {
                "Provider is present in the normal OpenCode configuration".to_string()
            }],
        });
    }
    Ok(candidates)
}

#[derive(Debug, Deserialize)]
struct OpenCodeConfigMetadata {
    model: Option<String>,
    #[serde(default)]
    provider: BTreeMap<String, OpenCodeProviderMetadata>,
}

#[derive(Debug, Deserialize)]
struct OpenCodeProviderMetadata {
    #[serde(default, rename = "baseURL", alias = "baseUrl", alias = "base_url")]
    base_url: Option<String>,
    #[serde(default)]
    options: Option<OpenCodeProviderOptionsMetadata>,
    #[serde(default)]
    models: BTreeMap<String, IgnoredAny>,
    #[serde(default)]
    protocol: Option<String>,
    #[serde(default)]
    api: Option<String>,
    #[serde(default)]
    npm: Option<String>,
    #[serde(default, rename = "apiKey", alias = "api_key")]
    api_key: SecretPresence,
    #[serde(default)]
    headers: SecretPresence,
    #[serde(default)]
    auth: SecretPresence,
    #[serde(default, rename = "requiresAuth", alias = "requires_auth")]
    requires_auth: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct OpenCodeProviderOptionsMetadata {
    #[serde(default, rename = "baseURL", alias = "baseUrl", alias = "base_url")]
    base_url: Option<String>,
    #[serde(default, rename = "apiKey", alias = "api_key")]
    api_key: SecretPresence,
    #[serde(default)]
    headers: SecretPresence,
    #[serde(default)]
    auth: SecretPresence,
}

impl OpenCodeProviderMetadata {
    fn base_url(&self) -> Option<String> {
        self.options
            .as_ref()
            .and_then(|options| options.base_url.clone())
            .or_else(|| self.base_url.clone())
    }

    fn protocol(&self) -> &'static str {
        self.protocol
            .as_deref()
            .or(self.api.as_deref())
            .and_then(verified_protocol)
            .or_else(|| {
                self.npm.as_deref().and_then(|npm| {
                    let normalized = npm.to_ascii_lowercase();
                    if normalized.contains("anthropic") {
                        Some("anthropic")
                    } else if normalized.contains("google") {
                        Some("google_generative_ai")
                    } else if normalized.contains("openai") {
                        Some("open_ai")
                    } else {
                        None
                    }
                })
            })
            .unwrap_or("unknown")
    }

    fn credential_present(&self) -> bool {
        self.requires_auth == Some(true)
            || self.api_key == SecretPresence::Present
            || self.headers == SecretPresence::Present
            || self.auth == SecretPresence::Present
            || self.options.as_ref().is_some_and(|options| {
                options.api_key == SecretPresence::Present
                    || options.headers == SecretPresence::Present
                    || options.auth == SecretPresence::Present
            })
    }
}

fn discover_agent_configs(
    detected_agents: &[dtoken_config_patcher::agent::DetectedAgent],
    candidates: &mut Vec<UpstreamCandidate>,
    sources: &mut Vec<Value>,
) {
    for detected in detected_agents {
        let Some(spec) = agent_descriptor(&detected.id).copied() else {
            continue;
        };
        let config_paths = detected
            .evidence
            .iter()
            .filter(|evidence| evidence.kind == AgentEvidenceKind::Config)
            .map(|evidence| evidence.path.as_path())
            .collect::<Vec<_>>();
        if config_paths.is_empty() {
            sources.push(json!({
                "id": format!("agent-config:{}", spec.id),
                "agent_id": spec.id,
                "state": "missing",
                "freshness": now_timestamp()
            }));
            continue;
        }
        for (index, path) in config_paths.iter().enumerate() {
            let extension = path.extension().and_then(|item| item.to_str());
            let result = if spec.id == "opencode" && extension == Some("json") {
                candidates_from_opencode_config(path)
            } else if spec.id == "claude-code"
                && path.file_name().and_then(|name| name.to_str()) == Some("settings.json")
            {
                candidate_from_claude_settings(path)
                    .map(|candidate| candidate.into_iter().collect::<Vec<_>>())
            } else if spec.id == "claude-code" {
                Err("unsupported_schema")
            } else if spec.id == "cline"
                && path.file_name().and_then(|name| name.to_str()) == Some("providers.json")
            {
                candidates_from_cline_config(path)
            } else if spec.id == "openclaw" && extension == Some("json") {
                candidates_from_openclaw_config(path)
            } else if spec.id == "goose" && matches!(extension, Some("yaml") | Some("yml")) {
                candidate_from_goose_config(path)
                    .map(|candidate| candidate.into_iter().collect::<Vec<_>>())
            } else if spec.id == "kilo-code" && extension == Some("json") {
                candidate_from_kilo_config(path)
                    .map(|candidate| candidate.into_iter().collect::<Vec<_>>())
            } else if spec.id == "codex" && extension == Some("toml") {
                candidate_from_codex_toml(path)
                    .map(|candidate| candidate.into_iter().collect::<Vec<_>>())
            } else {
                Err(
                    if matches!(
                        extension,
                        Some("json") | Some("toml") | Some("yaml") | Some("yml")
                    ) {
                        "unsupported_schema"
                    } else {
                        "unsupported_format"
                    },
                )
            };
            let source_id = format!("agent-config:{}:{}", spec.id, index + 1);
            match result {
                Ok(mut discovered) => {
                    if config_paths.len() > 1 {
                        for candidate in &mut discovered {
                            candidate.id = format!("{}-config-{}", candidate.id, index + 1);
                        }
                    }
                    let state = if discovered.is_empty() {
                        "loaded_no_safe_upstream"
                    } else {
                        "loaded"
                    };
                    candidates.extend(discovered);
                    sources.push(json!({
                        "id": source_id,
                        "agent_id": spec.id,
                        "state": state,
                        "freshness": now_timestamp(),
                        "path": display_path(path)
                    }));
                }
                Err(state) => sources.push(json!({
                    "id": source_id,
                    "agent_id": spec.id,
                    "state": state,
                    "freshness": now_timestamp(),
                    "path": display_path(path)
                })),
            }
        }
    }
}

fn candidate_from_claude_settings(path: &Path) -> Result<Option<UpstreamCandidate>, &'static str> {
    let metadata = fs::metadata(path).map_err(|_| "unavailable")?;
    if !metadata.is_file() || metadata.len() > 2 * 1024 * 1024 {
        return Err("too_large");
    }
    let file = fs::File::open(path).map_err(|_| "unavailable")?;
    let value: ClaudeSettingsMetadata =
        serde_json::from_reader(file).map_err(|_| "unsupported_schema")?;
    let Some(env) = value.env else {
        return Ok(None);
    };
    let base_url = env.base_url.as_deref().and_then(redact_base_url);
    let model = env
        .model
        .as_deref()
        .filter(|model| !model.trim().is_empty())
        .map(str::to_string);
    if base_url.is_none() || model.is_none() {
        return Ok(None);
    }
    let credential_present =
        env.api_key == SecretPresence::Present || env.auth_token == SecretPresence::Present;
    Ok(Some(UpstreamCandidate {
        id: "claude-code-current".to_string(),
        name: "Claude Code current upstream".to_string(),
        protocol: "anthropic".to_string(),
        base_url,
        models: model.into_iter().collect(),
        source: "agent-config:claude-code-settings-v1".to_string(),
        freshness: now_timestamp(),
        auth_mode: if credential_present {
            "unavailable"
        } else {
            "none"
        }
        .to_string(),
        active: true,
        port: None,
        state: "discovered".to_string(),
        agent_ids: vec!["claude-code".to_string()],
        confidence: 99,
        reasons: vec![
            "Claude Code settings.env explicitly selects this endpoint and model".to_string(),
        ],
    }))
}

#[derive(Debug, Deserialize)]
struct ClaudeSettingsMetadata {
    env: Option<ClaudeEnvironmentMetadata>,
}

#[derive(Debug, Deserialize)]
struct ClaudeEnvironmentMetadata {
    #[serde(rename = "ANTHROPIC_BASE_URL")]
    base_url: Option<String>,
    #[serde(rename = "ANTHROPIC_MODEL")]
    model: Option<String>,
    #[serde(default, rename = "ANTHROPIC_API_KEY")]
    api_key: SecretPresence,
    #[serde(default, rename = "ANTHROPIC_AUTH_TOKEN")]
    auth_token: SecretPresence,
}

fn candidates_from_cline_config(path: &Path) -> Result<Vec<UpstreamCandidate>, &'static str> {
    let metadata = fs::metadata(path).map_err(|_| "unavailable")?;
    if metadata.len() > 2 * 1024 * 1024 {
        return Err("too_large");
    }
    let file = fs::File::open(path).map_err(|_| "unavailable")?;
    let value: ClineConfigMetadata = serde_json::from_reader(file).map_err(|_| "invalid")?;
    if value.version != Some(1) {
        return Err("unsupported_schema");
    }
    let mut candidates = Vec::new();
    for (provider_id, provider) in value.providers {
        let Some(settings) = provider.settings else {
            continue;
        };
        let base_url = settings.base_url.as_deref().and_then(redact_base_url);
        let model = settings.model.filter(|model| !model.trim().is_empty());
        if base_url.is_none() && model.is_none() {
            continue;
        }
        let active = value.last_used_provider.as_deref() == Some(provider_id.as_str());
        let has_credential = settings.api_key == ClineSecretState::Present
            || settings.auth == ClineSecretState::Present;
        candidates.push(UpstreamCandidate {
            id: format!("cline-{provider_id}"),
            name: format!("Cline {provider_id}"),
            protocol: "open_ai".to_string(),
            base_url,
            models: model.into_iter().collect(),
            source: "agent-config:cline-cli-v1".to_string(),
            freshness: now_timestamp(),
            auth_mode: if has_credential {
                "unavailable"
            } else {
                "none"
            }
            .to_string(),
            active,
            port: None,
            state: "discovered".to_string(),
            agent_ids: vec!["cline".to_string()],
            confidence: if active { 99 } else { 82 },
            reasons: vec![if active {
                "Cline CLI schema v1 currently selects this provider/model".to_string()
            } else {
                "Provider is present in the Cline CLI schema v1 configuration".to_string()
            }],
        });
    }
    Ok(candidates)
}

#[derive(Debug, Deserialize)]
struct ClineConfigMetadata {
    version: Option<i64>,
    #[serde(default, rename = "lastUsedProvider")]
    last_used_provider: Option<String>,
    #[serde(default)]
    providers: BTreeMap<String, ClineProviderMetadata>,
}

#[derive(Debug, Deserialize)]
struct ClineProviderMetadata {
    settings: Option<ClineProviderSettingsMetadata>,
}

#[derive(Debug, Deserialize)]
struct ClineProviderSettingsMetadata {
    model: Option<String>,
    #[serde(default, rename = "baseUrl")]
    base_url: Option<String>,
    #[serde(default, rename = "apiKey")]
    api_key: ClineSecretState,
    #[serde(default)]
    auth: ClineSecretState,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum SecretPresence {
    #[default]
    Absent,
    Present,
}

impl<'de> Deserialize<'de> for SecretPresence {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(SecretPresenceVisitor)
    }
}

struct SecretPresenceVisitor;

impl<'de> Visitor<'de> for SecretPresenceVisitor {
    type Value = SecretPresence;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("an ignored credential-like value")
    }

    fn visit_none<E>(self) -> Result<Self::Value, E> {
        Ok(SecretPresence::Absent)
    }

    fn visit_unit<E>(self) -> Result<Self::Value, E> {
        Ok(SecretPresence::Absent)
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E> {
        Ok(if value.trim().is_empty() {
            SecretPresence::Absent
        } else {
            SecretPresence::Present
        })
    }

    fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        self.visit_str(&value)
    }

    fn visit_bool<E>(self, value: bool) -> Result<Self::Value, E> {
        Ok(if value {
            SecretPresence::Present
        } else {
            SecretPresence::Absent
        })
    }

    fn visit_i64<E>(self, _value: i64) -> Result<Self::Value, E> {
        Ok(SecretPresence::Present)
    }

    fn visit_u64<E>(self, _value: u64) -> Result<Self::Value, E> {
        Ok(SecretPresence::Present)
    }

    fn visit_f64<E>(self, _value: f64) -> Result<Self::Value, E> {
        Ok(SecretPresence::Present)
    }

    fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let mut present = false;
        while sequence.next_element::<IgnoredAny>()?.is_some() {
            present = true;
        }
        Ok(if present {
            SecretPresence::Present
        } else {
            SecretPresence::Absent
        })
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut present = false;
        while map.next_entry::<IgnoredAny, IgnoredAny>()?.is_some() {
            present = true;
        }
        Ok(if present {
            SecretPresence::Present
        } else {
            SecretPresence::Absent
        })
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum ClineSecretState {
    #[default]
    Absent,
    DtokenMarker,
    Present,
}

impl<'de> Deserialize<'de> for ClineSecretState {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(ClineSecretStateVisitor)
    }
}

#[derive(Debug, Deserialize)]
struct GooseProviderMetadata {
    engine: Option<String>,
    #[serde(default)]
    base_url: Option<String>,
    #[serde(default)]
    api_key_env: SecretPresence,
    #[serde(default)]
    headers: SecretPresence,
    #[serde(default)]
    env_vars: SecretPresence,
    #[serde(default)]
    requires_auth: bool,
}

#[derive(Debug, Deserialize)]
struct KiloConfigMetadata {
    model: Option<String>,
    #[serde(default)]
    provider: BTreeMap<String, KiloProviderMetadata>,
}

#[derive(Debug, Deserialize)]
struct KiloProviderMetadata {
    #[serde(default)]
    options: Option<KiloProviderOptionsMetadata>,
    #[serde(default)]
    protocol: Option<String>,
    #[serde(default)]
    api: Option<String>,
}

#[derive(Debug, Deserialize)]
struct KiloProviderOptionsMetadata {
    #[serde(default, rename = "baseURL", alias = "baseUrl", alias = "base_url")]
    base_url: Option<String>,
    #[serde(default, rename = "apiKey", alias = "api_key")]
    api_key: SecretPresence,
    #[serde(default)]
    headers: SecretPresence,
    #[serde(default)]
    auth: SecretPresence,
}

fn candidate_from_goose_config(path: &Path) -> Result<Option<UpstreamCandidate>, &'static str> {
    let raw = fs::read_to_string(path).map_err(|_| "unavailable")?;
    if raw.len() > 2 * 1024 * 1024 {
        return Err("too_large");
    }
    let (provider_id, model) = parse_goose_selection_metadata(&raw)?;
    let (Some(provider_id), Some(model)) = (provider_id, model) else {
        return Ok(None);
    };
    if !safe_local_provider_id(&provider_id) || !safe_model_id(&model) {
        return Err("invalid");
    }
    let provider_path = path
        .parent()
        .ok_or("invalid")?
        .join("custom_providers")
        .join(format!("{provider_id}.json"));
    let metadata = fs::metadata(&provider_path).map_err(|_| "provider_metadata_unavailable")?;
    if !metadata.is_file() || metadata.len() > 2 * 1024 * 1024 {
        return Err("provider_metadata_unavailable");
    }
    let file = fs::File::open(&provider_path).map_err(|_| "provider_metadata_unavailable")?;
    let provider: GooseProviderMetadata =
        serde_json::from_reader(file).map_err(|_| "unsupported_schema")?;
    let base_url = provider
        .base_url
        .as_deref()
        .and_then(redact_base_url)
        .ok_or("missing_endpoint")?;
    let protocol = provider
        .engine
        .as_deref()
        .and_then(verified_protocol)
        .ok_or("unsupported_protocol")?;
    let credential_present = provider.requires_auth
        || provider.api_key_env == SecretPresence::Present
        || provider.headers == SecretPresence::Present
        || provider.env_vars == SecretPresence::Present;
    Ok(Some(UpstreamCandidate {
        id: format!("goose-{provider_id}"),
        name: format!("Goose {provider_id}"),
        protocol: protocol.to_string(),
        base_url: Some(base_url),
        models: vec![model],
        source: "agent-config:goose-v1".to_string(),
        freshness: now_timestamp(),
        auth_mode: if credential_present {
            "unavailable"
        } else {
            "none"
        }
        .to_string(),
        active: true,
        port: None,
        state: "discovered".to_string(),
        agent_ids: vec!["goose".to_string()],
        confidence: 99,
        reasons: vec![
            "Goose top-level provider/model selection resolves to this custom provider".to_string(),
        ],
    }))
}

fn parse_goose_selection_metadata(
    raw: &str,
) -> Result<(Option<String>, Option<String>), &'static str> {
    let mut provider = None;
    let mut model = None;
    for line in raw.lines() {
        if line.trim().is_empty() || line.trim_start().starts_with('#') {
            continue;
        }
        if line.len() != line.trim_start().len() {
            continue;
        }
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let target = match key.trim() {
            "GOOSE_PROVIDER" => &mut provider,
            "GOOSE_MODEL" => &mut model,
            _ => continue,
        };
        if target.is_some() {
            return Err("invalid");
        }
        let value = unquote_metadata_scalar(value.trim());
        if !value.is_empty() {
            *target = Some(value);
        }
    }
    Ok((provider, model))
}

fn candidate_from_kilo_config(path: &Path) -> Result<Option<UpstreamCandidate>, &'static str> {
    let metadata = fs::metadata(path).map_err(|_| "unavailable")?;
    if !metadata.is_file() || metadata.len() > 2 * 1024 * 1024 {
        return Err("too_large");
    }
    let file = fs::File::open(path).map_err(|_| "unavailable")?;
    let config: KiloConfigMetadata =
        serde_json::from_reader(file).map_err(|_| "unsupported_schema")?;
    let Some(selection) = config.model.as_deref() else {
        return Ok(None);
    };
    let Some((provider_id, model)) = selection.split_once('/') else {
        return Err("unsupported_schema");
    };
    if !safe_local_provider_id(provider_id) || !safe_model_id(model) {
        return Err("invalid");
    }
    let provider = config
        .provider
        .get(provider_id)
        .ok_or("provider_metadata_unavailable")?;
    let options = provider
        .options
        .as_ref()
        .ok_or("provider_metadata_unavailable")?;
    let base_url = options
        .base_url
        .as_deref()
        .and_then(redact_base_url)
        .ok_or("missing_endpoint")?;
    let protocol = provider
        .protocol
        .as_deref()
        .or(provider.api.as_deref())
        .and_then(verified_protocol)
        .unwrap_or("open_ai");
    let credential_present = options.api_key == SecretPresence::Present
        || options.headers == SecretPresence::Present
        || options.auth == SecretPresence::Present;
    Ok(Some(UpstreamCandidate {
        id: format!("kilo-code-{provider_id}"),
        name: format!("Kilo Code {provider_id}"),
        protocol: protocol.to_string(),
        base_url: Some(base_url),
        models: vec![model.to_string()],
        source: "agent-config:kilo-code-v1".to_string(),
        freshness: now_timestamp(),
        auth_mode: if credential_present {
            "unavailable"
        } else {
            "none"
        }
        .to_string(),
        active: true,
        port: None,
        state: "discovered".to_string(),
        agent_ids: vec!["kilo-code".to_string()],
        confidence: 99,
        reasons: vec!["Kilo Code explicitly selects this provider/model block".to_string()],
    }))
}

fn verified_protocol(value: &str) -> Option<&'static str> {
    match value.trim().to_ascii_lowercase().as_str() {
        "openai" | "open_ai" | "openai-compatible" | "openai-completions" => Some("open_ai"),
        "openai-responses" | "responses" => Some("open_ai_responses"),
        "anthropic" | "anthropic-messages" => Some("anthropic"),
        "google" | "google-generative-ai" => Some("google_generative_ai"),
        _ => None,
    }
}

fn safe_local_provider_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        && value != "."
        && value != ".."
}

fn safe_model_id(value: &str) -> bool {
    !value.trim().is_empty()
        && value.len() <= 256
        && !value.contains(['\0', '\n', '\r'])
        && !value.contains("://")
}

fn unquote_metadata_scalar(value: &str) -> String {
    if value.len() >= 2 {
        let bytes = value.as_bytes();
        if (bytes[0] == b'"' && bytes[value.len() - 1] == b'"')
            || (bytes[0] == b'\'' && bytes[value.len() - 1] == b'\'')
        {
            return value[1..value.len() - 1].to_string();
        }
    }
    value.to_string()
}

struct ClineSecretStateVisitor;

impl<'de> Visitor<'de> for ClineSecretStateVisitor {
    type Value = ClineSecretState;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("an ignored credential value")
    }

    fn visit_none<E>(self) -> Result<Self::Value, E> {
        Ok(ClineSecretState::Absent)
    }

    fn visit_unit<E>(self) -> Result<Self::Value, E> {
        Ok(ClineSecretState::Absent)
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E> {
        Ok(match value {
            "" => ClineSecretState::Absent,
            "dtoken-loopback" => ClineSecretState::DtokenMarker,
            _ => ClineSecretState::Present,
        })
    }

    fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        self.visit_str(&value)
    }

    fn visit_bool<E>(self, _value: bool) -> Result<Self::Value, E> {
        Ok(ClineSecretState::Present)
    }

    fn visit_i64<E>(self, _value: i64) -> Result<Self::Value, E> {
        Ok(ClineSecretState::Present)
    }

    fn visit_u64<E>(self, _value: u64) -> Result<Self::Value, E> {
        Ok(ClineSecretState::Present)
    }

    fn visit_f64<E>(self, _value: f64) -> Result<Self::Value, E> {
        Ok(ClineSecretState::Present)
    }

    fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        while sequence.next_element::<IgnoredAny>()?.is_some() {}
        Ok(ClineSecretState::Present)
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        while map.next_entry::<IgnoredAny, IgnoredAny>()?.is_some() {}
        Ok(ClineSecretState::Present)
    }
}

fn candidates_from_openclaw_config(path: &Path) -> Result<Vec<UpstreamCandidate>, &'static str> {
    let raw = fs::read_to_string(path).map_err(|_| "unavailable")?;
    if raw.len() > 2 * 1024 * 1024 {
        return Err("too_large");
    }
    let value: OpenClawConfigMetadata = json5::from_str(&raw).map_err(|_| "unsupported_schema")?;
    let primary = value
        .agents
        .as_ref()
        .and_then(|agents| agents.defaults.as_ref())
        .and_then(|defaults| defaults.model.as_ref())
        .and_then(|model| model.primary.as_deref());
    let active_provider =
        primary.and_then(|model| model.split_once('/').map(|(provider, _)| provider));
    let active_model = primary.and_then(|model| model.split_once('/').map(|(_, model)| model));
    let Some(models) = value.models else {
        return Ok(Vec::new());
    };
    let mut candidates = Vec::new();
    for (provider_id, provider) in models.providers {
        if !safe_local_provider_id(&provider_id) {
            continue;
        }
        let base_url = provider.base_url.as_deref().and_then(redact_base_url);
        let protocol = provider
            .api
            .as_deref()
            .and_then(verified_protocol)
            .unwrap_or("unknown")
            .to_string();
        let active = active_provider == Some(provider_id.as_str());
        let mut models = provider
            .models
            .into_iter()
            .filter_map(|model| model.id)
            .filter(|model| safe_model_id(model))
            .collect::<Vec<_>>();
        if active {
            if let Some(active_model) = active_model {
                models = vec![active_model.to_string()];
            }
        }
        models.sort();
        models.dedup();
        if base_url.is_none() && models.is_empty() {
            continue;
        }
        let has_credential = provider.api_key == SecretPresence::Present
            || provider.headers == SecretPresence::Present
            || provider.auth == SecretPresence::Present;
        candidates.push(UpstreamCandidate {
            id: format!("openclaw-{provider_id}"),
            name: format!("OpenClaw {provider_id}"),
            protocol,
            base_url,
            models,
            source: "agent-config:openclaw-json5".to_string(),
            freshness: now_timestamp(),
            auth_mode: if has_credential {
                "unavailable"
            } else {
                "none"
            }
            .to_string(),
            active,
            port: None,
            state: "discovered".to_string(),
            agent_ids: vec!["openclaw".to_string()],
            confidence: if active { 99 } else { 82 },
            reasons: vec![if active {
                "OpenClaw currently selects this provider/model".to_string()
            } else {
                "Provider is present in the OpenClaw multi-provider configuration".to_string()
            }],
        });
    }
    Ok(candidates)
}

#[derive(Debug, Deserialize)]
struct OpenClawConfigMetadata {
    models: Option<OpenClawModelsMetadata>,
    agents: Option<OpenClawAgentsMetadata>,
}

#[derive(Debug, Deserialize)]
struct OpenClawModelsMetadata {
    #[serde(default)]
    providers: BTreeMap<String, OpenClawProviderMetadata>,
}

#[derive(Debug, Deserialize)]
struct OpenClawProviderMetadata {
    #[serde(default, rename = "baseUrl", alias = "baseURL", alias = "base_url")]
    base_url: Option<String>,
    api: Option<String>,
    #[serde(default)]
    models: Vec<OpenClawModelMetadata>,
    #[serde(default, rename = "apiKey", alias = "api_key")]
    api_key: SecretPresence,
    #[serde(default)]
    headers: SecretPresence,
    #[serde(default)]
    auth: SecretPresence,
}

#[derive(Debug, Deserialize)]
struct OpenClawModelMetadata {
    id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenClawAgentsMetadata {
    defaults: Option<OpenClawDefaultsMetadata>,
}

#[derive(Debug, Deserialize)]
struct OpenClawDefaultsMetadata {
    model: Option<OpenClawDefaultModelMetadata>,
}

#[derive(Debug, Deserialize)]
struct OpenClawDefaultModelMetadata {
    primary: Option<String>,
}

/// Outcome of resolving the active Codex profile.
enum CodexProfileResolution<'a> {
    /// No profiles (or an empty table): use the root-level selection.
    InheritRoot,
    /// An unambiguous active profile: `[profiles.default]` or the only profile.
    Profile(&'a toml::Value),
    /// Multiple profiles without an explicit default: ambiguous, fail closed.
    Ambiguous,
}

/// Resolves the effective Codex profile without reading auth: `default` wins
/// when present, otherwise a single profile is unambiguous and treated as
/// active; multiple profiles with no explicit default are ambiguous because
/// Codex may select one externally (e.g. `codex --profile <name>`).
fn resolve_codex_profile<'a>(value: &'a toml::Value) -> CodexProfileResolution<'a> {
    let Some(profiles) = value.get("profiles").and_then(toml::Value::as_table) else {
        return CodexProfileResolution::InheritRoot;
    };
    if let Some(default) = profiles.get("default") {
        return CodexProfileResolution::Profile(default);
    }
    if profiles.len() == 1 {
        return CodexProfileResolution::Profile(profiles.values().next().expect("non-empty"));
    }
    if profiles.is_empty() {
        return CodexProfileResolution::InheritRoot;
    }
    CodexProfileResolution::Ambiguous
}

fn candidate_from_codex_toml(path: &Path) -> Result<Option<UpstreamCandidate>, &'static str> {
    let raw = fs::read_to_string(path).map_err(|_| "unavailable")?;
    if raw.len() > 2 * 1024 * 1024 {
        return Err("too_large");
    }
    let value: toml::Value = toml::from_str(&raw).map_err(|_| "invalid")?;
    // Root-level selection: preserved as the fallback for profiles that do
    // not declare their own provider/model.
    let root_provider_id = value
        .get("model_provider")
        .and_then(toml::Value::as_str)
        .unwrap_or("openai");
    let root_model = value
        .get("model")
        .and_then(toml::Value::as_str)
        .map(str::to_string);
    // Real configs put the active selection under [profiles.default] or a
    // single active profile. A profile's model_provider/model overrides the
    // root selection; keys it does not declare inherit the root. Multiple
    // profiles without an explicit default are ambiguous, so discovery fails
    // closed and reports no candidate instead of guessing the active provider.
    let active_profile = match resolve_codex_profile(&value) {
        CodexProfileResolution::Ambiguous => return Ok(None),
        CodexProfileResolution::Profile(profile) => Some(profile),
        CodexProfileResolution::InheritRoot => None,
    };
    let provider_id = active_profile
        .and_then(|profile| profile.get("model_provider").and_then(toml::Value::as_str))
        .unwrap_or(root_provider_id)
        .to_string();
    let model = active_profile
        .and_then(|profile| profile.get("model").and_then(toml::Value::as_str))
        .map(str::to_string)
        .or(root_model);
    let provider = value
        .get("model_providers")
        .and_then(toml::Value::as_table)
        .and_then(|providers| providers.get(&provider_id));
    let base_url = provider
        .and_then(|provider| provider.get("base_url"))
        .and_then(toml::Value::as_str)
        .and_then(redact_base_url);
    let protocol = provider
        .and_then(|provider| provider.get("wire_api"))
        .and_then(toml::Value::as_str)
        .unwrap_or("responses")
        .to_string();
    let has_env_key = provider
        .and_then(|provider| provider.get("env_key"))
        .and_then(toml::Value::as_str)
        .is_some();
    // Real Codex providers declare requires_openai_auth explicitly; when true
    // the OpenAI credential cannot be reused by d-token, so report a credential
    // gap even without an env_key (fail-closed).
    let requires_openai_auth = provider
        .and_then(|provider| provider.get("requires_openai_auth"))
        .and_then(toml::Value::as_bool)
        .unwrap_or(false);
    if base_url.is_none() && model.is_none() {
        return Ok(None);
    }
    Ok(Some(UpstreamCandidate {
        id: format!("codex-{provider_id}"),
        name: format!("Codex {provider_id}"),
        protocol,
        base_url,
        models: model.into_iter().collect(),
        source: "agent-config:codex".to_string(),
        freshness: now_timestamp(),
        auth_mode: if has_env_key || requires_openai_auth {
            "unavailable"
        } else {
            "none"
        }
        .to_string(),
        active: true,
        port: None,
        state: "discovered".to_string(),
        agent_ids: vec!["codex".to_string()],
        confidence: 96,
        reasons: vec!["Codex currently selects this model provider".to_string()],
    }))
}

fn discover_cc_switch(candidates: &mut Vec<UpstreamCandidate>, sources: &mut Vec<Value>) {
    let paths = cc_switch_db_paths();
    let mut metadata_unavailable = None;
    for path in &paths {
        match fs::metadata(path) {
            Ok(metadata) if metadata.is_file() => {
                discover_cc_switch_at(path, candidates, sources);
                return;
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => {
                metadata_unavailable.get_or_insert(path);
            }
        }
    }
    sources.push(cc_switch_source(
        if metadata_unavailable.is_some() {
            "metadata_unavailable"
        } else {
            "missing"
        },
        metadata_unavailable
            .map(PathBuf::as_path)
            .or_else(|| paths.first().map(PathBuf::as_path)),
        None,
    ));
}

fn discover_cc_switch_at(
    path: &Path,
    candidates: &mut Vec<UpstreamCandidate>,
    sources: &mut Vec<Value>,
) {
    if !path.is_file() {
        sources.push(cc_switch_source("missing", Some(path), None));
        return;
    }
    let flags = OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX;
    let connection = match Connection::open_with_flags(path, flags) {
        Ok(connection) => connection,
        Err(_) => {
            sources.push(cc_switch_source("locked_or_corrupt", Some(path), None));
            return;
        }
    };
    let version = match connection.query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
    {
        Ok(version) => version,
        Err(_) => {
            sources.push(cc_switch_source("locked_or_corrupt", Some(path), None));
            return;
        }
    };
    // Structural probe: newer schemas that still expose the required tables
    // and columns are accepted with a compatibility label.  Only truly
    // incompatible shapes (missing tables or columns) are rejected.
    let schema_kind = if version > CC_SWITCH_PROBE_MAX_SCHEMA_VERSION {
        // Way beyond our known range — still probe structurally.
        if probe_cc_switch_schema(&connection) {
            "loaded_future_schema"
        } else {
            "unsupported_schema"
        }
    } else if version > CC_SWITCH_MIN_SCHEMA_VERSION {
        // Slightly newer than our documented minimum — structurally probe.
        if probe_cc_switch_schema(&connection) {
            "loaded_compatible_newer"
        } else {
            "unsupported_schema"
        }
    } else if version < CC_SWITCH_MIN_SCHEMA_VERSION {
        "unsupported_schema"
    } else {
        "loaded"
    };

    if schema_kind == "unsupported_schema" {
        sources.push(cc_switch_source(
            "unsupported_schema",
            Some(path),
            Some(version),
        ));
        return;
    }
    let mut statement = match connection.prepare(
        "SELECT id, app_type, name, is_current,
                COALESCE(json_extract(settings_config,'$.base_url'), json_extract(settings_config,'$.baseUrl'), json_extract(settings_config,'$.options.baseURL'), json_extract(settings_config,'$.env.ANTHROPIC_BASE_URL'), json_extract(settings_config,'$.env.OPENAI_BASE_URL'), json_extract(settings_config,'$.env.GEMINI_BASE_URL')),
                COALESCE(json_extract(settings_config,'$.protocol'), json_extract(settings_config,'$.apiFormat'), json_extract(settings_config,'$.api_format')),
                COALESCE(json_extract(settings_config,'$.model'), json_extract(settings_config,'$.model_name'), json_extract(settings_config,'$.env.ANTHROPIC_MODEL'), json_extract(settings_config,'$.env.OPENAI_MODEL'), json_extract(settings_config,'$.env.GEMINI_MODEL')),
                CASE WHEN json_type(settings_config,'$.api_key') IS NOT NULL OR json_type(settings_config,'$.apiKey') IS NOT NULL OR json_type(settings_config,'$.token') IS NOT NULL OR json_type(settings_config,'$.auth') IS NOT NULL OR json_type(settings_config,'$.env.ANTHROPIC_AUTH_TOKEN') IS NOT NULL OR json_type(settings_config,'$.env.OPENAI_API_KEY') IS NOT NULL OR json_type(settings_config,'$.env.GEMINI_API_KEY') IS NOT NULL THEN 1 ELSE 0 END
         FROM providers WHERE app_type IN ('claude','codex','gemini','opencode') ORDER BY app_type, sort_index, id"
    ) {
        Ok(statement) => statement,
        Err(_) => {
            sources.push(cc_switch_source("invalid_schema", Some(path), Some(version)));
            return;
        }
    };
    let rows = statement.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, bool>(3)?,
            row.get::<_, Option<String>>(4)?,
            row.get::<_, Option<String>>(5)?,
            row.get::<_, Option<String>>(6)?,
            row.get::<_, bool>(7)?,
        ))
    });
    if let Ok(rows) = rows {
        for row in rows.flatten() {
            let (id, app_type, name, active, base_url, protocol, model, has_auth_field) = row;
            // An endpoint may be used as a route target, but user-info,
            // query credentials, and fragments are never returned or reused.
            let base_url = base_url.and_then(|value| redact_base_url(&value));
            if base_url.is_none() && model.is_none() {
                continue;
            }
            candidates.push(UpstreamCandidate {
                id,
                name,
                protocol: protocol.unwrap_or_else(|| protocol_for_app(&app_type).to_string()),
                base_url,
                models: model.into_iter().collect(),
                source: format!("cc-switch:{app_type}"),
                freshness: now_timestamp(),
                auth_mode: if has_auth_field {
                    "unavailable"
                } else {
                    "none"
                }
                .to_string(),
                active,
                port: None,
                state: "discovered".to_string(),
                agent_ids: agent_ids_for_cc_switch_app(&app_type),
                confidence: if active { 96 } else { 72 },
                reasons: vec![if active {
                    "cc-switch marks this provider as current".to_string()
                } else {
                    "Provider exists in the cc-switch database".to_string()
                }],
            });
        }
    }
    if let Ok(mut proxy) =
        connection.prepare("SELECT app_type, listen_port, enabled, proxy_enabled FROM proxy_config")
    {
        if let Ok(rows) = proxy.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, u16>(1)?,
                row.get::<_, bool>(2)?,
                row.get::<_, bool>(3)?,
            ))
        }) {
            for row in rows.flatten() {
                let (app_type, port, enabled, proxy_enabled) = row;
                candidates.push(UpstreamCandidate {
                    id: format!("cc-switch-proxy-{app_type}"),
                    name: format!("cc-switch {app_type} proxy"),
                    protocol: protocol_for_app(&app_type).to_string(),
                    base_url: Some(format!("http://127.0.0.1:{port}")),
                    models: vec![],
                    source: "cc-switch:proxy_config".to_string(),
                    freshness: now_timestamp(),
                    auth_mode: "none".to_string(),
                    active: enabled && proxy_enabled,
                    port: Some(port),
                    state: if enabled && proxy_enabled {
                        "running_configured"
                    } else {
                        "stopped"
                    }
                    .to_string(),
                    agent_ids: agent_ids_for_cc_switch_app(&app_type),
                    confidence: if enabled && proxy_enabled { 99 } else { 65 },
                    reasons: vec![if enabled && proxy_enabled {
                        "cc-switch proxy is enabled for this Agent".to_string()
                    } else {
                        "cc-switch proxy is configured but not enabled".to_string()
                    }],
                });
            }
        }
    }
    let profile_count = connection
        .query_row("SELECT COUNT(*) FROM profiles", [], |row| {
            row.get::<_, i64>(0)
        })
        .unwrap_or(0);
    sources.push(json!({"id":"cc-switch-sqlite","state":"loaded","schema_kind":schema_kind,"freshness":now_timestamp(),"path":display_path(path),"schema_version":version,"profile_count":profile_count,"mode":"read_only"}));
}

fn normalize_cc_switch_proxy_candidates(candidates: &mut [UpstreamCandidate]) {
    let active_models = candidates
        .iter()
        .filter(|candidate| {
            candidate.source.starts_with("cc-switch:")
                && candidate.active
                && !candidate.source.ends_with("proxy_config")
        })
        .flat_map(|candidate| {
            candidate
                .agent_ids
                .iter()
                .map(move |agent_id| (agent_id.clone(), candidate.models.clone()))
        })
        .collect::<Vec<_>>();
    for proxy in candidates
        .iter_mut()
        .filter(|candidate| candidate.source == "cc-switch:proxy_config" && candidate.active)
    {
        if let Some((_, models)) = active_models
            .iter()
            .find(|(agent_id, _)| proxy.agent_ids.contains(agent_id))
        {
            proxy.models = models.clone();
        }
    }
    let proxy_agents = candidates
        .iter()
        .filter(|candidate| candidate.source == "cc-switch:proxy_config" && candidate.active)
        .flat_map(|candidate| candidate.agent_ids.clone())
        .collect::<Vec<_>>();
    for direct in candidates.iter_mut().filter(|candidate| {
        candidate.source.starts_with("cc-switch:") && candidate.source != "cc-switch:proxy_config"
    }) {
        if direct
            .agent_ids
            .iter()
            .any(|agent_id| proxy_agents.contains(agent_id))
        {
            direct.active = false;
            direct.reasons.push(
                "The enabled cc-switch proxy is the safe connection target for this Agent"
                    .to_string(),
            );
        }
    }
}

fn resolve_agent_bindings(
    candidates: &[UpstreamCandidate],
    detected_agents: &[dtoken_config_patcher::agent::DetectedAgent],
) -> Vec<AgentUpstreamBinding> {
    let observed_at_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| u64::try_from(duration.as_millis()).ok())
        .unwrap_or(1);
    let scan_context = current_process_scan_context(observed_at_ms).ok();
    detected_agents
        .iter()
        .filter_map(|detected| {
            let spec = agent_descriptor(&detected.id)?;
            let config_paths = detected
                .evidence
                .iter()
                .filter(|evidence| evidence.kind == AgentEvidenceKind::Config)
                .map(|evidence| evidence.path.clone())
                .collect::<Vec<_>>();
            let transport = agent_transport_adapter(spec.id);
            let can_configure = transport.is_some_and(|adapter| adapter.can_configure);
            let (integration_kind, truthful_limitation, provides_route_evidence) = transport
                .map(|adapter| {
                    let kind = match adapter.integration_kind {
                        dtoken_config_patcher::agent_kind::AgentIntegrationKind::Route => "route",
                        dtoken_config_patcher::agent_kind::AgentIntegrationKind::Mcp => "mcp",
                        dtoken_config_patcher::agent_kind::AgentIntegrationKind::Hook => "hook",
                    }
                    .to_string();
                    let limitation = adapter.truthful_limitation.map(|tl| tl.description);
                    let provides_evidence = adapter
                        .truthful_limitation
                        .map(|tl| tl.provides_route_evidence)
                        .unwrap_or(true);
                    (kind, limitation, provides_evidence)
                })
                .unwrap_or_else(|| ("unsupported".to_string(), None, false));
            let allowed_paths = scan_context
                .as_ref()
                .and_then(|context| allowed_agent_config_paths(spec.id, context).ok())
                .unwrap_or_default();
            let config_path =
                preferred_agent_transport_config_path(spec.id, &config_paths, &allowed_paths);
            let binary_path = detected
                .evidence
                .iter()
                .find(|evidence| evidence.kind == AgentEvidenceKind::Executable)
                .or_else(|| {
                    detected.evidence.iter().find(|evidence| {
                        matches!(
                            evidence.kind,
                            AgentEvidenceKind::AppBundle | AgentEvidenceKind::Extension
                        )
                    })
                })
                .map(|evidence| display_path(&evidence.path));
            let all_candidates = candidates
                .iter()
                .filter(|candidate| candidate.agent_ids.iter().any(|id| id == spec.id))
                .cloned()
                .collect::<Vec<_>>();
            let already_connected = all_candidates.iter().any(|candidate| {
                candidate
                    .base_url
                    .as_deref()
                    .is_some_and(is_dtoken_listener)
            });
            let eligible = all_candidates
                .iter()
                .filter(|candidate| {
                    !candidate
                        .base_url
                        .as_deref()
                        .is_some_and(is_dtoken_listener)
                })
                .cloned()
                .collect::<Vec<_>>();
            let active = eligible
                .iter()
                .filter(|candidate| candidate.active)
                .cloned()
                .collect::<Vec<_>>();
            let equivalent_active = collapse_equivalent_active_candidates(spec.id, &active);
            let selected = if let Some(current) =
                preferred_current_agent_candidate(spec.id, &equivalent_active, &active)
            {
                Some(current)
            } else if equivalent_active.len() == 1 {
                equivalent_active.first().cloned()
            } else if equivalent_active.is_empty() && eligible.len() == 1 {
                eligible.first().cloned()
            } else if equivalent_active.is_empty() && spec.id == "opencode" {
                preferred_opencode_free_candidate(&eligible)
            } else {
                None
            };
            let (resolution, confidence, reasons) = resolve_binding_state(
                spec,
                can_configure,
                already_connected,
                &eligible,
                &equivalent_active,
                selected.as_ref(),
                config_path.is_some(),
            );
            // Clear is enabled only when this exact on-disk configuration has
            // a complete d-token ownership record and still matches the
            // expected current selection. A schema-capable legacy deployment
            // must remain fail-closed until the user restores it manually.
            let can_clear = config_path
                .as_ref()
                .is_some_and(|path| can_clear_agent_transport(spec.id, path));
            // For MCP and Hook agents, check if a verified MCP config path
            // exists. The transport adapter cannot configure these agents
            // (it needs the resolved bridge path), but the agent_integration
            // module can write MCP entries using the resolved bridge path.
            // MCP agents resolve their config path platform-aware so Windows
            // agents (VS Code, Claude Desktop) land on %APPDATA% roaming
            // instead of the macOS home-relative layout. Hook agents (e.g.
            // cortex-code) resolve their verified hooks.json target instead.
            let hook_agent = is_verified_hook_agent(spec.id);
            let managed_tool_path = scan_context.as_ref().and_then(|context| {
                if hook_agent {
                    resolve_hook_config_path(spec.id, &context.home_dir)
                } else {
                    resolve_mcp_config_path_for(
                        spec.id,
                        context.platform,
                        &context.home_dir,
                        context.roaming_app_data.as_deref(),
                    )
                }
            });
            let mcp_available = !hook_agent && managed_tool_path.is_some();
            let hook_available = hook_agent && managed_tool_path.is_some();
            let managed_tool_available = mcp_available || hook_available;
            let mcp_config_path = managed_tool_path.as_ref().map(|p| display_path(p));
            // Override can_configure for MCP/Hook agents when a verified write
            // target exists. The transport adapter reports false because it
            // cannot write MCP entries, but the resolved config path is a
            // valid write target for the agent_integration module.
            let is_mcp_or_hook = matches!(
                transport.map(|a| a.integration_kind),
                Some(dtoken_config_patcher::agent_kind::AgentIntegrationKind::Mcp)
                    | Some(dtoken_config_patcher::agent_kind::AgentIntegrationKind::Hook)
            );
            let mcp_can_configure = is_mcp_or_hook && managed_tool_available;
            // can_clear must stay fail-closed without exact d-token-owned
            // entry evidence. Discovery cannot build the desktop-resolved
            // bridge command, so it never asserts an owned entry exists; the
            // desktop-managed integration flow performs exact verification.
            let transport_supported = can_configure || can_clear || mcp_can_configure;
            let transport_status = if is_mcp_or_hook && managed_tool_available {
                "managed_tool_available"
            } else {
                match transport {
                    Some(adapter) if adapter.can_configure && can_clear => "supported",
                    Some(adapter) if adapter.can_configure => "configure_only",
                    Some(_) => "advanced_required",
                    None => "schema_unverified",
                }
            }
            .to_string();
            let mut transport_reason = transport
                .and_then(|adapter| adapter.advanced_required_reason)
                .map(str::to_string);
            if is_mcp_or_hook && managed_tool_available && transport_reason.is_none() {
                transport_reason = Some(
                    "A verified write target exists. Clear requires exact d-token-owned entry \
                     evidence, which discovery cannot verify without the resolved bridge \
                     executable; use the desktop-managed integration flow."
                        .to_string(),
                );
            }
            let current_config_source = selected
                .as_ref()
                .map(|candidate| candidate.source.clone())
                .or_else(|| {
                    config_path
                        .as_ref()
                        .map(|_| format!("agent-config:{}", spec.id))
                })
                .unwrap_or_else(|| "unknown".to_string());
            Some(AgentUpstreamBinding {
                agent_id: spec.id.to_string(),
                agent_name: spec.name.to_string(),
                installed: true,
                binary_path,
                config_path: config_path.as_deref().map(display_path),
                config_paths: config_paths.iter().map(|path| display_path(path)).collect(),
                current_config_source,
                candidates: eligible,
                resolved_candidate: if matches!(
                    resolution.as_str(),
                    "auto-resolved" | "needs-credential"
                ) {
                    selected
                } else {
                    None
                },
                resolution,
                confidence,
                reasons,
                capability: spec.install_capability.as_str().to_string(),
                can_configure: if mcp_can_configure {
                    true
                } else {
                    can_configure
                },
                can_clear,
                transport_supported,
                transport_status,
                transport_reason,
                already_connected,
                integration_kind,
                truthful_limitation: truthful_limitation.map(|s| s.to_string()),
                provides_route_evidence,
                mcp_available,
                hook_available,
                mcp_config_path,
            })
        })
        .collect()
}

fn preferred_agent_transport_config_path(
    agent_id: &str,
    existing: &[PathBuf],
    allowed: &[PathBuf],
) -> Option<PathBuf> {
    let preferred_suffixes: &[&str] = match agent_id {
        "claude-code" => &[".claude/settings.json"],
        "codex" => &[".codex/config.toml"],
        "cline" => &[
            ".cline/data/settings/providers.json",
            ".cline/settings/providers.json",
        ],
        "goose" => &[".config/goose/config.yaml"],
        "kilo-code" => &[".kilocode/settings.json"],
        "openclaw" => &[".openclaw/openclaw.json"],
        _ => &[],
    };
    let find_preferred = |paths: &[PathBuf]| {
        preferred_suffixes.iter().find_map(|suffix| {
            paths
                .iter()
                .find(|path| path.to_string_lossy().replace('\\', "/").ends_with(suffix))
                .cloned()
        })
    };
    find_preferred(existing)
        .or_else(|| find_preferred(allowed))
        .or_else(|| existing.first().cloned())
        .or_else(|| allowed.first().cloned())
}

fn collapse_equivalent_active_candidates(
    agent_id: &str,
    active: &[UpstreamCandidate],
) -> Vec<UpstreamCandidate> {
    let mut collapsed: Vec<UpstreamCandidate> = Vec::new();
    for candidate in active {
        let route_key = candidate_route_key(candidate);
        if let Some(current) = collapsed
            .iter_mut()
            .find(|current| candidate_route_key(current) == route_key)
        {
            let candidate_is_agent_config =
                is_agent_config_candidate_source(agent_id, &candidate.source);
            if candidate_is_agent_config && candidate.models.len() == 1 {
                current.models = candidate.models.clone();
                current.source = candidate.source.clone();
                current.name = candidate.name.clone();
            }
            if current.auth_mode == "unavailable" && candidate.auth_mode == "none" {
                current.auth_mode = "none".to_string();
            }
            current.confidence = current.confidence.max(candidate.confidence);
            current.reasons.extend(candidate.reasons.clone());
            current.reasons.sort();
            current.reasons.dedup();
        } else {
            collapsed.push(candidate.clone());
        }
    }
    collapsed
}

fn candidate_route_key(candidate: &UpstreamCandidate) -> String {
    candidate
        .base_url
        .as_deref()
        .map(|url| {
            format!(
                "{}|{}",
                url.trim_end_matches('/').to_ascii_lowercase(),
                candidate.protocol
            )
        })
        .unwrap_or_else(|| {
            format!(
                "{}|{}|{}",
                candidate.source, candidate.id, candidate.protocol
            )
        })
}

fn preferred_current_agent_candidate(
    agent_id: &str,
    collapsed_active: &[UpstreamCandidate],
    raw_active: &[UpstreamCandidate],
) -> Option<UpstreamCandidate> {
    let mut matches = collapsed_active
        .iter()
        .filter(|candidate| is_agent_config_candidate_source(agent_id, &candidate.source));
    let mut current = matches.next()?.clone();
    if matches.next().is_some() {
        return None;
    }

    // An Agent's current config is the strongest evidence of what it will use.
    // A cc-switch proxy may describe the same loopback endpoint with a different
    // protocol label; in that case the proxy also proves d-token does not need
    // to reuse the Agent's private credential.
    if current.auth_mode == "unavailable"
        && current.base_url.as_deref().is_some_and(|base_url| {
            let endpoint = normalized_candidate_endpoint(base_url);
            raw_active.iter().any(|candidate| {
                candidate.source == "cc-switch:proxy_config"
                    && candidate.auth_mode == "none"
                    && candidate.base_url.as_deref().is_some_and(|proxy_url| {
                        normalized_candidate_endpoint(proxy_url) == endpoint
                    })
            })
        })
    {
        current.auth_mode = "none".to_string();
        current.reasons.push(
            "The active cc-switch loopback proxy can be used without reusing the Agent credential"
                .to_string(),
        );
    }
    Some(current)
}

fn is_agent_config_candidate_source(agent_id: &str, source: &str) -> bool {
    let prefix = format!("agent-config:{agent_id}");
    source == prefix
        || source
            .strip_prefix(&prefix)
            .is_some_and(|suffix| suffix.starts_with('-') || suffix.starts_with(':'))
        || (agent_id == "opencode" && source == "opencode-normal-config")
}

fn normalized_candidate_endpoint(value: &str) -> String {
    value.trim().trim_end_matches('/').to_ascii_lowercase()
}

fn preferred_opencode_free_candidate(eligible: &[UpstreamCandidate]) -> Option<UpstreamCandidate> {
    let mut candidate = eligible
        .iter()
        .find(|candidate| candidate.source.starts_with("opencode-provider-catalog"))?
        .clone();
    let preferred = [
        "deepseek-v4-flash-free",
        "north-mini-code-free",
        "mimo-v2.5-free",
        "hy3-free",
        "nemotron-3-ultra-free",
        "big-pickle",
    ]
    .into_iter()
    .find(|model| candidate.models.iter().any(|item| item == model))
    .map(str::to_string)
    .or_else(|| candidate.models.first().cloned())?;
    candidate.models = vec![preferred];
    candidate.confidence = 90;
    candidate.reasons = vec![
        "No active OpenCode model was configured; the review plan defaults to an official active zero-cost model"
            .to_string(),
    ];
    Some(candidate)
}

fn resolve_binding_state(
    spec: &AgentDescriptor,
    can_configure: bool,
    already_connected: bool,
    eligible: &[UpstreamCandidate],
    active: &[UpstreamCandidate],
    selected: Option<&UpstreamCandidate>,
    has_config: bool,
) -> (String, u8, Vec<String>) {
    if already_connected {
        return (
            "already-connected".to_string(),
            100,
            vec![format!(
                "{} already points to the d-token listener",
                spec.name
            )],
        );
    }
    if !can_configure
        && matches!(
            spec.install_capability,
            AgentInstallCapability::ManualOnly | AgentInstallCapability::PromptOnly
        )
    {
        return (
            "unsupported".to_string(),
            0,
            vec![format!(
                "{} is detected, but this version has no verified automatic d-token transport adapter",
                spec.name
            )],
        );
    }
    if eligible.is_empty() {
        return (
            if has_config {
                "unsupported"
            } else {
                "unavailable"
            }
            .to_string(),
            0,
            vec![if has_config {
                "A config file exists, but its upstream format could not be parsed safely"
                    .to_string()
            } else {
                "No safe upstream metadata was found".to_string()
            }],
        );
    }
    if selected.is_none() && (active.len() > 1 || (active.is_empty() && eligible.len() > 1)) {
        return (
            "needs-choice".to_string(),
            0,
            vec!["Multiple equally plausible upstreams were found".to_string()],
        );
    }
    let Some(candidate) = selected else {
        return (
            "needs-choice".to_string(),
            0,
            vec!["Select one discovered upstream".to_string()],
        );
    };
    if candidate.protocol == "unknown" {
        return (
            "unsupported".to_string(),
            candidate.confidence,
            vec![
                "The detected upstream protocol is not verified for automatic routing".to_string(),
            ],
        );
    }
    if candidate.base_url.is_none() {
        return (
            "unsupported".to_string(),
            candidate.confidence,
            vec![
                "The detected provider does not expose a safe endpoint for d-token routing"
                    .to_string(),
            ],
        );
    }
    if candidate.auth_mode == "unavailable" {
        return (
            "needs-credential".to_string(),
            candidate.confidence,
            vec![
                "The upstream is known, but its credential cannot be reused by d-token".to_string(),
            ],
        );
    }
    if candidate.models.len() != 1 {
        return (
            "needs-choice".to_string(),
            candidate.confidence,
            vec!["Select the model to keep for this Agent".to_string()],
        );
    }
    (
        "auto-resolved".to_string(),
        candidate.confidence,
        candidate.reasons.clone(),
    )
}

fn cc_switch_sync_state(sources: &[Value], candidates: &[UpstreamCandidate]) -> Value {
    let source = sources
        .iter()
        .find(|source| source.get("id").and_then(Value::as_str) == Some("cc-switch-sqlite"));
    let source_state = source
        .and_then(|source| source.get("state"))
        .and_then(Value::as_str)
        .unwrap_or("missing");
    let detected = source_state == "loaded";
    // Error states: locked/corrupt, unsupported schema, or metadata-unavailable
    let error_state = matches!(
        source_state,
        "locked_or_corrupt" | "unsupported_schema" | "invalid_schema" | "metadata_unavailable"
    );
    let active_upstreams = candidates
        .iter()
        .filter(|candidate| {
            // When cc-switch's own loopback proxy is enabled the direct
            // provider is intentionally deactivated above. The proxy is the
            // only safe no-credential d-token target, so it must remain an
            // active sync candidate rather than being discarded here.
            candidate.source.starts_with("cc-switch:") && candidate.active
        })
        .map(|candidate| {
            json!({
                "provider_id": candidate.id,
                "source": candidate.source,
                "protocol": candidate.protocol,
                "upstream": candidate.base_url,
                "models": candidate.models,
                "agent_ids": candidate.agent_ids,
            })
        })
        .collect::<Vec<_>>();
    let current = active_upstreams.first();
    // Derive provisional status. The final synced/out-of-sync decision is
    // made by `derive_cc_switch_status` in main.rs after comparing against
    // d-token's own active route; here we only set the pre-enrichment state.
    let status = if !detected && error_state {
        "error"
    } else if !detected {
        "unavailable"
    } else if active_upstreams.len() > 1 {
        "needs-selection"
    } else if active_upstreams.is_empty() {
        "detected"
    } else {
        // One active upstream: enrichment will decide synced vs out-of-sync
        // vs needs-confirmation by comparing against d-token's active route.
        // Keep needs-confirmation as the pre-enrichment placeholder.
        "needs-confirmation"
    };
    // Compute a stable fingerprint for the sync state itself, used for
    // change detection independent of the full CcSwitchSnapshot fingerprint.
    let sync_stable = json!({
        "source_state": source_state,
        "active_upstreams": active_upstreams,
    });
    let sync_fingerprint = metadata_fingerprint("cc-switch-sync", &sync_stable);
    json!({
        "detected": detected,
        "source_state": source_state,
        "status": status,
        "error": if error_state {
            Value::String(source_state.to_string())
        } else {
            Value::Null
        },
        // The provider candidate name is not a profile name. Keep the profile
        // reference opaque until the schema exposes a safe current-profile
        // relation; never display a provider name as a profile.
        "profile": Value::Null,
        "profile_source": "not_exposed_by_safe_schema",
        "upstream": current.and_then(|candidate| candidate.get("upstream").and_then(Value::as_str)),
        "active_upstreams": active_upstreams,
        "connection_path": current.and_then(|candidate| candidate.get("provider_id").and_then(Value::as_str)).map(|provider_id| format!("Agent -> d-token -> {provider_id}")),
        "source": "cc-switch-read-only",
        "writes_performed": false,
        // Evidence-bound source identity for change detection.
        "source_fingerprint": sync_fingerprint,
        "source_revision": format!("cc-switch-sync-{sync_fingerprint}"),
        // Populated by enrich_upstream_discovery in main.rs.
        "route_snapshot_identity": Value::Null,
        "last_receipt": Value::Null,
    })
}

fn discovery_revision(bindings: &[AgentUpstreamBinding], cc_switch_sync: &Value) -> String {
    let stable = json!({
        "bindings": bindings.iter().map(|binding| json!({
            "agent_id": binding.agent_id,
            "installed": binding.installed,
            "config_path": binding.config_path,
            "resolution": binding.resolution,
            "resolved": binding.resolved_candidate.as_ref().map(|candidate| json!({
                "id": candidate.id,
                "source": candidate.source,
                "models": candidate.models,
                "base_url": candidate.base_url,
                "auth_mode": candidate.auth_mode
            })),
            "candidate_count": binding.candidates.len(),
            "can_configure": binding.can_configure,
            "can_clear": binding.can_clear,
            "transport_status": binding.transport_status,
            "mcp_available": binding.mcp_available,
            "hook_available": binding.hook_available,
            "mcp_config_path": binding.mcp_config_path,
            "already_connected": binding.already_connected
        })).collect::<Vec<_>>(),
        "cc_switch": cc_switch_sync
    })
    .to_string();
    format!("setup-{:x}", Sha256::digest(stable.as_bytes()))
}

fn run_capture_with_timeout(
    program: &str,
    args: &[&str],
    timeout: Duration,
) -> Result<(bool, String), String> {
    let mut child = Command::new(program)
        .args(args)
        .env_remove("OPENCODE_CONFIG_CONTENT")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| error.to_string())?;
    let started = Instant::now();
    loop {
        match child.try_wait().map_err(|error| error.to_string())? {
            Some(status) => {
                let mut stdout = Vec::new();
                if let Some(mut pipe) = child.stdout.take() {
                    let _ = pipe.by_ref().take(256 * 1024).read_to_end(&mut stdout);
                }
                return Ok((
                    status.success(),
                    String::from_utf8_lossy(&stdout).to_string(),
                ));
            }
            None if started.elapsed() >= timeout => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("timeout".to_string());
            }
            None => thread::sleep(Duration::from_millis(20)),
        }
    }
}

fn redact_base_url(value: &str) -> Option<String> {
    let mut parsed = url::Url::parse(value).ok()?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return None;
    }
    let _ = parsed.set_username("");
    let _ = parsed.set_password(None);
    parsed.set_query(None);
    parsed.set_fragment(None);
    Some(parsed.to_string().trim_end_matches('/').to_string())
}

fn is_dtoken_listener(value: &str) -> bool {
    url::Url::parse(value).ok().is_some_and(|url| {
        matches!(url.host_str(), Some("127.0.0.1" | "localhost"))
            && url.port_or_known_default() == Some(18001)
    })
}

fn agent_ids_for_cc_switch_app(app: &str) -> Vec<String> {
    match app {
        "claude" => vec!["claude-code".to_string()],
        "codex" => vec!["codex".to_string()],
        "gemini" => vec!["gemini-cli".to_string()],
        "opencode" => vec!["opencode".to_string()],
        _ => vec![],
    }
}

fn cc_switch_db_path() -> Option<PathBuf> {
    if let Some(home) = env::var_os("CC_SWITCH_TEST_HOME") {
        return Some(PathBuf::from(home).join(".cc-switch").join("cc-switch.db"));
    }
    #[cfg(windows)]
    {
        env::var_os("USERPROFILE")
            .map(PathBuf::from)
            .map(|home| home.join(".cc-switch").join("cc-switch.db"))
    }
    #[cfg(not(windows))]
    {
        env::var_os("HOME")
            .map(PathBuf::from)
            .map(|home| home.join(".cc-switch").join("cc-switch.db"))
    }
}

fn cc_switch_db_paths() -> Vec<PathBuf> {
    if let Some(override_path) = env::var_os("CC_SWITCH_DB_PATH") {
        return vec![PathBuf::from(override_path)];
    }
    if let Some(test_home) = env::var_os("CC_SWITCH_TEST_HOME") {
        return vec![PathBuf::from(test_home)
            .join(".cc-switch")
            .join("cc-switch.db")];
    }
    let mut paths = Vec::new();
    let home = env::var_os("HOME").map(PathBuf::from);
    if let Some(home) = home {
        paths.push(home.join(".cc-switch").join("cc-switch.db"));
        paths.push(home.join("Library/Application Support/cc-switch/cc-switch.db"));
    }
    for variable in ["LOCALAPPDATA", "APPDATA", "USERPROFILE"] {
        if let Some(root) = env::var_os(variable) {
            let root = PathBuf::from(root);
            paths.push(root.join("cc-switch").join("cc-switch.db"));
            paths.push(root.join(".cc-switch").join("cc-switch.db"));
        }
    }
    if paths.is_empty() {
        if let Some(path) = cc_switch_db_path() {
            paths.push(path);
        }
    }
    paths.sort();
    paths.dedup();
    paths
}

fn cc_switch_source(state: &str, path: Option<&Path>, version: Option<i64>) -> Value {
    json!({"id":"cc-switch-sqlite","state":state,"freshness":now_timestamp(),"path":path.map(display_path),"schema_version":version,"mode":"read_only"})
}

/// Checks whether the required tables (`providers`, `proxy_config`)
/// and their expected columns exist.  Returns `true` if the minimum
/// reading contract is satisfied, regardless of schema version.
fn probe_cc_switch_schema(connection: &Connection) -> bool {
    const REQUIRED_TABLES: &[(&str, &[&str])] = &[
        (
            "providers",
            &["id", "app_type", "name", "settings_config", "is_current"],
        ),
        (
            "proxy_config",
            &["app_type", "listen_port", "enabled", "proxy_enabled"],
        ),
    ];
    for &(table, columns) in REQUIRED_TABLES {
        let mut statement = match connection.prepare(&format!(
            "SELECT {cols} FROM {table} LIMIT 0",
            cols = columns.join(","),
            table = table
        )) {
            Ok(s) => s,
            Err(_) => return false,
        };
        if statement.query([]).is_err() {
            return false;
        }
    }
    true
}

fn protocol_for_app(app: &str) -> &'static str {
    if app == "claude" {
        "anthropic"
    } else {
        "open_ai"
    }
}
fn display_path(path: &Path) -> String {
    ["HOME", "USERPROFILE"]
        .into_iter()
        .filter_map(env::var_os)
        .map(PathBuf::from)
        .find_map(|home| {
            path.strip_prefix(home)
                .ok()
                .map(|relative| format!("~/{}", relative.display()))
        })
        .unwrap_or_else(|| {
            format!(
                "<local>/{}",
                path.file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("config")
            )
        })
}
fn now_timestamp() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| format!("unix:{}", duration.as_secs()))
        .unwrap_or_else(|_| "unix:0".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::params;

    fn source_state(sources: &[Value]) -> &str {
        sources
            .first()
            .and_then(|source| source.get("state"))
            .and_then(Value::as_str)
            .expect("cc-switch source state")
    }

    fn create_schema_12(path: &Path) -> Connection {
        let connection = Connection::open(path).expect("create cc-switch fixture database");
        connection
            .execute_batch(
                "PRAGMA user_version = 12;
                 CREATE TABLE providers (
                   id TEXT PRIMARY KEY,
                   app_type TEXT NOT NULL,
                   name TEXT NOT NULL,
                   settings_config TEXT NOT NULL,
                   is_current INTEGER NOT NULL DEFAULT 0,
                   sort_index INTEGER NOT NULL DEFAULT 0
                 );
                 CREATE TABLE provider_endpoints (
                   id TEXT PRIMARY KEY,
                   provider_id TEXT NOT NULL,
                   endpoint TEXT NOT NULL
                 );
                 CREATE TABLE proxy_config (
                   app_type TEXT PRIMARY KEY,
                   listen_port INTEGER NOT NULL,
                   enabled INTEGER NOT NULL DEFAULT 0,
                   proxy_enabled INTEGER NOT NULL DEFAULT 0
                 );
                 CREATE TABLE profiles (
                   id TEXT PRIMARY KEY,
                   name TEXT NOT NULL
                 );",
            )
            .expect("create schema 12 tables");
        connection
    }

    #[test]
    fn official_catalog_shape_is_no_auth_and_explicit() {
        let value = discover();
        assert!(value["selection_required"].is_boolean());
        assert!(value["agent_bindings"].is_array());
        assert!(value["revision"]
            .as_str()
            .is_some_and(|revision| revision.starts_with("setup-")));
        assert_eq!(value["writes_performed"], false);
        assert_eq!(value["external_config_snapshot"]["version"], 1);
        assert_eq!(
            value["external_config_snapshot"]["mode"],
            "metadata_polling"
        );
        assert_eq!(
            value["external_config_snapshot"]["automatic_apply_allowed"],
            false
        );
        assert!(value["external_config_snapshot"]["fingerprint"]
            .as_str()
            .is_some_and(|fingerprint| fingerprint.starts_with("external-config-")));
    }

    fn test_external_snapshot(
        fingerprint: &str,
        cc_switch_fingerprint: &str,
        agent_configs: Vec<AgentConfigSnapshot>,
    ) -> ExternalConfigSnapshot {
        ExternalConfigSnapshot {
            version: EXTERNAL_CONFIG_SNAPSHOT_VERSION,
            mode: "metadata_polling".to_string(),
            poll_interval_ms: EXTERNAL_CONFIG_POLL_INTERVAL_MS,
            debounce_ms: EXTERNAL_CONFIG_DEBOUNCE_MS,
            observed_at: "unix:1".to_string(),
            cc_switch: CcSwitchSnapshot {
                source_state: CcSwitchSourceState::Loaded,
                schema_version: Some(12),
                schema_kind: Some("loaded".to_string()),
                path: Some("~/cc-switch.db".to_string()),
                files: vec![],
                app_types: vec!["codex".to_string()],
                active_upstreams: vec![],
                selection_state: "no_active_upstream".to_string(),
                fingerprint: cc_switch_fingerprint.to_string(),
                revision: format!("cc-switch-{cc_switch_fingerprint}"),
                observed_at: "unix:1".to_string(),
                mode: "metadata_polling".to_string(),
            },
            agent_configs,
            fingerprint: fingerprint.to_string(),
            automatic_apply_allowed: false,
        }
    }

    #[test]
    fn metadata_snapshot_never_includes_config_contents() {
        const SECRET: &str = "EXTERNAL_CONFIG_SECRET_MUST_NOT_ESCAPE";
        let temporary = tempfile::tempdir().expect("temporary directory");
        let path = temporary.path().join("agent-config.json");
        fs::write(&path, format!("{{\"apiKey\":\"{SECRET}\"}}")).expect("fixture config");

        let snapshot = agent_config_snapshot("fixture-agent", &path);

        assert_eq!(snapshot.state, "present");
        assert!(!serde_json::to_string(&snapshot)
            .expect("serialize snapshot")
            .contains(SECRET));
    }

    #[test]
    fn claude_settings_parser_reads_only_explicit_upstream_metadata() {
        const SECRET: &str = "CLAUDE_SETTINGS_SECRET_MUST_NOT_ESCAPE";
        let temporary = tempfile::tempdir().expect("temporary directory");
        let path = temporary.path().join("settings.json");
        fs::write(
            &path,
            format!(
                r#"{{
                  "env": {{
                    "ANTHROPIC_BASE_URL": "https://claude.example.test/v1",
                    "ANTHROPIC_MODEL": "claude-test",
                    "ANTHROPIC_API_KEY": "{SECRET}"
                  }},
                  "unrelated": {{"baseURL": "https://wrong.example.test/v1", "model": "wrong"}}
                }}"#
            ),
        )
        .expect("write Claude settings");

        let candidate = candidate_from_claude_settings(&path)
            .expect("parse Claude settings")
            .expect("Claude candidate");

        assert_eq!(
            candidate.base_url.as_deref(),
            Some("https://claude.example.test/v1")
        );
        assert_eq!(candidate.models, ["claude-test"]);
        assert_eq!(candidate.protocol, "anthropic");
        assert_eq!(candidate.auth_mode, "unavailable");
        assert!(!serde_json::to_string(&candidate)
            .expect("serialize candidate")
            .contains(SECRET));
        assert!(!candidate
            .base_url
            .as_deref()
            .is_some_and(|endpoint| endpoint.contains("wrong.example")));
    }

    #[test]
    fn cc_switch_source_state_distinguishes_safe_failure_modes() {
        assert_eq!(
            CcSwitchSourceState::from_source(Some("metadata_unavailable")),
            CcSwitchSourceState::MetadataUnavailable
        );
        assert_eq!(
            serde_json::to_value(CcSwitchSourceState::Missing).expect("serialize"),
            json!("missing")
        );
        assert_eq!(
            serde_json::to_value(CcSwitchSourceState::LockedOrCorrupt).expect("serialize"),
            json!("locked_or_corrupt")
        );
        assert_eq!(
            serde_json::to_value(CcSwitchSourceState::UnsupportedSchema).expect("serialize"),
            json!("unsupported_schema")
        );
        assert_eq!(
            serde_json::to_value(CcSwitchSourceState::MetadataUnavailable).expect("serialize"),
            json!("metadata_unavailable")
        );
    }

    #[test]
    fn cc_switch_snapshot_includes_only_redacted_resolvable_metadata() {
        const SECRET: &str = "cc_switch_snapshot_secret";
        let source = json!({
            "id":"cc-switch-sqlite",
            "state":"loaded",
            "schema_version":13,
            "schema_kind":"loaded_compatible_newer",
        });
        let candidate = UpstreamCandidate {
            id: "provider-a".to_string(),
            name: "Provider A".to_string(),
            protocol: "open_ai".to_string(),
            base_url: Some(format!(
                "https://user:{SECRET}@proxy.example.test/v1?token={SECRET}"
            )),
            models: vec!["model-a".to_string()],
            source: "cc-switch:codex".to_string(),
            freshness: "unix:1".to_string(),
            auth_mode: "unavailable".to_string(),
            active: true,
            port: None,
            state: "discovered".to_string(),
            agent_ids: vec!["codex".to_string()],
            confidence: 96,
            reasons: vec![],
        };

        let snapshot = cc_switch_snapshot(Some(&source), &[candidate]);

        assert_eq!(snapshot.selection_state, "resolved");
        assert_eq!(snapshot.active_upstreams[0].app_type, "codex");
        assert_eq!(snapshot.active_upstreams[0].profile_scope, "unknown");
        assert_eq!(
            snapshot.active_upstreams[0].model.as_deref(),
            Some("model-a")
        );
        assert_eq!(snapshot.active_upstreams[0].protocol, "open_ai");
        assert_eq!(
            snapshot.active_upstreams[0].endpoint.as_deref(),
            Some("https://proxy.example.test/v1")
        );
        assert!(snapshot.active_upstreams[0].credential_present);
        assert!(snapshot.revision.starts_with("cc-switch-"));
        assert!(!serde_json::to_string(&snapshot)
            .expect("serialize snapshot")
            .contains(SECRET));
    }

    #[test]
    fn cc_switch_watcher_filters_database_wal_shm_and_parent_only() {
        let database = PathBuf::from("/tmp/cc-switch/cc-switch.db");
        let databases = vec![database.clone()];
        assert!(is_cc_switch_watch_path_for_databases(&database, &databases));
        assert!(is_cc_switch_watch_path_for_databases(
            &PathBuf::from("/tmp/cc-switch/cc-switch.db-wal"),
            &databases
        ));
        assert!(is_cc_switch_watch_path_for_databases(
            &PathBuf::from("/tmp/cc-switch/cc-switch.db-shm"),
            &databases
        ));
        assert!(is_cc_switch_watch_path_for_databases(
            &PathBuf::from("/tmp/cc-switch"),
            &databases
        ));
        assert!(!is_cc_switch_watch_path_for_databases(
            &PathBuf::from("/tmp/cc-switch/unrelated.json"),
            &databases
        ));
    }

    #[test]
    fn external_watcher_filters_confirmed_agent_configs_and_cc_switch_files() {
        let agent = PathBuf::from("/tmp/agent/settings.json");
        let auxiliary = PathBuf::from("/tmp/agent/custom_providers/current.json");
        let database = PathBuf::from("/tmp/cc-switch/cc-switch.db");
        assert!(is_external_config_watch_path_for_targets(
            &agent,
            &[agent.clone(), auxiliary.clone()],
            std::slice::from_ref(&database),
        ));
        assert!(is_external_config_watch_path_for_targets(
            &auxiliary,
            &[agent.clone(), auxiliary.clone()],
            std::slice::from_ref(&database),
        ));
        assert!(is_external_config_watch_path_for_targets(
            &PathBuf::from("/tmp/cc-switch/cc-switch.db-wal"),
            std::slice::from_ref(&agent),
            std::slice::from_ref(&database),
        ));
        assert!(!is_external_config_watch_path_for_targets(
            &PathBuf::from("/tmp/agent/unrelated.json"),
            &[agent],
            &[database],
        ));
    }

    #[test]
    fn goose_auxiliary_provider_file_participates_in_watch_and_fingerprint_targets() {
        let root = tempfile::tempdir().expect("temporary directory");
        let config = root.path().join("config.yaml");
        fs::write(
            &config,
            "GOOSE_PROVIDER: selected-provider\nGOOSE_MODEL: selected-model\n",
        )
        .expect("Goose config");

        let files = agent_config_files_for_snapshot("goose", &config);

        assert_eq!(
            files,
            vec![
                config,
                root.path()
                    .join("custom_providers")
                    .join("selected-provider.json"),
            ]
        );
    }

    #[test]
    fn cortex_connections_snapshot_is_strictly_metadata_only() {
        let root = tempfile::tempdir().expect("temporary directory");
        let connections = root.path().join("connections.toml");
        fs::write(&connections, "credential = \"must-not-be-read\"\n")
            .expect("Cortex metadata fixture");

        let snapshot = agent_config_snapshot_with_reader(
            "cortex-code",
            &connections,
            |_| -> std::io::Result<Vec<u8>> {
                panic!("Cortex connections.toml content must not be read")
            },
        );

        assert_eq!(snapshot.agent_id, "cortex-code");
        assert_eq!(snapshot.state, "present");
        assert!(snapshot.fingerprint.starts_with("agent-config-"));
        assert!(!snapshot.fingerprint.contains("must-not-be-read"));
    }

    #[test]
    fn refresh_coordinator_debounces_change_and_requires_review() {
        let agent = AgentConfigSnapshot {
            agent_id: "codex".to_string(),
            state: "present".to_string(),
            path: "~/.codex/config.toml".to_string(),
            fingerprint: "agent-a".to_string(),
        };
        let initial = test_external_snapshot("snapshot-a", "cc-a", vec![agent.clone()]);
        let changed = test_external_snapshot("snapshot-b", "cc-b", vec![agent]);
        let mut coordinator = ExternalConfigRefreshCoordinator::with_intervals(1_000, 100);

        assert!(coordinator.poll_due(0));
        let initial_event = coordinator.observe(0, initial);
        assert_eq!(initial_event.state, "external_config_snapshot");
        assert!(!initial_event.review_required);
        assert!(!initial_event.automatic_apply_allowed);
        assert!(!coordinator.poll_due(999));
        assert!(coordinator.poll_due(1_000));

        let pending = coordinator.observe(1_000, changed.clone());
        assert_eq!(pending.state, "change_pending_debounce");
        assert_eq!(pending.changed_sources, ["cc_switch"]);
        assert!(!pending.review_required);

        let review = coordinator.observe(1_100, changed);
        assert_eq!(review.state, "external_config_review_required");
        assert_eq!(review.previous_fingerprint.as_deref(), Some("snapshot-a"));
        assert_eq!(review.changed_sources, ["cc_switch"]);
        assert!(review.review_required);
        assert!(!review.automatic_apply_allowed);
    }

    #[test]
    fn refresh_coordinator_restarts_debounce_after_change_reverts_to_stable() {
        let initial = test_external_snapshot("snapshot-a", "cc-a", Vec::new());
        let changed = test_external_snapshot("snapshot-b", "cc-b", Vec::new());
        let mut coordinator = ExternalConfigRefreshCoordinator::with_intervals(1_000, 100);

        assert_eq!(
            coordinator.observe(0, initial.clone()).state,
            "external_config_snapshot"
        );
        assert_eq!(
            coordinator.observe(10, changed.clone()).state,
            "change_pending_debounce"
        );
        assert_eq!(coordinator.observe(20, initial).state, "unchanged");

        // A -> B -> A -> B must not inherit the first B's elapsed time.
        assert_eq!(
            coordinator.observe(30, changed.clone()).state,
            "change_pending_debounce"
        );
        assert_eq!(
            coordinator.observe(129, changed.clone()).state,
            "change_pending_debounce"
        );
        let review = coordinator.observe(130, changed);
        assert_eq!(review.state, "external_config_review_required");
        assert_eq!(review.previous_fingerprint.as_deref(), Some("snapshot-a"));
        assert_eq!(review.changed_sources, ["cc_switch"]);
    }

    #[test]
    fn opencode_cache_keeps_only_active_zero_cost_models() {
        let models = parse_opencode_free_models(
            br#"{
              "opencode": {"models": {
                "free-active": {"cost": {"input": 0, "output": 0}},
                "free-deprecated": {"status": "deprecated", "cost": {"input": 0, "output": 0}},
                "paid": {"cost": {"input": 1, "output": 2}},
                "unknown": {}
              }}
            }"#,
        );

        assert_eq!(models, vec!["free-active"]);
    }

    #[test]
    fn cline_cli_v1_parser_returns_active_metadata_without_secret_values() {
        let root = tempfile::tempdir().expect("temporary directory");
        let path = root.path().join("providers.json");
        fs::write(
            &path,
            r#"{
  "version": 1,
  "lastUsedProvider": "openai-compatible",
  "providers": {
    "openai-compatible": {
      "settings": {
        "provider": "openai-compatible",
        "apiKey": "must-not-escape",
        "model": "cline-model",
        "baseUrl": "https://proxy.example/v1?token=must-not-escape"
      },
      "tokenSource": "manual"
    }
  }
}"#,
        )
        .expect("Cline fixture");

        let candidates = candidates_from_cline_config(&path).expect("Cline candidates");
        assert_eq!(candidates.len(), 1);
        assert!(candidates[0].active);
        assert_eq!(candidates[0].models, ["cline-model"]);
        assert_eq!(
            candidates[0].base_url.as_deref(),
            Some("https://proxy.example/v1")
        );
        assert_eq!(candidates[0].auth_mode, "unavailable");
        assert!(!serde_json::to_string(&candidates)
            .unwrap()
            .contains("must-not-escape"));
    }

    #[test]
    fn goose_parser_resolves_only_the_selected_custom_provider_without_secret_values() {
        const SECRET: &str = "GOOSE_SECRET_MUST_NOT_ESCAPE";
        let root = tempfile::tempdir().expect("temporary directory");
        let config_dir = root.path().join(".config/goose");
        let providers_dir = config_dir.join("custom_providers");
        fs::create_dir_all(&providers_dir).expect("provider directory");
        let path = config_dir.join("config.yaml");
        fs::write(
            &path,
            "GOOSE_PROVIDER: custom-dtoken\nGOOSE_MODEL: routed-model\nunrelated:\n  base_url: https://wrong.example.test/v1\n",
        )
        .expect("Goose config");
        fs::write(
            providers_dir.join("custom-dtoken.json"),
            format!(
                r#"{{
                  "name": "custom-dtoken",
                  "engine": "openai",
                  "base_url": "https://user:{SECRET}@goose.example.test/v1?token={SECRET}",
                  "api_key_env": "{SECRET}",
                  "models": [{{"name": "routed-model"}}]
                }}"#
            ),
        )
        .expect("Goose provider");

        let candidate = candidate_from_goose_config(&path)
            .expect("parse Goose config")
            .expect("Goose candidate");

        assert!(candidate.active);
        assert_eq!(candidate.id, "goose-custom-dtoken");
        assert_eq!(candidate.models, ["routed-model"]);
        assert_eq!(candidate.protocol, "open_ai");
        assert_eq!(
            candidate.base_url.as_deref(),
            Some("https://goose.example.test/v1")
        );
        assert_eq!(candidate.auth_mode, "unavailable");
        let encoded = serde_json::to_string(&candidate).expect("serialize Goose candidate");
        assert!(!encoded.contains(SECRET));
        assert!(!encoded.contains("wrong.example"));
    }

    #[test]
    fn kilo_parser_uses_the_explicit_active_provider_block_without_recursive_guessing() {
        const SECRET: &str = "KILO_SECRET_MUST_NOT_ESCAPE";
        let root = tempfile::tempdir().expect("temporary directory");
        let path = root.path().join("settings.json");
        fs::write(
            &path,
            format!(
                r#"{{
                  "model": "custom/routed-model",
                  "provider": {{
                    "custom": {{
                      "name": "Current provider",
                      "options": {{
                        "baseURL": "https://user:{SECRET}@kilo.example.test/v1?token={SECRET}",
                        "apiKey": "{SECRET}"
                      }},
                      "models": {{"routed-model": {{"name": "Routed"}}}}
                    }}
                  }},
                  "unrelated": {{
                    "baseURL": "https://wrong.example.test/v1",
                    "model": "wrong-model"
                  }}
                }}"#
            ),
        )
        .expect("Kilo config");

        let candidate = candidate_from_kilo_config(&path)
            .expect("parse Kilo config")
            .expect("Kilo candidate");

        assert!(candidate.active);
        assert_eq!(candidate.id, "kilo-code-custom");
        assert_eq!(candidate.models, ["routed-model"]);
        assert_eq!(candidate.protocol, "open_ai");
        assert_eq!(
            candidate.base_url.as_deref(),
            Some("https://kilo.example.test/v1")
        );
        assert_eq!(candidate.auth_mode, "unavailable");
        let encoded = serde_json::to_string(&candidate).expect("serialize Kilo candidate");
        assert!(!encoded.contains(SECRET));
        assert!(!encoded.contains("wrong.example"));
        assert!(!encoded.contains("wrong-model"));
    }

    #[test]
    fn opencode_parser_uses_explicit_provider_fields_and_drops_secret_values() {
        const SECRET: &str = "OPENCODE_SECRET_MUST_NOT_ESCAPE";
        let root = tempfile::tempdir().expect("temporary directory");
        let path = root.path().join("opencode.json");
        fs::write(
            &path,
            format!(
                r#"{{
                  "model": "custom/routed-model",
                  "provider": {{
                    "custom": {{
                      "npm": "@ai-sdk/openai-compatible",
                      "options": {{
                        "baseURL": "https://user:{SECRET}@opencode.example.test/v1?token={SECRET}",
                        "apiKey": "{SECRET}"
                      }},
                      "models": {{"routed-model": {{"name": "Routed"}}}},
                      "unrelated": {{
                        "baseURL": "https://wrong.example.test/v1",
                        "model": "wrong-model"
                      }}
                    }}
                  }}
                }}"#
            ),
        )
        .expect("OpenCode config");

        let candidates = candidates_from_opencode_config(&path).expect("OpenCode candidates");
        assert_eq!(candidates.len(), 1);
        let candidate = &candidates[0];
        assert!(candidate.active);
        assert_eq!(candidate.id, "custom");
        assert_eq!(candidate.models, ["routed-model"]);
        assert_eq!(candidate.protocol, "open_ai");
        assert_eq!(
            candidate.base_url.as_deref(),
            Some("https://opencode.example.test/v1")
        );
        assert_eq!(candidate.auth_mode, "unavailable");
        let encoded = serde_json::to_string(&candidates).expect("serialize candidates");
        assert!(!encoded.contains(SECRET));
        assert!(!encoded.contains("wrong.example"));
        assert!(!encoded.contains("wrong-model"));
    }

    #[test]
    fn codex_toml_parser_reads_local_responses_proxy_and_requires_openai_auth() {
        // Real fixture: Codex selects a local OpenAI-compatible Responses proxy
        // (ocxlocal on 127.0.0.1:10100) with requires_openai_auth=false. Discovery
        // must expose the provider identity, the redacted local base URL, the
        // Responses wire protocol, and the explicit no-auth credential state.
        const FIXTURE: &str = include_str!(
            "../../../../crates/dtoken-config-patcher/tests/fixtures/codex-ocxlocal.toml"
        );
        let root = tempfile::tempdir().expect("temporary directory");
        let path = root.path().join("config.toml");
        fs::write(&path, FIXTURE).expect("Codex config");

        let candidate = candidate_from_codex_toml(&path)
            .expect("parse")
            .expect("candidate");
        assert_eq!(candidate.id, "codex-ocxlocal");
        assert_eq!(candidate.name, "Codex ocxlocal");
        assert_eq!(
            candidate.base_url.as_deref(),
            Some("http://127.0.0.1:10100/v1")
        );
        assert_eq!(candidate.protocol, "responses");
        assert_eq!(candidate.auth_mode, "none");
        assert!(candidate.active);
        assert!(candidate.models.is_empty());
        assert_eq!(candidate.agent_ids, ["codex"]);

        // When the provider declares requires_openai_auth=true, discovery must
        // report a credential gap even without an env_key (fail-closed: the
        // OpenAI credential cannot be reused by d-token).
        let auth_path = root.path().join("config-auth.toml");
        fs::write(
            &auth_path,
            FIXTURE.replace(
                "requires_openai_auth = false",
                "requires_openai_auth = true",
            ),
        )
        .expect("Codex auth config");
        let candidate = candidate_from_codex_toml(&auth_path)
            .expect("parse")
            .expect("candidate");
        assert_eq!(candidate.auth_mode, "unavailable");
    }

    #[test]
    fn codex_toml_root_selection_behavior_is_preserved() {
        // Root-only config keeps the original behavior: the top-level
        // model_provider, model and model_providers table define the candidate.
        let root = tempfile::tempdir().expect("temporary directory");
        let path = root.path().join("config.toml");
        fs::write(
            &path,
            r#"
model_provider = "custom"
model = "gpt-root"

[model_providers.custom]
base_url = "https://custom.example/v1"
wire_api = "responses"
"#,
        )
        .expect("Codex config");

        let candidate = candidate_from_codex_toml(&path)
            .expect("parse")
            .expect("candidate");
        assert_eq!(candidate.id, "codex-custom");
        assert_eq!(
            candidate.base_url.as_deref(),
            Some("https://custom.example/v1")
        );
        assert_eq!(candidate.models, ["gpt-root"]);
        assert_eq!(candidate.protocol, "responses");
        assert_eq!(candidate.auth_mode, "none");
        assert!(candidate.active);
    }

    #[test]
    fn codex_toml_profiles_default_overrides_root_selection() {
        // [profiles.default] is the explicit active profile; its
        // model_provider and model override the root selection while other
        // profiles exist but are not selected.
        let root = tempfile::tempdir().expect("temporary directory");
        let path = root.path().join("config.toml");
        fs::write(
            &path,
            r#"
model_provider = "root-provider"
model = "gpt-root"

[model_providers.root-provider]
base_url = "https://root.example/v1"
wire_api = "responses"

[model_providers.profile-provider]
base_url = "https://profile.example/v1"
wire_api = "responses"

[profiles.default]
model_provider = "profile-provider"
model = "gpt-profile"

[profiles.work]
model_provider = "root-provider"
"#,
        )
        .expect("Codex config");

        let candidate = candidate_from_codex_toml(&path)
            .expect("parse")
            .expect("candidate");
        assert_eq!(candidate.id, "codex-profile-provider");
        assert_eq!(
            candidate.base_url.as_deref(),
            Some("https://profile.example/v1")
        );
        assert_eq!(candidate.models, ["gpt-profile"]);
    }

    #[test]
    fn codex_toml_single_profile_is_active_and_inherits_root_provider() {
        // A lone profile with no explicit default is unambiguous and treated
        // as active; keys it does not declare (model_provider) inherit the
        // root selection.
        let root = tempfile::tempdir().expect("temporary directory");
        let path = root.path().join("config.toml");
        fs::write(
            &path,
            r#"
model_provider = "root-provider"
model = "gpt-root"

[model_providers.root-provider]
base_url = "https://root.example/v1"
wire_api = "responses"

[profiles.solo]
model = "gpt-solo"
"#,
        )
        .expect("Codex config");

        let candidate = candidate_from_codex_toml(&path)
            .expect("parse")
            .expect("candidate");
        assert_eq!(candidate.id, "codex-root-provider");
        assert_eq!(
            candidate.base_url.as_deref(),
            Some("https://root.example/v1")
        );
        assert_eq!(candidate.models, ["gpt-solo"]);
    }

    #[test]
    fn codex_toml_ambiguous_multiple_profiles_fail_closed() {
        // Multiple profiles without [profiles.default] are ambiguous: Codex
        // may be invoked with --profile <name>, so discovery must fail closed
        // and report no candidate instead of guessing the active provider.
        let root = tempfile::tempdir().expect("temporary directory");
        let path = root.path().join("config.toml");
        fs::write(
            &path,
            r#"
model_provider = "root-provider"

[model_providers.root-provider]
base_url = "https://root.example/v1"

[model_providers.alpha-provider]
base_url = "https://alpha.example/v1"

[model_providers.beta-provider]
base_url = "https://beta.example/v1"

[profiles.alpha]
model_provider = "alpha-provider"

[profiles.beta]
model_provider = "beta-provider"
"#,
        )
        .expect("Codex config");

        let candidate = candidate_from_codex_toml(&path).expect("parse");
        assert!(candidate.is_none());
    }

    #[test]
    fn unverified_json_agent_is_not_recursively_guessed_as_an_upstream() {
        let root = tempfile::tempdir().expect("temporary directory");
        let path = root.path().join("config.json");
        fs::write(
            &path,
            r#"{
              "unrelated": {
                "base_url": "https://must-not-be-treated-as-upstream.example/v1",
                "model": "must-not-be-treated-as-model"
              }
            }"#,
        )
        .expect("unverified Agent config");
        let detected = dtoken_config_patcher::agent::DetectedAgent {
            id: "commandcode".to_string(),
            name: "CommandCode".to_string(),
            install_capability: AgentInstallCapability::ManualOnly,
            session_capability:
                dtoken_config_patcher::agent::AgentSessionCapability::DescriptorOnly,
            evidence: vec![dtoken_config_patcher::agent::AgentDetectionEvidence {
                kind: AgentEvidenceKind::Config,
                path,
            }],
        };
        let mut candidates = Vec::new();
        let mut sources = Vec::new();

        discover_agent_configs(&[detected], &mut candidates, &mut sources);

        assert!(candidates.is_empty());
        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0]["state"], "unsupported_schema");
    }

    #[test]
    fn cline_cli_parser_treats_auth_objects_as_presence_only() {
        let root = tempfile::tempdir().expect("temporary directory");
        let path = root.path().join("providers.json");
        fs::write(
            &path,
            r#"{
  "version": 1,
  "lastUsedProvider": "cline",
  "providers": {
    "cline": {
      "settings": {
        "provider": "cline",
        "model": "cline-model",
        "auth": {
          "accessToken": "CLINE_ACCESS_TOKEN_MUST_NOT_ESCAPE",
          "refreshToken": "CLINE_REFRESH_TOKEN_MUST_NOT_ESCAPE"
        }
      }
    },
    "dtoken-local-route": {
      "settings": {
        "provider": "openai-compatible",
        "apiKey": "dtoken-loopback",
        "model": "dtoken-model",
        "baseUrl": "http://127.0.0.1:18001/v1"
      }
    }
  }
}"#,
        )
        .expect("Cline credential fixture");

        let candidates = candidates_from_cline_config(&path).expect("Cline candidates");
        assert_eq!(candidates.len(), 2);
        let cline = candidates
            .iter()
            .find(|candidate| candidate.id == "cline-cline")
            .expect("Cline provider");
        assert_eq!(cline.auth_mode, "unavailable");
        let dtoken = candidates
            .iter()
            .find(|candidate| candidate.id == "cline-dtoken-local-route")
            .expect("d-token provider");
        assert_eq!(dtoken.auth_mode, "none");
        let encoded = serde_json::to_string(&candidates).expect("serialize metadata");
        assert!(!encoded.contains("CLINE_ACCESS_TOKEN_MUST_NOT_ESCAPE"));
        assert!(!encoded.contains("CLINE_REFRESH_TOKEN_MUST_NOT_ESCAPE"));
    }

    #[test]
    fn cline_cli_3_transport_path_is_preferred_over_legacy_location() {
        let home = PathBuf::from("/Users/test");
        let current = home.join(".cline/data/settings/providers.json");
        let legacy = home.join(".cline/settings/providers.json");

        assert_eq!(
            preferred_agent_transport_config_path(
                "cline",
                &[legacy.clone(), current.clone()],
                &[legacy],
            ),
            Some(current)
        );
    }

    #[test]
    fn openclaw_json5_parser_preserves_multi_provider_identity_and_active_model() {
        let root = tempfile::tempdir().expect("temporary directory");
        let path = root.path().join("openclaw.json");
        fs::write(
            &path,
            r#"{
  models: {
    providers: {
      first: {
        baseUrl: "https://first.example/v1",
        api: "openai-completions",
        models: [{ id: "first-model" }],
      },
      second: {
        baseUrl: "https://second.example/v1",
        api: "anthropic-messages",
        apiKey: "${SECOND_API_KEY}",
        models: [{ id: "second-model" }, { id: "other-model" }],
      },
    },
  },
  agents: { defaults: { model: { primary: "second/second-model" } } },
}"#,
        )
        .expect("OpenClaw fixture");

        let candidates = candidates_from_openclaw_config(&path).expect("OpenClaw candidates");
        assert_eq!(candidates.len(), 2);
        let active = candidates
            .iter()
            .find(|candidate| candidate.active)
            .expect("active OpenClaw provider");
        assert_eq!(active.id, "openclaw-second");
        assert_eq!(active.models, ["second-model"]);
        assert_eq!(active.protocol, "anthropic");
        assert_eq!(active.auth_mode, "unavailable");
        assert!(!serde_json::to_string(&candidates)
            .unwrap()
            .contains("SECOND_API_KEY"));
    }

    #[test]
    fn verified_transport_adapter_overrides_legacy_manual_capability_state() {
        let spec = AGENT_REGISTRY
            .iter()
            .find(|spec| spec.id == "openclaw")
            .expect("OpenClaw adapter");
        assert_eq!(spec.install_capability, AgentInstallCapability::ManualOnly);
        let candidate = UpstreamCandidate {
            id: "openclaw-current".to_string(),
            name: "OpenClaw current".to_string(),
            protocol: "open_ai".to_string(),
            base_url: Some("https://upstream.example/v1".to_string()),
            models: vec!["model-a".to_string()],
            source: "agent-config:openclaw-json5".to_string(),
            freshness: "unix:1".to_string(),
            auth_mode: "none".to_string(),
            active: true,
            port: None,
            state: "discovered".to_string(),
            agent_ids: vec!["openclaw".to_string()],
            confidence: 99,
            reasons: vec!["verified schema".to_string()],
        };
        let (resolution, _, _) = resolve_binding_state(
            spec,
            true,
            false,
            std::slice::from_ref(&candidate),
            std::slice::from_ref(&candidate),
            Some(&candidate),
            true,
        );
        assert_eq!(resolution, "auto-resolved");
    }

    #[test]
    fn bindings_include_only_backend_detected_agents() {
        use dtoken_config_patcher::agent::{
            AgentDetectionEvidence, AgentInstallCapability, AgentSessionCapability, DetectedAgent,
        };

        let detected = vec![DetectedAgent {
            id: "codex".to_string(),
            name: "Codex".to_string(),
            install_capability: AgentInstallCapability::ConfirmRequired,
            session_capability: AgentSessionCapability::Metadata,
            evidence: vec![AgentDetectionEvidence {
                kind: AgentEvidenceKind::Config,
                path: PathBuf::from("/tmp/codex-config.toml"),
            }],
        }];

        let bindings = resolve_agent_bindings(&[], &detected);

        assert_eq!(bindings.len(), 1);
        assert_eq!(bindings[0].agent_id, "codex");
        assert!(bindings[0].installed);
    }

    #[test]
    fn verified_mcp_and_hook_agents_report_configuration_capability() {
        use dtoken_config_patcher::agent::{
            AgentDetectionEvidence, AgentInstallCapability, AgentSessionCapability, DetectedAgent,
        };

        let detected = vec![
            DetectedAgent {
                id: "vscode-copilot-chat".to_string(),
                name: "VS Code Copilot Chat".to_string(),
                install_capability: AgentInstallCapability::ManualOnly,
                session_capability: AgentSessionCapability::Unsupported,
                evidence: vec![AgentDetectionEvidence {
                    kind: AgentEvidenceKind::Extension,
                    path: PathBuf::from("/tmp/vscode-extension"),
                }],
            },
            DetectedAgent {
                id: "cortex-code".to_string(),
                name: "Cortex Code".to_string(),
                install_capability: AgentInstallCapability::ManualOnly,
                session_capability: AgentSessionCapability::Unsupported,
                evidence: vec![AgentDetectionEvidence {
                    kind: AgentEvidenceKind::Config,
                    path: PathBuf::from("/tmp/hooks.json"),
                }],
            },
        ];

        let bindings = resolve_agent_bindings(&[], &detected);
        let vscode = bindings
            .iter()
            .find(|binding| binding.agent_id == "vscode-copilot-chat")
            .expect("VS Code Copilot Chat binding");
        assert!(
            vscode.can_configure,
            "vscode-copilot-chat has a verified mcp.json `servers` write target"
        );
        assert!(
            !vscode.can_clear,
            "discovery cannot verify an exact d-token-owned MCP entry without the resolved bridge, so clear must stay fail-closed"
        );
        assert!(
            vscode.mcp_available,
            "vscode-copilot-chat MCP config path must be resolvable"
        );
        assert!(
            vscode.mcp_config_path.is_some(),
            "vscode-copilot-chat binding must expose the resolved mcp.json path"
        );

        let cortex = bindings
            .iter()
            .find(|binding| binding.agent_id == "cortex-code")
            .expect("Cortex Code binding");
        assert!(
            cortex.can_configure,
            "cortex-code has a verified desktop-resolved hooks.json write target"
        );
        assert!(
            !cortex.can_clear,
            "cortex-code clear requires exact d-token-owned SessionStart entry evidence, which discovery cannot produce"
        );
        assert!(
            cortex.hook_available,
            "cortex-code hook config path must be resolvable via the hook resolver"
        );
        assert!(
            !cortex.mcp_available,
            "cortex-code has no MCP config target; only hook_available is truthful"
        );
    }

    #[test]
    fn vscode_copilot_chat_mcp_path_resolves_platform_aware() {
        let context = current_process_scan_context(1).expect("scan context");
        // The platform-aware resolver must return the macOS home-relative
        // mcp.json on macOS and the %APPDATA% path shape on Windows.
        let path = dtoken_config_patcher::agent_mcp::resolve_mcp_config_path_for(
            "vscode-copilot-chat",
            context.platform,
            &context.home_dir,
            context.roaming_app_data.as_deref(),
        );
        let path = path.expect("platform-aware vscode-copilot-chat MCP path");
        let rendered = path.to_string_lossy().replace('\\', "/");
        assert!(
            rendered.contains("Code/User/mcp.json"),
            "resolved path must be VS Code's user mcp.json, got {rendered}"
        );
        assert!(
            dtoken_config_patcher::agent_mcp::resolve_mcp_config_path(
                "vscode-copilot-chat",
                &context.home_dir,
            )
            .is_some(),
            "home-relative resolution must also work"
        );
    }

    #[test]
    fn discovery_records_every_detected_config_and_requires_choice_for_distinct_routes() {
        use dtoken_config_patcher::agent::{
            AgentDetectionEvidence, AgentInstallCapability, AgentSessionCapability, DetectedAgent,
        };

        let temporary = tempfile::tempdir().expect("temporary directory");
        let first = temporary.path().join("config.toml");
        let second = temporary.path().join("profile.toml");
        fs::write(
            &first,
            r#"model_provider = "first"
model = "model-a"
[model_providers.first]
base_url = "https://first.example/v1"
wire_api = "responses"
"#,
        )
        .expect("first config");
        fs::write(
            &second,
            r#"model_provider = "second"
model = "model-b"
[model_providers.second]
base_url = "https://second.example/v1"
wire_api = "responses"
"#,
        )
        .expect("second config");
        let detected = vec![DetectedAgent {
            id: "codex".to_string(),
            name: "Codex".to_string(),
            install_capability: AgentInstallCapability::ConfirmRequired,
            session_capability: AgentSessionCapability::Metadata,
            evidence: vec![
                AgentDetectionEvidence {
                    kind: AgentEvidenceKind::Config,
                    path: first,
                },
                AgentDetectionEvidence {
                    kind: AgentEvidenceKind::Config,
                    path: second,
                },
            ],
        }];
        let mut candidates = Vec::new();
        let mut sources = Vec::new();

        discover_agent_configs(&detected, &mut candidates, &mut sources);

        assert_eq!(sources.len(), 2);
        assert!(sources.iter().all(|source| source["state"] == "loaded"));
        assert_eq!(candidates.len(), 2);
        assert_ne!(candidates[0].id, candidates[1].id);
        let bindings = resolve_agent_bindings(&candidates, &detected);
        assert_eq!(bindings[0].config_paths.len(), 2);
        assert_eq!(bindings[0].resolution, "needs-choice");
        assert!(bindings[0].resolved_candidate.is_none());
    }

    #[test]
    fn unique_current_candidate_is_auto_resolved_but_multiple_candidates_require_choice() {
        let spec = AGENT_REGISTRY
            .iter()
            .find(|spec| spec.id == "opencode")
            .unwrap();
        let current = UpstreamCandidate {
            id: "current".to_string(),
            name: "Current".to_string(),
            protocol: "open_ai".to_string(),
            base_url: Some("https://example.test/v1".to_string()),
            models: vec!["model-a".to_string()],
            source: "agent-config:opencode".to_string(),
            freshness: "unix:1".to_string(),
            auth_mode: "none".to_string(),
            active: true,
            port: None,
            state: "discovered".to_string(),
            agent_ids: vec!["opencode".to_string()],
            confidence: 90,
            reasons: vec![],
        };
        let (resolution, _, _) = resolve_binding_state(
            spec,
            true,
            false,
            std::slice::from_ref(&current),
            std::slice::from_ref(&current),
            Some(&current),
            true,
        );
        assert_eq!(resolution, "auto-resolved");

        let second = UpstreamCandidate {
            id: "second".to_string(),
            active: false,
            ..current.clone()
        };
        let (resolution, _, _) =
            resolve_binding_state(spec, true, false, &[current, second], &[], None, true);
        assert_eq!(resolution, "needs-choice");
    }

    #[test]
    fn opencode_without_an_active_model_uses_a_reviewed_official_free_default() {
        let catalog = UpstreamCandidate {
            id: "opencode".to_string(),
            name: "OpenCode official free catalog".to_string(),
            protocol: "open_ai".to_string(),
            base_url: Some("https://opencode.ai/zen/v1".to_string()),
            models: vec![
                "big-pickle".to_string(),
                "deepseek-v4-flash-free".to_string(),
            ],
            source: "opencode-provider-catalog-cache".to_string(),
            freshness: "unix:1".to_string(),
            auth_mode: "none".to_string(),
            active: false,
            port: Some(443),
            state: "available".to_string(),
            agent_ids: vec!["opencode".to_string()],
            confidence: 85,
            reasons: vec![],
        };

        let selected = preferred_opencode_free_candidate(&[catalog]).expect("official default");

        assert_eq!(selected.models, vec!["deepseek-v4-flash-free"]);
        assert_eq!(selected.confidence, 90);
        let custom = UpstreamCandidate {
            id: "custom".to_string(),
            name: "Custom".to_string(),
            base_url: Some("https://custom.example/v1".to_string()),
            models: vec!["custom-model".to_string()],
            source: "opencode-normal-config".to_string(),
            auth_mode: "unavailable".to_string(),
            confidence: 80,
            ..selected.clone()
        };
        let spec = AGENT_REGISTRY
            .iter()
            .find(|spec| spec.id == "opencode")
            .expect("OpenCode adapter");
        let (resolution, _, _) = resolve_binding_state(
            spec,
            true,
            false,
            &[selected.clone(), custom],
            &[],
            Some(&selected),
            true,
        );
        assert_eq!(resolution, "auto-resolved");
    }

    #[test]
    fn agent_config_and_cc_switch_proxy_for_the_same_endpoint_are_not_ambiguous() {
        let config = UpstreamCandidate {
            id: "claude-code-current".to_string(),
            name: "Claude Code current upstream".to_string(),
            protocol: "anthropic".to_string(),
            base_url: Some("http://127.0.0.1:15721".to_string()),
            models: vec!["opus".to_string()],
            source: "agent-config:claude-code".to_string(),
            freshness: "unix:1".to_string(),
            auth_mode: "unavailable".to_string(),
            active: true,
            port: None,
            state: "discovered".to_string(),
            agent_ids: vec!["claude-code".to_string()],
            confidence: 96,
            reasons: vec![],
        };
        let proxy = UpstreamCandidate {
            id: "cc-switch-proxy-claude".to_string(),
            name: "cc-switch claude proxy".to_string(),
            source: "cc-switch:proxy_config".to_string(),
            auth_mode: "none".to_string(),
            confidence: 99,
            ..config.clone()
        };

        let collapsed = collapse_equivalent_active_candidates("claude-code", &[config, proxy]);

        assert_eq!(collapsed.len(), 1);
        assert_eq!(collapsed[0].models, vec!["opus"]);
        assert_eq!(collapsed[0].auth_mode, "none");
    }

    #[test]
    fn current_agent_config_wins_over_saved_profiles_and_proxy_protocol_labels() {
        let current = UpstreamCandidate {
            id: "claude-code-current".to_string(),
            name: "Claude Code current upstream".to_string(),
            protocol: "anthropic".to_string(),
            base_url: Some("http://127.0.0.1:15721".to_string()),
            models: vec!["opus".to_string()],
            source: "agent-config:claude-code".to_string(),
            freshness: "unix:1".to_string(),
            auth_mode: "unavailable".to_string(),
            active: true,
            port: None,
            state: "discovered".to_string(),
            agent_ids: vec!["claude-code".to_string()],
            confidence: 96,
            reasons: vec![],
        };
        let proxy = UpstreamCandidate {
            id: "cc-switch-proxy-claude".to_string(),
            name: "cc-switch claude proxy".to_string(),
            protocol: "open_ai".to_string(),
            source: "cc-switch:proxy_config".to_string(),
            auth_mode: "none".to_string(),
            confidence: 99,
            ..current.clone()
        };
        let saved_profile = UpstreamCandidate {
            id: "saved-profile".to_string(),
            name: "Saved profile".to_string(),
            base_url: Some("https://saved.example/v1".to_string()),
            source: "cc-switch:claude".to_string(),
            active: false,
            ..current.clone()
        };
        let raw_active = vec![current.clone(), proxy];
        let collapsed = collapse_equivalent_active_candidates("claude-code", &raw_active);
        let selected = preferred_current_agent_candidate("claude-code", &collapsed, &raw_active)
            .expect("current Agent config");

        assert_eq!(selected.source, "agent-config:claude-code");
        assert_eq!(selected.models, ["opus"]);
        assert_eq!(selected.auth_mode, "none");
        let spec = AGENT_REGISTRY
            .iter()
            .find(|spec| spec.id == "claude-code")
            .expect("Claude Code adapter");
        let (resolution, _, _) = resolve_binding_state(
            spec,
            true,
            false,
            &[current, saved_profile],
            &collapsed,
            Some(&selected),
            true,
        );
        assert_eq!(resolution, "auto-resolved");
    }

    #[test]
    fn credential_gap_and_existing_dtoken_listener_are_distinct_states() {
        let spec = AGENT_REGISTRY
            .iter()
            .find(|spec| spec.id == "opencode")
            .unwrap();
        let credential = UpstreamCandidate {
            id: "current".to_string(),
            name: "Current".to_string(),
            protocol: "open_ai".to_string(),
            base_url: Some("https://example.test/v1".to_string()),
            models: vec!["model-a".to_string()],
            source: "agent-config:opencode".to_string(),
            freshness: "unix:1".to_string(),
            auth_mode: "unavailable".to_string(),
            active: true,
            port: None,
            state: "discovered".to_string(),
            agent_ids: vec!["opencode".to_string()],
            confidence: 90,
            reasons: vec![],
        };
        let (resolution, _, _) = resolve_binding_state(
            spec,
            true,
            false,
            std::slice::from_ref(&credential),
            std::slice::from_ref(&credential),
            Some(&credential),
            true,
        );
        assert_eq!(resolution, "needs-credential");

        let (resolution, _, _) = resolve_binding_state(spec, true, true, &[], &[], None, true);
        assert_eq!(resolution, "already-connected");
    }
    #[test]
    fn cc_switch_missing_database_is_reported_without_candidates() {
        let temporary = tempfile::tempdir().expect("temporary directory");
        let path = temporary.path().join("missing.db");
        let mut candidates = Vec::new();
        let mut sources = Vec::new();

        discover_cc_switch_at(&path, &mut candidates, &mut sources);

        assert!(candidates.is_empty());
        assert_eq!(source_state(&sources), "missing");
        assert_eq!(sources[0]["mode"], "read_only");
    }

    #[test]
    fn cc_switch_gemini_candidates_bind_to_the_backend_gemini_adapter() {
        assert_eq!(agent_ids_for_cc_switch_app("gemini"), ["gemini-cli"]);
    }

    #[test]
    fn cc_switch_corrupt_database_fails_closed() {
        let temporary = tempfile::tempdir().expect("temporary directory");
        let path = temporary.path().join("cc-switch.db");
        fs::write(&path, b"not a sqlite database").expect("write corrupt fixture");
        let mut candidates = Vec::new();
        let mut sources = Vec::new();

        discover_cc_switch_at(&path, &mut candidates, &mut sources);

        assert!(candidates.is_empty());
        assert_eq!(source_state(&sources), "locked_or_corrupt");
        assert!(sources[0]["schema_version"].is_null());
    }

    #[test]
    fn cc_switch_locked_database_fails_closed() {
        let temporary = tempfile::tempdir().expect("temporary directory");
        let path = temporary.path().join("cc-switch.db");
        let owner = create_schema_12(&path);
        owner
            .execute_batch("BEGIN EXCLUSIVE; UPDATE profiles SET name = name;")
            .expect("hold exclusive database lock");
        let mut candidates = Vec::new();
        let mut sources = Vec::new();

        discover_cc_switch_at(&path, &mut candidates, &mut sources);

        assert!(candidates.is_empty());
        assert_eq!(source_state(&sources), "locked_or_corrupt");
        owner
            .execute_batch("ROLLBACK")
            .expect("release fixture lock");
    }

    #[test]
    fn cc_switch_schema_13_with_compatible_tables_is_accepted() {
        let temporary = tempfile::tempdir().expect("temporary directory");
        let path = temporary.path().join("cc-switch.db");
        let connection = create_schema_12(&path);
        connection
            .execute_batch("PRAGMA user_version = 13;")
            .expect("bump to schema 13");
        drop(connection);
        let mut candidates = Vec::new();
        let mut sources = Vec::new();

        discover_cc_switch_at(&path, &mut candidates, &mut sources);

        assert_eq!(source_state(&sources), "loaded");
        assert_eq!(sources[0]["schema_version"], 13);
        assert_eq!(sources[0]["schema_kind"], "loaded_compatible_newer");
        assert_eq!(sources[0]["profile_count"], 0);
    }

    #[test]
    fn cc_switch_newer_schema_without_tables_is_rejected() {
        let temporary = tempfile::tempdir().expect("temporary directory");
        let path = temporary.path().join("cc-switch.db");
        let connection = Connection::open(&path).expect("create empty db");
        connection
            .execute_batch("PRAGMA user_version = 13;")
            .expect("set future schema version");
        drop(connection);
        let mut candidates = Vec::new();
        let mut sources = Vec::new();

        discover_cc_switch_at(&path, &mut candidates, &mut sources);

        assert!(candidates.is_empty());
        assert_eq!(source_state(&sources), "unsupported_schema");
        assert_eq!(sources[0]["schema_version"], 13);
    }

    #[test]
    fn cc_switch_far_future_schema_with_tables_is_accepted_with_flag() {
        let temporary = tempfile::tempdir().expect("temporary directory");
        let path = temporary.path().join("cc-switch.db");
        let connection = create_schema_12(&path);
        connection
            .execute_batch("PRAGMA user_version = 25;")
            .expect("bump to far-future schema");
        drop(connection);
        let mut candidates = Vec::new();
        let mut sources = Vec::new();

        discover_cc_switch_at(&path, &mut candidates, &mut sources);

        assert_eq!(source_state(&sources), "loaded");
        assert_eq!(sources[0]["schema_version"], 25);
        assert_eq!(sources[0]["schema_kind"], "loaded_future_schema");
    }

    #[test]
    fn cc_switch_schema_12_returns_multiple_safe_candidates_without_secret_payloads() {
        const SECRET_MARKER: &str = "CC_SWITCH_SECRET_MUST_NOT_ESCAPE_8f31";
        let temporary = tempfile::tempdir().expect("temporary directory");
        let path = temporary.path().join("cc-switch.db");
        let connection = create_schema_12(&path);
        connection
            .execute(
                "INSERT INTO providers
                 (id, app_type, name, settings_config, is_current, sort_index)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    "claude-primary",
                    "claude",
                    "Claude primary",
                    json!({
                        "env": {
                            "ANTHROPIC_BASE_URL": "https://claude.example.test/v1",
                            "ANTHROPIC_MODEL": "claude-test",
                            "ANTHROPIC_AUTH_TOKEN": SECRET_MARKER
                        },
                        "private_metadata": SECRET_MARKER
                    })
                    .to_string(),
                    true,
                    1
                ],
            )
            .expect("insert claude provider");
        connection
            .execute(
                "INSERT INTO providers
                 (id, app_type, name, settings_config, is_current, sort_index)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    "opencode-secondary",
                    "opencode",
                    "OpenCode secondary",
                    json!({
                        "baseUrl": "https://opencode.example.test/v1",
                        "protocol": "open_ai",
                        "model": "safe-model"
                    })
                    .to_string(),
                    false,
                    2
                ],
            )
            .expect("insert opencode provider");
        connection
            .execute(
                "INSERT INTO proxy_config (app_type, listen_port, enabled, proxy_enabled)
                 VALUES ('codex', 15721, 1, 1)",
                [],
            )
            .expect("insert proxy config");
        connection
            .execute(
                "INSERT INTO profiles (id, name) VALUES ('one', 'One'), ('two', 'Two')",
                [],
            )
            .expect("insert profiles");
        drop(connection);
        let mut candidates = Vec::new();
        let mut sources = Vec::new();

        discover_cc_switch_at(&path, &mut candidates, &mut sources);

        assert_eq!(source_state(&sources), "loaded");
        assert_eq!(sources[0]["schema_version"], 12);
        assert_eq!(sources[0]["profile_count"], 2);
        assert_eq!(candidates.len(), 3);
        let claude = candidates
            .iter()
            .find(|candidate| candidate.id == "claude-primary")
            .expect("claude candidate");
        assert_eq!(claude.protocol, "anthropic");
        assert_eq!(claude.models, ["claude-test"]);
        assert_eq!(claude.auth_mode, "unavailable");
        assert!(claude.active);
        let opencode = candidates
            .iter()
            .find(|candidate| candidate.id == "opencode-secondary")
            .expect("opencode candidate");
        assert_eq!(opencode.models, ["safe-model"]);
        assert_eq!(opencode.auth_mode, "none");
        let proxy = candidates
            .iter()
            .find(|candidate| candidate.id == "cc-switch-proxy-codex")
            .expect("proxy candidate");
        assert_eq!(proxy.base_url.as_deref(), Some("http://127.0.0.1:15721"));
        assert!(proxy.active);

        let returned = serde_json::to_string(&(candidates, sources)).expect("serialize discovery");
        assert!(!returned.contains(SECRET_MARKER));
        assert!(!returned.contains("private_metadata"));
    }

    #[test]
    fn cc_switch_static_provider_switch_updates_redacted_snapshot_revision() {
        let temporary = tempfile::tempdir().expect("temporary directory");
        let path = temporary.path().join("cc-switch.db");
        let connection = create_schema_12(&path);
        for (id, model, is_current) in [
            ("codex-first", "model-first", true),
            ("codex-second", "model-second", false),
        ] {
            connection
                .execute(
                    "INSERT INTO providers
                     (id, app_type, name, settings_config, is_current, sort_index)
                     VALUES (?1, 'codex', ?2, ?3, ?4, ?5)",
                    params![
                        id,
                        id,
                        json!({
                            "baseUrl": "https://proxy.example.test/v1",
                            "protocol": "open_ai",
                            "model": model
                        })
                        .to_string(),
                        is_current,
                        if is_current { 1 } else { 2 },
                    ],
                )
                .expect("insert codex provider");
        }

        let discover_snapshot = || {
            let mut candidates = Vec::new();
            let mut sources = Vec::new();
            discover_cc_switch_at(&path, &mut candidates, &mut sources);
            (cc_switch_snapshot(sources.first(), &candidates), sources)
        };
        let (before, before_sources) = discover_snapshot();
        assert_eq!(source_state(&before_sources), "loaded");
        assert_eq!(before.selection_state, "resolved");
        assert_eq!(before.active_upstreams.len(), 1);
        assert_eq!(before.active_upstreams[0].app_type, "codex");
        assert_eq!(before.active_upstreams[0].profile_scope, "unknown");
        assert_eq!(
            before.active_upstreams[0].model.as_deref(),
            Some("model-first")
        );
        assert_eq!(before.active_upstreams[0].protocol, "open_ai");
        assert_eq!(
            before.active_upstreams[0].endpoint.as_deref(),
            Some("https://proxy.example.test/v1")
        );

        connection
            .execute(
                "UPDATE providers SET is_current = CASE id
                    WHEN 'codex-first' THEN 0
                    WHEN 'codex-second' THEN 1
                    ELSE is_current END
                 WHERE app_type = 'codex'",
                [],
            )
            .expect("switch current provider");

        let (after, after_sources) = discover_snapshot();
        assert_eq!(source_state(&after_sources), "loaded");
        assert_eq!(after.selection_state, "resolved");
        assert_eq!(after.active_upstreams.len(), 1);
        assert_eq!(
            after.active_upstreams[0].model.as_deref(),
            Some("model-second")
        );
        assert_ne!(after.fingerprint, before.fingerprint);
        assert_ne!(after.revision, before.revision);
    }

    #[test]
    #[ignore = "requires an explicitly supplied real cc-switch database; read-only static check"]
    fn live_cc_switch_static_snapshot_is_redacted_when_explicitly_enabled() {
        let snapshot = discover()["external_config_snapshot"]["cc_switch"].clone();
        assert_eq!(snapshot["source_state"], "loaded", "{snapshot:#}");
        let active_upstreams = snapshot["active_upstreams"]
            .as_array()
            .expect("redacted active-upstream list");
        let redacted = json!({
            "fingerprint": snapshot["fingerprint"],
            "revision": snapshot["revision"],
            "selection_state": snapshot["selection_state"],
            "active_upstreams": active_upstreams.iter().map(|upstream| json!({
                "app_type": upstream["app_type"],
                "profile_scope": upstream["profile_scope"],
                "model": upstream["model"],
                "protocol": upstream["protocol"],
                "endpoint": upstream["endpoint"],
                "agent_ids": upstream["agent_ids"],
                "credential_present": upstream["credential_present"],
            })).collect::<Vec<_>>(),
        });
        eprintln!("cc-switch static snapshot (redacted): {redacted}");
        let serialized = redacted.to_string();
        for forbidden in [
            "api_key",
            "apiKey",
            "token",
            "authorization",
            "settings_config",
        ] {
            assert!(
                !serialized.contains(forbidden),
                "snapshot leaked {forbidden}"
            );
        }
    }

    #[test]
    fn cc_switch_sync_state_returns_error_for_locked_corrupt_source() {
        let sources = vec![json!({"id":"cc-switch-sqlite","state":"locked_or_corrupt"})];
        let candidates = vec![];
        let result = cc_switch_sync_state(&sources, &candidates);
        assert_eq!(result["status"], "error");
        assert_eq!(result["error"], "locked_or_corrupt");
        assert_eq!(result["detected"], false);
        assert!(result["source_fingerprint"]
            .as_str()
            .is_some_and(|fp| fp.starts_with("cc-switch-sync-")));
        assert!(result["route_snapshot_identity"].is_null());
        assert!(result["last_receipt"].is_null());
    }

    #[test]
    fn cc_switch_sync_state_returns_error_for_unsupported_schema() {
        let sources = vec![json!({"id":"cc-switch-sqlite","state":"unsupported_schema"})];
        let candidates = vec![];
        let result = cc_switch_sync_state(&sources, &candidates);
        assert_eq!(result["status"], "error");
        assert_eq!(result["error"], "unsupported_schema");
        assert_eq!(result["detected"], false);
    }

    #[test]
    fn cc_switch_sync_state_returns_unavailable_for_missing_source() {
        let sources = vec![];
        let candidates = vec![];
        let result = cc_switch_sync_state(&sources, &candidates);
        assert_eq!(result["status"], "unavailable");
        assert_eq!(result["error"], Value::Null);
        assert_eq!(result["detected"], false);
    }

    fn cc_switch_active_candidate(id: &str, models: &[&str]) -> UpstreamCandidate {
        UpstreamCandidate {
            id: id.to_string(),
            name: id.to_string(),
            protocol: "open_ai".to_string(),
            base_url: Some(format!("http://127.0.0.1:19{id}")),
            models: models.iter().map(|model| model.to_string()).collect(),
            source: "cc-switch:claude".to_string(),
            freshness: "unix:1".to_string(),
            auth_mode: "none".to_string(),
            active: true,
            port: None,
            state: "discovered".to_string(),
            agent_ids: vec!["claude-code".to_string()],
            confidence: 96,
            reasons: vec!["cc-switch marks this provider as current".to_string()],
        }
    }

    #[test]
    fn cc_switch_sync_state_requires_selection_for_multiple_active_upstreams() {
        let sources = vec![json!({"id":"cc-switch-sqlite","state":"loaded"})];
        let candidates = vec![
            cc_switch_active_candidate("prod-a", &["model-a"]),
            cc_switch_active_candidate("prod-b", &["model-b"]),
        ];
        let result = cc_switch_sync_state(&sources, &candidates);
        assert_eq!(result["status"], "needs-selection");
        assert_eq!(result["detected"], true);
        assert_eq!(result["error"], Value::Null);
        assert_eq!(result["active_upstreams"].as_array().map(Vec::len), Some(2));
        assert!(result["writes_performed"] == false);
    }

    #[test]
    fn cc_switch_sync_state_reports_detected_without_active_upstream() {
        let sources = vec![json!({"id":"cc-switch-sqlite","state":"loaded"})];
        let result = cc_switch_sync_state(&sources, &[]);
        assert_eq!(result["status"], "detected");
        assert_eq!(result["detected"], true);
        assert!(result["upstream"].is_null());
        assert!(result["active_upstreams"]
            .as_array()
            .is_some_and(|items| items.is_empty()));
    }

    #[test]
    fn cc_switch_sync_state_single_active_upstream_is_confirmation_placeholder() {
        let sources = vec![json!({"id":"cc-switch-sqlite","state":"loaded"})];
        let candidates = vec![cc_switch_active_candidate("prod-a", &["model-a"])];
        let result = cc_switch_sync_state(&sources, &candidates);
        // The pre-enrichment placeholder; main.rs enrichment decides
        // synced/out-of-sync/needs-confirmation against the active route.
        assert_eq!(result["status"], "needs-confirmation");
        assert_eq!(result["upstream"], "http://127.0.0.1:19prod-a");
        assert_eq!(result["source"], "cc-switch-read-only");
        assert_eq!(result["writes_performed"], false);
        assert!(result["connection_path"]
            .as_str()
            .is_some_and(|path| path.contains("prod-a")));
    }

    #[test]
    fn cc_switch_sync_state_fingerprint_is_stable_and_tracks_active_upstreams() {
        let sources = vec![json!({"id":"cc-switch-sqlite","state":"loaded"})];
        let candidates = vec![cc_switch_active_candidate("prod-a", &["model-a"])];
        let first = cc_switch_sync_state(&sources, &candidates);
        let second = cc_switch_sync_state(&sources, &candidates);
        let fingerprint = first["source_fingerprint"]
            .as_str()
            .expect("sync fingerprint");
        assert!(fingerprint.starts_with("cc-switch-sync-"));
        assert_eq!(fingerprint, second["source_fingerprint"].as_str().unwrap());
        assert_eq!(
            first["source_revision"].as_str().unwrap(),
            &format!("cc-switch-sync-{fingerprint}")
        );

        let changed = cc_switch_sync_state(
            &sources,
            &[cc_switch_active_candidate("prod-b", &["model-b"])],
        );
        assert_ne!(fingerprint, changed["source_fingerprint"].as_str().unwrap());
    }

    #[test]
    fn refresh_coordinator_enforces_poll_interval_and_clamps_bounds() {
        let initial = test_external_snapshot("snapshot-a", "cc-a", Vec::new());
        let mut coordinator = ExternalConfigRefreshCoordinator::with_intervals(1_000, 100);
        assert!(coordinator.poll_due(0));
        coordinator.observe(0, initial.clone());
        // The poll deadline advances by the interval after every observe.
        assert!(!coordinator.poll_due(999));
        assert!(coordinator.poll_due(1_000));
        coordinator.observe(1_000, initial.clone());
        assert!(!coordinator.poll_due(1_999));
        assert!(coordinator.poll_due(2_000));

        // Intervals are clamped to the documented bounds: below the minimum,
        // above the maximum, and the debounce window cannot collapse to zero.
        let mut clamped_low = ExternalConfigRefreshCoordinator::with_intervals(50, 50);
        clamped_low.observe(0, initial.clone());
        assert!(!clamped_low.poll_due(999));
        assert!(clamped_low.poll_due(1_000));

        let mut clamped_high =
            ExternalConfigRefreshCoordinator::with_intervals(1_000_000, 1_000_000);
        clamped_high.observe(0, initial);
        assert!(!clamped_high.poll_due(299_999));
        assert!(clamped_high.poll_due(300_000));
    }
}
