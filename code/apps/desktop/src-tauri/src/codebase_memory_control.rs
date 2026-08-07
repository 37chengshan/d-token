use dtoken_codebase_memory::{
    artifact_spec_for_current_platform, inspect_cache, scope_ref,
    verify_upstream_artifact_with_digest, AgentSessionReceipt, IndexReceipt, UiHostReady,
    VerifiedUpstreamArtifact, AGENT_SESSION_RECEIPT_FILE, COMPONENT as LAUNCHER_COMPONENT,
    DEFAULT_CACHE_QUOTA_BYTES, POLICY_VERSION, PROTOCOL_VERSION, VERSION as LAUNCHER_VERSION,
};
use dtoken_config_patcher::codebase_memory_integration::{
    apply_codebase_memory_integration as apply_cbm_integration_tx,
    get_codebase_memory_integration_transaction as get_cbm_integration_tx,
    prepare_codebase_memory_integration as prepare_cbm_integration_tx,
    rollback_codebase_memory_integration as rollback_cbm_integration_tx,
    verify_codebase_memory_integration as verify_cbm_integration_tx,
    CodebaseMemoryIntegrationRequest, CodebaseMemoryIntegrationScope,
    CodebaseMemoryIntegrationSurface,
};
use dtoken_process::{
    executable_identity_matches, file_sha256, is_process_running, listener_owned_by_pid,
    process_birth_token, sanitize_command_output, terminate_process,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    env, fs,
    io::{BufRead, BufReader, Read, Write},
    net::TcpStream,
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    sync::{Mutex, OnceLock},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tauri::{AppHandle, Emitter};
use tauri_plugin_opener::OpenerExt;
use url::Url;
use uuid::Uuid;

const STATUS_SCHEMA_VERSION: u32 = 1;
const UI_HOST_TTL_SECS: u64 = 900;
const UI_READY_TIMEOUT_SECS: u64 = 5;
const LOOPBACK_CONNECT_TIMEOUT_MS: u64 = 1200;
const LOOPBACK_READ_TIMEOUT_MS: u64 = 1800;
const MAX_LOOPBACK_RESPONSE_BYTES: u64 = 64 * 1024;
const UPSTREAM_VERIFICATION_CACHE_TTL: Duration = Duration::from_secs(30);
const CODEBASE_MEMORY_SOURCE: &str = "d-token-codebase-memory";
const SELECTION_STATE_PATH: &[&str] = &["codebase-memory", "selection.json"];
const INTEGRATION_STATE_PATH: &[&str] = &["codebase-memory", "integration.json"];
const INTEGRATION_STATE_SCHEMA_VERSION: u32 = 2;
const MAX_INTEGRATION_STATE_RECORDS: usize = 128;
const INDEX_STATE_PATH: &[&str] = &["codebase-memory", "index.json"];
const PACKAGE_LOCATION_STATE_PATH: &[&str] = &["codebase-memory", "package-location.json"];
// Mirrors the release tag of the current manifest/allowlist
// (ARTIFACT_SPECS upstream_version) at release time; an app update bumps this
// URL together with the manifest so the download page stays current.
const UPSTREAM_RELEASE_URL: &str =
    "https://github.com/DeusData/codebase-memory-mcp/releases/tag/v0.9.1-rc.1";

static CONTROLLER: OnceLock<Mutex<ControllerState>> = OnceLock::new();
static STATUS_MONITOR: OnceLock<()> = OnceLock::new();
static UPSTREAM_VERIFICATION_CACHE: OnceLock<Mutex<UpstreamVerificationCache>> = OnceLock::new();
static INTEGRATION_STATE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

#[derive(Default)]
struct ControllerState {
    ui: Option<UiSession>,
    index: Option<IndexSession>,
}

struct UiSession {
    child: Child,
    stdin: Option<ChildStdin>,
    ready: UiHostReady,
    token: String,
    launcher_path: PathBuf,
    launcher_sha256: String,
    process_birth_token: String,
}

struct IndexSession {
    child: Child,
    stdin: Option<ChildStdin>,
    project_root: PathBuf,
    mode: String,
}

#[derive(Clone)]
struct ResolvedLauncher {
    path: PathBuf,
    sha256: String,
}

type ResolvedUpstream = VerifiedUpstreamArtifact;

#[derive(Debug, Clone, PartialEq, Eq)]
struct ArtifactFileIdentity {
    path: PathBuf,
    len: u64,
    modified_nanos: u128,
    platform_marker: String,
}

#[derive(Debug, Clone)]
struct CachedResolvedUpstream {
    identity: ArtifactFileIdentity,
    resolved: ResolvedUpstream,
    verified_at: Instant,
}

#[derive(Debug, Default)]
struct UpstreamVerificationCache {
    entry: Option<CachedResolvedUpstream>,
}

impl UpstreamVerificationCache {
    fn resolve_with<F>(&mut self, candidate: &Path, verify: F) -> Result<ResolvedUpstream, String>
    where
        F: FnOnce(&Path) -> Result<ResolvedUpstream, String>,
    {
        let before = artifact_file_identity(candidate)?;
        if let Some(entry) = self.entry.as_ref() {
            if entry.identity == before
                && entry.verified_at.elapsed() <= UPSTREAM_VERIFICATION_CACHE_TTL
            {
                return Ok(entry.resolved.clone());
            }
        }

        let resolved = verify(&before.path)?;
        let after = artifact_file_identity(&resolved.path)?;
        if before != after {
            self.entry = None;
            return Err("Code Graph upstream changed while it was being verified".to_string());
        }
        self.entry = Some(CachedResolvedUpstream {
            identity: after,
            resolved: resolved.clone(),
            verified_at: Instant::now(),
        });
        Ok(resolved)
    }

    fn clear(&mut self) {
        self.entry = None;
    }
}

#[derive(Clone)]
struct InstallContext {
    launcher: ResolvedLauncher,
    upstream: ResolvedUpstream,
}

#[derive(Clone)]
struct CodebaseMemoryPaths {
    project_root: PathBuf,
    cache_dir: PathBuf,
}

#[derive(Debug, Clone, Serialize)]
struct StatusDimensionDto {
    state: String,
    reason_code: String,
    detail: String,
    checked_at_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    owner: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    freshness: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    warnings: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CodebaseMemoryStatusDto {
    schema_version: u32,
    source: String,
    freshness: String,
    revision: String,
    approved_project_root: Option<String>,
    package_directory: String,
    package_directory_custom: bool,
    package: StatusDimensionDto,
    integration: StatusDimensionDto,
    agent_session: StatusDimensionDto,
    ui_session: StatusDimensionDto,
    index: StatusDimensionDto,
    active_integrations: Vec<CodebaseMemoryActiveIntegrationDto>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CodebaseMemoryActiveIntegrationDto {
    transaction_id: String,
    agent_id: String,
    state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    rollback_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CodebaseMemoryIntegrationPreviewDto {
    ok: bool,
    source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    summary: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    reviewed_digest: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    review_digest: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    diff: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    target: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    backup_summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    warnings: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    draft: Option<CodebaseMemoryDraftRequest>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CodebaseMemoryIntegrationReceiptDto {
    ok: bool,
    state: String,
    source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    summary: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    transaction_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    rollback_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    verified: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    new_session_required: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CodebaseMemoryActionResultDto {
    ok: bool,
    state: String,
    source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    summary: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CodebaseMemoryDraftRequest {
    agent_id: String,
    scope: String,
    project_root: String,
    surface: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ApplyCodebaseMemoryIntegrationPayload {
    draft: CodebaseMemoryDraftRequest,
    reviewed_digest: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CodebaseMemoryTransactionRequestPayload {
    transaction_id: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StartCodebaseMemoryIndexPayload {
    project_root: String,
    mode: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StartCodebaseMemoryUiPayload {
    project_root: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DeleteCodebaseMemoryIndexPayload {
    project_root: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SetCodebaseMemoryPackageDirectoryPayload {
    directory: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SelectionState {
    approved_project_root: String,
    updated_at_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PackageLocationState {
    directory: String,
    updated_at_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct IntegrationState {
    transaction_id: String,
    state: String,
    updated_at_ms: u64,
    target_path: String,
    new_session_required: bool,
    #[serde(default)]
    agent_id: String,
    #[serde(default)]
    scope: String,
    #[serde(default)]
    surface: String,
    #[serde(default)]
    project_root: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct IntegrationStateStore {
    schema_version: u32,
    transactions: Vec<IntegrationState>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct IndexStateRecord {
    state: String,
    outcome: Option<String>,
    reason_code: String,
    summary: String,
    updated_at_ms: u64,
    project_root: String,
    node_count: Option<u64>,
    edge_count: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct LauncherIdentity {
    component: String,
    protocol_version: u32,
    policy_version: u32,
    version: String,
    arch: String,
    os: String,
}

fn controller() -> &'static Mutex<ControllerState> {
    CONTROLLER.get_or_init(|| Mutex::new(ControllerState::default()))
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

fn default_dimension(
    state: &str,
    reason_code: &str,
    detail: impl Into<String>,
) -> StatusDimensionDto {
    StatusDimensionDto {
        state: state.to_string(),
        reason_code: reason_code.to_string(),
        detail: detail.into(),
        checked_at_ms: now_ms(),
        owner: Some("none".to_string()),
        source: Some(CODEBASE_MEMORY_SOURCE.to_string()),
        freshness: Some("live".to_string()),
        warnings: None,
    }
}

fn stale_dimension(
    state: &str,
    reason_code: &str,
    detail: impl Into<String>,
) -> StatusDimensionDto {
    StatusDimensionDto {
        freshness: Some("recent".to_string()),
        ..default_dimension(state, reason_code, detail)
    }
}

fn action_result(
    ok: bool,
    state: &str,
    summary: impl Into<String>,
    error: Option<String>,
) -> CodebaseMemoryActionResultDto {
    CodebaseMemoryActionResultDto {
        ok,
        state: state.to_string(),
        source: CODEBASE_MEMORY_SOURCE.to_string(),
        error,
        summary: summary.into(),
    }
}

fn selection_state() -> Option<SelectionState> {
    let value = super::read_app_state(SELECTION_STATE_PATH).ok()?;
    serde_json::from_value(value).ok()
}

fn active_mcp_integration_state() -> Option<IntegrationState> {
    let mut records = integration_states();
    records.sort_by_key(|record| record.updated_at_ms);
    latest_active_mcp_state(&records)
}

fn latest_active_mcp_state(records: &[IntegrationState]) -> Option<IntegrationState> {
    records
        .iter()
        .rev()
        .find(|record| {
            record.surface == "mcp"
                && matches!(record.state.as_str(), "applied" | "verified" | "changed")
        })
        .cloned()
}

fn integration_states() -> Vec<IntegrationState> {
    let _guard = INTEGRATION_STATE_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    read_integration_states_unlocked()
}

fn read_integration_states_unlocked() -> Vec<IntegrationState> {
    let Ok(value) = super::read_app_state(INTEGRATION_STATE_PATH) else {
        return Vec::new();
    };
    if let Ok(store) = serde_json::from_value::<IntegrationStateStore>(value.clone()) {
        if store.schema_version == INTEGRATION_STATE_SCHEMA_VERSION {
            return store.transactions;
        }
        return Vec::new();
    }
    // Backward-compatible read of the original singleton state. The next
    // successful write migrates it to the bounded transaction set.
    serde_json::from_value::<IntegrationState>(value)
        .map(|record| vec![record])
        .unwrap_or_default()
}

fn index_state_record() -> Option<IndexStateRecord> {
    let value = super::read_app_state(INDEX_STATE_PATH).ok()?;
    serde_json::from_value(value).ok()
}

fn package_location_state() -> Option<PackageLocationState> {
    let value = super::read_app_state(PACKAGE_LOCATION_STATE_PATH).ok()?;
    serde_json::from_value(value).ok()
}

fn write_package_location_state(directory: &Path) -> Result<(), String> {
    super::write_json_app_state(
        PACKAGE_LOCATION_STATE_PATH,
        &json!(PackageLocationState {
            directory: directory.display().to_string(),
            updated_at_ms: now_ms(),
        }),
    )?;
    Ok(())
}

fn write_selection_state(project_root: &Path) -> Result<(), String> {
    super::write_json_app_state(
        SELECTION_STATE_PATH,
        &json!(SelectionState {
            approved_project_root: project_root.display().to_string(),
            updated_at_ms: now_ms(),
        }),
    )?;
    Ok(())
}

fn write_integration_state(record: &IntegrationState) -> Result<(), String> {
    let _guard = INTEGRATION_STATE_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let records = merge_integration_state(read_integration_states_unlocked(), record.clone());
    super::write_json_app_state(
        INTEGRATION_STATE_PATH,
        &json!(IntegrationStateStore {
            schema_version: INTEGRATION_STATE_SCHEMA_VERSION,
            transactions: records,
        }),
    )?;
    Ok(())
}

fn merge_integration_state(
    mut records: Vec<IntegrationState>,
    record: IntegrationState,
) -> Vec<IntegrationState> {
    if let Some(existing) = records
        .iter_mut()
        .find(|existing| existing.transaction_id == record.transaction_id)
    {
        *existing = record;
    } else {
        records.push(record);
    }
    records.sort_by_key(|item| item.updated_at_ms);
    if records.len() > MAX_INTEGRATION_STATE_RECORDS {
        records.drain(..records.len() - MAX_INTEGRATION_STATE_RECORDS);
    }
    records
}

fn integration_state_from_transaction(
    transaction_id: &str,
    state: &str,
    updated_at_ms: u64,
    new_session_required: bool,
) -> Result<IntegrationState, String> {
    let manifest = get_cbm_integration_tx(transaction_id, &super::config_backup_root())
        .map_err(|error| sanitize_command_output(&format!("{error:#}")))?;
    Ok(IntegrationState {
        transaction_id: manifest.transaction_id,
        state: state.to_string(),
        updated_at_ms,
        target_path: manifest.plan.target.target_path.display().to_string(),
        new_session_required,
        agent_id: manifest.plan.agent_id,
        scope: match manifest.plan.scope {
            CodebaseMemoryIntegrationScope::Global => "global",
            CodebaseMemoryIntegrationScope::Project => "project",
        }
        .to_string(),
        surface: match manifest.plan.surface {
            CodebaseMemoryIntegrationSurface::Mcp => "mcp",
            CodebaseMemoryIntegrationSurface::Hook => "hook",
        }
        .to_string(),
        project_root: manifest
            .plan
            .canonical_project_root
            .map(|root| root.display().to_string())
            .unwrap_or_default(),
    })
}

fn write_index_state(record: &IndexStateRecord) -> Result<(), String> {
    super::write_json_app_state(INDEX_STATE_PATH, &json!(record))?;
    Ok(())
}

fn canonical_project_root(raw: &str) -> Result<PathBuf, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("Project root is required".to_string());
    }
    let canonical = fs::canonicalize(trimmed)
        .map_err(|error| format!("Project root normalization failed: {error}"))?;
    if !canonical.is_dir() {
        return Err("Project root must be an existing directory".to_string());
    }
    Ok(canonical)
}

fn current_selection_root() -> Result<PathBuf, String> {
    if cfg!(debug_assertions) {
        if let Ok(explicit) = env::var("DTOKEN_CODEBASE_MEMORY_PROJECT_ROOT") {
            return canonical_project_root(&explicit);
        }
    }
    selection_state()
        .map(|record| canonical_project_root(&record.approved_project_root))
        .transpose()?
        .ok_or_else(|| "No approved project root is selected yet".to_string())
}

fn cache_base_dir() -> Result<PathBuf, String> {
    #[cfg(target_os = "windows")]
    {
        env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .map(|path| path.join("d-token").join("cache").join("codebase-memory"))
            .ok_or_else(|| "LOCALAPPDATA is required for codebase-memory cache".to_string())
    }
    #[cfg(target_os = "macos")]
    {
        env::var_os("HOME")
            .map(PathBuf::from)
            .map(|path| {
                path.join("Library")
                    .join("Caches")
                    .join("d-token")
                    .join("codebase-memory")
            })
            .ok_or_else(|| "HOME is required for codebase-memory cache".to_string())
    }
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    {
        if let Some(path) = env::var_os("XDG_CACHE_HOME") {
            return Ok(PathBuf::from(path).join("d-token").join("codebase-memory"));
        }
        env::var_os("HOME")
            .map(PathBuf::from)
            .map(|path| path.join(".cache").join("d-token").join("codebase-memory"))
            .ok_or_else(|| {
                "HOME or XDG_CACHE_HOME is required for codebase-memory cache".to_string()
            })
    }
}

fn default_package_directory() -> Result<PathBuf, String> {
    #[cfg(target_os = "windows")]
    {
        env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .map(|path| {
                path.join("d-token")
                    .join("codebase-memory")
                    .join("upstream")
            })
            .ok_or_else(|| {
                "LOCALAPPDATA is required for the Code Graph package directory".to_string()
            })
    }
    #[cfg(target_os = "macos")]
    {
        env::var_os("HOME")
            .map(PathBuf::from)
            .map(|path| {
                path.join("Library")
                    .join("Application Support")
                    .join("d-token")
                    .join("codebase-memory")
                    .join("upstream")
            })
            .ok_or_else(|| "HOME is required for the Code Graph package directory".to_string())
    }
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    {
        env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")))
            .map(|path| {
                path.join("d-token")
                    .join("codebase-memory")
                    .join("upstream")
            })
            .ok_or_else(|| {
                "A user data directory is required for the Code Graph package".to_string()
            })
    }
}

fn effective_package_directory() -> Result<(PathBuf, bool), String> {
    if let Some(record) = package_location_state() {
        let configured = PathBuf::from(record.directory);
        if !configured.is_absolute() {
            return Err("Configured Code Graph package directory is not absolute".to_string());
        }
        return Ok((configured, true));
    }
    Ok((default_package_directory()?, false))
}

fn validate_package_directory(path: &Path) -> Result<PathBuf, String> {
    let canonical = fs::canonicalize(path)
        .map_err(|error| format!("Code Graph package directory could not be resolved: {error}"))?;
    if !canonical.is_dir() || canonical.parent().is_none() {
        return Err(
            "Code Graph package location must be an existing non-root directory".to_string(),
        );
    }
    Ok(canonical)
}

fn upstream_names() -> &'static [&'static str] {
    if cfg!(target_os = "windows") {
        &["codebase-memory-mcp.exe"]
    } else {
        &["codebase-memory-mcp"]
    }
}

fn package_directory_candidates(directory: &Path) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    for base in [directory.to_path_buf(), directory.join("bin")] {
        for name in upstream_names() {
            candidates.push(base.join(name));
        }
    }
    if let Ok(entries) = fs::read_dir(directory) {
        for entry in entries.flatten().take(128) {
            let path = entry.path();
            let Ok(metadata) = fs::symlink_metadata(&path) else {
                continue;
            };
            if !metadata.is_dir() || metadata.file_type().is_symlink() {
                continue;
            }
            for name in upstream_names() {
                candidates.push(path.join(name));
                candidates.push(path.join("bin").join(name));
            }
        }
    }
    candidates
}

fn short_hash(path: &Path) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(path.display().to_string().as_bytes());
    format!("{:x}", digest)[..16].to_string()
}

fn project_cache_namespace(project_root: &Path) -> Result<PathBuf, String> {
    Ok(cache_base_dir()?.join(short_hash(project_root)))
}

fn project_paths(project_root: &Path) -> Result<CodebaseMemoryPaths, String> {
    let cache_dir = project_cache_namespace(project_root)?.join("ui");
    Ok(CodebaseMemoryPaths {
        project_root: project_root.to_path_buf(),
        cache_dir,
    })
}

fn integration_paths(project_root: &Path) -> Result<CodebaseMemoryPaths, String> {
    let cache_dir = project_cache_namespace(project_root)?.join("agent");
    Ok(CodebaseMemoryPaths {
        project_root: project_root.to_path_buf(),
        cache_dir,
    })
}

fn candidate_launcher_paths() -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    #[cfg(debug_assertions)]
    if let Ok(explicit) = env::var("DTOKEN_CODEBASE_MEMORY_LAUNCHER_EXE") {
        candidates.push(PathBuf::from(explicit));
    }
    if let Ok(current) = env::current_exe() {
        if let Some(dir) = current.parent() {
            for name in [
                "dtoken-codebase-memory-launcher-aarch64-apple-darwin",
                "dtoken-codebase-memory-launcher-x86_64-apple-darwin",
                "dtoken-codebase-memory-launcher-x86_64-pc-windows-msvc.exe",
                "dtoken-codebase-memory-launcher.exe",
                "dtoken-codebase-memory-launcher",
            ] {
                candidates.push(dir.join(name));
                candidates.push(dir.join("binaries").join(name));
            }
        }
    }
    #[cfg(debug_assertions)]
    for prefix in [
        PathBuf::from("code/apps/desktop/src-tauri/binaries"),
        PathBuf::from("code/target/debug"),
        PathBuf::from("code/target/release"),
        PathBuf::from("target/debug"),
        PathBuf::from("target/release"),
    ] {
        candidates.push(prefix.join("dtoken-codebase-memory-launcher"));
        candidates.push(prefix.join("dtoken-codebase-memory-launcher.exe"));
        candidates.push(prefix.join("dtoken-codebase-memory-launcher-aarch64-apple-darwin"));
        candidates.push(prefix.join("dtoken-codebase-memory-launcher-x86_64-apple-darwin"));
        candidates.push(prefix.join("dtoken-codebase-memory-launcher-x86_64-pc-windows-msvc.exe"));
    }
    candidates
}

fn current_platform_upstream_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Ok((directory, _)) = effective_package_directory() {
        candidates.extend(package_directory_candidates(&directory));
    }
    #[cfg(debug_assertions)]
    if let Ok(explicit) = env::var("DTOKEN_CODEBASE_MEMORY_UPSTREAM_EXE") {
        candidates.push(PathBuf::from(explicit));
    }
    candidates
}

fn canonical_existing_file(path: &Path, label: &str) -> Result<PathBuf, String> {
    let canonical = fs::canonicalize(path)
        .map_err(|error| format!("Failed to resolve {label} {}: {error}", path.display()))?;
    if !canonical.is_file() {
        return Err(format!("{label} is not a file"));
    }
    Ok(canonical)
}

fn resolve_launcher() -> Result<ResolvedLauncher, String> {
    for candidate in candidate_launcher_paths() {
        let candidate = if candidate.is_absolute() {
            candidate
        } else {
            env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("."))
                .join(candidate)
        };
        if !candidate.is_file() {
            continue;
        }
        let canonical = canonical_existing_file(&candidate, "launcher executable")?;
        super::verify_agent_bridge_platform_signature(&canonical)?;
        let output = Command::new(&canonical)
            .arg("identity")
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .map_err(|error| format!("Failed to inspect launcher identity: {error}"))?;
        if !output.status.success() || output.stdout.len() > 16 * 1024 {
            continue;
        }
        let identity: LauncherIdentity = serde_json::from_slice(&output.stdout)
            .map_err(|error| format!("Launcher identity output is invalid: {error}"))?;
        if identity.component != LAUNCHER_COMPONENT
            || identity.protocol_version != PROTOCOL_VERSION
            || identity.policy_version != POLICY_VERSION
            || identity.version != LAUNCHER_VERSION
            || identity.arch != env::consts::ARCH
            || identity.os != env::consts::OS
        {
            continue;
        }
        return Ok(ResolvedLauncher {
            path: canonical.clone(),
            sha256: file_sha256(&canonical)?,
        });
    }
    Err(
        "dtoken-codebase-memory-launcher was not found or did not satisfy identity verification"
            .to_string(),
    )
}

fn artifact_file_identity(path: &Path) -> Result<ArtifactFileIdentity, String> {
    let path = canonical_existing_file(path, "Code Graph upstream")?;
    let metadata = fs::metadata(&path)
        .map_err(|error| format!("Failed to inspect Code Graph upstream metadata: {error}"))?;
    let modified_nanos = metadata
        .modified()
        .ok()
        .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
        .map(|value| value.as_nanos())
        .unwrap_or(0);
    let platform_marker = platform_file_marker(&path, &metadata)?;
    Ok(ArtifactFileIdentity {
        path,
        len: metadata.len(),
        modified_nanos,
        platform_marker,
    })
}

#[cfg(unix)]
fn platform_file_marker(_path: &Path, metadata: &fs::Metadata) -> Result<String, String> {
    use std::os::unix::fs::MetadataExt;
    Ok(format!(
        "{}:{}:{}:{}",
        metadata.dev(),
        metadata.ino(),
        metadata.ctime(),
        metadata.ctime_nsec()
    ))
}

#[cfg(windows)]
fn platform_file_marker(path: &Path, _metadata: &fs::Metadata) -> Result<String, String> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        FileIdInfo, GetFileInformationByHandle, GetFileInformationByHandleEx,
        BY_HANDLE_FILE_INFORMATION, FILE_ID_INFO,
    };

    let file = fs::File::open(path).map_err(|error| {
        format!("Failed to open Code Graph upstream for identity verification: {error}")
    })?;
    let handle = file.as_raw_handle();

    let mut file_id = unsafe { std::mem::zeroed::<FILE_ID_INFO>() };
    let inspected_file_id = unsafe {
        GetFileInformationByHandleEx(
            handle,
            FileIdInfo,
            std::ptr::addr_of_mut!(file_id).cast(),
            std::mem::size_of::<FILE_ID_INFO>() as u32,
        )
    };
    if inspected_file_id != 0 {
        let identifier = file_id
            .FileId
            .Identifier
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        return Ok(format!("{}:{identifier}", file_id.VolumeSerialNumber));
    }

    let mut info = unsafe { std::mem::zeroed::<BY_HANDLE_FILE_INFORMATION>() };
    let inspected = unsafe { GetFileInformationByHandle(handle, &mut info) };
    if inspected == 0 {
        return Err(format!(
            "Failed to inspect Code Graph upstream file identity: {}",
            std::io::Error::last_os_error()
        ));
    }

    Ok(format!(
        "{}:{}:{}",
        info.dwVolumeSerialNumber, info.nFileIndexHigh, info.nFileIndexLow
    ))
}

#[cfg(not(any(unix, windows)))]
fn platform_file_marker(_path: &Path, _metadata: &fs::Metadata) -> Result<String, String> {
    Ok("portable".to_string())
}

fn resolve_upstream() -> Result<ResolvedUpstream, String> {
    for candidate in current_platform_upstream_candidates() {
        let candidate = if candidate.is_absolute() {
            candidate
        } else {
            env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("."))
                .join(candidate)
        };
        if !candidate.is_file() {
            continue;
        }
        let resolved = UPSTREAM_VERIFICATION_CACHE
            .get_or_init(|| Mutex::new(UpstreamVerificationCache::default()))
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .resolve_with(&candidate, |path| {
                verify_upstream_artifact_with_digest(path)
                    .map_err(|error| sanitize_command_output(&format!("{error:#}")))
            });
        if let Ok(resolved) = resolved {
            return Ok(resolved);
        }
    }
    Err(
        "No manifest-verified codebase-memory upstream binary was found for this platform"
            .to_string(),
    )
}

fn resolve_install_context() -> Result<InstallContext, String> {
    Ok(InstallContext {
        launcher: resolve_launcher()?,
        upstream: resolve_upstream()?,
    })
}

fn build_integration_request(
    draft: &CodebaseMemoryDraftRequest,
    install: &InstallContext,
    paths: &CodebaseMemoryPaths,
) -> Result<CodebaseMemoryIntegrationRequest, String> {
    let agent_id = draft.agent_id.trim();
    if !matches!(agent_id, "opencode" | "codex" | "claude-code") {
        return Err("Agent id must be opencode, codex, or claude-code".to_string());
    }
    let scope = match draft.scope.trim() {
        "project" => CodebaseMemoryIntegrationScope::Project,
        "global" => CodebaseMemoryIntegrationScope::Global,
        _ => return Err("Scope must be project or global".to_string()),
    };
    let surface = match draft.surface.trim() {
        "mcp" => CodebaseMemoryIntegrationSurface::Mcp,
        "hook" => CodebaseMemoryIntegrationSurface::Hook,
        _ => return Err("Surface must be mcp or hook".to_string()),
    };
    Ok(CodebaseMemoryIntegrationRequest {
        agent_id: agent_id.to_string(),
        scope,
        project_root: Some(paths.project_root.clone()),
        launcher_path: install.launcher.path.clone(),
        launcher_sha256: install.launcher.sha256.clone(),
        upstream_path: install.upstream.path.clone(),
        upstream_sha256: install.upstream.sha256.clone(),
        cache_dir: paths.cache_dir.clone(),
        surface,
    })
}

fn validate_integration_paths(
    install: &InstallContext,
    paths: &CodebaseMemoryPaths,
) -> Result<(), String> {
    dtoken_codebase_memory::LaunchConfig::validate_verified(
        &install.upstream,
        &paths.project_root,
        &paths.cache_dir,
    )
    .map(|_| ())
    .map_err(|error| sanitize_command_output(&format!("{error:#}")))
}

fn build_paths_for_request(project_root: &str) -> Result<CodebaseMemoryPaths, String> {
    let project_root = canonical_project_root(project_root)?;
    integration_paths(&project_root)
}

fn status_revision() -> String {
    artifact_spec_for_current_platform()
        .map(|spec| format!("cbm-{}-{}", spec.upstream_version, spec.upstream_commit))
        .unwrap_or_else(|_| "cbm-unavailable".to_string())
}

fn cleanup_ui_session(state: &mut ControllerState) {
    let should_remove = state
        .ui
        .as_mut()
        .and_then(|session| session.child.try_wait().ok().flatten())
        .is_some();
    if should_remove {
        state.ui = None;
    }
}

fn index_record_from_receipt(receipt: IndexReceipt, project_root: &Path) -> IndexStateRecord {
    IndexStateRecord {
        state: if receipt.outcome == "ready" {
            "ready".to_string()
        } else {
            "stopped".to_string()
        },
        outcome: Some(receipt.outcome.clone()),
        reason_code: receipt.reason_code.clone(),
        summary: if receipt.outcome == "ready" {
            format!(
                "Index completed with {} nodes and {} edges.",
                receipt.node_count.unwrap_or(0),
                receipt.edge_count.unwrap_or(0)
            )
        } else if receipt.outcome == "cancelled" {
            "Indexing was cancelled before completion.".to_string()
        } else {
            "Indexing stopped without ready evidence.".to_string()
        },
        updated_at_ms: now_ms(),
        project_root: project_root.display().to_string(),
        node_count: receipt.node_count,
        edge_count: receipt.edge_count,
    }
}

fn refresh_index_session(state: &mut ControllerState) {
    let finished = state
        .index
        .as_mut()
        .and_then(|session| session.child.try_wait().ok().flatten())
        .map(|status| status.success());
    let Some(success) = finished else {
        return;
    };
    let Some(mut session) = state.index.take() else {
        return;
    };
    let mut output = String::new();
    if let Some(mut stdout) = session.child.stdout.take() {
        let _ = stdout.read_to_string(&mut output);
    }
    let record = if success {
        match serde_json::from_str::<IndexReceipt>(&output) {
            Ok(receipt) => index_record_from_receipt(receipt, &session.project_root),
            Err(error) => IndexStateRecord {
                state: "failed".to_string(),
                outcome: Some("invalid_receipt".to_string()),
                reason_code: "invalid_receipt".to_string(),
                summary: sanitize_command_output(&format!(
                    "Index process completed but receipt was invalid: {error}"
                )),
                updated_at_ms: now_ms(),
                project_root: session.project_root.display().to_string(),
                node_count: None,
                edge_count: None,
            },
        }
    } else {
        IndexStateRecord {
            state: "failed".to_string(),
            outcome: Some("process_failed".to_string()),
            reason_code: "process_failed".to_string(),
            summary: "Codebase Memory indexing failed before a valid receipt was emitted."
                .to_string(),
            updated_at_ms: now_ms(),
            project_root: session.project_root.display().to_string(),
            node_count: None,
            edge_count: None,
        }
    };
    let _ = write_index_state(&record);
    let _ = super::append_desktop_event(
        "codebase_memory_index",
        if success { "ok" } else { "failed" },
        json!({
            "project_ref": short_hash(&session.project_root),
            "state": record.state,
            "reason_code": record.reason_code,
            "mode": session.mode,
        }),
    );
}

fn package_dimension(checked_at_ms: u64) -> StatusDimensionDto {
    let launcher = resolve_launcher();
    let upstream = resolve_upstream();
    match (launcher, upstream) {
        (Ok(launcher), Ok(upstream)) => {
            let spec = artifact_spec_for_current_platform().ok();
            StatusDimensionDto {
                state: "ready".to_string(),
                reason_code: "package_verified".to_string(),
                detail: match spec {
                    Some(spec) => format!(
                        "Manifest-verified Code Graph release {} ({}) and constrained launcher are ready.",
                        spec.upstream_version, spec.upstream_commit
                    ),
                    None => "Manifest-verified Code Graph release and constrained launcher are ready.".to_string(),
                },
                checked_at_ms,
                owner: Some("dtoken".to_string()),
                source: Some(CODEBASE_MEMORY_SOURCE.to_string()),
                freshness: Some("live".to_string()),
                warnings: Some(vec![
                    format!("launcher_sha256={}", launcher.sha256),
                    format!("upstream_sha256={}", upstream.sha256),
                ]),
            }
        }
        (Err(error), Ok(_)) => default_dimension(
            "unavailable",
            "launcher_unavailable",
            sanitize_command_output(&error),
        ),
        (Ok(_), Err(error)) => {
            let candidate_present =
                effective_package_directory()
                    .ok()
                    .is_some_and(|(directory, _)| {
                        package_directory_candidates(&directory)
                            .iter()
                            .any(|path| path.is_file())
                    });
            if candidate_present {
                default_dimension(
                    "untrusted",
                    "upstream_untrusted",
                    sanitize_command_output(&error),
                )
            } else {
                default_dimension(
                    "unavailable",
                    "upstream_not_imported",
                    "Download the current Code Graph package from the official release and extract it into the selected package directory.",
                )
            }
        }
        (Err(launcher_error), Err(upstream_error)) => StatusDimensionDto {
            state: "unavailable".to_string(),
            reason_code: "package_unavailable".to_string(),
            detail:
                "Neither the constrained launcher nor a manifest-verified Code Graph artifact is available."
                    .to_string(),
            checked_at_ms,
            owner: Some("dtoken".to_string()),
            source: Some(CODEBASE_MEMORY_SOURCE.to_string()),
            freshness: Some("live".to_string()),
            warnings: Some(vec![
                sanitize_command_output(&launcher_error),
                sanitize_command_output(&upstream_error),
            ]),
        },
    }
}

fn integration_dimension(checked_at_ms: u64) -> StatusDimensionDto {
    let records = integration_states();
    let active: Vec<_> = records
        .iter()
        .filter(|record| record.state != "rolled_back")
        .collect();
    if !active.is_empty() && active.iter().all(|record| record.state == "verified") {
        return StatusDimensionDto {
            state: "ready".to_string(),
            reason_code: "integration_verified".to_string(),
            detail: format!(
                "{} Agent integration transaction(s) are read-back verified.",
                active.len()
            ),
            checked_at_ms,
            owner: Some("agent".to_string()),
            source: Some(CODEBASE_MEMORY_SOURCE.to_string()),
            freshness: Some("recent".to_string()),
            warnings: None,
        };
    }
    if active.iter().any(|record| record.state == "changed") {
        return stale_dimension(
            "failed",
            "integration_changed",
            "At least one Agent integration changed after apply and must be reviewed or rolled back.",
        );
    }
    if !active.is_empty() {
        return stale_dimension(
            "partial",
            "integration_unverified",
            format!(
                "{} Agent integration transaction(s) are applied but not all are verified.",
                active.len()
            ),
        );
    }
    if !records.is_empty() {
        return default_dimension(
            "not_configured",
            "integration_rolled_back",
            "All recorded Agent integrations were rolled back.",
        );
    }
    default_dimension(
        "not_configured",
        "integration_not_configured",
        "Agent MCP integration has not been configured.",
    )
}

fn agent_session_dimension(checked_at_ms: u64) -> StatusDimensionDto {
    let Some(integration) = active_mcp_integration_state() else {
        return default_dimension(
            "not_configured",
            "agent_session_not_configured",
            "Agent session status is unavailable until MCP integration is configured.",
        );
    };
    if !matches!(integration.state.as_str(), "verified" | "applied") {
        return default_dimension(
            "not_configured",
            "agent_mcp_not_configured",
            "The selected transaction does not configure an Agent MCP session.",
        );
    }
    let receipt = (|| -> Result<AgentSessionReceipt, String> {
        let root = if integration.project_root.trim().is_empty() {
            current_selection_root()?
        } else {
            canonical_project_root(&integration.project_root)?
        };
        let paths = integration_paths(&root)?;
        let receipt_path = paths.cache_dir.join(AGENT_SESSION_RECEIPT_FILE);
        let metadata = fs::symlink_metadata(&receipt_path)
            .map_err(|_| "Agent session receipt is unavailable".to_string())?;
        if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > 64 * 1024 {
            return Err("Agent session receipt failed file validation".to_string());
        }
        let bytes = fs::read(&receipt_path)
            .map_err(|error| format!("Agent session receipt could not be read: {error}"))?;
        let receipt: AgentSessionReceipt = serde_json::from_slice(&bytes)
            .map_err(|error| format!("Agent session receipt is invalid: {error}"))?;
        if receipt.schema_version != 1
            || receipt.owner != "agent"
            || receipt.project_ref != scope_ref(&root)
            || receipt.cache_ref != scope_ref(&paths.cache_dir)
        {
            return Err("Agent session receipt scope or ownership mismatch".to_string());
        }
        if matches!(receipt.state.as_str(), "exited" | "failed") {
            return Ok(receipt);
        }
        let install = resolve_install_context()?;
        if receipt.launcher_sha256 != install.launcher.sha256
            || receipt.upstream_sha256 != install.upstream.sha256
            || !is_process_running(receipt.pid)
            || process_birth_token(receipt.pid).as_deref()
                != Some(receipt.process_birth_token.as_str())
            || !executable_identity_matches(receipt.pid, &install.launcher.path)
        {
            return Err("Agent session process evidence is stale or mismatched".to_string());
        }
        Ok(receipt)
    })();
    match receipt {
        Ok(receipt)
            if receipt.state == "observed"
                && receipt.initialize_count > 0
                && receipt.tool_call_count > receipt.denied_count =>
        {
            StatusDimensionDto {
                state: "ready".to_string(),
                reason_code: "agent_session_verified".to_string(),
                detail: "The Agent-owned MCP session has verified initialize and tool-call evidence."
                    .to_string(),
                checked_at_ms,
                owner: Some("agent".to_string()),
                source: Some(CODEBASE_MEMORY_SOURCE.to_string()),
                freshness: Some(if checked_at_ms.saturating_sub(receipt.last_seen_at_ms) <= 300_000 {
                    "live"
                } else {
                    "stale"
                }.to_string()),
                warnings: None,
            }
        }
        Ok(receipt) if receipt.state == "observed" && receipt.initialize_count > 0 => StatusDimensionDto {
            state: "partial".to_string(),
            reason_code: "agent_session_observed".to_string(),
            detail: "The Agent-owned MCP session initialized, but no graph tool call is observed yet."
                .to_string(),
            checked_at_ms,
            owner: Some("agent".to_string()),
            source: Some(CODEBASE_MEMORY_SOURCE.to_string()),
            freshness: Some("recent".to_string()),
            warnings: None,
        },
        Ok(receipt) if receipt.state == "exited" => stale_dimension(
            "stopped",
            "agent_session_exited",
            "The last observed Agent-owned MCP session exited.",
        ),
        Ok(receipt) if receipt.state == "failed" => stale_dimension(
            "failed",
            "agent_session_failed",
            "The last observed Agent-owned MCP session failed.",
        ),
        Ok(_) => stale_dimension(
            "partial",
            "agent_session_starting",
            "The Agent-owned launcher started but MCP initialize evidence is not available yet.",
        ),
        Err(error) => StatusDimensionDto {
            state: "unknown".to_string(),
            reason_code: "new_session_required".to_string(),
            detail: "A new selected Agent session is required before Agent-side Codebase Memory status can be trusted."
                .to_string(),
            checked_at_ms,
            owner: Some("agent".to_string()),
            source: Some(CODEBASE_MEMORY_SOURCE.to_string()),
            freshness: Some("unknown".to_string()),
            warnings: Some(vec![sanitize_command_output(&error)]),
        },
    }
}

fn ui_session_dimension(checked_at_ms: u64) -> StatusDimensionDto {
    let mut guard = controller()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    cleanup_ui_session(&mut guard);
    let Some(session) = guard.ui.as_ref() else {
        return default_dimension(
            "stopped",
            "ui_session_stopped",
            "The d-token Codebase Memory graph page is not running.",
        );
    };
    match verify_ui_health(session) {
        Ok(health) => StatusDimensionDto {
            state: "ready".to_string(),
            reason_code: "ui_session_ready".to_string(),
            detail: format!(
                "Read-only graph host is verified on 127.0.0.1:{} for instance {}.",
                session.ready.port,
                health
                    .get("instance_id")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown")
            ),
            checked_at_ms,
            owner: Some("dtoken".to_string()),
            source: Some(CODEBASE_MEMORY_SOURCE.to_string()),
            freshness: Some("live".to_string()),
            warnings: None,
        },
        Err(error) => stale_dimension(
            "failed",
            "ui_session_unhealthy",
            sanitize_command_output(&error),
        ),
    }
}

fn index_dimension(
    checked_at_ms: u64,
    package_ready: bool,
    approved_project_root: Option<&str>,
) -> StatusDimensionDto {
    let mut guard = controller()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    refresh_index_session(&mut guard);
    if let Some(session) = guard.index.as_ref() {
        return StatusDimensionDto {
            state: "busy".to_string(),
            reason_code: "index_in_progress".to_string(),
            detail: format!(
                "Indexing {} with mode {}.",
                session.project_root.display(),
                session.mode
            ),
            checked_at_ms,
            owner: Some("dtoken".to_string()),
            source: Some(CODEBASE_MEMORY_SOURCE.to_string()),
            freshness: Some("live".to_string()),
            warnings: None,
        };
    }
    if !package_ready {
        return default_dimension(
            "unavailable",
            "index_unavailable",
            "Index status is unavailable until the current manifest-verified package is available.",
        );
    }
    if approved_project_root.is_none() {
        return default_dimension(
            "not_configured",
            "index_root_missing",
            "Select a project root before indexing.",
        );
    }
    match index_state_record() {
        Some(record) if record.state == "ready" => {
            let mut dimension = stale_dimension("ready", &record.reason_code, record.summary);
            if let Some(inspection) = approved_project_root
                .and_then(|value| canonical_project_root(value).ok())
                .and_then(|root| project_paths(&root).ok())
                .and_then(|paths| inspect_cache(&paths.cache_dir, checked_at_ms).ok())
            {
                dimension.warnings = Some(vec![format!(
                    "cache_bytes={}; quota_bytes={}; ttl_state={}",
                    inspection.bytes,
                    DEFAULT_CACHE_QUOTA_BYTES,
                    if inspection.stale { "stale" } else { "current" }
                )]);
            }
            dimension
        }
        Some(record) if record.state == "failed" => {
            stale_dimension("failed", &record.reason_code, record.summary)
        }
        Some(record) => stale_dimension("stopped", &record.reason_code, record.summary),
        None => default_dimension(
            "stopped",
            "index_not_started",
            "The approved project root has not been indexed by d-token yet.",
        ),
    }
}

fn package_is_ready(dimension: &StatusDimensionDto) -> bool {
    dimension.state == "ready"
}

fn file_fingerprint(path: &Path) -> String {
    fs::metadata(path)
        .ok()
        .and_then(|metadata| {
            let modified = metadata.modified().ok()?.duration_since(UNIX_EPOCH).ok()?;
            Some(format!("{}:{}", metadata.len(), modified.as_millis()))
        })
        .unwrap_or_else(|| "missing".to_string())
}

fn status_monitor_fingerprint() -> String {
    let receipt = current_selection_root()
        .ok()
        .and_then(|root| integration_paths(&root).ok())
        .map(|paths| file_fingerprint(&paths.cache_dir.join(AGENT_SESSION_RECEIPT_FILE)))
        .unwrap_or_else(|| "unselected".to_string());
    let index = file_fingerprint(&super::app_state_path(INDEX_STATE_PATH));
    let package = effective_package_directory()
        .ok()
        .map(|(directory, custom)| {
            let files = package_directory_candidates(&directory)
                .iter()
                .map(|path| file_fingerprint(path))
                .collect::<Vec<_>>()
                .join(",");
            format!("{custom}:{files}")
        })
        .unwrap_or_else(|| "package-unavailable".to_string());
    let guard = controller()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let ui_pid = guard
        .ui
        .as_ref()
        .map(|session| session.child.id())
        .unwrap_or(0);
    let index_pid = guard
        .index
        .as_ref()
        .map(|session| session.child.id())
        .unwrap_or(0);
    format!("{receipt}|{index}|{package}|{ui_pid}|{index_pid}")
}

fn start_status_monitor(app: &AppHandle) {
    let app = app.clone();
    STATUS_MONITOR.get_or_init(|| {
        std::thread::spawn(move || {
            let mut previous = status_monitor_fingerprint();
            loop {
                std::thread::sleep(Duration::from_secs(1));
                {
                    let mut guard = controller()
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    refresh_index_session(&mut guard);
                    cleanup_ui_session(&mut guard);
                }
                let current = status_monitor_fingerprint();
                if current != previous {
                    previous = current;
                    let _ = app.emit(
                        "dtoken:codebase-memory-status",
                        json!({"revision": now_ms()}),
                    );
                }
            }
        });
    });
}

fn build_codebase_memory_status(app: &AppHandle) -> CodebaseMemoryStatusDto {
    start_status_monitor(app);
    let checked_at_ms = now_ms();
    let package = package_dimension(checked_at_ms);
    let integration = integration_dimension(checked_at_ms);
    let approved_project_root = selection_state().map(|record| record.approved_project_root);
    let (package_directory, package_directory_custom) =
        effective_package_directory().unwrap_or_else(|_| (PathBuf::new(), false));
    let active_integrations = integration_states()
        .into_iter()
        .filter(|record| record.state != "rolled_back")
        .map(|record| {
            let rollback_id =
                get_cbm_integration_tx(&record.transaction_id, &super::config_backup_root())
                    .ok()
                    .and_then(|manifest| manifest.rollback_id);
            CodebaseMemoryActiveIntegrationDto {
                transaction_id: record.transaction_id,
                agent_id: record.agent_id,
                state: record.state,
                rollback_id,
            }
        })
        .collect();
    CodebaseMemoryStatusDto {
        schema_version: STATUS_SCHEMA_VERSION,
        source: CODEBASE_MEMORY_SOURCE.to_string(),
        freshness: "live".to_string(),
        revision: status_revision(),
        approved_project_root: approved_project_root.clone(),
        package_directory: package_directory.display().to_string(),
        package_directory_custom,
        package: package.clone(),
        integration,
        agent_session: agent_session_dimension(checked_at_ms),
        ui_session: ui_session_dimension(checked_at_ms),
        index: index_dimension(
            checked_at_ms,
            package_is_ready(&package),
            approved_project_root.as_deref(),
        ),
        active_integrations,
    }
}

#[tauri::command]
pub async fn get_codebase_memory_status(app: AppHandle) -> Result<CodebaseMemoryStatusDto, String> {
    tauri::async_runtime::spawn_blocking(move || build_codebase_memory_status(&app))
        .await
        .map_err(|_| "Code graph status worker stopped unexpectedly".to_string())
}

#[tauri::command]
pub fn set_codebase_memory_package_directory(
    app: AppHandle,
    request: SetCodebaseMemoryPackageDirectoryPayload,
) -> CodebaseMemoryActionResultDto {
    let result = (|| -> Result<String, String> {
        let directory = request
            .directory
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let selected = if let Some(directory) = directory {
            let selected = validate_package_directory(Path::new(directory))?;
            write_package_location_state(&selected)?;
            selected
        } else {
            let state_path = super::app_state_path(PACKAGE_LOCATION_STATE_PATH);
            if state_path.exists() {
                super::durable_remove_file(&state_path)?;
            }
            let selected = default_package_directory()?;
            fs::create_dir_all(&selected).map_err(|error| {
                format!("Default Code Graph package directory could not be created: {error}")
            })?;
            selected
        };
        UPSTREAM_VERIFICATION_CACHE
            .get_or_init(|| Mutex::new(UpstreamVerificationCache::default()))
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clear();
        let _ = app.emit(
            "dtoken:codebase-memory-status",
            json!({"revision": now_ms()}),
        );
        Ok(format!(
            "Code Graph package directory is now {}.",
            selected.display()
        ))
    })();
    match result {
        Ok(summary) => action_result(true, "ready", summary, None),
        Err(error) => action_result(
            false,
            "blocked",
            "Code Graph package directory was not changed.",
            Some(sanitize_command_output(&error)),
        ),
    }
}

#[tauri::command]
pub fn open_codebase_memory_package_directory(app: AppHandle) -> CodebaseMemoryActionResultDto {
    let result = (|| -> Result<(), String> {
        let (directory, custom) = effective_package_directory()?;
        if custom {
            validate_package_directory(&directory)?;
        } else {
            fs::create_dir_all(&directory).map_err(|error| {
                format!("Default Code Graph package directory could not be created: {error}")
            })?;
        }
        app.opener()
            .open_path(directory.display().to_string(), None::<&str>)
            .map_err(|error| format!("Code Graph package directory could not be opened: {error}"))
    })();
    match result {
        Ok(()) => action_result(true, "opened", "Code Graph package directory opened.", None),
        Err(error) => action_result(
            false,
            "failed",
            "Code Graph package directory could not be opened.",
            Some(sanitize_command_output(&error)),
        ),
    }
}

#[tauri::command]
pub fn open_codebase_memory_download_page(app: AppHandle) -> CodebaseMemoryActionResultDto {
    match app.opener().open_url(UPSTREAM_RELEASE_URL, None::<&str>) {
        Ok(()) => action_result(
            true,
            "opened",
            "Official Code Graph release page opened.",
            None,
        ),
        Err(error) => action_result(
            false,
            "failed",
            "Official Code Graph release page could not be opened.",
            Some(sanitize_command_output(&error.to_string())),
        ),
    }
}

#[tauri::command]
pub async fn preview_codebase_memory_integration(
    request: CodebaseMemoryDraftRequest,
) -> Result<CodebaseMemoryIntegrationPreviewDto, String> {
    tauri::async_runtime::spawn_blocking(move || preview_codebase_memory_integration_sync(request))
        .await
        .map_err(|_| "Code Graph integration preview worker stopped unexpectedly".to_string())
}

fn preview_codebase_memory_integration_sync(
    request: CodebaseMemoryDraftRequest,
) -> CodebaseMemoryIntegrationPreviewDto {
    let project_root = match build_paths_for_request(&request.project_root) {
        Ok(paths) => paths,
        Err(error) => {
            return CodebaseMemoryIntegrationPreviewDto {
                ok: false,
                source: CODEBASE_MEMORY_SOURCE.to_string(),
                error: Some(sanitize_command_output(&error)),
                summary: "Codebase Memory integration preview is blocked.".to_string(),
                reviewed_digest: None,
                review_digest: None,
                diff: None,
                target: None,
                backup_summary: None,
                warnings: None,
                draft: None,
            }
        }
    };
    let install = match resolve_install_context() {
        Ok(install) => install,
        Err(error) => {
            return CodebaseMemoryIntegrationPreviewDto {
                ok: false,
                source: CODEBASE_MEMORY_SOURCE.to_string(),
                error: Some(sanitize_command_output(&error)),
                summary: "Codebase Memory package verification failed.".to_string(),
                reviewed_digest: None,
                review_digest: None,
                diff: None,
                target: None,
                backup_summary: None,
                warnings: None,
                draft: None,
            }
        }
    };
    if let Err(error) = validate_integration_paths(&install, &project_root) {
        return CodebaseMemoryIntegrationPreviewDto {
            ok: false,
            source: CODEBASE_MEMORY_SOURCE.to_string(),
            error: Some(error),
            summary: "Project root or private Agent cache failed policy validation.".to_string(),
            reviewed_digest: None,
            review_digest: None,
            diff: None,
            target: None,
            backup_summary: None,
            warnings: None,
            draft: None,
        };
    }
    let integration_request = match build_integration_request(&request, &install, &project_root) {
        Ok(value) => value,
        Err(error) => {
            return CodebaseMemoryIntegrationPreviewDto {
                ok: false,
                source: CODEBASE_MEMORY_SOURCE.to_string(),
                error: Some(sanitize_command_output(&error)),
                summary: "Codebase Memory integration request is invalid.".to_string(),
                reviewed_digest: None,
                review_digest: None,
                diff: None,
                target: None,
                backup_summary: None,
                warnings: None,
                draft: None,
            }
        }
    };
    let home = match super::user_home_dir() {
        Ok(home) => home,
        Err(error) => {
            return CodebaseMemoryIntegrationPreviewDto {
                ok: false,
                source: CODEBASE_MEMORY_SOURCE.to_string(),
                error: Some(sanitize_command_output(&error)),
                summary: "User home directory is unavailable.".to_string(),
                reviewed_digest: None,
                review_digest: None,
                diff: None,
                target: None,
                backup_summary: None,
                warnings: None,
                draft: None,
            }
        }
    };
    match prepare_cbm_integration_tx(&integration_request, &home, &super::config_backup_root()) {
        Ok(prepared) => CodebaseMemoryIntegrationPreviewDto {
            ok: true,
            source: CODEBASE_MEMORY_SOURCE.to_string(),
            error: None,
            summary: "Codebase Memory integration preview is ready for review.".to_string(),
            reviewed_digest: Some(prepared.plan.review_digest.clone()),
            review_digest: Some(prepared.plan.review_digest.clone()),
            diff: Some(prepared.plan.target.diff.clone()),
            target: Some(prepared.plan.target.target_path.display().to_string()),
            backup_summary: Some(format!(
                "Atomic config backup will be stored under {} before apply.",
                super::config_backup_root().display()
            )),
            warnings: None,
            draft: Some(CodebaseMemoryDraftRequest {
                project_root: project_root.project_root.display().to_string(),
                ..request
            }),
        },
        Err(error) => CodebaseMemoryIntegrationPreviewDto {
            ok: false,
            source: CODEBASE_MEMORY_SOURCE.to_string(),
            error: Some(sanitize_command_output(&format!("{error:#}"))),
            summary: "Codebase Memory integration preview failed.".to_string(),
            reviewed_digest: None,
            review_digest: None,
            diff: None,
            target: None,
            backup_summary: None,
            warnings: None,
            draft: None,
        },
    }
}

#[tauri::command]
pub async fn apply_codebase_memory_integration(
    request: ApplyCodebaseMemoryIntegrationPayload,
) -> Result<CodebaseMemoryIntegrationReceiptDto, String> {
    tauri::async_runtime::spawn_blocking(move || apply_codebase_memory_integration_sync(request))
        .await
        .map_err(|_| "Code Graph integration apply worker stopped unexpectedly".to_string())
}

fn apply_codebase_memory_integration_sync(
    request: ApplyCodebaseMemoryIntegrationPayload,
) -> CodebaseMemoryIntegrationReceiptDto {
    let paths = match build_paths_for_request(&request.draft.project_root) {
        Ok(paths) => paths,
        Err(error) => {
            return failure_receipt(
                "blocked",
                "Codebase Memory integration apply is blocked.",
                &error,
            )
        }
    };
    let install = match resolve_install_context() {
        Ok(install) => install,
        Err(error) => {
            return failure_receipt(
                "failed",
                "Codebase Memory package verification failed.",
                &error,
            )
        }
    };
    if let Err(error) = validate_integration_paths(&install, &paths) {
        return failure_receipt(
            "blocked",
            "Project root or private Agent cache failed policy validation.",
            &error,
        );
    }
    let integration_request = match build_integration_request(&request.draft, &install, &paths) {
        Ok(value) => value,
        Err(error) => {
            return failure_receipt(
                "blocked",
                "Codebase Memory integration request is invalid.",
                &error,
            )
        }
    };
    let home = match super::user_home_dir() {
        Ok(home) => home,
        Err(error) => {
            return failure_receipt("failed", "User home directory is unavailable.", &error)
        }
    };
    let prepared =
        match prepare_cbm_integration_tx(&integration_request, &home, &super::config_backup_root())
        {
            Ok(prepared) => prepared,
            Err(error) => {
                return failure_receipt(
                    "failed",
                    "Codebase Memory integration prepare failed.",
                    &format!("{error:#}"),
                )
            }
        };
    match apply_cbm_integration_tx(
        prepared,
        &super::config_backup_root(),
        &request.reviewed_digest,
    ) {
        Ok(receipt) => {
            let record = IntegrationState {
                transaction_id: receipt.transaction_id.clone(),
                state: "applied".to_string(),
                updated_at_ms: now_ms(),
                target_path: receipt.target_path.display().to_string(),
                new_session_required: receipt.new_session_required,
                agent_id: request.draft.agent_id.clone(),
                scope: request.draft.scope.clone(),
                surface: request.draft.surface.clone(),
                project_root: paths.project_root.display().to_string(),
            };
            if let Err(error) = write_integration_state(&record)
                .and_then(|_| write_selection_state(&paths.project_root))
            {
                return match rollback_cbm_integration_tx(
                    &receipt.transaction_id,
                    &super::config_backup_root(),
                ) {
                    Ok(rollback) if rollback.restored => {
                        let mut rolled_back = record;
                        rolled_back.state = "rolled_back".to_string();
                        rolled_back.updated_at_ms = rollback.rolled_back_at_ms;
                        rolled_back.new_session_required = false;
                        let _ = write_integration_state(&rolled_back);
                        failure_receipt(
                            "restored",
                            "Codebase Memory integration state could not be committed, so the Agent configuration was restored.",
                            &error,
                        )
                    }
                    Ok(_) => recovery_required_receipt(
                        "Codebase Memory integration state could not be committed and automatic restoration was incomplete.",
                        &error,
                        &receipt.transaction_id,
                        receipt.rollback_id.clone(),
                    ),
                    Err(rollback_error) => recovery_required_receipt(
                        "Codebase Memory integration state could not be committed and automatic restoration failed.",
                        &format!("{error}; rollback: {rollback_error:#}"),
                        &receipt.transaction_id,
                        receipt.rollback_id.clone(),
                    ),
                };
            }
            let _ = super::append_desktop_event(
                "codebase_memory_integration_apply",
                "ok",
                json!({
                    "transaction_id": receipt.transaction_id,
                    "target_ref": short_hash(&receipt.target_path),
                    "agent_id": request.draft.agent_id,
                    "scope": request.draft.scope,
                    "surface": request.draft.surface,
                    "new_session_required": receipt.new_session_required,
                }),
            );
            CodebaseMemoryIntegrationReceiptDto {
                ok: true,
                state: "applied".to_string(),
                source: CODEBASE_MEMORY_SOURCE.to_string(),
                error: None,
                summary: "Codebase Memory Agent integration was written atomically. Start a new Agent session before runtime validation.".to_string(),
                transaction_id: Some(receipt.transaction_id),
                rollback_id: receipt.rollback_id,
                verified: Some(false),
                new_session_required: Some(receipt.new_session_required),
            }
        }
        Err(error) => failure_receipt(
            "failed",
            "Codebase Memory integration apply failed.",
            &format!("{error:#}"),
        ),
    }
}

#[tauri::command]
pub async fn verify_codebase_memory_integration(
    request: CodebaseMemoryTransactionRequestPayload,
) -> Result<CodebaseMemoryIntegrationReceiptDto, String> {
    tauri::async_runtime::spawn_blocking(move || verify_codebase_memory_integration_sync(request))
        .await
        .map_err(|_| "Code Graph integration verify worker stopped unexpectedly".to_string())
}

fn verify_codebase_memory_integration_sync(
    request: CodebaseMemoryTransactionRequestPayload,
) -> CodebaseMemoryIntegrationReceiptDto {
    match verify_cbm_integration_tx(&request.transaction_id, &super::config_backup_root()) {
        Ok(verification) => {
            let state = if verification.ok {
                "verified"
            } else {
                "changed"
            };
            let record = match integration_state_from_transaction(
                &verification.transaction_id,
                state,
                now_ms(),
                true,
            ) {
                Ok(record) => record,
                Err(error) => {
                    return recovery_required_receipt(
                        "Codebase Memory integration was verified but its recovery metadata could not be loaded.",
                        &error,
                        &verification.transaction_id,
                        None,
                    )
                }
            };
            if let Err(error) = write_integration_state(&record) {
                return recovery_required_receipt(
                    "Codebase Memory integration was verified but its recovery state could not be saved.",
                    &error,
                    &verification.transaction_id,
                    None,
                );
            }
            CodebaseMemoryIntegrationReceiptDto {
                ok: verification.ok,
                state: if verification.ok {
                    "verified"
                } else {
                    "changed"
                }
                .to_string(),
                source: CODEBASE_MEMORY_SOURCE.to_string(),
                error: None,
                summary: verification.detail,
                transaction_id: Some(verification.transaction_id),
                rollback_id: None,
                verified: Some(verification.ok),
                new_session_required: Some(true),
            }
        }
        Err(error) => failure_receipt(
            "failed",
            "Codebase Memory integration verify failed.",
            &format!("{error:#}"),
        ),
    }
}

#[tauri::command]
pub async fn rollback_codebase_memory_integration(
    request: CodebaseMemoryTransactionRequestPayload,
) -> Result<CodebaseMemoryIntegrationReceiptDto, String> {
    tauri::async_runtime::spawn_blocking(move || rollback_codebase_memory_integration_sync(request))
        .await
        .map_err(|_| "Code Graph integration rollback worker stopped unexpectedly".to_string())
}

fn rollback_codebase_memory_integration_sync(
    request: CodebaseMemoryTransactionRequestPayload,
) -> CodebaseMemoryIntegrationReceiptDto {
    match rollback_cbm_integration_tx(&request.transaction_id, &super::config_backup_root()) {
        Ok(receipt) => {
            let record = match integration_state_from_transaction(
                &receipt.transaction_id,
                "rolled_back",
                receipt.rolled_back_at_ms,
                false,
            ) {
                Ok(record) => record,
                Err(error) => {
                    return recovery_required_receipt(
                        "Codebase Memory Agent configuration was restored, but its status metadata could not be loaded.",
                        &error,
                        &receipt.transaction_id,
                        Some(format!("rollback-{}", receipt.rolled_back_at_ms)),
                    )
                }
            };
            if let Err(error) = write_integration_state(&record) {
                return recovery_required_receipt(
                    "Codebase Memory Agent configuration was restored, but its status could not be saved.",
                    &error,
                    &receipt.transaction_id,
                    Some(format!("rollback-{}", receipt.rolled_back_at_ms)),
                );
            }
            let _ = super::append_desktop_event(
                "codebase_memory_integration_rollback",
                "ok",
                json!({
                    "transaction_id": receipt.transaction_id,
                    "target_ref": short_hash(&receipt.target_path),
                    "restored": receipt.restored,
                }),
            );
            CodebaseMemoryIntegrationReceiptDto {
                ok: true,
                state: "restored".to_string(),
                source: CODEBASE_MEMORY_SOURCE.to_string(),
                error: None,
                summary: "Codebase Memory integration transaction was restored.".to_string(),
                transaction_id: Some(receipt.transaction_id),
                rollback_id: Some(format!("rollback-{}", receipt.rolled_back_at_ms)),
                verified: Some(receipt.restored),
                new_session_required: Some(false),
            }
        }
        Err(error) => failure_receipt(
            "failed",
            "Codebase Memory integration rollback failed.",
            &format!("{error:#}"),
        ),
    }
}

fn failure_receipt(state: &str, summary: &str, error: &str) -> CodebaseMemoryIntegrationReceiptDto {
    CodebaseMemoryIntegrationReceiptDto {
        ok: false,
        state: state.to_string(),
        source: CODEBASE_MEMORY_SOURCE.to_string(),
        error: Some(sanitize_command_output(error)),
        summary: summary.to_string(),
        transaction_id: None,
        rollback_id: None,
        verified: None,
        new_session_required: None,
    }
}

fn recovery_required_receipt(
    summary: &str,
    error: &str,
    transaction_id: &str,
    rollback_id: Option<String>,
) -> CodebaseMemoryIntegrationReceiptDto {
    CodebaseMemoryIntegrationReceiptDto {
        ok: false,
        state: "recovery_required".to_string(),
        source: CODEBASE_MEMORY_SOURCE.to_string(),
        error: Some(sanitize_command_output(error)),
        summary: summary.to_string(),
        transaction_id: Some(transaction_id.to_string()),
        rollback_id,
        verified: Some(false),
        new_session_required: Some(true),
    }
}

fn health_request(host: &str, port: u16, path: &str) -> Result<Value, String> {
    let address = format!("{host}:{port}");
    let mut stream = TcpStream::connect_timeout(
        &address
            .parse()
            .map_err(|error| format!("Loopback address parse failed: {error}"))?,
        Duration::from_millis(LOOPBACK_CONNECT_TIMEOUT_MS),
    )
    .map_err(|error| format!("Loopback connection failed: {error}"))?;
    let _ = stream.set_read_timeout(Some(Duration::from_millis(LOOPBACK_READ_TIMEOUT_MS)));
    let _ = stream.set_write_timeout(Some(Duration::from_millis(LOOPBACK_READ_TIMEOUT_MS)));
    let request = format!(
        "GET {path} HTTP/1.1\r\nHost: {host}:{port}\r\nConnection: close\r\nAccept: application/json\r\n\r\n"
    );
    stream
        .write_all(request.as_bytes())
        .map_err(|error| format!("Loopback request write failed: {error}"))?;
    let mut raw = String::new();
    stream
        .take(MAX_LOOPBACK_RESPONSE_BYTES + 1)
        .read_to_string(&mut raw)
        .map_err(|error| format!("Loopback response read failed: {error}"))?;
    if u64::try_from(raw.len()).unwrap_or(u64::MAX) > MAX_LOOPBACK_RESPONSE_BYTES {
        return Err("Loopback response exceeded the bounded size limit".to_string());
    }
    let body = raw
        .split_once("\r\n\r\n")
        .map(|(_, body)| body)
        .ok_or_else(|| "Loopback response body is missing".to_string())?;
    serde_json::from_str(body)
        .map_err(|error| format!("Loopback response JSON is invalid: {error}"))
}

fn verify_ui_health(session: &UiSession) -> Result<Value, String> {
    if !is_process_running(session.child.id()) {
        return Err("UI host process is not running".to_string());
    }
    let birth = process_birth_token(session.child.id())
        .ok_or_else(|| "UI host birth token is unavailable".to_string())?;
    if birth != session.process_birth_token {
        return Err("UI host birth token changed unexpectedly".to_string());
    }
    if !executable_identity_matches(session.child.id(), &session.launcher_path) {
        return Err("UI host executable path no longer matches the verified launcher".to_string());
    }
    let current_sha = file_sha256(&session.launcher_path)?;
    if current_sha != session.launcher_sha256 {
        return Err("UI host launcher hash changed after start".to_string());
    }
    if !listener_owned_by_pid(session.child.id(), session.ready.port) {
        return Err("UI host loopback port is no longer owned by the tracked process".to_string());
    }
    let health = health_request(
        &session.ready.host,
        session.ready.port,
        &format!("/s/{}/api/health", session.token),
    )?;
    if health.get("instance_id").and_then(Value::as_str) != Some(session.ready.instance_id.as_str())
    {
        return Err("UI host instance id does not match the verified session".to_string());
    }
    Ok(health)
}

fn random_token() -> String {
    format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}

fn wait_for_ready_line(stdout: impl Read + Send + 'static) -> Result<String, String> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        let result = reader.read_line(&mut line).map(|_| line);
        let _ = tx.send(result);
    });
    match rx.recv_timeout(Duration::from_secs(UI_READY_TIMEOUT_SECS)) {
        Ok(Ok(line)) if !line.trim().is_empty() => Ok(line),
        Ok(Ok(_)) => Err("UI host did not emit a ready receipt".to_string()),
        Ok(Err(error)) => Err(format!("UI host ready read failed: {error}")),
        Err(_) => Err("UI host did not become ready before timeout".to_string()),
    }
}

fn stop_ui_session_locked(state: &mut ControllerState) -> Result<(), String> {
    let Some(mut session) = state.ui.take() else {
        return Ok(());
    };
    if let Some(stdin) = session.stdin.take() {
        drop(stdin);
    }
    for _ in 0..20 {
        if session
            .child
            .try_wait()
            .map_err(|error| error.to_string())?
            .is_some()
        {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let validation = (|| {
        if !executable_identity_matches(session.child.id(), &session.launcher_path) {
            return Err(
                "UI host executable path no longer matches the verified launcher".to_string(),
            );
        }
        if file_sha256(&session.launcher_path)? != session.launcher_sha256 {
            return Err("UI host launcher hash changed after start".to_string());
        }
        let actual_birth = process_birth_token(session.child.id())
            .ok_or_else(|| "UI host birth token changed unexpectedly".to_string())?;
        if session.process_birth_token != actual_birth {
            return Err("UI host birth token changed unexpectedly".to_string());
        }
        if !listener_owned_by_pid(session.child.id(), session.ready.port) {
            return Err("UI host loopback listener ownership could not be revalidated".to_string());
        }
        Ok(())
    })();
    if let Err(error) = validation {
        state.ui = Some(session);
        return Err(error);
    }
    if let Err(error) = terminate_process(session.child.id()) {
        state.ui = Some(session);
        return Err(format!("UI host termination failed: {error}"));
    }
    Ok(())
}

#[tauri::command]
pub async fn start_codebase_memory_ui(
    request: StartCodebaseMemoryUiPayload,
) -> Result<CodebaseMemoryActionResultDto, String> {
    tauri::async_runtime::spawn_blocking(move || start_codebase_memory_ui_sync(request))
        .await
        .map_err(|_| "Code Graph UI worker stopped unexpectedly".to_string())
}

fn start_codebase_memory_ui_sync(
    request: StartCodebaseMemoryUiPayload,
) -> CodebaseMemoryActionResultDto {
    let project_root = match canonical_project_root(&request.project_root) {
        Ok(project_root) => project_root,
        Err(error) => {
            return CodebaseMemoryActionResultDto {
                ok: false,
                state: "blocked".to_string(),
                source: CODEBASE_MEMORY_SOURCE.to_string(),
                error: Some(sanitize_command_output(&error)),
                summary: "Select a valid project root before starting the graph page.".to_string(),
            }
        }
    };
    let paths = match project_paths(&project_root) {
        Ok(paths) => paths,
        Err(error) => {
            return CodebaseMemoryActionResultDto {
                ok: false,
                state: "failed".to_string(),
                source: CODEBASE_MEMORY_SOURCE.to_string(),
                error: Some(sanitize_command_output(&error)),
                summary: "Codebase Memory cache path is unavailable.".to_string(),
            }
        }
    };
    let install = match resolve_install_context() {
        Ok(install) => install,
        Err(error) => {
            return CodebaseMemoryActionResultDto {
                ok: false,
                state: "failed".to_string(),
                source: CODEBASE_MEMORY_SOURCE.to_string(),
                error: Some(sanitize_command_output(&error)),
                summary: "Manifest-verified Code Graph package is unavailable.".to_string(),
            }
        }
    };
    if let Err(error) = dtoken_codebase_memory::LaunchConfig::validate_verified(
        &install.upstream,
        &paths.project_root,
        &paths.cache_dir,
    ) {
        return CodebaseMemoryActionResultDto {
            ok: false,
            state: "blocked".to_string(),
            source: CODEBASE_MEMORY_SOURCE.to_string(),
            error: Some(sanitize_command_output(&format!("{error:#}"))),
            summary: "Project root or cache failed Codebase Memory policy validation.".to_string(),
        };
    }
    let mut guard = controller()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    cleanup_ui_session(&mut guard);
    refresh_index_session(&mut guard);
    if guard.index.is_some() {
        return CodebaseMemoryActionResultDto {
            ok: false,
            state: "busy".to_string(),
            source: CODEBASE_MEMORY_SOURCE.to_string(),
            error: None,
            summary: "Wait for or cancel indexing before starting the graph page.".to_string(),
        };
    }
    if guard.ui.is_some() {
        if let Err(error) = stop_ui_session_locked(&mut guard) {
            return CodebaseMemoryActionResultDto {
                ok: false,
                state: "blocked".to_string(),
                source: CODEBASE_MEMORY_SOURCE.to_string(),
                error: Some(sanitize_command_output(&error)),
                summary: "The existing graph host could not be stopped safely.".to_string(),
            };
        }
    }
    let token = random_token();
    let mut command = Command::new(&install.launcher.path);
    command
        .args([
            "ui-host",
            &install.upstream.path.display().to_string(),
            &paths.project_root.display().to_string(),
            &paths.cache_dir.display().to_string(),
            &std::process::id().to_string(),
            &UI_HOST_TTL_SECS.to_string(),
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            return CodebaseMemoryActionResultDto {
                ok: false,
                state: "failed".to_string(),
                source: CODEBASE_MEMORY_SOURCE.to_string(),
                error: Some(sanitize_command_output(&error.to_string())),
                summary: "Failed to start the constrained Codebase Memory graph host.".to_string(),
            }
        }
    };
    let mut stdin = match child.stdin.take() {
        Some(stdin) => stdin,
        None => {
            let _ = child.kill();
            return CodebaseMemoryActionResultDto {
                ok: false,
                state: "failed".to_string(),
                source: CODEBASE_MEMORY_SOURCE.to_string(),
                error: Some("UI host stdin was unavailable".to_string()),
                summary: "Failed to initialize the constrained graph host.".to_string(),
            };
        }
    };
    if let Err(error) = writeln!(stdin, "{}", json!({ "token": token })) {
        let _ = child.kill();
        return CodebaseMemoryActionResultDto {
            ok: false,
            state: "failed".to_string(),
            source: CODEBASE_MEMORY_SOURCE.to_string(),
            error: Some(sanitize_command_output(&error.to_string())),
            summary: "Failed to send the graph host authorization secret.".to_string(),
        };
    }
    let stdout = match child.stdout.take() {
        Some(stdout) => stdout,
        None => {
            let _ = child.kill();
            return CodebaseMemoryActionResultDto {
                ok: false,
                state: "failed".to_string(),
                source: CODEBASE_MEMORY_SOURCE.to_string(),
                error: Some("UI host stdout was unavailable".to_string()),
                summary: "Failed to read the graph host ready receipt.".to_string(),
            };
        }
    };
    let ready_line = match wait_for_ready_line(stdout) {
        Ok(line) => line,
        Err(error) => {
            let _ = child.kill();
            return CodebaseMemoryActionResultDto {
                ok: false,
                state: "failed".to_string(),
                source: CODEBASE_MEMORY_SOURCE.to_string(),
                error: Some(sanitize_command_output(&error)),
                summary: "The constrained graph host did not become ready.".to_string(),
            };
        }
    };
    let ready: UiHostReady = match serde_json::from_str(ready_line.trim()) {
        Ok(ready) => ready,
        Err(error) => {
            let _ = child.kill();
            return CodebaseMemoryActionResultDto {
                ok: false,
                state: "failed".to_string(),
                source: CODEBASE_MEMORY_SOURCE.to_string(),
                error: Some(sanitize_command_output(&error.to_string())),
                summary: "The graph host ready receipt was invalid.".to_string(),
            };
        }
    };
    let now = now_ms();
    if ready.schema_version != 1
        || ready.component != "dtoken-codebase-memory-ui-host"
        || ready.owner != "d-token"
        || ready.host != "127.0.0.1"
        || ready.port == 0
        || ready.parent_pid != std::process::id()
        || ready.policy_version != POLICY_VERSION
        || ready.instance_id.trim().is_empty()
        || ready.expires_at_ms <= now
        || ready.expires_at_ms > now.saturating_add(UI_HOST_TTL_SECS.saturating_mul(1000) + 5_000)
    {
        let _ = child.kill();
        return CodebaseMemoryActionResultDto {
            ok: false,
            state: "failed".to_string(),
            source: CODEBASE_MEMORY_SOURCE.to_string(),
            error: Some("Graph host ready receipt failed identity validation".to_string()),
            summary: "The graph host ready receipt did not match the requested managed instance."
                .to_string(),
        };
    }
    let process_birth_token = match process_birth_token(child.id()) {
        Some(token) => token,
        None => {
            let _ = child.kill();
            return CodebaseMemoryActionResultDto {
                ok: false,
                state: "failed".to_string(),
                source: CODEBASE_MEMORY_SOURCE.to_string(),
                error: Some("UI host birth token was unavailable".to_string()),
                summary: "The constrained graph host could not be verified safely.".to_string(),
            };
        }
    };
    let session = UiSession {
        child,
        stdin: Some(stdin),
        ready,
        token,
        launcher_path: install.launcher.path.clone(),
        launcher_sha256: install.launcher.sha256.clone(),
        process_birth_token,
    };
    if let Err(error) = verify_ui_health(&session) {
        let _ = terminate_process(session.child.id());
        return CodebaseMemoryActionResultDto {
            ok: false,
            state: "failed".to_string(),
            source: CODEBASE_MEMORY_SOURCE.to_string(),
            error: Some(sanitize_command_output(&error)),
            summary: "The constrained graph host failed post-start verification.".to_string(),
        };
    }
    if let Err(error) = write_selection_state(&paths.project_root) {
        let _ = terminate_process(session.child.id());
        return CodebaseMemoryActionResultDto {
            ok: false,
            state: "failed".to_string(),
            source: CODEBASE_MEMORY_SOURCE.to_string(),
            error: Some(sanitize_command_output(&error)),
            summary: "The graph host started but project approval state could not be saved."
                .to_string(),
        };
    }
    let _ = super::append_desktop_event(
        "codebase_memory_ui_start",
        "ok",
        json!({
            "project_ref": short_hash(&paths.project_root),
            "port": session.ready.port,
            "instance_id": session.ready.instance_id,
        }),
    );
    guard.ui = Some(session);
    CodebaseMemoryActionResultDto {
        ok: true,
        state: "ready".to_string(),
        source: CODEBASE_MEMORY_SOURCE.to_string(),
        error: None,
        summary: "The read-only Codebase Memory graph page is running on a verified loopback host."
            .to_string(),
    }
}

#[tauri::command]
pub async fn stop_codebase_memory_ui() -> Result<CodebaseMemoryActionResultDto, String> {
    tauri::async_runtime::spawn_blocking(stop_codebase_memory_ui_sync)
        .await
        .map_err(|_| "Code Graph UI stop worker stopped unexpectedly".to_string())
}

fn stop_codebase_memory_ui_sync() -> CodebaseMemoryActionResultDto {
    let mut guard = controller()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    match stop_ui_session_locked(&mut guard) {
        Ok(()) => CodebaseMemoryActionResultDto {
            ok: true,
            state: "stopped".to_string(),
            source: CODEBASE_MEMORY_SOURCE.to_string(),
            error: None,
            summary: "The read-only Codebase Memory graph page was stopped.".to_string(),
        },
        Err(error) => CodebaseMemoryActionResultDto {
            ok: false,
            state: "failed".to_string(),
            source: CODEBASE_MEMORY_SOURCE.to_string(),
            error: Some(sanitize_command_output(&error)),
            summary: "The read-only Codebase Memory graph page could not be stopped safely."
                .to_string(),
        },
    }
}

#[tauri::command]
pub async fn start_codebase_memory_index(
    request: StartCodebaseMemoryIndexPayload,
) -> Result<CodebaseMemoryActionResultDto, String> {
    tauri::async_runtime::spawn_blocking(move || start_codebase_memory_index_sync(request))
        .await
        .map_err(|_| "Code Graph index worker stopped unexpectedly".to_string())
}

fn start_codebase_memory_index_sync(
    request: StartCodebaseMemoryIndexPayload,
) -> CodebaseMemoryActionResultDto {
    let project_root =
        match canonical_project_root(&request.project_root).and_then(|root| project_paths(&root)) {
            Ok(paths) => paths,
            Err(error) => {
                return CodebaseMemoryActionResultDto {
                    ok: false,
                    state: "blocked".to_string(),
                    source: CODEBASE_MEMORY_SOURCE.to_string(),
                    error: Some(sanitize_command_output(&error)),
                    summary: "Select a valid project root before indexing.".to_string(),
                }
            }
        };
    let install = match resolve_install_context() {
        Ok(install) => install,
        Err(error) => {
            return CodebaseMemoryActionResultDto {
                ok: false,
                state: "failed".to_string(),
                source: CODEBASE_MEMORY_SOURCE.to_string(),
                error: Some(sanitize_command_output(&error)),
                summary: "Manifest-verified Code Graph package is unavailable.".to_string(),
            }
        }
    };
    let mode = match request.mode.as_str() {
        "explicit" => "fast",
        "fast" | "moderate" | "full" => request.mode.as_str(),
        _ => {
            return CodebaseMemoryActionResultDto {
                ok: false,
                state: "blocked".to_string(),
                source: CODEBASE_MEMORY_SOURCE.to_string(),
                error: Some("Index mode must be explicit, fast, moderate, or full".to_string()),
                summary: "Index request is invalid.".to_string(),
            }
        }
    };
    if let Err(error) = dtoken_codebase_memory::LaunchConfig::validate_verified(
        &install.upstream,
        &project_root.project_root,
        &project_root.cache_dir,
    ) {
        return CodebaseMemoryActionResultDto {
            ok: false,
            state: "blocked".to_string(),
            source: CODEBASE_MEMORY_SOURCE.to_string(),
            error: Some(sanitize_command_output(&format!("{error:#}"))),
            summary: "Project root or cache failed Codebase Memory policy validation.".to_string(),
        };
    }
    if let Err(error) = write_selection_state(&project_root.project_root) {
        return CodebaseMemoryActionResultDto {
            ok: false,
            state: "failed".to_string(),
            source: CODEBASE_MEMORY_SOURCE.to_string(),
            error: Some(sanitize_command_output(&error)),
            summary: "The approved project scope could not be saved before indexing.".to_string(),
        };
    }
    let mut guard = controller()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    refresh_index_session(&mut guard);
    cleanup_ui_session(&mut guard);
    if guard.index.is_some() {
        return CodebaseMemoryActionResultDto {
            ok: false,
            state: "busy".to_string(),
            source: CODEBASE_MEMORY_SOURCE.to_string(),
            error: None,
            summary: "An index operation is already running.".to_string(),
        };
    }
    if guard.ui.is_some() {
        return CodebaseMemoryActionResultDto {
            ok: false,
            state: "busy".to_string(),
            source: CODEBASE_MEMORY_SOURCE.to_string(),
            error: None,
            summary: "Stop the graph page before rebuilding its index.".to_string(),
        };
    }
    let mut child = match Command::new(&install.launcher.path)
        .args([
            "index",
            &install.upstream.path.display().to_string(),
            &project_root.project_root.display().to_string(),
            &project_root.cache_dir.display().to_string(),
            mode,
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(child) => child,
        Err(error) => {
            return CodebaseMemoryActionResultDto {
                ok: false,
                state: "failed".to_string(),
                source: CODEBASE_MEMORY_SOURCE.to_string(),
                error: Some(sanitize_command_output(&error.to_string())),
                summary: "Failed to start Codebase Memory indexing.".to_string(),
            }
        }
    };
    let stdin = child.stdin.take();
    guard.index = Some(IndexSession {
        child,
        stdin,
        project_root: project_root.project_root.clone(),
        mode: mode.to_string(),
    });
    let _ = write_index_state(&IndexStateRecord {
        state: "busy".to_string(),
        outcome: None,
        reason_code: "index_started".to_string(),
        summary: "Indexing the approved project in its private cache.".to_string(),
        updated_at_ms: now_ms(),
        project_root: project_root.project_root.display().to_string(),
        node_count: None,
        edge_count: None,
    });
    CodebaseMemoryActionResultDto {
        ok: true,
        state: "busy".to_string(),
        source: CODEBASE_MEMORY_SOURCE.to_string(),
        error: None,
        summary: "Codebase Memory indexing started in a constrained subprocess.".to_string(),
    }
}

#[tauri::command]
pub fn cancel_codebase_memory_index() -> CodebaseMemoryActionResultDto {
    let mut guard = controller()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let Some(session) = guard.index.as_mut() else {
        return CodebaseMemoryActionResultDto {
            ok: false,
            state: "stopped".to_string(),
            source: CODEBASE_MEMORY_SOURCE.to_string(),
            error: None,
            summary: "No Codebase Memory index operation is running.".to_string(),
        };
    };
    if let Some(stdin) = session.stdin.as_mut() {
        if let Err(error) = stdin.write_all(b"cancel\n") {
            return CodebaseMemoryActionResultDto {
                ok: false,
                state: "failed".to_string(),
                source: CODEBASE_MEMORY_SOURCE.to_string(),
                error: Some(sanitize_command_output(&error.to_string())),
                summary: "Failed to send Codebase Memory index cancellation.".to_string(),
            };
        }
    }
    CodebaseMemoryActionResultDto {
        ok: true,
        state: "busy".to_string(),
        source: CODEBASE_MEMORY_SOURCE.to_string(),
        error: None,
        summary: "Codebase Memory index cancellation was requested.".to_string(),
    }
}

fn selected_or_requested_project_root(value: Option<&str>) -> Result<PathBuf, String> {
    match value {
        Some(project_root) if !project_root.trim().is_empty() => {
            canonical_project_root(project_root)
        }
        _ => current_selection_root(),
    }
}

#[tauri::command]
pub async fn delete_codebase_memory_index(
    request: DeleteCodebaseMemoryIndexPayload,
) -> Result<CodebaseMemoryActionResultDto, String> {
    tauri::async_runtime::spawn_blocking(move || delete_codebase_memory_index_sync(request))
        .await
        .map_err(|_| "Code Graph index deletion worker stopped unexpectedly".to_string())
}

fn delete_codebase_memory_index_sync(
    request: DeleteCodebaseMemoryIndexPayload,
) -> CodebaseMemoryActionResultDto {
    let project_root =
        match selected_or_requested_project_root(request.project_root.as_deref()) {
            Ok(project_root) => project_root,
            Err(error) => return CodebaseMemoryActionResultDto {
                ok: false,
                state: "blocked".to_string(),
                source: CODEBASE_MEMORY_SOURCE.to_string(),
                error: Some(sanitize_command_output(&error)),
                summary:
                    "An approved project root is required before deleting Codebase Memory cache."
                        .to_string(),
            },
        };
    let cache_namespace = match project_cache_namespace(&project_root) {
        Ok(path) => path,
        Err(error) => {
            return CodebaseMemoryActionResultDto {
                ok: false,
                state: "failed".to_string(),
                source: CODEBASE_MEMORY_SOURCE.to_string(),
                error: Some(sanitize_command_output(&error)),
                summary: "The Codebase Memory cache path is unavailable.".to_string(),
            }
        }
    };
    let cache_base = match cache_base_dir() {
        Ok(path) => path,
        Err(error) => {
            return CodebaseMemoryActionResultDto {
                ok: false,
                state: "failed".to_string(),
                source: CODEBASE_MEMORY_SOURCE.to_string(),
                error: Some(sanitize_command_output(&error)),
                summary: "The Codebase Memory cache base path is unavailable.".to_string(),
            }
        }
    };
    let canonical_parent = cache_namespace
        .parent()
        .and_then(|path| fs::canonicalize(path).ok())
        .unwrap_or_else(|| cache_base.clone());
    let canonical_base = fs::canonicalize(&cache_base).unwrap_or(cache_base.clone());
    if canonical_parent != canonical_base && !canonical_parent.starts_with(&canonical_base) {
        return CodebaseMemoryActionResultDto {
            ok: false,
            state: "failed".to_string(),
            source: CODEBASE_MEMORY_SOURCE.to_string(),
            error: Some("Derived cache path escaped the d-token cache namespace".to_string()),
            summary: "The derived Codebase Memory cache path is unsafe.".to_string(),
        };
    }
    if let Ok(metadata) = fs::symlink_metadata(&cache_namespace) {
        if metadata.file_type().is_symlink() {
            return CodebaseMemoryActionResultDto {
                ok: false,
                state: "blocked".to_string(),
                source: CODEBASE_MEMORY_SOURCE.to_string(),
                error: Some(
                    "Refusing to delete a symlinked Codebase Memory cache directory".to_string(),
                ),
                summary: "The derived cache path is a symlink and was not deleted.".to_string(),
            };
        }
    }
    {
        let mut guard = controller()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        cleanup_ui_session(&mut guard);
        refresh_index_session(&mut guard);
        if guard
            .ui
            .as_ref()
            .is_some_and(|session| session.ready.port > 0 && session.child.id() > 0)
        {
            return CodebaseMemoryActionResultDto {
                ok: false,
                state: "busy".to_string(),
                source: CODEBASE_MEMORY_SOURCE.to_string(),
                error: Some("The d-token UI graph session is still active".to_string()),
                summary: "Stop the Codebase Memory graph page before deleting cache.".to_string(),
            };
        }
        if guard
            .index
            .as_ref()
            .is_some_and(|session| session.project_root == project_root)
        {
            return CodebaseMemoryActionResultDto {
                ok: false,
                state: "busy".to_string(),
                source: CODEBASE_MEMORY_SOURCE.to_string(),
                error: Some("The d-token index session is still active".to_string()),
                summary: "Cancel or wait for indexing before deleting cache.".to_string(),
            };
        }
    }
    if integration_states().iter().any(|record| {
        record.surface == "mcp"
            && matches!(record.state.as_str(), "applied" | "verified" | "changed")
            && (record.project_root.trim().is_empty()
                || canonical_project_root(&record.project_root)
                    .is_ok_and(|root| root == project_root))
    }) {
        return CodebaseMemoryActionResultDto {
            ok: false,
            state: "blocked".to_string(),
            source: CODEBASE_MEMORY_SOURCE.to_string(),
            error: Some(
                "Agent MCP integration is configured, so d-token cannot prove no Agent-owned session is using this cache".to_string(),
            ),
            summary: "Delete is blocked until the Codebase Memory Agent integration is rolled back or revalidated out of use."
                .to_string(),
        };
    }
    if !cache_namespace.exists() {
        return CodebaseMemoryActionResultDto {
            ok: true,
            state: "stopped".to_string(),
            source: CODEBASE_MEMORY_SOURCE.to_string(),
            error: None,
            summary: "The Codebase Memory cache directory was already absent.".to_string(),
        };
    }
    match fs::remove_dir_all(&cache_namespace) {
        Ok(()) => {
            let _ = write_index_state(&IndexStateRecord {
                state: "stopped".to_string(),
                outcome: Some("deleted".to_string()),
                reason_code: "cache_deleted".to_string(),
                summary:
                    "Deleted the d-token-owned Codebase Memory cache for the approved project."
                        .to_string(),
                updated_at_ms: now_ms(),
                project_root: project_root.display().to_string(),
                node_count: None,
                edge_count: None,
            });
            let _ = super::append_desktop_event(
                "codebase_memory_index_delete",
                "ok",
                json!({
                    "project_ref": short_hash(&project_root),
                    "reason_code": "cache_deleted",
                }),
            );
            CodebaseMemoryActionResultDto {
                ok: true,
                state: "stopped".to_string(),
                source: CODEBASE_MEMORY_SOURCE.to_string(),
                error: None,
                summary: "The d-token-owned Codebase Memory cache directory was deleted."
                    .to_string(),
            }
        }
        Err(error) => CodebaseMemoryActionResultDto {
            ok: false,
            state: "failed".to_string(),
            source: CODEBASE_MEMORY_SOURCE.to_string(),
            error: Some(sanitize_command_output(&error.to_string())),
            summary: "The d-token-owned Codebase Memory cache directory could not be deleted."
                .to_string(),
        },
    }
}

fn build_graph_url(session: &UiSession) -> Result<String, String> {
    let url = format!(
        "http://127.0.0.1:{}/s/{}/",
        session.ready.port, session.token
    );
    let parsed = Url::parse(&url).map_err(|error| format!("Graph URL parse failed: {error}"))?;
    if parsed.scheme() != "http"
        || parsed.host_str() != Some("127.0.0.1")
        || parsed.port() != Some(session.ready.port)
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.fragment().is_some()
        || !parsed.path().starts_with("/s/")
    {
        return Err("Graph URL failed loopback safety validation".to_string());
    }
    Ok(parsed.to_string())
}

#[tauri::command]
pub async fn open_codebase_memory_graph(
    app: AppHandle,
) -> Result<CodebaseMemoryActionResultDto, String> {
    tauri::async_runtime::spawn_blocking(move || open_codebase_memory_graph_sync(app))
        .await
        .map_err(|_| "Code Graph open worker stopped unexpectedly".to_string())
}

fn open_codebase_memory_graph_sync(app: AppHandle) -> CodebaseMemoryActionResultDto {
    let guard = controller()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let Some(session) = guard.ui.as_ref() else {
        return CodebaseMemoryActionResultDto {
            ok: false,
            state: "stopped".to_string(),
            source: CODEBASE_MEMORY_SOURCE.to_string(),
            error: None,
            summary: "Start the Codebase Memory graph page before opening it.".to_string(),
        };
    };
    if let Err(error) = verify_ui_health(session) {
        return CodebaseMemoryActionResultDto {
            ok: false,
            state: "failed".to_string(),
            source: CODEBASE_MEMORY_SOURCE.to_string(),
            error: Some(sanitize_command_output(&error)),
            summary: "The Codebase Memory graph page failed verification and was not opened."
                .to_string(),
        };
    }
    let url = match build_graph_url(session) {
        Ok(url) => url,
        Err(error) => {
            return CodebaseMemoryActionResultDto {
                ok: false,
                state: "failed".to_string(),
                source: CODEBASE_MEMORY_SOURCE.to_string(),
                error: Some(sanitize_command_output(&error)),
                summary: "The Codebase Memory graph URL failed validation.".to_string(),
            }
        }
    };
    match app.opener().open_url(url, None::<&str>) {
        Ok(()) => CodebaseMemoryActionResultDto {
            ok: true,
            state: "ready".to_string(),
            source: CODEBASE_MEMORY_SOURCE.to_string(),
            error: None,
            summary: "The verified read-only Codebase Memory graph page was opened.".to_string(),
        },
        Err(error) => CodebaseMemoryActionResultDto {
            ok: false,
            state: "failed".to_string(),
            source: CODEBASE_MEMORY_SOURCE.to_string(),
            error: Some(sanitize_command_output(&error.to_string())),
            summary: "The verified Codebase Memory graph page could not be opened.".to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_graph_url_requires_verified_loopback_shape() {
        let session = UiSession {
            child: Command::new(std::env::current_exe().expect("current exe"))
                .arg("--help")
                .spawn()
                .expect("spawn current exe"),
            stdin: None,
            ready: UiHostReady {
                schema_version: 1,
                component: "dtoken-codebase-memory-ui-host".to_string(),
                instance_id: "instance".to_string(),
                owner: "d-token".to_string(),
                host: "127.0.0.1".to_string(),
                port: 41234,
                parent_pid: 1,
                expires_at_ms: now_ms() + 1000,
                policy_version: 1,
            },
            token: "0123456789abcdef0123456789abcdef".to_string(),
            launcher_path: PathBuf::from("/tmp/launcher"),
            launcher_sha256: "sha".to_string(),
            process_birth_token: "birth".to_string(),
        };
        let url = build_graph_url(&session).expect("url");
        assert_eq!(
            url,
            "http://127.0.0.1:41234/s/0123456789abcdef0123456789abcdef/"
        );
    }

    #[test]
    fn short_hash_is_stable() {
        let path = PathBuf::from("/workspace/project");
        assert_eq!(short_hash(&path).len(), 16);
        assert_eq!(short_hash(&path), short_hash(&path));
    }

    #[test]
    fn package_directory_candidates_cover_common_extract_layouts() {
        let root = env::temp_dir().join(format!("dtoken-cbm-package-test-{}", Uuid::new_v4()));
        let nested = root.join("codebase-memory-mcp-v0.9.1-rc.1").join("bin");
        fs::create_dir_all(&nested).expect("create package layout");
        let binary = nested.join(upstream_names()[0]);
        fs::write(&binary, b"fixture").expect("write package fixture");

        let candidates = package_directory_candidates(&root);
        assert!(candidates.contains(&binary));
        assert!(candidates.len() <= 4 + 128 * 2);

        fs::remove_dir_all(root).expect("remove package fixture");
    }

    #[test]
    fn custom_package_directory_must_exist_and_not_be_a_root() {
        let missing = env::temp_dir().join(format!("dtoken-cbm-missing-{}", Uuid::new_v4()));
        assert!(validate_package_directory(&missing).is_err());
        assert!(validate_package_directory(Path::new("/")).is_err());
    }

    #[test]
    fn upstream_verification_cache_reuses_only_an_unchanged_file() {
        let root = env::temp_dir().join(format!("dtoken-cbm-cache-test-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).expect("create cache test root");
        let binary = root.join("codebase-memory-mcp");
        fs::write(&binary, b"first").expect("write first fixture");
        let calls = std::cell::Cell::new(0_u32);
        let mut cache = UpstreamVerificationCache::default();

        let verify = |path: &Path| {
            calls.set(calls.get() + 1);
            Ok(ResolvedUpstream {
                path: path.to_path_buf(),
                sha256: format!("sha-{}", calls.get()),
            })
        };
        let first = cache
            .resolve_with(&binary, verify)
            .expect("first verification");
        let second = cache
            .resolve_with(&binary, verify)
            .expect("cached verification");
        assert_eq!(calls.get(), 1);
        assert_eq!(first, second);

        fs::write(&binary, b"changed-and-longer").expect("change fixture");
        let third = cache
            .resolve_with(&binary, verify)
            .expect("changed verification");
        assert_eq!(calls.get(), 2);
        assert_ne!(second.sha256, third.sha256);

        fs::remove_dir_all(root).expect("remove cache test root");
    }

    #[test]
    fn integration_state_store_preserves_and_updates_each_batch_transaction() {
        let record =
            |transaction_id: &str, agent_id: &str, state: &str, updated_at_ms| IntegrationState {
                transaction_id: transaction_id.to_string(),
                state: state.to_string(),
                updated_at_ms,
                target_path: format!("/{agent_id}/config"),
                new_session_required: state != "rolled_back",
                agent_id: agent_id.to_string(),
                scope: "project".to_string(),
                surface: "mcp".to_string(),
                project_root: "/approved/project".to_string(),
            };

        let records = merge_integration_state(
            merge_integration_state(Vec::new(), record("tx-a", "opencode", "verified", 1)),
            record("tx-b", "codex", "verified", 2),
        );
        assert_eq!(records.len(), 2);

        let records =
            merge_integration_state(records, record("tx-a", "opencode", "rolled_back", 3));
        assert_eq!(records.len(), 2);
        assert_eq!(
            records
                .iter()
                .find(|item| item.transaction_id == "tx-a")
                .map(|item| item.state.as_str()),
            Some("rolled_back")
        );
        assert_eq!(
            records
                .iter()
                .find(|item| item.transaction_id == "tx-b")
                .map(|item| item.agent_id.as_str()),
            Some("codex")
        );

        let records = merge_integration_state(
            records,
            IntegrationState {
                surface: "hook".to_string(),
                ..record("tx-hook", "claude-code", "verified", 4)
            },
        );
        assert_eq!(
            latest_active_mcp_state(&records).map(|item| item.agent_id),
            Some("codex".to_string()),
            "a newer Hook transaction must not hide an active MCP integration"
        );
    }

    #[test]
    #[ignore = "requires DTOKEN_CBM_PERF_UPSTREAM_PATH pointing at the pinned local artifact"]
    fn measure_upstream_verification_cache_cold_and_warm() {
        let path = PathBuf::from(
            env::var("DTOKEN_CBM_PERF_UPSTREAM_PATH").expect("performance artifact path"),
        );
        let mut cache = UpstreamVerificationCache::default();
        let verify = |candidate: &Path| {
            verify_upstream_artifact_with_digest(candidate)
                .map_err(|error| sanitize_command_output(&format!("{error:#}")))
        };

        let cold_started = Instant::now();
        let cold = cache
            .resolve_with(&path, verify)
            .expect("cold artifact verification");
        let cold_elapsed = cold_started.elapsed();
        let warm_started = Instant::now();
        let warm = cache
            .resolve_with(&path, |_| panic!("warm lookup must not rehash"))
            .expect("warm artifact verification");
        let warm_elapsed = warm_started.elapsed();

        assert_eq!(cold, warm);
        assert!(warm_elapsed < cold_elapsed);
        eprintln!(
            "{{\"bytes\":{},\"cold_ms\":{},\"warm_us\":{}}}",
            fs::metadata(&path).expect("artifact metadata").len(),
            cold_elapsed.as_millis(),
            warm_elapsed.as_micros()
        );
    }

    #[test]
    fn tauri_command_payloads_use_one_request_layer() {
        let draft = serde_json::from_value::<CodebaseMemoryDraftRequest>(json!({
            "agent_id": "opencode",
            "scope": "project",
            "project_root": "/approved/project",
            "surface": "mcp"
        }))
        .expect("flat draft payload");
        assert_eq!(draft.agent_id, "opencode");
        assert!(serde_json::from_value::<CodebaseMemoryDraftRequest>(json!({
            "request": {
                "agent_id": "opencode",
                "scope": "project",
                "project_root": "/approved/project",
                "surface": "mcp"
            }
        }))
        .is_err());
    }

    #[test]
    fn cancelled_index_receipt_never_claims_completion() {
        let record = index_record_from_receipt(
            IndexReceipt {
                schema_version: 1,
                operation: "index_repository".to_string(),
                outcome: "cancelled".to_string(),
                reason_code: "cancelled_by_owner".to_string(),
                policy_version: POLICY_VERSION,
                indexed_files: None,
                node_count: Some(12),
                edge_count: Some(34),
                occurred_at_ms: now_ms(),
            },
            Path::new("/approved/project"),
        );
        assert_eq!(record.state, "stopped");
        assert_eq!(record.outcome.as_deref(), Some("cancelled"));
        assert_eq!(record.summary, "Indexing was cancelled before completion.");
        assert!(!record.summary.contains("completed with"));
    }
}
