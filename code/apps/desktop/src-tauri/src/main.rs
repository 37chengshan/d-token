mod agent_registry;
mod autostart;
mod codebase_memory_control;
mod learning_discovery;
mod opencode_adapter;
mod upstream_discovery;

use codebase_memory_control::{
    apply_codebase_memory_integration, cancel_codebase_memory_index, delete_codebase_memory_index,
    get_codebase_memory_status, open_codebase_memory_download_page, open_codebase_memory_graph,
    open_codebase_memory_package_directory, preview_codebase_memory_integration,
    rollback_codebase_memory_integration, set_codebase_memory_package_directory,
    start_codebase_memory_index, start_codebase_memory_ui, stop_codebase_memory_ui,
    verify_codebase_memory_integration,
};
use dtoken_agent_bridge::{BRIDGE_COMPONENT, BRIDGE_PROTOCOL_VERSION, BRIDGE_VERSION};
use dtoken_config_patcher::{
    agent_integration::{
        apply_agent_integration, get_agent_integration_transaction,
        inspect_agent_integration_evidence, prepare_agent_integration, rollback_agent_integration,
        verify_agent_integration_transaction, AgentIntegrationRequest, AgentIntegrationScope,
        AgentIntegrationTransactionManifest, AgentIntegrationTransactionState,
    },
    stable_hash, ConfigPatcher, ExpectedFileState,
};
use dtoken_doctor::{
    doctor_repair_modules, DoctorEvidence, DoctorEvidenceKind, DoctorEvidenceState,
    DoctorModuleEvidence, DoctorModuleId, DoctorRepairOutcome,
    DoctorRepairPlan as DoctorDomainPlan, DoctorRepairReceipt as DoctorDomainReceipt,
    DoctorRepairScope, DoctorRepairSelection, DoctorSnapshotSource,
    DoctorSnapshotV3 as DoctorDomainSnapshot, DoctorVerificationState,
};
use dtoken_event_ledger::JsonlStore;
use dtoken_process::{
    executable_identity_matches, is_process_running, listener_owned_by_pid, process_birth_token,
    sanitize_command_output, terminate_process, ProcessIdentity, ProcessManager,
    SystemProcessInspector,
};
#[cfg(debug_assertions)]
use dtoken_process::{opencode_command_plan, run_opencode_model_test};
use dtoken_types::{ConfigToolKind, RiskLevel};
use opencode_adapter::{
    OpenCodeConnectionReceipt, OpenCodeConnectionRequest, OpenCodeConnectionReview,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    env, fs,
    io::{Read, Write},
    net::{SocketAddr, TcpStream},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex, OnceLock},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use uuid::Uuid;

const DTOKEN_SERVICE_PORT: u16 = 18001;
const EXTERNAL_HEADROOM_REFERENCE_PORT: u16 = 8787;
const LEGACY_OPENCODE_REFERENCE_PORT: u16 = 8788;
const MANAGED_SERVICE_MANIFEST_SCHEMA_VERSION: u32 = 2;
const LAST_VERIFIED_DESKTOP_SUMMARY: &[&str] = &["dashboard", "last-verified-summary.json"];
const LAST_VERIFIED_DESKTOP_DETAIL: &[&str] = &["dashboard", "last-verified-detail.json"];
const MAX_NATIVE_PERF_OBSERVATIONS: usize = 256;
static NATIVE_PERF_OBSERVATIONS: OnceLock<Mutex<VecDeque<NativePerfObservation>>> = OnceLock::new();
static NATIVE_PERF_RUN_ID: OnceLock<String> = OnceLock::new();

#[derive(Clone, Serialize)]
struct NativePerfObservation {
    schema_version: u8,
    perf_run_id: String,
    layer: &'static str,
    operation: &'static str,
    phase: &'static str,
    duration_us: u64,
    result_state: &'static str,
    cache_hit: bool,
    error_code: Option<&'static str>,
}

fn record_native_performance(
    operation: &'static str,
    phase: &'static str,
    started_at: Instant,
    result_state: &'static str,
    cache_hit: bool,
    error_code: Option<&'static str>,
) {
    let observation = NativePerfObservation {
        schema_version: 1,
        perf_run_id: NATIVE_PERF_RUN_ID
            .get_or_init(|| Uuid::new_v4().to_string())
            .clone(),
        layer: "native",
        operation,
        phase,
        duration_us: u64::try_from(started_at.elapsed().as_micros()).unwrap_or(u64::MAX),
        result_state,
        cache_hit,
        error_code,
    };
    let mut observations = NATIVE_PERF_OBSERVATIONS
        .get_or_init(|| Mutex::new(VecDeque::with_capacity(MAX_NATIVE_PERF_OBSERVATIONS)))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if observations.len() == MAX_NATIVE_PERF_OBSERVATIONS {
        observations.pop_front();
    }
    observations.push_back(observation);
}

fn native_performance_snapshot() -> Vec<NativePerfObservation> {
    NATIVE_PERF_OBSERVATIONS
        .get_or_init(|| Mutex::new(VecDeque::with_capacity(MAX_NATIVE_PERF_OBSERVATIONS)))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .iter()
        .cloned()
        .collect()
}

#[derive(Serialize)]
struct ToolCheck {
    id: String,
    label: String,
    installed: bool,
    version: Option<String>,
    path: Option<String>,
    status: String,
    detail: String,
}

#[derive(Serialize)]
struct ActionLog {
    time: String,
    level: String,
    message: String,
}

#[derive(Serialize)]
struct ActionResult {
    ok: bool,
    summary: String,
    logs: Vec<ActionLog>,
    tools: Vec<ToolCheck>,
}

#[derive(Serialize, Deserialize)]
struct DoctorRemediation {
    kind: String,
    label: String,
    risk: String,
    writes_config: bool,
    requires_confirmation: bool,
    destination: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct DoctorIssue {
    id: String,
    category: String,
    severity: String,
    state: String,
    title: String,
    impact: String,
    cause: String,
    evidence: String,
    remediation: Option<DoctorRemediation>,
}

#[derive(Serialize, Deserialize)]
struct DoctorCounts {
    healthy: usize,
    repairable: usize,
    manual: usize,
    blocked: usize,
}

#[derive(Serialize, Deserialize)]
struct DoctorReport {
    ok: bool,
    #[serde(rename = "mode")]
    mode_name: String,
    revision: String,
    source: String,
    freshness: String,
    collected_at: String,
    summary: String,
    counts: DoctorCounts,
    issues: Vec<DoctorIssue>,
}

#[derive(Clone, Deserialize)]
struct DoctorRepairRequest {
    report_revision: String,
    issue_ids: Vec<String>,
    confirmed: Option<bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
struct DoctorModuleRepairSelection {
    #[serde(default)]
    agent_ids: Vec<String>,
    #[serde(default)]
    route_ids: Vec<String>,
    #[serde(default = "default_doctor_scope")]
    scope: String,
    project_root: Option<String>,
    #[serde(default = "default_true")]
    install_instructions: bool,
    #[serde(default = "default_true")]
    install_hook_or_plugin: bool,
    #[serde(default = "default_true")]
    install_skill: bool,
    #[serde(default)]
    install_mcp: bool,
}

fn default_doctor_scope() -> String {
    "global".to_string()
}

fn default_true() -> bool {
    true
}

#[derive(Clone, Debug, Deserialize)]
struct PreviewDoctorModuleRepairRequest {
    snapshot_revision: String,
    module_id: String,
    selection: DoctorModuleRepairSelection,
}

#[derive(Clone, Debug, Deserialize)]
struct ApplyDoctorModuleRepairRequest {
    plan_id: String,
    review_digest: String,
    snapshot_revision: String,
    module_id: String,
    selection: DoctorModuleRepairSelection,
    confirmed: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct StoredDoctorModulePlan {
    schema_version: u16,
    plan_id: String,
    snapshot_revision: String,
    module_id: String,
    selection: DoctorModuleRepairSelection,
    created_at_ms: u64,
    expires_at_ms: u64,
    review_digest: String,
    domain_plan: DoctorDomainPlan,
    agent_review_digests: HashMap<String, String>,
}

#[derive(Clone, Debug, Deserialize)]
struct VerifyDoctorRepairRequest {
    operation_id: Option<String>,
}

#[derive(Deserialize)]
struct PreviewRollbackRequest {
    rollback_id: String,
}

#[derive(Deserialize)]
struct ApplyRollbackRequest {
    rollback_id: String,
    reviewed_revision: String,
    expected_current_sha256: String,
    confirmed: bool,
}

#[derive(Serialize)]
struct HeadroomEndpoint {
    port: u16,
    label: String,
    healthy: bool,
    detail: String,
}

#[derive(Deserialize)]
struct InstallPlanRequest {
    target: String,
    install_root: String,
    proxy_port: u16,
    auto_backup: bool,
    run_doctor_after: bool,
    rollback_point: bool,
    mode: String,
}

#[derive(Deserialize)]
struct ProjectPolicyRecommendRequest {
    project_id: Option<String>,
    language_stack: Option<Vec<String>>,
    avg_request_tokens_7d: Option<u64>,
    avg_cli_noise_ratio_7d: Option<f32>,
    retrieve_rate_7d: Option<f32>,
    failure_rate_7d: Option<f32>,
    high_risk_task: Option<bool>,
    manual_override: Option<bool>,
}

#[derive(Deserialize)]
struct LearningManagedBlockRequest {
    target: String,
    content: String,
    reviewed_before_sha256: Option<String>,
    reviewed_after_sha256: Option<String>,
}

#[derive(Deserialize)]
struct LearningProjectAnalysisRequest {
    project_path: String,
}

#[derive(Deserialize)]
struct LearningDelegationRequest {
    request_id: String,
    agent: String,
    project_root: String,
    prompt: String,
    timeout_ms: u64,
    confirmed: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
struct DelegatedLearningContext {
    summary: String,
    conventions: Vec<String>,
    risks: Vec<String>,
    recommendations: Vec<String>,
    open_questions: Vec<String>,
}

static LEARNING_DELEGATIONS: OnceLock<Mutex<HashMap<String, Arc<Mutex<Child>>>>> = OnceLock::new();

#[derive(Deserialize)]
struct RouteProfileDraftRequest {
    route_id: String,
    route_name: String,
    provider_id: String,
    provider_name: String,
    provider_kind: String,
    protocol: String,
    base_url: String,
    api_key_ref: Option<String>,
    models: Vec<String>,
    chain_mode: String,
    compression_policy: String,
    local_port: u16,
    agent: String,
}

#[derive(Deserialize)]
struct ApplyRouteDraftRequest {
    route_id: String,
    reviewed_before_sha256: Option<String>,
    reviewed_after_sha256: Option<String>,
}

#[derive(Clone, Deserialize)]
struct OpticalGlobalDraftRequest {
    enabled: bool,
    default_mode: String,
}

#[derive(Clone, Deserialize)]
struct OpticalRouteDraftRequest {
    route_id: String,
    enabled: bool,
    mode: String,
    allow_tool_result: bool,
    allow_history: bool,
    confirmed_experimental_lossy: bool,
}

#[derive(Clone, Deserialize)]
struct OpticalConfigDraftRequest {
    global: OpticalGlobalDraftRequest,
    routes: Vec<OpticalRouteDraftRequest>,
}

#[derive(Deserialize)]
struct PreviewOpticalConfigRequest {
    draft: OpticalConfigDraftRequest,
}

#[derive(Deserialize)]
struct ApplyOpticalConfigRequest {
    draft: OpticalConfigDraftRequest,
    reviewed_before_sha256: Option<String>,
    reviewed_after_sha256: Option<String>,
    restart_service: Option<bool>,
}

#[derive(Deserialize)]
struct RollbackOpticalConfigRequest {
    rollback_id: String,
}

#[derive(Clone, Deserialize)]
struct HybridAgentIntegrationDraftRequest {
    agent_ids: Vec<String>,
    scope: String,
    project_root: Option<String>,
    install_instructions: bool,
    install_hook_or_plugin: bool,
    install_skill: bool,
    install_mcp: bool,
}

#[derive(Deserialize)]
struct ApplyHybridAgentIntegrationRequest {
    draft: HybridAgentIntegrationDraftRequest,
    reviewed_digest: String,
}

#[derive(Deserialize)]
struct VerifyHybridAgentIntegrationRequest {
    transaction_id: String,
}

#[derive(Deserialize)]
struct RollbackHybridAgentIntegrationRequest {
    transaction_id: String,
}

#[derive(Deserialize)]
struct GetHybridAgentIntegrationTransactionRequest {
    transaction_id: String,
}

#[derive(Deserialize)]
struct RouteDraftReviewRequest {
    route_id: String,
}

#[derive(Clone, Deserialize)]
struct ConnectionRouteBindingRequest {
    agent_id: String,
    candidate_id: String,
    candidate_source: String,
    model: String,
}

#[derive(Clone, Deserialize)]
struct ConnectionRoutePlanRequest {
    discovery_revision: String,
    bindings: Vec<ConnectionRouteBindingRequest>,
}

#[derive(Deserialize)]
struct ApplyConnectionRoutePlanRequest {
    plan: ConnectionRoutePlanRequest,
    reviewed_before_sha256: Option<String>,
    reviewed_after_sha256: Option<String>,
}

#[derive(Deserialize)]
struct RollbackConnectionRoutePlanRequest {
    rollback_id: String,
    expected_after_sha256: String,
}

#[derive(Deserialize)]
struct StoreProviderCredentialRequest {
    provider_id: String,
    credential: String,
}

#[derive(Deserialize)]
struct DeleteProviderCredentialRequest {
    provider_id: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
struct ManagedServiceManifest {
    #[serde(default)]
    schema_version: u32,
    managed_by: String,
    identity: String,
    pid: u32,
    port: u16,
    exe_path: String,
    #[serde(default)]
    exe_sha256: Option<String>,
    #[serde(default)]
    process_birth_token: Option<String>,
    #[serde(default)]
    runtime_instance_nonce: Option<String>,
    config_path: String,
    data_dir: String,
    started_at: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ManagedServiceBinaryEvidence {
    state: &'static str,
    launch_sha256: Option<String>,
    current_sha256: Option<String>,
    compatible: bool,
}

fn now_label() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() % 86_400)
        .unwrap_or_default();
    format!(
        "{:02}:{:02}:{:02}",
        secs / 3_600,
        (secs % 3_600) / 60,
        secs % 60
    )
}

fn now_timestamp() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| format!("unix:{}", duration.as_secs()))
        .unwrap_or_else(|_| "unix:0".to_string())
}

fn epoch_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(u128::from(u64::MAX)) as u64)
        .unwrap_or_default()
}

fn log(level: &str, message: impl Into<String>) -> ActionLog {
    ActionLog {
        time: now_label(),
        level: level.to_string(),
        message: message.into(),
    }
}

fn app_state_dir() -> PathBuf {
    if let Some(path) = env::var_os("DTOKEN_TEST_STATE_DIR") {
        return PathBuf::from(path);
    }

    #[cfg(target_os = "windows")]
    {
        env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .expect("LOCALAPPDATA is required for persistent d-token state")
            .join("d-token")
    }
    #[cfg(target_os = "macos")]
    {
        env::var_os("HOME")
            .map(PathBuf::from)
            .expect("HOME is required for persistent d-token state")
            .join("Library")
            .join("Application Support")
            .join("d-token")
    }
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    {
        if let Some(path) = env::var_os("XDG_DATA_HOME") {
            return PathBuf::from(path).join("d-token");
        }
        env::var_os("HOME")
            .map(PathBuf::from)
            .expect("HOME or XDG_DATA_HOME is required for persistent d-token state")
            .join(".local")
            .join("share")
            .join("d-token")
    }
}

fn app_state_path(relative: &[&str]) -> PathBuf {
    let mut path = app_state_dir();
    for part in relative {
        path.push(part);
    }
    path
}

fn user_home_dir() -> Result<PathBuf, String> {
    env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .map(PathBuf::from)
        .filter(|path| path.is_absolute() && path.is_dir())
        .ok_or_else(|| "An existing absolute user home directory is required".to_string())
}

fn sync_parent_directory(_path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        let parent = _path
            .parent()
            .ok_or_else(|| "state file has no parent directory".to_string())?;
        fs::File::open(parent)
            .and_then(|directory| directory.sync_all())
            .map_err(|err| format!("sync {}: {err}", parent.display()))?;
    }
    Ok(())
}

#[cfg(not(windows))]
fn atomic_replace_file(source: &Path, destination: &Path) -> Result<(), String> {
    fs::rename(source, destination).map_err(|err| {
        format!(
            "replace {} with {}: {err}",
            destination.display(),
            source.display()
        )
    })
}

#[cfg(windows)]
fn atomic_replace_file(source: &Path, destination: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };

    let source = source
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let destination = destination
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let moved = unsafe {
        MoveFileExW(
            source.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if moved == 0 {
        Err(format!(
            "atomic state replace failed: {}",
            std::io::Error::last_os_error()
        ))
    } else {
        Ok(())
    }
}

pub(crate) fn durable_atomic_write(path: &Path, content: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }

    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "state path has no valid file name".to_string())?;
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let temporary =
        path.with_file_name(format!(".{file_name}.{}.{}.tmp", std::process::id(), nonce));
    let result = (|| {
        let mut file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)
            .map_err(|err| format!("create {}: {err}", temporary.display()))?;
        file.write_all(content)
            .map_err(|err| format!("write {}: {err}", temporary.display()))?;
        file.sync_all()
            .map_err(|err| format!("sync {}: {err}", temporary.display()))?;
        atomic_replace_file(&temporary, path)?;
        sync_parent_directory(path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

pub(crate) fn durable_remove_file(path: &Path) -> Result<(), String> {
    match fs::remove_file(path) {
        Ok(()) => sync_parent_directory(path),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(format!("remove {}: {err}", path.display())),
    }
}

fn write_app_state(relative: &[&str], content: &str) -> Result<PathBuf, String> {
    let path = app_state_path(relative);
    durable_atomic_write(&path, content.as_bytes())?;
    Ok(path)
}

fn write_json_app_state(relative: &[&str], value: &Value) -> Result<PathBuf, String> {
    let content = serde_json::to_vec_pretty(value).map_err(|err| err.to_string())?;
    let path = app_state_path(relative);
    durable_atomic_write(&path, &content)?;
    Ok(path)
}

fn append_desktop_event(stage: &str, status: &str, metadata: Value) -> Result<(), String> {
    JsonlStore::new(app_state_path(&["events", "desktop-transactions.jsonl"]))
        .append(&json!({
            "schema_version": 1,
            "timestamp": now_timestamp(),
            "stage": stage,
            "source": "dtoken-desktop-control-plane",
            "status": status,
            "metadata": metadata,
            "privacy": "redacted metadata only; no credentials, prompt, response, or auth state"
        }))
        .map_err(|error| format!("append desktop transaction event: {error:#}"))
}

fn desktop_transaction_events() -> Vec<Value> {
    JsonlStore::new(app_state_path(&["events", "desktop-transactions.jsonl"]))
        .recent(100)
        .unwrap_or_default()
}

fn read_app_state(relative: &[&str]) -> Result<Value, String> {
    let path = app_state_path(relative);
    let content = fs::read_to_string(&path).map_err(|err| err.to_string())?;
    serde_json::from_str(&content).map_err(|err| err.to_string())
}

fn merge_json_objects(base: &mut Value, patch: Value) {
    match (base, patch) {
        (Value::Object(base_map), Value::Object(patch_map)) => {
            for (key, value) in patch_map {
                match (base_map.get_mut(&key), value) {
                    (Some(existing @ Value::Object(_)), next @ Value::Object(_)) => {
                        merge_json_objects(existing, next);
                    }
                    (_, next) => {
                        base_map.insert(key, next);
                    }
                }
            }
        }
        (base_slot, next) => {
            *base_slot = next;
        }
    }
}

fn active_config_path() -> PathBuf {
    app_state_dir()
        .join("active-config")
        .join("dtoken-config.json")
}

fn config_backup_root() -> PathBuf {
    fs::canonicalize(app_state_dir())
        .unwrap_or_else(|_| app_state_dir())
        .join("config-backups")
}

fn config_patcher() -> ConfigPatcher {
    ConfigPatcher::new(config_backup_root())
}

fn patchable_active_config_path() -> Result<PathBuf, String> {
    let path = active_config_path();
    let parent = path
        .parent()
        .ok_or_else(|| "Active config parent path is unavailable".to_string())?;
    let canonical_parent = fs::canonicalize(parent)
        .map_err(|err| format!("Active config parent normalization failed: {err}"))?;
    let file_name = path
        .file_name()
        .ok_or_else(|| "Active config file name is unavailable".to_string())?;
    Ok(canonical_parent.join(file_name))
}

fn active_route_config_status() -> Value {
    let path = active_config_path();
    let exists = path.exists();
    let mut status = json!({
        "source": "d-token-desktop",
        "exists": exists,
        "path": path.display().to_string(),
        "runtime_load_order": [
            "--config",
            "DTOKEN_CONFIG",
            "%LOCALAPPDATA%\\d-token\\active-config\\dtoken-config.json",
            "~/.dtoken/active-config/dtoken-config.json"
        ],
        "privacy": "d-token active config only; external OpenCode, Headroom, RTK, cc-switch, shell profile, and system proxy configs are not read."
    });
    if !exists {
        if let Some(object) = status.as_object_mut() {
            object.insert("state".to_string(), json!("missing"));
            object.insert("provider_count".to_string(), json!(0));
            object.insert("route_count".to_string(), json!(0));
            object.insert("instance_count".to_string(), json!(0));
        }
        return status;
    }

    let content = match fs::read_to_string(&path) {
        Ok(content) => content,
        Err(err) => {
            if let Some(object) = status.as_object_mut() {
                object.insert("state".to_string(), json!("unreadable"));
                object.insert("error".to_string(), json!(err.to_string()));
            }
            return status;
        }
    };
    let config: Value = match serde_json::from_str(&content) {
        Ok(config) => config,
        Err(err) => {
            if let Some(object) = status.as_object_mut() {
                object.insert("state".to_string(), json!("invalid_json"));
                object.insert("error".to_string(), json!(err.to_string()));
            }
            return status;
        }
    };

    let providers = config
        .get("providers")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let routes = config
        .get("routes")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let instances = config
        .get("instances")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let first_provider = providers.first().cloned().unwrap_or_else(|| json!({}));
    let first_route = routes.first().cloned().unwrap_or_else(|| json!({}));
    let first_instance = instances.first().cloned().unwrap_or_else(|| json!({}));
    let fallback_disabled = routes.iter().all(|route| {
        route
            .get("fallback_provider_id")
            .map(Value::is_null)
            .unwrap_or(true)
    });
    let provider_summaries = providers
        .iter()
        .map(|provider| {
            json!({
                "id": provider.get("id").cloned().unwrap_or(Value::Null),
                "protocol": provider.get("protocol").cloned().unwrap_or(Value::Null),
                "base_url": provider.get("base_url").cloned().unwrap_or(Value::Null),
                "models": provider.get("models").cloned().unwrap_or_else(|| json!([])),
            })
        })
        .collect::<Vec<_>>();
    if let Some(object) = status.as_object_mut() {
        object.insert("state".to_string(), json!("loaded"));
        object.insert("provider_count".to_string(), json!(providers.len()));
        object.insert("route_count".to_string(), json!(routes.len()));
        object.insert("instance_count".to_string(), json!(instances.len()));
        object.insert("fallback_disabled".to_string(), json!(fallback_disabled));
        object.insert("providers".to_string(), json!(provider_summaries));
        object.insert(
            "provider".to_string(),
            json!({
                "id": first_provider.get("id").cloned().unwrap_or(Value::Null),
                "name": first_provider.get("name").cloned().unwrap_or(Value::Null),
                "kind": first_provider.get("kind").cloned().unwrap_or(Value::Null),
                "protocol": first_provider.get("protocol").cloned().unwrap_or(Value::Null),
                "base_url": first_provider.get("base_url").cloned().unwrap_or(Value::Null),
                "models": first_provider.get("models").cloned().unwrap_or_else(|| json!([])),
                "has_api_key_ref": first_provider.get("api_key_ref").map(|value| !value.is_null()).unwrap_or(false),
            }),
        );
        object.insert(
            "route".to_string(),
            json!({
                "id": first_route.get("id").cloned().unwrap_or(Value::Null),
                "name": first_route.get("name").cloned().unwrap_or(Value::Null),
                "chain_mode": first_route.get("chain_mode").cloned().unwrap_or(Value::Null),
                "compression_policy": first_route.get("compression_policy").cloned().unwrap_or(Value::Null),
                "local_port": first_route.get("local_port").cloned().unwrap_or(Value::Null),
                "target_provider_id": first_route.get("target_provider_id").cloned().unwrap_or(Value::Null),
            }),
        );
        object.insert(
            "instance".to_string(),
            json!({
                "id": first_instance.get("id").cloned().unwrap_or(Value::Null),
                "agent": first_instance.get("agent").cloned().unwrap_or(Value::Null),
                "process_status": first_instance.get("process_status").cloned().unwrap_or(Value::Null),
            }),
        );
    }
    status
}

fn backup_original_text(backup: &dtoken_config_patcher::BackupRecord) -> Result<String, String> {
    if !backup.original.exists {
        return Err("Rollback point represents a previously missing target".to_string());
    }
    let content_file = backup
        .content_file
        .as_deref()
        .ok_or_else(|| "Rollback point is missing its verified original content".to_string())?;
    fs::read_to_string(backup.backup_dir.join(content_file))
        .map_err(|error| format!("Read verified rollback content: {error}"))
}

fn agent_integration_transaction_for_rollback(rollback_id: &str) -> Option<String> {
    let directory = config_backup_root().join("agent-integration-transactions");
    let entries = fs::read_dir(directory).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|extension| extension.to_str()) != Some("json") {
            continue;
        }
        let Some(transaction_id) = path.file_stem().and_then(|stem| stem.to_str()) else {
            continue;
        };
        let Ok(transaction) =
            get_agent_integration_transaction(transaction_id, &config_backup_root())
        else {
            continue;
        };
        if transaction
            .rollback_ids
            .iter()
            .any(|candidate| candidate == rollback_id)
        {
            return Some(transaction.transaction_id);
        }
    }
    None
}

fn agent_transaction_preflight(
    transaction_id: &str,
) -> Result<
    (
        AgentIntegrationTransactionManifest,
        Vec<dtoken_config_patcher::BackupRecord>,
        String,
        bool,
    ),
    String,
> {
    let transaction = get_agent_integration_transaction(transaction_id, &config_backup_root())
        .map_err(|error| format!("Agent transaction evidence is unavailable: {error:#}"))?;
    if transaction.rollback_ids.is_empty() {
        return Err("Agent transaction has no rollback evidence".to_string());
    }
    let patcher = config_patcher();
    let backups = transaction
        .rollback_ids
        .iter()
        .map(|rollback_id| {
            patcher
                .load_backup(rollback_id)
                .map_err(|error| format!("Agent rollback evidence is invalid: {error:#}"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let current_states = backups
        .iter()
        .map(|backup| {
            json!({
                "rollback_id": backup.rollback_id,
                "manifest_sha256": backup.manifest_sha256,
                "current": backup_expected_current_token(backup),
            })
        })
        .collect::<Vec<_>>();
    let expected_current = format!(
        "transaction-{}",
        stable_hash(
            &json!({
                "transaction_id": transaction.transaction_id,
                "current_states": current_states,
            })
            .to_string()
        )
    );
    let state_allows_restore = matches!(
        transaction.state,
        AgentIntegrationTransactionState::Applied | AgentIntegrationTransactionState::Verified
    );
    let currently_restorable =
        state_allows_restore && backups.iter().all(backup_matches_expected_current_state);
    Ok((transaction, backups, expected_current, currently_restorable))
}

fn agent_integration_transaction_points() -> (Vec<Value>, HashSet<String>) {
    let directory = config_backup_root().join("agent-integration-transactions");
    let Ok(entries) = fs::read_dir(directory) else {
        return (Vec::new(), HashSet::new());
    };
    let mut points = Vec::new();
    let mut transaction_rollback_ids = HashSet::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|extension| extension.to_str()) != Some("json") {
            continue;
        }
        let Some(transaction_id) = path.file_stem().and_then(|stem| stem.to_str()) else {
            continue;
        };
        match agent_transaction_preflight(transaction_id) {
            Ok((transaction, backups, expected_current, currently_restorable)) => {
                transaction_rollback_ids.extend(transaction.rollback_ids.iter().cloned());
                let primary_rollback_id = transaction
                    .rollback_ids
                    .first()
                    .cloned()
                    .unwrap_or_else(|| format!("agent-transaction-{transaction_id}"));
                let created_at = backups
                    .first()
                    .map(|backup| backup.created_at.clone())
                    .unwrap_or_else(|| transaction.applied_at_ms.to_string());
                let blocked_reason = if currently_restorable {
                    ""
                } else if transaction.state == AgentIntegrationTransactionState::RolledBack {
                    "This Agent integration transaction was already rolled back."
                } else {
                    "One or more Agent targets changed or the transaction is not in a restorable state."
                };
                points.push(json!({
                    "id": primary_rollback_id,
                    "label": format!("Agent integration transaction {}", transaction.transaction_id),
                    "when": created_at,
                    "files": transaction.rollback_ids.len(),
                    "tools": ["Agent Integration"],
                    "verified": true,
                    "integrity_verified": true,
                    "currently_restorable": currently_restorable,
                    "blocked_reason": blocked_reason,
                    "source": "dtoken-agent-integration",
                    "target": format!("{} managed Agent target(s)", transaction.rollback_ids.len()),
                    "scope": "agent-integration",
                    "executor_kind": "agent_integration",
                    "transaction_id": transaction.transaction_id,
                    "expected_current_sha256": expected_current,
                }));
            }
            Err(error) => points.push(json!({
                "id": format!("agent-transaction-{transaction_id}"),
                "label": format!("Unavailable Agent integration transaction {transaction_id}"),
                "when": "unknown",
                "files": 0,
                "tools": ["Agent Integration"],
                "verified": false,
                "integrity_verified": false,
                "currently_restorable": false,
                "blocked_reason": sanitize_command_output(&error),
                "source": "dtoken-agent-integration-invalid",
                "scope": "agent-integration",
                "executor_kind": "agent_integration",
                "transaction_id": transaction_id,
            })),
        }
    }
    (points, transaction_rollback_ids)
}

fn backup_changes_only_optical(backup: &dtoken_config_patcher::BackupRecord) -> bool {
    if patchable_active_config_path().ok().as_ref() != Some(&backup.target_path)
        || backup_current_sha256(backup).as_deref() != Some(&backup.planned_after_sha256)
    {
        return false;
    }
    let Ok(original_text) = backup_original_text(backup) else {
        return false;
    };
    let Ok(mut original) = serde_json::from_str::<Value>(&original_text) else {
        return false;
    };
    let Ok(current_text) = fs::read_to_string(&backup.target_path) else {
        return false;
    };
    let Ok(mut current) = serde_json::from_str::<Value>(&current_text) else {
        return false;
    };
    let Some(original_object) = original.as_object_mut() else {
        return false;
    };
    let Some(current_object) = current.as_object_mut() else {
        return false;
    };
    let original_optical = original_object.remove("optical");
    let current_optical = current_object.remove("optical");
    original == current && original_optical != current_optical
}

fn rollback_scope_for_backup(backup: &dtoken_config_patcher::BackupRecord) -> &'static str {
    if agent_integration_transaction_for_rollback(&backup.rollback_id).is_some() {
        "agent-integration"
    } else if backup_changes_only_optical(backup) {
        "optical-config"
    } else if backup
        .display_target
        .to_ascii_lowercase()
        .contains("opencode")
        || backup
            .target_path
            .file_name()
            .and_then(|value| value.to_str())
            == Some("opencode.json")
    {
        "opencode-config"
    } else if matches!(
        backup
            .target_path
            .file_name()
            .and_then(|value| value.to_str()),
        Some("AGENTS.md" | "CLAUDE.md" | "GEMINI.md" | "d-token.md")
    ) {
        "learning-managed-block"
    } else if patchable_active_config_path().ok().as_ref() == Some(&backup.target_path) {
        "dtoken-active-config"
    } else {
        "unknown"
    }
}

fn backup_current_sha256(backup: &dtoken_config_patcher::BackupRecord) -> Option<String> {
    file_sha256(&backup.target_path).ok()
}

fn rollback_expected_state_override_path(rollback_id: &str) -> PathBuf {
    app_state_dir()
        .join("rollback-expected-states")
        .join(format!("{rollback_id}.json"))
}

fn rollback_expects_missing_target(backup: &dtoken_config_patcher::BackupRecord) -> bool {
    let Ok(content) =
        fs::read_to_string(rollback_expected_state_override_path(&backup.rollback_id))
    else {
        return false;
    };
    let Ok(value) = serde_json::from_str::<Value>(&content) else {
        return false;
    };
    value.get("schema_version").and_then(Value::as_u64) == Some(1)
        && value.get("rollback_id").and_then(Value::as_str) == Some(&backup.rollback_id)
        && value.get("manifest_sha256").and_then(Value::as_str) == Some(&backup.manifest_sha256)
        && value.get("expected_state").and_then(Value::as_str) == Some("missing")
}

fn persist_missing_rollback_expectation(
    backup: &dtoken_config_patcher::BackupRecord,
) -> Result<(), String> {
    let file_name = format!("{}.json", backup.rollback_id);
    write_json_app_state(
        &["rollback-expected-states", &file_name],
        &json!({
            "schema_version": 1,
            "rollback_id": backup.rollback_id,
            "manifest_sha256": backup.manifest_sha256,
            "expected_state": "missing",
        }),
    )
    .map(|_| ())
}

fn backup_matches_expected_current_state(backup: &dtoken_config_patcher::BackupRecord) -> bool {
    if rollback_expects_missing_target(backup) {
        !backup.target_path.exists()
    } else {
        backup_current_sha256(backup).as_deref() == Some(&backup.planned_after_sha256)
    }
}

fn backup_expected_current_token(backup: &dtoken_config_patcher::BackupRecord) -> String {
    if rollback_expects_missing_target(backup) {
        "missing".to_string()
    } else {
        backup_current_sha256(backup).unwrap_or_default()
    }
}

fn generic_rollback_executor(scope: &str) -> bool {
    matches!(
        scope,
        "opencode-config" | "learning-managed-block" | "dtoken-active-config"
    )
}

fn rollback_executor_kind(scope: &str) -> &'static str {
    match scope {
        "agent-integration" => "agent_integration",
        "optical-config" => "optical",
        "opencode-config" | "learning-managed-block" | "dtoken-active-config" => "generic",
        _ => "locked",
    }
}

fn backup_currently_restorable(backup: &dtoken_config_patcher::BackupRecord) -> bool {
    let scope = rollback_scope_for_backup(backup);
    match scope {
        "agent-integration" => agent_integration_transaction_for_rollback(&backup.rollback_id)
            .and_then(|transaction_id| agent_transaction_preflight(&transaction_id).ok())
            .is_some_and(|(_, _, _, currently_restorable)| currently_restorable),
        "optical-config" => backup_matches_expected_current_state(backup),
        _ => generic_rollback_executor(scope) && backup_matches_expected_current_state(backup),
    }
}

fn rollback_blocked_reason(scope: &str, current_state_matches: bool) -> &'static str {
    match scope {
        "unknown" => "The backup target is outside the supported transaction allowlist.",
        _ if !current_state_matches => {
            "The current target state changed after this rollback point was created."
        }
        _ => "",
    }
}

fn rollback_points_result() -> Result<Vec<Value>, String> {
    let dir = app_state_dir().join("rollback-points");
    let mut points = Vec::new();
    match fs::read_dir(&dir) {
        Ok(entries) => {
            for entry in entries {
                let entry = entry.map_err(|err| err.to_string())?;
                let path = entry.path();
                if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                    continue;
                }
                let id = path
                    .file_stem()
                    .and_then(|stem| stem.to_str())
                    .unwrap_or("rollback")
                    .to_string();
                let content = fs::read_to_string(&path)
                    .map_err(|err| format!("read {}: {err}", path.display()))?;
                let parsed: Value = serde_json::from_str(&content)
                    .map_err(|err| format!("parse {}: {err}", path.display()))?;
                points.push(json!({
                "id": id,
                "label": format!("Rollback point {}", parsed.get("target").and_then(Value::as_str).unwrap_or("install")),
                "when": parsed.get("createdAt").and_then(Value::as_str).unwrap_or("local"),
                "files": 1,
                "tools": parsed.get("target").and_then(Value::as_str).map(|target| vec![target]).unwrap_or_else(|| vec!["d-token"]),
                "verified": parsed.get("ok").and_then(Value::as_bool).unwrap_or(false),
                "integrity_verified": false,
                "currently_restorable": false,
                "scope": "unknown",
                "executor_kind": "locked",
                "blocked_reason": "Legacy install rollback manifests do not prove transaction scope or the current target hash. Return to the source workflow."
            }));
            }
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
        Err(err) => return Err(format!("read {}: {err}", dir.display())),
    }

    let (agent_transaction_points, agent_transaction_rollback_ids) =
        agent_integration_transaction_points();
    points.extend(agent_transaction_points);

    let patcher = config_patcher();
    let backup_root = config_backup_root();
    match fs::read_dir(&backup_root) {
        Ok(entries) => {
            for entry in entries {
                let entry = entry.map_err(|err| err.to_string())?;
                let file_type = entry.file_type().map_err(|err| err.to_string())?;
                if !file_type.is_dir() {
                    continue;
                }
                let Some(id) = entry.file_name().to_str().map(str::to_string) else {
                    return Err("Config backup id is not valid UTF-8".to_string());
                };
                if id == "agent-integration-transactions" {
                    continue;
                }
                if agent_transaction_rollback_ids.contains(&id) {
                    continue;
                }
                match patcher.load_backup(&id) {
                    Ok(backup) => {
                        let scope = rollback_scope_for_backup(&backup);
                        let currently_restorable = backup_currently_restorable(&backup);
                        let current_state_matches = backup_matches_expected_current_state(&backup);
                        let expected_current = backup_expected_current_token(&backup);
                        points.push(json!({
                            "id": backup.rollback_id,
                            "label": format!("Rollback point {}", backup.display_target),
                            "when": backup.created_at,
                            "files": 1,
                            "tools": if scope == "opencode-config" { vec!["OpenCode"] } else if scope == "learning-managed-block" { vec!["Smart Learning"] } else if matches!(scope, "optical-config" | "agent-integration") { vec!["Visual Compression"] } else { vec!["d-token"] },
                            "verified": true,
                            "integrity_verified": true,
                            "currently_restorable": currently_restorable,
                            "blocked_reason": rollback_blocked_reason(scope, current_state_matches),
                            "source": "dtoken-config-patcher",
                            "target": backup.target_path.display().to_string(),
                            "scope": scope,
                            "executor_kind": rollback_executor_kind(scope),
                            "expected_current_sha256": expected_current,
                            "expectedCurrentSha256": backup.planned_after_sha256,
                        }));
                    }
                    Err(_) => points.push(json!({
                        "id": id,
                        "label": "Unavailable rollback point",
                        "when": "unknown",
                        "files": 0,
                        "tools": ["d-token"],
                        "verified": false,
                        "integrity_verified": false,
                        "currently_restorable": false,
                        "source": "dtoken-config-patcher-invalid",
                        "scope": "invalid-backup-evidence",
                        "executor_kind": "locked",
                        "reason": "Backup metadata or target identity could not be verified; restore is disabled.",
                        "blocked_reason": "Backup metadata or target identity could not be verified; restore is disabled."
                    })),
                }
            }
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
        Err(err) => return Err(format!("read {}: {err}", backup_root.display())),
    }
    points.sort_by(|a, b| {
        b.get("when")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .cmp(a.get("when").and_then(Value::as_str).unwrap_or_default())
    });
    Ok(points)
}

fn route_profile_drafts_result() -> Result<Vec<Value>, String> {
    let dir = app_state_dir().join("routing");
    let mut drafts = Vec::new();
    let applied_hashes = verified_applied_route_hashes();
    match fs::read_dir(&dir) {
        Ok(entries) => {
            for entry in entries {
                let entry = entry.map_err(|err| err.to_string())?;
                let path = entry.path();
                if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                    continue;
                }
                let content = fs::read_to_string(&path)
                    .map_err(|err| format!("read {}: {err}", path.display()))?;
                let mut value = serde_json::from_str::<Value>(&content)
                    .map_err(|err| format!("parse {}: {err}", path.display()))?;
                let already_resolved =
                    value.pointer("/resolution/state").and_then(Value::as_str) == Some("applied");
                if !already_resolved {
                    if let Some(after_sha256) = route_draft_config_hash(&value)
                        .filter(|hash| applied_hashes.iter().any(|applied| applied == hash))
                    {
                        if let Some(object) = value.as_object_mut() {
                            object.insert(
                                "resolution".to_string(),
                                json!({
                                    "state": "applied",
                                    "after_sha256": after_sha256,
                                    "resolvedAt": now_label(),
                                    "evidence": "verified-config-backup-after-hash"
                                }),
                            );
                        }
                        let file_name = path
                            .file_name()
                            .and_then(|name| name.to_str())
                            .ok_or_else(|| "Route draft file name is invalid".to_string())?;
                        write_json_app_state(&["routing", file_name], &value)?;
                    }
                }
                if let Some(object) = value.as_object_mut() {
                    object.insert("path".to_string(), json!(path.display().to_string()));
                }
                drafts.push(value);
            }
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
        Err(err) => return Err(format!("read {}: {err}", dir.display())),
    }
    drafts.sort_by(|a, b| {
        b.get("savedAt")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .cmp(a.get("savedAt").and_then(Value::as_str).unwrap_or_default())
    });
    Ok(drafts)
}

fn route_draft_config_hash(draft: &Value) -> Option<String> {
    let config = draft.get("config")?;
    serde_json::to_string_pretty(config)
        .ok()
        .map(|content| stable_hash(&content))
}

fn verified_applied_route_hashes() -> Vec<String> {
    let patcher = config_patcher();
    fs::read_dir(config_backup_root())
        .ok()
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().ok().is_some_and(|kind| kind.is_dir()))
        .filter_map(|entry| entry.file_name().to_str().map(str::to_string))
        .filter_map(|id| patcher.load_backup(&id).ok())
        .filter(|backup| {
            backup
                .target_path
                .file_name()
                .and_then(|name| name.to_str())
                == Some("dtoken-config.json")
        })
        .map(|backup| backup.planned_after_sha256)
        .collect()
}

fn mark_route_draft_resolved(
    route_id: &str,
    after_sha256: &str,
    transaction_id: &str,
    rollback_id: &str,
) -> Result<(), String> {
    let path = app_state_dir()
        .join("routing")
        .join(format!("{route_id}.json"));
    let content = fs::read_to_string(&path).map_err(|err| format!("read route draft: {err}"))?;
    let mut draft: Value =
        serde_json::from_str(&content).map_err(|err| format!("parse route draft: {err}"))?;
    if route_draft_config_hash(&draft).as_deref() != Some(after_sha256) {
        return Err("route draft hash no longer matches applied configuration".to_string());
    }
    let object = draft
        .as_object_mut()
        .ok_or_else(|| "route draft is not an object".to_string())?;
    object.insert(
        "resolution".to_string(),
        json!({
            "state": "applied",
            "after_sha256": after_sha256,
            "transaction_id": transaction_id,
            "rollback_id": rollback_id,
            "resolvedAt": now_label(),
            "evidence": "config-transaction-receipt"
        }),
    );
    write_json_app_state(&["routing", &format!("{route_id}.json")], &draft).map(|_| ())
}

fn route_profile_drafts_json() -> Vec<Value> {
    route_profile_drafts_result().unwrap_or_default()
}

fn install_targets(target: &str) -> Vec<&'static str> {
    match target {
        "rtk" => vec!["rtk"],
        "headroom" => vec!["headroom"],
        "dtoken" => vec!["dtoken"],
        "all" => vec!["rtk", "headroom", "dtoken"],
        _ => vec![],
    }
}

fn install_plan_json(req: &InstallPlanRequest) -> Value {
    let targets = install_targets(&req.target);
    let writes_enabled = matches!(req.mode.as_str(), "install" | "repair" | "connect");
    json!({
        "target": req.target,
        "mode": req.mode,
        "installRoot": req.install_root,
        "targets": targets,
        "writesEnabled": writes_enabled,
        "opencodeRoutePort": DTOKEN_SERVICE_PORT,
        "routeScope": "agent-routed-connection",
        "diagnosticScope": "agent-connection-verification",
        "excludedAgents": ["claude-code", "cursor", "codex-cli", "aider", "cline", "roo", "continue", "windsurf"],
        "transaction": [
            { "id": "scan", "label": "扫描", "detail": "检测 RTK、Headroom、d-token desktop、OpenCode 与本地端口" },
            { "id": "plan", "label": "生成计划", "detail": "根据所选目标生成安装/验证计划" },
            { "id": "diff", "label": "展示 diff", "detail": "写入前展示 d-token 将管理的配置差异" },
            { "id": "backup", "label": "备份", "detail": if req.auto_backup { "写入前创建 d-token 回滚清单" } else { "用户关闭自动备份，事务会记录风险" } },
            { "id": "apply", "label": "应用", "detail": if writes_enabled { "仅执行用户选择的托管动作" } else { "只读连接模式只验证，不写入外部配置" } },
            { "id": "verify", "label": "校验", "detail": "读取版本、路径、d-token 18001 健康端点和 OpenCode route 状态" },
            { "id": "doctor", "label": "Doctor", "detail": if req.run_doctor_after { "安装后自动运行 Doctor" } else { "用户选择跳过自动 Doctor" } },
            { "id": "rollback", "label": "回滚点", "detail": if req.rollback_point { "提交可审计回滚点" } else { "不创建回滚点" } }
        ],
        "diff": [
            { "type": "ctx", "text": "# d-token installation transaction" },
            { "type": "add", "text": format!("+ mode = \"{}\"", req.mode) },
            { "type": "add", "text": format!("+ install_root = \"{}\"", req.install_root) },
            { "type": "add", "text": format!("+ targets = {:?}", targets) },
            { "type": "add", "text": format!("+ opencode_route_base_url = \"http://127.0.0.1:{DTOKEN_SERVICE_PORT}/v1\"") },
            { "type": "add", "text": "+ route_scope = \"agent-routed-connection\"" },
            { "type": "add", "text": "+ excluded_agents = [\"claude-code\", \"cursor\", \"codex-cli\", \"aider\", \"cline\", \"roo\"]" },
            { "type": "add", "text": format!("+ auto_backup = {}", req.auto_backup) },
            { "type": "add", "text": format!("+ doctor_after_apply = {}", req.run_doctor_after) },
            { "type": "add", "text": format!("+ rollback_point = {}", req.rollback_point) }
        ],
        "warnings": if req.proxy_port == DTOKEN_SERVICE_PORT {
            vec!["OpenCode route verification should use the d-token local service port."]
        } else {
            vec!["The selected port is not the d-token service default 18001; verify it belongs to a d-token-managed local service."]
        }
    })
}

fn run_capture(program: &str, args: &[&str]) -> Result<(bool, String), String> {
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| format!("{program}: {err}"))?;
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().map_err(|err| err.to_string())? {
            break status;
        }
        if started.elapsed() >= Duration::from_millis(800) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!("{program}: version check timed out"));
        }
        thread::sleep(Duration::from_millis(20));
    };
    let mut text = String::new();
    if let Some(mut stdout) = child.stdout.take() {
        let _ = stdout.by_ref().take(64 * 1024).read_to_string(&mut text);
    }
    if let Some(mut stderr) = child.stderr.take() {
        let _ = stderr.by_ref().take(64 * 1024).read_to_string(&mut text);
    }
    Ok((status.success(), text.trim().to_string()))
}

#[cfg(windows)]
fn where_tool(name: &str) -> Option<String> {
    let extensions = env::var("PATHEXT")
        .unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".to_string())
        .split(';')
        .filter(|extension| !extension.is_empty())
        .map(str::to_ascii_lowercase)
        .collect::<Vec<_>>();
    for directory in env::var_os("PATH")
        .into_iter()
        .flat_map(|path| env::split_paths(&path).collect::<Vec<_>>())
    {
        let direct = directory.join(name);
        if direct.is_file() {
            return Some(direct.display().to_string());
        }
        if Path::new(name).extension().is_some() {
            continue;
        }
        for extension in &extensions {
            let candidate = directory.join(format!("{name}{extension}"));
            if candidate.is_file() {
                return Some(candidate.display().to_string());
            }
        }
    }
    None
}

#[cfg(not(windows))]
fn where_tool(name: &str) -> Option<String> {
    env::var_os("PATH")
        .into_iter()
        .flat_map(|path| env::split_paths(&path).collect::<Vec<_>>())
        .map(|directory| directory.join(name))
        .find(|path| path.is_file())
        .map(|path| path.display().to_string())
}

fn version_for(name: &str) -> Option<String> {
    run_capture(name, &["--version"])
        .ok()
        .filter(|(ok, _)| *ok)
        .map(|(_, text)| {
            if text.is_empty() {
                "installed".to_string()
            } else {
                text.lines().next().unwrap_or("installed").to_string()
            }
        })
}

fn check_tool(id: &str, label: &str) -> ToolCheck {
    let path = where_tool(id);
    let version = path.as_ref().and_then(|_| version_for(id));
    let installed = path.is_some();
    ToolCheck {
        id: id.to_string(),
        label: label.to_string(),
        installed,
        version,
        path,
        status: if installed { "ok" } else { "missing" }.to_string(),
        detail: if installed {
            format!("{label} is available on PATH")
        } else {
            format!("{label} was not found on PATH")
        },
    }
}

fn loopback_http_operation(path: &str) -> &'static str {
    match path {
        "/health" => "loopback_health",
        "/config/active" => "loopback_active_config",
        "/dashboard/payload" => "loopback_dashboard_detail",
        "/dashboard/summary" => "loopback_dashboard_summary",
        "/export/json" => "loopback_export_json",
        "/export/csv" => "loopback_export_csv",
        "/compatibility/agents" => "loopback_compatibility",
        "/protocol-injection" => "loopback_protocol_injection",
        "/chain/status" => "loopback_chain_status",
        "/compression/optical/status" => "loopback_optical_status",
        "/compression/optical/capabilities" => "loopback_optical_capabilities",
        "/compression/optical/capabilities/lifecycle" => "loopback_optical_lifecycle",
        "/compression/optical/events/recent" => "loopback_optical_events",
        _ => "loopback_service",
    }
}

fn retain_http_body(response: &mut String) {
    if let Some(header_end) = response.find("\r\n\r\n") {
        response.drain(..header_end + 4);
    }
}

fn simple_http_get(host: &str, port: u16, path: &str) -> Result<String, String> {
    let operation = loopback_http_operation(path);
    let total_started = Instant::now();
    if host != "127.0.0.1" {
        record_native_performance(
            operation,
            "http_total",
            total_started,
            "error",
            false,
            Some("host_rejected"),
        );
        return Err("desktop control-plane HTTP is restricted to 127.0.0.1".to_string());
    }
    let address = SocketAddr::from(([127, 0, 0, 1], port));
    let connect_started = Instant::now();
    let mut stream = match TcpStream::connect_timeout(&address, Duration::from_millis(120)) {
        Ok(stream) => {
            record_native_performance(operation, "connect", connect_started, "ok", false, None);
            stream
        }
        Err(error) => {
            record_native_performance(
                operation,
                "connect",
                connect_started,
                "error",
                false,
                Some("connect_failed"),
            );
            record_native_performance(
                operation,
                "http_total",
                total_started,
                "error",
                false,
                Some("connect_failed"),
            );
            return Err(error.to_string());
        }
    };
    let configure_started = Instant::now();
    if let Err(error) = stream.set_read_timeout(Some(Duration::from_millis(350))) {
        record_native_performance(
            operation,
            "configure",
            configure_started,
            "error",
            false,
            Some("read_timeout_config_failed"),
        );
        record_native_performance(
            operation,
            "http_total",
            total_started,
            "error",
            false,
            Some("read_timeout_config_failed"),
        );
        return Err(error.to_string());
    }
    if let Err(error) = stream.set_write_timeout(Some(Duration::from_millis(200))) {
        record_native_performance(
            operation,
            "configure",
            configure_started,
            "error",
            false,
            Some("write_timeout_config_failed"),
        );
        record_native_performance(
            operation,
            "http_total",
            total_started,
            "error",
            false,
            Some("write_timeout_config_failed"),
        );
        return Err(error.to_string());
    }
    record_native_performance(operation, "configure", configure_started, "ok", false, None);

    let write_started = Instant::now();
    if let Err(error) = write!(
        stream,
        "GET {path} HTTP/1.1\r\nHost: {host}:{port}\r\nConnection: close\r\n\r\n"
    ) {
        record_native_performance(
            operation,
            "write",
            write_started,
            "error",
            false,
            Some("write_failed"),
        );
        record_native_performance(
            operation,
            "http_total",
            total_started,
            "error",
            false,
            Some("write_failed"),
        );
        return Err(error.to_string());
    }
    record_native_performance(operation, "write", write_started, "ok", false, None);

    let mut response = String::new();
    let read_started = Instant::now();
    if let Err(error) = stream.read_to_string(&mut response) {
        record_native_performance(
            operation,
            "read",
            read_started,
            "error",
            false,
            Some("read_failed"),
        );
        record_native_performance(
            operation,
            "http_total",
            total_started,
            "error",
            false,
            Some("read_failed"),
        );
        return Err(error.to_string());
    }
    record_native_performance(operation, "read", read_started, "ok", false, None);

    let validate_started = Instant::now();
    if let Err(error) = ensure_http_success(&response) {
        record_native_performance(
            operation,
            "validate",
            validate_started,
            "error",
            false,
            Some("http_status_invalid"),
        );
        record_native_performance(
            operation,
            "http_total",
            total_started,
            "error",
            false,
            Some("http_status_invalid"),
        );
        return Err(error);
    }
    record_native_performance(operation, "validate", validate_started, "ok", false, None);
    retain_http_body(&mut response);
    record_native_performance(operation, "http_total", total_started, "ok", false, None);
    Ok(response)
}

const OPTICAL_PREVIEW_MAX_BODY_BYTES: usize = 512 * 1024;

fn simple_http_post_json(host: &str, port: u16, path: &str, body: &str) -> Result<String, String> {
    if host != "127.0.0.1" {
        return Err("desktop control-plane HTTP is restricted to 127.0.0.1".to_string());
    }
    if body.len() > OPTICAL_PREVIEW_MAX_BODY_BYTES {
        return Err(format!(
            "optical preview body exceeds {OPTICAL_PREVIEW_MAX_BODY_BYTES} bytes"
        ));
    }
    let address = SocketAddr::from(([127, 0, 0, 1], port));
    let mut stream = TcpStream::connect_timeout(&address, Duration::from_millis(200))
        .map_err(|err| err.to_string())?;
    stream
        .set_read_timeout(Some(Duration::from_millis(2_000)))
        .map_err(|err| err.to_string())?;
    stream
        .set_write_timeout(Some(Duration::from_millis(500)))
        .map_err(|err| err.to_string())?;
    write!(
        stream,
        "POST {path} HTTP/1.1\r\nHost: {host}:{port}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .map_err(|err| err.to_string())?;
    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .map_err(|err| err.to_string())?;
    ensure_http_success(&response)?;
    Ok(response)
}

fn http_response_status(raw: &str) -> Result<u16, String> {
    let status_line = raw.lines().next().unwrap_or("").trim();
    if status_line.is_empty() {
        return Err("missing HTTP status line".to_string());
    }
    let mut parts = status_line.split_whitespace();
    let _version = parts
        .next()
        .ok_or_else(|| "missing HTTP version".to_string())?;
    let code = parts
        .next()
        .ok_or_else(|| "missing HTTP status code".to_string())?;
    code.parse::<u16>()
        .map_err(|err| format!("invalid HTTP status code: {err}"))
}

fn ensure_http_success(raw: &str) -> Result<(), String> {
    let code = http_response_status(raw)?;
    if (200..300).contains(&code) {
        Ok(())
    } else {
        Err(format!("HTTP {code}"))
    }
}

fn optical_slice(port: u16, path: &str) -> Value {
    match service_json(port, path) {
        Ok(data) => json!({
            "ok": true,
            "source": "dtoken-local-service",
            "error": "",
            "data": data,
        }),
        Err(err) => json!({
            "ok": false,
            "source": "unavailable",
            "error": err,
            "data": Value::Null,
        }),
    }
}

fn optical_slice_join_error(label: &str) -> Value {
    json!({
        "ok": false,
        "source": "unavailable",
        "error": format!("optical {label} task failed"),
        "data": Value::Null,
    })
}

fn optical_observability_value() -> Value {
    let port = DTOKEN_SERVICE_PORT;
    // Four independent loopback GETs — run in parallel to cut wall-clock latency.
    let (status, capabilities, lifecycle, events) = thread::scope(|scope| {
        let status_handle = scope.spawn(|| optical_slice(port, "/compression/optical/status"));
        let capabilities_handle =
            scope.spawn(|| optical_slice(port, "/compression/optical/capabilities"));
        let lifecycle_handle =
            scope.spawn(|| optical_slice(port, "/compression/optical/capabilities/lifecycle"));
        let events_handle =
            scope.spawn(|| optical_slice(port, "/compression/optical/events/recent"));
        (
            status_handle
                .join()
                .unwrap_or_else(|_| optical_slice_join_error("status")),
            capabilities_handle
                .join()
                .unwrap_or_else(|_| optical_slice_join_error("capabilities")),
            lifecycle_handle
                .join()
                .unwrap_or_else(|_| optical_slice_join_error("lifecycle")),
            events_handle
                .join()
                .unwrap_or_else(|_| optical_slice_join_error("events")),
        )
    });
    let any_ok = status.get("ok").and_then(Value::as_bool).unwrap_or(false)
        || capabilities
            .get("ok")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        || lifecycle
            .get("ok")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        || events.get("ok").and_then(Value::as_bool).unwrap_or(false);
    let all_ok = status.get("ok").and_then(Value::as_bool).unwrap_or(false)
        && capabilities
            .get("ok")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && lifecycle
            .get("ok")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        && events.get("ok").and_then(Value::as_bool).unwrap_or(false);
    let collected_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0);
    json!({
        "ok": any_ok,
        "complete": all_ok,
        "partial": any_ok && !all_ok,
        "source": if any_ok { "dtoken-local-service" } else { "unavailable" },
        "collected_at": collected_at,
        "service_url": format!("http://127.0.0.1:{port}"),
        "status": status,
        "capabilities": capabilities,
        "lifecycle": lifecycle,
        "events": events,
    })
}

fn preview_optical_plan_value(request: Value) -> Value {
    let Some(object) = request.as_object() else {
        return json!({
            "ok": false,
            "source": "unavailable",
            "error": "optical preview request must be a JSON object",
            "data": Value::Null,
        });
    };
    if !object.contains_key("body") {
        return json!({
            "ok": false,
            "source": "unavailable",
            "error": "optical preview request requires a body field",
            "data": Value::Null,
        });
    }
    if !object.contains_key("endpoint_path") {
        return json!({
            "ok": false,
            "source": "unavailable",
            "error": "optical preview request requires an endpoint_path field",
            "data": Value::Null,
        });
    }
    let body = match serde_json::to_string(&request) {
        Ok(body) => body,
        Err(err) => {
            return json!({
                "ok": false,
                "source": "unavailable",
                "error": format!("invalid optical preview request: {err}"),
                "data": Value::Null,
            });
        }
    };
    match simple_http_post_json(
        "127.0.0.1",
        DTOKEN_SERVICE_PORT,
        "/compression/optical/preview",
        &body,
    ) {
        Ok(raw) => match serde_json::from_str::<Value>(http_body(&raw)) {
            Ok(data) => json!({
                "ok": true,
                "source": "dtoken-local-service",
                "error": "",
                "data": data,
            }),
            Err(err) => json!({
                "ok": false,
                "source": "unavailable",
                "error": format!("optical preview response is not JSON: {err}"),
                "data": Value::Null,
            }),
        },
        Err(err) => json!({
            "ok": false,
            "source": "unavailable",
            "error": err,
            "data": Value::Null,
        }),
    }
}

#[tauri::command]
async fn get_optical_observability() -> Value {
    tauri::async_runtime::spawn_blocking(optical_observability_value)
        .await
        .unwrap_or_else(|err| {
            json!({
                "ok": false,
                "complete": false,
                "partial": false,
                "source": "unavailable",
                "collected_at": 0,
                "service_url": format!("http://127.0.0.1:{DTOKEN_SERVICE_PORT}"),
                "error": format!("optical observability task failed: {err}"),
                "status": { "ok": false, "source": "unavailable", "error": "task_failed", "data": null },
                "capabilities": { "ok": false, "source": "unavailable", "error": "task_failed", "data": null },
                "lifecycle": { "ok": false, "source": "unavailable", "error": "task_failed", "data": null },
                "events": { "ok": false, "source": "unavailable", "error": "task_failed", "data": null },
            })
        })
}

#[tauri::command]
async fn preview_optical_plan(request: Value) -> Value {
    tauri::async_runtime::spawn_blocking(move || preview_optical_plan_value(request))
        .await
        .unwrap_or_else(|err| {
            json!({
                "ok": false,
                "source": "unavailable",
                "error": format!("optical preview task failed: {err}"),
                "data": Value::Null,
            })
        })
}

fn optical_safe_mode(raw: &str) -> Result<&'static str, String> {
    match raw.trim() {
        "off" => Ok("off"),
        "shadow_preview" => Ok("shadow_preview"),
        "tool_results_only" | "history_only" | "full_experimental" => Err(
            "product path only allows off or shadow_preview; active-leaning modes are blocked"
                .to_string(),
        ),
        other => Err(format!("unsupported optical mode: {other}")),
    }
}

fn read_active_config_value() -> Result<Value, String> {
    let path = active_config_path();
    if !path.exists() {
        return Err(
            "Active config is missing. Create a route in Install & Connect first.".to_string(),
        );
    }
    let content =
        fs::read_to_string(&path).map_err(|err| format!("Active config is unreadable: {err}"))?;
    if content.contains("sk-") {
        return Err("Active config contains a plaintext-looking secret".to_string());
    }
    serde_json::from_str(&content).map_err(|err| format!("Active config is invalid JSON: {err}"))
}

fn known_route_ids(config: &Value) -> Vec<String> {
    config
        .get("routes")
        .and_then(Value::as_array)
        .map(|routes| {
            routes
                .iter()
                .filter_map(|route| {
                    route
                        .get("id")
                        .and_then(Value::as_str)
                        .map(|id| id.to_string())
                })
                .collect()
        })
        .unwrap_or_default()
}

fn clamp_optical_draft(
    draft: &OpticalConfigDraftRequest,
    known_routes: &[String],
) -> Result<Value, String> {
    let known: std::collections::HashSet<&str> = known_routes.iter().map(String::as_str).collect();
    if known.is_empty() {
        return Err("Active config has no routes; configure a route first".to_string());
    }
    let global_mode = optical_safe_mode(&draft.global.default_mode)?;
    let mut route_policies = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for route in &draft.routes {
        let route_id = route.route_id.trim();
        if route_id.is_empty() {
            return Err("route policy is missing route_id".to_string());
        }
        if !known.contains(route_id) {
            return Err(format!("unknown route_id: {route_id}"));
        }
        if !seen.insert(route_id.to_string()) {
            return Err(format!("duplicate route policy: {route_id}"));
        }
        let mode = optical_safe_mode(&route.mode)?;
        route_policies.push(json!({
            "route_id": route_id,
            "policy": {
                "enabled": route.enabled,
                "mode": mode,
                "allow_tool_result": route.allow_tool_result,
                "allow_history": route.allow_history,
                "allow_schema_annotations": false,
                "allow_system_static_slab": false,
                "min_net_saved_tokens": 1024,
                "confirmed_experimental_lossy": route.confirmed_experimental_lossy,
            }
        }));
    }
    // Fill missing known routes with off defaults so apply is deterministic.
    for route_id in known_routes {
        if seen.contains(route_id) {
            continue;
        }
        route_policies.push(json!({
            "route_id": route_id,
            "policy": {
                "enabled": false,
                "mode": "off",
                "allow_tool_result": false,
                "allow_history": false,
                "allow_schema_annotations": false,
                "allow_system_static_slab": false,
                "min_net_saved_tokens": 1024,
                "confirmed_experimental_lossy": false,
            }
        }));
    }

    Ok(json!({
        "schema_version": 1,
        "global_policy": {
            "enabled": draft.global.enabled,
            "default_mode": global_mode,
            "allow_active": false,
            "public_summary_only": true,
        },
        "route_policies": route_policies,
    }))
}

fn merge_optical_into_active_config(
    mut config: Value,
    optical_patch: Value,
) -> Result<Value, String> {
    let existing_optical = config.get("optical").cloned().unwrap_or_else(|| json!({}));
    let mut merged_optical = existing_optical;
    let object = merged_optical
        .as_object_mut()
        .ok_or_else(|| "optical section is not an object".to_string())?;

    object.insert(
        "schema_version".to_string(),
        optical_patch
            .get("schema_version")
            .cloned()
            .unwrap_or(json!(1)),
    );

    let mut global = object
        .get("global_policy")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let global_object = global
        .as_object_mut()
        .ok_or_else(|| "optical.global_policy is not an object".to_string())?;
    if let Some(patch_global) = optical_patch
        .get("global_policy")
        .and_then(Value::as_object)
    {
        for key in [
            "enabled",
            "default_mode",
            "allow_active",
            "public_summary_only",
        ] {
            if let Some(value) = patch_global.get(key) {
                global_object.insert(key.to_string(), value.clone());
            }
        }
    }
    // Hard close active regardless of prior disk state for this product path.
    global_object.insert("allow_active".to_string(), json!(false));
    global_object.insert("public_summary_only".to_string(), json!(true));
    object.insert("global_policy".to_string(), global);
    object.insert(
        "route_policies".to_string(),
        optical_patch
            .get("route_policies")
            .cloned()
            .unwrap_or_else(|| json!([])),
    );
    // Preserve capability_snapshots / trusted_authorities / eval_artifacts if present.

    if let Some(root) = config.as_object_mut() {
        root.insert("optical".to_string(), merged_optical);
    } else {
        return Err("active config root is not an object".to_string());
    }
    let serialized = serde_json::to_string(&config)
        .map_err(|err| format!("failed to serialize merged config: {err}"))?;
    if serialized.contains("sk-") {
        return Err("merged config contains a plaintext-looking secret".to_string());
    }
    Ok(config)
}

fn prepare_optical_config_change(
    draft: &OpticalConfigDraftRequest,
) -> Result<(dtoken_config_patcher::PreparedChange, Value), String> {
    let current = read_active_config_value()?;
    let known = known_route_ids(&current);
    let optical_patch = clamp_optical_draft(draft, &known)?;
    let merged = merge_optical_into_active_config(current, optical_patch.clone())?;
    let serialized = serde_json::to_string_pretty(&merged)
        .map_err(|err| format!("Active config serialize failed: {err}"))?;
    let target_path = patchable_active_config_path()?;
    let prepared = config_patcher()
        .prepare_change(
            target_path,
            &serialized,
            ConfigToolKind::Dtoken,
            None,
            RiskLevel::Medium,
        )
        .map_err(|err| format!("Optical config plan failed: {err:#}"))?;
    Ok((prepared, optical_patch))
}

fn optical_runtime_projection(value: &Value) -> Value {
    let global = value
        .get("global_policy")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let routes = value
        .get("route_policies")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|entry| {
            let policy = entry.get("policy").cloned().unwrap_or_else(|| json!({}));
            json!({
                "route_id": entry.get("route_id").cloned().unwrap_or(Value::Null),
                "policy": {
                    "enabled": policy.get("enabled").cloned().unwrap_or(json!(false)),
                    "mode": policy.get("mode").cloned().unwrap_or(json!("off")),
                    "allow_tool_result": policy.get("allow_tool_result").cloned().unwrap_or(json!(false)),
                    "allow_history": policy.get("allow_history").cloned().unwrap_or(json!(false)),
                    "confirmed_experimental_lossy": policy.get("confirmed_experimental_lossy").cloned().unwrap_or(json!(false)),
                }
            })
        })
        .collect::<Vec<_>>();
    json!({
        "global_policy": {
            "enabled": global.get("enabled").cloned().unwrap_or(json!(false)),
            "default_mode": global.get("default_mode").cloned().unwrap_or(json!("shadow_preview")),
            "allow_active": global.get("allow_active").cloned().unwrap_or(json!(false)),
            "public_summary_only": global.get("public_summary_only").cloned().unwrap_or(json!(true)),
        },
        "route_policies": routes,
    })
}

fn runtime_optical_matches(expected: &Value) -> bool {
    service_json(DTOKEN_SERVICE_PORT, "/config/active")
        .ok()
        .and_then(|value| value.get("optical").cloned())
        .is_some_and(|actual| {
            optical_runtime_projection(&actual) == optical_runtime_projection(expected)
        })
}

fn optical_editor_value() -> Value {
    let service = dtoken_service_status_value();
    let service_state = service
        .get("state")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let service_running = service_state == "running";
    let path = active_config_path();
    if !path.exists() {
        return json!({
            "ok": false,
            "source": "dtoken-desktop-control-plane",
            "error": "active_config_missing",
            "service_running": service_running,
            "service_state": service_state,
            "active_config_exists": false,
            "active_config_path": path.display().to_string(),
            "route_count": 0,
            "routes": [],
            "draft": {
                "global": { "enabled": false, "default_mode": "shadow_preview" },
                "routes": []
            },
            "issues": [],
            "privacy": "No active config was read beyond existence check."
        });
    }

    let config = match read_active_config_value() {
        Ok(config) => config,
        Err(err) => {
            return json!({
                "ok": false,
                "source": "dtoken-desktop-control-plane",
                "error": err,
                "service_running": service_running,
                "service_state": service_state,
                "active_config_exists": true,
                "active_config_path": path.display().to_string(),
                "route_count": 0,
                "routes": [],
                "draft": {
                    "global": { "enabled": false, "default_mode": "shadow_preview" },
                    "routes": []
                },
                "issues": [],
            });
        }
    };

    let routes = config
        .get("routes")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let route_options: Vec<Value> = routes
        .iter()
        .filter_map(|route| {
            let id = route.get("id").and_then(Value::as_str)?;
            let name = route.get("name").and_then(Value::as_str).unwrap_or(id);
            Some(json!({ "route_id": id, "label": name }))
        })
        .collect();

    let optical = config.get("optical").cloned().unwrap_or_else(|| json!({}));
    let global = optical
        .get("global_policy")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let enabled = global
        .get("enabled")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let default_mode = global
        .get("default_mode")
        .and_then(Value::as_str)
        .unwrap_or("shadow_preview");
    let default_mode = optical_safe_mode(default_mode).unwrap_or("shadow_preview");

    let policy_by_route: HashMap<String, Value> = optical
        .get("route_policies")
        .and_then(Value::as_array)
        .map(|entries| {
            entries
                .iter()
                .filter_map(|entry| {
                    let route_id = entry.get("route_id").and_then(Value::as_str)?.to_string();
                    let policy = entry.get("policy").cloned().unwrap_or_else(|| json!({}));
                    Some((route_id, policy))
                })
                .collect()
        })
        .unwrap_or_default();

    let draft_routes: Vec<Value> = route_options
        .iter()
        .filter_map(|route| {
            let route_id = route.get("route_id").and_then(Value::as_str)?;
            let policy = policy_by_route.get(route_id).cloned().unwrap_or_else(|| json!({}));
            let mode = policy
                .get("mode")
                .and_then(Value::as_str)
                .unwrap_or("off");
            let mode = optical_safe_mode(mode).unwrap_or("shadow_preview");
            Some(json!({
                "route_id": route_id,
                "enabled": policy.get("enabled").and_then(Value::as_bool).unwrap_or(false),
                "mode": mode,
                "allow_tool_result": policy.get("allow_tool_result").and_then(Value::as_bool).unwrap_or(false),
                "allow_history": policy.get("allow_history").and_then(Value::as_bool).unwrap_or(false),
                "confirmed_experimental_lossy": policy.get("confirmed_experimental_lossy").and_then(Value::as_bool).unwrap_or(false),
            }))
        })
        .collect();

    let issues = match dtoken_config::DtokenConfig::load_json(&path) {
        Ok(parsed) => parsed
            .optical_config_issues()
            .into_iter()
            .map(|issue| format!("{issue:?}"))
            .collect::<Vec<_>>(),
        Err(_) => Vec::new(),
    };

    json!({
        "ok": true,
        "source": "dtoken-desktop-control-plane",
        "error": "",
        "service_running": service_running,
        "service_state": service_state,
        "active_config_exists": true,
        "active_config_path": path.display().to_string(),
        "route_count": route_options.len(),
        "routes": route_options,
        "draft": {
            "global": {
                "enabled": enabled,
                "default_mode": default_mode,
            },
            "routes": draft_routes,
        },
        "issues": issues,
        "allow_active_forced_off": true,
        "privacy": "Only optical policy fields and route ids/names are exposed; credentials and request bodies are not read into this editor payload."
    })
}

#[tauri::command]
async fn get_optical_config_editor() -> Value {
    tauri::async_runtime::spawn_blocking(optical_editor_value)
        .await
        .unwrap_or_else(|err| {
            json!({
                "ok": false,
                "source": "unavailable",
                "error": format!("optical editor task failed: {err}"),
                "service_running": false,
                "service_state": "unknown",
                "active_config_exists": false,
                "route_count": 0,
                "routes": [],
                "draft": {
                    "global": { "enabled": false, "default_mode": "shadow_preview" },
                    "routes": []
                },
                "issues": [],
            })
        })
}

#[tauri::command]
fn preview_optical_config_change(request: PreviewOpticalConfigRequest) -> Value {
    match prepare_optical_config_change(&request.draft) {
        Ok((prepared, optical_patch)) => json!({
            "ok": true,
            "source": "dtoken-desktop-control-plane",
            "mode": "optical-config-review",
            "target": prepared.plan.target_file,
            "before_sha256": prepared.plan.before_hash,
            "after_sha256": prepared.plan.after_hash,
            "diff": prepared.plan.diff,
            "risk_level": prepared.plan.risk_level,
            "requires_explicit_confirmation": true,
            "restart_required": true,
            "allow_active": false,
            "optical_summary": optical_patch,
            "privacy": "Read-only review. No config, backup, runtime, external agent, or credential state was changed."
        }),
        Err(err) => json!({
            "ok": false,
            "source": "dtoken-desktop-control-plane",
            "mode": "optical-config-review",
            "error": sanitize_command_output(&err),
            "requires_explicit_confirmation": true,
        }),
    }
}

#[tauri::command]
fn apply_optical_config_change(request: ApplyOpticalConfigRequest) -> Value {
    let (Some(reviewed_before), Some(reviewed_after)) = (
        request.reviewed_before_sha256.as_deref(),
        request.reviewed_after_sha256.as_deref(),
    ) else {
        return json!({
            "ok": false,
            "source": "dtoken-desktop-control-plane",
            "error": "Review the optical config diff before applying it",
            "summary": "Missing reviewed before/after hashes",
        });
    };

    let (prepared, optical_patch) = match prepare_optical_config_change(&request.draft) {
        Ok(prepared) => prepared,
        Err(err) => {
            return json!({
                "ok": false,
                "source": "dtoken-desktop-control-plane",
                "error": sanitize_command_output(&err),
                "summary": "Optical config could not be prepared for apply",
            });
        }
    };

    if prepared.plan.before_hash != reviewed_before || prepared.plan.after_hash != reviewed_after {
        return json!({
            "ok": false,
            "source": "dtoken-desktop-control-plane",
            "error": "Optical config review is stale; review the current diff again",
            "summary": "Stale review hashes",
        });
    }

    let transaction = match config_patcher().apply_transaction(&[prepared]) {
        Ok(transaction) => transaction,
        Err(err) => {
            return json!({
                "ok": false,
                "source": "dtoken-desktop-control-plane",
                "error": sanitize_command_output(&format!("{err:#}")),
                "summary": "Optical config transaction failed and was not committed",
            });
        }
    };
    let Some(receipt) = transaction.changes.first() else {
        return json!({
            "ok": false,
            "source": "dtoken-desktop-control-plane",
            "error": "Optical config transaction returned no receipt",
            "summary": "Missing verification evidence",
        });
    };

    let restart_requested = request.restart_service.unwrap_or(true);
    let restart = if restart_requested {
        restart_dtoken_service_impl()
    } else {
        ActionResult {
            ok: true,
            summary: "Service restart explicitly skipped".to_string(),
            logs: Vec::new(),
            tools: Vec::new(),
        }
    };
    let runtime_verified =
        restart_requested && restart.ok && runtime_optical_matches(&optical_patch);
    let _ = append_desktop_event(
        "optical_config_apply",
        if runtime_verified { "ok" } else { "partial" },
        json!({
            "transaction_id": &transaction.transaction_id,
            "rollback_id": &receipt.backup.rollback_id,
            "target": receipt.target_path.display().to_string(),
            "after_sha256": &receipt.after.sha256,
            "config_applied": true,
            "runtime_verified": runtime_verified,
        }),
    );

    json!({
        "ok": runtime_verified,
        "source": "dtoken-desktop-control-plane",
        "summary": if runtime_verified {
            "Optical shadow config applied, reloaded, and verified with a durable rollback point"
        } else {
            "Optical config was written, but the managed service could not be reloaded; retry restart or restore the rollback point"
        },
        "transaction_id": transaction.transaction_id,
        "rollback_id": receipt.backup.rollback_id,
        "before_sha256": receipt.before.sha256,
        "after_sha256": receipt.after.sha256,
        "config_applied": true,
        "runtime_verified": runtime_verified,
        "restarted": restart_requested && restart.ok,
        "restart_summary": restart.summary,
        "allow_active": false,
        "optical_summary": optical_patch,
        "privacy": "Only d-token active-config optical fields were changed. No external Agent configs or credentials were written."
    })
}

#[tauri::command]
fn rollback_optical_config_change(request: RollbackOpticalConfigRequest) -> Value {
    let rollback_id = request.rollback_id.trim();
    if rollback_id.is_empty() || rollback_id != request.rollback_id {
        return json!({
            "ok": false,
            "source": "dtoken-desktop-control-plane",
            "summary": "Invalid optical rollback point id",
            "config_restored": false,
            "runtime_verified": false,
        });
    }

    let backup = match config_patcher().load_backup(rollback_id) {
        Ok(backup) => backup,
        Err(err) => {
            return json!({
                "ok": false,
                "source": "dtoken-desktop-control-plane",
                "summary": "Optical rollback point failed integrity verification",
                "error": sanitize_command_output(&format!("{err:#}")),
                "config_restored": false,
                "runtime_verified": false,
            });
        }
    };
    let expected_target = match patchable_active_config_path() {
        Ok(path) => path,
        Err(err) => {
            return json!({
                "ok": false,
                "source": "dtoken-desktop-control-plane",
                "summary": "Active config target is unavailable",
                "error": sanitize_command_output(&err),
                "config_restored": false,
                "runtime_verified": false,
            });
        }
    };
    if backup.target_path != expected_target {
        return json!({
            "ok": false,
            "source": "dtoken-desktop-control-plane",
            "summary": "Rollback point does not belong to the d-token active config",
            "config_restored": false,
            "runtime_verified": false,
        });
    }

    let restored = manage_backup("restore".to_string(), rollback_id.to_string());
    if !restored.ok {
        return json!({
            "ok": false,
            "source": "dtoken-desktop-control-plane",
            "summary": restored.summary,
            "config_restored": false,
            "runtime_verified": false,
        });
    }

    let restart = restart_dtoken_service_impl();
    let restored_optical = read_active_config_value()
        .ok()
        .and_then(|value| value.get("optical").cloned());
    let runtime_verified = restart.ok
        && restored_optical
            .as_ref()
            .is_some_and(runtime_optical_matches);
    let _ = append_desktop_event(
        "optical_config_rollback",
        if runtime_verified { "ok" } else { "partial" },
        json!({
            "rollback_id": rollback_id,
            "config_restored": true,
            "runtime_verified": runtime_verified,
        }),
    );
    json!({
        "ok": runtime_verified,
        "source": "dtoken-desktop-control-plane",
        "summary": if runtime_verified {
            "Optical config restored and runtime after-state verified"
        } else {
            "Optical config restored, but runtime after-state could not be verified"
        },
        "rollback_id": rollback_id,
        "config_restored": true,
        "runtime_verified": runtime_verified,
        "restarted": restart.ok,
        "restart_summary": restart.summary,
        "allow_active": false,
    })
}

fn hybrid_agent_integration_request(
    draft: &HybridAgentIntegrationDraftRequest,
) -> Result<AgentIntegrationRequest, String> {
    let scope = match draft.scope.as_str() {
        "global" => AgentIntegrationScope::Global,
        "project" => AgentIntegrationScope::Project,
        _ => return Err("Agent integration scope must be global or project".to_string()),
    };
    let bridge_path = resolve_dtoken_agent_bridge_exe()?;
    Ok(AgentIntegrationRequest {
        agent_ids: draft.agent_ids.clone(),
        scope,
        project_root: draft.project_root.as_deref().map(PathBuf::from),
        bridge_path,
        install_instructions: draft.install_instructions,
        install_hook_or_plugin: draft.install_hook_or_plugin,
        install_skill: draft.install_skill,
        install_mcp: draft.install_mcp,
    })
}

fn prepare_hybrid_agent_integration(
    draft: &HybridAgentIntegrationDraftRequest,
) -> Result<dtoken_config_patcher::agent_integration::PreparedAgentIntegration, String> {
    let request = hybrid_agent_integration_request(draft)?;
    let home = user_home_dir()?;
    prepare_agent_integration(&request, &home, &config_backup_root())
        .map_err(|error| format!("Agent integration plan failed: {error:#}"))
}

#[tauri::command]
fn get_hybrid_agent_integrations() -> Value {
    let bridge = resolve_dtoken_agent_bridge_exe();
    let (bridge_available, bridge_path, bridge_error) = match bridge {
        Ok(path) => (true, Some(path.display().to_string()), None),
        Err(error) => (false, None, Some(sanitize_command_output(&error))),
    };
    let service = dtoken_service_status_value();
    let active_config = active_route_config_status();
    let default_project_root = env::current_dir().ok().filter(|path| {
        path.is_dir()
            && (path.join(".git").exists()
                || path.join("AGENTS.md").exists()
                || path.join("Cargo.toml").exists()
                || path.join("package.json").exists())
    });
    let default_scope = if default_project_root.is_some() {
        "project"
    } else {
        "global"
    };
    let (agents, discovery_error) = match agent_registry::scan_current_process_agents() {
        Ok(snapshot) => (
            snapshot
                .agents
                .into_iter()
                .map(|agent| {
                    let integration_supported =
                        matches!(agent.id.as_str(), "claude-code" | "codex" | "opencode");
                    json!({
                        "id": agent.id,
                        "name": agent.name,
                        "installed": true,
                        "detected": true,
                        "integration_supported": integration_supported,
                        "install_capability": agent.install_capability,
                        "session_capability": agent.session_capability,
                        "evidence_count": agent.evidence.len(),
                        "surfaces": if integration_supported {
                            json!(["transport", "instructions", "hook_or_plugin", "skill", "mcp"])
                        } else {
                            json!(["transport", "discovery"])
                        },
                        "transport_state": "separate_route_prerequisite",
                    })
                })
                .collect::<Vec<_>>(),
            None,
        ),
        Err(error) => (
            Vec::new(),
            Some(sanitize_command_output(&format!("{error:#}"))),
        ),
    };
    json!({
        "ok": bridge_available,
        "source": "d-token-agent-integration",
        "agents": agents,
        "discovery_error": discovery_error,
        "bridge": {
            "available": bridge_available,
            "path": bridge_path,
            "error": bridge_error,
        },
        "service": service,
        "transport": {
            "configured": active_config.get("state").and_then(Value::as_str) == Some("loaded"),
            "route_count": active_config.get("route_count").and_then(Value::as_u64).unwrap_or(0),
            "detail": "Provider/base URL routing is configured separately and this integration never changes credentials."
        },
        "defaults": {
            "install_instructions": true,
            "install_hook_or_plugin": true,
            "install_skill": true,
            "install_mcp": false,
            "scope": default_scope,
            "project_root": default_project_root.map(|path| path.display().to_string())
        }
    })
}

#[tauri::command]
fn preview_hybrid_agent_integration(request: HybridAgentIntegrationDraftRequest) -> Value {
    match prepare_hybrid_agent_integration(&request) {
        Ok(prepared) => json!({
            "ok": true,
            "state": "review",
            "source": "d-token-agent-integration",
            "plan": prepared.plan,
            "requires_explicit_confirmation": true,
            "privacy": "Preview only; no Agent config, backup, credential, prompt, or transcript was written."
        }),
        Err(error) => json!({
            "ok": false,
            "state": "blocked",
            "source": "d-token-agent-integration",
            "error": sanitize_command_output(&error),
        }),
    }
}

#[tauri::command]
fn apply_hybrid_agent_integration(request: ApplyHybridAgentIntegrationRequest) -> Value {
    let prepared = match prepare_hybrid_agent_integration(&request.draft) {
        Ok(prepared) => prepared,
        Err(error) => {
            return json!({"ok": false, "state": "blocked", "error": sanitize_command_output(&error)})
        }
    };
    match apply_agent_integration(prepared, &config_backup_root(), &request.reviewed_digest) {
        Ok(receipt) => {
            let _ = append_desktop_event(
                "agent_integration_apply",
                "ok",
                json!({
                    "transaction_id": &receipt.transaction_id,
                    "rollback_ids": &receipt.rollback_ids,
                    "rollback_point_count": receipt.rollback_ids.len(),
                    "new_session_required": receipt.new_session_required,
                }),
            );
            json!({
                "ok": true,
                "state": "applied",
                "source": "d-token-agent-integration",
                "receipt": receipt,
                "summary": "Agent integration was written atomically. Verify this transaction before judging activation."
            })
        }
        Err(error) => json!({
            "ok": false,
            "state": "failed",
            "source": "d-token-agent-integration",
            "error": sanitize_command_output(&format!("{error:#}")),
        }),
    }
}

#[tauri::command]
fn verify_hybrid_agent_integration(request: VerifyHybridAgentIntegrationRequest) -> Value {
    match verify_agent_integration_transaction(&request.transaction_id, &config_backup_root()) {
        Ok(verification) => json!({
            "ok": verification.ok,
            "state": if verification.ok {
                "verified"
            } else if verification.verified_targets.is_empty() {
                "changed"
            } else {
                "partial"
            },
            "source": "d-token-agent-integration",
            "verification": verification,
            "service": dtoken_service_status_value(),
            "transport": active_route_config_status(),
        }),
        Err(error) => {
            json!({"ok": false, "state": "failed", "error": sanitize_command_output(&format!("{error:#}"))})
        }
    }
}

#[tauri::command]
fn rollback_hybrid_agent_integration(request: RollbackHybridAgentIntegrationRequest) -> Value {
    if request.transaction_id.trim().is_empty() {
        return json!({"ok": false, "state": "blocked", "error": "Rollback requires a transaction id"});
    }
    match rollback_agent_integration(&request.transaction_id, &config_backup_root()) {
        Ok(restored) => {
            let _ = append_desktop_event(
                "agent_integration_rollback",
                "ok",
                json!({
                    "transaction_id": &request.transaction_id,
                    "restored_target_count": restored.len(),
                }),
            );
            json!({
                "ok": true,
                "state": "restored",
                "source": "d-token-agent-integration",
                "restored_targets": restored,
                "summary": "Agent integration transaction was restored. Provider credentials were not changed."
            })
        }
        Err(error) => {
            json!({"ok": false, "state": "failed", "error": sanitize_command_output(&format!("{error:#}"))})
        }
    }
}

#[tauri::command]
fn get_hybrid_agent_integration_transaction(
    request: GetHybridAgentIntegrationTransactionRequest,
) -> Value {
    match get_agent_integration_transaction(&request.transaction_id, &config_backup_root()) {
        Ok(transaction) => json!({
            "ok": true,
            "state": transaction.state,
            "source": "d-token-agent-integration",
            "transaction": transaction,
        }),
        Err(error) => json!({
            "ok": false,
            "state": "unavailable",
            "source": "d-token-agent-integration",
            "error": sanitize_command_output(&format!("{error:#}")),
        }),
    }
}

fn http_body(raw: &str) -> &str {
    raw.split_once("\r\n\r\n")
        .map(|(_, body)| body)
        .unwrap_or(raw)
}

fn service_json(port: u16, path: &str) -> Result<Value, String> {
    let operation = loopback_http_operation(path);
    let total_started = Instant::now();
    let body = match simple_http_get("127.0.0.1", port, path) {
        Ok(body) => body,
        Err(error) => {
            record_native_performance(
                operation,
                "total",
                total_started,
                "error",
                false,
                Some("transport_failed"),
            );
            return Err(error);
        }
    };
    let parse_started = Instant::now();
    match serde_json::from_str(&body) {
        Ok(value) => {
            record_native_performance(operation, "parse", parse_started, "ok", false, None);
            record_native_performance(operation, "total", total_started, "ok", false, None);
            Ok(value)
        }
        Err(error) => {
            record_native_performance(
                operation,
                "parse",
                parse_started,
                "error",
                false,
                Some("json_parse_failed"),
            );
            record_native_performance(
                operation,
                "total",
                total_started,
                "error",
                false,
                Some("json_parse_failed"),
            );
            Err(error.to_string())
        }
    }
}

fn service_text(port: u16, path: &str) -> Result<String, String> {
    simple_http_get("127.0.0.1", port, path)
}

fn service_manifest_path() -> PathBuf {
    app_state_dir().join("service").join("managed-service.json")
}

fn service_log_path() -> PathBuf {
    app_state_dir().join("service").join("dtoken-proxy.log")
}

fn service_data_dir() -> PathBuf {
    app_state_dir().join("runtime")
}

fn read_service_manifest() -> Option<ManagedServiceManifest> {
    let path = service_manifest_path();
    let content = fs::read_to_string(path).ok()?;
    serde_json::from_str(&content).ok()
}

fn write_service_manifest(manifest: &ManagedServiceManifest) -> Result<PathBuf, String> {
    let content = serde_json::to_string_pretty(manifest).map_err(|err| err.to_string())?;
    write_app_state(&["service", "managed-service.json"], &content)
}

fn remove_service_manifest() -> Result<(), String> {
    durable_remove_file(&service_manifest_path())
}

fn file_sha256(path: &Path) -> Result<String, String> {
    use sha2::{Digest, Sha256};

    let mut file = fs::File::open(path).map_err(|error| error.to_string())?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(|error| error.to_string())?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn managed_service_binary_evidence(
    manifest: &ManagedServiceManifest,
) -> ManagedServiceBinaryEvidence {
    let launch_sha256 = manifest
        .exe_sha256
        .as_deref()
        .filter(|value| is_sha256(value))
        .map(str::to_ascii_lowercase);
    if manifest.schema_version == 0 || manifest.exe_sha256.is_none() {
        return ManagedServiceBinaryEvidence {
            state: "legacy_manifest",
            launch_sha256,
            current_sha256: None,
            compatible: false,
        };
    }
    if manifest.schema_version != MANAGED_SERVICE_MANIFEST_SCHEMA_VERSION {
        return ManagedServiceBinaryEvidence {
            state: "unsupported_manifest_schema",
            launch_sha256,
            current_sha256: None,
            compatible: false,
        };
    }
    let Some(launch_sha256) = launch_sha256 else {
        return ManagedServiceBinaryEvidence {
            state: "invalid_launch_identity",
            launch_sha256: None,
            current_sha256: None,
            compatible: false,
        };
    };
    match file_sha256(Path::new(&manifest.exe_path)) {
        Ok(current_sha256) => ManagedServiceBinaryEvidence {
            state: if current_sha256 == launch_sha256 {
                "matched"
            } else {
                "bundle_replaced"
            },
            compatible: current_sha256 == launch_sha256,
            launch_sha256: Some(launch_sha256),
            current_sha256: Some(current_sha256),
        },
        Err(_) => ManagedServiceBinaryEvidence {
            state: "current_binary_unavailable",
            launch_sha256: Some(launch_sha256),
            current_sha256: None,
            compatible: false,
        },
    }
}

fn process_birth_identity_matches(manifest: &ManagedServiceManifest) -> bool {
    manifest
        .process_birth_token
        .as_deref()
        .zip(process_birth_token(manifest.pid).as_deref())
        .is_some_and(|(expected, actual)| expected == actual)
}

fn candidate_proxy_paths() -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Ok(explicit) = env::var("DTOKEN_PROXY_EXE") {
        candidates.push(PathBuf::from(explicit));
    }
    if let Ok(current) = env::current_exe() {
        if let Some(dir) = current.parent() {
            candidates.push(dir.join("dtoken-proxy-x86_64-pc-windows-gnu.exe"));
            candidates.push(dir.join("dtoken-proxy-x86_64-pc-windows-gnu"));
            candidates.push(dir.join("dtoken-proxy-x86_64-pc-windows-msvc.exe"));
            candidates.push(dir.join("dtoken-proxy-x86_64-pc-windows-msvc"));
            candidates.push(dir.join("dtoken-proxy.exe"));
            candidates.push(dir.join("dtoken-proxy"));
            candidates.push(
                dir.join("binaries")
                    .join("dtoken-proxy-x86_64-pc-windows-gnu.exe"),
            );
            candidates.push(
                dir.join("binaries")
                    .join("dtoken-proxy-x86_64-pc-windows-gnu"),
            );
            candidates.push(
                dir.join("binaries")
                    .join("dtoken-proxy-x86_64-pc-windows-msvc.exe"),
            );
            candidates.push(
                dir.join("binaries")
                    .join("dtoken-proxy-x86_64-pc-windows-msvc"),
            );
            candidates.push(dir.join("..").join("dtoken-proxy.exe"));
            candidates.push(dir.join("..").join("dtoken-proxy"));
        }
    }
    candidates.push(
        PathBuf::from("code")
            .join("apps")
            .join("desktop")
            .join("src-tauri")
            .join("binaries")
            .join("dtoken-proxy-x86_64-pc-windows-gnu.exe"),
    );
    candidates.push(
        PathBuf::from("code")
            .join("apps")
            .join("desktop")
            .join("src-tauri")
            .join("binaries")
            .join("dtoken-proxy-x86_64-pc-windows-gnu"),
    );
    candidates.push(
        PathBuf::from("code")
            .join("apps")
            .join("desktop")
            .join("src-tauri")
            .join("binaries")
            .join("dtoken-proxy-x86_64-pc-windows-msvc.exe"),
    );
    candidates.push(
        PathBuf::from("code")
            .join("apps")
            .join("desktop")
            .join("src-tauri")
            .join("binaries")
            .join("dtoken-proxy-x86_64-pc-windows-msvc"),
    );
    candidates.push(
        PathBuf::from("target")
            .join("release")
            .join("dtoken-proxy.exe"),
    );
    candidates.push(
        PathBuf::from("target")
            .join("debug")
            .join("dtoken-proxy.exe"),
    );
    candidates.push(
        PathBuf::from("..")
            .join("..")
            .join("target")
            .join("release")
            .join("dtoken-proxy.exe"),
    );
    candidates.push(
        PathBuf::from("..")
            .join("..")
            .join("target")
            .join("debug")
            .join("dtoken-proxy.exe"),
    );
    candidates.push(
        PathBuf::from("code")
            .join("target")
            .join("release")
            .join("dtoken-proxy.exe"),
    );
    candidates.push(
        PathBuf::from("code")
            .join("target")
            .join("debug")
            .join("dtoken-proxy.exe"),
    );
    candidates
}

fn resolve_dtoken_proxy_exe() -> Result<PathBuf, String> {
    for candidate in candidate_proxy_paths() {
        let path = if candidate.is_absolute() {
            candidate
        } else {
            env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("."))
                .join(candidate)
        };
        if path.exists() {
            return fs::canonicalize(&path).map_err(|err| err.to_string());
        }
    }
    Err("dtoken-proxy executable was not found; build or bundle d-token runtime first".to_string())
}

fn candidate_agent_bridge_paths() -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    #[cfg(debug_assertions)]
    if let Ok(explicit) = env::var("DTOKEN_AGENT_BRIDGE_EXE") {
        candidates.push(PathBuf::from(explicit));
    }
    if let Ok(current) = env::current_exe() {
        if let Some(dir) = current.parent() {
            for name in [
                "dtoken-agent-bridge-aarch64-apple-darwin",
                "dtoken-agent-bridge-x86_64-apple-darwin",
                "dtoken-agent-bridge-x86_64-pc-windows-msvc.exe",
                "dtoken-agent-bridge.exe",
                "dtoken-agent-bridge",
            ] {
                candidates.push(dir.join(name));
                candidates.push(dir.join("binaries").join(name));
            }
        }
    }
    #[cfg(debug_assertions)]
    {
        for prefix in [
            PathBuf::from("code/apps/desktop/src-tauri/binaries"),
            PathBuf::from("code/target/debug"),
            PathBuf::from("code/target/release"),
            PathBuf::from("target/debug"),
            PathBuf::from("target/release"),
        ] {
            candidates.push(prefix.join("dtoken-agent-bridge"));
            candidates.push(prefix.join("dtoken-agent-bridge.exe"));
            candidates.push(prefix.join("dtoken-agent-bridge-aarch64-apple-darwin"));
            candidates.push(prefix.join("dtoken-agent-bridge-x86_64-apple-darwin"));
            candidates.push(prefix.join("dtoken-agent-bridge-x86_64-pc-windows-msvc.exe"));
        }
    }
    candidates
}

fn resolve_dtoken_agent_bridge_exe() -> Result<PathBuf, String> {
    for candidate in candidate_agent_bridge_paths() {
        let path = if candidate.is_absolute() {
            candidate
        } else {
            env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("."))
                .join(candidate)
        };
        if path.is_file() {
            let canonical = fs::canonicalize(path).map_err(|error| error.to_string())?;
            verify_dtoken_agent_bridge(&canonical)?;
            return Ok(canonical);
        }
    }
    Err(
        "dtoken-agent-bridge executable was not found; build or bundle the Agent bridge first"
            .to_string(),
    )
}

fn verify_dtoken_agent_bridge(path: &Path) -> Result<(), String> {
    use sha2::{Digest, Sha256};

    let metadata = fs::metadata(path).map_err(|error| error.to_string())?;
    if !metadata.is_file() || metadata.len() == 0 {
        return Err("dtoken-agent-bridge is not a non-empty regular file".to_string());
    }
    if agent_bridge_requires_embedded_hash() {
        let expected_sha256 = env!("DTOKEN_AGENT_BRIDGE_SHA256");
        if expected_sha256 == "unavailable" {
            return Err(
                "dtoken-agent-bridge release hash was not embedded at build time".to_string(),
            );
        }
        let actual_sha256 = format!(
            "{:x}",
            Sha256::digest(fs::read(path).map_err(|error| error.to_string())?)
        );
        if actual_sha256 != expected_sha256 {
            return Err("dtoken-agent-bridge release hash mismatch".to_string());
        }
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o111 == 0 {
            return Err("dtoken-agent-bridge is not executable".to_string());
        }
    }
    let identity = verify_platform_before_agent_bridge_execution(
        path,
        verify_agent_bridge_platform_signature,
        load_dtoken_agent_bridge_identity,
    )?;
    if identity.get("component").and_then(Value::as_str) != Some(BRIDGE_COMPONENT)
        || identity.get("protocol_version").and_then(Value::as_u64)
            != Some(u64::from(BRIDGE_PROTOCOL_VERSION))
        || identity.get("arch").and_then(Value::as_str) != Some(env::consts::ARCH)
        || identity.get("os").and_then(Value::as_str) != Some(env::consts::OS)
        || identity.get("version").and_then(Value::as_str) != Some(BRIDGE_VERSION)
    {
        return Err("dtoken-agent-bridge bundle identity mismatch".to_string());
    }
    Ok(())
}

fn verify_platform_before_agent_bridge_execution<T>(
    path: &Path,
    verify_platform: impl FnOnce(&Path) -> Result<(), String>,
    execute_identity: impl FnOnce(&Path) -> Result<T, String>,
) -> Result<T, String> {
    verify_platform(path)?;
    execute_identity(path)
}

fn load_dtoken_agent_bridge_identity(path: &Path) -> Result<Value, String> {
    let output = Command::new(path)
        .arg("identity")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .map_err(|error| format!("dtoken-agent-bridge identity failed: {error}"))?;
    if !output.status.success() || output.stdout.len() > 16 * 1024 {
        return Err("dtoken-agent-bridge identity response is invalid".to_string());
    }
    let identity: Value = serde_json::from_slice(&output.stdout)
        .map_err(|_| "dtoken-agent-bridge identity is not JSON".to_string())?;
    Ok(identity)
}

fn agent_bridge_requires_embedded_hash() -> bool {
    // Tauri signs Mach-O sidecars after build.rs has embedded its digest. Since
    // codesign changes the Mach-O bytes, macOS release authority is the strict
    // Developer ID/Team ID check below plus the bridge protocol identity.
    !cfg!(target_os = "macos")
}

#[cfg(any(debug_assertions, not(any(target_os = "macos", target_os = "windows"))))]
fn verify_agent_bridge_platform_signature(_path: &Path) -> Result<(), String> {
    Ok(())
}

#[cfg(all(target_os = "macos", not(debug_assertions)))]
fn verify_agent_bridge_platform_signature(path: &Path) -> Result<(), String> {
    let team_id = env!("DTOKEN_EXPECTED_APPLE_TEAM_ID");
    if team_id.is_empty() {
        return Err("expected Apple Team ID was not embedded in the release".to_string());
    }
    let verify = Command::new("/usr/bin/codesign")
        .args(["--verify", "--strict"])
        .arg(path)
        .status()
        .map_err(|error| format!("bridge signature verification failed: {error}"))?;
    if !verify.success() {
        return Err("dtoken-agent-bridge signature verification failed".to_string());
    }
    let details = Command::new("/usr/bin/codesign")
        .args(["-dv", "--verbose=4"])
        .arg(path)
        .output()
        .map_err(|error| format!("bridge signature identity failed: {error}"))?;
    let evidence = String::from_utf8_lossy(&details.stderr);
    if !details.status.success()
        || !evidence
            .lines()
            .any(|line| line.trim() == format!("TeamIdentifier={team_id}"))
    {
        return Err("dtoken-agent-bridge signer identity mismatch".to_string());
    }
    Ok(())
}

#[cfg(all(target_os = "windows", not(debug_assertions)))]
fn verify_agent_bridge_platform_signature(path: &Path) -> Result<(), String> {
    let expected = env!("DTOKEN_WINDOWS_SIGNING_THUMBPRINT")
        .replace(' ', "")
        .to_ascii_uppercase();
    if expected.is_empty() {
        return Err(
            "expected Windows signing thumbprint was not embedded in the release".to_string(),
        );
    }
    let script = "$s=Get-AuthenticodeSignature -LiteralPath $args[0]; if($s.Status -ne 'Valid'){exit 1}; [Console]::Out.Write($s.SignerCertificate.Thumbprint)";
    let output = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", script, "--"])
        .arg(path)
        .output()
        .map_err(|error| format!("bridge Authenticode verification failed: {error}"))?;
    let observed = String::from_utf8_lossy(&output.stdout)
        .trim()
        .replace(' ', "")
        .to_ascii_uppercase();
    if !output.status.success() || observed != expected {
        return Err("dtoken-agent-bridge Authenticode identity mismatch".to_string());
    }
    Ok(())
}

fn service_health_payload() -> Option<Value> {
    service_json(DTOKEN_SERVICE_PORT, "/health").ok()
}

fn service_health_payload_cached() -> Option<Value> {
    type HealthPayloadCache = Mutex<Option<(Instant, Option<Value>)>>;
    static CACHE: OnceLock<HealthPayloadCache> = OnceLock::new();
    let started_at = Instant::now();
    let cache = CACHE.get_or_init(|| Mutex::new(None));
    let mut guard = cache
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some((collected_at, value)) = guard.as_ref() {
        if collected_at.elapsed() <= Duration::from_millis(200) {
            record_native_performance(
                "loopback_health",
                "cache",
                started_at,
                if value.is_some() { "ok" } else { "error" },
                true,
                value.is_none().then_some("cached_unavailable"),
            );
            return value.clone();
        }
    }
    let value = service_health_payload();
    *guard = Some((Instant::now(), value.clone()));
    record_native_performance(
        "loopback_health",
        "cache",
        started_at,
        if value.is_some() { "ok" } else { "error" },
        false,
        value.is_none().then_some("unavailable"),
    );
    value
}

fn legacy_managed_process_requires_restart(
    runtime_identity: Option<&str>,
    managed_process_owned: bool,
    binary_identity_state: Option<&str>,
) -> bool {
    runtime_identity.is_none()
        && managed_process_owned
        && binary_identity_state == Some("legacy_manifest")
}

fn dtoken_service_status_value() -> Value {
    let manifest = read_service_manifest();
    let health = service_health_payload_cached();
    let runtime_identity = health
        .as_ref()
        .and_then(|payload| payload.get("runtime_identity").and_then(Value::as_str));
    let runtime_version = health
        .as_ref()
        .and_then(|payload| payload.get("version").and_then(Value::as_str));
    let runtime_instance_nonce = health.as_ref().and_then(|payload| {
        payload
            .get("runtime_instance_nonce")
            .and_then(Value::as_str)
    });
    let healthy = health
        .as_ref()
        .and_then(|payload| payload.get("status").and_then(Value::as_str))
        .map(|status| status == "healthy" || status == "ready")
        .unwrap_or(false);
    let component_ok = health
        .as_ref()
        .and_then(|payload| payload.get("component").and_then(Value::as_str))
        .map(|component| component == "dtoken-core-api")
        .unwrap_or(false);
    let runtime_identity_ok = health
        .as_ref()
        .and_then(|payload| payload.get("runtime_identity").and_then(Value::as_str))
        .is_some_and(|identity| identity == "dtoken-runtime-local");
    let runtime_version_ok = health
        .as_ref()
        .and_then(|payload| payload.get("version").and_then(Value::as_str))
        .is_some_and(|version| !version.trim().is_empty());
    let pid_running = manifest
        .as_ref()
        .map(|item| is_process_running(item.pid))
        .unwrap_or(false);
    let manifest_identity_ok = manifest
        .as_ref()
        .map(|item| {
            item.managed_by == "d-token-desktop"
                && item.identity == "dtoken-core-api"
                && item.port == DTOKEN_SERVICE_PORT
        })
        .unwrap_or(false);
    let executable_owned = manifest
        .as_ref()
        .filter(|_| pid_running)
        .map(|item| executable_identity_matches(item.pid, Path::new(&item.exe_path)))
        .unwrap_or(false);
    let process_birth_owned = manifest
        .as_ref()
        .filter(|_| pid_running)
        .is_some_and(process_birth_identity_matches);
    let runtime_instance_owned = manifest.as_ref().is_some_and(|item| {
        item.runtime_instance_nonce
            .as_deref()
            .zip(runtime_instance_nonce)
            .is_some_and(|(expected, actual)| expected == actual)
    });
    let port_owned = manifest
        .as_ref()
        .filter(|_| pid_running)
        .map(|item| listener_owned_by_pid(item.pid, item.port))
        .unwrap_or(false);
    let manifest_stale = manifest.is_some() && !pid_running && !healthy;
    let managed_process_owned = healthy
        && component_ok
        && runtime_version_ok
        && pid_running
        && manifest_identity_ok
        && executable_owned
        && port_owned
        && process_birth_owned
        && runtime_instance_owned;
    let binary_evidence = manifest
        .as_ref()
        .filter(|_| managed_process_owned)
        .map(managed_service_binary_evidence);
    let binary_compatible = binary_evidence
        .as_ref()
        .map(|evidence| evidence.compatible)
        .unwrap_or(false);
    let managed_ownership_verified = managed_process_owned && runtime_identity_ok;
    let legacy_restart_required = legacy_managed_process_requires_restart(
        runtime_identity,
        managed_process_owned,
        binary_evidence.as_ref().map(|evidence| evidence.state),
    );
    let managed_running = managed_ownership_verified && binary_compatible;
    let restart_required =
        legacy_restart_required || (managed_ownership_verified && !binary_compatible);
    let external_or_unmanaged = healthy
        && component_ok
        && runtime_identity_ok
        && runtime_version_ok
        && !managed_ownership_verified;

    json!({
        "ok": managed_running || external_or_unmanaged,
        "mode": "dtoken-service-lifecycle",
        "state": if managed_running {
            "running"
        } else if restart_required {
            "restart_required"
        } else if external_or_unmanaged {
            "unmanaged_running"
        } else {
            "stopped"
        },
        "managed": managed_running || restart_required,
        "compatible": managed_running,
        "restart_required": restart_required,
        "pid": manifest.as_ref().map(|item| item.pid),
        "pid_running": pid_running,
        "manifest_identity_ok": manifest_identity_ok,
        "manifest_schema_version": manifest.as_ref().map(|item| item.schema_version),
        "runtime_identity_ok": runtime_identity_ok,
        "runtime_version_ok": runtime_version_ok,
        "runtime_identity": runtime_identity,
        "runtime_version": runtime_version,
        "runtime_instance_owned": runtime_instance_owned,
        "executable_owned": executable_owned,
        "process_birth_owned": process_birth_owned,
        "port_owned": port_owned,
        "binary_identity_state": binary_evidence.as_ref().map(|evidence| evidence.state).unwrap_or("unverified"),
        "launch_exe_sha256": binary_evidence.as_ref().and_then(|evidence| evidence.launch_sha256.clone()),
        "current_exe_sha256": binary_evidence.as_ref().and_then(|evidence| evidence.current_sha256.clone()),
        "manifest_stale": manifest_stale,
        "service_url": format!("http://127.0.0.1:{DTOKEN_SERVICE_PORT}"),
        "manifest_path": service_manifest_path().display().to_string(),
        "log_path": service_log_path().display().to_string(),
        "health": health,
        "summary": if managed_running {
            "d-token local service is running and managed by this app"
        } else if restart_required {
            "The managed d-token service was started from an older or unverifiable bundle and requires an explicit restart"
        } else if external_or_unmanaged {
            "A d-token-compatible service is responding, but it was not started by this app"
        } else {
            "d-token local service is not running"
        },
        "privacy": "Only d-token-owned service manifest and 127.0.0.1 health endpoint are inspected. External Headroom, RTK, cc-switch, shell profiles, and agent configs are not touched."
    })
}

fn validate_active_config_for_start() -> Result<Value, String> {
    let config = read_app_state(&["active-config", "dtoken-config.json"])
        .map_err(|err| format!("No applied d-token route configuration is available: {err}"))?;
    let providers = config
        .get("providers")
        .and_then(Value::as_array)
        .ok_or_else(|| "Active config providers must be an array".to_string())?;
    let routes = config
        .get("routes")
        .and_then(Value::as_array)
        .ok_or_else(|| "Active config routes must be an array".to_string())?;
    let instances = config
        .get("instances")
        .and_then(Value::as_array)
        .ok_or_else(|| "Active config instances must be an array".to_string())?;
    if providers.is_empty() || routes.is_empty() || instances.is_empty() {
        return Err(
            "Active config is empty; apply a real provider, route, and instance before starting"
                .to_string(),
        );
    }
    if !routes.iter().any(|route| {
        route.get("local_port").and_then(Value::as_u64) == Some(u64::from(DTOKEN_SERVICE_PORT))
            && route.get("bind_host").and_then(Value::as_str) == Some("127.0.0.1")
    }) {
        return Err(format!(
            "Active config has no d-token-owned 127.0.0.1:{DTOKEN_SERVICE_PORT} route"
        ));
    }
    for provider in providers {
        let Some(base_url) = provider.get("base_url").and_then(Value::as_str) else {
            return Err("Active config provider is missing base_url".to_string());
        };
        if provider_targets_local_listener(base_url, DTOKEN_SERVICE_PORT) {
            return Err("Active config contains a provider self-loop into d-token".to_string());
        }
    }
    Ok(config)
}

fn open_log_file(path: &Path) -> Result<fs::File, String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|err| err.to_string())
}

fn check_headroom_endpoint(port: u16, label: &str) -> HeadroomEndpoint {
    match simple_http_get("127.0.0.1", port, "/health") {
        Ok(raw) => {
            let healthy = raw.contains("\"healthy\"")
                || raw.contains("\"ready\":true")
                || raw.contains("healthy")
                || raw.contains("ready");
            HeadroomEndpoint {
                port,
                label: label.to_string(),
                healthy,
                detail: if healthy {
                    format!("127.0.0.1:{port}/health responded")
                } else {
                    format!("127.0.0.1:{port}/health responded but was not clearly healthy")
                },
            }
        }
        Err(err) => HeadroomEndpoint {
            port,
            label: label.to_string(),
            healthy: false,
            detail: format!("127.0.0.1:{port}/health failed: {err}"),
        },
    }
}

fn headroom_label(port: u16) -> &'static str {
    match port {
        EXTERNAL_HEADROOM_REFERENCE_PORT => "External Headroom reference",
        LEGACY_OPENCODE_REFERENCE_PORT => "Legacy OpenCode reference",
        _ => "External Headroom reference",
    }
}

fn headroom_endpoints() -> Vec<HeadroomEndpoint> {
    vec![
        check_headroom_endpoint(
            EXTERNAL_HEADROOM_REFERENCE_PORT,
            "External Headroom reference",
        ),
        check_headroom_endpoint(LEGACY_OPENCODE_REFERENCE_PORT, "Legacy OpenCode reference"),
    ]
}

fn all_checks() -> Vec<ToolCheck> {
    vec![
        check_tool("rtk", "RTK"),
        check_tool("headroom", "Headroom"),
        check_tool("opencode", "OpenCode"),
    ]
}

fn install_or_verify(id: &str) -> ActionResult {
    let label = match id {
        "rtk" => "RTK",
        "headroom" => "Headroom",
        _ => "Unknown tool",
    };
    let mut logs = vec![
        log("info", format!("Starting {label} install check")),
        log("info", format!("Looking for {id} on PATH")),
    ];

    let check = check_tool(id, label);
    if check.installed {
        logs.push(log(
            "info",
            format!(
                "{label} already installed: {}",
                check
                    .version
                    .clone()
                    .unwrap_or_else(|| "version unknown".to_string())
            ),
        ));
        logs.push(log(
            "info",
            format!("{label} verification finished successfully"),
        ));
        return ActionResult {
            ok: true,
            summary: format!("{label} already installed and verified"),
            logs,
            tools: all_checks(),
        };
    }

    logs.push(log(
        "error",
        format!("{label} is missing; automatic installer is not configured for this machine yet"),
    ));
    logs.push(log(
        "warn",
        "No files were changed. Add a signed installer source before enabling write actions.",
    ));
    ActionResult {
        ok: false,
        summary: format!("{label} is missing"),
        logs,
        tools: all_checks(),
    }
}

#[tauri::command]
fn save_app_settings(section: String, payload: Value) -> ActionResult {
    let allowed = [
        "rtk",
        "headroom",
        "app",
        "project-policy",
        "routing",
        "preferences",
        "smart-learning",
    ];
    let mut logs = vec![log(
        "info",
        format!("Saving d-token settings section: {section}"),
    )];
    if !allowed.contains(&section.as_str()) {
        logs.push(log(
            "error",
            format!("Unsupported settings section: {section}"),
        ));
        return ActionResult {
            ok: false,
            summary: "Settings section is not supported".to_string(),
            logs,
            tools: all_checks(),
        };
    }

    let existing = read_app_state(&["settings", &format!("{section}.json")])
        .ok()
        .and_then(|value| value.get("payload").cloned())
        .unwrap_or_else(|| json!({}));
    let mut merged_payload = existing;
    merge_json_objects(&mut merged_payload, payload);
    let content = json!({
        "section": section,
        "savedAt": now_label(),
        "payload": merged_payload,
        "privacy": "d-token local state only; no API keys or raw CCR content should be stored here"
    });
    match write_json_app_state(
        &[
            "settings",
            &format!("{}.json", content["section"].as_str().unwrap_or("app")),
        ],
        &content,
    ) {
        Ok(path) => {
            logs.push(log("info", format!("Settings saved to {}", path.display())));
            ActionResult {
                ok: true,
                summary: "Settings saved".to_string(),
                logs,
                tools: all_checks(),
            }
        }
        Err(err) => {
            logs.push(log("error", format!("Settings save failed: {err}")));
            ActionResult {
                ok: false,
                summary: "Settings save failed".to_string(),
                logs,
                tools: all_checks(),
            }
        }
    }
}

#[tauri::command]
fn get_app_settings(section: String) -> Value {
    let allowed = [
        "rtk",
        "headroom",
        "app",
        "project-policy",
        "routing",
        "preferences",
        "smart-learning",
    ];
    if !allowed.contains(&section.as_str()) {
        return json!({
            "ok": false,
            "section": section,
            "source": "d-token-local-settings",
            "state": "unsupported_section",
            "payload": {}
        });
    }
    match read_app_state(&["settings", &format!("{section}.json")]) {
        Ok(mut value) => {
            if let Some(object) = value.as_object_mut() {
                object.insert("ok".to_string(), json!(true));
                object.insert("source".to_string(), json!("d-token-local-settings"));
                object.insert("state".to_string(), json!("loaded"));
            }
            value
        }
        Err(err) => json!({
            "ok": false,
            "section": section,
            "source": "d-token-local-settings",
            "state": "missing",
            "error": err,
            "payload": {}
        }),
    }
}

#[tauri::command]
fn check_for_updates(channel: String) -> Value {
    let channel = match channel.as_str() {
        "stable" | "preview" => channel,
        _ => {
            return json!({
                "ok": false,
                "state": "invalid_channel",
                "source": "d-token-update-control-plane",
                "channel": channel,
                "summary": "Update channel must be stable or preview"
            });
        }
    };
    json!({
        "ok": false,
        "state": "unavailable",
        "source": "d-token-update-control-plane",
        "freshness": now_label(),
        "channel": channel,
        "current_version": env!("CARGO_PKG_VERSION"),
        "latest_version": null,
        "signature_verified": false,
        "summary": "No signed release feed is configured; no update was downloaded or installed",
        "privacy": "Update checks never include prompts, responses, source code, credentials, or local paths"
    })
}

fn safe_slug(value: &str, fallback: &str) -> String {
    let slug = value
        .chars()
        .filter_map(|ch| {
            if ch.is_ascii_alphanumeric() {
                Some(ch.to_ascii_lowercase())
            } else if ch == '-' || ch == '_' {
                Some('-')
            } else {
                None
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string();
    if slug.is_empty() {
        fallback.to_string()
    } else {
        slug
    }
}

fn parse_protocol(value: &str) -> &'static str {
    match value.trim().to_ascii_lowercase().as_str() {
        "anthropic" => "anthropic",
        _ => "open_ai",
    }
}

fn parse_provider_kind(value: &str) -> &'static str {
    match value.trim().to_ascii_lowercase().as_str() {
        "official" => "official",
        "local_chain" | "cc_switch" | "cc-switch" => "local_chain",
        _ => "custom",
    }
}

fn parse_chain_mode(value: &str) -> &'static str {
    match value.trim().to_ascii_lowercase().as_str() {
        "chain_bridge" | "compress_then_cc_switch" | "cc_switch" | "cc-switch" => "chain_bridge",
        _ => "direct",
    }
}

fn parse_compression_policy(value: &str) -> &'static str {
    match value.trim().to_ascii_lowercase().as_str() {
        "off" => "off",
        "safe" => "safe",
        "aggressive" => "aggressive",
        "max" => "max",
        _ => "balanced",
    }
}

fn provider_targets_local_listener(base_url: &str, local_port: u16) -> bool {
    url::Url::parse(base_url).ok().is_some_and(|parsed| {
        let host_is_local = match parsed.host() {
            Some(url::Host::Domain(domain)) => domain.eq_ignore_ascii_case("localhost"),
            Some(url::Host::Ipv4(address)) => address.is_loopback(),
            Some(url::Host::Ipv6(address)) => address.is_loopback(),
            None => false,
        };
        host_is_local && parsed.port_or_known_default() == Some(local_port)
    })
}

fn normalize_credential_reference(value: Option<String>) -> Result<Option<String>, ()> {
    let Some(value) = value.map(|value| value.trim().to_string()) else {
        return Ok(None);
    };
    if value.is_empty() {
        return Ok(None);
    }
    let parsed = url::Url::parse(&value).map_err(|_| ())?;
    let allowed_scheme = matches!(parsed.scheme(), "os-credential" | "wincred" | "keychain");
    let has_target = parsed
        .host_str()
        .is_some_and(|host| !host.trim().is_empty())
        || !parsed.path().trim_matches('/').is_empty();
    if !allowed_scheme
        || !has_target
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
    {
        return Err(());
    }
    Ok(Some(value))
}

fn credential_reference_for_provider(provider_id: &str) -> (String, String) {
    let mut provider_slug = String::new();
    let mut last_was_separator = true;
    for character in provider_id.trim().chars() {
        if character.is_ascii_alphanumeric() {
            provider_slug.push(character.to_ascii_lowercase());
            last_was_separator = false;
        } else if matches!(character, '-' | '_' | ' ') && !last_was_separator {
            provider_slug.push('-');
            last_was_separator = true;
        }
    }
    let provider_id = provider_slug.trim_matches('-');
    let provider_id = if provider_id.is_empty() {
        "provider"
    } else {
        provider_id
    };
    let account = format!("providers/{provider_id}");
    (format!("os-credential://d-token/{account}"), account)
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
fn platform_store_credential(account: &str, credential: &str) -> Result<(), ()> {
    let entry = keyring::Entry::new("d-token", account).map_err(|_| ())?;
    entry.set_password(credential).map_err(|_| ())
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn platform_store_credential(_account: &str, _credential: &str) -> Result<(), ()> {
    Err(())
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
fn platform_delete_credential(account: &str) -> Result<(), ()> {
    let entry = keyring::Entry::new("d-token", account).map_err(|_| ())?;
    entry.delete_credential().map_err(|_| ())
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn platform_delete_credential(_account: &str) -> Result<(), ()> {
    Err(())
}

#[tauri::command]
fn store_provider_credential(request: StoreProviderCredentialRequest) -> ActionResult {
    let credential = request.credential.trim();
    if credential.is_empty() || credential.len() > 65_536 {
        return ActionResult {
            ok: false,
            summary: "Provider credential is empty or too large".to_string(),
            logs: vec![log(
                "error",
                "Credential was not written to the OS credential store",
            )],
            tools: all_checks(),
        };
    }
    let (_, account) = credential_reference_for_provider(&request.provider_id);
    match platform_store_credential(&account, credential) {
        Ok(()) => ActionResult {
            ok: true,
            summary: "Provider credential saved in the operating-system credential store"
                .to_string(),
            logs: vec![log(
                "info",
                "Credential stored by reference; plaintext was not written to d-token config or logs",
            )],
            tools: all_checks(),
        },
        Err(()) => ActionResult {
            ok: false,
            summary: "Operating-system credential store is unavailable".to_string(),
            logs: vec![log(
                "error",
                "Credential was not written; no plaintext fallback is allowed",
            )],
            tools: all_checks(),
        },
    }
}

#[tauri::command]
fn delete_provider_credential(request: DeleteProviderCredentialRequest) -> ActionResult {
    let (_, account) = credential_reference_for_provider(&request.provider_id);
    match platform_delete_credential(&account) {
        Ok(()) => ActionResult {
            ok: true,
            summary: "Provider credential removed from the operating-system credential store"
                .to_string(),
            logs: vec![log("info", "Credential-store entry removed")],
            tools: all_checks(),
        },
        Err(()) => ActionResult {
            ok: false,
            summary: "Provider credential could not be removed".to_string(),
            logs: vec![log(
                "error",
                "Credential-store removal failed; d-token config was not modified",
            )],
            tools: all_checks(),
        },
    }
}

#[tauri::command]
fn save_route_profile_draft(request: RouteProfileDraftRequest) -> ActionResult {
    let mut logs = vec![log("info", "Saving d-token route/provider draft")];
    let provider_id = safe_slug(&request.provider_id, "provider");
    let route_id = safe_slug(&request.route_id, "route");
    let instance_id = format!("{route_id}-instance");
    let base_url = request.base_url.trim().trim_end_matches('/').to_string();
    if !(base_url.starts_with("http://") || base_url.starts_with("https://")) {
        logs.push(log(
            "error",
            "Provider base URL must start with http:// or https://",
        ));
        return ActionResult {
            ok: false,
            summary: "Invalid provider base URL".to_string(),
            logs,
            tools: all_checks(),
        };
    }
    if request.local_port == 0 {
        logs.push(log("error", "Local port must be a positive TCP port"));
        return ActionResult {
            ok: false,
            summary: "Invalid local route port".to_string(),
            logs,
            tools: all_checks(),
        };
    }
    if provider_targets_local_listener(&base_url, request.local_port) {
        logs.push(log(
            "error",
            "Provider upstream cannot point back to the same d-token listener",
        ));
        return ActionResult {
            ok: false,
            summary: "Provider route would loop back into d-token".to_string(),
            logs,
            tools: all_checks(),
        };
    }

    let models = request
        .models
        .into_iter()
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
        .collect::<Vec<_>>();
    let api_key_ref = match normalize_credential_reference(request.api_key_ref) {
        Ok(reference) => reference,
        Err(()) => {
            logs.push(log(
                "error",
                "Provider credential reference must be an opaque OS credential-store reference",
            ));
            return ActionResult {
                ok: false,
                summary: "Invalid provider credential reference".to_string(),
                logs,
                tools: all_checks(),
            };
        }
    };
    let protocol = parse_protocol(&request.protocol);
    let content = json!({
        "source": "d-token-desktop",
        "savedAt": now_label(),
        "privacy": "d-token-owned routing draft only; api_key_ref is a reference and plaintext secrets are rejected.",
        "config": {
            "providers": [{
                "id": provider_id,
                "name": request.provider_name,
                "kind": parse_provider_kind(&request.provider_kind),
                "protocol": protocol,
                "base_url": base_url,
                "api_key_ref": api_key_ref,
                "models": models,
                "timeout_ms": 120000,
                "healthcheck_path": "/health"
            }],
            "routes": [{
                "id": route_id,
                "name": request.route_name,
                "input_protocol": protocol,
                "instance_id": instance_id,
                "target_provider_id": provider_id,
                "fallback_provider_id": Value::Null,
                "chain_mode": parse_chain_mode(&request.chain_mode),
                "compression_policy": parse_compression_policy(&request.compression_policy),
                "bind_host": "127.0.0.1",
                "local_port": request.local_port
            }],
            "instances": [{
                "id": instance_id,
                "name": format!("{} instance", request.agent),
                "agent": request.agent,
                "route_id": route_id,
                "local_port": request.local_port,
                "process_status": "stopped",
                "log_path": format!("./logs/{route_id}.jsonl")
            }]
        },
        "diff": [
            { "type": "ctx", "text": format!("[provider.{provider_id}]") },
            { "type": "add", "text": format!("+ base_url = \"{}\"", base_url) },
            { "type": "ctx", "text": format!("[route.{route_id}]") },
            { "type": "add", "text": "+ bind_host = \"127.0.0.1\"" },
            { "type": "add", "text": format!("+ local_port = {}", request.local_port) },
            { "type": "add", "text": "+ fallback = \"disabled\"" }
        ]
    });
    let Some(active_parent) = active_config_path().parent().map(Path::to_path_buf) else {
        logs.push(log("error", "Active config parent path is unavailable"));
        return ActionResult {
            ok: false,
            summary: "Routing draft could not provision local control-plane state".to_string(),
            logs,
            tools: all_checks(),
        };
    };
    if let Err(err) = fs::create_dir_all(&active_parent) {
        logs.push(log(
            "error",
            format!("Active config directory create failed: {err}"),
        ));
        return ActionResult {
            ok: false,
            summary: "Routing draft could not provision local control-plane state".to_string(),
            logs,
            tools: all_checks(),
        };
    }
    match write_json_app_state(&["routing", &format!("{route_id}.json")], &content) {
        Ok(path) => {
            logs.push(log(
                "info",
                format!("Routing draft saved to {}", path.display()),
            ));
            logs.push(log("info", "No OpenCode, Headroom, RTK, cc-switch, shell profile, or system proxy config was changed"));
            ActionResult {
                ok: true,
                summary: "Route/provider draft saved".to_string(),
                logs,
                tools: all_checks(),
            }
        }
        Err(err) => {
            logs.push(log("error", format!("Routing draft save failed: {err}")));
            ActionResult {
                ok: false,
                summary: "Routing draft save failed".to_string(),
                logs,
                tools: all_checks(),
            }
        }
    }
}

#[tauri::command]
fn list_route_profile_drafts() -> Value {
    let drafts = route_profile_drafts_json();
    json!({
        "source": "d-token-desktop",
        "drafts": drafts,
        "count": drafts.len(),
        "privacy": "d-token local routing drafts only; external agent configs are not read."
    })
}

fn prepare_route_profile_draft_apply(
    requested_route_id: &str,
) -> Result<(String, dtoken_config_patcher::PreparedChange), String> {
    let requested_route_id = requested_route_id.trim();
    let route_id = safe_slug(requested_route_id, "");
    if route_id.is_empty() || route_id != requested_route_id {
        return Err("Route draft id is invalid or not normalized".to_string());
    }
    let draft_path = app_state_dir()
        .join("routing")
        .join(format!("{route_id}.json"));
    let draft_content = match fs::read_to_string(&draft_path) {
        Ok(content) => content,
        Err(err) => return Err(format!("Route draft not found: {err}")),
    };
    let draft: Value = match serde_json::from_str(&draft_content) {
        Ok(value) => value,
        Err(err) => return Err(format!("Route draft parse failed: {err}")),
    };
    let Some(config) = draft.get("config").cloned() else {
        return Err("Route draft has no config section".to_string());
    };
    if config.to_string().contains("sk-") {
        return Err("Route draft contains a plaintext-looking secret".to_string());
    }
    let serialized_config = serde_json::to_string_pretty(&config)
        .map_err(|err| format!("Active config serialize failed: {err}"))?;
    let target_path = patchable_active_config_path()?;
    let prepared = config_patcher()
        .prepare_change(
            target_path,
            &serialized_config,
            ConfigToolKind::Dtoken,
            None,
            RiskLevel::Medium,
        )
        .map_err(|err| format!("Active config plan failed: {err:#}"))?;
    Ok((route_id, prepared))
}

#[tauri::command]
fn preview_route_profile_draft_apply(request: RouteDraftReviewRequest) -> Value {
    match prepare_route_profile_draft_apply(&request.route_id) {
        Ok((route_id, prepared)) => json!({
            "ok": true,
            "source": "dtoken-desktop-control-plane",
            "mode": "route-apply-review",
            "route_id": route_id,
            "target": prepared.plan.target_file,
            "before_sha256": prepared.plan.before_hash,
            "after_sha256": prepared.plan.after_hash,
            "diff": prepared.plan.diff,
            "risk_level": prepared.plan.risk_level,
            "requires_explicit_confirmation": true,
            "runtime_required": false,
            "privacy": "Read-only review. No config, backup, runtime, external agent, or credential state was changed."
        }),
        Err(err) => json!({
            "ok": false,
            "source": "dtoken-desktop-control-plane",
            "mode": "route-apply-review",
            "error": sanitize_command_output(&err),
            "requires_explicit_confirmation": true,
            "runtime_required": false,
        }),
    }
}

#[tauri::command]
fn apply_route_profile_draft(request: ApplyRouteDraftRequest) -> ActionResult {
    let mut logs = vec![log(
        "info",
        format!(
            "Applying reviewed d-token route draft: {}",
            request.route_id
        ),
    )];
    let (Some(reviewed_before), Some(reviewed_after)) = (
        request.reviewed_before_sha256.as_deref(),
        request.reviewed_after_sha256.as_deref(),
    ) else {
        logs.push(log(
            "error",
            "Apply rejected because no reviewed before/after hashes were supplied",
        ));
        return ActionResult {
            ok: false,
            summary: "Review the route diff before applying it".to_string(),
            logs,
            tools: all_checks(),
        };
    };
    let (route_id, prepared) = match prepare_route_profile_draft_apply(&request.route_id) {
        Ok(prepared) => prepared,
        Err(err) => {
            logs.push(log("error", sanitize_command_output(&err)));
            return ActionResult {
                ok: false,
                summary: "Route draft could not be prepared for apply".to_string(),
                logs,
                tools: all_checks(),
            };
        }
    };
    if prepared.plan.before_hash != reviewed_before || prepared.plan.after_hash != reviewed_after {
        logs.push(log(
            "error",
            "Apply rejected because the active config or route draft changed after review",
        ));
        return ActionResult {
            ok: false,
            summary: "Route review is stale; review the current diff again".to_string(),
            logs,
            tools: all_checks(),
        };
    }

    let transaction = match config_patcher().apply_transaction(&[prepared]) {
        Ok(transaction) => transaction,
        Err(err) => {
            logs.push(log(
                "error",
                format!("Active config transaction failed: {err:#}"),
            ));
            return ActionResult {
                ok: false,
                summary: "Active config transaction failed and was not committed".to_string(),
                logs,
                tools: all_checks(),
            };
        }
    };
    let Some(receipt) = transaction.changes.first() else {
        logs.push(log(
            "error",
            "Active config transaction returned no receipt",
        ));
        return ActionResult {
            ok: false,
            summary: "Active config transaction did not return verification evidence".to_string(),
            logs,
            tools: all_checks(),
        };
    };
    logs.push(log(
        "info",
        format!(
            "Verified active config transaction {} for route {route_id}; rollback point {}",
            transaction.transaction_id, receipt.backup.rollback_id
        ),
    ));
    logs.push(log(
        "info",
        "No OpenCode, Headroom, RTK, cc-switch, shell profile, or system proxy config was changed",
    ));
    if let Err(error) = mark_route_draft_resolved(
        &route_id,
        &receipt.after.sha256.clone().unwrap_or_default(),
        &transaction.transaction_id,
        &receipt.backup.rollback_id,
    ) {
        logs.push(log(
            "warn",
            format!("Route draft lifecycle receipt could not be persisted: {error}"),
        ));
    }
    ActionResult {
        ok: true,
        summary: "Reviewed route draft applied and verified with a durable rollback point"
            .to_string(),
        logs,
        tools: all_checks(),
    }
}

fn prepare_connection_route_plan(
    request: &ConnectionRoutePlanRequest,
) -> Result<(String, dtoken_config_patcher::PreparedChange, Vec<String>), String> {
    let discovery = enrich_upstream_discovery(upstream_discovery::discover());
    prepare_connection_route_plan_from_discovery(request, &discovery)
}

fn prepare_connection_route_plan_from_discovery(
    request: &ConnectionRoutePlanRequest,
    discovery: &Value,
) -> Result<(String, dtoken_config_patcher::PreparedChange, Vec<String>), String> {
    if request.bindings.is_empty() || request.bindings.len() > 16 {
        return Err(
            "Connection plan must contain between one and sixteen Agent bindings".to_string(),
        );
    }
    let current_revision = discovery
        .get("revision")
        .and_then(Value::as_str)
        .ok_or_else(|| "Current discovery revision is unavailable".to_string())?;
    if current_revision != request.discovery_revision {
        return Err(
            "Agent or upstream configuration changed after discovery; scan again before review"
                .to_string(),
        );
    }
    let discovered_bindings = discovery
        .get("agent_bindings")
        .and_then(Value::as_array)
        .ok_or_else(|| "Current Agent discovery result is unavailable".to_string())?;

    let mut providers = Vec::new();
    let mut routes = Vec::new();
    let mut instances = Vec::new();
    let mut selected_agents = Vec::new();
    let mut route_keys: HashMap<(String, String), (String, String)> = HashMap::new();

    let mut requested = request.bindings.clone();
    requested.sort_by(|left, right| left.agent_id.cmp(&right.agent_id));
    for binding_request in requested {
        let agent_id = safe_slug(&binding_request.agent_id, "");
        if agent_id.is_empty() || agent_id != binding_request.agent_id {
            return Err("Connection plan contains an invalid Agent id".to_string());
        }
        if selected_agents.iter().any(|selected| selected == &agent_id) {
            return Err(format!("Connection plan repeats Agent {agent_id}"));
        }
        let binding = discovered_bindings
            .iter()
            .find(|binding| binding.get("agent_id").and_then(Value::as_str) == Some(&agent_id))
            .ok_or_else(|| format!("Agent {agent_id} is no longer discoverable"))?;
        if binding.get("installed").and_then(Value::as_bool) != Some(true) {
            return Err(format!("Agent {agent_id} is no longer installed"));
        }
        if matches!(
            binding.get("capability").and_then(Value::as_str),
            Some("prompt-only" | "manual-only")
        ) {
            return Err(format!(
                "Agent {agent_id} does not support an automatic connection plan"
            ));
        }

        let candidate = binding
            .get("candidates")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .find(|candidate| {
                candidate.get("id").and_then(Value::as_str)
                    == Some(binding_request.candidate_id.as_str())
                    && candidate.get("source").and_then(Value::as_str)
                        == Some(binding_request.candidate_source.as_str())
                    && candidate
                        .get("models")
                        .and_then(Value::as_array)
                        .is_some_and(|models| {
                            models
                                .iter()
                                .any(|model| model.as_str() == Some(binding_request.model.as_str()))
                        })
            })
            .ok_or_else(|| {
                format!(
                    "The reviewed upstream/model for {agent_id} is no longer present in discovery"
                )
            })?;
        if candidate.get("auth_mode").and_then(Value::as_str) != Some("none") {
            return Err(format!(
                "The upstream credential for {agent_id} cannot be reused safely"
            ));
        }
        let base_url = candidate
            .get("base_url")
            .and_then(Value::as_str)
            .map(|value| value.trim().trim_end_matches('/').to_string())
            .filter(|value| value.starts_with("http://") || value.starts_with("https://"))
            .ok_or_else(|| format!("The upstream URL for {agent_id} is unavailable"))?;
        if provider_targets_local_listener(&base_url, DTOKEN_SERVICE_PORT) {
            return Err(format!(
                "The upstream for {agent_id} points back to the d-token listener"
            ));
        }
        let protocol = parse_protocol(
            candidate
                .get("protocol")
                .and_then(Value::as_str)
                .unwrap_or("open_ai"),
        )
        .to_string();
        let model = binding_request.model.trim().to_string();
        if model.is_empty() || model.len() > 256 || model.contains(['\n', '\r', '\0']) {
            return Err(format!("The model selected for {agent_id} is invalid"));
        }

        let route_key = (protocol.clone(), model.clone());
        if let Some((existing_url, existing_agent)) = route_keys.get(&route_key) {
            if existing_url != &base_url {
                return Err(format!(
                    "{agent_id} and {existing_agent} use the same protocol/model with different upstreams; a unique reviewed route is required"
                ));
            }
            selected_agents.push(agent_id);
            continue;
        }

        let candidate_name = candidate
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("Discovered upstream");
        let candidate_source = candidate
            .get("source")
            .and_then(Value::as_str)
            .unwrap_or("discovery");
        let provider_id = safe_slug(
            &format!("auto-{agent_id}-{}", binding_request.candidate_id),
            &format!("auto-{agent_id}-provider"),
        );
        let route_id = safe_slug(
            &format!("auto-{agent_id}-route"),
            &format!("auto-{agent_id}"),
        );
        let instance_id = format!("{route_id}-instance");
        let local_chain = url::Url::parse(&base_url).ok().is_some_and(|parsed| {
            matches!(parsed.host(), Some(url::Host::Ipv4(address)) if address.is_loopback())
                || matches!(parsed.host(), Some(url::Host::Ipv6(address)) if address.is_loopback())
                || matches!(parsed.host(), Some(url::Host::Domain(domain)) if domain.eq_ignore_ascii_case("localhost"))
        }) || candidate_source.starts_with("cc-switch:");
        let provider_kind = if local_chain {
            "local_chain"
        } else if candidate_source.starts_with("opencode-provider-catalog") {
            "official"
        } else {
            "custom"
        };

        providers.push(json!({
            "id": provider_id,
            "name": candidate_name,
            "kind": provider_kind,
            "protocol": protocol,
            "base_url": base_url,
            "api_key_ref": Value::Null,
            "models": [model],
            "timeout_ms": 120000,
            "healthcheck_path": "/health"
        }));
        routes.push(json!({
            "id": route_id,
            "name": format!("{} automatic route", agent_id),
            "input_protocol": protocol,
            "instance_id": instance_id,
            "target_provider_id": provider_id,
            "fallback_provider_id": Value::Null,
            "chain_mode": if local_chain { "chain_bridge" } else { "direct" },
            "compression_policy": "balanced",
            "bind_host": "127.0.0.1",
            "local_port": DTOKEN_SERVICE_PORT
        }));
        instances.push(json!({
            "id": instance_id,
            "name": format!("{} automatic instance", agent_id),
            "agent": agent_id,
            "route_id": route_id,
            "local_port": DTOKEN_SERVICE_PORT,
            "process_status": "stopped",
            "log_path": format!("./logs/{route_id}.jsonl")
        }));
        route_keys.insert(route_key, (base_url, agent_id.clone()));
        selected_agents.push(agent_id);
    }

    if routes.is_empty() {
        return Err("Connection plan did not produce a routable upstream".to_string());
    }
    let content = serde_json::to_string_pretty(&json!({
        "providers": providers,
        "routes": routes,
        "instances": instances,
    }))
    .map_err(|error| format!("Connection route plan serialize failed: {error}"))?;
    let parent = active_config_path()
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| "Active config parent path is unavailable".to_string())?;
    fs::create_dir_all(&parent)
        .map_err(|error| format!("Active config directory create failed: {error}"))?;
    let prepared = config_patcher()
        .prepare_change(
            patchable_active_config_path()?,
            &content,
            ConfigToolKind::Dtoken,
            Some("Automatic Agent connection plan".to_string()),
            RiskLevel::Medium,
        )
        .map_err(|error| format!("Connection route review could not be prepared: {error:#}"))?;
    Ok((current_revision.to_string(), prepared, selected_agents))
}

fn preview_connection_route_plan_value(request: ConnectionRoutePlanRequest) -> Value {
    match prepare_connection_route_plan(&request) {
        Ok((revision, prepared, agents)) => json!({
            "ok": true,
            "state": "review_required",
            "source": "dtoken-desktop-auto-route-plan",
            "freshness": "fresh",
            "discovery_revision": revision,
            "target": prepared.plan.target_file,
            "before_sha256": prepared.plan.before_hash,
            "after_sha256": prepared.plan.after_hash,
            "diff": prepared.plan.diff,
            "risk_level": prepared.plan.risk_level,
            "agents": agents,
            "requires_explicit_confirmation": true,
            "summary": "Review the automatically discovered d-token route plan before applying it",
            "privacy": "The plan is rebuilt from current read-only discovery; credential values and third-party configuration are not returned or modified."
        }),
        Err(error) => json!({
            "ok": false,
            "state": "blocked",
            "source": "dtoken-desktop-auto-route-plan",
            "freshness": "unknown",
            "error": sanitize_command_output(&error),
            "requires_explicit_confirmation": true,
        }),
    }
}

#[tauri::command]
async fn preview_connection_route_plan(request: ConnectionRoutePlanRequest) -> Value {
    tauri::async_runtime::spawn_blocking(move || preview_connection_route_plan_value(request))
        .await
        .unwrap_or_else(|error| {
            json!({
                "ok": false,
                "state": "blocked",
                "source": "dtoken-desktop-auto-route-plan",
                "error": format!("Connection route preview worker failed: {error}")
            })
        })
}

fn apply_connection_route_plan_value(request: ApplyConnectionRoutePlanRequest) -> Value {
    let (Some(reviewed_before), Some(reviewed_after)) = (
        request.reviewed_before_sha256.as_deref(),
        request.reviewed_after_sha256.as_deref(),
    ) else {
        return json!({
            "ok": false,
            "state": "blocked",
            "summary": "Review the automatic route diff before applying it"
        });
    };
    let (revision, prepared, agents) = match prepare_connection_route_plan(&request.plan) {
        Ok(prepared) => prepared,
        Err(error) => {
            return json!({
                "ok": false,
                "state": "stale",
                "summary": sanitize_command_output(&error)
            });
        }
    };
    if prepared.plan.before_hash != reviewed_before || prepared.plan.after_hash != reviewed_after {
        return json!({
            "ok": false,
            "state": "stale",
            "summary": "The active route or discovered upstream changed after review"
        });
    }
    let transaction = match config_patcher().apply_transaction(&[prepared]) {
        Ok(transaction) => transaction,
        Err(error) => {
            return json!({
                "ok": false,
                "state": "failed",
                "summary": format!("Automatic route transaction failed: {error:#}")
            });
        }
    };
    let Some(receipt) = transaction.changes.first() else {
        return json!({
            "ok": false,
            "state": "failed",
            "summary": "Automatic route transaction returned no verification receipt"
        });
    };
    let after_sha256 = receipt.after.sha256.clone().unwrap_or_default();
    let result = json!({
        "ok": true,
        "state": "applied_verified",
        "source": "dtoken-desktop-auto-route-plan",
        "freshness": "fresh",
        "discovery_revision": revision,
        "transaction_id": transaction.transaction_id,
        "before_sha256": receipt.before.sha256,
        "after_sha256": after_sha256,
        "rollback_id": receipt.backup.rollback_id,
        "agents": agents,
        "verified": true,
        "summary": "Reviewed automatic d-token routes were applied with a durable rollback point"
    });
    let _ = append_desktop_event(
        "install_route_plan_apply",
        "ok",
        json!({
            "discovery_revision": result.get("discovery_revision"),
            "transaction_id": result.get("transaction_id"),
            "after_sha256": result.get("after_sha256"),
            "rollback_id": result.get("rollback_id"),
            "agents": result.get("agents"),
        }),
    );
    result
}

#[tauri::command]
async fn apply_connection_route_plan(request: ApplyConnectionRoutePlanRequest) -> Value {
    tauri::async_runtime::spawn_blocking(move || apply_connection_route_plan_value(request))
        .await
        .unwrap_or_else(|error| {
            json!({
                "ok": false,
                "state": "failed",
                "summary": format!("Connection route apply worker failed: {error}")
            })
        })
}

#[tauri::command]
fn rollback_connection_route_plan(request: RollbackConnectionRoutePlanRequest) -> ActionResult {
    let mut logs = vec![log(
        "info",
        "Restoring the reviewed d-token route rollback point",
    )];
    let patcher = config_patcher();
    let backup = match patcher.load_backup(&request.rollback_id) {
        Ok(backup) => backup,
        Err(error) => {
            logs.push(log(
                "error",
                format!("Route rollback point could not be loaded: {error:#}"),
            ));
            return ActionResult {
                ok: false,
                summary: "Route rollback point is unavailable".to_string(),
                logs,
                tools: all_checks(),
            };
        }
    };
    if patchable_active_config_path().ok().as_ref() != Some(&backup.target_path) {
        logs.push(log(
            "error",
            "Rollback point does not target the d-token active config",
        ));
        return ActionResult {
            ok: false,
            summary: "Refusing to restore an unrelated rollback point".to_string(),
            logs,
            tools: all_checks(),
        };
    }
    match patcher.restore_backup(
        &backup,
        &ExpectedFileState::Hash(request.expected_after_sha256),
    ) {
        Ok(_) => {
            logs.push(log(
                "info",
                "Exact previous d-token route bytes were restored",
            ));
            ActionResult {
                ok: true,
                summary: "The d-token route plan was rolled back exactly".to_string(),
                logs,
                tools: all_checks(),
            }
        }
        Err(error) => {
            logs.push(log(
                "error",
                format!("Route rollback failed closed: {error:#}"),
            ));
            ActionResult {
                ok: false,
                summary: "Route rollback needs attention; no unrelated file was modified"
                    .to_string(),
                logs,
                tools: all_checks(),
            }
        }
    }
}

#[tauri::command]
fn recommend_project_policy(request: ProjectPolicyRecommendRequest) -> Value {
    let input = dtoken_project_policy::RecommendationInput {
        project_id: request
            .project_id
            .unwrap_or_else(|| "desktop-project".to_string()),
        language_stack: request
            .language_stack
            .unwrap_or_else(|| vec!["rust".to_string(), "typescript".to_string()]),
        avg_request_tokens_7d: request.avg_request_tokens_7d.unwrap_or(47_307),
        avg_cli_noise_ratio_7d: request.avg_cli_noise_ratio_7d.unwrap_or(0.42),
        retrieve_rate_7d: request.retrieve_rate_7d.unwrap_or(0.02),
        failure_rate_7d: request.failure_rate_7d.unwrap_or(0.0),
        high_risk_task: request.high_risk_task.unwrap_or(false),
        manual_override: request.manual_override.unwrap_or(false),
    };
    let recommendation = dtoken_project_policy::RuleBasedRecommendationEngine.recommend(input);
    json!({
        "source": "dtoken-project-policy",
        "runtime": "d-token-owned",
        "recommendation": recommendation,
        "mapping": {
            "enable_ccr": true,
            "enable_memory": true,
            "enable_learn": true,
            "enable_output_shaper": true,
            "holdout_ratio": 0.1
        },
        "privacy": "Recommendation uses aggregate project/runtime metrics only; raw prompt/output is not read."
    })
}

fn learning_allowed_root(path: &Path) -> bool {
    if !path.is_absolute()
        || path.parent().is_none()
        || path.components().any(|part| part.as_os_str() == "..")
    {
        return false;
    }
    if env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .map(PathBuf::from)
        .as_deref()
        == Some(path)
    {
        return false;
    }
    let lower = path.to_string_lossy().to_lowercase();
    ![
        "/.ssh",
        "/.gnupg",
        "/keychains",
        "/secrets",
        "/credentials",
        "/.aws",
        "/.config/gcloud",
    ]
    .iter()
    .any(|part| lower.contains(part))
}

#[tauri::command]
fn start_learning_discovery(
    app: tauri::AppHandle,
    request: learning_discovery::StartLearningDiscoveryRequest,
) -> Value {
    learning_discovery::start(request, Some(app))
}

#[tauri::command]
fn get_learning_discovery_status(job_id: String) -> Value {
    learning_discovery::status(&job_id)
}

#[tauri::command]
fn list_learning_candidates(job_id: String) -> Value {
    learning_discovery::candidates(&job_id)
}

#[tauri::command]
fn cancel_learning_discovery(job_id: String) -> Value {
    learning_discovery::cancel(&job_id)
}

#[tauri::command]
fn analyze_learning_project(request: LearningProjectAnalysisRequest) -> Value {
    let path = PathBuf::from(&request.project_path);
    if !learning_allowed_root(&path) {
        return json!({
            "ok": false,
            "state": "blocked",
            "source": "dtoken-learning-core",
            "error": "Project path is outside the allowed learning boundary",
            "writes_performed": false
        });
    }
    match dtoken_learning::analyze_project(&path) {
        Ok(analysis) => {
            let mut value = serde_json::to_value(analysis).unwrap_or_else(|_| json!({}));
            if let Some(object) = value.as_object_mut() {
                object.insert("ok".to_string(), Value::Bool(true));
                object.insert("freshness".to_string(), Value::String(now_timestamp()));
            }
            value
        }
        Err(error) => json!({
            "ok": false,
            "state": "blocked",
            "source": "dtoken-learning-core",
            "error": error.to_string(),
            "writes_performed": false
        }),
    }
}

fn learning_target_path(target: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(target);
    let file = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    let parent = path.parent();
    let unsafe_parent = parent.is_none_or(|parent| {
        !learning_allowed_root(parent)
            || env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
                .map(PathBuf::from)
                .as_deref()
                == Some(parent)
    });
    if !learning_allowed_root(&path)
        || unsafe_parent
        || !matches!(file, "CLAUDE.md" | "AGENTS.md" | "GEMINI.md" | "d-token.md")
    {
        return Err("Learning target is outside the allowed managed-document boundary".to_string());
    }
    if let Ok(metadata) = fs::symlink_metadata(&path) {
        if metadata.file_type().is_symlink() {
            return Err("Learning target symlinks are rejected".to_string());
        }
        if metadata.len() > 1_048_576 {
            return Err("Learning target exceeds the 1 MiB safety limit".to_string());
        }
    }
    if path.exists() {
        return fs::canonicalize(&path)
            .map_err(|error| format!("Learning target canonicalization failed: {error}"));
    }
    let parent = path
        .parent()
        .ok_or_else(|| "Learning target has no parent".to_string())?;
    let canonical_parent = fs::canonicalize(parent)
        .map_err(|error| format!("Learning target parent is unavailable: {error}"))?;
    Ok(canonical_parent.join(file))
}

fn learning_managed_content(target: &Path, content: &str) -> Result<String, String> {
    if content.len() > 16_384 {
        return Err("Managed block exceeds the 16 KiB limit".to_string());
    }
    let before = fs::read_to_string(target).unwrap_or_default();
    let start = "<!-- d-token:learning:start -->";
    let end = "<!-- d-token:learning:end -->";
    if before.matches(start).count() > 1 || before.matches(end).count() > 1 {
        return Err("Managed block markers are ambiguous".to_string());
    }
    let block = format!("{start}\n{}\n{end}", content.trim());
    if let (Some(left), Some(right)) = (before.find(start), before.find(end)) {
        if right < left {
            return Err("Managed block markers are invalid".to_string());
        }
        let tail = right + end.len();
        Ok(format!("{}{}{}", &before[..left], block, &before[tail..]))
    } else if before.contains(start) || before.contains(end) {
        Err("Managed block marker is incomplete".to_string())
    } else if before.trim().is_empty() {
        Ok(format!("{block}\n"))
    } else {
        Ok(format!("{}\n\n{block}\n", before.trim_end()))
    }
}

fn prepare_learning_managed_block(
    request: &LearningManagedBlockRequest,
) -> Result<dtoken_config_patcher::PreparedChange, String> {
    let path = learning_target_path(&request.target)?;
    let next = learning_managed_content(&path, &request.content)?;
    config_patcher()
        .prepare_change(path, &next, ConfigToolKind::Dtoken, None, RiskLevel::Medium)
        .map_err(|error| format!("Managed block plan failed: {error:#}"))
}

#[tauri::command]
fn preview_learning_managed_block(request: LearningManagedBlockRequest) -> Value {
    match prepare_learning_managed_block(&request) {
        Ok(prepared) => {
            json!({ "ok": true, "state": "review", "source": "d-token-learning-control-plane", "target": prepared.plan.target_file, "before_sha256": prepared.plan.before_hash, "after_sha256": prepared.plan.after_hash, "diff": prepared.plan.diff, "requires_explicit_confirmation": true, "privacy": "Read-only preview; no file or backup was written" })
        }
        Err(error) => {
            json!({ "ok": false, "state": "blocked", "source": "d-token-learning-control-plane", "error": error })
        }
    }
}

#[tauri::command]
fn apply_learning_managed_block(request: LearningManagedBlockRequest) -> Value {
    let prepared = match prepare_learning_managed_block(&request) {
        Ok(value) => value,
        Err(error) => return json!({ "ok": false, "state": "blocked", "error": error }),
    };
    if request.reviewed_before_sha256.as_deref() != Some(&prepared.plan.before_hash)
        || request.reviewed_after_sha256.as_deref() != Some(&prepared.plan.after_hash)
    {
        return json!({ "ok": false, "state": "stale_review", "summary": "Managed block changed after review; review again" });
    }
    match config_patcher().apply_transaction(&[prepared]) {
        Ok(transaction) => transaction.changes.first().map(|receipt| json!({ "ok": true, "state": "verified", "source": "d-token-learning-control-plane", "transaction_id": transaction.transaction_id, "rollback_id": receipt.backup.rollback_id, "target": receipt.target_path, "before_sha256": receipt.before.sha256, "after_sha256": receipt.after.sha256, "verified": true, "summary": "Managed block written atomically and verified" })).unwrap_or_else(|| json!({ "ok": false, "state": "missing_receipt" })),
        Err(error) => json!({ "ok": false, "state": "failed", "summary": format!("Managed block transaction failed: {error:#}") })
    }
}

#[tauri::command]
fn get_learning_delegation_capabilities() -> Value {
    let agents = ["claude", "codex"]
        .into_iter()
        .map(|agent| {
            json!({
                "id": agent,
                "available": where_tool(agent).is_some(),
                "version": version_for(agent),
                "execution": "explicit-confirmation-only"
            })
        })
        .collect::<Vec<_>>();
    json!({ "ok": true, "source": "d-token-learning-control-plane", "agents": agents, "default_mode": "prompt", "output_policy": "raw model output is bounded, never persisted or returned; only a validated structured learning context can be returned for review" })
}

fn delegated_learning_prompt(input: &str) -> String {
    format!(
        "You are reviewing a local project for a user-approved project-learning draft. Do not read credentials, auth files, environment secrets, dependency trees, or unrelated files. Return exactly one JSON object and no markdown with this schema: {{\"summary\":\"one short project summary\",\"conventions\":[\"...\"],\"risks\":[\"...\"],\"recommendations\":[\"...\"],\"open_questions\":[\"...\"]}}. Each list may contain at most 8 short items. Do not include source code, prompt text, credentials, absolute paths, command output, or file contents.\n\nExisting local draft:\n{}",
        input
    )
}

fn read_bounded_output(mut reader: impl Read, limit: usize) -> Vec<u8> {
    let mut collected = Vec::with_capacity(limit.min(8 * 1024));
    let mut chunk = [0_u8; 4 * 1024];
    while let Ok(read) = reader.read(&mut chunk) {
        if read == 0 {
            break;
        }
        let remaining = limit.saturating_sub(collected.len());
        if remaining > 0 {
            collected.extend_from_slice(&chunk[..read.min(remaining)]);
        }
    }
    collected
}

fn normalize_delegated_text(value: &str) -> Option<String> {
    let normalized = value.split_whitespace().collect::<Vec<_>>().join(" ");
    let lower = normalized.to_ascii_lowercase();
    if normalized.is_empty()
        || normalized.len() > 512
        || normalized.contains("```")
        || [
            "authorization:",
            "bearer ",
            "api_key",
            "api-key",
            "cookie:",
            "password",
            "private key",
            "sk-ant-",
        ]
        .iter()
        .any(|marker| lower.contains(marker))
    {
        return None;
    }
    Some(normalized)
}

fn delegated_string_list(value: Option<&Value>) -> Option<Vec<String>> {
    let values = value?.as_array()?;
    if values.len() > 8 {
        return None;
    }
    values
        .iter()
        .map(|item| item.as_str().and_then(normalize_delegated_text))
        .collect()
}

fn delegated_context_from_value_at(
    value: &Value,
    depth: usize,
) -> Option<DelegatedLearningContext> {
    if depth > 8 {
        return None;
    }
    if let Some(object) = value.as_object() {
        if object.contains_key("summary") {
            return Some(DelegatedLearningContext {
                summary: object
                    .get("summary")
                    .and_then(Value::as_str)
                    .and_then(normalize_delegated_text)?,
                conventions: delegated_string_list(object.get("conventions"))?,
                risks: delegated_string_list(object.get("risks"))?,
                recommendations: delegated_string_list(object.get("recommendations"))?,
                open_questions: delegated_string_list(object.get("open_questions"))?,
            });
        }
        for key in ["result", "text", "content", "message"] {
            if let Some(text) = object.get(key).and_then(Value::as_str) {
                if let Some(context) = delegated_context_from_text_at(text, depth + 1) {
                    return Some(context);
                }
            }
        }
        for nested in object.values() {
            if let Some(context) = delegated_context_from_value_at(nested, depth + 1) {
                return Some(context);
            }
        }
    } else if let Some(values) = value.as_array() {
        for nested in values.iter().rev() {
            if let Some(context) = delegated_context_from_value_at(nested, depth + 1) {
                return Some(context);
            }
        }
    }
    None
}

fn delegated_context_from_text(raw: &str) -> Option<DelegatedLearningContext> {
    delegated_context_from_text_at(raw, 0)
}

fn delegated_context_from_text_at(raw: &str, depth: usize) -> Option<DelegatedLearningContext> {
    if depth > 8 || raw.len() > 64 * 1024 {
        return None;
    }
    let trimmed = raw
        .trim()
        .strip_prefix("```json")
        .or_else(|| raw.trim().strip_prefix("```"))
        .unwrap_or(raw.trim())
        .strip_suffix("```")
        .unwrap_or(raw.trim())
        .trim();
    if let Ok(value) = serde_json::from_str::<Value>(trimmed) {
        if let Some(context) = delegated_context_from_value_at(&value, depth + 1) {
            return Some(context);
        }
    }
    for line in raw.lines().rev() {
        if let Ok(value) = serde_json::from_str::<Value>(line.trim()) {
            if let Some(context) = delegated_context_from_value_at(&value, depth + 1) {
                return Some(context);
            }
        }
    }
    None
}

#[tauri::command]
fn spawn_learning_delegation(request: LearningDelegationRequest) -> Value {
    let project_root = PathBuf::from(&request.project_root);
    if !request.confirmed
        || !matches!(request.agent.as_str(), "claude" | "codex")
        || !learning_allowed_root(&project_root)
        || request.prompt.is_empty()
        || request.prompt.len() > 16_384
        || request.timeout_ms < 1_000
        || request.timeout_ms > 300_000
    {
        return json!({ "ok": false, "state": "blocked", "request_id": request.request_id, "summary": "Delegation request failed safety validation" });
    }
    if request.request_id.len() > 128
        || !request
            .request_id
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | ':'))
    {
        return json!({ "ok": false, "state": "blocked", "summary": "Delegation request id is invalid" });
    }
    let Some(executable) = where_tool(&request.agent) else {
        return json!({ "ok": false, "state": "unavailable", "request_id": request.request_id, "agent": request.agent, "summary": "Selected agent executable is unavailable" });
    };
    let project_root = match fs::canonicalize(&project_root) {
        Ok(path) if path.is_dir() && learning_allowed_root(&path) => path,
        _ => {
            return json!({ "ok": false, "state": "blocked", "request_id": request.request_id, "agent": request.agent, "summary": "Delegation project root is unavailable or unsafe" })
        }
    };
    let args: &[&str] = if request.agent == "claude" {
        &["-p", "--output-format", "json"]
    } else {
        &["exec", "-", "--json"]
    };
    let prompt = delegated_learning_prompt(&request.prompt);
    let started = Instant::now();
    let mut child = match Command::new(&executable)
        .args(args)
        .current_dir(&project_root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(child) => child,
        Err(error) => {
            return json!({ "ok": false, "state": "failed", "request_id": request.request_id, "agent": request.agent, "summary": format!("Agent spawn failed: {error}") })
        }
    };
    if let Some(mut stdin) = child.stdin.take() {
        if stdin.write_all(prompt.as_bytes()).is_err() {
            let _ = child.kill();
            return json!({ "ok": false, "state": "failed", "request_id": request.request_id, "agent": request.agent, "summary": "Prompt could not be written to agent stdin" });
        }
    }
    let output_reader = child
        .stdout
        .take()
        .map(|stdout| thread::spawn(move || read_bounded_output(stdout, 64 * 1024)));
    let child = Arc::new(Mutex::new(child));
    LEARNING_DELEGATIONS
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .expect("delegation registry poisoned")
        .insert(request.request_id.clone(), Arc::clone(&child));
    let (state, exit_code) = loop {
        if started.elapsed() >= Duration::from_millis(request.timeout_ms) {
            if let Ok(mut process) = child.lock() {
                let _ = process.kill();
                let _ = process.wait();
            }
            break ("timed_out", None);
        }
        match child
            .lock()
            .ok()
            .and_then(|mut process| process.try_wait().ok())
            .flatten()
        {
            Some(status) => {
                break (
                    if status.success() {
                        "complete"
                    } else {
                        "failed"
                    },
                    status.code(),
                );
            }
            None => thread::sleep(Duration::from_millis(50)),
        }
    };
    LEARNING_DELEGATIONS
        .get()
        .and_then(|registry| registry.lock().ok())
        .map(|mut registry| registry.remove(&request.request_id));
    let raw_output = output_reader
        .and_then(|reader| reader.join().ok())
        .unwrap_or_default();
    let structured_context = (state == "complete")
        .then(|| String::from_utf8(raw_output).ok())
        .flatten()
        .and_then(|raw| delegated_context_from_text(&raw));
    let completed = state == "complete" && structured_context.is_some();
    json!({
        "ok": completed,
        "state": if state == "complete" && !completed { "invalid-structured-output" } else { state },
        "source": "d-token-learning-control-plane",
        "request_id": request.request_id,
        "agent": request.agent,
        "project_root": project_root,
        "exit_code": exit_code,
        "elapsed_ms": started.elapsed().as_millis(),
        "prompt_bytes": request.prompt.len(),
        "raw_output_persisted": false,
        "raw_output_returned": false,
        "structured_context_returned": structured_context.is_some(),
        "context": structured_context,
        "summary": if completed { "Delegation returned a validated structured learning context for review" } else if state == "complete" { "Agent completed but did not return the required safe structured context" } else { "Delegation did not complete" }
    })
}

#[tauri::command]
fn cancel_learning_delegation(request_id: String) -> Value {
    let process = LEARNING_DELEGATIONS
        .get()
        .and_then(|registry| registry.lock().ok())
        .and_then(|registry| registry.get(&request_id).cloned());
    match process {
        Some(process) => {
            let stopped = process
                .lock()
                .ok()
                .is_some_and(|mut child| child.kill().is_ok());
            json!({ "ok": stopped, "state": if stopped { "cancel_requested" } else { "cancel_failed" }, "request_id": request_id, "output_persisted": false })
        }
        None => json!({ "ok": false, "state": "not_running", "request_id": request_id }),
    }
}

#[tauri::command]
fn manage_backup(action: String, id: String) -> ActionResult {
    let mut logs = vec![log(
        "info",
        format!("Backup action requested: {action} {id}"),
    )];
    let safe_id = id
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '-' || *ch == '_')
        .collect::<String>();
    if safe_id.is_empty() || safe_id != id {
        logs.push(log("error", "Invalid rollback point id"));
        return ActionResult {
            ok: false,
            summary: "Invalid rollback point id".to_string(),
            logs,
            tools: all_checks(),
        };
    }

    let config_backup_path = config_backup_root().join(&safe_id);
    if fs::symlink_metadata(&config_backup_path).is_ok() {
        return match action.as_str() {
            "restore" => {
                let patcher = config_patcher();
                let backup = match patcher.load_backup(&safe_id) {
                    Ok(backup) => backup,
                    Err(err) => {
                        logs.push(log(
                            "error",
                            format!("Config backup verification failed: {err:#}"),
                        ));
                        return ActionResult {
                            ok: false,
                            summary: "Config rollback point failed integrity verification"
                                .to_string(),
                            logs,
                            tools: all_checks(),
                        };
                    }
                };
                let expected = ExpectedFileState::Hash(backup.planned_after_sha256.clone());
                match patcher.restore_backup(&backup, &expected) {
                    Ok(receipt) => {
                        let restored_learning_document = matches!(
                            backup
                                .target_path
                                .file_name()
                                .and_then(|value| value.to_str()),
                            Some("AGENTS.md" | "CLAUDE.md" | "GEMINI.md" | "d-token.md")
                        );
                        let restored_opencode_config = backup
                            .target_path
                            .file_name()
                            .and_then(|value| value.to_str())
                            == Some("opencode.json");
                        logs.push(log(
                            "info",
                            format!(
                                "Verified config rollback {} to {}",
                                receipt.rollback_id,
                                receipt.target_path.display()
                            ),
                        ));
                        ActionResult {
                            ok: true,
                            summary: if restored_learning_document {
                                "Learning document restored exactly from verified backup"
                                    .to_string()
                            } else if restored_opencode_config {
                                "OpenCode configuration restored exactly from verified backup"
                                    .to_string()
                            } else {
                                "d-token active config restored from verified backup".to_string()
                            },
                            logs,
                            tools: all_checks(),
                        }
                    }
                    Err(err) => {
                        logs.push(log("error", format!("Config rollback refused: {err:#}")));
                        ActionResult {
                            ok: false,
                            summary: "Config rollback refused because current state or backup evidence changed"
                                .to_string(),
                            logs,
                            tools: all_checks(),
                        }
                    }
                }
            }
            "cleanup" => {
                logs.push(log(
                    "warn",
                    "Verified config backups are retained; cleanup is not available until an audited retention API is implemented",
                ));
                ActionResult {
                    ok: false,
                    summary: "Verified config backup retention is enforced".to_string(),
                    logs,
                    tools: all_checks(),
                }
            }
            _ => {
                logs.push(log("error", format!("Unsupported backup action: {action}")));
                ActionResult {
                    ok: false,
                    summary: "Unsupported backup action".to_string(),
                    logs,
                    tools: all_checks(),
                }
            }
        };
    }

    let path = app_state_dir()
        .join("rollback-points")
        .join(format!("{safe_id}.json"));
    match action.as_str() {
        "restore" => match fs::read_to_string(&path) {
            Ok(content) => {
                let parsed: Value = serde_json::from_str(&content).unwrap_or_else(|_| json!({}));
                logs.push(log(
                    "info",
                    format!("Restore plan loaded from {}", path.display()),
                ));
                logs.push(log(
                    "info",
                    format!("Restore manifest bytes: {}", content.len()),
                ));
                let active_path = active_config_path();
                let is_dtoken_active_config = parsed
                    .get("target")
                    .and_then(Value::as_str)
                    .map(|target| target == "d-token active route config")
                    .unwrap_or(false);
                if is_dtoken_active_config {
                    match parsed.get("previous_config") {
                        Some(Value::String(previous)) => {
                            if let Err(err) = serde_json::from_str::<Value>(previous) {
                                logs.push(log(
                                    "error",
                                    format!("Previous active config is not valid JSON: {err}"),
                                ));
                                return ActionResult {
                                    ok: false,
                                    summary: "Rollback manifest invalid".to_string(),
                                    logs,
                                    tools: all_checks(),
                                };
                            }
                            if let Err(err) =
                                durable_atomic_write(&active_path, previous.as_bytes())
                            {
                                logs.push(log(
                                    "error",
                                    format!("Active config restore failed: {err}"),
                                ));
                                return ActionResult {
                                    ok: false,
                                    summary: "Rollback restore failed".to_string(),
                                    logs,
                                    tools: all_checks(),
                                };
                            }
                            logs.push(log(
                                "info",
                                format!(
                                    "Restored previous d-token active config to {}",
                                    active_path.display()
                                ),
                            ));
                            logs.push(log("info", "No external agent, Headroom, RTK, cc-switch, shell profile, or system proxy config was changed"));
                            ActionResult {
                                ok: true,
                                summary: "d-token active config restored".to_string(),
                                logs,
                                tools: all_checks(),
                            }
                        }
                        Some(Value::Null) | None => {
                            if active_path.exists() {
                                if let Err(err) = durable_remove_file(&active_path) {
                                    logs.push(log(
                                        "error",
                                        format!("Active config remove failed: {err}"),
                                    ));
                                    return ActionResult {
                                        ok: false,
                                        summary: "Rollback restore failed".to_string(),
                                        logs,
                                        tools: all_checks(),
                                    };
                                }
                            }
                            logs.push(log("info", "Removed d-token active config because the rollback point had no previous config"));
                            logs.push(log("info", "No external agent, Headroom, RTK, cc-switch, shell profile, or system proxy config was changed"));
                            ActionResult {
                                ok: true,
                                summary: "d-token active config restored to empty state"
                                    .to_string(),
                                logs,
                                tools: all_checks(),
                            }
                        }
                        _ => {
                            logs.push(log("error", "Rollback manifest previous_config is invalid"));
                            ActionResult {
                                ok: false,
                                summary: "Rollback manifest invalid".to_string(),
                                logs,
                                tools: all_checks(),
                            }
                        }
                    }
                } else {
                    logs.push(log(
                        "warn",
                        "External config restore is gated; this action validates and stages the rollback plan only",
                    ));
                    ActionResult {
                        ok: true,
                        summary: "Rollback point staged for restore".to_string(),
                        logs,
                        tools: all_checks(),
                    }
                }
            }
            Err(err) => {
                logs.push(log("error", format!("Restore plan not found: {err}")));
                ActionResult {
                    ok: false,
                    summary: "Rollback point not found".to_string(),
                    logs,
                    tools: all_checks(),
                }
            }
        },
        "cleanup" => match fs::remove_file(&path) {
            Ok(()) => {
                logs.push(log(
                    "info",
                    format!("Removed rollback point {}", path.display()),
                ));
                ActionResult {
                    ok: true,
                    summary: "Rollback point cleaned up".to_string(),
                    logs,
                    tools: all_checks(),
                }
            }
            Err(err) => {
                logs.push(log("error", format!("Cleanup failed: {err}")));
                ActionResult {
                    ok: false,
                    summary: "Rollback cleanup failed".to_string(),
                    logs,
                    tools: all_checks(),
                }
            }
        },
        _ => {
            logs.push(log("error", format!("Unsupported backup action: {action}")));
            ActionResult {
                ok: false,
                summary: "Unsupported backup action".to_string(),
                logs,
                tools: all_checks(),
            }
        }
    }
}

fn rollback_preview_value(rollback_id: &str) -> Value {
    let safe_id = rollback_id
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '-' || *ch == '_')
        .collect::<String>();
    if safe_id.is_empty() || safe_id != rollback_id {
        return json!({
            "ok": false,
            "mode": "rollback-preview",
            "rollback_id": rollback_id,
            "integrity_verified": false,
            "currently_restorable": false,
            "blocked_reason": "Invalid rollback point id",
        });
    }
    if fs::symlink_metadata(config_backup_root().join(&safe_id)).is_err() {
        return json!({
            "ok": false,
            "mode": "rollback-preview",
            "rollback_id": rollback_id,
            "scope": "unknown",
            "integrity_verified": false,
            "currently_restorable": false,
            "blocked_reason": "Legacy or unknown rollback manifests cannot prove transaction scope and current hashes.",
        });
    }
    let backup = match config_patcher().load_backup(&safe_id) {
        Ok(backup) => backup,
        Err(error) => {
            return json!({
                "ok": false,
                "mode": "rollback-preview",
                "rollback_id": rollback_id,
                "scope": "unknown",
                "integrity_verified": false,
                "currently_restorable": false,
                "blocked_reason": format!("Rollback integrity verification failed: {}", sanitize_command_output(&format!("{error:#}"))),
            })
        }
    };
    let scope = rollback_scope_for_backup(&backup);
    if scope == "agent-integration" {
        let Some(transaction_id) = agent_integration_transaction_for_rollback(&backup.rollback_id)
        else {
            return json!({
                "ok": false,
                "mode": "rollback-preview",
                "rollback_id": backup.rollback_id,
                "scope": scope,
                "executor_kind": "agent_integration",
                "integrity_verified": false,
                "currently_restorable": false,
                "blocked_reason": "Agent transaction ownership could not be revalidated.",
            });
        };
        let (transaction, backups, expected_current, currently_restorable) =
            match agent_transaction_preflight(&transaction_id) {
                Ok(preflight) => preflight,
                Err(error) => {
                    return json!({
                        "ok": false,
                        "mode": "rollback-preview",
                        "rollback_id": backup.rollback_id,
                        "scope": scope,
                        "executor_kind": "agent_integration",
                        "transaction_id": transaction_id,
                        "integrity_verified": false,
                        "currently_restorable": false,
                        "blocked_reason": sanitize_command_output(&error),
                    })
                }
            };
        let revision = format!(
            "rollback-{}",
            stable_hash(
                &json!({
                    "rollback_id": backup.rollback_id,
                    "transaction_id": transaction.transaction_id,
                    "review_digest": transaction.review_digest,
                    "expected_current": expected_current,
                    "scope": scope,
                })
                .to_string()
            )
        );
        return json!({
            "ok": currently_restorable,
            "mode": "rollback-preview",
            "revision": revision,
            "rollback_id": backup.rollback_id,
            "scope": scope,
            "executor_kind": "agent_integration",
            "transaction_id": transaction.transaction_id,
            "target": format!("{} managed Agent target(s)", backups.len()),
            "files": backups.len(),
            "integrity_verified": true,
            "currently_restorable": currently_restorable,
            "blocked_reason": if currently_restorable { "" } else { "One or more Agent targets changed or the transaction is not in a restorable state." },
            "expected_current_sha256": expected_current,
            "expected_current_state": "transaction_hashes",
            "summary": if currently_restorable { "Agent transaction evidence and all current target hashes are verified" } else { "Agent transaction rollback is blocked by current-state preflight" },
        });
    }
    let current_state_matches = backup_matches_expected_current_state(&backup);
    let expected_current = backup_expected_current_token(&backup);
    let currently_restorable = backup_currently_restorable(&backup);
    let revision = format!(
        "rollback-{}",
        stable_hash(
            &json!({
                "rollback_id": backup.rollback_id,
                "manifest_sha256": backup.manifest_sha256,
                "planned_after_sha256": backup.planned_after_sha256,
                "expected_current": &expected_current,
                "scope": scope,
                "executor_kind": rollback_executor_kind(scope),
            })
            .to_string()
        )
    );
    json!({
        "ok": currently_restorable,
        "mode": "rollback-preview",
        "revision": revision,
        "rollback_id": backup.rollback_id,
        "scope": scope,
        "executor_kind": rollback_executor_kind(scope),
        "target": backup.target_path.display().to_string(),
        "files": 1,
        "integrity_verified": true,
        "currently_restorable": currently_restorable,
        "blocked_reason": rollback_blocked_reason(scope, current_state_matches),
        "expected_current_sha256": expected_current,
        "expected_current_state": if rollback_expects_missing_target(&backup) { "missing" } else { "hash" },
        "summary": if currently_restorable { "Rollback point integrity and current target hash are verified" } else { "Rollback is blocked by current-state preflight" },
    })
}

fn create_reverse_safety_backup(
    patcher: &ConfigPatcher,
    backup: &dtoken_config_patcher::BackupRecord,
    scope: &str,
) -> Result<dtoken_config_patcher::BackupRecord, String> {
    let rollback_content = if backup.original.exists {
        backup_original_text(backup)?
    } else {
        fs::read_to_string(&backup.target_path)
            .map_err(|error| format!("Current state could not be protected: {error}"))?
    };
    let prepared = patcher
        .prepare_change(
            &backup.target_path,
            &rollback_content,
            if matches!(scope, "dtoken-active-config" | "optical-config") {
                ConfigToolKind::Dtoken
            } else {
                ConfigToolKind::Agent
            },
            None,
            RiskLevel::High,
        )
        .map_err(|error| {
            format!(
                "Reverse safety backup could not be prepared: {}",
                sanitize_command_output(&format!("{error:#}"))
            )
        })?;
    let reverse_backup = patcher.backup_prepared(&prepared).map_err(|error| {
        format!(
            "Reverse safety backup failed: {}",
            sanitize_command_output(&format!("{error:#}"))
        )
    })?;
    if !backup.original.exists {
        persist_missing_rollback_expectation(&reverse_backup)?;
    }
    Ok(reverse_backup)
}

fn protect_agent_transaction_before_rollback(transaction_id: &str) -> Result<Vec<String>, String> {
    let (_, backups, _, currently_restorable) = agent_transaction_preflight(transaction_id)?;
    if !currently_restorable {
        return Err(
            "Agent transaction current hashes no longer match the reviewed state".to_string(),
        );
    }
    let patcher = config_patcher();
    backups
        .iter()
        .map(|backup| {
            create_reverse_safety_backup(&patcher, backup, "agent-integration")
                .map(|reverse| reverse.rollback_id)
        })
        .collect()
}

#[tauri::command]
fn preview_rollback(request: PreviewRollbackRequest) -> Value {
    rollback_preview_value(&request.rollback_id)
}

#[tauri::command]
fn apply_rollback(request: ApplyRollbackRequest) -> Value {
    if !request.confirmed {
        return json!({
            "ok": false,
            "mode": "rollback-receipt",
            "rollback_id": request.rollback_id,
            "summary": "Explicit rollback confirmation is required",
            "verification_state": "failed",
        });
    }
    let preview = rollback_preview_value(&request.rollback_id);
    let preview_ok = preview.get("ok").and_then(Value::as_bool).unwrap_or(false);
    let preview_revision = preview
        .get("revision")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let expected_current = preview
        .get("expected_current_sha256")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if !preview_ok
        || preview_revision != request.reviewed_revision
        || expected_current != request.expected_current_sha256
    {
        return json!({
            "ok": false,
            "mode": "rollback-receipt",
            "rollback_id": request.rollback_id,
            "summary": "Rollback review is stale or current state changed; review again",
            "verification_state": "failed",
        });
    }

    let patcher = config_patcher();
    let backup = match patcher.load_backup(&request.rollback_id) {
        Ok(backup) => backup,
        Err(error) => {
            return json!({
                "ok": false,
                "mode": "rollback-receipt",
                "rollback_id": request.rollback_id,
                "summary": format!("Rollback integrity verification failed: {}", sanitize_command_output(&format!("{error:#}"))),
                "verification_state": "failed",
            })
        }
    };
    let scope = rollback_scope_for_backup(&backup);
    if scope == "agent-integration" {
        let Some(transaction_id) = agent_integration_transaction_for_rollback(&backup.rollback_id)
        else {
            return json!({
                "ok": false,
                "mode": "rollback-receipt",
                "rollback_id": request.rollback_id,
                "summary": "Agent transaction ownership could not be revalidated",
                "verification_state": "failed",
            });
        };
        let reverse_rollback_ids = match protect_agent_transaction_before_rollback(&transaction_id)
        {
            Ok(rollback_ids) => rollback_ids,
            Err(error) => {
                return json!({
                    "ok": false,
                    "mode": "rollback-receipt",
                    "rollback_id": request.rollback_id,
                    "transaction_id": transaction_id,
                    "summary": sanitize_command_output(&error),
                    "verification_state": "failed",
                })
            }
        };
        let restored = match rollback_agent_integration(&transaction_id, &config_backup_root()) {
            Ok(restored) => restored,
            Err(error) => {
                return json!({
                    "ok": false,
                    "mode": "rollback-receipt",
                    "rollback_id": request.rollback_id,
                    "transaction_id": transaction_id,
                    "reverse_rollback_ids": reverse_rollback_ids,
                    "summary": format!("Agent transaction rollback failed: {}", sanitize_command_output(&format!("{error:#}"))),
                    "verification_state": "failed",
                })
            }
        };
        let transaction_verified =
            get_agent_integration_transaction(&transaction_id, &config_backup_root()).is_ok_and(
                |transaction| transaction.state == AgentIntegrationTransactionState::RolledBack,
            );
        let verification_state = if transaction_verified {
            "verified"
        } else {
            "failed"
        };
        let _ = append_desktop_event(
            "rollback_apply",
            if transaction_verified { "ok" } else { "failed" },
            json!({
                "rollback_id": request.rollback_id,
                "transaction_id": transaction_id,
                "reverse_rollback_ids": reverse_rollback_ids,
                "scope": scope,
                "executor_kind": "agent_integration",
                "restored_target_count": restored.len(),
                "verification_state": verification_state,
            }),
        );
        return json!({
            "ok": transaction_verified,
            "mode": "rollback-receipt",
            "rollback_id": request.rollback_id,
            "transaction_id": transaction_id,
            "reverse_rollback_ids": reverse_rollback_ids,
            "summary": if transaction_verified { "Agent integration transaction restored and verified" } else { "Agent transaction restore finished without complete verification" },
            "verification_state": verification_state,
        });
    }
    if scope == "optical-config" {
        let reverse_backup = match create_reverse_safety_backup(&patcher, &backup, scope) {
            Ok(reverse_backup) => reverse_backup,
            Err(error) => {
                return json!({
                    "ok": false,
                    "mode": "rollback-receipt",
                    "rollback_id": request.rollback_id,
                    "summary": error,
                    "verification_state": "failed",
                })
            }
        };
        let specialized = rollback_optical_config_change(RollbackOpticalConfigRequest {
            rollback_id: request.rollback_id.clone(),
        });
        let config_restored = specialized
            .get("config_restored")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let runtime_verified = specialized
            .get("runtime_verified")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let verification_state = if !config_restored {
            "failed"
        } else if runtime_verified {
            "verified"
        } else {
            "runtime_offline"
        };
        let _ = append_desktop_event(
            "rollback_apply",
            if config_restored { "ok" } else { "failed" },
            json!({
                "rollback_id": request.rollback_id,
                "reverse_rollback_id": reverse_backup.rollback_id,
                "scope": scope,
                "executor_kind": "optical",
                "config_restored": config_restored,
                "runtime_verified": runtime_verified,
                "verification_state": verification_state,
            }),
        );
        return json!({
            "ok": config_restored,
            "mode": "rollback-receipt",
            "rollback_id": request.rollback_id,
            "reverse_rollback_id": reverse_backup.rollback_id,
            "summary": specialized.get("summary").cloned().unwrap_or_else(|| json!("Optical rollback completed")),
            "verification_state": verification_state,
        });
    }
    let reverse_next_content = if backup.original.exists {
        match backup_original_text(&backup) {
            Ok(content) => content,
            Err(error) => {
                return json!({
                    "ok": false,
                    "mode": "rollback-receipt",
                    "rollback_id": request.rollback_id,
                    "summary": format!("Restored state could not be bound to the reverse safety backup: {error}"),
                    "verification_state": "failed",
                })
            }
        }
    } else {
        match fs::read_to_string(&backup.target_path) {
            Ok(content) => content,
            Err(error) => {
                return json!({
                    "ok": false,
                    "mode": "rollback-receipt",
                    "rollback_id": request.rollback_id,
                    "summary": format!("Current state could not be protected before rollback: {error}"),
                    "verification_state": "failed",
                })
            }
        }
    };
    let reverse_prepared = match patcher.prepare_change(
        &backup.target_path,
        &reverse_next_content,
        if scope == "dtoken-active-config" {
            ConfigToolKind::Dtoken
        } else {
            ConfigToolKind::Agent
        },
        None,
        RiskLevel::High,
    ) {
        Ok(prepared) => prepared,
        Err(error) => {
            return json!({
                "ok": false,
                "mode": "rollback-receipt",
                "rollback_id": request.rollback_id,
                "summary": format!("Reverse safety backup could not be prepared: {}", sanitize_command_output(&format!("{error:#}"))),
                "verification_state": "failed",
            })
        }
    };
    let reverse_backup = match patcher.backup_prepared(&reverse_prepared) {
        Ok(record) => record,
        Err(error) => {
            return json!({
                "ok": false,
                "mode": "rollback-receipt",
                "rollback_id": request.rollback_id,
                "summary": format!("Reverse safety backup failed: {}", sanitize_command_output(&format!("{error:#}"))),
                "verification_state": "failed",
            })
        }
    };
    if !backup.original.exists {
        if let Err(error) = persist_missing_rollback_expectation(&reverse_backup) {
            return json!({
                "ok": false,
                "mode": "rollback-receipt",
                "rollback_id": request.rollback_id,
                "reverse_rollback_id": reverse_backup.rollback_id,
                "summary": format!("Reverse safety backup could not bind the expected missing state: {error}"),
                "verification_state": "failed",
            });
        }
    }

    let expected_current_state = if request.expected_current_sha256 == "missing" {
        ExpectedFileState::Missing
    } else {
        ExpectedFileState::Hash(request.expected_current_sha256.clone())
    };
    let restore = patcher.restore_backup(&backup, &expected_current_state);
    if let Err(error) = restore {
        return json!({
            "ok": false,
            "mode": "rollback-receipt",
            "rollback_id": request.rollback_id,
            "reverse_rollback_id": reverse_backup.rollback_id,
            "summary": format!("Rollback restore was refused: {}", sanitize_command_output(&format!("{error:#}"))),
            "verification_state": "failed",
        });
    }

    let file_verified = if backup.original.exists {
        backup.original.sha256.as_deref() == file_sha256(&backup.target_path).ok().as_deref()
    } else {
        !backup.target_path.exists()
    };
    let verification_state = if !file_verified {
        "failed"
    } else if scope == "dtoken-active-config" {
        let state = dtoken_service_status_value();
        match state.get("state").and_then(Value::as_str) {
            Some("running" | "restart_required") => {
                let restart = restart_dtoken_service();
                if restart.ok
                    && verify_agent_route_config()
                        .get("ok")
                        .and_then(Value::as_bool)
                        .unwrap_or(false)
                {
                    "verified"
                } else {
                    "runtime_offline"
                }
            }
            _ => "runtime_offline",
        }
    } else {
        "verified"
    };
    let ok = file_verified && verification_state != "failed";
    let _ = append_desktop_event(
        "rollback_apply",
        if ok { "ok" } else { "failed" },
        json!({
            "rollback_id": request.rollback_id,
            "reverse_rollback_id": reverse_backup.rollback_id,
            "scope": scope,
            "target": backup.target_path.display().to_string(),
            "file_verified": file_verified,
            "verification_state": verification_state,
        }),
    );
    json!({
        "ok": ok,
        "mode": "rollback-receipt",
        "rollback_id": request.rollback_id,
        "reverse_rollback_id": reverse_backup.rollback_id,
        "summary": if verification_state == "verified" { "Rollback restored the exact prior state and verification passed" } else if ok { "Rollback restored the exact prior file state; runtime verification is pending" } else { "Rollback file verification failed" },
        "verification_state": verification_state,
    })
}

#[tauri::command]
fn detect_tools() -> ActionResult {
    let tools = all_checks();
    let mut logs = vec![log(
        "info",
        "Scanning local PATH for RTK, Headroom, and OpenCode",
    )];
    for tool in &tools {
        let level = if tool.installed { "info" } else { "warn" };
        logs.push(log(
            level,
            format!(
                "{}: {}",
                tool.label,
                tool.version.clone().unwrap_or_else(|| tool.detail.clone())
            ),
        ));
    }
    ActionResult {
        ok: tools.iter().all(|tool| tool.installed),
        summary: "Local tool scan complete".to_string(),
        logs,
        tools,
    }
}

#[tauri::command]
fn install_tool(target: String) -> ActionResult {
    match target.as_str() {
        "rtk" | "headroom" => install_or_verify(&target),
        "all" => {
            let mut logs = vec![log("info", "Starting one-click install workflow")];
            let rtk = install_or_verify("rtk");
            logs.extend(rtk.logs);
            let headroom = install_or_verify("headroom");
            logs.extend(headroom.logs);
            logs.push(log("info", "Running final local verification"));
            let tools = all_checks();
            let ok = tools
                .iter()
                .filter(|tool| tool.id == "rtk" || tool.id == "headroom")
                .all(|tool| tool.installed);
            logs.push(log(
                if ok { "info" } else { "error" },
                if ok {
                    "One-click workflow completed successfully"
                } else {
                    "One-click workflow finished with missing tools"
                },
            ));
            ActionResult {
                ok,
                summary: if ok {
                    "RTK and Headroom verified".to_string()
                } else {
                    "Some tools are still missing".to_string()
                },
                logs,
                tools,
            }
        }
        _ => ActionResult {
            ok: false,
            summary: format!("Unknown install target: {target}"),
            logs: vec![log("error", format!("Unknown install target: {target}"))],
            tools: all_checks(),
        },
    }
}

#[tauri::command]
fn preview_install_plan(request: InstallPlanRequest) -> Value {
    install_plan_json(&request)
}

#[tauri::command]
fn preview_opencode_connection(request: OpenCodeConnectionRequest) -> OpenCodeConnectionReview {
    let review = opencode_adapter::preview(&request, &config_backup_root());
    let status = if review.ok { "ok" } else { "failed" };
    let _ = append_desktop_event(
        "install_scan_plan_diff",
        status,
        json!({
            "transaction": review.transaction,
            "revision": review.revision,
            "target": review.target,
            "model": review.model,
            "listener": review.listener,
            "before_sha256": review.before_sha256,
            "after_sha256": review.after_sha256,
            "review_required": review.ok,
        }),
    );
    review
}

#[tauri::command]
fn apply_opencode_connection(request: OpenCodeConnectionRequest) -> Value {
    if service_health_payload().is_none() {
        let receipt = OpenCodeConnectionReceipt {
            ok: false,
            state: "failed".to_string(),
            source: "dtoken-desktop-opencode-adapter",
            freshness: "fresh",
            target: "<normal-opencode-config>".to_string(),
            model: request.model.clone(),
            listener: format!("127.0.0.1:{}", request.listener_port),
            transaction: "scan-plan-diff-backup-confirm-apply-verify-doctor-rollback".to_string(),
            revision: request.reviewed_revision.clone(),
            before_sha256: None,
            after_sha256: None,
            rollback_id: None,
            verified: false,
            summary: "d-token runtime identity is not healthy; OpenCode was not modified".to_string(),
            privacy: "Normal strict JSON config only; auth.json, credentials, prompts, and responses are never read or returned",
        };
        let _ = append_desktop_event(
            "install_confirm",
            "failed",
            json!({ "revision": request.reviewed_revision, "reason": "runtime_identity_unavailable" }),
        );
        return json!(receipt);
    }

    let _ = append_desktop_event(
        "install_confirm",
        "ok",
        json!({
            "revision": request.reviewed_revision,
            "before_sha256": request.reviewed_before_sha256,
            "after_sha256": request.reviewed_after_sha256,
            "model": request.model,
        }),
    );
    let mut receipt = opencode_adapter::apply(&request, &config_backup_root());
    if !receipt.ok {
        let _ = append_desktop_event(
            "install_backup_apply_verify",
            "failed",
            json!({ "revision": receipt.revision, "summary": receipt.summary }),
        );
        return json!(receipt);
    }

    let active = active_route_config_status();
    let doctor_ok = active.get("state").and_then(Value::as_str) == Some("loaded")
        && active.get("fallback_disabled").and_then(Value::as_bool) == Some(true)
        && service_health_payload().is_some();
    if !doctor_ok {
        let rollback_id = receipt.rollback_id.clone().unwrap_or_default();
        let recovery = config_patcher()
            .load_backup(&rollback_id)
            .and_then(|backup| {
                config_patcher().restore_backup(
                    &backup,
                    &ExpectedFileState::Hash(receipt.after_sha256.clone().unwrap_or_default()),
                )
            });
        receipt.ok = false;
        receipt.verified = false;
        receipt.state = if recovery.is_ok() {
            "recovered"
        } else {
            "recovery_failed"
        }
        .to_string();
        receipt.summary = if recovery.is_ok() {
            "Post-apply Doctor failed; exact original OpenCode bytes were restored".to_string()
        } else {
            "Post-apply Doctor failed and exact recovery needs attention".to_string()
        };
        let _ = append_desktop_event(
            "install_doctor_recovery",
            if recovery.is_ok() {
                "recovered"
            } else {
                "failed"
            },
            json!({
                "revision": receipt.revision,
                "rollback_id": receipt.rollback_id,
                "exact_bytes_restored": recovery.is_ok(),
            }),
        );
        return json!(receipt);
    }

    let _ = append_desktop_event(
        "install_backup_apply_verify_doctor_commit",
        "ok",
        json!({
            "revision": receipt.revision,
            "before_sha256": receipt.before_sha256,
            "after_sha256": receipt.after_sha256,
            "rollback_id": receipt.rollback_id,
            "model": receipt.model,
            "listener": receipt.listener,
            "verified": true,
        }),
    );
    json!(receipt)
}

#[tauri::command]
async fn discover_upstream_candidates() -> Value {
    tauri::async_runtime::spawn_blocking(upstream_discovery::discover)
        .await
        .map(enrich_upstream_discovery)
        .unwrap_or_else(|error| {
            json!({
                "source": "dtoken-desktop-read-only-discovery",
                "freshness": "unknown",
                "revision": "unavailable",
                "state": "unavailable",
                "candidates": [],
                "agent_bindings": [],
                "sources": [],
                "selection_required": false,
                "cc_switch_sync": null,
                "writes_performed": false,
                "privacy": "No credential values were read or returned",
                "error": format!("Discovery worker failed: {error}")
            })
        })
}

fn enrich_upstream_discovery(mut discovery: Value) -> Value {
    let active = active_route_config_status();
    let active_state = active
        .get("state")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let active_endpoints = active
        .get("providers")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|provider| provider.get("base_url").and_then(Value::as_str))
        .map(normalized_endpoint)
        .collect::<Vec<_>>();
    if let Some(sync) = discovery
        .get_mut("cc_switch_sync")
        .and_then(Value::as_object_mut)
    {
        let detected = sync
            .get("detected")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let cc_endpoint = sync
            .get("upstream")
            .and_then(Value::as_str)
            .map(normalized_endpoint);
        let status = derive_cc_switch_status(
            detected,
            cc_endpoint.as_deref(),
            &active_endpoints,
            active_state,
        );
        sync.insert("status".to_string(), Value::String(status.to_string()));
    }
    if let Some(object) = discovery.as_object_mut() {
        let prior_revision = object
            .get("revision")
            .and_then(Value::as_str)
            .unwrap_or("setup-unavailable");
        let route_revision = stable_hash(
            &json!({
                "discovery_revision": prior_revision,
                "active_state": active_state,
                "active_endpoints": active_endpoints,
                "cc_switch_status": object.get("cc_switch_sync").and_then(|value| value.get("status"))
            })
            .to_string(),
        );
        object.insert(
            "revision".to_string(),
            Value::String(format!("setup-{route_revision}")),
        );
        object.insert(
            "dtoken_route_state".to_string(),
            json!({
                "state": active_state,
                "provider_count": active.get("provider_count").cloned().unwrap_or(json!(0)),
                "route_count": active.get("route_count").cloned().unwrap_or(json!(0)),
                "source": "d-token-active-config",
            }),
        );
    }
    discovery
}

fn normalized_endpoint(value: &str) -> String {
    value.trim().trim_end_matches('/').to_ascii_lowercase()
}

fn derive_cc_switch_status(
    detected: bool,
    cc_endpoint: Option<&str>,
    active_endpoints: &[String],
    active_state: &str,
) -> &'static str {
    if !detected {
        "unavailable"
    } else if cc_endpoint
        .is_some_and(|endpoint| active_endpoints.iter().any(|active| active == endpoint))
    {
        "synced"
    } else if active_state == "loaded" {
        "out-of-sync"
    } else {
        "needs-confirmation"
    }
}

#[tauri::command]
async fn get_dtoken_service_status() -> Value {
    tauri::async_runtime::spawn_blocking(dtoken_service_status_value)
        .await
        .unwrap_or_else(|err| {
            json!({
                "ok": false,
                "state": "unknown",
                "summary": format!("d-token service status task failed: {err}")
            })
        })
}

#[tauri::command]
fn get_autostart_status() -> autostart::AutostartStatus {
    autostart::status()
}

#[tauri::command]
fn enable_autostart() -> autostart::AutostartActionResult {
    let result = autostart::enable();
    let _ = append_desktop_event(
        "autostart_enable",
        if result.ok { "ok" } else { "failed" },
        json!({
            "state": result.status.state,
            "source": result.status.source,
            "rollback_performed": result.rollback_performed,
        }),
    );
    result
}

#[tauri::command]
fn disable_autostart() -> autostart::AutostartActionResult {
    let result = autostart::disable();
    let _ = append_desktop_event(
        "autostart_disable",
        if result.ok { "ok" } else { "failed" },
        json!({
            "state": result.status.state,
            "source": result.status.source,
            "rollback_performed": result.rollback_performed,
        }),
    );
    result
}

fn start_service_when_launched_by_autostart() {
    if !autostart::has_service_start_argument(env::args_os()) {
        return;
    }
    thread::spawn(|| {
        let result = start_dtoken_service();
        let _ = append_desktop_event(
            "autostart_service_start",
            if result.ok { "ok" } else { "failed" },
            json!({ "ok": result.ok, "summary": result.summary }),
        );
    });
}

#[tauri::command]
fn start_dtoken_service() -> ActionResult {
    let result = start_dtoken_service_impl();
    let _ = append_desktop_event(
        "service_start",
        if result.ok { "ok" } else { "failed" },
        json!({ "ok": result.ok, "summary": result.summary.clone() }),
    );
    result
}

#[tauri::command]
fn restart_dtoken_service() -> ActionResult {
    let result = restart_dtoken_service_impl();
    let _ = append_desktop_event(
        "service_restart",
        if result.ok { "ok" } else { "failed" },
        json!({ "ok": result.ok, "summary": result.summary.clone() }),
    );
    result
}

fn restart_dtoken_service_impl() -> ActionResult {
    let current = dtoken_service_status_value();
    match current.get("state").and_then(Value::as_str) {
        Some("running" | "restart_required") => {
            let stop = stop_dtoken_service_impl();
            if !stop.ok {
                return ActionResult {
                    ok: false,
                    summary: format!(
                        "Managed d-token service could not be stopped for reload: {}",
                        stop.summary
                    ),
                    logs: stop.logs,
                    tools: stop.tools,
                };
            }
            let mut start = start_dtoken_service_impl();
            if start.ok {
                start.summary = "Managed d-token service restarted and verified".to_string();
            }
            start
        }
        Some("unmanaged_running") => ActionResult {
            ok: false,
            summary: "An unmanaged d-token-compatible service owns the port; reload was refused"
                .to_string(),
            logs: vec![log(
                "warn",
                "Refused to stop or replace an unmanaged process on 127.0.0.1:18001",
            )],
            tools: all_checks(),
        },
        _ => {
            let mut start = start_dtoken_service_impl();
            if start.ok {
                start.summary = "Managed d-token service started and verified".to_string();
            }
            start
        }
    }
}

fn start_dtoken_service_impl() -> ActionResult {
    let mut logs = vec![log(
        "info",
        "Starting d-token-owned local service lifecycle action",
    )];
    let current = dtoken_service_status_value();
    if current.get("state").and_then(Value::as_str) == Some("running") {
        logs.push(log(
            "info",
            "d-token local service is already running and managed by this app",
        ));
        return ActionResult {
            ok: true,
            summary: "d-token local service is already running".to_string(),
            logs,
            tools: all_checks(),
        };
    }
    if current.get("state").and_then(Value::as_str) == Some("restart_required") {
        logs.push(log(
            "warn",
            "The managed service belongs to an older or unverifiable bundle; use the explicit restart action to replace it safely",
        ));
        return ActionResult {
            ok: false,
            summary: "Managed d-token service requires an explicit restart".to_string(),
            logs,
            tools: all_checks(),
        };
    }
    if current.get("state").and_then(Value::as_str) == Some("unmanaged_running") {
        logs.push(log(
            "warn",
            "127.0.0.1:18001 is responding as d-token but was not started by this app; leaving it untouched",
        ));
        return ActionResult {
            ok: false,
            summary: "Unmanaged d-token-compatible service is already using the port".to_string(),
            logs,
            tools: all_checks(),
        };
    }

    if let Err(err) = validate_active_config_for_start() {
        logs.push(log(
            "error",
            format!("Active config validation failed: {err}"),
        ));
        return ActionResult {
            ok: false,
            summary: "Configure and apply a real d-token route before starting the service"
                .to_string(),
            logs,
            tools: all_checks(),
        };
    }

    let exe = match resolve_dtoken_proxy_exe() {
        Ok(path) => path,
        Err(err) => {
            logs.push(log("error", err));
            return ActionResult {
                ok: false,
                summary: "d-token runtime executable is missing".to_string(),
                logs,
                tools: all_checks(),
            };
        }
    };
    let launch_exe_sha256 = match file_sha256(&exe) {
        Ok(sha256) => sha256,
        Err(err) => {
            logs.push(log(
                "error",
                format!("Runtime executable identity could not be read before launch: {err}"),
            ));
            return ActionResult {
                ok: false,
                summary: "d-token runtime executable identity is unavailable".to_string(),
                logs,
                tools: all_checks(),
            };
        }
    };
    let config_path = active_config_path();
    let data_dir = service_data_dir();
    if let Err(err) = fs::create_dir_all(&data_dir) {
        logs.push(log(
            "error",
            format!("Runtime data dir create failed: {err}"),
        ));
        return ActionResult {
            ok: false,
            summary: "d-token service start failed before launch".to_string(),
            logs,
            tools: all_checks(),
        };
    }
    let log_path = service_log_path();
    let stdout = match open_log_file(&log_path) {
        Ok(file) => file,
        Err(err) => {
            logs.push(log("error", format!("Service log open failed: {err}")));
            return ActionResult {
                ok: false,
                summary: "d-token service start failed before launch".to_string(),
                logs,
                tools: all_checks(),
            };
        }
    };
    let stderr = match stdout.try_clone() {
        Ok(file) => file,
        Err(err) => {
            logs.push(log("error", format!("Service log clone failed: {err}")));
            return ActionResult {
                ok: false,
                summary: "d-token service start failed before launch".to_string(),
                logs,
                tools: all_checks(),
            };
        }
    };

    let runtime_instance_nonce = Uuid::new_v4().to_string();
    let child = Command::new(&exe)
        .arg("--host")
        .arg("127.0.0.1")
        .arg("--port")
        .arg(DTOKEN_SERVICE_PORT.to_string())
        .arg("--config")
        .arg(&config_path)
        .arg("--data-dir")
        .arg(&data_dir)
        .env("DTOKEN_RUNTIME_INSTANCE_NONCE", &runtime_instance_nonce)
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr))
        .spawn();
    let child = match child {
        Ok(child) => child,
        Err(err) => {
            logs.push(log("error", format!("Service process spawn failed: {err}")));
            return ActionResult {
                ok: false,
                summary: "d-token service start failed".to_string(),
                logs,
                tools: all_checks(),
            };
        }
    };
    let pid = child.id();
    let Some(process_birth_token) = process_birth_token(pid) else {
        let _ = terminate_process(pid);
        logs.push(log(
            "error",
            "spawned process birth identity could not be captured; process was stopped",
        ));
        return ActionResult {
            ok: false,
            summary: "d-token service process identity is unavailable".to_string(),
            logs,
            tools: all_checks(),
        };
    };
    logs.push(log(
        "info",
        format!(
            "spawn: d-token runtime pid={} exe={} port={}",
            pid,
            exe.display(),
            DTOKEN_SERVICE_PORT
        ),
    ));

    let mut healthy = false;
    for _ in 0..40 {
        std::thread::sleep(Duration::from_millis(100));
        let health = service_health_payload();
        healthy = health.as_ref().is_some_and(|payload| {
            payload.get("component").and_then(Value::as_str) == Some("dtoken-core-api")
                && payload.get("runtime_identity").and_then(Value::as_str)
                    == Some("dtoken-runtime-local")
                && payload
                    .get("runtime_instance_nonce")
                    .and_then(Value::as_str)
                    == Some(runtime_instance_nonce.as_str())
        });
        if healthy {
            break;
        }
    }

    if !healthy {
        logs.push(log(
            "error",
            "verify: d-token service did not become healthy on 127.0.0.1:18001",
        ));
        let _ = terminate_process(pid);
        let _ = remove_service_manifest();
        return ActionResult {
            ok: false,
            summary: "d-token service did not become healthy".to_string(),
            logs,
            tools: all_checks(),
        };
    }

    if !executable_identity_matches(pid, &exe) || !listener_owned_by_pid(pid, DTOKEN_SERVICE_PORT) {
        let _ = terminate_process(pid);
        let _ = remove_service_manifest();
        logs.push(log(
            "error",
            "verify: spawned PID, executable, and listening port ownership could not be proven",
        ));
        return ActionResult {
            ok: false,
            summary: "d-token service ownership verification failed".to_string(),
            logs,
            tools: all_checks(),
        };
    }

    match file_sha256(&exe) {
        Ok(current_sha256) if current_sha256 == launch_exe_sha256 => {}
        Ok(_) => {
            let _ = terminate_process(pid);
            let _ = remove_service_manifest();
            logs.push(log(
                "error",
                "verify: runtime executable changed while the service was starting; the newly spawned process was stopped",
            ));
            return ActionResult {
                ok: false,
                summary:
                    "d-token service start rolled back because the bundle changed during launch"
                        .to_string(),
                logs,
                tools: all_checks(),
            };
        }
        Err(err) => {
            let _ = terminate_process(pid);
            let _ = remove_service_manifest();
            logs.push(log(
                "error",
                format!(
                    "verify: runtime executable identity could not be read after launch; the newly spawned process was stopped: {err}"
                ),
            ));
            return ActionResult {
                ok: false,
                summary:
                    "d-token service start rolled back because executable identity verification failed"
                        .to_string(),
                logs,
                tools: all_checks(),
            };
        }
    }

    let manifest = ManagedServiceManifest {
        schema_version: MANAGED_SERVICE_MANIFEST_SCHEMA_VERSION,
        managed_by: "d-token-desktop".to_string(),
        identity: "dtoken-core-api".to_string(),
        pid,
        port: DTOKEN_SERVICE_PORT,
        exe_path: exe.display().to_string(),
        exe_sha256: Some(launch_exe_sha256),
        process_birth_token: Some(process_birth_token),
        runtime_instance_nonce: Some(runtime_instance_nonce),
        config_path: config_path.display().to_string(),
        data_dir: data_dir.display().to_string(),
        started_at: now_label(),
    };
    let manifest_path = match write_service_manifest(&manifest) {
        Ok(path) => path,
        Err(err) => {
            let _ = terminate_process(pid);
            let _ = remove_service_manifest();
            logs.push(log(
                "error",
                format!("Durable service manifest write failed; process was stopped: {err}"),
            ));
            return ActionResult {
                ok: false,
                summary:
                    "d-token service start rolled back because lifecycle state was not durable"
                        .to_string(),
                logs,
                tools: all_checks(),
            };
        }
    };
    if read_service_manifest().as_ref() != Some(&manifest) {
        let _ = terminate_process(pid);
        let _ = remove_service_manifest();
        logs.push(log(
            "error",
            "Service manifest verification failed; process was stopped",
        ));
        return ActionResult {
            ok: false,
            summary:
                "d-token service start rolled back because lifecycle state verification failed"
                    .to_string(),
            logs,
            tools: all_checks(),
        };
    }
    logs.push(log(
        "info",
        format!(
            "manifest: d-token service manifest durably written to {}",
            manifest_path.display()
        ),
    ));
    logs.push(log(
        "info",
        "verify: d-token local service is healthy and managed by this app",
    ));
    ActionResult {
        ok: true,
        summary: "d-token local service started".to_string(),
        logs,
        tools: all_checks(),
    }
}

#[tauri::command]
fn stop_dtoken_service() -> ActionResult {
    let result = stop_dtoken_service_impl();
    let _ = append_desktop_event(
        "service_stop",
        if result.ok { "ok" } else { "failed" },
        json!({ "ok": result.ok, "summary": result.summary.clone() }),
    );
    result
}

fn stop_dtoken_service_impl() -> ActionResult {
    let mut logs = vec![log(
        "info",
        "Stopping d-token-owned local service lifecycle action",
    )];
    let Some(manifest) = read_service_manifest() else {
        logs.push(log(
            "warn",
            "No d-token service manifest exists; no process will be stopped",
        ));
        return ActionResult {
            ok: false,
            summary: "No managed d-token service is registered".to_string(),
            logs,
            tools: all_checks(),
        };
    };
    let owned_manifest =
        manifest.managed_by == "d-token-desktop" && manifest.identity == "dtoken-core-api";
    if !owned_manifest {
        logs.push(log(
            "error",
            format!(
                "Service manifest identity mismatch: managed_by='{}' identity='{}'",
                manifest.managed_by, manifest.identity
            ),
        ));
        return ActionResult {
            ok: false,
            summary: "Service manifest identity check failed".to_string(),
            logs,
            tools: all_checks(),
        };
    }
    if !is_process_running(manifest.pid) {
        logs.push(log(
            "warn",
            format!(
                "Manifest pid {} is not running; clearing stale manifest",
                manifest.pid
            ),
        ));
        let _ = remove_service_manifest();
        return ActionResult {
            ok: true,
            summary: "Stale d-token service manifest cleared".to_string(),
            logs,
            tools: all_checks(),
        };
    }
    let health = service_health_payload();
    let health_identity_ok = health.as_ref().is_some_and(|payload| {
        payload.get("component").and_then(Value::as_str) == Some("dtoken-core-api")
            && payload.get("runtime_identity").and_then(Value::as_str)
                == Some("dtoken-runtime-local")
            && manifest.runtime_instance_nonce.as_deref().is_some()
            && payload
                .get("runtime_instance_nonce")
                .and_then(Value::as_str)
                == manifest.runtime_instance_nonce.as_deref()
    });
    if !health_identity_ok {
        logs.push(log(
            "error",
            "Service health identity could not be verified on 127.0.0.1:18001; refusing to stop process",
        ));
        return ActionResult {
            ok: false,
            summary: "Service health identity check failed".to_string(),
            logs,
            tools: all_checks(),
        };
    }
    let identity = ProcessIdentity {
        managed_by: manifest.managed_by.clone(),
        component: manifest.identity.clone(),
        pid: manifest.pid,
        port: manifest.port,
        executable_path: PathBuf::from(&manifest.exe_path),
        executable_sha256: manifest.exe_sha256.clone().unwrap_or_default(),
        process_birth_token: manifest.process_birth_token.clone().unwrap_or_default(),
    };
    match ProcessManager::new().stop_verified(&identity, &SystemProcessInspector) {
        Ok(()) => {
            if let Err(err) = remove_service_manifest() {
                logs.push(log(
                    "error",
                    format!("Managed service stopped but manifest cleanup failed: {err}"),
                ));
                return ActionResult {
                    ok: false,
                    summary: "d-token service stopped but lifecycle state cleanup failed"
                        .to_string(),
                    logs,
                    tools: all_checks(),
                };
            }
            logs.push(log(
                "info",
                format!("Stopped managed d-token service pid {}", manifest.pid),
            ));
            ActionResult {
                ok: true,
                summary: "d-token local service stopped".to_string(),
                logs,
                tools: all_checks(),
            }
        }
        Err(err) => {
            logs.push(log("error", format!("Process stop failed: {err}")));
            ActionResult {
                ok: false,
                summary: "d-token local service stop failed".to_string(),
                logs,
                tools: all_checks(),
            }
        }
    }
}

#[tauri::command]
fn run_install_transaction(request: InstallPlanRequest) -> ActionResult {
    let targets = install_targets(&request.target);
    let mut logs = vec![
        log(
            "info",
            format!(
                "scan: starting installation transaction target={} mode={}",
                request.target, request.mode
            ),
        ),
        log(
            "info",
            "scope: OpenCode connection route is scoped to d-token; Claude Code, Cursor, Codex CLI and system proxy settings are excluded",
        ),
    ];

    if targets.is_empty() {
        logs.push(log(
            "error",
            format!("plan: unknown install target {}", request.target),
        ));
        return ActionResult {
            ok: false,
            summary: "Installation target is unknown".to_string(),
            logs,
            tools: all_checks(),
        };
    }

    let plan = install_plan_json(&request);
    logs.push(log(
        "info",
        format!(
            "plan: {} target(s), install_root={}, dtoken_route_port={}",
            targets.len(),
            request.install_root,
            request.proxy_port
        ),
    ));
    logs.push(log(
        "info",
        "diff: generated config diff for user confirmation",
    ));

    let writes_enabled = matches!(request.mode.as_str(), "install" | "repair" | "connect");
    let backup_persisted = if writes_enabled && request.auto_backup {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_secs().to_string())
            .unwrap_or_else(|_| "unknown".to_string());
        match write_json_app_state(&["backups", &stamp, "install-plan.json"], &plan) {
            Ok(path) => {
                logs.push(log(
                    "info",
                    format!(
                        "backup: wrote d-token rollback manifest to {}",
                        path.display()
                    ),
                ));
                true
            }
            Err(err) => {
                logs.push(log(
                    "error",
                    format!("backup: failed to write manifest: {err}"),
                ));
                false
            }
        }
    } else if writes_enabled {
        logs.push(log(
            "warn",
            "backup: automatic backup is disabled for this explicit state-changing action",
        ));
        true
    } else {
        logs.push(log(
            "info",
            "backup: read-only mode does not persist a backup or rollback artifact",
        ));
        true
    };

    if !writes_enabled {
        logs.push(log(
            "info",
            "apply: current mode is read-only/manual, so external tool installation is verified but not changed",
        ));
    }

    let mut ok = backup_persisted;
    if !backup_persisted && writes_enabled {
        logs.push(log(
            "error",
            "apply: aborted because the requested recovery manifest was not durable",
        ));
        return ActionResult {
            ok: false,
            summary: "Installation transaction aborted before apply".to_string(),
            logs,
            tools: all_checks(),
        };
    }
    for target in &targets {
        match *target {
            "rtk" => {
                let check = check_tool("rtk", "RTK");
                if check.installed {
                    logs.push(log(
                        "info",
                        format!(
                            "apply: RTK already installed and verified ({})",
                            check
                                .version
                                .unwrap_or_else(|| "version unknown".to_string())
                        ),
                    ));
                } else if writes_enabled {
                    ok = false;
                    logs.push(log(
                        "error",
                        "apply: RTK is missing and no signed RTK installer source is configured yet",
                    ));
                } else {
                    ok = false;
                    logs.push(log("warn", "verify: RTK is missing in read-only mode"));
                }
            }
            "headroom" => {
                let check = check_tool("headroom", "Headroom");
                if check.installed {
                    logs.push(log(
                        "info",
                        format!(
                            "apply: Headroom already installed and verified ({})",
                            check
                                .version
                                .unwrap_or_else(|| "version unknown".to_string())
                        ),
                    ));
                } else if writes_enabled {
                    ok = false;
                    logs.push(log(
                        "error",
                        "apply: Headroom is missing and no signed Headroom installer source is configured yet",
                    ));
                } else {
                    ok = false;
                    logs.push(log("warn", "verify: Headroom is missing in read-only mode"));
                }
            }
            "dtoken" => {
                match env::current_exe() {
                    Ok(path) => logs.push(log(
                        "info",
                        format!(
                            "apply: d-token desktop binary verified at {}",
                            path.display()
                        ),
                    )),
                    Err(err) => {
                        ok = false;
                        logs.push(log(
                            "error",
                            format!("apply: d-token binary check failed: {err}"),
                        ));
                    }
                }
                if writes_enabled || request.mode == "connect" {
                    let service = start_dtoken_service();
                    ok &= service.ok;
                    logs.extend(service.logs);
                }
                let service_status = dtoken_service_status_value();
                logs.push(log(
                    if service_status.get("state").and_then(Value::as_str) == Some("running") {
                        "info"
                    } else {
                        "warn"
                    },
                    format!(
                        "verify: d-token service lifecycle state={} managed={} url={}",
                        service_status
                            .get("state")
                            .and_then(Value::as_str)
                            .unwrap_or("unknown"),
                        service_status
                            .get("managed")
                            .and_then(Value::as_bool)
                            .unwrap_or(false),
                        service_status
                            .get("service_url")
                            .and_then(Value::as_str)
                            .unwrap_or("http://127.0.0.1:18001")
                    ),
                ));
            }
            _ => {}
        }
    }

    let tools = all_checks();
    logs.push(log("info", "verify: refreshed local tool scan"));
    for endpoint in headroom_endpoints() {
        logs.push(log(
            if endpoint.healthy { "info" } else { "warn" },
            format!("verify: {} -> {}", endpoint.label, endpoint.detail),
        ));
    }

    if request.run_doctor_after {
        let doctor_ok = targets.iter().all(|target| match *target {
            "dtoken" => {
                let status = dtoken_service_status_value();
                status.get("state").and_then(Value::as_str) == Some("running")
                    && status.get("managed").and_then(Value::as_bool) == Some(true)
                    && active_route_config_status()
                        .get("state")
                        .and_then(Value::as_str)
                        == Some("loaded")
            }
            "rtk" | "headroom" => tools
                .iter()
                .find(|tool| tool.id == *target)
                .is_some_and(|tool| tool.installed),
            _ => false,
        });
        ok = ok && doctor_ok;
        logs.push(log(
            if doctor_ok { "info" } else { "error" },
            if doctor_ok {
                "doctor: install prerequisites passed"
            } else {
                "doctor: install prerequisites failed"
            },
        ));
    } else {
        logs.push(log("warn", "doctor: skipped by user option"));
    }

    if request.rollback_point && writes_enabled {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_secs().to_string())
            .unwrap_or_else(|_| "unknown".to_string());
        let rollback = json!({
            "createdAt": now_label(),
            "target": request.target,
            "mode": request.mode,
            "ok": ok,
            "tools": tools,
            "note": "d-token-owned rollback point; external restore remains gated behind explicit restore action"
        });
        match write_json_app_state(&["rollback-points", &format!("{stamp}.json")], &rollback) {
            Ok(path) => logs.push(log(
                "info",
                format!("rollback: committed rollback point {}", path.display()),
            )),
            Err(err) => {
                ok = false;
                logs.push(log(
                    "error",
                    format!("rollback: failed to write point: {err}"),
                ));
            }
        }
    } else if writes_enabled {
        logs.push(log(
            "warn",
            "rollback: rollback point disabled by user option",
        ));
    } else {
        logs.push(log(
            "info",
            "rollback: read-only mode does not create a rollback point",
        ));
    }

    ActionResult {
        ok,
        summary: if ok {
            format!("{} installation transaction completed", request.target)
        } else {
            format!(
                "{} installation transaction needs attention",
                request.target
            )
        },
        logs,
        tools: all_checks(),
    }
}

const DOCTOR_V3_TTL_MS: u64 = 5 * 60 * 1_000;
const DOCTOR_PLAN_TTL_MS: u64 = 10 * 60 * 1_000;

fn doctor_v3_snapshot_value() -> Option<Value> {
    read_app_state(&["doctor", "latest-v3.json"]).ok()
}

fn doctor_v3_snapshot_is_fresh(value: &Value, now_ms: u64) -> bool {
    value.get("schema_version").and_then(Value::as_u64) == Some(3)
        && value
            .get("collected_at_ms")
            .and_then(Value::as_u64)
            .zip(value.get("ttl_ms").and_then(Value::as_u64))
            .filter(|(collected, ttl)| *collected > 0 && *ttl > 0 && now_ms >= *collected)
            .and_then(|(collected, ttl)| collected.checked_add(ttl))
            .is_some_and(|expires| now_ms <= expires)
}

fn doctor_v2_snapshot_freshness(value: &Value, now_ms: u64) -> &'static str {
    let collected_at_ms = value
        .get("collected_at")
        .and_then(Value::as_str)
        .and_then(|value| value.strip_prefix("unix:"))
        .and_then(|value| value.parse::<u64>().ok())
        .and_then(|seconds| seconds.checked_mul(1_000));

    match collected_at_ms {
        Some(collected) if collected > 0 && now_ms >= collected => collected
            .checked_add(DOCTOR_V3_TTL_MS)
            .map_or(
                "unknown",
                |expires| {
                    if now_ms <= expires {
                        "fresh"
                    } else {
                        "stale"
                    }
                },
            ),
        _ => "unknown",
    }
}

fn doctor_v2_snapshot_value_at(now_ms: u64) -> Option<Value> {
    let mut value = read_app_state(&["doctor", "latest.json"]).ok()?;
    let freshness = doctor_v2_snapshot_freshness(&value, now_ms);
    value
        .as_object_mut()?
        .insert("freshness".to_string(), json!(freshness));
    Some(value)
}

fn current_doctor_project_root() -> Option<PathBuf> {
    env::current_dir().ok().filter(|path| {
        path.is_dir()
            && (path.join(".git").exists()
                || path.join("AGENTS.md").exists()
                || path.join("CLAUDE.md").exists()
                || path.join("Cargo.toml").exists()
                || path.join("package.json").exists())
    })
}

fn doctor_selection_scope(
    selection: &DoctorModuleRepairSelection,
) -> Result<(AgentIntegrationScope, Option<PathBuf>), String> {
    match selection.scope.as_str() {
        "global" => Ok((AgentIntegrationScope::Global, None)),
        "project" => {
            let root = selection
                .project_root
                .as_deref()
                .filter(|value| !value.trim().is_empty())
                .map(PathBuf::from)
                .ok_or_else(|| "Project scope requires an absolute project root".to_string())?;
            if !root.is_absolute() || !root.is_dir() {
                return Err("Project root must be an existing absolute directory".to_string());
            }
            Ok((AgentIntegrationScope::Project, Some(root)))
        }
        _ => Err("Doctor repair scope must be global or project".to_string()),
    }
}

fn doctor_agent_has_managed_evidence(
    agent_id: &str,
    selection: &DoctorModuleRepairSelection,
) -> Result<bool, String> {
    let home = user_home_dir()?;
    let (scope, project_root) = doctor_selection_scope(selection)?;
    let evidence =
        inspect_agent_integration_evidence(agent_id, scope, &home, project_root.as_deref())
            .map_err(|error| format!("Agent managed-surface inspection failed: {error:#}"))?;
    Ok(!evidence.managed_surfaces.is_empty())
}

fn doctor_prepare_agent_integration(
    agent_id: &str,
    selection: &DoctorModuleRepairSelection,
) -> Result<dtoken_config_patcher::agent_integration::PreparedAgentIntegration, String> {
    if !selection.install_instructions
        && !selection.install_hook_or_plugin
        && !selection.install_skill
        && !selection.install_mcp
    {
        return Err("Select at least one Agent integration surface".to_string());
    }
    let (_, project_root) = doctor_selection_scope(selection)?;
    prepare_hybrid_agent_integration(&HybridAgentIntegrationDraftRequest {
        agent_ids: vec![agent_id.to_string()],
        scope: selection.scope.clone(),
        project_root: project_root.map(|path| path.display().to_string()),
        install_instructions: selection.install_instructions,
        install_hook_or_plugin: selection.install_hook_or_plugin,
        install_skill: selection.install_skill,
        install_mcp: selection.install_mcp,
    })
}

fn doctor_agent_inventory() -> (Vec<Value>, bool, usize) {
    let snapshot = match agent_registry::scan_current_process_agents() {
        Ok(snapshot) => snapshot,
        Err(error) => {
            return (
                vec![json!({
                    "id": "registry",
                    "name": "Agent registry",
                    "detected": false,
                    "supported": false,
                    "state": "unavailable",
                    "summary": sanitize_command_output(&format!("{error:#}")),
                })],
                false,
                0,
            )
        }
    };
    let home = match user_home_dir() {
        Ok(home) => home,
        Err(_) => return (Vec::new(), false, 0),
    };
    let project_root = current_doctor_project_root();
    let mut complete = true;
    let mut repairable = 0usize;
    let agents = snapshot
        .agents
        .into_iter()
        .map(|agent| {
            let supported = matches!(agent.id.as_str(), "claude-code" | "codex" | "opencode");
            if !supported {
                return json!({
                    "id": agent.id,
                    "name": agent.name,
                    "detected": true,
                    "supported": false,
                    "state": "unsupported",
                    "managed_scopes": [],
                });
            }
            let global = inspect_agent_integration_evidence(
                &agent.id,
                AgentIntegrationScope::Global,
                &home,
                None,
            );
            let project = project_root.as_deref().map(|root| {
                inspect_agent_integration_evidence(
                    &agent.id,
                    AgentIntegrationScope::Project,
                    &home,
                    Some(root),
                )
            });
            if global.is_err() || project.as_ref().is_some_and(Result::is_err) {
                complete = false;
            }
            let global_managed = global
                .as_ref()
                .is_ok_and(|evidence| !evidence.managed_surfaces.is_empty());
            let project_managed = project
                .as_ref()
                .and_then(|result| result.as_ref().ok())
                .is_some_and(|evidence| !evidence.managed_surfaces.is_empty());
            let managed = global_managed || project_managed;
            let mut needs_repair = false;
            for (scope, root, enabled) in [
                ("global", None, global_managed),
                (
                    "project",
                    project_root.as_ref().map(|path| path.display().to_string()),
                    project_managed,
                ),
            ] {
                if !enabled {
                    continue;
                }
                let selection = DoctorModuleRepairSelection {
                    agent_ids: vec![agent.id.clone()],
                    route_ids: Vec::new(),
                    scope: scope.to_string(),
                    project_root: root,
                    install_instructions: true,
                    install_hook_or_plugin: true,
                    install_skill: true,
                    install_mcp: false,
                };
                match doctor_prepare_agent_integration(&agent.id, &selection) {
                    Ok(prepared) => {
                        needs_repair |= prepared
                            .plan
                            .targets
                            .iter()
                            .any(|target| target.before_sha256 != target.after_sha256);
                    }
                    Err(_) => complete = false,
                }
            }
            if needs_repair {
                repairable += 1;
            }
            let managed_scopes = [
                global_managed.then_some("global"),
                project_managed.then_some("project"),
            ]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();
            let managed_targets = [
                global_managed.then(|| json!({ "scope": "global", "project_root": Value::Null })),
                project_managed.then(|| json!({
                    "scope": "project",
                    "project_root": project_root.as_ref().map(|path| path.display().to_string()),
                })),
            ]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();
            json!({
                "id": agent.id,
                "name": agent.name,
                "detected": true,
                "supported": true,
                "state": if !managed { "detected_only" } else if needs_repair { "repairable" } else { "verified" },
                "managed_scopes": managed_scopes,
                "managed_targets": managed_targets,
                "needs_repair": needs_repair,
            })
        })
        .collect();
    (agents, complete, repairable)
}

fn doctor_module_state_rank(state: &str) -> u8 {
    match state {
        "blocked" => 3,
        "manual" | "repairable" => 2,
        "healthy" => 1,
        _ => 0,
    }
}

fn run_doctor_scan_value() -> Value {
    let collected_at_ms = epoch_ms();
    let service = dtoken_service_status_value();
    let service_state = service
        .get("state")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let active = active_route_config_status();
    let route = verify_agent_route_config();
    let active_ready = active.get("state").and_then(Value::as_str) == Some("loaded")
        && active
            .get("route_count")
            .and_then(Value::as_u64)
            .unwrap_or(0)
            > 0
        && active
            .get("provider_count")
            .and_then(Value::as_u64)
            .unwrap_or(0)
            > 0
        && active.get("fallback_disabled").and_then(Value::as_bool) == Some(true);
    let route_verified = route.get("ok").and_then(Value::as_bool).unwrap_or(false);
    let core_state = match service_state {
        "running" if active_ready && route_verified => "healthy",
        "running" if !active_ready || !route_verified => "manual",
        "stopped" => "repairable",
        "restart_required" => "repairable",
        "unmanaged_running" => "blocked",
        _ => "unknown",
    };
    let core_summary = match core_state {
        "healthy" => "d-token 服务身份、监听端口和活动路由已验证",
        "repairable" => "d-token 自有服务未运行或需要安全重载",
        "blocked" => "监听端口存在无法证明归属的进程，自动操作已阻止",
        "manual" => "服务在线，但活动路由或运行时复检尚未通过",
        _ => "核心服务证据不完整",
    };

    let (agents, agent_complete, agent_repairs) = doctor_agent_inventory();
    let managed_agents = agents
        .iter()
        .filter(|agent| {
            matches!(
                agent.get("state").and_then(Value::as_str),
                Some("repairable" | "verified")
            )
        })
        .count();
    let agent_state = if !agent_complete {
        "manual"
    } else if agent_repairs > 0 {
        "repairable"
    } else if managed_agents > 0 {
        "healthy"
    } else {
        "manual"
    };

    let optical = optical_editor_value();
    let optical_ok = optical.get("ok").and_then(Value::as_bool).unwrap_or(false);
    let optical_issues = optical
        .get("issues")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);
    let optical_state = if optical_ok && optical_issues == 0 {
        "healthy"
    } else if optical_ok {
        "manual"
    } else {
        "unknown"
    };

    let autostart = autostart::status();
    let platform_state = if autostart.supported && autostart.state != "error" {
        "healthy"
    } else if autostart.supported {
        "manual"
    } else {
        "unknown"
    };

    let rollback = rollback_points_result();
    let (recovery_state, recovery_summary) = match &rollback {
        Ok(points)
            if points.iter().any(|point| {
                point.get("integrity_verified").and_then(Value::as_bool) != Some(true)
                    || matches!(
                        point.get("scope").and_then(Value::as_str),
                        Some("unknown" | "invalid-backup-evidence")
                    )
            }) =>
        {
            ("blocked", "至少一个备份或事务证据不完整，恢复操作保持锁定")
        }
        Ok(_) => ("healthy", "备份元数据与事务归属检查完成"),
        Err(_) => ("blocked", "无法读取备份与回退证据"),
    };

    let modules = vec![
        json!({ "module_id": "core", "state": core_state, "severity": if core_state == "blocked" { "error" } else { "warning" }, "summary": core_summary }),
        json!({ "module_id": "agent", "state": agent_state, "severity": "warning", "summary": if managed_agents == 0 { "未发现已有 d-token 接入证据的受支持 Agent" } else if agent_repairs > 0 { "已发现需要重新注入或修复的 d-token Agent 管理区" } else { "现有 Agent 管理区已验证，可按需重新配置" }, "agents": agents }),
        json!({ "module_id": "optical", "state": optical_state, "severity": "warning", "summary": if optical_state == "healthy" { "Optical 保持 off/shadow_preview 安全边界" } else { "Optical Shadow 配置需要在视觉压缩工作台处理" } }),
        json!({ "module_id": "platform", "state": platform_state, "severity": "warning", "summary": autostart.summary }),
        json!({ "module_id": "recovery", "state": recovery_state, "severity": if recovery_state == "blocked" { "error" } else { "warning" }, "summary": recovery_summary }),
    ];
    let mut healthy_count = 0usize;
    let mut repairable_count = 0usize;
    let mut manual_count = 0usize;
    let mut blocked_count = 0usize;
    for module in &modules {
        if let Some(state) = module.get("state").and_then(Value::as_str) {
            match state {
                "healthy" => healthy_count += 1,
                "repairable" => repairable_count += 1,
                "manual" => manual_count += 1,
                "blocked" => blocked_count += 1,
                _ => {}
            }
        }
    }
    let counts = json!({
        "healthy": healthy_count,
        "repairable": repairable_count,
        "manual": manual_count,
        "blocked": blocked_count,
    });
    let overall_state = modules
        .iter()
        .filter_map(|module| module.get("state").and_then(Value::as_str))
        .max_by_key(|state| doctor_module_state_rank(state))
        .unwrap_or("unknown");
    let revision = format!(
        "doctor-v3-{}",
        stable_hash(
            &json!({
                "collected_at_ms": collected_at_ms,
                "service": service,
                "active": active,
                "route_ok": route_verified,
                "modules": modules,
            })
            .to_string(),
        )
    );
    let domain_modules = vec![
        DoctorModuleEvidence {
            module_id: DoctorModuleId::CoreServiceLifecycle,
            state: doctor_domain_state(core_state),
            observed_at_ms: collected_at_ms,
            evidence: vec![
                DoctorEvidence {
                    kind: DoctorEvidenceKind::Identity,
                    state: doctor_domain_state(core_state),
                },
                DoctorEvidence {
                    kind: DoctorEvidenceKind::Ownership,
                    state: doctor_domain_state(core_state),
                },
            ],
        },
        DoctorModuleEvidence {
            module_id: DoctorModuleId::AgentHookPlugin,
            state: doctor_domain_state(agent_state),
            observed_at_ms: collected_at_ms,
            evidence: vec![DoctorEvidence {
                kind: DoctorEvidenceKind::Configuration,
                state: doctor_domain_state(agent_state),
            }],
        },
        DoctorModuleEvidence {
            module_id: DoctorModuleId::OpticalShadowPolicy,
            state: doctor_domain_state(optical_state),
            observed_at_ms: collected_at_ms,
            evidence: vec![DoctorEvidence {
                kind: DoctorEvidenceKind::Policy,
                state: doctor_domain_state(optical_state),
            }],
        },
        DoctorModuleEvidence {
            module_id: DoctorModuleId::PlatformAutostart,
            state: doctor_domain_state(platform_state),
            observed_at_ms: collected_at_ms,
            evidence: vec![DoctorEvidence {
                kind: DoctorEvidenceKind::Configuration,
                state: doctor_domain_state(platform_state),
            }],
        },
        DoctorModuleEvidence {
            module_id: DoctorModuleId::RecoveryRollbackEvidence,
            state: doctor_domain_state(recovery_state),
            observed_at_ms: collected_at_ms,
            evidence: vec![DoctorEvidence {
                kind: DoctorEvidenceKind::Rollback,
                state: doctor_domain_state(recovery_state),
            }],
        },
    ];
    let domain = DoctorDomainSnapshot::new(
        revision.clone(),
        DoctorSnapshotSource::Combined,
        collected_at_ms,
        DOCTOR_V3_TTL_MS,
        doctor_domain_state(overall_state),
        domain_modules,
    );
    let contract = domain
        .ok()
        .and_then(|snapshot| serde_json::to_value(snapshot).ok())
        .unwrap_or(Value::Null);
    let result = json!({
        "schema_version": 3,
        "revision": revision,
        "source": "dtoken-desktop-control-plane",
        "state": overall_state,
        "collected_at_ms": collected_at_ms,
        "ttl_ms": DOCTOR_V3_TTL_MS,
        "counts": counts,
        "coverage": {
            "completed": modules.iter().filter(|module| module.get("state").and_then(Value::as_str) != Some("unknown")).count(),
            "total": modules.len(),
            "agents_detected": modules.get(1).and_then(|module| module.get("agents")).and_then(Value::as_array).map(Vec::len).unwrap_or(0),
            "agents_selected": managed_agents,
        },
        "modules": modules,
        "module_contract": contract,
        "redaction_policy_version": 1,
    });
    let _ = write_json_app_state(&["doctor", "latest-v3.json"], &result);
    let _ = append_desktop_event(
        "doctor_scan_v3",
        if overall_state == "healthy" {
            "ok"
        } else {
            "attention"
        },
        json!({
            "revision": result.get("revision"),
            "state": overall_state,
            "repairable": result.get("counts").and_then(|value| value.get("repairable")),
            "blocked": result.get("counts").and_then(|value| value.get("blocked")),
        }),
    );
    result
}

fn doctor_domain_state(state: &str) -> DoctorEvidenceState {
    match state {
        "healthy" => DoctorEvidenceState::Healthy,
        "repairable" | "manual" => DoctorEvidenceState::Attention,
        "blocked" => DoctorEvidenceState::Blocked,
        "unknown" => DoctorEvidenceState::Unknown,
        _ => DoctorEvidenceState::Unavailable,
    }
}

#[tauri::command]
fn get_doctor_snapshot() -> Value {
    doctor_v3_snapshot_value()
        .or_else(|| doctor_v2_snapshot_value_at(epoch_ms()))
        .unwrap_or(Value::Null)
}

#[tauri::command]
fn run_doctor_scan() -> Value {
    run_doctor_scan_value()
}

#[tauri::command]
fn get_doctor_repair_modules() -> Value {
    json!({
        "schema_version": 3,
        "source": "dtoken-doctor",
        "modules": doctor_repair_modules(),
        "supported_agent_integrations": ["claude-code", "codex", "opencode"],
        "agent_transport_executor": ["opencode"],
        "optical_active_allowed": false,
        "cc_switch_write_allowed": false,
    })
}

#[tauri::command]
fn run_doctor() -> DoctorReport {
    let tools = all_checks();
    let service = dtoken_service_status_value();
    let active = active_route_config_status();
    let route = verify_agent_route_config();
    let rollback_state = rollback_points_result();
    let mut issues = Vec::new();
    let mut healthy = 0usize;

    match service.get("state").and_then(Value::as_str) {
        Some("running") => healthy += 1,
        Some("restart_required") => issues.push(DoctorIssue {
            id: "service-restart-required".to_string(),
            category: "runtime".to_string(),
            severity: "warning".to_string(),
            state: "repairable".to_string(),
            title: "d-token 本地服务需要重启".to_string(),
            impact: "当前服务版本或二进制身份已变化，新请求可能无法使用最新能力。".to_string(),
            cause: "桌面应用确认该进程属于 d-token，但运行中的二进制与当前版本不一致。".to_string(),
            evidence: "服务清单、PID、端口所有权与 runtime identity 已验证；binary identity 要求重新加载。".to_string(),
            remediation: Some(DoctorRemediation {
                kind: "restart_managed_service".to_string(),
                label: "重启并验证本地服务".to_string(),
                risk: "low".to_string(),
                writes_config: false,
                requires_confirmation: true,
                destination: None,
            }),
        }),
        Some("unmanaged_running") => issues.push(DoctorIssue {
            id: "external-port-owner".to_string(),
            category: "port".to_string(),
            severity: "error".to_string(),
            state: "blocked".to_string(),
            title: "端口由非 d-token 托管进程占用".to_string(),
            impact: "桌面应用不能安全启动或重启自己的本地服务。".to_string(),
            cause: "127.0.0.1:18001 有兼容响应，但 PID、可执行文件或启动清单不能证明属于当前应用。".to_string(),
            evidence: "进程身份与端口所有权未同时满足 d-token-owned lifecycle 合同。".to_string(),
            remediation: None,
        }),
        _ => issues.push(DoctorIssue {
            id: "service-not-running".to_string(),
            category: "runtime".to_string(),
            severity: "error".to_string(),
            state: "repairable".to_string(),
            title: "d-token 本地服务未运行".to_string(),
            impact: "Agent 请求当前无法经过 d-token。".to_string(),
            cause: "未检测到由当前桌面应用管理的运行实例。".to_string(),
            evidence: "127.0.0.1:18001 未返回已验证的 d-token runtime identity。".to_string(),
            remediation: Some(DoctorRemediation {
                kind: "restart_managed_service".to_string(),
                label: "启动并验证本地服务".to_string(),
                risk: "low".to_string(),
                writes_config: false,
                requires_confirmation: true,
                destination: None,
            }),
        }),
    }

    if doctor_agent_ready(&tools) {
        healthy += 1;
    } else {
        issues.push(DoctorIssue {
            id: "supported-agent-missing".to_string(),
            category: "route".to_string(),
            severity: "warning".to_string(),
            state: "manual".to_string(),
            title: "未检测到当前验收 Agent".to_string(),
            impact: "无法证明真实 Agent 请求已经连接到 d-token。".to_string(),
            cause: "OpenCode 当前不在 PATH，或尚未完成安装与连接。".to_string(),
            evidence: "本次本地工具扫描未检测到 OpenCode；RTK 与外部 Headroom 不参与此门禁。"
                .to_string(),
            remediation: Some(DoctorRemediation {
                kind: "open_install".to_string(),
                label: "前往安装与连接".to_string(),
                risk: "medium".to_string(),
                writes_config: true,
                requires_confirmation: true,
                destination: Some("install".to_string()),
            }),
        });
    }

    let active_ok = active.get("state").and_then(Value::as_str) == Some("loaded")
        && active
            .get("route_count")
            .and_then(Value::as_u64)
            .unwrap_or(0)
            > 0
        && active
            .get("provider_count")
            .and_then(Value::as_u64)
            .unwrap_or(0)
            > 0
        && active.get("fallback_disabled").and_then(Value::as_bool) == Some(true);
    if active_ok {
        healthy += 1;
    } else {
        issues.push(DoctorIssue {
            id: "active-config-not-ready".to_string(),
            category: "route".to_string(),
            severity: "error".to_string(),
            state: "manual".to_string(),
            title: "活动路由配置尚未就绪".to_string(),
            impact: "本地服务即使启动，也不能安全路由到已确认的 Provider。".to_string(),
            cause: "活动配置缺少路由、Provider，或静默 fallback 尚未被证明禁用。".to_string(),
            evidence: format!(
                "state={} · routes={} · providers={} · fallback_disabled={}",
                active
                    .get("state")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown"),
                active
                    .get("route_count")
                    .and_then(Value::as_u64)
                    .unwrap_or(0),
                active
                    .get("provider_count")
                    .and_then(Value::as_u64)
                    .unwrap_or(0),
                active
                    .get("fallback_disabled")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
            ),
            remediation: Some(DoctorRemediation {
                kind: "open_install".to_string(),
                label: "前往安装与连接".to_string(),
                risk: "high".to_string(),
                writes_config: true,
                requires_confirmation: true,
                destination: Some("install".to_string()),
            }),
        });
    }

    if route.get("ok").and_then(Value::as_bool).unwrap_or(false) {
        healthy += 1;
    } else if active_ok && service.get("state").and_then(Value::as_str) == Some("running") {
        issues.push(DoctorIssue {
            id: "route-verification-failed".to_string(),
            category: "route".to_string(),
            severity: "warning".to_string(),
            state: "manual".to_string(),
            title: "Agent 路由复检未通过".to_string(),
            impact: "配置文件存在，但运行时、Dashboard 或脱敏导出证据不完整。".to_string(),
            cause: "至少一个路由验证步骤未返回当前快照的通过证据。".to_string(),
            evidence: format!(
                "passed={}/{}",
                route.get("passed").and_then(Value::as_u64).unwrap_or(0),
                route.get("total").and_then(Value::as_u64).unwrap_or(0)
            ),
            remediation: Some(DoctorRemediation {
                kind: "open_install".to_string(),
                label: "检查连接配置".to_string(),
                risk: "medium".to_string(),
                writes_config: true,
                requires_confirmation: true,
                destination: Some("install".to_string()),
            }),
        });
    }

    match rollback_state {
        Ok(_) => healthy += 1,
        Err(error) => issues.push(DoctorIssue {
            id: "rollback-state-unreadable".to_string(),
            category: "security".to_string(),
            severity: "error".to_string(),
            state: "manual".to_string(),
            title: "回退证据无法读取".to_string(),
            impact: "新的配置写入与回退操作应保持锁定，避免丢失恢复能力。".to_string(),
            cause: "桌面事务存储或配置备份目录无法完成只读验证。".to_string(),
            evidence: sanitize_command_output(&error),
            remediation: Some(DoctorRemediation {
                kind: "open_changes".to_string(),
                label: "查看备份与回退".to_string(),
                risk: "high".to_string(),
                writes_config: false,
                requires_confirmation: false,
                destination: Some("changes".to_string()),
            }),
        }),
    }

    let collected_at = now_timestamp();
    let revision = format!(
        "doctor-{}",
        stable_hash(
            &json!({
                "collected_at": collected_at,
                "service": service,
                "active": active,
                "route_ok": route.get("ok"),
                "issues": issues.iter().map(|issue| issue.id.as_str()).collect::<Vec<_>>()
            })
            .to_string()
        )
    );
    let repairable = issues
        .iter()
        .filter(|issue| issue.state == "repairable")
        .count();
    let manual = issues
        .iter()
        .filter(|issue| issue.state == "manual")
        .count();
    let blocked = issues
        .iter()
        .filter(|issue| issue.state == "blocked")
        .count();
    let report = DoctorReport {
        ok: issues.is_empty(),
        mode_name: "doctor-report-v2".to_string(),
        revision,
        source: "dtoken-desktop-control-plane".to_string(),
        freshness: "fresh".to_string(),
        collected_at,
        summary: if issues.is_empty() {
            "All common Doctor checks passed".to_string()
        } else {
            format!("{} common Doctor issue(s) need attention", issues.len())
        },
        counts: DoctorCounts {
            healthy,
            repairable,
            manual,
            blocked,
        },
        issues,
    };
    if let Ok(value) = serde_json::to_value(&report) {
        let _ = write_json_app_state(&["doctor", "latest.json"], &value);
    }
    let _ = append_desktop_event(
        "doctor_scan",
        if report.ok { "ok" } else { "attention" },
        json!({
            "revision": report.revision,
            "repairable": report.counts.repairable,
            "manual": report.counts.manual,
            "blocked": report.counts.blocked,
        }),
    );
    report
}

fn validate_doctor_repair_request(request: &DoctorRepairRequest) -> Result<Vec<String>, String> {
    let snapshot = read_app_state(&["doctor", "latest.json"])
        .map_err(|_| "Run Doctor before preparing a repair".to_string())?;
    if doctor_v2_snapshot_freshness(&snapshot, epoch_ms()) != "fresh" {
        return Err("The Doctor snapshot is stale; scan again before repairing".to_string());
    }
    let report: DoctorReport = serde_json::from_value(snapshot)
        .map_err(|_| "The saved Doctor snapshot is invalid".to_string())?;
    if request.report_revision != report.revision {
        return Err("The Doctor snapshot changed; scan again before repairing".to_string());
    }
    if request.issue_ids.is_empty() || request.issue_ids.len() > 8 {
        return Err("Select at least one valid Doctor issue".to_string());
    }
    let mut kinds = Vec::new();
    for issue_id in &request.issue_ids {
        let issue = report
            .issues
            .iter()
            .find(|issue| &issue.id == issue_id)
            .ok_or_else(|| "A selected Doctor issue no longer exists".to_string())?;
        let remediation = issue
            .remediation
            .as_ref()
            .filter(|item| issue.state == "repairable" && item.risk == "low")
            .ok_or_else(|| {
                "A selected Doctor issue is not eligible for common repair".to_string()
            })?;
        if remediation.kind != "restart_managed_service" {
            return Err("The selected Doctor repair is not on the safe allowlist".to_string());
        }
        kinds.push(remediation.kind.clone());
    }
    kinds.sort();
    kinds.dedup();
    Ok(kinds)
}

#[tauri::command]
fn preview_doctor_common_repair(request: DoctorRepairRequest) -> Value {
    match validate_doctor_repair_request(&request) {
        Ok(_) => json!({
            "ok": true,
            "mode": "doctor-repair-plan",
            "report_revision": request.report_revision,
            "issue_ids": request.issue_ids,
            "risk": "low",
            "summary": "Restart the verified d-token-owned service and run Doctor again.",
            "writes_config": false,
            "external_processes_touched": false,
        }),
        Err(error) => json!({
            "ok": false,
            "mode": "doctor-repair-plan",
            "report_revision": request.report_revision,
            "issue_ids": request.issue_ids,
            "risk": "blocked",
            "summary": error,
        }),
    }
}

#[tauri::command]
fn apply_doctor_common_repair(request: DoctorRepairRequest) -> Value {
    if request.confirmed != Some(true) {
        return json!({
            "ok": false,
            "mode": "doctor-action-receipt",
            "summary": "Explicit confirmation is required",
            "report": get_doctor_snapshot(),
        });
    }
    let kinds = match validate_doctor_repair_request(&request) {
        Ok(kinds) => kinds,
        Err(error) => {
            return json!({
                "ok": false,
                "mode": "doctor-action-receipt",
                "summary": error,
                "report": get_doctor_snapshot(),
            })
        }
    };
    let operation_id = format!("doctor-repair-{}", stable_hash(&now_timestamp()));
    let mut action_ok = true;
    for kind in kinds {
        if kind == "restart_managed_service" {
            let result = restart_dtoken_service();
            action_ok &= result.ok;
        }
    }
    let report = run_doctor();
    let _ = append_desktop_event(
        "doctor_repair",
        if action_ok { "ok" } else { "failed" },
        json!({
            "operation_id": operation_id,
            "issue_ids": request.issue_ids,
            "post_scan_revision": report.revision,
            "post_scan_ok": report.ok,
        }),
    );
    json!({
        "ok": action_ok,
        "mode": "doctor-action-receipt",
        "operation_id": operation_id,
        "summary": if action_ok { "Repair completed and Doctor verification reran" } else { "Repair was refused or failed; Doctor verification reran" },
        "report": report,
    })
}

fn doctor_domain_module_id(module_id: &str) -> Option<DoctorModuleId> {
    match module_id {
        "core" | "core_service_lifecycle" => Some(DoctorModuleId::CoreServiceLifecycle),
        "agent" | "agent_hybrid_reinject" => Some(DoctorModuleId::AgentHookPlugin),
        "agent_instructions" => Some(DoctorModuleId::AgentInstructions),
        "agent_hook_plugin" => Some(DoctorModuleId::AgentHookPlugin),
        "agent_skill" => Some(DoctorModuleId::AgentSkill),
        "agent_metadata_mcp" => Some(DoctorModuleId::AgentMetadataMcp),
        "optical" | "optical_shadow_policy" => Some(DoctorModuleId::OpticalShadowPolicy),
        "platform" | "platform_autostart" => Some(DoctorModuleId::PlatformAutostart),
        "recovery" | "recovery_rollback_evidence" => Some(DoctorModuleId::RecoveryRollbackEvidence),
        _ => None,
    }
}

fn doctor_domain_selection(
    module_id: DoctorModuleId,
    selection: &DoctorModuleRepairSelection,
) -> Result<DoctorRepairSelection, String> {
    let scope = match selection.scope.as_str() {
        "global" => DoctorRepairScope::Global,
        "project" => DoctorRepairScope::Project,
        _ => return Err("Doctor repair scope must be global or project".to_string()),
    };
    let agent_module = matches!(
        module_id,
        DoctorModuleId::AgentInstructions
            | DoctorModuleId::AgentHookPlugin
            | DoctorModuleId::AgentSkill
            | DoctorModuleId::AgentMetadataMcp
    );
    Ok(DoctorRepairSelection {
        scope,
        agent_ids: if agent_module {
            selection.agent_ids.clone()
        } else {
            Vec::new()
        },
        route_ids: Vec::new(),
    })
}

fn doctor_plan_path(plan_id: &str) -> Result<PathBuf, String> {
    if plan_id.is_empty()
        || plan_id.len() > 128
        || !plan_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err("Invalid Doctor repair plan id".to_string());
    }
    Ok(app_state_dir()
        .join("doctor")
        .join("plans")
        .join(format!("{plan_id}.json")))
}

fn store_doctor_plan(plan: &StoredDoctorModulePlan) -> Result<(), String> {
    let file_name = format!("{}.json", plan.plan_id);
    write_json_app_state(
        &["doctor", "plans", &file_name],
        &serde_json::to_value(plan).map_err(|error| error.to_string())?,
    )
    .map(|_| ())
}

fn load_doctor_plan(plan_id: &str) -> Result<StoredDoctorModulePlan, String> {
    let path = doctor_plan_path(plan_id)?;
    let bytes = fs::read(&path).map_err(|_| "Doctor repair plan is unavailable".to_string())?;
    if bytes.len() > 2 * 1024 * 1024 {
        return Err("Doctor repair plan exceeds the size limit".to_string());
    }
    serde_json::from_slice(&bytes).map_err(|_| "Doctor repair plan is invalid".to_string())
}

fn doctor_plan_snapshot_is_current(snapshot_revision: &str) -> bool {
    doctor_v3_snapshot_value().is_some_and(|snapshot| {
        doctor_v3_snapshot_is_fresh(&snapshot, epoch_ms())
            && snapshot.get("revision").and_then(Value::as_str) == Some(snapshot_revision)
    })
}

#[tauri::command]
fn preview_doctor_repair(request: PreviewDoctorModuleRepairRequest) -> Value {
    if !doctor_plan_snapshot_is_current(&request.snapshot_revision) {
        return json!({
            "ok": false,
            "schema_version": 3,
            "error": "Doctor snapshot is stale; scan again before preparing a repair",
        });
    }
    let Some(domain_module_id) = doctor_domain_module_id(&request.module_id) else {
        return json!({"ok": false, "schema_version": 3, "error": "Unknown Doctor repair module"});
    };
    if matches!(
        request.module_id.as_str(),
        "optical" | "optical_shadow_policy"
    ) {
        return json!({
            "ok": false,
            "schema_version": 3,
            "error": "Optical Shadow changes must be reviewed in Visual Compression",
            "destination": "visual",
        });
    }
    if matches!(
        request.module_id.as_str(),
        "platform" | "platform_autostart"
    ) {
        return json!({
            "ok": false,
            "schema_version": 3,
            "error": "Autostart intent must be selected in Install & Connect",
            "destination": "install",
        });
    }
    if matches!(
        request.module_id.as_str(),
        "recovery" | "recovery_rollback_evidence"
    ) {
        return json!({
            "ok": false,
            "schema_version": 3,
            "error": "Rollback must be previewed from Backup & Rollback",
            "destination": "changes",
        });
    }
    let created_at_ms = epoch_ms();
    let expires_at_ms = created_at_ms.saturating_add(DOCTOR_PLAN_TTL_MS);
    let plan_id = format!("doctor-plan-{}", Uuid::new_v4().simple());
    let domain_selection = match doctor_domain_selection(domain_module_id, &request.selection) {
        Ok(selection) => selection,
        Err(error) => return json!({"ok": false, "schema_version": 3, "error": error}),
    };
    let domain_plan = match DoctorDomainPlan::new(
        plan_id.clone(),
        request.snapshot_revision.clone(),
        domain_module_id,
        domain_selection,
        created_at_ms,
        expires_at_ms,
    ) {
        Ok(plan) => plan,
        Err(error) => return json!({"ok": false, "schema_version": 3, "error": error.to_string()}),
    };

    let mut agent_review_digests = HashMap::new();
    let mut agent_plans = Vec::new();
    if matches!(
        domain_module_id,
        DoctorModuleId::AgentInstructions
            | DoctorModuleId::AgentHookPlugin
            | DoctorModuleId::AgentSkill
            | DoctorModuleId::AgentMetadataMcp
    ) {
        if request.selection.agent_ids.is_empty() {
            return json!({"ok": false, "schema_version": 3, "error": "Select at least one Agent"});
        }
        for agent_id in &request.selection.agent_ids {
            if !matches!(agent_id.as_str(), "claude-code" | "codex" | "opencode") {
                agent_plans.push(json!({
                    "agent_id": agent_id,
                    "state": "blocked",
                    "summary": "This Agent has no verified d-token integration executor",
                    "targets": [],
                }));
                continue;
            }
            match doctor_agent_has_managed_evidence(agent_id, &request.selection) {
                Ok(true) => {}
                Ok(false) => {
                    agent_plans.push(json!({
                        "agent_id": agent_id,
                        "state": "blocked",
                        "summary": "No existing d-token managed integration evidence was found; use Install & Connect for first-time setup",
                        "targets": [],
                    }));
                    continue;
                }
                Err(error) => {
                    agent_plans.push(json!({
                        "agent_id": agent_id,
                        "state": "blocked",
                        "summary": sanitize_command_output(&error),
                        "targets": [],
                    }));
                    continue;
                }
            }
            match doctor_prepare_agent_integration(agent_id, &request.selection) {
                Ok(prepared) => {
                    agent_review_digests
                        .insert(agent_id.clone(), prepared.plan.review_digest.clone());
                    agent_plans.push(json!({
                        "agent_id": agent_id,
                        "state": if prepared.plan.targets.iter().all(|target| target.before_sha256 == target.after_sha256) { "no_change" } else { "review" },
                        "summary": "Existing d-token managed surfaces were inspected and bound to this review",
                        "review_digest": prepared.plan.review_digest,
                        "new_session_required": prepared.plan.new_session_required,
                        "targets": prepared.plan.targets,
                    }));
                }
                Err(error) => agent_plans.push(json!({
                    "agent_id": agent_id,
                    "state": "blocked",
                    "summary": sanitize_command_output(&error),
                    "targets": [],
                })),
            }
        }
        if agent_review_digests.is_empty() {
            return json!({
                "ok": false,
                "schema_version": 3,
                "error": "None of the selected Agents has an eligible existing d-token managed target",
                "agents": agent_plans,
            });
        }
    }

    let mut digest_entries = agent_review_digests.iter().collect::<Vec<_>>();
    digest_entries.sort_by(|left, right| left.0.cmp(right.0));
    let review_digest = format!(
        "doctor-review-{}",
        stable_hash(
            &json!({
                "domain": domain_plan.review_digest,
                "module_id": request.module_id,
                "selection": request.selection,
                "agents": digest_entries,
            })
            .to_string(),
        )
    );
    let stored = StoredDoctorModulePlan {
        schema_version: 3,
        plan_id: plan_id.clone(),
        snapshot_revision: request.snapshot_revision.clone(),
        module_id: request.module_id.clone(),
        selection: request.selection.clone(),
        created_at_ms,
        expires_at_ms,
        review_digest: review_digest.clone(),
        domain_plan,
        agent_review_digests,
    };
    if let Err(error) = store_doctor_plan(&stored) {
        return json!({"ok": false, "schema_version": 3, "error": sanitize_command_output(&error)});
    }
    json!({
        "ok": true,
        "schema_version": 3,
        "plan_id": plan_id,
        "snapshot_revision": request.snapshot_revision,
        "module_id": request.module_id,
        "selection": request.selection,
        "risk": if agent_plans.is_empty() { "low" } else { "medium" },
        "created_at_ms": created_at_ms,
        "expires_at_ms": expires_at_ms,
        "review_digest": review_digest,
        "summary": if agent_plans.is_empty() { "Restart or start only the identity-verified d-token-owned service, then rescan." } else { "Apply each selected Agent as an independent reviewed transaction, continue after individual failures, then rescan." },
        "agents": agent_plans,
        "actions": if matches!(domain_module_id, DoctorModuleId::CoreServiceLifecycle) { json!([{ "action_id": "restart_managed_service", "summary": "Start or restart the verified d-token-owned service", "targets": [] }]) } else { json!([]) },
    })
}

#[tauri::command]
fn apply_doctor_repair(request: ApplyDoctorModuleRepairRequest) -> Value {
    if !request.confirmed {
        return json!({"ok": false, "schema_version": 3, "outcome": "blocked", "summary": "Explicit confirmation is required"});
    }
    let stored = match load_doctor_plan(&request.plan_id) {
        Ok(plan) => plan,
        Err(error) => {
            return json!({"ok": false, "schema_version": 3, "outcome": "blocked", "summary": error})
        }
    };
    let now = epoch_ms();
    let matches_review = stored.schema_version == 3
        && stored.review_digest == request.review_digest
        && stored.snapshot_revision == request.snapshot_revision
        && stored.module_id == request.module_id
        && stored.selection == request.selection
        && now < stored.expires_at_ms
        && doctor_plan_snapshot_is_current(&stored.snapshot_revision)
        && stored
            .domain_plan
            .validate_at(now, &stored.snapshot_revision)
            .is_ok();
    if !matches_review {
        return json!({"ok": false, "schema_version": 3, "outcome": "blocked", "summary": "Doctor repair review is stale; scan and review again"});
    }
    let started_at_ms = now;
    let operation_id = format!("doctor-operation-{}", Uuid::new_v4().simple());
    let mut agent_receipts = Vec::new();
    let mut attempted = 0usize;
    let mut verified = 0usize;
    let mut core_ok = None;

    if stored.module_id == "core" || stored.module_id == "core_service_lifecycle" {
        let before = dtoken_service_status_value();
        let state = before
            .get("state")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        let action = if state == "running" {
            ActionResult {
                ok: true,
                summary: "d-token service is already running and identity-verified".to_string(),
                logs: Vec::new(),
                tools: all_checks(),
            }
        } else {
            restart_dtoken_service_impl()
        };
        core_ok = Some(action.ok);
    } else {
        for agent_id in &stored.selection.agent_ids {
            let Some(expected_digest) = stored.agent_review_digests.get(agent_id) else {
                agent_receipts.push(json!({
                    "agent_id": agent_id,
                    "outcome": "blocked",
                    "summary": "This Agent was blocked during preview and was not attempted",
                }));
                continue;
            };
            attempted += 1;
            if !doctor_agent_has_managed_evidence(agent_id, &stored.selection).unwrap_or(false) {
                agent_receipts.push(json!({
                    "agent_id": agent_id,
                    "outcome": "blocked",
                    "summary": "Existing d-token managed evidence changed after review",
                }));
                continue;
            }
            let prepared = match doctor_prepare_agent_integration(agent_id, &stored.selection) {
                Ok(prepared) if &prepared.plan.review_digest == expected_digest => prepared,
                Ok(_) => {
                    agent_receipts.push(json!({
                        "agent_id": agent_id,
                        "outcome": "blocked",
                        "summary": "Agent files changed after review; this Agent was skipped",
                    }));
                    continue;
                }
                Err(error) => {
                    agent_receipts.push(json!({
                        "agent_id": agent_id,
                        "outcome": "failed",
                        "summary": sanitize_command_output(&error),
                    }));
                    continue;
                }
            };
            match apply_agent_integration(prepared, &config_backup_root(), expected_digest) {
                Ok(receipt) => {
                    let verification = verify_agent_integration_transaction(
                        &receipt.transaction_id,
                        &config_backup_root(),
                    );
                    let ok = verification.as_ref().is_ok_and(|result| result.ok);
                    if ok {
                        verified += 1;
                    }
                    agent_receipts.push(json!({
                        "agent_id": agent_id,
                        "outcome": if ok { "verified" } else { "partial" },
                        "summary": if ok { "Agent transaction applied and target hashes verified" } else { "Agent files were written, but post-verification is incomplete" },
                        "transaction_id": receipt.transaction_id,
                        "rollback_ids": receipt.rollback_ids,
                        "new_session_required": receipt.new_session_required,
                        "trust_required": receipt.trust_required,
                    }));
                }
                Err(error) => agent_receipts.push(json!({
                    "agent_id": agent_id,
                    "outcome": "failed",
                    "summary": sanitize_command_output(&format!("{error:#}")),
                })),
            }
        }
    }
    let _ = durable_remove_file(&doctor_plan_path(&stored.plan_id).unwrap_or_default());
    let post_snapshot = run_doctor_scan_value();
    let post_revision = post_snapshot
        .get("revision")
        .and_then(Value::as_str)
        .map(str::to_string);
    let outcome = if let Some(ok) = core_ok {
        if ok {
            "verified"
        } else {
            "failed"
        }
    } else if attempted > 0 && verified == attempted {
        "verified"
    } else if verified > 0 {
        "partial"
    } else {
        "failed"
    };
    let domain_outcome = if outcome == "failed" {
        DoctorRepairOutcome::Failed
    } else {
        DoctorRepairOutcome::Applied
    };
    let domain_verification = match outcome {
        "verified" => DoctorVerificationState::Verified,
        "partial" => DoctorVerificationState::Partial,
        _ => DoctorVerificationState::Failed,
    };
    let completed_at_ms = epoch_ms();
    let domain_receipt = DoctorDomainReceipt::from_plan(
        &stored.domain_plan,
        operation_id.clone(),
        domain_outcome,
        domain_verification,
        post_revision.clone(),
        started_at_ms,
        completed_at_ms,
    )
    .ok();
    let receipt = json!({
        "ok": outcome != "failed",
        "schema_version": 3,
        "operation_id": operation_id,
        "plan_id": stored.plan_id,
        "plan_digest": stored.review_digest,
        "snapshot_revision": stored.snapshot_revision,
        "module_id": stored.module_id,
        "outcome": outcome,
        "verification": outcome,
        "started_at_ms": started_at_ms,
        "completed_at_ms": completed_at_ms,
        "post_scan_revision": post_revision,
        "agents": agent_receipts,
        "snapshot": post_snapshot,
        "contract": domain_receipt,
    });
    let receipt_name = format!(
        "{}.json",
        receipt
            .get("operation_id")
            .and_then(Value::as_str)
            .unwrap_or("doctor-operation")
    );
    let _ = write_json_app_state(&["doctor", "receipts", &receipt_name], &receipt);
    let _ = append_desktop_event(
        "doctor_repair_v3",
        if outcome == "verified" { "ok" } else { outcome },
        json!({
            "operation_id": receipt.get("operation_id"),
            "module_id": receipt.get("module_id"),
            "outcome": outcome,
            "post_scan_revision": receipt.get("post_scan_revision"),
        }),
    );
    receipt
}

#[tauri::command]
fn verify_doctor_repair(request: VerifyDoctorRepairRequest) -> Value {
    let snapshot = run_doctor_scan_value();
    json!({
        "ok": doctor_v3_snapshot_is_fresh(&snapshot, epoch_ms()),
        "schema_version": 3,
        "operation_id": request.operation_id,
        "snapshot": snapshot,
    })
}

fn doctor_agent_ready(tools: &[ToolCheck]) -> bool {
    tools
        .iter()
        .any(|tool| tool.id == "opencode" && tool.installed)
}

#[tauri::command]
fn manage_headroom(action: String, port: u16) -> ActionResult {
    let mut logs = vec![log(
        "info",
        format!("External Headroom reference action requested: {action} on 127.0.0.1:{port}"),
    )];
    if !(port == EXTERNAL_HEADROOM_REFERENCE_PORT || port == LEGACY_OPENCODE_REFERENCE_PORT) {
        logs.push(log(
            "error",
            "Only external reference ports 8787 and 8788 are recognized for read-only health checks",
        ));
        return ActionResult {
            ok: false,
            summary: "Unsupported external reference port".to_string(),
            logs,
            tools: all_checks(),
        };
    }

    match action.as_str() {
        "health" => {
            let endpoint = check_headroom_endpoint(port, headroom_label(port));
            logs.push(log(
                "info",
                "No external process was started, stopped, restarted, or reconfigured",
            ));
            logs.push(log(
                if endpoint.healthy { "info" } else { "warn" },
                endpoint.detail,
            ));
            ActionResult {
                ok: endpoint.healthy,
                summary: format!("External Headroom reference health check complete for {port}"),
                logs,
                tools: all_checks(),
            }
        }
        "start" | "stop" | "restart" => {
            logs.push(log(
                "error",
                "Blocked by PRD: external Headroom is read-only reference only; d-token-owned runtime must own lifecycle",
            ));
            logs.push(log(
                "info",
                format!("Use d-token local service on 127.0.0.1:{DTOKEN_SERVICE_PORT} for managed runtime work"),
            ));
            ActionResult {
                ok: false,
                summary: "External Headroom lifecycle actions are disabled".to_string(),
                logs,
                tools: all_checks(),
            }
        }
        _ => {
            logs.push(log(
                "error",
                format!("Unknown external reference action: {action}"),
            ));
            ActionResult {
                ok: false,
                summary: format!("Unknown external reference action: {action}"),
                logs,
                tools: all_checks(),
            }
        }
    }
}

#[cfg(debug_assertions)]
#[tauri::command]
fn test_opencode(model: String) -> ActionResult {
    let plan = opencode_command_plan(&model);
    let mut logs = vec![
        log("info", format!("Testing OpenCode model: {model}")),
        log(
            "info",
            format!("Command: {} {}", plan.program, plan.args.join(" ")),
        ),
        log(
            "info",
            format!(
                "scope={} · excluded={}",
                plan.envs
                    .get("DTOKEN_TEST_SCOPE")
                    .map(String::as_str)
                    .unwrap_or("opencode-only"),
                plan.envs
                    .get("DTOKEN_TEST_EXCLUDE")
                    .map(String::as_str)
                    .unwrap_or("")
            ),
        ),
        log(
            "info",
            "direct-model-connectivity=true · routed_dtoken_18001=false · not final route evidence",
        ),
    ];
    match run_opencode_model_test(&model) {
        Ok(outcome) => {
            logs.push(log(
                "info",
                format!("run_id={} category={}", outcome.run_id, outcome.category),
            ));
            logs.push(log(
                "info",
                "No Claude Code, Cursor, Codex CLI, OpenCode config, or system-wide proxy configuration was changed",
            ));
            if outcome.output_summary.is_empty() {
                logs.push(log("warn", "OpenCode returned no visible output"));
            } else {
                logs.push(log(
                    if outcome.ok { "info" } else { "warn" },
                    format!("Output summary: {}", outcome.output_summary),
                ));
            }
            let summary = if outcome.ok {
                format!("OpenCode model {model} responded with d-token-ok")
            } else {
                format!("OpenCode model {model} result: {}", outcome.category)
            };
            ActionResult {
                ok: outcome.ok,
                summary,
                logs,
                tools: all_checks(),
            }
        }
        Err(err) => {
            let summary = sanitize_command_output(&err);
            logs.push(log(
                "error",
                format!("OpenCode command could not start: {summary}"),
            ));
            ActionResult {
                ok: false,
                summary: "OpenCode command could not start".to_string(),
                logs,
                tools: all_checks(),
            }
        }
    }
}

#[cfg(debug_assertions)]
#[tauri::command]
fn test_opencode_models(models: Vec<String>) -> Value {
    let mut results = Vec::new();
    for model in models {
        let action = test_opencode(model.clone());
        results.push(json!({
            "model": model,
            "ok": action.ok,
            "summary": action.summary,
            "logs": action.logs,
        }));
    }
    json!({
        "mode": "opencode-test",
        "route": "direct-model-connectivity",
        "routedDtoken18001": false,
        "fallback": "disabled",
        "results": results,
    })
}

#[cfg(debug_assertions)]
fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .map(PathBuf::from)
        .unwrap_or_else(|| env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
}

#[cfg(debug_assertions)]
#[tauri::command]
fn run_opencode_workload_benchmark() -> Result<Value, String> {
    let root = workspace_root();
    let script = root
        .join("support")
        .join("scripts")
        .join("run_opencode_workload_benchmark.ps1");
    if !script.exists() {
        return Err(format!(
            "OpenCode workload script not found: {}",
            script.display()
        ));
    }

    let output = Command::new("powershell")
        .arg("-NoProfile")
        .arg("-ExecutionPolicy")
        .arg("Bypass")
        .arg("-File")
        .arg(&script)
        .current_dir(&root)
        .output()
        .map_err(|err| format!("failed to launch OpenCode workload benchmark: {err}"))?;

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = sanitize_command_output(&String::from_utf8_lossy(&output.stderr));
    if !output.status.success() {
        return Ok(json!({
            "ok": false,
            "summary": "OpenCode workload benchmark failed",
            "exit_code": output.status.code(),
            "stderr": stderr,
            "stdout": sanitize_command_output(&stdout),
            "tools": all_checks(),
        }));
    }

    let mut parsed: Value = serde_json::from_str(&stdout)
        .map_err(|err| format!("OpenCode workload benchmark returned invalid JSON: {err}"))?;
    if let Some(object) = parsed.as_object_mut() {
        object.insert("ok".to_string(), json!(true));
        object.insert(
            "summary".to_string(),
            json!(format!(
                "OpenCode real workload routed: {} workloads, {} saved tokens, {}% savings",
                object
                    .get("workload_count")
                    .and_then(Value::as_i64)
                    .unwrap_or_default(),
                object
                    .get("total_saved_tokens")
                    .and_then(Value::as_i64)
                    .unwrap_or_default(),
                object
                    .get("savings_percent")
                    .and_then(Value::as_f64)
                    .unwrap_or_default()
            )),
        );
        object.insert("tools".to_string(), json!(all_checks()));
        object.insert(
            "privacy".to_string(),
            json!("Uses user's existing OpenCode config; secrets are not read, printed, persisted, or committed."),
        );
    }
    Ok(parsed)
}

#[cfg(debug_assertions)]
#[tauri::command]
fn run_headroom_capability_smoke() -> Value {
    let install = run_install_transaction(InstallPlanRequest {
        target: "dtoken".to_string(),
        install_root: "%LOCALAPPDATA%\\d-token".to_string(),
        proxy_port: DTOKEN_SERVICE_PORT,
        auto_backup: true,
        run_doctor_after: true,
        rollback_point: true,
        mode: "connect".to_string(),
    });
    let detect = detect_tools();
    let doctor = run_doctor();
    let policy = recommend_project_policy(ProjectPolicyRecommendRequest {
        project_id: Some("desktop-auto-scan".to_string()),
        language_stack: Some(vec!["rust".to_string(), "typescript".to_string()]),
        avg_request_tokens_7d: Some(49_224),
        avg_cli_noise_ratio_7d: Some(0.42),
        retrieve_rate_7d: Some(0.02),
        failure_rate_7d: Some(0.0),
        high_risk_task: Some(false),
        manual_override: Some(false),
    });
    let settings = save_app_settings(
        "project-policy".to_string(),
        json!({
            "source": "headroom-capability-smoke",
            "project_id": "desktop-auto-scan",
            "recommended_level": policy["recommendation"]["recommended_level"].clone(),
            "enable_ccr": true,
            "enable_memory": true,
            "enable_learn": true,
            "enable_output_shaper": true,
            "privacy": "local d-token setting only; no raw prompt/output"
        }),
    );
    let dashboard = match service_json(DTOKEN_SERVICE_PORT, "/dashboard/payload") {
        Ok(payload) => json!({
            "ok": true,
            "source": payload.get("source").cloned().unwrap_or(Value::Null),
            "runtime": payload.get("runtime").cloned().unwrap_or(Value::Null),
            "stats": payload.get("stats").cloned().unwrap_or(Value::Null),
            "metrics": payload.get("metrics").cloned().unwrap_or(Value::Null),
            "sections": payload.get("dashboard_sections").cloned().unwrap_or(Value::Null),
        }),
        Err(err) => json!({
            "ok": false,
            "error": sanitize_command_output(&err),
            "source": "dtoken-local-service-unavailable"
        }),
    };
    let export = match service_json(DTOKEN_SERVICE_PORT, "/export/json") {
        Ok(payload) => json!({
            "ok": true,
            "redacted": payload.get("redacted").and_then(Value::as_bool).unwrap_or(false),
            "raw_available": payload.get("raw_available").and_then(Value::as_bool).unwrap_or(true),
            "active_config_source": payload.pointer("/active_config/config/source").cloned().unwrap_or(Value::Null),
        }),
        Err(err) => json!({
            "ok": false,
            "error": sanitize_command_output(&err),
        }),
    };
    let hook_effective = active_route_config_status();
    let checks = vec![
        json!({
            "id": "install_transaction",
            "label": "d-token install transaction",
            "ok": install.ok,
            "summary": install.summary,
        }),
        json!({
            "id": "tool_and_hook_scan",
            "label": "OpenCode / RTK / Headroom source scan",
            "ok": detect.ok,
            "summary": detect.summary,
            "tools": detect.tools,
        }),
        json!({
            "id": "doctor",
            "label": "Desktop Doctor",
            "ok": doctor.ok,
            "summary": doctor.summary,
        }),
        json!({
            "id": "auto_policy",
            "label": "Auto scan compression level",
            "ok": policy["recommendation"]["recommended_level"].is_string(),
            "summary": policy["recommendation"]["recommended_level"].clone(),
            "detail": policy,
        }),
        json!({
            "id": "config_write",
            "label": "Policy settings write",
            "ok": settings.ok,
            "summary": settings.summary,
        }),
        json!({
            "id": "runtime_data",
            "label": "d-token runtime dashboard payload",
            "ok": dashboard["ok"].as_bool().unwrap_or(false),
            "summary": dashboard["source"].clone(),
            "detail": dashboard,
        }),
        json!({
            "id": "export_privacy",
            "label": "Redacted runtime export",
            "ok": export["ok"].as_bool().unwrap_or(false)
                && export["redacted"].as_bool().unwrap_or(false)
                && !export["raw_available"].as_bool().unwrap_or(true),
            "summary": export.clone(),
        }),
        json!({
            "id": "hook_effective",
            "label": "d-token active config / hook effectiveness",
            "ok": hook_effective.pointer("/config/loaded").and_then(Value::as_bool).unwrap_or(false),
            "summary": hook_effective.pointer("/config/source").cloned().unwrap_or(Value::Null),
            "detail": hook_effective,
        }),
    ];
    let passed = checks
        .iter()
        .filter(|check| check.get("ok").and_then(Value::as_bool).unwrap_or(false))
        .count();
    json!({
        "ok": passed == checks.len(),
        "passed": passed,
        "total": checks.len(),
        "summary": format!("Headroom-derived capability smoke: {passed}/{} checks passed", checks.len()),
        "checks": checks,
        "logs": install.logs,
        "privacy": "No external Headroom/RTK/cc-switch service is started, stopped, reconfigured, or used as product runtime."
    })
}

#[tauri::command]
fn run_product_readiness_check() -> Value {
    let install = preview_install_plan(InstallPlanRequest {
        target: "dtoken".to_string(),
        install_root: app_state_dir().display().to_string(),
        proxy_port: DTOKEN_SERVICE_PORT,
        auto_backup: false,
        run_doctor_after: false,
        rollback_point: false,
        mode: "manual".to_string(),
    });
    let detect = detect_tools();
    let doctor = run_doctor();
    let route = verify_agent_route_config();
    let local_state = match (route_profile_drafts_result(), rollback_points_result()) {
        (Ok(drafts), Ok(points)) => json!({
            "ok": true,
            "source": "dtoken-desktop-control-plane",
            "route_draft_count": drafts.len(),
            "rollback_point_count": points.len(),
            "runtime_required": false,
        }),
        (drafts, points) => json!({
            "ok": false,
            "source": "dtoken-desktop-control-plane",
            "route_state_error": drafts.err(),
            "rollback_state_error": points.err(),
            "runtime_required": false,
        }),
    };
    let active = active_route_config_status();
    let dashboard = runtime_probe("/dashboard/payload");
    let export = runtime_probe("/export/json");
    let capability = headroom_derived_runtime_capability_status(&dashboard);

    let compression_ok = dashboard
        .pointer("/payload/source")
        .and_then(Value::as_str)
        .map(|source| source == "dtoken-owned-runtime")
        .unwrap_or(false);
    let export_redacted = export
        .pointer("/payload/redacted")
        .and_then(Value::as_bool)
        .unwrap_or(false)
        && !export
            .pointer("/payload/raw_available")
            .and_then(Value::as_bool)
            .unwrap_or(true);
    let route_ok = route.get("ok").and_then(Value::as_bool).unwrap_or(false);
    let capability_ok = capability
        .get("ok")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let active_ok = active.get("state").and_then(Value::as_str) == Some("loaded");
    let detected_tools = detect.tools.iter().filter(|tool| tool.installed).count();
    let install_ok = install
        .get("targets")
        .and_then(Value::as_array)
        .is_some_and(|targets| !targets.is_empty())
        && install.get("writesEnabled").and_then(Value::as_bool) == Some(false);

    let checks = vec![
        json!({
            "id": "install_connect",
            "label": "Install / connect plan",
            "ok": install_ok,
            "summary": if install_ok { "Read-only install/connect plan is available" } else { "Install/connect plan is invalid" },
            "detail": install,
        }),
        json!({
            "id": "agent_detection",
            "label": "Agent and tool discovery",
            "ok": doctor_agent_ready(&detect.tools),
            "summary": format!("{detected_tools}/{} known local tools available", detect.tools.len()),
            "detail": { "tools": detect.tools },
        }),
        json!({
            "id": "route_config",
            "label": "Agent route configuration",
            "ok": route_ok,
            "summary": route.get("summary").cloned().unwrap_or(Value::Null),
            "detail": route,
        }),
        json!({
            "id": "compression_data",
            "label": "Compression and dashboard data",
            "ok": compression_ok,
            "summary": dashboard.pointer("/payload/source").cloned().unwrap_or(Value::Null),
            "detail": dashboard,
        }),
        json!({
            "id": "hook_or_config_effect",
            "label": "Hook / config effect",
            "ok": active_ok,
            "summary": active.get("state").cloned().unwrap_or(Value::Null),
            "detail": active,
        }),
        json!({
            "id": "local_transaction_state",
            "label": "Local transaction state",
            "ok": local_state.get("ok").and_then(Value::as_bool).unwrap_or(false),
            "summary": if local_state.get("ok").and_then(Value::as_bool).unwrap_or(false) { "Desktop-owned drafts and rollback state are readable" } else { "Desktop-owned transaction state is unreadable" },
            "detail": local_state,
        }),
        json!({
            "id": "headroom_derived_capabilities",
            "label": "Headroom-derived capabilities",
            "ok": capability_ok,
            "summary": capability.get("summary").cloned().unwrap_or(Value::Null),
            "detail": capability,
        }),
        json!({
            "id": "export_redaction",
            "label": "Logs and export redaction",
            "ok": export_redacted,
            "summary": export.pointer("/payload/privacy").cloned().unwrap_or_else(|| json!("redacted runtime export")),
            "detail": export,
        }),
        json!({
            "id": "doctor",
            "label": "Doctor diagnostics",
            "ok": doctor.ok,
            "summary": doctor.summary,
            "detail": doctor,
        }),
    ];
    let passed = checks
        .iter()
        .filter(|check| check.get("ok").and_then(Value::as_bool).unwrap_or(false))
        .count();

    json!({
        "ok": passed == checks.len(),
        "mode": "product-readiness-check",
        "passed": passed,
        "total": checks.len(),
        "summary": format!("Product readiness check: {passed}/{} checks passed", checks.len()),
        "checks": checks,
        "privacy": "Product action only: install/connect/verify/diagnose/status/log/export evidence. No development validation surface and no raw prompt/response/secrets are returned.",
        "external_services": "Does not start, stop, reconfigure, or depend on external Headroom, RTK, or cc-switch services.",
    })
}

fn headroom_derived_runtime_capability_status(dashboard: &Value) -> Value {
    let payload = dashboard.get("payload").unwrap_or(&Value::Null);
    let source_ok = payload
        .get("source")
        .and_then(Value::as_str)
        .map(|source| source == "dtoken-owned-runtime")
        .unwrap_or(false);
    let compression = payload.get("stats").is_some() || payload.get("metrics").is_some();
    let ccr_raw_blocked = payload
        .pointer("/ccr/raw_available")
        .or_else(|| payload.pointer("/cache_summary/raw_available"))
        .and_then(Value::as_bool)
        .map(|raw| !raw)
        .unwrap_or(false);
    let memory_raw_blocked = payload
        .pointer("/memory/raw_available")
        .and_then(Value::as_bool)
        .map(|raw| !raw)
        .unwrap_or(false);
    let output_shape_raw_blocked = payload
        .pointer("/output_shape/raw_available")
        .and_then(Value::as_bool)
        .map(|raw| !raw)
        .unwrap_or(false)
        || payload
            .get("dashboard_sections")
            .and_then(Value::as_array)
            .is_some_and(|sections| {
                sections
                    .iter()
                    .any(|section| section.as_str() == Some("cli_filtering"))
            });
    let route_adapter = payload.get("route").is_some()
        || payload.get("route_decisions").is_some()
        || payload.pointer("/stats/route_overhead").is_some();

    let checks = vec![
        json!({
            "id": "runtime_source",
            "label": "d-token owned runtime",
            "ok": source_ok,
        }),
        json!({
            "id": "compression_pipeline",
            "label": "Compression / metrics",
            "ok": source_ok && compression,
        }),
        json!({
            "id": "ccr_metadata",
            "label": "CCR metadata boundary",
            "ok": source_ok && ccr_raw_blocked,
        }),
        json!({
            "id": "memory_learn",
            "label": "Memory / learn boundary",
            "ok": source_ok && memory_raw_blocked,
        }),
        json!({
            "id": "output_shaping",
            "label": "Output shaping boundary",
            "ok": source_ok && output_shape_raw_blocked,
        }),
        json!({
            "id": "route_adapter",
            "label": "Route adapter visibility",
            "ok": source_ok && route_adapter,
        }),
    ];
    let passed = checks
        .iter()
        .filter(|check| check.get("ok").and_then(Value::as_bool).unwrap_or(false))
        .count();

    json!({
        "ok": passed == checks.len(),
        "passed": passed,
        "total": checks.len(),
        "summary": format!("Headroom-derived d-token runtime capabilities: {passed}/{} checks passed", checks.len()),
        "checks": checks,
        "privacy": "Uses d-token runtime metadata only; no external Headroom service, raw prompt, raw response, or secrets are required.",
    })
}

#[tauri::command]
fn get_active_route_config_status() -> Value {
    active_route_config_status()
}

fn runtime_probe(path: &str) -> Value {
    match service_json(DTOKEN_SERVICE_PORT, path) {
        Ok(payload) => json!({
            "ok": true,
            "path": path,
            "payload": payload,
        }),
        Err(err) => json!({
            "ok": false,
            "path": path,
            "error": sanitize_command_output(&err),
        }),
    }
}

#[tauri::command]
fn verify_agent_route_config() -> Value {
    let active = active_route_config_status();
    let active_loaded = active.get("state").and_then(Value::as_str) == Some("loaded");
    let fallback_disabled = active
        .get("fallback_disabled")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let active_route = active.pointer("/route/id").cloned().unwrap_or(Value::Null);
    let active_provider = active
        .pointer("/provider/id")
        .cloned()
        .unwrap_or(Value::Null);

    let health = runtime_probe("/health");
    let health_ok = health
        .pointer("/payload/status")
        .and_then(Value::as_str)
        .map(|status| status == "healthy" || status == "ready")
        .unwrap_or_else(|| health.get("ok").and_then(Value::as_bool).unwrap_or(false));

    let runtime_active = runtime_probe("/config/active");
    let runtime_loaded = runtime_active
        .pointer("/payload/config/loaded")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let runtime_route_count = runtime_active
        .pointer("/payload/config/route_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);

    let dashboard = runtime_probe("/dashboard/payload");
    let dashboard_source = dashboard
        .pointer("/payload/source")
        .and_then(Value::as_str)
        .unwrap_or("unavailable");
    let dashboard_ok = dashboard_source == "dtoken-owned-runtime";
    let request_count = dashboard
        .pointer("/payload/stats/requests/total")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let route_decisions = dashboard
        .pointer("/payload/route_decisions")
        .and_then(Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);

    let export = runtime_probe("/export/json");
    let export_ok = export.get("ok").and_then(Value::as_bool).unwrap_or(false);
    let export_redacted = export
        .pointer("/payload/redacted")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let export_raw_available = export
        .pointer("/payload/raw_available")
        .and_then(Value::as_bool)
        .unwrap_or(true);

    let checks = vec![
        json!({
            "id": "active_config",
            "label": "Active d-token config",
            "ok": active_loaded,
            "summary": if active_loaded { "d-token active route config is loaded" } else { "No applied d-token active route config yet" },
            "detail": active,
        }),
        json!({
            "id": "fallback",
            "label": "Fallback boundary",
            "ok": fallback_disabled,
            "summary": if fallback_disabled { "Silent fallback is disabled" } else { "Fallback has not been proven disabled" },
            "detail": { "route": active_route, "provider": active_provider },
        }),
        json!({
            "id": "runtime_health",
            "label": "d-token local service",
            "ok": health_ok,
            "summary": if health_ok { "127.0.0.1:18001 responded healthy" } else { "d-token local service is not responding as healthy" },
            "detail": health,
        }),
        json!({
            "id": "runtime_active_config",
            "label": "Runtime route loader",
            "ok": runtime_loaded && runtime_route_count > 0,
            "summary": if runtime_loaded && runtime_route_count > 0 { "Runtime loaded the active route config" } else { "Runtime has not loaded the active route config yet" },
            "detail": runtime_active,
        }),
        json!({
            "id": "dashboard",
            "label": "Dashboard data source",
            "ok": dashboard_ok,
            "summary": format!("{dashboard_source} · {request_count} requests · {route_decisions} route decisions"),
            "detail": dashboard,
        }),
        json!({
            "id": "export_redaction",
            "label": "Redacted export",
            "ok": export_ok && export_redacted && !export_raw_available,
            "summary": if export_ok && export_redacted && !export_raw_available { "Runtime export is redacted" } else { "Runtime export redaction is not proven yet" },
            "detail": export,
        }),
    ];
    let passed = checks
        .iter()
        .filter(|check| check.get("ok").and_then(Value::as_bool).unwrap_or(false))
        .count();
    let ok = passed == checks.len();
    json!({
        "ok": ok,
        "mode": "agent-route-verification",
        "passed": passed,
        "total": checks.len(),
        "summary": if ok {
            "Agent route configuration is active and observable through d-token runtime"
        } else {
            "Agent route configuration still needs attention"
        },
        "checks": checks,
        "route": active_route,
        "provider": active_provider,
        "service_url": format!("http://127.0.0.1:{DTOKEN_SERVICE_PORT}"),
        "privacy": "Read-only d-token local verification; no external Headroom/RTK/cc-switch service is touched and no secrets are read."
    })
}

#[tauri::command]
async fn get_dtoken_dashboard_payload() -> Value {
    match service_json(DTOKEN_SERVICE_PORT, "/dashboard/payload") {
        Ok(mut payload) => {
            if let Some(object) = payload.as_object_mut() {
                object.insert(
                    "desktop_data_source".to_string(),
                    json!("dtoken-local-service"),
                );
                object.insert(
                    "desktop_service_url".to_string(),
                    json!(format!("http://127.0.0.1:{DTOKEN_SERVICE_PORT}")),
                );
                object.insert(
                    "desktop_active_config".to_string(),
                    active_route_config_status(),
                );
                if let Ok(active) = service_json(DTOKEN_SERVICE_PORT, "/config/active") {
                    object.insert("runtime_active_config".to_string(), active);
                }
            }
            payload
        }
        Err(err) => {
            json!({
                "source": "unavailable",
                "desktop_data_source": "unavailable",
                "evidence_state": "unavailable",
                "freshness": "unavailable",
                "complete": false,
                "partial": true,
                "desktop_service_error": err,
                "desktop_service_url": format!("http://127.0.0.1:{DTOKEN_SERVICE_PORT}"),
                "desktop_active_config": active_route_config_status(),
                "providers": [],
                "routes": [],
                "instances": [],
                "request_logs": [],
                "logs": [],
                "recentEvents": []
            })
        }
    }
}

#[tauri::command]
async fn export_dtoken_runtime(format: String) -> Result<Value, String> {
    let normalized = format.trim().to_ascii_lowercase();
    let exported_at = now_label();
    match normalized.as_str() {
        "json" => {
            let payload = service_json(DTOKEN_SERVICE_PORT, "/export/json")?;
            let content = serde_json::to_string_pretty(&payload).map_err(|err| err.to_string())?;
            Ok(json!({
                "format": "json",
                "mime": "application/json",
                "filename": format!("dtoken-runtime-redacted-{exported_at}.json").replace(':', "-"),
                "content": content,
                "source": "dtoken-local-service",
                "service_url": format!("http://127.0.0.1:{DTOKEN_SERVICE_PORT}/export/json"),
                "redacted": payload.get("redacted").and_then(Value::as_bool).unwrap_or(true)
            }))
        }
        "csv" => {
            let content = service_text(DTOKEN_SERVICE_PORT, "/export/csv")?;
            Ok(json!({
                "format": "csv",
                "mime": "text/csv",
                "filename": format!("dtoken-runtime-redacted-{exported_at}.csv").replace(':', "-"),
                "content": content,
                "source": "dtoken-local-service",
                "service_url": format!("http://127.0.0.1:{DTOKEN_SERVICE_PORT}/export/csv"),
                "redacted": true
            }))
        }
        _ => Err("Unsupported export format; use json or csv".to_string()),
    }
}

fn offline_control_plane_data(
    reason: &str,
    route_drafts: Vec<Value>,
    backup_points: Vec<Value>,
) -> Value {
    let changes = changes_from_route_drafts(&route_drafts);
    if let Some(mut cached) = load_stale_runtime_snapshot(
        LAST_VERIFIED_DESKTOP_DETAIL,
        "detail",
        reason,
        "get_desktop_data",
    ) {
        let desktop_events = desktop_transaction_events();
        if let Some(object) = cached.as_object_mut() {
            let mut recent_events = object
                .get("recentEvents")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            recent_events.extend(desktop_events.iter().cloned());
            object.insert("active_config".to_string(), active_route_config_status());
            object.insert("service".to_string(), dtoken_service_status_value());
            object.insert("tools".to_string(), json!(all_checks()));
            object.insert("recentEvents".to_string(), json!(recent_events));
            object.insert(
                "desktopTransactionEvents".to_string(),
                json!(desktop_events),
            );
            object.insert("changes".to_string(), json!(changes));
            object.insert("routeDrafts".to_string(), json!(route_drafts));
            object.insert("backupPoints".to_string(), json!(backup_points));
        }
        return cached;
    }
    json!({
        "source": "dtoken-desktop-control-plane",
        "desktop_data_source": "dtoken-desktop-control-plane",
        "evidence_state": "partial",
        "freshness": "local",
        "complete": false,
        "partial": true,
        "runtime_available": false,
        "runtime_evidence_state": "unavailable",
        "error": reason,
        "transaction_provenance": {
            "source": "dtoken-desktop-control-plane",
            "command": "get_desktop_data",
            "schema_version": 1,
            "generated_at": now_timestamp(),
            "state_root": app_state_dir().display().to_string(),
            "runtime_required": false
        },
        "active_config": active_route_config_status(),
        "service": dtoken_service_status_value(),
        "tools": all_checks(),
        "health": [],
        "doctorChecks": [],
        "agents": [],
        "projects": [],
        "savings": [],
        "rtkTrend": [],
        "headroomTrend": [],
        "commandFamilies": [],
        "recentDecisions": [],
        "recentEvents": desktop_transaction_events(),
        "desktopTransactionEvents": desktop_transaction_events(),
        "routes": [],
        "providers": [],
        "instances": [],
        "request_logs": [],
        "logs": [],
        "changes": changes,
        "routeDrafts": route_drafts,
        "backupPoints": backup_points,
        "runtimeCompatibility": {
            "source": "unavailable",
            "agents": [],
            "protocolInjection": [],
            "chainStatus": null
        }
    })
}

fn offline_desktop_summary(reason: &str) -> Value {
    if let Some(mut cached) = load_stale_runtime_snapshot(
        LAST_VERIFIED_DESKTOP_SUMMARY,
        "summary",
        reason,
        "get_desktop_summary",
    ) {
        if let Some(object) = cached.as_object_mut() {
            object.insert("active_config".to_string(), active_route_config_status());
        }
        return cached;
    }
    json!({
        "source": "dtoken-desktop-control-plane",
        "desktop_data_source": "dtoken-desktop-control-plane",
        "desktop_payload_kind": "summary",
        "evidence_state": "partial",
        "freshness": "local",
        "complete": false,
        "partial": true,
        "runtime_available": false,
        "runtime_evidence_state": "unavailable",
        "error": reason,
        "stats": {},
        "history": {},
        "metrics": {},
        "providers": [],
        "routes": [],
        "instances": [],
        "route_decisions": [],
        "request_logs": [],
        "active_config": active_route_config_status(),
        "transaction_provenance": {
            "source": "dtoken-desktop-control-plane",
            "command": "get_desktop_summary",
            "schema_version": 1,
            "generated_at": now_timestamp(),
            "runtime_required": false
        }
    })
}

fn changes_from_route_drafts(route_drafts: &[Value]) -> Vec<Value> {
    route_drafts
        .iter()
        .filter(|draft| draft.pointer("/resolution/state").and_then(Value::as_str) != Some("applied"))
        .filter_map(|draft| {
            let route = draft
                .get("config")?
                .get("routes")?
                .as_array()?
                .first()?;
            let route_id = route.get("id")?.as_str()?;
            Some(json!({
                "id": format!("draft-{route_id}"),
                "summary": format!("Apply d-token route draft {route_id}"),
                "target": draft.get("path").cloned().unwrap_or(Value::Null),
                "risk": "medium",
                "backup": "created durably before active config replacement",
                "diff": draft.get("diff").cloned().unwrap_or_else(|| json!([])),
                "postChecks": ["d-token active config", "local service health", "agent route verification"]
            }))
        })
        .collect()
}

fn complete_desktop_data(
    mut runtime: Value,
    route_drafts: Vec<Value>,
    backup_points: Vec<Value>,
    compatibility: Option<Value>,
    protocol_injection: Option<Value>,
    chain_status: Option<Value>,
) -> Value {
    let changes = changes_from_route_drafts(&route_drafts);
    let object = runtime.as_object_mut().expect("validated runtime object");
    object.insert("source".to_string(), json!("dtoken-owned-runtime"));
    object.insert(
        "desktop_data_source".to_string(),
        json!("dtoken-local-service"),
    );
    object.insert("evidence_state".to_string(), json!("verified"));
    object.insert("desktop_payload_kind".to_string(), json!("detail"));
    object.insert("freshness".to_string(), json!("fresh"));
    object.insert("complete".to_string(), json!(true));
    object.insert("partial".to_string(), json!(false));
    object.insert("runtime_available".to_string(), json!(true));
    object.insert("runtime_evidence_state".to_string(), json!("verified"));
    let desktop_events = desktop_transaction_events();
    let mut recent_events = object
        .get("recentEvents")
        .or_else(|| object.get("events"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    recent_events.extend(desktop_events.iter().cloned());
    object.insert("recentEvents".to_string(), json!(recent_events));
    object.insert(
        "desktopTransactionEvents".to_string(),
        json!(desktop_events),
    );
    object.insert(
        "transaction_provenance".to_string(),
        json!({
            "source": "dtoken-owned-runtime",
            "command": "get_desktop_data",
            "schema_version": 1,
            "generated_at": now_timestamp(),
            "service_url": format!("http://127.0.0.1:{DTOKEN_SERVICE_PORT}"),
            "state_root": app_state_dir().display().to_string()
        }),
    );
    object.insert("routeDrafts".to_string(), json!(route_drafts));
    object.insert("backupPoints".to_string(), json!(backup_points));
    object.insert("changes".to_string(), json!(changes));
    object.insert(
        "runtimeCompatibility".to_string(),
        json!({
            "source": if compatibility.is_some() || protocol_injection.is_some() || chain_status.is_some() {
                "dtoken-local-service"
            } else {
                "unavailable"
            },
            "agents": compatibility.unwrap_or_else(|| json!([])),
            "protocolInjection": protocol_injection.unwrap_or_else(|| json!([])),
            "chainStatus": chain_status.unwrap_or(Value::Null),
            "privacy": "Compatibility evidence is read only from the d-token-owned local service."
        }),
    );
    runtime
}

fn is_verified_runtime_snapshot(payload: &Value, payload_kind: &str) -> bool {
    payload.get("source").and_then(Value::as_str) == Some("dtoken-owned-runtime")
        && payload.get("desktop_data_source").and_then(Value::as_str)
            == Some("dtoken-local-service")
        && payload.get("desktop_payload_kind").and_then(Value::as_str) == Some(payload_kind)
        && payload.get("evidence_state").and_then(Value::as_str) == Some("verified")
        && payload.get("runtime_available").and_then(Value::as_bool) == Some(true)
}

fn stale_runtime_snapshot(
    mut payload: Value,
    payload_kind: &str,
    reason: &str,
    command: &str,
) -> Option<Value> {
    if !is_verified_runtime_snapshot(&payload, payload_kind) {
        return None;
    }
    let cached_collected_at = payload
        .get("desktop_collected_at")
        .and_then(Value::as_str)
        .unwrap_or("unknown")
        .to_string();
    let object = payload.as_object_mut()?;
    object.insert("evidence_state".to_string(), json!("stale"));
    object.insert("freshness".to_string(), json!("stale"));
    object.insert("complete".to_string(), json!(false));
    object.insert("partial".to_string(), json!(true));
    object.insert("runtime_available".to_string(), json!(false));
    object.insert("runtime_evidence_state".to_string(), json!("unavailable"));
    object.insert("error".to_string(), json!(reason));
    object.insert("runtime_health".to_string(), Value::Null);
    object.insert(
        "cached_snapshot".to_string(),
        json!({
            "state": "stale",
            "source": "dtoken-owned-runtime",
            "collected_at": cached_collected_at,
            "privacy": "redacted runtime evidence only; no prompt, response, credential, or auth state"
        }),
    );
    object.insert(
        "transaction_provenance".to_string(),
        json!({
            "source": "dtoken-desktop-control-plane",
            "command": command,
            "schema_version": 1,
            "generated_at": now_timestamp(),
            "runtime_required": false,
            "cached_runtime_source": "dtoken-owned-runtime"
        }),
    );
    Some(payload)
}

fn load_stale_runtime_snapshot(
    path: &[&str],
    payload_kind: &str,
    reason: &str,
    command: &str,
) -> Option<Value> {
    read_app_state(path)
        .ok()
        .and_then(|payload| stale_runtime_snapshot(payload, payload_kind, reason, command))
}

fn persist_verified_runtime_snapshot(path: &[&str], payload: &mut Value, payload_kind: &str) {
    if !is_verified_runtime_snapshot(payload, payload_kind) {
        return;
    }
    let revision = payload
        .get("desktop_revision")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let already_persisted = revision.as_deref().is_some_and(|revision| {
        read_app_state(path)
            .ok()
            .and_then(|cached| {
                cached
                    .get("desktop_revision")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            })
            .as_deref()
            == Some(revision)
    });
    if let Some(object) = payload.as_object_mut() {
        object.insert(
            "snapshot_persistence".to_string(),
            json!({"state": "available", "source": "dtoken-desktop-control-plane"}),
        );
    }
    if already_persisted {
        return;
    }
    if let Err(error) = write_json_app_state(path, payload) {
        if let Some(object) = payload.as_object_mut() {
            object.insert(
                "snapshot_persistence".to_string(),
                json!({
                    "state": "unavailable",
                    "source": "dtoken-desktop-control-plane",
                    "error": format!("Failed to persist the last verified runtime snapshot: {error}")
                }),
            );
        }
    }
}

fn complete_desktop_summary(mut runtime: Value, health: Value) -> Value {
    let object = runtime.as_object_mut().expect("validated runtime summary");
    object.insert("source".to_string(), json!("dtoken-owned-runtime"));
    object.insert(
        "desktop_data_source".to_string(),
        json!("dtoken-local-service"),
    );
    object.insert("desktop_payload_kind".to_string(), json!("summary"));
    object.insert("evidence_state".to_string(), json!("verified"));
    object.insert("freshness".to_string(), json!("fresh"));
    object.insert("complete".to_string(), json!(true));
    object.insert("partial".to_string(), json!(false));
    object.insert("runtime_available".to_string(), json!(true));
    object.insert("runtime_evidence_state".to_string(), json!("verified"));
    object.insert("runtime_health".to_string(), health);
    object.insert(
        "desktop_service_url".to_string(),
        json!(format!("http://127.0.0.1:{DTOKEN_SERVICE_PORT}")),
    );
    object.insert("active_config".to_string(), active_route_config_status());
    object.insert(
        "transaction_provenance".to_string(),
        json!({
            "source": "dtoken-owned-runtime",
            "command": "get_desktop_summary",
            "schema_version": 1,
            "generated_at": now_timestamp(),
            "service_url": format!("http://127.0.0.1:{DTOKEN_SERVICE_PORT}")
        }),
    );
    runtime
}

fn collect_desktop_summary_value() -> Value {
    let health = match service_health_payload_cached() {
        Some(health)
            if health.get("component").and_then(Value::as_str) == Some("dtoken-core-api")
                && matches!(
                    health.get("status").and_then(Value::as_str),
                    Some("healthy" | "ready")
                ) =>
        {
            health
        }
        Some(_) => {
            return offline_desktop_summary(
                "The local listener did not prove dtoken-core-api health",
            )
        }
        None => return offline_desktop_summary("d-token local service is unavailable"),
    };
    match service_json(DTOKEN_SERVICE_PORT, "/dashboard/summary") {
        Ok(payload)
            if payload.is_object()
                && payload.get("source").and_then(Value::as_str)
                    == Some("dtoken-owned-runtime") =>
        {
            complete_desktop_summary(payload, health)
        }
        Ok(_) => {
            offline_desktop_summary("Dashboard summary did not prove d-token runtime provenance")
        }
        Err(error) => {
            offline_desktop_summary(&format!("Dashboard summary is unavailable: {error}"))
        }
    }
}

fn collect_desktop_data_value() -> Value {
    let mut slice_errors = serde_json::Map::new();
    let route_drafts = route_profile_drafts_result().unwrap_or_else(|err| {
        slice_errors.insert(
            "route_drafts".to_string(),
            json!({"stage": "desktop_section", "state": "unavailable", "error": err}),
        );
        vec![]
    });
    let backup_points = rollback_points_result().unwrap_or_else(|err| {
        slice_errors.insert(
            "rollback_points".to_string(),
            json!({"stage": "desktop_section", "state": "unavailable", "error": err}),
        );
        vec![]
    });
    let health = match service_health_payload_cached() {
        Some(health)
            if health.get("component").and_then(Value::as_str) == Some("dtoken-core-api")
                && matches!(
                    health.get("status").and_then(Value::as_str),
                    Some("healthy" | "ready")
                ) =>
        {
            health
        }
        Some(_) => {
            return offline_control_plane_data(
                "The local listener did not prove dtoken-core-api health",
                route_drafts,
                backup_points,
            )
        }
        None => {
            return offline_control_plane_data(
                "d-token local service is unavailable",
                route_drafts,
                backup_points,
            )
        }
    };
    let (runtime_result, compatibility, protocol_injection, chain_status) =
        thread::scope(|scope| {
            let runtime = scope.spawn(|| service_json(DTOKEN_SERVICE_PORT, "/dashboard/payload"));
            let compatibility =
                scope.spawn(|| service_json(DTOKEN_SERVICE_PORT, "/compatibility/agents").ok());
            let protocol_injection =
                scope.spawn(|| service_json(DTOKEN_SERVICE_PORT, "/protocol-injection").ok());
            let chain_status =
                scope.spawn(|| service_json(DTOKEN_SERVICE_PORT, "/chain/status").ok());
            (
                runtime
                    .join()
                    .unwrap_or_else(|_| Err("dashboard worker panicked".to_string())),
                compatibility.join().unwrap_or(None),
                protocol_injection.join().unwrap_or(None),
                chain_status.join().unwrap_or(None),
            )
        });
    let runtime = match runtime_result {
        Ok(payload)
            if payload.is_object()
                && payload.get("source").and_then(Value::as_str)
                    == Some("dtoken-owned-runtime") =>
        {
            payload
        }
        Ok(_) => {
            return offline_control_plane_data(
                "Dashboard payload did not prove d-token runtime provenance",
                route_drafts,
                backup_points,
            )
        }
        Err(err) => {
            return offline_control_plane_data(
                &format!("Dashboard payload is unavailable: {err}"),
                route_drafts,
                backup_points,
            )
        }
    };
    let mut payload = complete_desktop_data(
        runtime,
        route_drafts,
        backup_points,
        compatibility,
        protocol_injection,
        chain_status,
    );
    if let Some(object) = payload.as_object_mut() {
        object.insert("runtime_health".to_string(), health);
        object.insert("active_config".to_string(), active_route_config_status());
        object.insert(
            "desktop_slice_errors".to_string(),
            Value::Object(slice_errors),
        );
    }
    payload
}

fn finalize_desktop_snapshot(mut payload: Value) -> Value {
    let is_summary = payload.get("desktop_payload_kind").and_then(Value::as_str) == Some("summary")
        || payload.get("payload_kind").and_then(Value::as_str) == Some("summary");
    let revision_input = if is_summary {
        json!({
            "source": payload.get("desktop_data_source").or_else(|| payload.get("source")),
            "runtime_identity": payload.get("runtime_health").and_then(|health| health.get("runtime_identity")),
            "runtime_instance_nonce": payload.get("runtime_health").and_then(|health| health.get("runtime_instance_nonce")),
            "slice_revisions": payload.get("slice_revisions"),
            "active_config": payload.get("active_config")
        })
    } else {
        json!({
            "source": payload.get("desktop_data_source").or_else(|| payload.get("source")),
            "health": payload.get("runtime_health"),
            "providers": payload.get("providers"),
            "routes": payload.get("routes"),
            "instances": payload.get("instances"),
            "request_logs": payload.get("request_logs"),
            "route_decisions": payload.get("route_decisions"),
            "changes": payload.get("changes"),
            "route_drafts": payload.get("routeDrafts"),
            "backup_points": payload.get("backupPoints"),
            "active_config": payload.get("active_config")
        })
    };
    if let Some(object) = payload.as_object_mut() {
        object.insert(
            "desktop_revision".to_string(),
            Value::String(stable_hash(&revision_input.to_string())),
        );
        let preserve_cached_timestamp = object.get("freshness").and_then(Value::as_str)
            == Some("stale")
            && object
                .get("desktop_collected_at")
                .and_then(Value::as_str)
                .is_some();
        if !preserve_cached_timestamp {
            object.insert(
                "desktop_collected_at".to_string(),
                Value::String(now_timestamp()),
            );
        }
    }
    payload
}

fn collect_cached_snapshot(
    cache: &Mutex<Option<(Instant, Value)>>,
    operation: &'static str,
    collector: impl FnOnce() -> Value,
) -> Value {
    let started_at = Instant::now();
    let mut guard = cache
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some((collected_at, payload)) = guard.as_ref() {
        if collected_at.elapsed() <= Duration::from_millis(300) {
            record_native_performance(operation, "complete", started_at, "ok", true, None);
            return payload.clone();
        }
    }
    let payload = collector();
    *guard = Some((Instant::now(), payload.clone()));
    record_native_performance(operation, "complete", started_at, "ok", false, None);
    payload
}

fn get_desktop_data_value() -> Value {
    static CACHE: OnceLock<Mutex<Option<(Instant, Value)>>> = OnceLock::new();
    collect_cached_snapshot(
        CACHE.get_or_init(|| Mutex::new(None)),
        "desktop_detail",
        || {
            let mut payload = finalize_desktop_snapshot(collect_desktop_data_value());
            persist_verified_runtime_snapshot(LAST_VERIFIED_DESKTOP_DETAIL, &mut payload, "detail");
            payload
        },
    )
}

fn get_desktop_summary_value() -> Value {
    static CACHE: OnceLock<Mutex<Option<(Instant, Value)>>> = OnceLock::new();
    collect_cached_snapshot(
        CACHE.get_or_init(|| Mutex::new(None)),
        "desktop_summary",
        || {
            let mut payload = finalize_desktop_snapshot(collect_desktop_summary_value());
            persist_verified_runtime_snapshot(
                LAST_VERIFIED_DESKTOP_SUMMARY,
                &mut payload,
                "summary",
            );
            payload
        },
    )
}

#[tauri::command]
fn get_native_performance_snapshot() -> Value {
    json!({
        "schema_version": 1,
        "source": "d-token-native-performance",
        "observations": native_performance_snapshot(),
        "privacy": "Metadata-only allowlisted timing fields."
    })
}

#[tauri::command]
async fn get_desktop_summary() -> Value {
    tauri::async_runtime::spawn_blocking(get_desktop_summary_value)
        .await
        .unwrap_or_else(|error| {
            offline_desktop_summary(&format!("Desktop summary task failed: {error}"))
        })
}

#[tauri::command]
async fn get_desktop_data() -> Value {
    tauri::async_runtime::spawn_blocking(get_desktop_data_value)
        .await
        .unwrap_or_else(|err| {
            offline_control_plane_data(&format!("Desktop data task failed: {err}"), vec![], vec![])
        })
}

fn collect_analytics_request_page_value(cursor: Option<String>, limit: Option<u16>) -> Value {
    let limit = limit.unwrap_or(20).clamp(1, 100);
    let cursor = cursor.filter(|value| {
        !value.is_empty()
            && value.len() <= 80
            && value.chars().all(|character| {
                character.is_ascii_alphanumeric() || matches!(character, ':' | '-')
            })
    });
    let path = cursor
        .as_deref()
        .map(|cursor| format!("/analytics/requests?limit={limit}&cursor={cursor}"))
        .unwrap_or_else(|| format!("/analytics/requests?limit={limit}"));
    service_json(DTOKEN_SERVICE_PORT, &path).unwrap_or_else(|error| {
        json!({
            "schema_version": 1,
            "source": "unavailable",
            "freshness": "unknown",
            "items": [],
            "returned_count": 0,
            "total_count": 0,
            "has_more": false,
            "next_cursor": null,
            "error": format!("Analytics request page is unavailable: {error}")
        })
    })
}

#[tauri::command]
async fn get_analytics_request_page(cursor: Option<String>, limit: Option<u16>) -> Value {
    tauri::async_runtime::spawn_blocking(move || {
        collect_analytics_request_page_value(cursor, limit)
    })
    .await
    .unwrap_or_else(|error| {
        json!({
            "schema_version": 1,
            "source": "unavailable",
            "freshness": "unknown",
            "items": [],
            "returned_count": 0,
            "total_count": 0,
            "has_more": false,
            "next_cursor": null,
            "error": format!("Analytics request page task failed: {error}")
        })
    })
}

macro_rules! desktop_release_handlers {
    ($consumer:ident $(, $arguments:tt)*) => {
        $consumer!($($arguments)* [
            get_native_performance_snapshot,
            get_desktop_summary,
            get_desktop_data,
            get_analytics_request_page,
            get_app_settings,
            save_app_settings,
            check_for_updates,
            save_route_profile_draft,
            store_provider_credential,
            delete_provider_credential,
            list_route_profile_drafts,
            preview_route_profile_draft_apply,
            apply_route_profile_draft,
            preview_connection_route_plan,
            apply_connection_route_plan,
            rollback_connection_route_plan,
            get_active_route_config_status,
            verify_agent_route_config,
            recommend_project_policy,
            start_learning_discovery,
            get_learning_discovery_status,
            list_learning_candidates,
            cancel_learning_discovery,
            analyze_learning_project,
            preview_learning_managed_block,
            apply_learning_managed_block,
            get_learning_delegation_capabilities,
            spawn_learning_delegation,
            cancel_learning_delegation,
            manage_backup,
            preview_rollback,
            apply_rollback,
            detect_tools,
            install_tool,
            preview_install_plan,
            preview_opencode_connection,
            apply_opencode_connection,
            discover_upstream_candidates,
            get_dtoken_service_status,
            get_autostart_status,
            enable_autostart,
            disable_autostart,
            start_dtoken_service,
            restart_dtoken_service,
            stop_dtoken_service,
            run_install_transaction,
            get_doctor_snapshot,
            run_doctor,
            run_doctor_scan,
            get_doctor_repair_modules,
            preview_doctor_common_repair,
            apply_doctor_common_repair,
            preview_doctor_repair,
            apply_doctor_repair,
            verify_doctor_repair,
            manage_headroom,
            run_product_readiness_check,
            get_dtoken_dashboard_payload,
            export_dtoken_runtime,
            get_optical_observability,
            preview_optical_plan,
            get_optical_config_editor,
            preview_optical_config_change,
            apply_optical_config_change,
            rollback_optical_config_change,
            get_hybrid_agent_integrations,
            preview_hybrid_agent_integration,
            apply_hybrid_agent_integration,
            verify_hybrid_agent_integration,
            get_hybrid_agent_integration_transaction,
            rollback_hybrid_agent_integration,
            get_codebase_memory_status,
            set_codebase_memory_package_directory,
            open_codebase_memory_package_directory,
            open_codebase_memory_download_page,
            preview_codebase_memory_integration,
            apply_codebase_memory_integration,
            verify_codebase_memory_integration,
            rollback_codebase_memory_integration,
            start_codebase_memory_ui,
            stop_codebase_memory_ui,
            start_codebase_memory_index,
            cancel_codebase_memory_index,
            delete_codebase_memory_index,
            open_codebase_memory_graph,
        ])
    };
}

#[cfg(any(debug_assertions, test))]
macro_rules! debug_desktop_handlers {
    ($consumer:ident) => {
        desktop_release_handlers!(
            $consumer,
            [
                test_opencode,
                test_opencode_models,
                run_opencode_workload_benchmark,
                run_headroom_capability_smoke,
            ]
        )
    };
}

macro_rules! run_desktop_app_with_handlers {
    ([$($debug_command:ident,)*] [$($release_command:ident,)*]) => {
        tauri::Builder::default()
            .plugin(tauri_plugin_opener::init())
            .setup(|_| {
                start_service_when_launched_by_autostart();
                Ok(())
            })
            .invoke_handler(tauri::generate_handler![
            $($release_command,)*
            $($debug_command,)*
            ])
            .run(tauri::generate_context!())
            .expect("error while running d-token desktop");
    };
}

#[cfg(debug_assertions)]
fn main() {
    debug_desktop_handlers!(run_desktop_app_with_handlers);
}

#[cfg(not(debug_assertions))]
fn main() {
    desktop_release_handlers!(run_desktop_app_with_handlers, []);
}

#[cfg(test)]
macro_rules! release_handler_names {
    ([$($release_command:ident,)*]) => {
        &[$(stringify!($release_command),)*]
    };
}

#[cfg(test)]
macro_rules! debug_handler_names {
    ([$($debug_command:ident,)*] [$($release_command:ident,)*]) => {
        &[$(stringify!($release_command),)* $(stringify!($debug_command),)*]
    };
}

#[cfg(test)]
macro_rules! debug_only_handler_names {
    ([$($debug_command:ident,)*] [$($_release_command:ident,)*]) => {
        &[$(stringify!($debug_command),)*]
    };
}

#[cfg(test)]
const RELEASE_HANDLER_NAMES: &[&str] = desktop_release_handlers!(release_handler_names);

#[cfg(test)]
const DEBUG_HANDLER_NAMES: &[&str] = debug_desktop_handlers!(debug_handler_names);

#[cfg(test)]
const DEBUG_ONLY_HANDLER_NAMES: &[&str] = debug_desktop_handlers!(debug_only_handler_names);

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::net::{Shutdown, TcpListener};
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, OnceLock,
    };
    use std::thread;

    fn test_state_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    fn clear_native_performance_observations() {
        NATIVE_PERF_OBSERVATIONS
            .get_or_init(|| Mutex::new(VecDeque::with_capacity(MAX_NATIVE_PERF_OBSERVATIONS)))
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clear();
    }

    #[test]
    fn debug_handler_registration_is_exact_release_superset() {
        let release = RELEASE_HANDLER_NAMES
            .iter()
            .copied()
            .collect::<HashSet<_>>();
        let debug = DEBUG_HANDLER_NAMES.iter().copied().collect::<HashSet<_>>();
        let debug_only = DEBUG_ONLY_HANDLER_NAMES
            .iter()
            .copied()
            .collect::<HashSet<_>>();

        assert_eq!(release.len(), RELEASE_HANDLER_NAMES.len());
        assert_eq!(debug.len(), DEBUG_HANDLER_NAMES.len());
        assert_eq!(debug_only.len(), DEBUG_ONLY_HANDLER_NAMES.len());
        assert!(
            !debug_only.is_empty(),
            "debug handler registration must explicitly declare its debug-only commands"
        );
        assert!(
            release.is_disjoint(&debug_only),
            "release handler registration must not contain debug-only commands"
        );

        let expected_debug = release.union(&debug_only).copied().collect::<HashSet<_>>();
        assert_eq!(
            debug, expected_debug,
            "debug registration must contain every release command plus only debug-only commands"
        );
    }

    fn serve_one_http_response(body: &str) -> (u16, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind one-shot HTTP fixture");
        let port = listener.local_addr().expect("fixture address").port();
        let response = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{body}",
            body.len()
        );
        let worker = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept one-shot HTTP request");
            let mut request = Vec::with_capacity(256);
            let mut chunk = [0_u8; 256];
            loop {
                let read = stream.read(&mut chunk).expect("read one-shot HTTP request");
                if read == 0 {
                    break;
                }
                request.extend_from_slice(&chunk[..read]);
                if request.windows(4).any(|window| window == b"\r\n\r\n") {
                    break;
                }
            }
            stream
                .write_all(response.as_bytes())
                .expect("write one-shot HTTP response");
            stream
                .shutdown(std::net::Shutdown::Write)
                .expect("finish one-shot HTTP response");
        });
        (port, worker)
    }

    #[test]
    fn native_performance_capture_is_bounded_and_metadata_only() {
        let guard = test_state_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        clear_native_performance_observations();
        for _ in 0..300 {
            record_native_performance(
                "desktop_summary",
                "complete",
                Instant::now(),
                "ok",
                false,
                None,
            );
        }

        let snapshot = native_performance_snapshot();
        assert_eq!(snapshot.len(), MAX_NATIVE_PERF_OBSERVATIONS);
        let serialized = serde_json::to_string(&snapshot).expect("serialize native perf snapshot");
        for forbidden in [
            "prompt",
            "response",
            "credential",
            "provider_id",
            "model",
            "route_id",
            "path",
        ] {
            assert!(!serialized.contains(forbidden), "leaked {forbidden}");
        }
        drop(guard);
    }

    #[test]
    fn loopback_service_json_records_fixed_metadata_only_phases() {
        let guard = test_state_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        clear_native_performance_observations();
        let (port, worker) = serve_one_http_response(
            r#"{"status":"healthy","private_fixture":"must-not-be-observed"}"#,
        );

        let payload = service_json(port, "/compression/optical/status")
            .expect("parse loopback fixture payload");
        worker.join().expect("join one-shot HTTP fixture");
        assert_eq!(payload["status"], "healthy");

        let observations = native_performance_snapshot()
            .into_iter()
            .filter(|observation| observation.operation == "loopback_optical_status")
            .collect::<Vec<_>>();
        let phases = observations
            .iter()
            .map(|observation| observation.phase)
            .collect::<HashSet<_>>();
        for required in ["connect", "write", "read", "parse", "total"] {
            assert!(phases.contains(required), "missing {required} phase");
        }
        assert!(observations.iter().all(|observation| {
            observation.result_state == "ok"
                && !observation.cache_hit
                && observation.error_code.is_none()
        }));

        let serialized = serde_json::to_string(&observations).expect("serialize observations");
        for forbidden in [
            "/compression/optical/status",
            "127.0.0.1",
            "must-not-be-observed",
            "provider_id",
            "model",
            "route_id",
        ] {
            assert!(!serialized.contains(forbidden), "leaked {forbidden}");
        }
        drop(guard);
    }

    #[test]
    fn loopback_json_parse_failure_uses_fixed_error_metadata() {
        let guard = test_state_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        clear_native_performance_observations();
        let (port, worker) = serve_one_http_response("free-text-parse-error-fixture");

        let error = service_json(port, "/compression/optical/capabilities")
            .expect_err("reject invalid JSON response");
        worker.join().expect("join one-shot HTTP fixture");
        assert!(!error.is_empty());

        let observations = native_performance_snapshot();
        assert!(observations.iter().any(|observation| {
            observation.operation == "loopback_optical_capabilities"
                && observation.phase == "parse"
                && observation.result_state == "error"
                && observation.error_code == Some("json_parse_failed")
        }));
        let serialized = serde_json::to_string(&observations).expect("serialize observations");
        assert!(!serialized.contains("free-text-parse-error-fixture"));
        assert!(!serialized.contains(&error));
        drop(guard);
    }

    #[test]
    fn retain_http_body_reuses_the_response_allocation() {
        let mut response = String::with_capacity(256);
        response.push_str("HTTP/1.1 200 OK\r\ncontent-type: application/json\r\n\r\n{\"ok\":true}");
        let allocation = response.as_ptr();
        let capacity = response.capacity();

        retain_http_body(&mut response);

        assert_eq!(response, r#"{"ok":true}"#);
        assert_eq!(response.as_ptr(), allocation);
        assert_eq!(response.capacity(), capacity);
    }

    #[test]
    fn desktop_snapshot_cache_coalesces_concurrent_collectors() {
        use std::sync::{atomic::AtomicUsize, Barrier};

        let cache = Arc::new(Mutex::new(None));
        let collect_count = Arc::new(AtomicUsize::new(0));
        let barrier = Arc::new(Barrier::new(8));
        let workers = (0..8)
            .map(|_| {
                let cache = Arc::clone(&cache);
                let collect_count = Arc::clone(&collect_count);
                let barrier = Arc::clone(&barrier);
                thread::spawn(move || {
                    barrier.wait();
                    collect_cached_snapshot(&cache, "desktop_summary", || {
                        collect_count.fetch_add(1, Ordering::SeqCst);
                        thread::sleep(Duration::from_millis(40));
                        json!({"source": "single-flight-fixture"})
                    })
                })
            })
            .collect::<Vec<_>>();

        for worker in workers {
            assert_eq!(
                worker.join().expect("join snapshot collector")["source"],
                "single-flight-fixture"
            );
        }
        assert_eq!(collect_count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn bridge_hash_authority_accounts_for_macos_codesign_mutation() {
        assert_eq!(
            agent_bridge_requires_embedded_hash(),
            !cfg!(target_os = "macos")
        );
    }

    #[test]
    fn bridge_platform_trust_is_verified_before_sidecar_execution() {
        let executed = AtomicBool::new(false);
        let result = verify_platform_before_agent_bridge_execution(
            Path::new("untrusted-dtoken-agent-bridge"),
            |_| Err("untrusted bridge signature".to_string()),
            |_| {
                executed.store(true, Ordering::SeqCst);
                Ok(json!({}))
            },
        );

        assert_eq!(result.unwrap_err(), "untrusted bridge signature");
        assert!(!executed.load(Ordering::SeqCst));
    }

    fn with_isolated_app_state<T>(name: &str, run: impl FnOnce() -> T) -> T {
        let _guard = test_state_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let previous = env::var("DTOKEN_TEST_STATE_DIR").ok();
        let root = env::temp_dir().join(format!(
            "dtoken-desktop-test-{name}-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .unwrap_or_default()
        ));
        fs::create_dir_all(&root).expect("create isolated app state");
        env::set_var("DTOKEN_TEST_STATE_DIR", &root);
        let result = run();
        match previous {
            Some(value) => env::set_var("DTOKEN_TEST_STATE_DIR", value),
            None => env::remove_var("DTOKEN_TEST_STATE_DIR"),
        }
        let _ = fs::remove_dir_all(root);
        result
    }

    fn with_fake_opencode_on_path<T>(run: impl FnOnce() -> T) -> T {
        let directory = app_state_dir().join("test-bin");
        fs::create_dir_all(&directory).expect("create test tool directory");
        let executable = directory.join(if cfg!(windows) {
            "opencode.exe"
        } else {
            "opencode"
        });
        fs::write(&executable, b"d-token test tool").expect("write fake OpenCode executable");

        let previous = env::var_os("PATH");
        let mut paths = vec![directory];
        if let Some(path) = previous.as_ref() {
            paths.extend(env::split_paths(path));
        }
        env::set_var("PATH", env::join_paths(paths).expect("compose test PATH"));
        let result = run();
        match previous {
            Some(path) => env::set_var("PATH", path),
            None => env::remove_var("PATH"),
        }
        result
    }

    fn review_and_apply_route(route_id: &str) -> ActionResult {
        let review = preview_route_profile_draft_apply(RouteDraftReviewRequest {
            route_id: route_id.to_string(),
        });
        assert_eq!(review["ok"], true, "route review failed: {review}");
        apply_route_profile_draft(ApplyRouteDraftRequest {
            route_id: route_id.to_string(),
            reviewed_before_sha256: review["before_sha256"].as_str().map(str::to_string),
            reviewed_after_sha256: review["after_sha256"].as_str().map(str::to_string),
        })
    }

    fn save_test_route(route_id: &str, base_url: &str) -> ActionResult {
        save_route_profile_draft(RouteProfileDraftRequest {
            route_id: route_id.to_string(),
            route_name: format!("{route_id} route"),
            provider_id: format!("{route_id}-provider"),
            provider_name: format!("{route_id} provider"),
            provider_kind: "custom".to_string(),
            protocol: "openai".to_string(),
            base_url: base_url.to_string(),
            api_key_ref: None,
            models: vec!["model/test".to_string()],
            chain_mode: "direct".to_string(),
            compression_policy: "balanced".to_string(),
            local_port: DTOKEN_SERVICE_PORT,
            agent: "OpenCode".to_string(),
        })
    }

    fn apply_test_config_change(
        target: &Path,
        before: &str,
        after: &str,
        tool: ConfigToolKind,
    ) -> dtoken_config_patcher::BackupRecord {
        fs::create_dir_all(target.parent().expect("test config parent"))
            .expect("create test config parent");
        fs::write(target, before).expect("write test config before state");
        let patcher = config_patcher();
        let prepared = patcher
            .prepare_change(target, after, tool, None, RiskLevel::Medium)
            .expect("prepare test config change");
        let backup = patcher
            .backup_prepared(&prepared)
            .expect("backup test config");
        patcher
            .apply_prepared(&prepared, &backup)
            .expect("apply test config change");
        backup
    }

    #[test]
    fn app_state_uses_only_the_explicit_test_override_for_temporary_storage() {
        with_isolated_app_state("state-location", || {
            let root = PathBuf::from(env::var("DTOKEN_TEST_STATE_DIR").expect("test override"));
            assert_eq!(app_state_dir(), root);
            assert!(!app_state_dir().starts_with(env::temp_dir().join("d-token")));
        });
    }

    #[test]
    fn durable_json_write_replaces_content_without_leaving_a_partial_file() {
        with_isolated_app_state("atomic-json", || {
            write_json_app_state(&["settings", "app.json"], &json!({"version": 1}))
                .expect("write first version");
            write_json_app_state(&["settings", "app.json"], &json!({"version": 2}))
                .expect("replace second version");

            let saved = read_app_state(&["settings", "app.json"]).expect("read saved JSON");
            assert_eq!(saved["version"], 2);
            let entries = fs::read_dir(app_state_dir().join("settings"))
                .expect("read settings directory")
                .flatten()
                .collect::<Vec<_>>();
            assert_eq!(
                entries.len(),
                1,
                "temporary transaction files must be removed"
            );
        });
    }

    #[test]
    fn missing_active_config_is_empty_and_never_materializes_a_placeholder_route() {
        with_isolated_app_state("empty-active-config", || {
            assert!(!active_config_path().exists());
            let result = validate_active_config_for_start();
            assert!(result.is_err());
            assert!(!active_config_path().exists());
            assert_eq!(active_route_config_status()["provider_count"], 0);
            assert_eq!(active_route_config_status()["route_count"], 0);
        });
    }

    #[test]
    fn offline_desktop_data_preserves_local_control_plane_without_runtime_claims() {
        with_isolated_app_state("desktop-unavailable", || {
            let drafts = vec![json!({
                "path": "/local/draft.json",
                "config": { "routes": [{ "id": "offline-route" }] },
                "diff": [{ "type": "add", "text": "+ route" }]
            })];
            let points = vec![json!({ "id": "offline-backup", "verified": true })];
            let payload = offline_control_plane_data("runtime unavailable", drafts, points);
            assert_eq!(payload["complete"], false);
            assert_eq!(payload["source"], "dtoken-desktop-control-plane");
            assert_eq!(payload["freshness"], "local");
            assert_eq!(payload["runtime_available"], false);
            assert_eq!(payload["transaction_provenance"]["runtime_required"], false);
            assert_eq!(payload["routeDrafts"].as_array().map(Vec::len), Some(1));
            assert_eq!(payload["backupPoints"].as_array().map(Vec::len), Some(1));
            assert_eq!(payload["changes"].as_array().map(Vec::len), Some(1));
            for field in [
                "health",
                "savings",
                "routes",
                "providers",
                "logs",
                "recentEvents",
            ] {
                assert!(
                    payload
                        .get(field)
                        .and_then(Value::as_array)
                        .is_some_and(Vec::is_empty),
                    "{field} must be explicitly empty"
                );
            }
        });
    }

    #[test]
    fn complete_desktop_data_has_the_required_transaction_provenance() {
        let runtime = json!({
            "source": "dtoken-owned-runtime",
            "providers": [],
            "routes": [],
            "instances": [],
            "request_logs": []
        });
        let payload = complete_desktop_data(runtime, vec![], vec![], None, None, None);
        assert_eq!(payload["source"], "dtoken-owned-runtime");
        assert_eq!(payload["desktop_data_source"], "dtoken-local-service");
        assert_eq!(payload["desktop_payload_kind"], "detail");
        assert_eq!(payload["freshness"], "fresh");
        assert_eq!(payload["complete"], true);
        assert_eq!(
            payload["transaction_provenance"]["source"],
            "dtoken-owned-runtime"
        );
        assert_eq!(
            payload["transaction_provenance"]["command"],
            "get_desktop_data"
        );
        assert_eq!(payload["transaction_provenance"]["schema_version"], 1);
        assert!(payload["transaction_provenance"]["generated_at"]
            .as_str()
            .is_some_and(|value| !value.is_empty()));
    }

    #[test]
    fn complete_desktop_summary_exposes_the_verified_loopback_address() {
        let payload = complete_desktop_summary(
            json!({"source": "dtoken-owned-runtime", "stats": {}}),
            json!({
                "status": "healthy",
                "component": "dtoken-core-api",
                "runtime_identity": "dtoken-runtime-local"
            }),
        );

        assert_eq!(
            payload["desktop_service_url"],
            format!("http://127.0.0.1:{DTOKEN_SERVICE_PORT}")
        );
        assert_eq!(
            payload["runtime_health"]["runtime_identity"],
            "dtoken-runtime-local"
        );
    }

    #[test]
    fn stale_runtime_summary_preserves_verified_metrics_and_original_timestamp() {
        let cached = finalize_desktop_snapshot(complete_desktop_summary(
            json!({
                "source": "dtoken-owned-runtime",
                "stats": {"requests": {"total": 42, "failed": 1}},
                "routes": [{"id": "route-a"}],
                "request_logs": [{"route_id": "route-a", "input_tokens_before": 12}]
            }),
            json!({
                "status": "healthy",
                "component": "dtoken-core-api",
                "runtime_identity": "dtoken-runtime-local"
            }),
        ));
        let collected_at = cached["desktop_collected_at"]
            .as_str()
            .expect("verified timestamp")
            .to_string();

        let stale = stale_runtime_snapshot(
            cached,
            "summary",
            "d-token local service is unavailable",
            "get_desktop_summary",
        )
        .expect("verified summary becomes stale evidence");
        let finalized = finalize_desktop_snapshot(stale);

        assert_eq!(finalized["stats"]["requests"]["total"], 42);
        assert_eq!(finalized["routes"][0]["id"], "route-a");
        assert_eq!(finalized["evidence_state"], "stale");
        assert_eq!(finalized["freshness"], "stale");
        assert_eq!(finalized["runtime_available"], false);
        assert_eq!(finalized["runtime_health"], Value::Null);
        assert_eq!(finalized["desktop_collected_at"], collected_at);
        assert_eq!(
            finalized["cached_snapshot"]["source"],
            "dtoken-owned-runtime"
        );
    }

    #[test]
    fn stale_runtime_snapshot_rejects_unverified_or_wrong_kind_payloads() {
        assert!(stale_runtime_snapshot(
            json!({
                "source": "dtoken-desktop-control-plane",
                "desktop_data_source": "dtoken-desktop-control-plane",
                "desktop_payload_kind": "summary",
                "evidence_state": "partial",
                "runtime_available": false
            }),
            "summary",
            "offline",
            "get_desktop_summary"
        )
        .is_none());
        assert!(stale_runtime_snapshot(
            json!({
                "source": "dtoken-owned-runtime",
                "desktop_data_source": "dtoken-local-service",
                "desktop_payload_kind": "detail",
                "evidence_state": "verified",
                "runtime_available": true
            }),
            "summary",
            "offline",
            "get_desktop_summary"
        )
        .is_none());
    }

    #[test]
    fn http_body_extracts_json_payload() {
        let raw = "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\n\r\n{\"source\":\"dtoken-owned-runtime\"}";
        assert_eq!(http_body(raw), "{\"source\":\"dtoken-owned-runtime\"}");
    }

    #[test]
    fn http_response_status_accepts_2xx_only() {
        assert!(ensure_http_success("HTTP/1.1 200 OK\r\n\r\n{}").is_ok());
        assert!(ensure_http_success("HTTP/1.1 204 No Content\r\n\r\n").is_ok());
        assert!(ensure_http_success("HTTP/1.1 404 Not Found\r\n\r\n{}")
            .unwrap_err()
            .contains("HTTP 404"));
        assert!(
            ensure_http_success("HTTP/1.1 500 Internal Server Error\r\n\r\n{}")
                .unwrap_err()
                .contains("HTTP 500")
        );
    }

    #[test]
    fn optical_product_path_rejects_active_modes_and_unknown_routes() {
        let known = vec!["route-a".to_string()];
        let blocked = OpticalConfigDraftRequest {
            global: OpticalGlobalDraftRequest {
                enabled: true,
                default_mode: "tool_results_only".to_string(),
            },
            routes: vec![],
        };
        assert!(clamp_optical_draft(&blocked, &known)
            .unwrap_err()
            .contains("active-leaning"));

        let unknown = OpticalConfigDraftRequest {
            global: OpticalGlobalDraftRequest {
                enabled: true,
                default_mode: "shadow_preview".to_string(),
            },
            routes: vec![OpticalRouteDraftRequest {
                route_id: "missing".to_string(),
                enabled: true,
                mode: "shadow_preview".to_string(),
                allow_tool_result: true,
                allow_history: false,
                confirmed_experimental_lossy: false,
            }],
        };
        assert!(clamp_optical_draft(&unknown, &known)
            .unwrap_err()
            .contains("unknown route_id"));

        let ok = OpticalConfigDraftRequest {
            global: OpticalGlobalDraftRequest {
                enabled: true,
                default_mode: "shadow_preview".to_string(),
            },
            routes: vec![OpticalRouteDraftRequest {
                route_id: "route-a".to_string(),
                enabled: true,
                mode: "shadow_preview".to_string(),
                allow_tool_result: true,
                allow_history: false,
                confirmed_experimental_lossy: true,
            }],
        };
        let patch = clamp_optical_draft(&ok, &known).expect("safe draft");
        assert_eq!(patch["global_policy"]["allow_active"], false);
        assert_eq!(patch["global_policy"]["public_summary_only"], true);
        assert_eq!(patch["global_policy"]["enabled"], true);
        assert_eq!(
            patch["route_policies"][0]["policy"]["mode"],
            "shadow_preview"
        );
    }

    #[test]
    fn optical_config_preview_requires_active_config() {
        with_isolated_app_state("optical-preview-missing", || {
            let result = preview_optical_config_change(PreviewOpticalConfigRequest {
                draft: OpticalConfigDraftRequest {
                    global: OpticalGlobalDraftRequest {
                        enabled: true,
                        default_mode: "shadow_preview".to_string(),
                    },
                    routes: vec![],
                },
            });
            assert_eq!(result["ok"], false);
            assert!(
                result["error"]
                    .as_str()
                    .unwrap_or_default()
                    .to_ascii_lowercase()
                    .contains("active config")
                    || result["error"]
                        .as_str()
                        .unwrap_or_default()
                        .contains("missing")
            );
        });
    }

    #[test]
    fn provider_target_cannot_point_back_to_the_same_dtoken_listener() {
        assert!(provider_targets_local_listener(
            "http://127.0.0.1:18001/v1",
            18001
        ));
        assert!(provider_targets_local_listener(
            "http://localhost:18001/v1",
            18001
        ));
        assert!(provider_targets_local_listener(
            "http://[::1]:18001/v1",
            18001
        ));
        assert!(!provider_targets_local_listener(
            "http://127.0.0.1:19001/v1",
            18001
        ));
        assert!(!provider_targets_local_listener(
            "https://api.example.com/v1",
            18001
        ));
    }

    #[test]
    fn provider_credential_reference_uses_a_stable_os_store_target() {
        assert_eq!(
            credential_reference_for_provider("OpenCode Zen"),
            (
                "os-credential://d-token/providers/opencode-zen".to_string(),
                "providers/opencode-zen".to_string()
            )
        );
    }

    #[test]
    fn automatic_connection_route_plan_is_bound_to_discovery_and_uses_agent_models() {
        with_isolated_app_state("automatic-route-plan", || {
            let discovery = json!({
                "revision": "setup-reviewed",
                "agent_bindings": [{
                    "agent_id": "opencode",
                    "installed": true,
                    "capability": "one-click",
                    "candidates": [{
                        "id": "opencode",
                        "name": "OpenCode official free catalog",
                        "source": "opencode-provider-catalog-cache",
                        "protocol": "open_ai",
                        "base_url": "https://opencode.ai/zen/v1",
                        "models": ["deepseek-v4-flash-free"],
                        "auth_mode": "none"
                    }]
                }]
            });
            let request = ConnectionRoutePlanRequest {
                discovery_revision: "setup-reviewed".to_string(),
                bindings: vec![ConnectionRouteBindingRequest {
                    agent_id: "opencode".to_string(),
                    candidate_id: "opencode".to_string(),
                    candidate_source: "opencode-provider-catalog-cache".to_string(),
                    model: "deepseek-v4-flash-free".to_string(),
                }],
            };

            let (revision, prepared, agents) =
                prepare_connection_route_plan_from_discovery(&request, &discovery)
                    .expect("prepare automatic route plan");
            assert_eq!(revision, "setup-reviewed");
            assert_eq!(agents, vec!["opencode"]);
            assert!(prepared.plan.diff.contains("deepseek-v4-flash-free"));
            assert!(prepared.plan.diff.contains("https://opencode.ai/zen/v1"));
            assert!(!prepared.plan.diff.contains("sk-"));
            assert!(!prepared.plan.diff.contains("secret-value"));

            let mut stale = request.clone();
            stale.discovery_revision = "setup-stale".to_string();
            let error = prepare_connection_route_plan_from_discovery(&stale, &discovery)
                .expect_err("stale discovery must fail");
            assert!(error.contains("changed after discovery"));
        });
    }

    #[test]
    fn automatic_connection_route_plan_rejects_non_reusable_credentials() {
        with_isolated_app_state("automatic-route-credential", || {
            let discovery = json!({
                "revision": "setup-reviewed",
                "agent_bindings": [{
                    "agent_id": "codex",
                    "installed": true,
                    "capability": "confirm-required",
                    "candidates": [{
                        "id": "private",
                        "name": "Private upstream",
                        "source": "agent-config:codex",
                        "protocol": "open_ai",
                        "base_url": "https://private.example/v1",
                        "models": ["private-model"],
                        "auth_mode": "unavailable"
                    }]
                }]
            });
            let request = ConnectionRoutePlanRequest {
                discovery_revision: "setup-reviewed".to_string(),
                bindings: vec![ConnectionRouteBindingRequest {
                    agent_id: "codex".to_string(),
                    candidate_id: "private".to_string(),
                    candidate_source: "agent-config:codex".to_string(),
                    model: "private-model".to_string(),
                }],
            };
            let error = prepare_connection_route_plan_from_discovery(&request, &discovery)
                .expect_err("credential reuse must fail closed");
            assert!(error.contains("cannot be reused safely"));
        });
    }

    #[test]
    fn project_policy_recommendation_uses_runtime_engine() {
        let result = recommend_project_policy(ProjectPolicyRecommendRequest {
            project_id: Some("desktop-project".to_string()),
            language_stack: Some(vec!["rust".to_string(), "typescript".to_string()]),
            avg_request_tokens_7d: Some(47_307),
            avg_cli_noise_ratio_7d: Some(0.42),
            retrieve_rate_7d: Some(0.02),
            failure_rate_7d: Some(0.0),
            high_risk_task: Some(false),
            manual_override: Some(false),
        });

        assert_eq!(result["source"], "dtoken-project-policy");
        assert_eq!(result["runtime"], "d-token-owned");
        assert_eq!(result["recommendation"]["project_id"], "desktop-project");
        assert!(result["recommendation"]["recommended_level"].is_string());
        assert_eq!(result["mapping"]["enable_output_shaper"], true);
    }

    #[test]
    fn install_transaction_for_dtoken_returns_auditable_logs() {
        with_isolated_app_state("manual-install-read-only", || {
            let result = run_install_transaction(InstallPlanRequest {
                target: "dtoken".to_string(),
                install_root: app_state_dir().display().to_string(),
                proxy_port: DTOKEN_SERVICE_PORT,
                auto_backup: true,
                run_doctor_after: false,
                rollback_point: true,
                mode: "manual".to_string(),
            });

            assert!(result.summary.contains("dtoken installation transaction"));
            assert!(result.logs.iter().any(|entry| entry
                .message
                .contains("scan: starting installation transaction")));
            assert!(result
                .logs
                .iter()
                .any(|entry| entry.message.contains("diff: generated config diff")));
            assert!(result
                .logs
                .iter()
                .any(|entry| entry.message.contains("verify: refreshed local tool scan")));
            assert!(result
                .logs
                .iter()
                .any(|entry| entry.message.contains("read-only mode does not persist")));
            assert!(!app_state_dir().join("backups").exists());
            assert!(!app_state_dir().join("rollback-points").exists());
        });
    }

    #[test]
    fn service_lifecycle_refuses_manifest_identity_mismatch() {
        with_isolated_app_state("service-identity-mismatch", || {
            let manifest = ManagedServiceManifest {
                schema_version: MANAGED_SERVICE_MANIFEST_SCHEMA_VERSION,
                managed_by: "another-app".to_string(),
                identity: "not-dtoken".to_string(),
                pid: 999_999,
                port: DTOKEN_SERVICE_PORT,
                exe_path: "unknown".to_string(),
                exe_sha256: None,
                process_birth_token: None,
                runtime_instance_nonce: None,
                config_path: "unknown".to_string(),
                data_dir: "unknown".to_string(),
                started_at: now_label(),
            };
            write_service_manifest(&manifest).expect("write manifest");

            let result = stop_dtoken_service();
            assert!(!result.ok);
            assert!(result.summary.contains("identity"));
            assert!(service_manifest_path().exists());
        });
    }

    #[test]
    fn service_lifecycle_clears_stale_owned_manifest() {
        with_isolated_app_state("service-stale-manifest", || {
            let manifest = ManagedServiceManifest {
                schema_version: MANAGED_SERVICE_MANIFEST_SCHEMA_VERSION,
                managed_by: "d-token-desktop".to_string(),
                identity: "dtoken-core-api".to_string(),
                pid: 999_999,
                port: DTOKEN_SERVICE_PORT,
                exe_path: "unknown".to_string(),
                exe_sha256: None,
                process_birth_token: None,
                runtime_instance_nonce: None,
                config_path: "unknown".to_string(),
                data_dir: "unknown".to_string(),
                started_at: now_label(),
            };
            write_service_manifest(&manifest).expect("write manifest");

            let result = stop_dtoken_service();
            assert!(result.ok);
            assert!(result.summary.contains("Stale"));
            assert!(!service_manifest_path().exists());
        });
    }

    #[test]
    fn managed_service_binary_evidence_matches_launch_content() {
        with_isolated_app_state("service-binary-match", || {
            let exe = app_state_dir().join("dtoken-proxy-fixture");
            fs::write(&exe, b"bundle-a").expect("write runtime fixture");
            let launch_sha256 = file_sha256(&exe).expect("hash runtime fixture");
            let manifest = ManagedServiceManifest {
                schema_version: MANAGED_SERVICE_MANIFEST_SCHEMA_VERSION,
                managed_by: "d-token-desktop".to_string(),
                identity: "dtoken-core-api".to_string(),
                pid: 1,
                port: DTOKEN_SERVICE_PORT,
                exe_path: exe.display().to_string(),
                exe_sha256: Some(launch_sha256.clone()),
                process_birth_token: None,
                runtime_instance_nonce: None,
                config_path: "fixture-config".to_string(),
                data_dir: "fixture-data".to_string(),
                started_at: now_label(),
            };

            let evidence = managed_service_binary_evidence(&manifest);
            assert_eq!(evidence.state, "matched");
            assert!(evidence.compatible);
            assert_eq!(
                evidence.launch_sha256.as_deref(),
                Some(launch_sha256.as_str())
            );
            assert_eq!(
                evidence.current_sha256.as_deref(),
                Some(launch_sha256.as_str())
            );
        });
    }

    #[test]
    fn legacy_owned_runtime_without_runtime_identity_requires_restart() {
        assert!(legacy_managed_process_requires_restart(
            None,
            true,
            Some("legacy_manifest")
        ));
        assert!(!legacy_managed_process_requires_restart(
            Some("dtoken-runtime-local"),
            true,
            Some("legacy_manifest")
        ));
        assert!(!legacy_managed_process_requires_restart(
            None,
            false,
            Some("legacy_manifest")
        ));
        assert!(!legacy_managed_process_requires_restart(
            None,
            true,
            Some("bundle_replaced")
        ));
    }

    #[test]
    fn managed_service_binary_evidence_detects_same_path_bundle_replacement() {
        with_isolated_app_state("service-binary-replaced", || {
            let exe = app_state_dir().join("dtoken-proxy-fixture");
            fs::write(&exe, b"bundle-a").expect("write original runtime fixture");
            let launch_sha256 = file_sha256(&exe).expect("hash original runtime fixture");
            let manifest = ManagedServiceManifest {
                schema_version: MANAGED_SERVICE_MANIFEST_SCHEMA_VERSION,
                managed_by: "d-token-desktop".to_string(),
                identity: "dtoken-core-api".to_string(),
                pid: 1,
                port: DTOKEN_SERVICE_PORT,
                exe_path: exe.display().to_string(),
                exe_sha256: Some(launch_sha256.clone()),
                process_birth_token: None,
                runtime_instance_nonce: None,
                config_path: "fixture-config".to_string(),
                data_dir: "fixture-data".to_string(),
                started_at: now_label(),
            };
            fs::write(&exe, b"bundle-b").expect("replace runtime fixture in place");

            let evidence = managed_service_binary_evidence(&manifest);
            assert_eq!(evidence.state, "bundle_replaced");
            assert!(!evidence.compatible);
            assert_eq!(
                evidence.launch_sha256.as_deref(),
                Some(launch_sha256.as_str())
            );
            assert_ne!(
                evidence.current_sha256.as_deref(),
                Some(launch_sha256.as_str())
            );
        });
    }

    #[test]
    fn managed_service_binary_evidence_rejects_legacy_manifest() {
        let manifest: ManagedServiceManifest = serde_json::from_value(json!({
            "managed_by": "d-token-desktop",
            "identity": "dtoken-core-api",
            "pid": 1,
            "port": DTOKEN_SERVICE_PORT,
            "exe_path": "legacy-runtime",
            "config_path": "legacy-config",
            "data_dir": "legacy-data",
            "started_at": "00:00:00"
        }))
        .expect("deserialize legacy manifest");

        let evidence = managed_service_binary_evidence(&manifest);
        assert_eq!(manifest.schema_version, 0);
        assert_eq!(manifest.exe_sha256, None);
        assert_eq!(evidence.state, "legacy_manifest");
        assert!(!evidence.compatible);
    }

    #[test]
    fn service_lifecycle_resolves_explicit_proxy_exe() {
        with_isolated_app_state("service-resolve-proxy", || {
            let proxy = app_state_dir().join("fake-dtoken-proxy.exe");
            fs::create_dir_all(proxy.parent().expect("fake proxy parent"))
                .expect("create fake proxy parent");
            fs::write(&proxy, "placeholder").expect("write fake proxy");
            let previous = env::var("DTOKEN_PROXY_EXE").ok();
            env::set_var("DTOKEN_PROXY_EXE", &proxy);
            let resolved = resolve_dtoken_proxy_exe();
            match previous {
                Some(value) => env::set_var("DTOKEN_PROXY_EXE", value),
                None => env::remove_var("DTOKEN_PROXY_EXE"),
            }
            assert_eq!(
                resolved.expect("resolve proxy"),
                fs::canonicalize(proxy).unwrap()
            );
        });
    }

    #[test]
    fn service_lifecycle_start_stop_real_proxy_when_enabled() {
        if env::var("DTOKEN_LIFECYCLE_TEST").ok().as_deref() != Some("1") {
            return;
        }
        with_isolated_app_state("service-lifecycle-real", || {
            assert!(
                service_health_payload().is_none(),
                "127.0.0.1:{DTOKEN_SERVICE_PORT} is already occupied; lifecycle test refuses to touch unmanaged services"
            );
            write_json_app_state(
                &["active-config", "dtoken-config.json"],
                &json!({
                    "providers": [{
                        "id": "lifecycle-provider",
                        "name": "Lifecycle test provider",
                        "kind": "custom",
                        "protocol": "open_ai",
                        "base_url": "http://127.0.0.1:39199",
                        "api_key_ref": null,
                        "models": ["lifecycle/test"]
                    }],
                    "routes": [{
                        "id": "lifecycle-route",
                        "name": "Lifecycle test route",
                        "input_protocol": "open_ai",
                        "instance_id": "lifecycle-instance",
                        "target_provider_id": "lifecycle-provider",
                        "fallback_provider_id": null,
                        "chain_mode": "direct",
                        "compression_policy": "off",
                        "bind_host": "127.0.0.1",
                        "local_port": DTOKEN_SERVICE_PORT
                    }],
                    "instances": [{
                        "id": "lifecycle-instance",
                        "name": "Lifecycle test instance",
                        "agent": "OpenCode",
                        "route_id": "lifecycle-route",
                        "local_port": DTOKEN_SERVICE_PORT,
                        "process_status": "stopped"
                    }]
                }),
            )
            .expect("write explicit lifecycle config");

            let start = start_dtoken_service();
            assert!(
                start.ok,
                "start failed: {}",
                start
                    .logs
                    .iter()
                    .map(|entry| entry.message.as_str())
                    .collect::<Vec<_>>()
                    .join(" | ")
            );

            let status = dtoken_service_status_value();
            let status_ok = status["state"] == "running" && status["managed"] == true;
            let stop = stop_dtoken_service();
            assert!(
                stop.ok,
                "stop failed: {}",
                stop.logs
                    .iter()
                    .map(|entry| entry.message.as_str())
                    .collect::<Vec<_>>()
                    .join(" | ")
            );

            for _ in 0..30 {
                if service_health_payload().is_none() {
                    break;
                }
                thread::sleep(Duration::from_millis(100));
            }

            assert!(
                status_ok,
                "unexpected lifecycle status after start: {status}"
            );
            assert_eq!(dtoken_service_status_value()["state"], "stopped");
        });
    }

    #[test]
    fn product_readiness_check_is_product_scoped_without_external_headroom_gate() {
        with_isolated_app_state("product-readiness", || {
            with_fake_opencode_on_path(|| {
                let result = run_product_readiness_check();
                assert_eq!(result["mode"], "product-readiness-check");
                assert_eq!(result["total"], 9);
                assert_eq!(result["external_services"], "Does not start, stop, reconfigure, or depend on external Headroom, RTK, or cc-switch services.");
                let checks = result["checks"].as_array().expect("readiness checks");
                assert!(checks.iter().any(|check| {
                    check["id"] == "install_connect" && check["ok"].as_bool().unwrap_or(false)
                }));
                assert!(checks.iter().any(|check| {
                    check["id"] == "agent_detection" && check["ok"].as_bool().unwrap_or(false)
                }));
                assert!(checks.iter().any(|check| {
                    check["id"] == "doctor"
                        && check["summary"]
                            .as_str()
                            .is_some_and(|summary| !summary.trim().is_empty())
                }));
                let text = result.to_string();
                assert!(!text.contains("headroom-capability-smoke"));
                assert!(!text.contains("workload UI"));
                assert!(!app_state_dir().join("settings").exists());
                assert!(!app_state_dir().join("backups").exists());
                assert!(!app_state_dir().join("rollback-points").exists());
                assert!(!app_state_dir().join("config-backups").exists());
            });
        });
    }

    #[test]
    fn product_readiness_check_passes_with_dtoken_runtime_metadata() {
        with_isolated_app_state("product-readiness-runtime", || {
            with_fake_opencode_on_path(|| {
                let config = json!({
                    "providers": [{
                        "id": "readiness-provider",
                        "name": "Readiness Provider",
                        "kind": "custom",
                        "protocol": "open_ai",
                        "base_url": "http://127.0.0.1:39199",
                        "api_key_ref": null,
                        "models": ["mock/model"]
                    }],
                    "routes": [{
                        "id": "readiness-route",
                        "name": "Readiness Route",
                        "input_protocol": "open_ai",
                        "instance_id": "readiness-instance",
                        "target_provider_id": "readiness-provider",
                        "fallback_provider_id": null,
                        "chain_mode": "direct",
                        "compression_policy": "balanced",
                        "bind_host": "127.0.0.1",
                        "local_port": DTOKEN_SERVICE_PORT
                    }],
                    "instances": [{
                        "id": "readiness-instance",
                        "name": "Readiness Instance",
                        "agent": "OpenCode",
                        "route_id": "readiness-route",
                        "local_port": DTOKEN_SERVICE_PORT,
                        "process_status": "running"
                    }]
                });
                write_app_state(
                    &["active-config", "dtoken-config.json"],
                    &serde_json::to_string_pretty(&config).expect("serialize active config"),
                )
                .expect("write active config");

                let Some(shutdown) = start_fake_dtoken_runtime() else {
                    // A real d-token service can legitimately own the fixed product port
                    // during workspace tests. Its live behavior is covered by the app UAT.
                    return;
                };
                let exe = std::env::current_exe().expect("resolve readiness runtime fixture");
                let manifest = ManagedServiceManifest {
                    schema_version: MANAGED_SERVICE_MANIFEST_SCHEMA_VERSION,
                    managed_by: "d-token-desktop".to_string(),
                    identity: "dtoken-core-api".to_string(),
                    pid: std::process::id(),
                    port: DTOKEN_SERVICE_PORT,
                    exe_sha256: Some(file_sha256(&exe).expect("hash readiness runtime fixture")),
                    exe_path: exe.display().to_string(),
                    process_birth_token: process_birth_token(std::process::id()),
                    runtime_instance_nonce: Some("test-runtime-nonce".to_string()),
                    config_path: app_state_dir()
                        .join("active-config")
                        .join("dtoken-config.json")
                        .display()
                        .to_string(),
                    data_dir: service_data_dir().display().to_string(),
                    started_at: now_label(),
                };
                write_service_manifest(&manifest).expect("write readiness service manifest");
                let result = run_product_readiness_check();
                shutdown.store(true, Ordering::SeqCst);

                assert_eq!(result["mode"], "product-readiness-check");
                assert_eq!(result["ok"], true, "{result:#}");
                assert_eq!(result["passed"], 9);
                assert_eq!(result["total"], 9);
                assert!(result["privacy"]
                    .as_str()
                    .unwrap_or_default()
                    .contains("Product action only"));
                let checks = result["checks"].as_array().expect("readiness checks");
                for expected in [
                    "install_connect",
                    "agent_detection",
                    "route_config",
                    "compression_data",
                    "hook_or_config_effect",
                    "local_transaction_state",
                    "headroom_derived_capabilities",
                    "export_redaction",
                    "doctor",
                ] {
                    assert!(
                        checks.iter().any(|check| {
                            check["id"] == expected && check["ok"].as_bool().unwrap_or(false)
                        }),
                        "missing or failed readiness check: {expected}"
                    );
                }
                assert!(!app_state_dir()
                    .join("settings")
                    .join("project-policy.json")
                    .exists());
                assert!(!app_state_dir().join("backups").exists());
            });
        });
    }

    fn start_fake_dtoken_runtime() -> Option<Arc<AtomicBool>> {
        let listener = match TcpListener::bind(("127.0.0.1", DTOKEN_SERVICE_PORT)) {
            Ok(listener) => listener,
            Err(error) if error.kind() == std::io::ErrorKind::AddrInUse => return None,
            Err(error) => panic!("bind fake d-token runtime on product port: {error}"),
        };
        listener
            .set_nonblocking(true)
            .expect("set fake runtime nonblocking");
        let shutdown = Arc::new(AtomicBool::new(false));
        let thread_shutdown = Arc::clone(&shutdown);
        thread::spawn(move || {
            while !thread_shutdown.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let mut buffer = [0_u8; 2048];
                        let read = stream.read(&mut buffer).unwrap_or_default();
                        let request = String::from_utf8_lossy(&buffer[..read]);
                        let path = request
                            .lines()
                            .next()
                            .and_then(|line| line.split_whitespace().nth(1))
                            .unwrap_or("/");
                        let body = fake_runtime_body(path);
                        let response = format!(
                            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
                            body.len(),
                            body
                        );
                        let _ = stream.write_all(response.as_bytes());
                        let _ = stream.flush();
                        let _ = stream.shutdown(Shutdown::Write);
                    }
                    Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(10));
                    }
                    Err(_) => break,
                }
            }
        });
        Some(shutdown)
    }

    fn fake_runtime_body(path: &str) -> String {
        match path {
            "/health" => json!({
                "status": "healthy",
                "component": "dtoken-core-api",
                "version": "0.1.0",
                "runtime_identity": "dtoken-runtime-local",
                "runtime_instance_nonce": "test-runtime-nonce"
            }),
            "/config/active" => json!({
                "config": {
                    "loaded": true,
                    "source": "desktop_active_config",
                    "fallback_disabled": true,
                    "route_count": 1,
                    "provider_count": 1,
                    "instance_count": 1
                }
            }),
            "/dashboard/payload" => json!({
                "source": "dtoken-owned-runtime",
                "desktop_data_source": "dtoken-local-service",
                "stats": {
                    "requests": { "total": 1 },
                    "route_overhead": { "tokens": 0, "percent": 0.0 }
                },
                "metrics": {
                    "saved_tokens": 12,
                    "latency_breakdown": {
                        "compression_ms": 1,
                        "route_adapter_ms": 1,
                        "provider_ms": 2
                    }
                },
                "route": { "id": "readiness-route" },
                "route_decisions": [{ "route": "readiness-route" }],
                "ccr": { "entries": 1, "raw_available": false },
                "memory": { "entries": 1, "raw_available": false },
                "output_shape": { "verbosity": "terse", "raw_available": false }
            }),
            "/export/json" => json!({
                "redacted": true,
                "raw_available": false,
                "privacy": "redacted runtime export"
            }),
            _ => json!({ "ok": true }),
        }
        .to_string()
    }

    #[test]
    fn headroom_derived_runtime_capability_status_uses_dtoken_dashboard_metadata() {
        let dashboard = json!({
            "ok": true,
            "payload": {
                "source": "dtoken-owned-runtime",
                "stats": { "requests": 2 },
                "metrics": { "saved_tokens": 12 },
                "ccr": { "raw_available": false },
                "memory": { "raw_available": false },
                "output_shape": { "raw_available": false },
                "route": { "id": "agent-route" }
            }
        });
        let result = headroom_derived_runtime_capability_status(&dashboard);
        assert_eq!(result["ok"], true);
        assert_eq!(result["passed"], 6);
        assert_eq!(result["total"], 6);
        assert!(result["privacy"]
            .as_str()
            .unwrap_or_default()
            .contains("no external Headroom service"));
    }

    #[test]
    fn headroom_derived_capability_accepts_current_owned_runtime_shape() {
        let dashboard = json!({
            "payload": {
                "source": "dtoken-owned-runtime",
                "stats": { "route_overhead": {} },
                "cache_summary": { "raw_available": false },
                "memory": { "raw_available": false },
                "dashboard_sections": ["cli_filtering"]
            }
        });
        assert_eq!(
            headroom_derived_runtime_capability_status(&dashboard)["ok"],
            true
        );
    }

    #[test]
    fn route_profile_draft_rejects_plaintext_secret_refs() {
        with_isolated_app_state("route-profile", || {
            let result = save_route_profile_draft(RouteProfileDraftRequest {
                route_id: "opencode-real".to_string(),
                route_name: "OpenCode Real".to_string(),
                provider_id: "opencode-zen".to_string(),
                provider_name: "OpenCode Zen".to_string(),
                provider_kind: "custom".to_string(),
                protocol: "openai".to_string(),
                base_url: "https://opencode.ai/zen/v1".to_string(),
                api_key_ref: Some("plaintext-secret-value".to_string()),
                models: vec!["deepseek-v4-flash-free".to_string()],
                chain_mode: "direct".to_string(),
                compression_policy: "balanced".to_string(),
                local_port: DTOKEN_SERVICE_PORT,
                agent: "OpenCode".to_string(),
            });

            assert!(!result.ok);
            assert!(result.summary.contains("credential reference"));
            assert_eq!(list_route_profile_drafts()["count"], 0);
        });
    }

    #[test]
    fn route_profile_draft_rejects_non_opaque_credential_references() {
        with_isolated_app_state("route-profile-invalid-ref", || {
            let result = save_route_profile_draft(RouteProfileDraftRequest {
                route_id: "secured-route".to_string(),
                route_name: "Secured route".to_string(),
                provider_id: "secured-provider".to_string(),
                provider_name: "Secured provider".to_string(),
                provider_kind: "official".to_string(),
                protocol: "openai".to_string(),
                base_url: "https://api.example.com/v1".to_string(),
                api_key_ref: Some("plain-token-value-without-sk-prefix".to_string()),
                models: vec!["model".to_string()],
                chain_mode: "direct".to_string(),
                compression_policy: "balanced".to_string(),
                local_port: DTOKEN_SERVICE_PORT,
                agent: "OpenCode".to_string(),
            });

            assert!(!result.ok);
            assert!(result.summary.contains("credential reference"));
            assert_eq!(list_route_profile_drafts()["count"], 0);
        });
    }

    #[test]
    fn route_apply_requires_a_current_explicit_review() {
        with_isolated_app_state("route-review", || {
            assert!(save_test_route("reviewed-route", "https://one.example.test/v1").ok);

            let unreviewed = apply_route_profile_draft(ApplyRouteDraftRequest {
                route_id: "reviewed-route".to_string(),
                reviewed_before_sha256: None,
                reviewed_after_sha256: None,
            });
            assert!(!unreviewed.ok);
            assert!(!active_config_path().exists());
            assert!(!config_backup_root().exists());

            let stale_review = preview_route_profile_draft_apply(RouteDraftReviewRequest {
                route_id: "reviewed-route".to_string(),
            });
            assert_eq!(stale_review["ok"], true);
            assert!(!active_config_path().exists());
            assert!(!config_backup_root().exists());

            assert!(save_test_route("reviewed-route", "https://two.example.test/v1").ok);
            let stale_apply = apply_route_profile_draft(ApplyRouteDraftRequest {
                route_id: "reviewed-route".to_string(),
                reviewed_before_sha256: stale_review["before_sha256"].as_str().map(str::to_string),
                reviewed_after_sha256: stale_review["after_sha256"].as_str().map(str::to_string),
            });
            assert!(!stale_apply.ok);
            assert!(stale_apply.summary.contains("stale"));
            assert!(!active_config_path().exists());

            let applied = review_and_apply_route("reviewed-route");
            assert!(applied.ok);
            assert_eq!(
                active_route_config_status()["route"]["id"],
                "reviewed-route"
            );
            assert_eq!(rollback_points_result().expect("rollback points").len(), 1);
            let drafts = route_profile_drafts_result().expect("resolved drafts");
            let reviewed = drafts
                .iter()
                .find(|draft| {
                    draft.pointer("/config/routes/0/id").and_then(Value::as_str)
                        == Some("reviewed-route")
                })
                .expect("reviewed route draft");
            assert_eq!(
                reviewed.pointer("/resolution/state"),
                Some(&json!("applied"))
            );
            assert!(changes_from_route_drafts(&drafts).is_empty());
        });
    }

    #[test]
    fn invalid_historical_backup_does_not_hide_verified_rollback_points() {
        with_isolated_app_state("rollback-list-partial", || {
            assert!(save_test_route("reviewed-route", "https://one.example.test/v1").ok);
            assert!(review_and_apply_route("reviewed-route").ok);

            let invalid_id = "00000000-0000-4000-8000-000000000001";
            fs::create_dir_all(config_backup_root().join(invalid_id))
                .expect("invalid historical backup fixture");

            let points = rollback_points_result().expect("partial rollback list remains readable");
            assert!(points.iter().any(|point| {
                point["verified"] == true && point["scope"] == "dtoken-active-config"
            }));
            assert!(points.iter().any(|point| {
                point["id"] == invalid_id
                    && point["verified"] == false
                    && point["scope"] == "invalid-backup-evidence"
            }));
        });
    }

    #[test]
    fn rollback_preview_rejects_concurrent_edits_and_unknown_or_legacy_evidence() {
        with_isolated_app_state("rollback-preflight", || {
            let target = fs::canonicalize(app_state_dir())
                .expect("canonical app state")
                .join("opencode.json");
            let backup = apply_test_config_change(
                &target,
                r#"{"provider":"original"}"#,
                r#"{"provider":"dtoken"}"#,
                ConfigToolKind::Agent,
            );
            let preview = rollback_preview_value(&backup.rollback_id);
            assert_eq!(preview["ok"], true, "preview={preview}");

            let unconfirmed = apply_rollback(ApplyRollbackRequest {
                rollback_id: backup.rollback_id.clone(),
                reviewed_revision: preview["revision"]
                    .as_str()
                    .expect("preview revision")
                    .to_string(),
                expected_current_sha256: preview["expected_current_sha256"]
                    .as_str()
                    .expect("preview current hash")
                    .to_string(),
                confirmed: false,
            });
            assert_eq!(unconfirmed["ok"], false);
            assert!(unconfirmed["summary"]
                .as_str()
                .is_some_and(|summary| summary.contains("confirmation")));

            fs::write(&target, r#"{"provider":"concurrent"}"#).expect("simulate concurrent edit");
            let result = apply_rollback(ApplyRollbackRequest {
                rollback_id: backup.rollback_id,
                reviewed_revision: preview["revision"]
                    .as_str()
                    .expect("preview revision")
                    .to_string(),
                expected_current_sha256: preview["expected_current_sha256"]
                    .as_str()
                    .expect("preview current hash")
                    .to_string(),
                confirmed: true,
            });
            assert_eq!(result["ok"], false);
            assert!(result["summary"]
                .as_str()
                .is_some_and(|summary| summary.contains("stale")));
            assert_eq!(
                fs::read_to_string(&target).expect("concurrent state remains"),
                r#"{"provider":"concurrent"}"#
            );

            let unknown = fs::canonicalize(app_state_dir())
                .expect("canonical app state")
                .join("outside-allowlist.txt");
            let unknown_backup =
                apply_test_config_change(&unknown, "before\n", "after\n", ConfigToolKind::Agent);
            let unknown_preview = rollback_preview_value(&unknown_backup.rollback_id);
            assert_eq!(unknown_preview["integrity_verified"], true);
            assert_eq!(unknown_preview["scope"], "unknown");
            assert_eq!(unknown_preview["currently_restorable"], false);

            fs::create_dir_all(app_state_dir().join("rollback-points"))
                .expect("legacy rollback directory");
            write_json_app_state(
                &["rollback-points", "legacy.json"],
                &json!({"target": "OpenCode", "createdAt": "local", "ok": true}),
            )
            .expect("legacy rollback evidence");
            let legacy = rollback_points_result()
                .expect("rollback list")
                .into_iter()
                .find(|point| point["id"] == "legacy")
                .expect("legacy point");
            assert_eq!(legacy["integrity_verified"], false);
            assert_eq!(legacy["currently_restorable"], false);

            let invalid_id = "00000000-0000-4000-8000-000000000002";
            fs::create_dir_all(config_backup_root().join(invalid_id))
                .expect("corrupt backup directory");
            let corrupt = rollback_preview_value(invalid_id);
            assert_eq!(corrupt["ok"], false);
            assert_eq!(corrupt["integrity_verified"], false);
        });
    }

    #[test]
    fn rollback_apply_creates_a_reusable_reverse_backup_and_persists_receipt() {
        with_isolated_app_state("rollback-reverse", || {
            let target = fs::canonicalize(app_state_dir())
                .expect("canonical app state")
                .join("opencode.json");
            let backup = apply_test_config_change(
                &target,
                r#"{"provider":"original"}"#,
                r#"{"provider":"dtoken"}"#,
                ConfigToolKind::Agent,
            );
            let preview = rollback_preview_value(&backup.rollback_id);
            let result = apply_rollback(ApplyRollbackRequest {
                rollback_id: backup.rollback_id.clone(),
                reviewed_revision: preview["revision"]
                    .as_str()
                    .expect("preview revision")
                    .to_string(),
                expected_current_sha256: preview["expected_current_sha256"]
                    .as_str()
                    .expect("preview current hash")
                    .to_string(),
                confirmed: true,
            });
            assert_eq!(result["ok"], true, "result={result}");
            assert_eq!(result["verification_state"], "verified");
            assert_eq!(
                fs::read_to_string(&target).expect("restored original"),
                r#"{"provider":"original"}"#
            );

            let reverse_id = result["reverse_rollback_id"]
                .as_str()
                .expect("reverse rollback id");
            let reverse_preview = rollback_preview_value(reverse_id);
            assert_eq!(reverse_preview["ok"], true, "preview={reverse_preview}");
            assert_eq!(reverse_preview["scope"], "opencode-config");
            assert!(desktop_transaction_events().iter().any(|event| {
                event["stage"] == "rollback_apply"
                    && event["metadata"]["rollback_id"] == backup.rollback_id
                    && event["metadata"]["reverse_rollback_id"] == reverse_id
            }));
        });
    }

    #[test]
    fn reverse_backup_can_recover_a_file_removed_by_rollback() {
        with_isolated_app_state("rollback-reverse-missing", || {
            let target = fs::canonicalize(app_state_dir())
                .expect("canonical app state")
                .join("opencode.json");
            let patcher = config_patcher();
            let prepared = patcher
                .prepare_change(
                    &target,
                    r#"{"provider":"dtoken"}"#,
                    ConfigToolKind::Agent,
                    None,
                    RiskLevel::Medium,
                )
                .expect("prepare creation");
            let backup = patcher
                .backup_prepared(&prepared)
                .expect("backup missing state");
            patcher
                .apply_prepared(&prepared, &backup)
                .expect("create config");

            let preview = rollback_preview_value(&backup.rollback_id);
            let removed = apply_rollback(ApplyRollbackRequest {
                rollback_id: backup.rollback_id,
                reviewed_revision: preview["revision"]
                    .as_str()
                    .expect("preview revision")
                    .to_string(),
                expected_current_sha256: preview["expected_current_sha256"]
                    .as_str()
                    .expect("preview current hash")
                    .to_string(),
                confirmed: true,
            });
            assert_eq!(removed["ok"], true, "result={removed}");
            assert!(!target.exists());

            let reverse_id = removed["reverse_rollback_id"]
                .as_str()
                .expect("reverse rollback id");
            let reverse_preview = rollback_preview_value(reverse_id);
            assert_eq!(reverse_preview["ok"], true, "preview={reverse_preview}");
            assert_eq!(reverse_preview["expected_current_state"], "missing");
            assert_eq!(reverse_preview["expected_current_sha256"], "missing");

            let recovered = apply_rollback(ApplyRollbackRequest {
                rollback_id: reverse_id.to_string(),
                reviewed_revision: reverse_preview["revision"]
                    .as_str()
                    .expect("reverse revision")
                    .to_string(),
                expected_current_sha256: "missing".to_string(),
                confirmed: true,
            });
            assert_eq!(recovered["ok"], true, "result={recovered}");
            assert_eq!(
                fs::read_to_string(target).expect("recovered created config"),
                r#"{"provider":"dtoken"}"#
            );
        });
    }

    #[test]
    fn active_config_rollback_restores_file_and_reports_runtime_offline() {
        with_isolated_app_state("rollback-runtime-offline", || {
            assert!(save_test_route("first-route", "https://one.example.test/v1").ok);
            assert!(review_and_apply_route("first-route").ok);
            assert!(save_test_route("second-route", "https://two.example.test/v1").ok);
            assert!(review_and_apply_route("second-route").ok);

            let point = rollback_points_result()
                .expect("rollback points")
                .into_iter()
                .find(|point| {
                    point["scope"] == "dtoken-active-config"
                        && point["currently_restorable"] == true
                })
                .expect("active config rollback point");
            let rollback_id = point["id"].as_str().expect("rollback id");
            let preview = rollback_preview_value(rollback_id);
            let result = apply_rollback(ApplyRollbackRequest {
                rollback_id: rollback_id.to_string(),
                reviewed_revision: preview["revision"]
                    .as_str()
                    .expect("preview revision")
                    .to_string(),
                expected_current_sha256: preview["expected_current_sha256"]
                    .as_str()
                    .expect("preview current hash")
                    .to_string(),
                confirmed: true,
            });
            assert_eq!(result["ok"], true, "result={result}");
            assert_eq!(result["verification_state"], "runtime_offline");
            assert_eq!(active_route_config_status()["route"]["id"], "first-route");
        });
    }

    #[test]
    fn specialized_transactions_use_source_executors_and_agent_points_are_aggregated() {
        with_isolated_app_state("rollback-specialized", || {
            let active_path = active_config_path();
            let active_parent = active_path.parent().expect("active parent");
            fs::create_dir_all(active_parent).expect("create active config parent");
            let active = patchable_active_config_path().expect("patchable active config");
            let optical_backup = apply_test_config_change(
                &active,
                r#"{"providers":[],"routes":[],"instances":[]}"#,
                r#"{"providers":[],"routes":[],"instances":[],"optical":{"global_policy":{"enabled":true,"default_mode":"shadow_preview","allow_active":false,"public_summary_only":true},"route_policies":[]}}"#,
                ConfigToolKind::Dtoken,
            );
            let optical = rollback_preview_value(&optical_backup.rollback_id);
            assert_eq!(optical["scope"], "optical-config", "preview={optical}");
            assert_eq!(optical["executor_kind"], "optical");
            assert_eq!(optical["integrity_verified"], true);
            assert_eq!(optical["currently_restorable"], true);

            let home = app_state_dir().join("home");
            let project = app_state_dir().join("project");
            fs::create_dir_all(&home).expect("create test home");
            fs::create_dir_all(&project).expect("create test project");
            let request = AgentIntegrationRequest {
                agent_ids: vec!["codex".to_string()],
                scope: AgentIntegrationScope::Project,
                project_root: Some(project),
                bridge_path: env::current_exe().expect("resolve absolute test bridge path"),
                install_instructions: true,
                install_hook_or_plugin: true,
                install_skill: true,
                install_mcp: false,
            };
            let prepared = prepare_agent_integration(&request, &home, &config_backup_root())
                .expect("prepare Agent integration");
            let digest = prepared.plan.review_digest.clone();
            let receipt = apply_agent_integration(prepared, &config_backup_root(), &digest)
                .expect("apply Agent integration");
            let agent_id = receipt.rollback_ids.first().expect("Agent rollback id");
            assert!(receipt.rollback_ids.len() > 1);
            let points = rollback_points_result().expect("transaction rollback points");
            let agent_points = points
                .iter()
                .filter(|point| point["scope"] == "agent-integration")
                .collect::<Vec<_>>();
            assert_eq!(agent_points.len(), 1, "points={points:?}");
            assert_eq!(
                agent_points[0]["files"].as_u64(),
                Some(receipt.rollback_ids.len() as u64)
            );
            assert_eq!(agent_points[0]["id"], agent_id.as_str());
            let agent = rollback_preview_value(agent_id);
            assert_eq!(agent["scope"], "agent-integration", "preview={agent}");
            assert_eq!(agent["executor_kind"], "agent_integration");
            assert_eq!(agent["transaction_id"], receipt.transaction_id);
            assert_eq!(agent["integrity_verified"], true);
            assert_eq!(agent["currently_restorable"], true);

            let restored = apply_rollback(ApplyRollbackRequest {
                rollback_id: agent_id.clone(),
                reviewed_revision: agent["revision"]
                    .as_str()
                    .expect("Agent rollback revision")
                    .to_string(),
                expected_current_sha256: agent["expected_current_sha256"]
                    .as_str()
                    .expect("Agent current transaction hash")
                    .to_string(),
                confirmed: true,
            });
            assert_eq!(restored["ok"], true, "receipt={restored}");
            assert_eq!(restored["verification_state"], "verified");
            assert_eq!(restored["transaction_id"], receipt.transaction_id);
            assert_eq!(
                restored["reverse_rollback_ids"].as_array().map(Vec::len),
                Some(receipt.rollback_ids.len())
            );
        });
    }

    #[test]
    fn rollback_restore_reinstates_previous_dtoken_active_config() {
        with_isolated_app_state("rollback", || {
            let first = save_route_profile_draft(RouteProfileDraftRequest {
                route_id: "first-route".to_string(),
                route_name: "First Route".to_string(),
                provider_id: "first-provider".to_string(),
                provider_name: "First Provider".to_string(),
                provider_kind: "custom".to_string(),
                protocol: "openai".to_string(),
                base_url: "https://first.example.test/v1".to_string(),
                api_key_ref: None,
                models: vec!["mock/model".to_string()],
                chain_mode: "direct".to_string(),
                compression_policy: "balanced".to_string(),
                local_port: DTOKEN_SERVICE_PORT,
                agent: "OpenCode".to_string(),
            });
            assert!(first.ok);
            assert!(review_and_apply_route("first-route").ok);

            let second = save_route_profile_draft(RouteProfileDraftRequest {
                route_id: "second-route".to_string(),
                route_name: "Second Route".to_string(),
                provider_id: "second-provider".to_string(),
                provider_name: "Second Provider".to_string(),
                provider_kind: "custom".to_string(),
                protocol: "openai".to_string(),
                base_url: "https://second.example.test/v1".to_string(),
                api_key_ref: None,
                models: vec!["mock/model".to_string()],
                chain_mode: "direct".to_string(),
                compression_policy: "balanced".to_string(),
                local_port: DTOKEN_SERVICE_PORT,
                agent: "OpenCode".to_string(),
            });
            assert!(second.ok);
            let second_review = preview_route_profile_draft_apply(RouteDraftReviewRequest {
                route_id: "second-route".to_string(),
            });
            assert_eq!(second_review["ok"], true);
            let second_after = second_review["after_sha256"]
                .as_str()
                .expect("second route after hash")
                .to_string();
            assert!(
                apply_route_profile_draft(ApplyRouteDraftRequest {
                    route_id: "second-route".to_string(),
                    reviewed_before_sha256: second_review["before_sha256"]
                        .as_str()
                        .map(str::to_string),
                    reviewed_after_sha256: Some(second_after.clone()),
                })
                .ok
            );
            assert_eq!(active_route_config_status()["route"]["id"], "second-route");

            let second_path = app_state_dir().join("routing").join("second-route.json");
            let mut legacy_applied: Value = serde_json::from_str(
                &fs::read_to_string(&second_path).expect("read applied legacy draft"),
            )
            .expect("parse applied legacy draft");
            legacy_applied
                .as_object_mut()
                .expect("draft object")
                .remove("resolution");
            write_json_app_state(&["routing", "second-route.json"], &legacy_applied)
                .expect("simulate pre-lifecycle applied draft");

            let points = rollback_points_result().expect("read rollback points");
            let rollback_id = points
                .iter()
                .find(|point| {
                    point.get("expectedCurrentSha256").and_then(Value::as_str)
                        == Some(second_after.as_str())
                })
                .and_then(|point| point.get("id").and_then(Value::as_str))
                .expect("second route rollback point should exist")
                .to_string();
            let restored = manage_backup("restore".to_string(), rollback_id);
            assert!(restored.ok);
            assert!(restored.summary.contains("active config restored"));
            assert_eq!(active_route_config_status()["route"]["id"], "first-route");
            let drafts = route_profile_drafts_result().expect("migrated route drafts");
            let second = drafts
                .iter()
                .find(|draft| {
                    draft.pointer("/config/routes/0/id").and_then(Value::as_str)
                        == Some("second-route")
                })
                .expect("second route draft");
            assert_eq!(second.pointer("/resolution/state"), Some(&json!("applied")));
            assert_eq!(
                second.pointer("/resolution/evidence"),
                Some(&json!("verified-config-backup-after-hash"))
            );
            assert!(changes_from_route_drafts(&drafts).is_empty());
        });
    }

    #[test]
    fn opencode_rollback_reports_the_restored_scope() {
        with_isolated_app_state("opencode-rollback-copy", || {
            let target = fs::canonicalize(app_state_dir())
                .expect("canonical app state")
                .join("opencode.json");
            fs::write(&target, r#"{"provider":"original"}"#).expect("original config");
            let patcher = config_patcher();
            let change = patcher
                .prepare_change(
                    &target,
                    r#"{"provider":"dtoken"}"#,
                    ConfigToolKind::Agent,
                    None,
                    RiskLevel::Medium,
                )
                .expect("prepare OpenCode config");
            let backup = patcher.backup_prepared(&change).expect("backup");
            patcher.apply_prepared(&change, &backup).expect("apply");

            let restored = manage_backup("restore".to_string(), backup.rollback_id);
            assert!(restored.ok);
            assert_eq!(
                restored.summary,
                "OpenCode configuration restored exactly from verified backup"
            );
            assert_eq!(
                fs::read_to_string(target).expect("restored OpenCode config"),
                r#"{"provider":"original"}"#
            );
        });
    }

    #[test]
    fn project_policy_settings_round_trip_merges_payloads() {
        with_isolated_app_state("project-policy", || {
            let first = save_app_settings(
                "project-policy".to_string(),
                json!({
                    "policy": {
                        "project_id": "desktop-project",
                        "level": "balanced",
                        "enable_ccr": true,
                        "enable_memory": true
                    },
                    "privacy": "aggregate runtime metrics only"
                }),
            );
            assert!(first.ok);

            let second = save_app_settings(
                "project-policy".to_string(),
                json!({
                    "policy": {
                        "enable_output_shaper": true,
                        "holdout_ratio": 0.1
                    },
                    "source": "dtoken-project-policy"
                }),
            );
            assert!(second.ok);

            let saved = get_app_settings("project-policy".to_string());
            assert_eq!(saved["state"], "loaded");
            assert_eq!(saved["payload"]["policy"]["project_id"], "desktop-project");
            assert_eq!(saved["payload"]["policy"]["level"], "balanced");
            assert_eq!(saved["payload"]["policy"]["enable_ccr"], true);
            assert_eq!(saved["payload"]["policy"]["enable_output_shaper"], true);
            assert_eq!(saved["payload"]["policy"]["holdout_ratio"], 0.1);
            assert_eq!(saved["payload"]["source"], "dtoken-project-policy");
        });
    }

    #[test]
    fn learning_roots_reject_filesystem_and_home_scopes() {
        let temp = env::temp_dir();
        let filesystem_root = temp.ancestors().last().expect("filesystem root");
        assert!(!learning_allowed_root(filesystem_root));
        if let Some(home) = env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }) {
            assert!(!learning_allowed_root(&PathBuf::from(home)));
        }
    }

    #[test]
    fn learning_managed_block_preserves_user_content_and_creates_rollback() {
        with_isolated_app_state("learning-write", || {
            let root = app_state_dir().join("sample-project");
            fs::create_dir_all(&root).expect("create sample project");
            let target = root.join("AGENTS.md");
            let original = b"# User instructions\n\nKeep this line.\n";
            fs::write(&target, original).expect("write target");
            let review = preview_learning_managed_block(LearningManagedBlockRequest {
                target: target.display().to_string(),
                content: "Use the balanced local policy.".to_string(),
                reviewed_before_sha256: None,
                reviewed_after_sha256: None,
            });
            assert_eq!(review["ok"], true, "review={review}");
            let applied = apply_learning_managed_block(LearningManagedBlockRequest {
                target: target.display().to_string(),
                content: "Use the balanced local policy.".to_string(),
                reviewed_before_sha256: review["before_sha256"].as_str().map(str::to_string),
                reviewed_after_sha256: review["after_sha256"].as_str().map(str::to_string),
            });
            assert_eq!(applied["ok"], true);
            assert_eq!(applied["verified"], true);
            assert!(applied["rollback_id"]
                .as_str()
                .is_some_and(|id| !id.is_empty()));
            let written = fs::read_to_string(&target).expect("read target");
            assert!(written.contains("# User instructions"));
            assert!(written.contains("Keep this line."));
            assert!(written.contains("<!-- d-token:learning:start -->"));
            assert!(written.contains("Use the balanced local policy."));

            let rollback_id = applied["rollback_id"]
                .as_str()
                .expect("learning write rollback id")
                .to_string();
            let restored = manage_backup("restore".to_string(), rollback_id);
            assert!(restored.ok);
            assert_eq!(
                restored.summary,
                "Learning document restored exactly from verified backup"
            );
            assert_eq!(fs::read(&target).expect("read restored target"), original);
        });
    }

    #[test]
    fn learning_delegation_rejects_non_allowlisted_agents_without_spawn() {
        let result = spawn_learning_delegation(LearningDelegationRequest {
            request_id: "learning:test-invalid-agent".to_string(),
            agent: "sh".to_string(),
            project_root: env::current_dir().unwrap().display().to_string(),
            prompt: "metadata-only test".to_string(),
            timeout_ms: 1_000,
            confirmed: true,
        });
        assert_eq!(result["ok"], false);
        assert_eq!(result["state"], "blocked");
    }

    #[test]
    fn learning_delegation_accepts_only_bounded_structured_context() {
        let raw = json!({
            "type": "result",
            "result": "{\"summary\":\"Rust desktop control plane\",\"conventions\":[\"Run workspace tests\"],\"risks\":[\"Keep route writes transactional\"],\"recommendations\":[\"Preserve explicit provider selection\"],\"open_questions\":[]}"
        })
        .to_string();
        let context = delegated_context_from_text(&raw).expect("structured context");
        assert_eq!(context.summary, "Rust desktop control plane");
        assert_eq!(context.conventions, vec!["Run workspace tests"]);

        let secret = r#"{"summary":"Authorization: Bearer secret","conventions":[],"risks":[],"recommendations":[],"open_questions":[]}"#;
        assert!(delegated_context_from_text(secret).is_none());
        assert!(delegated_context_from_text(&"x".repeat(64 * 1024 + 1)).is_none());
    }

    #[test]
    fn learning_delegation_cancel_is_fail_closed_when_not_running() {
        let result = cancel_learning_delegation("learning:not-running".to_string());
        assert_eq!(result["ok"], false);
        assert_eq!(result["state"], "not_running");
    }

    #[test]
    fn doctor_requires_opencode_but_not_optional_headroom() {
        let tools = vec![
            ToolCheck {
                id: "rtk".to_string(),
                label: "RTK".to_string(),
                installed: true,
                version: None,
                path: None,
                status: "installed".to_string(),
                detail: String::new(),
            },
            ToolCheck {
                id: "headroom".to_string(),
                label: "Headroom".to_string(),
                installed: false,
                version: None,
                path: None,
                status: "missing".to_string(),
                detail: String::new(),
            },
            ToolCheck {
                id: "opencode".to_string(),
                label: "OpenCode".to_string(),
                installed: true,
                version: None,
                path: None,
                status: "installed".to_string(),
                detail: String::new(),
            },
        ];
        assert!(doctor_agent_ready(&tools));
    }

    #[test]
    fn doctor_repair_rejects_stale_unconfirmed_and_non_allowlisted_requests() {
        with_isolated_app_state("doctor-repair-guards", || {
            let report = DoctorReport {
                ok: false,
                mode_name: "doctor-report-v2".to_string(),
                revision: "doctor-current".to_string(),
                source: "test".to_string(),
                freshness: "fresh".to_string(),
                collected_at: now_timestamp(),
                summary: "repairable issue".to_string(),
                counts: DoctorCounts {
                    healthy: 0,
                    repairable: 1,
                    manual: 0,
                    blocked: 0,
                },
                issues: vec![DoctorIssue {
                    id: "service-not-running".to_string(),
                    category: "runtime".to_string(),
                    severity: "error".to_string(),
                    state: "repairable".to_string(),
                    title: "service unavailable".to_string(),
                    impact: "requests are unavailable".to_string(),
                    cause: "managed runtime is stopped".to_string(),
                    evidence: "runtime identity is absent".to_string(),
                    remediation: Some(DoctorRemediation {
                        kind: "restart_managed_service".to_string(),
                        label: "restart".to_string(),
                        risk: "low".to_string(),
                        writes_config: false,
                        requires_confirmation: true,
                        destination: None,
                    }),
                }],
            };
            write_json_app_state(
                &["doctor", "latest.json"],
                &serde_json::to_value(report).expect("serialize Doctor report"),
            )
            .expect("persist Doctor report");

            let stale = preview_doctor_common_repair(DoctorRepairRequest {
                report_revision: "doctor-stale".to_string(),
                issue_ids: vec!["service-not-running".to_string()],
                confirmed: None,
            });
            assert_eq!(stale["ok"], false);
            assert!(stale["summary"]
                .as_str()
                .is_some_and(|summary| summary.contains("changed")));

            let unconfirmed = apply_doctor_common_repair(DoctorRepairRequest {
                report_revision: "doctor-current".to_string(),
                issue_ids: vec!["service-not-running".to_string()],
                confirmed: Some(false),
            });
            assert_eq!(unconfirmed["ok"], false);
            assert!(unconfirmed["summary"]
                .as_str()
                .is_some_and(|summary| summary.contains("confirmation")));

            let mut snapshot =
                read_app_state(&["doctor", "latest.json"]).expect("read Doctor report");
            snapshot["issues"][0]["remediation"]["kind"] = json!("open_install");
            write_json_app_state(&["doctor", "latest.json"], &snapshot)
                .expect("persist non-allowlisted repair");
            let blocked = preview_doctor_common_repair(DoctorRepairRequest {
                report_revision: "doctor-current".to_string(),
                issue_ids: vec!["service-not-running".to_string()],
                confirmed: None,
            });
            assert_eq!(blocked["ok"], false);
            assert!(blocked["summary"]
                .as_str()
                .is_some_and(|summary| summary.contains("allowlist")));
        });
    }

    #[test]
    fn doctor_v3_freshness_is_dynamic_and_fails_closed_on_overflow() {
        let now = epoch_ms();
        let fresh = json!({
            "schema_version": 3,
            "collected_at_ms": now,
            "ttl_ms": 1_000,
        });
        assert!(doctor_v3_snapshot_is_fresh(&fresh, now));
        assert!(!doctor_v3_snapshot_is_fresh(
            &fresh,
            now.saturating_add(1_001)
        ));
        assert!(!doctor_v3_snapshot_is_fresh(
            &json!({
                "schema_version": 3,
                "collected_at_ms": u64::MAX,
                "ttl_ms": 1,
            }),
            now,
        ));
        assert!(!doctor_v3_snapshot_is_fresh(
            &json!({
                "schema_version": 3,
                "collected_at_ms": now.saturating_add(1),
                "ttl_ms": 1_000,
            }),
            now,
        ));
        assert!(!doctor_v3_snapshot_is_fresh(
            &json!({
                "schema_version": 3,
                "collected_at_ms": now,
                "ttl_ms": 0,
            }),
            now,
        ));
    }

    #[test]
    fn doctor_v2_compatibility_snapshot_uses_dynamic_freshness() {
        let now = 10_000_000;
        let fresh = json!({
            "mode": "doctor-report-v2",
            "freshness": "fresh",
            "collected_at": "unix:9950",
        });
        let stale = json!({
            "mode": "doctor-report-v2",
            "freshness": "fresh",
            "collected_at": "unix:1",
        });
        let future = json!({
            "mode": "doctor-report-v2",
            "freshness": "fresh",
            "collected_at": "unix:10001",
        });

        assert_eq!(doctor_v2_snapshot_freshness(&fresh, now), "fresh");
        assert_eq!(doctor_v2_snapshot_freshness(&stale, now), "stale");
        assert_eq!(doctor_v2_snapshot_freshness(&future, now), "unknown");
    }

    #[test]
    fn doctor_v3_rejects_stale_snapshot_digest_mismatch_and_expired_plan() {
        with_isolated_app_state("doctor-v3-plan-guards", || {
            let selection = DoctorModuleRepairSelection {
                agent_ids: Vec::new(),
                route_ids: Vec::new(),
                scope: "global".to_string(),
                project_root: None,
                install_instructions: false,
                install_hook_or_plugin: false,
                install_skill: false,
                install_mcp: false,
            };
            write_json_app_state(
                &["doctor", "latest-v3.json"],
                &json!({
                    "schema_version": 3,
                    "revision": "doctor-v3-stale",
                    "collected_at_ms": epoch_ms().saturating_sub(10_000),
                    "ttl_ms": 1,
                }),
            )
            .expect("persist stale Doctor snapshot");
            let stale = preview_doctor_repair(PreviewDoctorModuleRepairRequest {
                snapshot_revision: "doctor-v3-stale".to_string(),
                module_id: "core_service_lifecycle".to_string(),
                selection: selection.clone(),
            });
            assert_eq!(stale["ok"], false);
            assert!(stale["error"]
                .as_str()
                .is_some_and(|error| error.contains("stale")));

            write_json_app_state(
                &["doctor", "latest-v3.json"],
                &json!({
                    "schema_version": 3,
                    "revision": "doctor-v3-current",
                    "collected_at_ms": epoch_ms(),
                    "ttl_ms": DOCTOR_V3_TTL_MS,
                }),
            )
            .expect("persist current Doctor snapshot");
            let preview = preview_doctor_repair(PreviewDoctorModuleRepairRequest {
                snapshot_revision: "doctor-v3-current".to_string(),
                module_id: "core_service_lifecycle".to_string(),
                selection: selection.clone(),
            });
            assert_eq!(preview["ok"], true, "preview={preview}");
            let plan_id = preview["plan_id"].as_str().expect("plan id").to_string();
            let digest = preview["review_digest"]
                .as_str()
                .expect("review digest")
                .to_string();
            let mismatched = apply_doctor_repair(ApplyDoctorModuleRepairRequest {
                plan_id: plan_id.clone(),
                review_digest: "doctor-review-tampered".to_string(),
                snapshot_revision: "doctor-v3-current".to_string(),
                module_id: "core_service_lifecycle".to_string(),
                selection: selection.clone(),
                confirmed: true,
            });
            assert_eq!(mismatched["ok"], false);
            assert_eq!(mismatched["outcome"], "blocked");

            let mut stored = load_doctor_plan(&plan_id).expect("stored Doctor plan");
            stored.expires_at_ms = epoch_ms().saturating_sub(1);
            store_doctor_plan(&stored).expect("expire Doctor plan");
            let expired = apply_doctor_repair(ApplyDoctorModuleRepairRequest {
                plan_id,
                review_digest: digest,
                snapshot_revision: "doctor-v3-current".to_string(),
                module_id: "core_service_lifecycle".to_string(),
                selection,
                confirmed: true,
            });
            assert_eq!(expired["ok"], false);
            assert_eq!(expired["outcome"], "blocked");
        });
    }

    #[test]
    fn doctor_v3_agent_repair_requires_existing_managed_evidence() {
        with_isolated_app_state("doctor-v3-agent-evidence", || {
            let project = app_state_dir().join("unmanaged-project");
            fs::create_dir_all(&project).expect("create unmanaged project");
            write_json_app_state(
                &["doctor", "latest-v3.json"],
                &json!({
                    "schema_version": 3,
                    "revision": "doctor-v3-agent-current",
                    "collected_at_ms": epoch_ms(),
                    "ttl_ms": DOCTOR_V3_TTL_MS,
                }),
            )
            .expect("persist current Doctor snapshot");
            let preview = preview_doctor_repair(PreviewDoctorModuleRepairRequest {
                snapshot_revision: "doctor-v3-agent-current".to_string(),
                module_id: "agent_instructions".to_string(),
                selection: DoctorModuleRepairSelection {
                    agent_ids: vec!["codex".to_string()],
                    route_ids: Vec::new(),
                    scope: "project".to_string(),
                    project_root: Some(project.display().to_string()),
                    install_instructions: true,
                    install_hook_or_plugin: false,
                    install_skill: false,
                    install_mcp: false,
                },
            });
            assert_eq!(preview["ok"], false, "preview={preview}");
            assert!(preview["error"]
                .as_str()
                .is_some_and(|error| error.contains("existing d-token managed target")));
            assert_eq!(preview["agents"][0]["state"], "blocked");
        });
    }

    #[test]
    fn desktop_snapshot_revision_changes_only_with_evidence() {
        let source = json!({
            "source": "dtoken-owned-runtime",
            "desktop_data_source": "dtoken-local-service",
            "providers": [{"id":"provider-a"}],
            "routes": [{"id":"route-a"}],
            "request_logs": []
        });
        let first = finalize_desktop_snapshot(source.clone());
        let second = finalize_desktop_snapshot(source);
        assert_eq!(first["desktop_revision"], second["desktop_revision"]);
        assert!(first["desktop_collected_at"].is_string());
    }

    #[test]
    fn desktop_summary_revision_uses_slice_revisions_instead_of_serializing_log_content() {
        let source = json!({
            "source": "dtoken-owned-runtime",
            "desktop_data_source": "dtoken-local-service",
            "desktop_payload_kind": "summary",
            "slice_revisions": {"runtime_summary": 8, "request_logs": 4, "history": 8, "cache_summary": 4},
            "runtime_health": {
                "runtime_identity": "dtoken-runtime-local",
                "runtime_instance_nonce": "fixture-nonce"
            },
            "active_config": {"revision": "config-a"},
            "request_logs": [{"private_field": "first"}]
        });
        let first = finalize_desktop_snapshot(source.clone());
        let mut content_only_change = source.clone();
        content_only_change["request_logs"] = json!([{"private_field": "second"}]);
        let second = finalize_desktop_snapshot(content_only_change);
        assert_eq!(first["desktop_revision"], second["desktop_revision"]);

        let mut slice_change = source;
        slice_change["slice_revisions"]["request_logs"] = json!(5);
        let third = finalize_desktop_snapshot(slice_change);
        assert_ne!(first["desktop_revision"], third["desktop_revision"]);
    }

    #[test]
    fn unavailable_loopback_listener_fails_within_offline_budget() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("reserve local port");
        let port = listener.local_addr().expect("local address").port();
        drop(listener);
        let started = Instant::now();
        let result = simple_http_get("127.0.0.1", port, "/health");
        assert!(result.is_err());
        assert!(started.elapsed() < Duration::from_millis(500));
    }

    #[test]
    fn cc_switch_sync_distinguishes_matching_and_diverged_routes() {
        let active = vec![normalized_endpoint("http://127.0.0.1:15721/")];
        assert_eq!(
            derive_cc_switch_status(true, Some("http://127.0.0.1:15721"), &active, "loaded"),
            "synced"
        );
        assert_eq!(
            derive_cc_switch_status(true, Some("http://127.0.0.1:15722"), &active, "loaded"),
            "out-of-sync"
        );
        assert_eq!(
            derive_cc_switch_status(true, Some("http://127.0.0.1:15721"), &[], "missing"),
            "needs-confirmation"
        );
    }

    #[cfg(not(windows))]
    #[test]
    fn tool_version_probe_has_a_hard_deadline() {
        let started = Instant::now();
        let result = run_capture("sh", &["-c", "sleep 2"]);
        assert!(result.is_err());
        assert!(started.elapsed() < Duration::from_millis(1_200));
    }
}
