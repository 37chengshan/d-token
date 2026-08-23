mod agent_registry;
mod autostart;
mod codebase_memory_control;
mod learning_discovery;
mod opencode_adapter;
mod tray;
mod upstream_discovery;

use codebase_memory_control::{
    apply_codebase_memory_integration, cancel_codebase_memory_index, delete_codebase_memory_index,
    get_codebase_memory_status, open_codebase_memory_download_page, open_codebase_memory_graph,
    open_codebase_memory_package_directory, preview_codebase_memory_integration,
    rollback_codebase_memory_integration, set_codebase_memory_package_directory,
    start_codebase_memory_index, start_codebase_memory_ui, stop_codebase_memory_ui,
    verify_codebase_memory_integration,
};
use tray::show_control_plane;

use dtoken_agent_bridge::{BRIDGE_COMPONENT, BRIDGE_PROTOCOL_VERSION, BRIDGE_VERSION};
use dtoken_config::DtokenConfig;
use dtoken_config_patcher::agent_transport::{
    apply_reviewed_agent_transport, preview_clear_agent_transport,
    preview_configure_agent_transport, verify_agent_transport, AgentTransportOperation,
    AgentTransportRequest as ManagedAgentTransportRequest,
};
use dtoken_config_patcher::{
    agent::{allowed_agent_config_paths, current_process_scan_context, AgentScanContext},
    agent_integration::{
        apply_agent_integration, get_agent_integration_transaction,
        inspect_agent_integration_evidence, prepare_agent_integration, rollback_agent_integration,
        verify_agent_integration_transaction, AgentIntegrationRequest, AgentIntegrationScope,
        AgentIntegrationTransactionManifest, AgentIntegrationTransactionState, ManagedMcpLaunch,
        PreparedAgentIntegration,
    },
    agent_mcp::{
        is_verified_hook_agent, resolve_hook_config_path, resolve_mcp_config_path_for,
        transform_hook_json_document, transform_mcp_json_document, verify_hook_json_cleared,
        verify_hook_json_document, verify_mcp_json_cleared, verify_mcp_json_document,
    },
    stable_hash, ConfigPatcher, ExpectedFileState,
};
use dtoken_context_store::{
    prepare_memory_capability_transaction_for_runtime, ContextStore, MemoryAccessScope,
    MemoryCapabilityPermission, MemoryCapabilityRouteBinding, MemoryCapabilityTransactionPlan,
    MemoryCapabilityTransactionReceipt, MemoryCapabilityTransactionRequest,
    PreparedMemoryCapabilityTransaction,
};
use dtoken_doctor::{
    doctor_repair_modules, DoctorEvidence, DoctorEvidenceKind, DoctorEvidenceState,
    DoctorModuleEvidence, DoctorModuleId, DoctorRepairOutcome,
    DoctorRepairPlan as DoctorDomainPlan, DoctorRepairReceipt as DoctorDomainReceipt,
    DoctorRepairScope, DoctorRepairSelection, DoctorSnapshotSource,
    DoctorSnapshotV3 as DoctorDomainSnapshot, DoctorVerificationState,
};
use dtoken_event_ledger::JsonlStore;
use dtoken_preprocessor::{
    inspect_managed_rtk_runtime, uninstall_managed_rtk_runtime, ManagedHookPaths,
    ManagedRtkLifecycle, MAX_MANAGED_RTK_OUTPUT_BYTES, MIN_MANAGED_RTK_OUTPUT_BYTES,
};
use dtoken_process::{
    executable_identity_matches, is_process_running, listener_owned_by_pid, process_birth_token,
    sanitize_command_output, terminate_process, ProcessIdentity, ProcessManager,
    SystemProcessInspector,
};
#[cfg(debug_assertions)]
use dtoken_process::{opencode_command_plan, run_opencode_model_test};
use dtoken_types::{
    agent::{
        AgentAutoFollowAuthorization, AgentRouteFollowBinding, AgentRouteSyncReceipt,
        CcSwitchAutoFollowAuthorization, CcSwitchRouteSyncReceipt, HostPlatform, UpstreamSignature,
    },
    ConfigToolKind, RiskLevel,
};
use opencode_adapter::{
    OpenCodeConnectionReceipt, OpenCodeConnectionRequest, OpenCodeConnectionReview,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use specta::Type;
use std::{
    collections::{BTreeMap, HashMap, HashSet, VecDeque},
    env, fs,
    io::{Read, Write},
    net::{SocketAddr, TcpStream},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, OnceLock,
    },
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
#[cfg(any(debug_assertions, test))]
use tauri_specta::collect_commands;
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
const CC_SWITCH_AUTO_FOLLOW_SCHEMA_VERSION: u8 = 2;
const AGENT_AUTO_FOLLOW_SCHEMA_VERSION: u8 = 1;
const CC_SWITCH_AUTO_FOLLOW_POLL_INTERVAL: Duration = Duration::from_secs(10);
const CC_SWITCH_AUTO_FOLLOW_DEBOUNCE: Duration = Duration::from_millis(750);
static CC_SWITCH_AUTO_FOLLOW_COORDINATOR: OnceLock<Arc<CcSwitchAutoFollowCoordinator>> =
    OnceLock::new();
const HYBRID_AGENT_PREVIEW_TTL_MS: u64 = 10 * 60 * 1_000;
const HYBRID_AGENT_MEMORY_CAPABILITY_TTL_MS: u64 = 30 * 24 * 60 * 60 * 1_000;
const MAX_PENDING_HYBRID_AGENT_PREVIEWS: usize = 16;
static PENDING_HYBRID_AGENT_PREVIEWS: OnceLock<
    Mutex<HashMap<String, PendingHybridAgentIntegration>>,
> = OnceLock::new();

#[derive(Debug, Clone, Serialize)]
struct ManagedAgentCapabilityBinding {
    agent_id: String,
    scope: MemoryAccessScope,
    route_binding: MemoryCapabilityRouteBinding,
    source_revision: String,
}

struct PendingHybridAgentIntegration {
    created_at_ms: u64,
    draft_digest: String,
    active_config_sha256: Option<String>,
    review_digest: String,
    agent_review_digest: String,
    agent: PreparedAgentIntegration,
    capabilities: Vec<PreparedMemoryCapabilityTransaction>,
    capability_plans: Vec<MemoryCapabilityTransactionPlan>,
}

#[derive(Clone, Serialize, Deserialize)]
struct ManagedAgentCapabilityReference {
    agent_id: String,
    transaction_id: String,
    target_path: PathBuf,
    document_sha256: String,
    review_digest: String,
    route_binding: MemoryCapabilityRouteBinding,
    expires_at_ms: u64,
}

#[derive(Clone, Serialize, Deserialize)]
struct HybridAgentCapabilityManifest {
    schema_version: u8,
    agent_transaction_id: String,
    composite_review_digest: String,
    active_config_sha256: Option<String>,
    state: String,
    capabilities: Vec<ManagedAgentCapabilityReference>,
    updated_at_ms: u64,
    failure_reason: Option<String>,
}

/// Coalesces watcher and fallback-poll notifications. The worker is deliberately
/// single-flight: config writes are serialized and every execution re-discovers
/// cc-switch before touching d-token's own route snapshot.
struct CcSwitchAutoFollowCoordinator {
    pending: AtomicBool,
    running: AtomicBool,
}

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

#[derive(Debug, Deserialize)]
struct ApplyManagedRtkRequest {
    transaction_id: String,
    reviewed_host_config_fingerprint: String,
    reviewed_bridge_sha256: String,
    max_output_bytes: Option<usize>,
    confirmed: bool,
}

#[derive(Debug, Deserialize)]
struct ClearManagedRtkRequest {
    transaction_id: String,
    reviewed_host_config_fingerprint: String,
    confirmed: bool,
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

#[derive(Clone, Serialize, Deserialize)]
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
    #[serde(default)]
    source_fingerprint: Option<String>,
    #[serde(default)]
    source_fingerprint_scope: Option<String>,
    bindings: Vec<ConnectionRouteBindingRequest>,
}

#[derive(Deserialize)]
struct ApplyConnectionRoutePlanRequest {
    plan: ConnectionRoutePlanRequest,
    reviewed_before_sha256: Option<String>,
    reviewed_after_sha256: Option<String>,
    #[serde(default)]
    enable_auto_follow: bool,
}

#[derive(Deserialize)]
struct FinalizeAgentAutoFollowRequest {
    expected_route_sha256: String,
}

#[derive(Deserialize)]
struct RollbackConnectionRoutePlanRequest {
    rollback_id: String,
    expected_after_sha256: String,
}

#[derive(Deserialize)]
struct CcSwitchSyncPlanRequest {
    /// The cc-switch-owned snapshot revision, not the aggregate Agent +
    /// cc-switch discovery revision.
    discovery_revision: String,
    source_fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CcSwitchRouteIdentity {
    upstream_signature: String,
    route_signature: String,
}

#[derive(Deserialize)]
struct ApplyCcSwitchSyncPlanRequest {
    plan: CcSwitchSyncPlanRequest,
    reviewed_before_sha256: Option<String>,
    reviewed_after_sha256: Option<String>,
    /// A user may opt in only while confirming a hash-bound initial sync.
    /// Subsequent changes are still restricted to d-token's active route.
    #[serde(default)]
    enable_auto_follow: bool,
}

#[derive(Deserialize)]
struct SetCcSwitchAutoFollowRequest {
    enabled: bool,
    discovery_revision: Option<String>,
    source_fingerprint: Option<String>,
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

fn hybrid_agent_draft_digest(draft: &HybridAgentIntegrationDraftRequest) -> Result<String, String> {
    serde_json::to_string(draft)
        .map(|encoded| stable_hash(&encoded))
        .map_err(|error| format!("Agent integration draft could not be hashed: {error}"))
}

fn hybrid_agent_composite_review_digest(
    draft_digest: &str,
    agent_review_digest: &str,
    active_config_sha256: Option<&str>,
    capabilities: &[MemoryCapabilityTransactionPlan],
) -> Result<String, String> {
    serde_json::to_string(&json!({
        "schema_version": 2,
        "draft_digest": draft_digest,
        "agent_review_digest": agent_review_digest,
        "active_config_sha256": active_config_sha256,
        "capabilities": capabilities,
    }))
    .map(|encoded| stable_hash(&encoded))
    .map_err(|error| format!("Agent integration review could not be hashed: {error}"))
}

fn pending_hybrid_agent_previews() -> &'static Mutex<HashMap<String, PendingHybridAgentIntegration>>
{
    PENDING_HYBRID_AGENT_PREVIEWS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn prune_pending_hybrid_agent_previews(
    pending: &mut HashMap<String, PendingHybridAgentIntegration>,
    now_ms: u64,
) {
    pending.retain(|_, preview| {
        now_ms.saturating_sub(preview.created_at_ms) <= HYBRID_AGENT_PREVIEW_TTL_MS
    });
    while pending.len() > MAX_PENDING_HYBRID_AGENT_PREVIEWS {
        let oldest = pending
            .iter()
            .min_by_key(|(_, preview)| preview.created_at_ms)
            .map(|(digest, _)| digest.clone());
        let Some(oldest) = oldest else {
            break;
        };
        pending.remove(&oldest);
    }
}

fn cache_pending_hybrid_agent_preview(
    prepared: PendingHybridAgentIntegration,
) -> Result<(), String> {
    let mut pending = pending_hybrid_agent_previews()
        .lock()
        .map_err(|_| "Agent integration preview cache is unavailable".to_string())?;
    prune_pending_hybrid_agent_previews(&mut pending, epoch_ms());
    let review_digest = prepared.review_digest.clone();
    pending.insert(review_digest, prepared);
    prune_pending_hybrid_agent_previews(&mut pending, epoch_ms());
    Ok(())
}

fn take_pending_hybrid_agent_preview(
    review_digest: &str,
) -> Result<PendingHybridAgentIntegration, String> {
    let mut pending = pending_hybrid_agent_previews()
        .lock()
        .map_err(|_| "Agent integration preview cache is unavailable".to_string())?;
    prune_pending_hybrid_agent_previews(&mut pending, epoch_ms());
    pending.remove(review_digest).ok_or_else(|| {
        "Agent integration preview is missing or expired; preview the current draft again"
            .to_string()
    })
}

fn active_config_snapshot() -> Result<(String, DtokenConfig, String), String> {
    let path = active_config_path();
    let active_config_sha256 = file_sha256(&path)
        .map_err(|error| format!("Active d-token config is unavailable: {error}"))?;
    let config = DtokenConfig::load_json(&path)
        .map_err(|error| format!("Active d-token config is invalid: {error:#}"))?;
    let config_json = serde_json::to_string(&config)
        .map_err(|error| format!("Active d-token config could not be normalized: {error}"))?;
    let config_revision = stable_hash(&config_json);
    Ok((active_config_sha256, config, config_revision))
}

fn validate_hybrid_agent_transaction_id(transaction_id: &str) -> Result<(), String> {
    if transaction_id.is_empty()
        || transaction_id.len() > 128
        || !transaction_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err("Agent integration transaction id is invalid".to_string());
    }
    Ok(())
}

fn hybrid_capability_manifest_path(transaction_id: &str) -> Result<PathBuf, String> {
    validate_hybrid_agent_transaction_id(transaction_id)?;
    Ok(config_backup_root()
        .join("agent-integration-capabilities")
        .join(format!("{transaction_id}.json")))
}

fn save_hybrid_capability_manifest(
    manifest: &HybridAgentCapabilityManifest,
) -> Result<PathBuf, String> {
    let path = hybrid_capability_manifest_path(&manifest.agent_transaction_id)?;
    let parent = path
        .parent()
        .ok_or_else(|| "Agent capability manifest parent is unavailable".to_string())?;
    if let Ok(metadata) = fs::symlink_metadata(parent) {
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err("Agent capability manifest parent must be a real directory".to_string());
        }
    } else {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "Create Agent capability manifest directory {}: {error}",
                parent.display()
            )
        })?;
    }
    let bytes = serde_json::to_vec_pretty(manifest)
        .map_err(|error| format!("Serialize Agent capability manifest: {error}"))?;
    durable_atomic_write(&path, &bytes)?;
    Ok(path)
}

fn load_hybrid_capability_manifest(
    transaction_id: &str,
) -> Result<Option<HybridAgentCapabilityManifest>, String> {
    let path = hybrid_capability_manifest_path(transaction_id)?;
    let bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(format!(
                "Read Agent capability manifest {}: {error}",
                path.display()
            ))
        }
    };
    let manifest: HybridAgentCapabilityManifest = serde_json::from_slice(&bytes)
        .map_err(|error| format!("Agent capability manifest is invalid: {error}"))?;
    if manifest.schema_version != 1 || manifest.agent_transaction_id != transaction_id {
        return Err("Agent capability manifest identity is invalid".to_string());
    }
    if manifest.capabilities.len() > 64 {
        return Err("Agent capability manifest exceeds the capability limit".to_string());
    }
    Ok(Some(manifest))
}

fn capability_target_is_runtime_owned(path: &Path) -> bool {
    let Some(runtime_dir) = fs::canonicalize(service_data_dir()).ok() else {
        return false;
    };
    let expected_root = runtime_dir.join("memory-capabilities");
    path.is_absolute() && path.parent() == Some(expected_root.as_path())
}

fn managed_mcp_context_store_path() -> Result<PathBuf, String> {
    fs::canonicalize(service_data_dir())
        .map(|runtime_dir| runtime_dir.join("runtime-state.sqlite3"))
        .map_err(|error| format!("Managed MCP runtime data directory is unavailable: {error}"))
}

fn current_memory_capability_route_binding(
    config: &DtokenConfig,
    reference: &ManagedAgentCapabilityReference,
    config_revision: &str,
) -> Result<MemoryCapabilityRouteBinding, String> {
    let route = config
        .find_route(&reference.route_binding.route_id)
        .ok_or_else(|| {
            format!(
                "Managed capability route {} is no longer available",
                reference.route_binding.route_id
            )
        })?;
    let provider = config
        .find_provider(&route.target_provider_id)
        .ok_or_else(|| {
            format!(
                "Managed capability provider {} is no longer available",
                route.target_provider_id
            )
        })?;
    if provider.id != reference.route_binding.provider_id {
        return Err(format!(
            "Managed capability provider changed for route {}",
            route.id
        ));
    }
    if provider.models.len() != 1
        || provider.models.first().map(String::as_str)
            != Some(reference.route_binding.model.as_str())
    {
        return Err(format!(
            "Managed capability model changed or became ambiguous for route {}",
            route.id
        ));
    }
    Ok(MemoryCapabilityRouteBinding {
        route_id: route.id.clone(),
        provider_id: provider.id.clone(),
        model: reference.route_binding.model.clone(),
        config_revision: config_revision.to_string(),
    })
}

fn revoke_hybrid_capabilities(
    store: &ContextStore,
    capabilities: &[ManagedAgentCapabilityReference],
) -> (Vec<MemoryCapabilityTransactionReceipt>, Vec<String>) {
    let mut receipts = Vec::new();
    let mut errors = Vec::new();
    for capability in capabilities.iter().rev() {
        match store.revoke_memory_capability_transaction(
            &capability.transaction_id,
            &capability.document_sha256,
            epoch_ms().max(1),
        ) {
            Ok(receipt) => receipts.push(receipt),
            Err(error) => errors.push(format!(
                "{}: {}",
                capability.transaction_id,
                sanitize_command_output(&format!("{error:#}"))
            )),
        }
    }
    (receipts, errors)
}

fn verify_hybrid_capabilities(
    manifest: &HybridAgentCapabilityManifest,
) -> (bool, Vec<Value>, Option<String>) {
    if manifest.capabilities.is_empty() {
        return (true, Vec::new(), None);
    }
    let (active_config_sha256, config, config_revision) = match active_config_snapshot() {
        Ok(snapshot) => snapshot,
        Err(error) => return (false, Vec::new(), Some(error)),
    };
    if manifest.active_config_sha256.as_deref() != Some(active_config_sha256.as_str()) {
        return (
            false,
            Vec::new(),
            Some(
                "Active d-token config changed after the Agent capability review; re-preview is required"
                    .to_string(),
            ),
        );
    }
    let store_path = match managed_mcp_context_store_path() {
        Ok(path) => path,
        Err(error) => return (false, Vec::new(), Some(error)),
    };
    let store = match ContextStore::open(store_path) {
        Ok(store) => store,
        Err(error) => {
            return (
                false,
                Vec::new(),
                Some(format!(
                    "Managed capability projection is unavailable: {}",
                    sanitize_command_output(&format!("{error:#}"))
                )),
            )
        }
    };
    let observed_at_ms = epoch_ms().max(1);
    let mut results = Vec::new();
    let mut failures = Vec::new();
    for capability in &manifest.capabilities {
        if !capability_target_is_runtime_owned(&capability.target_path) {
            failures.push(format!(
                "{}: capability target is outside the d-token runtime",
                capability.transaction_id
            ));
            continue;
        }
        let expected =
            match current_memory_capability_route_binding(&config, capability, &config_revision) {
                Ok(binding) => binding,
                Err(error) => {
                    failures.push(format!("{}: {error}", capability.transaction_id));
                    continue;
                }
            };
        match store.resolve_memory_capability_file(
            &capability.target_path,
            &expected,
            observed_at_ms,
        ) {
            Ok(resolved) if resolved.scope.agent_id == capability.agent_id => {
                results.push(json!({
                    "agent_id": capability.agent_id,
                    "transaction_id": capability.transaction_id,
                    "state": "verified",
                    "target_path": capability.target_path,
                    "document_sha256": capability.document_sha256,
                    "route_binding": resolved.route_binding,
                    "expires_at_ms": resolved.expires_at_ms,
                }));
            }
            Ok(_) => failures.push(format!(
                "{}: capability Agent binding changed",
                capability.transaction_id
            )),
            Err(error) => failures.push(format!(
                "{}: {}",
                capability.transaction_id,
                sanitize_command_output(&format!("{error:#}"))
            )),
        }
    }
    let error = (!failures.is_empty()).then(|| failures.join("; "));
    (failures.is_empty(), results, error)
}

fn managed_agent_project_identity(project_root: &Path) -> String {
    format!(
        "project-{}",
        &stable_hash(project_root.to_string_lossy().as_ref())[..32]
    )
}

fn managed_agent_session_identity(project_id: &str, agent_id: &str, route_id: &str) -> String {
    format!(
        "session-{}",
        &stable_hash(&format!("{project_id}\n{agent_id}\n{route_id}"))[..32]
    )
}

fn resolve_managed_agent_capability_bindings(
    draft: &HybridAgentIntegrationDraftRequest,
    config: &DtokenConfig,
    config_revision: &str,
) -> Result<Vec<ManagedAgentCapabilityBinding>, String> {
    if !draft.install_mcp {
        return Ok(Vec::new());
    }
    if draft.scope != "project" {
        return Err(
            "Managed SharedContext MCP requires project scope; global memory authorization is refused"
                .to_string(),
        );
    }
    let project_root = draft
        .project_root
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| "Managed SharedContext MCP requires a project root".to_string())?;
    if !project_root.is_absolute() || !project_root.is_dir() {
        return Err(
            "Managed SharedContext MCP project root must be an existing absolute directory"
                .to_string(),
        );
    }
    let project_root = fs::canonicalize(project_root).map_err(|error| {
        format!("Managed SharedContext MCP project root is unavailable: {error}")
    })?;
    let project_id = managed_agent_project_identity(&project_root);
    let source_revision = format!("managed-agent-integration-{}", &config_revision[..16]);
    let selected = draft.agent_ids.iter().collect::<HashSet<_>>();
    if selected.len() != draft.agent_ids.len() {
        return Err("Managed SharedContext MCP Agent selection contains duplicates".to_string());
    }

    let mut bindings = Vec::new();
    for agent_id in &draft.agent_ids {
        let matching_instances = config
            .instances
            .iter()
            .filter(|instance| instance.agent == *agent_id)
            .collect::<Vec<_>>();
        if matching_instances.is_empty() {
            return Err(format!(
                "Managed SharedContext MCP has no active route for Agent {agent_id}"
            ));
        }
        if matching_instances.len() != 1 {
            return Err(format!(
                "Managed SharedContext MCP found multiple active routes for Agent {agent_id}; choose one before preview"
            ));
        }
        let instance = matching_instances[0];
        let route = config.find_route(&instance.route_id).ok_or_else(|| {
            format!(
                "Managed SharedContext MCP route {} is unavailable for Agent {agent_id}",
                instance.route_id
            )
        })?;
        if route.instance_id != instance.id {
            return Err(format!(
                "Managed SharedContext MCP route/instance binding is inconsistent for Agent {agent_id}"
            ));
        }
        let provider = config
            .find_provider(&route.target_provider_id)
            .ok_or_else(|| {
                format!(
                    "Managed SharedContext MCP provider {} is unavailable for Agent {agent_id}",
                    route.target_provider_id
                )
            })?;
        if provider.models.len() != 1 {
            return Err(format!(
                "Managed SharedContext MCP model is ambiguous for Agent {agent_id}; choose exactly one model before preview"
            ));
        }
        let model = provider.models[0].trim();
        if model.is_empty() {
            return Err(format!(
                "Managed SharedContext MCP model is unavailable for Agent {agent_id}"
            ));
        }
        let route_binding = MemoryCapabilityRouteBinding {
            route_id: route.id.clone(),
            provider_id: provider.id.clone(),
            model: model.to_string(),
            config_revision: config_revision.to_string(),
        };
        bindings.push(ManagedAgentCapabilityBinding {
            agent_id: agent_id.clone(),
            scope: MemoryAccessScope {
                project_id: project_id.clone(),
                agent_id: agent_id.clone(),
                session_id: Some(managed_agent_session_identity(
                    &project_id,
                    agent_id,
                    &route.id,
                )),
            },
            route_binding,
            source_revision: source_revision.clone(),
        });
    }
    bindings.sort_by(|left, right| left.agent_id.cmp(&right.agent_id));
    Ok(bindings)
}

fn hybrid_agent_integration_request(
    draft: &HybridAgentIntegrationDraftRequest,
    managed_mcp_launches: BTreeMap<String, ManagedMcpLaunch>,
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
        managed_mcp_launches,
        install_instructions: draft.install_instructions,
        install_hook_or_plugin: draft.install_hook_or_plugin,
        install_skill: draft.install_skill,
        install_mcp: draft.install_mcp,
    })
}

fn prepare_hybrid_agent_integration(
    draft: &HybridAgentIntegrationDraftRequest,
) -> Result<PendingHybridAgentIntegration, String> {
    let draft_digest = hybrid_agent_draft_digest(draft)?;
    let (active_config_sha256, config, bindings) = if draft.install_mcp {
        let (active_config_sha256, config, config_revision) = active_config_snapshot()?;
        let bindings = resolve_managed_agent_capability_bindings(draft, &config, &config_revision)?;
        (Some(active_config_sha256), Some(config), bindings)
    } else {
        (None, None, Vec::new())
    };
    let runtime_path = if draft.install_mcp {
        Some(resolve_dtoken_proxy_exe()?)
    } else {
        None
    };
    let data_dir = service_data_dir();
    if draft.install_mcp {
        let metadata = fs::symlink_metadata(&data_dir).map_err(|error| {
            format!("Managed SharedContext MCP runtime data directory is unavailable: {error}")
        })?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(
                "Managed SharedContext MCP runtime data path must be an existing directory"
                    .to_string(),
            );
        }
    }
    let data_dir = if draft.install_mcp {
        fs::canonicalize(&data_dir).map_err(|error| {
            format!("Managed SharedContext MCP runtime data directory is unavailable: {error}")
        })?
    } else {
        data_dir
    };
    let issued_at_ms = epoch_ms().max(1);
    let expires_at_ms = issued_at_ms.saturating_add(HYBRID_AGENT_MEMORY_CAPABILITY_TTL_MS);
    let mut capabilities = Vec::new();
    let mut managed_mcp_launches = BTreeMap::new();
    for binding in bindings {
        let prepared = prepare_memory_capability_transaction_for_runtime(
            &data_dir,
            &MemoryCapabilityTransactionRequest {
                scope: binding.scope,
                route_binding: binding.route_binding.clone(),
                permissions: vec![
                    MemoryCapabilityPermission::List,
                    MemoryCapabilityPermission::Export,
                    MemoryCapabilityPermission::Write,
                    MemoryCapabilityPermission::Delete,
                ],
                source_revision: binding.source_revision,
                issued_at_ms,
                expires_at_ms,
            },
        )
        .map_err(|error| {
            format!(
                "Managed SharedContext MCP capability plan failed for {}: {error:#}",
                binding.agent_id
            )
        })?;
        managed_mcp_launches.insert(
            binding.agent_id,
            ManagedMcpLaunch {
                runtime_path: runtime_path
                    .as_ref()
                    .expect("runtime path is present when MCP is selected")
                    .clone(),
                active_config_path: fs::canonicalize(active_config_path()).map_err(|error| {
                    format!("Managed SharedContext MCP active config is unavailable: {error}")
                })?,
                data_dir: fs::canonicalize(&data_dir).map_err(|error| {
                    format!("Managed SharedContext MCP runtime data is unavailable: {error}")
                })?,
                capability_path: prepared.plan.target_path.clone(),
                route_id: prepared.plan.route_binding.route_id.clone(),
                model: prepared.plan.route_binding.model.clone(),
            },
        );
        capabilities.push(prepared);
    }
    let request = hybrid_agent_integration_request(draft, managed_mcp_launches)?;
    let home = user_home_dir()?;
    let agent = prepare_agent_integration(&request, &home, &config_backup_root())
        .map_err(|error| format!("Agent integration plan failed: {error:#}"))?;
    let capability_plans = capabilities
        .iter()
        .map(|prepared| prepared.plan.clone())
        .collect::<Vec<_>>();
    let review_digest = hybrid_agent_composite_review_digest(
        &draft_digest,
        &agent.plan.review_digest,
        active_config_sha256.as_deref(),
        &capability_plans,
    )?;
    drop(config);
    Ok(PendingHybridAgentIntegration {
        created_at_ms: issued_at_ms,
        draft_digest,
        active_config_sha256,
        review_digest,
        agent_review_digest: agent.plan.review_digest.clone(),
        agent,
        capabilities,
        capability_plans,
    })
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
        Ok(prepared) => {
            let mut plan = match serde_json::to_value(&prepared.agent.plan) {
                Ok(Value::Object(plan)) => plan,
                Ok(_) => {
                    return json!({
                        "ok": false,
                        "state": "blocked",
                        "source": "d-token-agent-integration",
                        "error": "Agent integration plan shape is invalid",
                    })
                }
                Err(error) => {
                    return json!({
                        "ok": false,
                        "state": "blocked",
                        "source": "d-token-agent-integration",
                        "error": sanitize_command_output(&error.to_string()),
                    })
                }
            };
            plan.insert("schema_version".to_string(), json!(2));
            plan.insert("review_digest".to_string(), json!(&prepared.review_digest));
            plan.insert(
                "agent_review_digest".to_string(),
                json!(&prepared.agent_review_digest),
            );
            plan.insert(
                "active_config_sha256".to_string(),
                json!(&prepared.active_config_sha256),
            );
            plan.insert(
                "capabilities".to_string(),
                json!(&prepared.capability_plans),
            );
            if let Err(error) = cache_pending_hybrid_agent_preview(prepared) {
                return json!({
                    "ok": false,
                    "state": "blocked",
                    "source": "d-token-agent-integration",
                    "error": sanitize_command_output(&error),
                });
            }
            json!({
                "ok": true,
                "state": "review",
                "source": "d-token-agent-integration",
                "plan": Value::Object(plan),
                "requires_explicit_confirmation": true,
                "privacy": "Preview only; no Agent config, backup, capability, credential, prompt, or transcript was written."
            })
        }
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
    if request.reviewed_digest.trim().is_empty() {
        return json!({
            "ok": false,
            "state": "blocked",
            "source": "d-token-agent-integration",
            "error": "Agent integration apply requires a reviewed digest",
        });
    }
    let prepared = match take_pending_hybrid_agent_preview(&request.reviewed_digest) {
        Ok(prepared) => prepared,
        Err(error) => {
            return json!({"ok": false, "state": "blocked", "error": sanitize_command_output(&error)})
        }
    };
    let current_draft_digest = match hybrid_agent_draft_digest(&request.draft) {
        Ok(digest) => digest,
        Err(error) => {
            return json!({
                "ok": false,
                "state": "blocked",
                "source": "d-token-agent-integration",
                "error": sanitize_command_output(&error),
            })
        }
    };
    if prepared.review_digest != request.reviewed_digest
        || prepared.draft_digest != current_draft_digest
    {
        return json!({
            "ok": false,
            "state": "blocked",
            "source": "d-token-agent-integration",
            "error": "Agent integration draft or review changed after preview",
        });
    }
    if let Some(expected_sha256) = prepared.active_config_sha256.as_deref() {
        match file_sha256(&active_config_path()) {
            Ok(current_sha256) if current_sha256 == expected_sha256 => {}
            Ok(_) => {
                return json!({
                    "ok": false,
                    "state": "blocked",
                    "source": "d-token-agent-integration",
                    "error": "Active d-token config changed after preview; review the current route again",
                })
            }
            Err(error) => {
                return json!({
                    "ok": false,
                    "state": "blocked",
                    "source": "d-token-agent-integration",
                    "error": sanitize_command_output(&format!(
                        "Active d-token config is unavailable: {error}"
                    )),
                })
            }
        }
    }

    let PendingHybridAgentIntegration {
        active_config_sha256,
        review_digest,
        agent_review_digest,
        agent,
        capabilities,
        ..
    } = prepared;
    let store = if capabilities.is_empty() {
        None
    } else {
        let store_path = match managed_mcp_context_store_path() {
            Ok(path) => path,
            Err(error) => {
                return json!({
                    "ok": false,
                    "state": "failed",
                    "source": "d-token-agent-integration",
                    "error": sanitize_command_output(&error),
                });
            }
        };
        match ContextStore::open(store_path) {
            Ok(store) => Some(store),
            Err(error) => {
                return json!({
                    "ok": false,
                    "state": "failed",
                    "source": "d-token-agent-integration",
                    "error": sanitize_command_output(&format!(
                        "Managed capability projection is unavailable: {error:#}"
                    )),
                })
            }
        }
    };
    let mut capability_receipts = Vec::new();
    let mut capability_references = Vec::new();
    for capability in capabilities {
        let plan = capability.plan.clone();
        let receipt = match store
            .as_ref()
            .expect("capability store exists when capability plans are present")
            .apply_memory_capability_transaction(capability, &plan.review_digest)
        {
            Ok(receipt) => receipt,
            Err(error) => {
                let (rollback_receipts, rollback_errors) = store
                    .as_ref()
                    .map(|store| revoke_hybrid_capabilities(store, &capability_references))
                    .unwrap_or_default();
                let _ = append_desktop_event(
                    "agent_integration_apply",
                    if rollback_errors.is_empty() {
                        "failed"
                    } else {
                        "partial"
                    },
                    json!({
                        "review_digest": &review_digest,
                        "capability_count": capability_references.len(),
                        "failure_stage": "capability_apply",
                        "rollback_error_count": rollback_errors.len(),
                    }),
                );
                return json!({
                    "ok": false,
                    "state": if rollback_errors.is_empty() { "failed" } else { "partial" },
                    "source": "d-token-agent-integration",
                    "error": sanitize_command_output(&format!(
                        "Managed capability apply failed: {error:#}"
                    )),
                    "capability_receipts": capability_receipts,
                    "capability_rollback_receipts": rollback_receipts,
                    "capability_rollback_errors": rollback_errors,
                });
            }
        };
        capability_references.push(ManagedAgentCapabilityReference {
            agent_id: plan.scope.agent_id.clone(),
            transaction_id: plan.transaction_id.clone(),
            target_path: plan.target_path.clone(),
            document_sha256: plan.document_sha256.clone(),
            review_digest: plan.review_digest.clone(),
            route_binding: plan.route_binding.clone(),
            expires_at_ms: plan.expires_at_ms,
        });
        capability_receipts.push(receipt);
    }

    let receipt = match apply_agent_integration(agent, &config_backup_root(), &agent_review_digest)
    {
        Ok(receipt) => receipt,
        Err(error) => {
            let (rollback_receipts, rollback_errors) = store
                .as_ref()
                .map(|store| revoke_hybrid_capabilities(store, &capability_references))
                .unwrap_or_default();
            let _ = append_desktop_event(
                "agent_integration_apply",
                if rollback_errors.is_empty() {
                    "failed"
                } else {
                    "partial"
                },
                json!({
                    "review_digest": &review_digest,
                    "capability_count": capability_references.len(),
                    "failure_stage": "agent_apply",
                    "rollback_error_count": rollback_errors.len(),
                }),
            );
            return json!({
                "ok": false,
                "state": if rollback_errors.is_empty() { "failed" } else { "partial" },
                "source": "d-token-agent-integration",
                "error": sanitize_command_output(&format!("{error:#}")),
                "capability_receipts": capability_receipts,
                "capability_rollback_receipts": rollback_receipts,
                "capability_rollback_errors": rollback_errors,
            });
        }
    };
    let verification = match verify_agent_integration_transaction(
        &receipt.transaction_id,
        &config_backup_root(),
    ) {
        Ok(verification) if verification.ok => verification,
        Ok(verification) => {
            let agent_rollback =
                rollback_agent_integration(&receipt.transaction_id, &config_backup_root());
            let (capability_rollback_receipts, capability_rollback_errors) = store
                .as_ref()
                .map(|store| revoke_hybrid_capabilities(store, &capability_references))
                .unwrap_or_default();
            let rollback_error = agent_rollback
                .as_ref()
                .err()
                .map(|error| sanitize_command_output(&format!("{error:#}")));
            return json!({
                "ok": false,
                "state": if rollback_error.is_none() && capability_rollback_errors.is_empty() { "failed" } else { "partial" },
                "source": "d-token-agent-integration",
                "error": "Agent integration post-write verification failed; rollback was attempted",
                "verification": verification,
                "agent_rollback_error": rollback_error,
                "capability_rollback_receipts": capability_rollback_receipts,
                "capability_rollback_errors": capability_rollback_errors,
            });
        }
        Err(error) => {
            let agent_rollback =
                rollback_agent_integration(&receipt.transaction_id, &config_backup_root());
            let (capability_rollback_receipts, capability_rollback_errors) = store
                .as_ref()
                .map(|store| revoke_hybrid_capabilities(store, &capability_references))
                .unwrap_or_default();
            let rollback_error = agent_rollback
                .as_ref()
                .err()
                .map(|rollback_error| sanitize_command_output(&format!("{rollback_error:#}")));
            return json!({
                "ok": false,
                "state": if rollback_error.is_none() && capability_rollback_errors.is_empty() { "failed" } else { "partial" },
                "source": "d-token-agent-integration",
                "error": sanitize_command_output(&format!(
                    "Agent integration verification failed: {error:#}"
                )),
                "agent_rollback_error": rollback_error,
                "capability_rollback_receipts": capability_rollback_receipts,
                "capability_rollback_errors": capability_rollback_errors,
            });
        }
    };

    if let Some(expected_sha256) = active_config_sha256.as_deref() {
        if file_sha256(&active_config_path()).as_deref() != Ok(expected_sha256) {
            let agent_rollback =
                rollback_agent_integration(&receipt.transaction_id, &config_backup_root());
            let (capability_rollback_receipts, capability_rollback_errors) = store
                .as_ref()
                .map(|store| revoke_hybrid_capabilities(store, &capability_references))
                .unwrap_or_default();
            let rollback_error = agent_rollback
                .as_ref()
                .err()
                .map(|error| sanitize_command_output(&format!("{error:#}")));
            return json!({
                "ok": false,
                "state": if rollback_error.is_none() && capability_rollback_errors.is_empty() { "failed" } else { "partial" },
                "source": "d-token-agent-integration",
                "error": "Active d-token config changed during apply; Agent and capability rollback was attempted",
                "agent_rollback_error": rollback_error,
                "capability_rollback_receipts": capability_rollback_receipts,
                "capability_rollback_errors": capability_rollback_errors,
            });
        }
    }

    let manifest_path = if capability_references.is_empty() {
        None
    } else {
        let manifest = HybridAgentCapabilityManifest {
            schema_version: 1,
            agent_transaction_id: receipt.transaction_id.clone(),
            composite_review_digest: review_digest.clone(),
            active_config_sha256: active_config_sha256.clone(),
            state: "verified".to_string(),
            capabilities: capability_references.clone(),
            updated_at_ms: epoch_ms().max(1),
            failure_reason: None,
        };
        match save_hybrid_capability_manifest(&manifest) {
            Ok(path) => Some(path),
            Err(error) => {
                let agent_rollback =
                    rollback_agent_integration(&receipt.transaction_id, &config_backup_root());
                let (capability_rollback_receipts, capability_rollback_errors) = store
                    .as_ref()
                    .map(|store| revoke_hybrid_capabilities(store, &capability_references))
                    .unwrap_or_default();
                let rollback_error = agent_rollback
                    .as_ref()
                    .err()
                    .map(|rollback_error| sanitize_command_output(&format!("{rollback_error:#}")));
                return json!({
                    "ok": false,
                    "state": if rollback_error.is_none() && capability_rollback_errors.is_empty() { "failed" } else { "partial" },
                    "source": "d-token-agent-integration",
                    "error": sanitize_command_output(&format!(
                        "Capability manifest persistence failed; rollback was attempted: {error}"
                    )),
                    "agent_rollback_error": rollback_error,
                    "capability_rollback_receipts": capability_rollback_receipts,
                    "capability_rollback_errors": capability_rollback_errors,
                });
            }
        }
    };
    let _ = append_desktop_event(
        "agent_integration_apply",
        "ok",
        json!({
            "transaction_id": &receipt.transaction_id,
            "composite_review_digest": &review_digest,
            "rollback_ids": &receipt.rollback_ids,
            "rollback_point_count": receipt.rollback_ids.len(),
            "capability_count": capability_references.len(),
            "new_session_required": receipt.new_session_required,
        }),
    );
    json!({
        "ok": true,
        "state": "applied",
        "source": "d-token-agent-integration",
        "receipt": receipt,
        "verification": verification,
        "composite_review_digest": review_digest,
        "capability_receipts": capability_receipts,
        "capability_manifest_path": manifest_path,
        "summary": "Agent integration and route-bound capabilities were applied and verified atomically."
    })
}

#[tauri::command]
fn verify_hybrid_agent_integration(request: VerifyHybridAgentIntegrationRequest) -> Value {
    let agent_verification =
        verify_agent_integration_transaction(&request.transaction_id, &config_backup_root());
    let mut capability_results = Vec::new();
    let mut capability_error = None;
    let mut capability_ok = true;
    let mut capability_manifest = match load_hybrid_capability_manifest(&request.transaction_id) {
        Ok(manifest) => manifest,
        Err(error) => {
            capability_ok = false;
            capability_error = Some(error);
            None
        }
    };
    if let Some(manifest) = capability_manifest.as_mut() {
        let (ok, results, error) = verify_hybrid_capabilities(manifest);
        capability_ok = ok;
        capability_results = results;
        capability_error = error;
        manifest.state = if ok {
            "verified".to_string()
        } else {
            "changed".to_string()
        };
        manifest.failure_reason = capability_error.clone();
        manifest.updated_at_ms = epoch_ms().max(1);
        if let Err(error) = save_hybrid_capability_manifest(manifest) {
            capability_ok = false;
            capability_error = Some(format!(
                "Capability verification manifest could not be persisted: {error}"
            ));
        }
    }
    let agent_ok = agent_verification.as_ref().is_ok_and(|result| result.ok);
    let ok = agent_ok && capability_ok;
    let state = if ok {
        "verified"
    } else if agent_verification
        .as_ref()
        .is_ok_and(|result| result.verified_targets.is_empty())
    {
        "changed"
    } else {
        "partial"
    };
    let agent_error = agent_verification
        .as_ref()
        .err()
        .map(|error| sanitize_command_output(&format!("{error:#}")));
    let verification_value = agent_verification.ok();
    json!({
        "ok": ok,
        "state": state,
        "source": "d-token-agent-integration",
        "verification": verification_value,
        "agent_error": agent_error,
        "capabilities": capability_results,
        "capability_error": capability_error.map(|error| sanitize_command_output(&error)),
        "service": dtoken_service_status_value(),
        "transport": active_route_config_status(),
        "summary": if ok {
            "Agent targets and route-bound capabilities were verified."
        } else {
            "Agent integration or route-bound capability verification needs attention."
        }
    })
}

#[tauri::command]
fn rollback_hybrid_agent_integration(request: RollbackHybridAgentIntegrationRequest) -> Value {
    if request.transaction_id.trim().is_empty() {
        return json!({"ok": false, "state": "blocked", "error": "Rollback requires a transaction id"});
    }
    let mut capability_manifest = match load_hybrid_capability_manifest(&request.transaction_id) {
        Ok(manifest) => manifest,
        Err(error) => {
            return json!({
                "ok": false,
                "state": "failed",
                "source": "d-token-agent-integration",
                "error": sanitize_command_output(&error),
            })
        }
    };
    let agent_rollback = rollback_agent_integration(&request.transaction_id, &config_backup_root());
    let mut capability_receipts = Vec::new();
    let mut capability_errors = Vec::new();
    if let Some(manifest) = capability_manifest.as_mut() {
        match managed_mcp_context_store_path()
            .and_then(|path| ContextStore::open(path).map_err(|error| format!("{error:#}")))
        {
            Ok(store) => {
                (capability_receipts, capability_errors) =
                    revoke_hybrid_capabilities(&store, &manifest.capabilities);
            }
            Err(error) => capability_errors.push(sanitize_command_output(&format!(
                "Managed capability projection is unavailable: {error:#}"
            ))),
        }
        let agent_ok = agent_rollback.is_ok();
        manifest.state = if agent_ok && capability_errors.is_empty() {
            "rolled_back".to_string()
        } else {
            "partial".to_string()
        };
        manifest.failure_reason = if agent_ok && capability_errors.is_empty() {
            None
        } else {
            Some(format!(
                "agent_rollback_ok={agent_ok}; capability_error_count={}",
                capability_errors.len()
            ))
        };
        manifest.updated_at_ms = epoch_ms().max(1);
        if let Err(error) = save_hybrid_capability_manifest(manifest) {
            capability_errors.push(format!(
                "Capability rollback manifest could not be persisted: {error}"
            ));
        }
    }
    let agent_error = agent_rollback
        .as_ref()
        .err()
        .map(|error| sanitize_command_output(&format!("{error:#}")));
    let restored = agent_rollback.unwrap_or_default();
    let ok = agent_error.is_none() && capability_errors.is_empty();
    let _ = append_desktop_event(
        "agent_integration_rollback",
        if ok { "ok" } else { "partial" },
        json!({
            "transaction_id": &request.transaction_id,
            "restored_target_count": restored.len(),
            "capability_receipt_count": capability_receipts.len(),
            "capability_error_count": capability_errors.len(),
        }),
    );
    json!({
        "ok": ok,
        "state": if ok { "restored" } else { "partial" },
        "source": "d-token-agent-integration",
        "restored_targets": restored,
        "agent_error": agent_error,
        "capability_receipts": capability_receipts,
        "capability_errors": capability_errors,
        "summary": if ok {
            "Agent integration and route-bound capabilities were restored to the pre-transaction state."
        } else {
            "Rollback was incomplete; inspect the Agent and capability receipts before retrying."
        }
    })
}

#[tauri::command]
fn get_hybrid_agent_integration_transaction(
    request: GetHybridAgentIntegrationTransactionRequest,
) -> Value {
    match get_agent_integration_transaction(&request.transaction_id, &config_backup_root()) {
        Ok(transaction) => match load_hybrid_capability_manifest(&request.transaction_id) {
            Ok(capability_manifest) => json!({
                "ok": true,
                "state": capability_manifest
                    .as_ref()
                    .map(|manifest| manifest.state.as_str())
                    .unwrap_or_else(|| match transaction.state {
                        AgentIntegrationTransactionState::Applied => "applied",
                        AgentIntegrationTransactionState::Verified => "verified",
                        AgentIntegrationTransactionState::Partial => "partial",
                        AgentIntegrationTransactionState::Changed => "changed",
                        AgentIntegrationTransactionState::RolledBack => "rolled_back",
                        AgentIntegrationTransactionState::Failed => "failed",
                    }),
                "source": "d-token-agent-integration",
                "transaction": transaction,
                "capability_manifest": capability_manifest,
            }),
            Err(error) => json!({
                "ok": false,
                "state": "partial",
                "source": "d-token-agent-integration",
                "transaction": transaction,
                "error": sanitize_command_output(&error),
            }),
        },
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

fn managed_rtk_root_path() -> PathBuf {
    service_data_dir().join("managed-rtk")
}

fn current_host_platform() -> HostPlatform {
    if cfg!(target_os = "windows") {
        HostPlatform::Windows
    } else if cfg!(target_os = "macos") {
        HostPlatform::Macos
    } else {
        HostPlatform::Linux
    }
}

fn managed_rtk_host_config_fingerprint() -> Result<String, String> {
    let path = active_config_path();
    if !path.is_file() {
        return Err(
            "A verified active d-token configuration is required before enabling integrated RTK"
                .to_string(),
        );
    }
    file_sha256(&path)
}

#[tauri::command]
fn get_managed_rtk_status() -> Value {
    let evidence = inspect_managed_rtk_runtime(&managed_rtk_root_path());
    json!({
        "ok": evidence.verified,
        "state": if evidence.verified && evidence.usage_count > 0 {
            "active_verified"
        } else if evidence.verified {
            "installed_no_usage"
        } else if evidence.installed {
            "unverified"
        } else {
            "not_installed"
        },
        "source": "dtoken-managed-rtk",
        "evidence": evidence,
        "writes_performed": false,
        "privacy": "Status excludes command argv, output, paths, and digests.",
    })
}

#[tauri::command]
fn preview_managed_rtk_install() -> Value {
    let host_config_fingerprint = match managed_rtk_host_config_fingerprint() {
        Ok(value) => value,
        Err(error) => {
            return json!({
                "ok": false,
                "state": "blocked",
                "summary": error,
                "writes_performed": false,
            });
        }
    };
    let bridge_path = match resolve_dtoken_agent_bridge_exe() {
        Ok(path) => path,
        Err(error) => {
            return json!({
                "ok": false,
                "state": "blocked",
                "summary": error,
                "writes_performed": false,
            });
        }
    };
    let bridge_sha256 = match file_sha256(&bridge_path) {
        Ok(value) => value,
        Err(error) => {
            return json!({
                "ok": false,
                "state": "blocked",
                "summary": format!("Managed RTK bridge identity is unavailable: {error}"),
                "writes_performed": false,
            });
        }
    };
    let transaction_id = format!("rtk-{}", Uuid::new_v4());
    let lifecycle = ManagedRtkLifecycle::plan(
        &transaction_id,
        current_host_platform(),
        &host_config_fingerprint,
    );
    json!({
        "ok": lifecycle.state() == dtoken_types::ManagedRtkTransactionState::Planned,
        "state": "review",
        "source": "dtoken-managed-rtk",
        "transaction_id": transaction_id,
        "host_config_fingerprint": host_config_fingerprint,
        "bridge_sha256": bridge_sha256,
        "target": managed_rtk_root_path(),
        "max_output_bytes": 65_536,
        "requires_explicit_confirmation": true,
        "writes_performed": false,
        "summary": "Install a d-token-owned RTK wrapper manifest and verify its bundled bridge identity. Agent command configuration remains a separate reviewed transaction.",
    })
}

#[tauri::command]
fn apply_managed_rtk_install(request: ApplyManagedRtkRequest) -> Value {
    if !request.confirmed {
        return json!({
            "ok": false,
            "state": "confirmation_required",
            "writes_performed": false,
        });
    }
    let current_fingerprint = match managed_rtk_host_config_fingerprint() {
        Ok(value) => value,
        Err(error) => {
            return json!({
                "ok": false,
                "state": "blocked",
                "summary": error,
                "writes_performed": false,
            });
        }
    };
    if current_fingerprint != request.reviewed_host_config_fingerprint {
        return json!({
            "ok": false,
            "state": "stale_review",
            "summary": "The active d-token configuration changed after review",
            "writes_performed": false,
        });
    }
    let bridge_path = match resolve_dtoken_agent_bridge_exe() {
        Ok(path) => path,
        Err(error) => {
            return json!({
                "ok": false,
                "state": "blocked",
                "summary": error,
                "writes_performed": false,
            });
        }
    };
    let bridge_sha256 = match file_sha256(&bridge_path) {
        Ok(value) => value,
        Err(error) => {
            return json!({
                "ok": false,
                "state": "blocked",
                "summary": format!("Managed RTK bridge identity is unavailable: {error}"),
                "writes_performed": false,
            });
        }
    };
    if bridge_sha256 != request.reviewed_bridge_sha256 {
        return json!({
            "ok": false,
            "state": "stale_review",
            "summary": "The d-token Agent bridge changed after review",
            "writes_performed": false,
        });
    }
    let max_output_bytes = request.max_output_bytes.unwrap_or(65_536);
    if !(MIN_MANAGED_RTK_OUTPUT_BYTES..=MAX_MANAGED_RTK_OUTPUT_BYTES).contains(&max_output_bytes) {
        return json!({
            "ok": false,
            "state": "blocked",
            "summary": "Managed RTK output limit must be between 1 KiB and 1 MiB",
            "writes_performed": false,
        });
    }
    let mut lifecycle = ManagedRtkLifecycle::plan(
        &request.transaction_id,
        current_host_platform(),
        &current_fingerprint,
    );
    let install = match lifecycle.install_hook(ManagedHookPaths {
        managed_root: managed_rtk_root_path(),
        bridge_path,
        max_output_bytes: Some(max_output_bytes),
    }) {
        Ok(receipt) => receipt,
        Err(error) => {
            return json!({
                "ok": false,
                "state": "failed",
                "summary": format!("Managed RTK install failed closed: {error:#}"),
                "writes_performed": false,
            });
        }
    };
    let verify = match lifecycle.verify_hook() {
        Ok(receipt) => receipt,
        Err(error) => {
            let _ = uninstall_managed_rtk_runtime(&managed_rtk_root_path());
            return json!({
                "ok": false,
                "state": "rolled_back",
                "summary": format!("Managed RTK verification failed; the manifest was removed: {error:#}"),
                "writes_performed": false,
            });
        }
    };
    let evidence = inspect_managed_rtk_runtime(&managed_rtk_root_path());
    let _ = append_desktop_event(
        "managed_rtk_installed",
        "ok",
        json!({
            "transaction_id": request.transaction_id,
            "verified": evidence.verified,
            "usage_count": evidence.usage_count,
        }),
    );
    json!({
        "ok": evidence.verified,
        "state": "installed_verified",
        "source": "dtoken-managed-rtk",
        "install_receipt": install,
        "verify_receipt": verify,
        "evidence": evidence,
        "writes_performed": true,
        "summary": "Managed RTK wrapper installed and verified; it becomes active only after a reviewed Agent command invokes it.",
    })
}

#[tauri::command]
fn preview_managed_rtk_clear() -> Value {
    let root = managed_rtk_root_path();
    match dtoken_preprocessor::verify_managed_hook_root(&root, false) {
        Ok(verification) => json!({
            "ok": true,
            "state": "review",
            "source": "dtoken-managed-rtk",
            "transaction_id": verification.manifest.transaction_id,
            "host_config_fingerprint": verification.manifest.host_config_fingerprint,
            "target": root,
            "requires_explicit_confirmation": true,
            "writes_performed": false,
            "summary": "Remove only the verified d-token-owned RTK manifest and bounded usage receipts.",
        }),
        Err(_) => json!({
            "ok": false,
            "state": "not_installed_or_unverified",
            "writes_performed": false,
            "summary": "No verified d-token-owned RTK manifest is available to clear.",
        }),
    }
}

#[tauri::command]
fn apply_managed_rtk_clear(request: ClearManagedRtkRequest) -> Value {
    if !request.confirmed {
        return json!({
            "ok": false,
            "state": "confirmation_required",
            "writes_performed": false,
        });
    }
    let root = managed_rtk_root_path();
    let verification = match dtoken_preprocessor::verify_managed_hook_root(&root, false) {
        Ok(value) => value,
        Err(_) => {
            return json!({
                "ok": false,
                "state": "not_installed_or_unverified",
                "writes_performed": false,
            });
        }
    };
    if verification.manifest.transaction_id != request.transaction_id
        || verification.manifest.host_config_fingerprint != request.reviewed_host_config_fingerprint
    {
        return json!({
            "ok": false,
            "state": "stale_review",
            "writes_performed": false,
            "summary": "Managed RTK ownership evidence changed after review.",
        });
    }
    match uninstall_managed_rtk_runtime(&root) {
        Ok(receipt) => {
            let _ = append_desktop_event(
                "managed_rtk_uninstalled",
                "ok",
                json!({"transaction_id": receipt.transaction_id}),
            );
            json!({
                "ok": true,
                "state": "uninstalled",
                "source": "dtoken-managed-rtk",
                "receipt": receipt,
                "writes_performed": true,
            })
        }
        Err(error) => json!({
            "ok": false,
            "state": "failed",
            "summary": format!("Managed RTK clear failed closed: {error:#}"),
            "writes_performed": false,
        }),
    }
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
    // Test isolation: the 200ms process-global cache must not leak a previous
    // test's probe result across isolated workspace tests (a stale "not
    // running" or foreign-nonce payload makes Doctor report service-not-running
    // even though the fake runtime is up). Production has no
    // DTOKEN_TEST_STATE_DIR and keeps the cache behavior unchanged.
    if env::var_os("DTOKEN_TEST_STATE_DIR").is_some() {
        return service_health_payload();
    }
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
    dtoken_service_status_value_with_health(service_health_payload_cached())
}

fn dtoken_service_status_value_fresh() -> Value {
    dtoken_service_status_value_with_health(service_health_payload())
}

fn dtoken_service_status_value_with_health(health: Option<Value>) -> Value {
    let manifest = read_service_manifest();
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

/// Validates a `model-pricing` settings payload and normalizes it in place.
///
/// Contract (stored under `settings/model-pricing.json`):
/// - `prices`: array of entries; an empty array is the user-maintained default
///   empty state. Fee calculation must treat a missing entry as "price unknown"
///   and never as zero.
/// - Each entry: `model` (non-empty, trimmed, ≤128 chars), `currency`
///   (USD/CNY), `inputPerMillion` / `outputPerMillion` (finite, ≥ 0).
/// - The backend stamps `source` (`"user"`) and `updatedAt` so the local table
///   always carries its own provenance; negative/NaN/Inf values, empty or
///   overlong model ids, unsupported currencies and duplicate model+currency
///   pairs are rejected before anything is written.
fn validate_model_pricing_payload(payload: &mut Value) -> Result<(), String> {
    let prices = payload
        .get_mut("prices")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| "model-pricing payload must contain a prices array".to_string())?;
    let mut seen: std::collections::HashSet<(String, String)> = std::collections::HashSet::new();
    let updated_at = now_timestamp();
    for (index, entry) in prices.iter_mut().enumerate() {
        let object = entry
            .as_object_mut()
            .ok_or_else(|| format!("prices[{index}] must be an object"))?;
        let model = object
            .get("model")
            .and_then(Value::as_str)
            .map(str::trim)
            .unwrap_or("")
            .to_string();
        if model.is_empty() {
            return Err(format!("prices[{index}].model must not be empty"));
        }
        if model.chars().count() > 128 {
            return Err(format!("prices[{index}].model is too long"));
        }
        let currency = object
            .get("currency")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        if currency != "USD" && currency != "CNY" {
            return Err(format!("prices[{index}].currency is not supported"));
        }
        for field in ["inputPerMillion", "outputPerMillion"] {
            match object.get(field).and_then(Value::as_f64) {
                Some(value) if value.is_finite() && value >= 0.0 => {}
                _ => {
                    return Err(format!(
                        "prices[{index}].{field} must be a finite non-negative number"
                    ));
                }
            }
        }
        if !seen.insert((model.clone(), currency.clone())) {
            return Err(format!("duplicate pricing for model {model}"));
        }
        object.insert("model".to_string(), json!(model));
        object.insert("currency".to_string(), json!(currency));
        object.insert("source".to_string(), json!("user"));
        object.insert("updatedAt".to_string(), json!(updated_at));
    }
    Ok(())
}

#[tauri::command]
fn save_app_settings(section: String, mut payload: Value) -> ActionResult {
    let allowed = [
        "rtk",
        "headroom",
        "app",
        "project-policy",
        "routing",
        "preferences",
        "smart-learning",
        "model-pricing",
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

    if section == "model-pricing" {
        if let Err(err) = validate_model_pricing_payload(&mut payload) {
            logs.push(log(
                "error",
                format!("Model pricing validation failed: {err}"),
            ));
            return ActionResult {
                ok: false,
                summary: format!("Model pricing is invalid: {err}"),
                logs,
                tools: all_checks(),
            };
        }
        logs.push(log("info", "Model pricing payload validated"));
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
        "model-pricing",
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

/// Codex's local OpenAI Responses proxy may advertise no static model list.
/// With an empty reviewed request model the provider keeps an empty `models`
/// array so the request model is passed through dynamically.
fn is_codex_responses_dynamic_passthrough(
    agent_id: &str,
    candidate: &Value,
    request_model: &str,
) -> bool {
    agent_id == "codex"
        && request_model.trim().is_empty()
        && candidate.get("protocol").and_then(Value::as_str) == Some("responses")
        && candidate.get("auth_mode").and_then(Value::as_str) == Some("none")
        && candidate
            .get("models")
            .and_then(Value::as_array)
            .is_some_and(|models| models.is_empty())
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

fn merge_connection_route_document(
    existing: Option<Value>,
    selected_agents: &[String],
    new_providers: Vec<Value>,
    new_routes: Vec<Value>,
    new_instances: Vec<Value>,
) -> Result<String, String> {
    let mut root = match existing {
        Some(Value::Object(root)) => root,
        Some(_) => return Err("Active config root must be a JSON object".to_string()),
        None => serde_json::Map::new(),
    };
    let mut providers = root
        .remove("providers")
        .map(|value| {
            value
                .as_array()
                .cloned()
                .ok_or_else(|| "Active config providers must be an array".to_string())
        })
        .transpose()?
        .unwrap_or_default();
    let mut routes = root
        .remove("routes")
        .map(|value| {
            value
                .as_array()
                .cloned()
                .ok_or_else(|| "Active config routes must be an array".to_string())
        })
        .transpose()?
        .unwrap_or_default();
    let mut instances = root
        .remove("instances")
        .map(|value| {
            value
                .as_array()
                .cloned()
                .ok_or_else(|| "Active config instances must be an array".to_string())
        })
        .transpose()?
        .unwrap_or_default();

    let selected_route_ids = selected_agents
        .iter()
        .map(|agent_id| {
            safe_slug(
                &format!("auto-{agent_id}-route"),
                &format!("auto-{agent_id}"),
            )
        })
        .collect::<HashSet<_>>();
    let selected_instance_ids = selected_route_ids
        .iter()
        .map(|route_id| format!("{route_id}-instance"))
        .collect::<HashSet<_>>();
    let removed_provider_ids = routes
        .iter()
        .filter(|route| {
            route
                .get("id")
                .and_then(Value::as_str)
                .is_some_and(|route_id| selected_route_ids.contains(route_id))
        })
        .flat_map(|route| {
            ["target_provider_id", "fallback_provider_id"]
                .into_iter()
                .filter_map(|field| route.get(field).and_then(Value::as_str))
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .collect::<HashSet<_>>();
    routes.retain(|route| {
        !route
            .get("id")
            .and_then(Value::as_str)
            .is_some_and(|route_id| selected_route_ids.contains(route_id))
    });
    instances.retain(|instance| {
        !instance
            .get("id")
            .and_then(Value::as_str)
            .is_some_and(|instance_id| selected_instance_ids.contains(instance_id))
    });
    let preserved_provider_references = routes
        .iter()
        .flat_map(|route| {
            ["target_provider_id", "fallback_provider_id"]
                .into_iter()
                .filter_map(|field| route.get(field).and_then(Value::as_str))
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .collect::<HashSet<_>>();
    let new_provider_ids = new_providers
        .iter()
        .filter_map(|provider| provider.get("id").and_then(Value::as_str))
        .map(str::to_string)
        .collect::<HashSet<_>>();
    providers.retain(|provider| {
        let Some(provider_id) = provider.get("id").and_then(Value::as_str) else {
            return true;
        };
        !new_provider_ids.contains(provider_id)
            && (!removed_provider_ids.contains(provider_id)
                || preserved_provider_references.contains(provider_id))
    });

    providers.extend(new_providers);
    routes.extend(new_routes);
    instances.extend(new_instances);
    root.insert("providers".to_string(), Value::Array(providers));
    root.insert("routes".to_string(), Value::Array(routes));
    root.insert("instances".to_string(), Value::Array(instances));
    let content = serde_json::to_string_pretty(&Value::Object(root))
        .map_err(|error| format!("Connection route plan serialize failed: {error}"))?;
    let parsed = serde_json::from_str::<DtokenConfig>(&content)
        .map_err(|error| format!("Connection route plan is not a valid d-token config: {error}"))?;
    validate_connection_route_config(&parsed)?;
    Ok(content)
}

fn validate_connection_route_config(config: &DtokenConfig) -> Result<(), String> {
    let mut provider_ids = HashSet::new();
    for provider in &config.providers {
        if !provider_ids.insert(provider.id.as_str()) {
            return Err(format!(
                "Connection route plan contains duplicate provider {}",
                provider.id
            ));
        }
        if provider_targets_local_listener(&provider.base_url, DTOKEN_SERVICE_PORT) {
            return Err(format!(
                "Connection route plan provider {} loops back into d-token",
                provider.id
            ));
        }
    }
    let mut route_ids = HashSet::new();
    for route in &config.routes {
        if !route_ids.insert(route.id.as_str()) {
            return Err(format!(
                "Connection route plan contains duplicate route {}",
                route.id
            ));
        }
        if !provider_ids.contains(route.target_provider_id.as_str()) {
            return Err(format!(
                "Connection route {} references a missing target provider",
                route.id
            ));
        }
        if route
            .fallback_provider_id
            .as_deref()
            .is_some_and(|provider_id| !provider_ids.contains(provider_id))
        {
            return Err(format!(
                "Connection route {} references a missing fallback provider",
                route.id
            ));
        }
    }
    let mut instance_ids = HashSet::new();
    for instance in &config.instances {
        if !instance_ids.insert(instance.id.as_str()) {
            return Err(format!(
                "Connection route plan contains duplicate instance {}",
                instance.id
            ));
        }
        if !route_ids.contains(instance.route_id.as_str()) {
            return Err(format!(
                "Connection instance {} references a missing route",
                instance.id
            ));
        }
    }
    for route in &config.routes {
        if !instance_ids.contains(route.instance_id.as_str()) {
            return Err(format!(
                "Connection route {} references a missing instance",
                route.id
            ));
        }
    }
    Ok(())
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
    let requested_agent_ids = request
        .bindings
        .iter()
        .map(|binding| binding.agent_id.clone())
        .collect::<Vec<_>>();
    let scoped_agent_fingerprint = (request.source_fingerprint_scope.as_deref()
        == Some("agent_configs"))
    .then(|| agent_config_source_fingerprint(discovery, &requested_agent_ids))
    .transpose()?;
    let current_revision = if request.source_fingerprint_scope.as_deref() == Some("cc_switch") {
        discovery
            .get("external_config_snapshot")
            .and_then(|snapshot| snapshot.get("cc_switch"))
            .and_then(|cc_switch| cc_switch.get("revision"))
            .and_then(Value::as_str)
            .ok_or_else(|| "Current cc-switch revision is unavailable".to_string())?
            .to_string()
    } else if let Some(fingerprint) = scoped_agent_fingerprint.as_deref() {
        format!("agent-config-{fingerprint}")
    } else {
        discovery
            .get("revision")
            .and_then(Value::as_str)
            .ok_or_else(|| "Current discovery revision is unavailable".to_string())?
            .to_string()
    };
    if current_revision != request.discovery_revision {
        return Err(
            "Agent or upstream configuration changed after discovery; scan again before review"
                .to_string(),
        );
    }
    if let Some(reviewed_fingerprint) = request.source_fingerprint.as_deref() {
        let snapshot = discovery
            .get("external_config_snapshot")
            .ok_or_else(|| "External configuration snapshot is unavailable".to_string())?;
        let (current_fingerprint, drift_error) = if request.source_fingerprint_scope.as_deref()
            == Some("cc_switch")
        {
            (
                snapshot
                    .get("cc_switch")
                    .and_then(|cc_switch| cc_switch.get("fingerprint"))
                    .and_then(Value::as_str),
                "cc-switch configuration changed after review; scan again before apply",
            )
        } else if request.source_fingerprint_scope.as_deref() == Some("agent_configs") {
            (
                scoped_agent_fingerprint.as_deref(),
                "External Agent configuration changed after review; scan again before apply",
            )
        } else {
            (
                    snapshot.get("fingerprint").and_then(Value::as_str),
                    "External Agent or cc-switch configuration changed after review; scan again before apply",
                )
        };
        let current_fingerprint = current_fingerprint
            .ok_or_else(|| "External configuration fingerprint is unavailable".to_string())?;
        if current_fingerprint != reviewed_fingerprint {
            return Err(drift_error.to_string());
        }
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
        ) && binding.get("can_configure").and_then(Value::as_bool) != Some(true)
        {
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
                    && (candidate
                        .get("models")
                        .and_then(Value::as_array)
                        .is_some_and(|models| {
                            models
                                .iter()
                                .any(|model| model.as_str() == Some(binding_request.model.as_str()))
                        })
                        || is_codex_responses_dynamic_passthrough(
                            &agent_id,
                            candidate,
                            &binding_request.model,
                        ))
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
        let dynamic_passthrough =
            is_codex_responses_dynamic_passthrough(&agent_id, candidate, &binding_request.model);
        if (!dynamic_passthrough && model.is_empty())
            || model.len() > 256
            || model.contains(['\n', '\r', '\0'])
        {
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
            "models": if dynamic_passthrough {
                Vec::<String>::new()
            } else {
                vec![model]
            },
            "wire_api": if dynamic_passthrough {
                "open_ai_responses"
            } else {
                "open_ai_chat"
            },
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
    let parent = active_config_path()
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| "Active config parent path is unavailable".to_string())?;
    fs::create_dir_all(&parent)
        .map_err(|error| format!("Active config directory create failed: {error}"))?;
    let existing = match fs::read_to_string(active_config_path()) {
        Ok(content) => Some(
            serde_json::from_str::<Value>(&content)
                .map_err(|error| format!("Active config is invalid JSON: {error}"))?,
        ),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(format!("Active config could not be read: {error}")),
    };
    let content =
        merge_connection_route_document(existing, &selected_agents, providers, routes, instances)?;
    let prepared = config_patcher()
        .prepare_change(
            patchable_active_config_path()?,
            &content,
            ConfigToolKind::Dtoken,
            Some("Automatic Agent connection plan".to_string()),
            RiskLevel::Medium,
        )
        .map_err(|error| format!("Connection route review could not be prepared: {error:#}"))?;
    Ok((current_revision, prepared, selected_agents))
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

fn normalize_follow_endpoint(value: &str) -> Result<String, String> {
    let parsed = url::Url::parse(value)
        .map_err(|_| "Agent upstream endpoint is not a valid URL".to_string())?;
    if !matches!(parsed.scheme(), "http" | "https")
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
    {
        return Err(
            "Agent upstream endpoint is unsupported or contains credential-like metadata"
                .to_string(),
        );
    }
    let normalized = parsed.to_string().trim_end_matches('/').to_string();
    if provider_targets_local_listener(&normalized, DTOKEN_SERVICE_PORT) {
        return Err("Agent upstream points back to the d-token listener".to_string());
    }
    Ok(normalized)
}

fn agent_follow_upstream_signature(binding: &AgentRouteFollowBinding) -> String {
    stable_hash(
        &json!({
            "agent_id": binding.agent_id,
            "provider_id": binding.upstream.provider_id,
            "candidate_source": binding.candidate_source,
            "protocol": binding.upstream.protocol,
            "endpoint": binding.upstream.normalized_endpoint,
            "profile_scope": binding.upstream.profile_scope,
        })
        .to_string(),
    )
}

fn agent_follow_route_signature(bindings: &[AgentRouteFollowBinding]) -> String {
    let mut stable = bindings
        .iter()
        .map(|binding| {
            json!({
                "agent_id": binding.agent_id,
                "upstream_signature": agent_follow_upstream_signature(binding),
                "model": binding.upstream.model,
            })
        })
        .collect::<Vec<_>>();
    stable.sort_by(|left, right| {
        left.get("agent_id")
            .and_then(Value::as_str)
            .cmp(&right.get("agent_id").and_then(Value::as_str))
    });
    stable_hash(&Value::Array(stable).to_string())
}

fn agent_follow_binding_from_candidate(
    agent_id: &str,
    candidate: &Value,
    model: &str,
) -> Result<AgentRouteFollowBinding, String> {
    if candidate.get("auth_mode").and_then(Value::as_str) != Some("none") {
        return Err(format!(
            "The current upstream credential for {agent_id} cannot be followed automatically"
        ));
    }
    if candidate
        .get("confidence")
        .and_then(Value::as_u64)
        .is_some_and(|confidence| confidence < 70)
    {
        return Err(format!(
            "The current upstream for {agent_id} has insufficient discovery confidence"
        ));
    }
    if model.trim().is_empty()
        || model.len() > 256
        || model.contains(['\n', '\r', '\0'])
        || !candidate
            .get("models")
            .and_then(Value::as_array)
            .is_some_and(|models| models.iter().any(|value| value.as_str() == Some(model)))
    {
        return Err(format!(
            "The current upstream model for {agent_id} is unavailable or ambiguous"
        ));
    }
    let candidate_id = candidate
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("The current upstream id for {agent_id} is unavailable"))?;
    let candidate_source = candidate
        .get("source")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("The current upstream source for {agent_id} is unavailable"))?;
    let endpoint = candidate
        .get("base_url")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("The current upstream endpoint for {agent_id} is unavailable"))
        .and_then(normalize_follow_endpoint)?;
    let protocol = parse_protocol(
        candidate
            .get("protocol")
            .and_then(Value::as_str)
            .unwrap_or("open_ai"),
    )
    .to_string();
    Ok(AgentRouteFollowBinding {
        agent_id: agent_id.to_string(),
        candidate_id: candidate_id.to_string(),
        candidate_source: candidate_source.to_string(),
        upstream: UpstreamSignature {
            provider_id: candidate_id.to_string(),
            model: model.to_string(),
            protocol,
            normalized_endpoint: endpoint,
            profile_scope: Some(candidate_source.to_string()),
        },
    })
}

fn agent_config_fingerprints(
    discovery: &Value,
    agent_ids: &[String],
) -> Result<BTreeMap<String, String>, String> {
    let configs = discovery
        .get("external_config_snapshot")
        .and_then(|snapshot| snapshot.get("agent_configs"))
        .and_then(Value::as_array)
        .ok_or_else(|| "External Agent configuration snapshot is unavailable".to_string())?;
    let requested = agent_ids.iter().map(String::as_str).collect::<HashSet<_>>();
    let mut grouped = BTreeMap::<String, Vec<Value>>::new();
    for config in configs {
        let Some(agent_id) = config.get("agent_id").and_then(Value::as_str) else {
            continue;
        };
        if !requested.contains(agent_id) {
            continue;
        }
        grouped
            .entry(agent_id.to_string())
            .or_default()
            .push(json!({
                "path": config.get("path"),
                "state": config.get("state"),
                "fingerprint": config.get("fingerprint"),
            }));
    }
    let mut fingerprints = BTreeMap::new();
    for agent_id in agent_ids {
        let mut entries = grouped.remove(agent_id).unwrap_or_default();
        entries.sort_by(|left, right| {
            left.get("path")
                .and_then(Value::as_str)
                .cmp(&right.get("path").and_then(Value::as_str))
        });
        fingerprints.insert(
            agent_id.clone(),
            stable_hash(
                &json!({
                    "agent_id": agent_id,
                    "configs": entries,
                })
                .to_string(),
            ),
        );
    }
    Ok(fingerprints)
}

fn agent_config_source_fingerprint(
    discovery: &Value,
    agent_ids: &[String],
) -> Result<String, String> {
    let mut ids = agent_ids.to_vec();
    ids.sort();
    ids.dedup();
    Ok(stable_hash(
        &serde_json::to_string(&agent_config_fingerprints(discovery, &ids)?)
            .map_err(|error| format!("Agent source fingerprint serialize failed: {error}"))?,
    ))
}

fn current_dtoken_transport_model(discovery: &Value, agent_id: &str) -> Option<String> {
    let mut models = discovery
        .get("candidates")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|candidate| {
            candidate.get("active").and_then(Value::as_bool) == Some(true)
                && candidate
                    .get("agent_ids")
                    .and_then(Value::as_array)
                    .is_some_and(|agent_ids| {
                        agent_ids
                            .iter()
                            .any(|value| value.as_str() == Some(agent_id))
                    })
                && candidate
                    .get("base_url")
                    .and_then(Value::as_str)
                    .is_some_and(|endpoint| {
                        provider_targets_local_listener(endpoint, DTOKEN_SERVICE_PORT)
                    })
        })
        .flat_map(|candidate| {
            candidate
                .get("models")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(str::to_string)
        })
        .collect::<Vec<_>>();
    models.sort();
    models.dedup();
    (models.len() == 1).then(|| models.remove(0))
}

fn selected_agent_follow_binding(
    discovery: &Value,
    request: &ConnectionRouteBindingRequest,
) -> Result<AgentRouteFollowBinding, String> {
    let binding = discovery
        .get("agent_bindings")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|binding| {
            binding.get("agent_id").and_then(Value::as_str) == Some(request.agent_id.as_str())
        })
        .ok_or_else(|| format!("Agent {} is no longer discoverable", request.agent_id))?;
    let candidate = binding
        .get("candidates")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|candidate| {
            candidate.get("id").and_then(Value::as_str) == Some(request.candidate_id.as_str())
                && candidate.get("source").and_then(Value::as_str)
                    == Some(request.candidate_source.as_str())
        })
        .ok_or_else(|| {
            format!(
                "The reviewed upstream for {} is no longer discoverable",
                request.agent_id
            )
        })?;
    agent_follow_binding_from_candidate(&request.agent_id, candidate, &request.model)
}

fn authorizable_agent_upstream_signatures(
    discovery: &Value,
    agent_id: &str,
) -> Result<Vec<String>, String> {
    let binding = discovery
        .get("agent_bindings")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|binding| binding.get("agent_id").and_then(Value::as_str) == Some(agent_id))
        .ok_or_else(|| format!("Agent {agent_id} is no longer discoverable"))?;
    let mut signatures = binding
        .get("candidates")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|candidate| {
            candidate.get("auth_mode").and_then(Value::as_str) == Some("none")
                && candidate
                    .get("source")
                    .and_then(Value::as_str)
                    .is_some_and(|source| !source.starts_with("cc-switch:"))
                && candidate
                    .get("confidence")
                    .and_then(Value::as_u64)
                    .unwrap_or(100)
                    >= 70
        })
        .filter_map(|candidate| {
            let model = candidate
                .get("models")
                .and_then(Value::as_array)?
                .first()?
                .as_str()?;
            let binding = agent_follow_binding_from_candidate(agent_id, candidate, model).ok()?;
            Some(agent_follow_upstream_signature(&binding))
        })
        .collect::<Vec<_>>();
    signatures.sort();
    signatures.dedup();
    if signatures.is_empty() {
        return Err(format!(
            "Agent {agent_id} has no credential-free upstream identity safe for follow mode"
        ));
    }
    Ok(signatures)
}

fn default_agent_auto_follow_authorization() -> AgentAutoFollowAuthorization {
    AgentAutoFollowAuthorization {
        schema_version: AGENT_AUTO_FOLLOW_SCHEMA_VERSION,
        enabled: false,
        authorized_at_ms: 0,
        authorized_upstream_signatures: BTreeMap::new(),
        last_synced_bindings: Vec::new(),
        last_observed_config_fingerprints: BTreeMap::new(),
        last_observed_source_fingerprint: None,
        last_synced_source_fingerprint: None,
        last_synced_route_signature: None,
        active_config_sha256: String::new(),
        runtime_identity: None,
        last_sync_state: "not_authorized".to_string(),
    }
}

fn load_agent_auto_follow_authorization() -> AgentAutoFollowAuthorization {
    read_app_state(&["routing", "agent-auto-follow.json"])
        .ok()
        .and_then(|value| serde_json::from_value(value).ok())
        .filter(|authorization: &AgentAutoFollowAuthorization| {
            authorization.schema_version == AGENT_AUTO_FOLLOW_SCHEMA_VERSION
        })
        .unwrap_or_else(default_agent_auto_follow_authorization)
}

fn save_agent_auto_follow_authorization(
    authorization: &AgentAutoFollowAuthorization,
) -> Result<(), String> {
    let value = serde_json::to_value(authorization)
        .map_err(|error| format!("Agent follow authorization serialize failed: {error}"))?;
    write_json_app_state(&["routing", "agent-auto-follow.json"], &value).map(|_| ())
}

fn stage_agent_auto_follow_authorization(
    plan: &ConnectionRoutePlanRequest,
    discovery: &Value,
    active_config_sha256: &str,
) -> Result<Option<AgentAutoFollowAuthorization>, String> {
    let selected = plan
        .bindings
        .iter()
        .filter(|binding| !binding.candidate_source.starts_with("cc-switch:"))
        .collect::<Vec<_>>();
    if selected.is_empty() {
        return Ok(None);
    }
    let agent_ids = selected
        .iter()
        .map(|binding| binding.agent_id.clone())
        .collect::<Vec<_>>();
    let mut bindings = Vec::with_capacity(selected.len());
    let mut authorized_upstream_signatures = BTreeMap::new();
    for request in selected {
        let binding = selected_agent_follow_binding(discovery, request)?;
        let signatures = authorizable_agent_upstream_signatures(discovery, &request.agent_id)?;
        if !signatures.contains(&agent_follow_upstream_signature(&binding)) {
            return Err(format!(
                "The reviewed upstream for {} is outside the safe follow set",
                request.agent_id
            ));
        }
        authorized_upstream_signatures.insert(request.agent_id.clone(), signatures);
        bindings.push(binding);
    }
    bindings.sort_by(|left, right| left.agent_id.cmp(&right.agent_id));
    let config_fingerprints = agent_config_fingerprints(discovery, &agent_ids)?;
    let source_fingerprint = agent_config_source_fingerprint(discovery, &agent_ids)?;
    Ok(Some(AgentAutoFollowAuthorization {
        schema_version: AGENT_AUTO_FOLLOW_SCHEMA_VERSION,
        enabled: false,
        authorized_at_ms: epoch_ms(),
        authorized_upstream_signatures,
        last_synced_route_signature: Some(agent_follow_route_signature(&bindings)),
        last_synced_bindings: bindings,
        last_observed_config_fingerprints: config_fingerprints,
        last_observed_source_fingerprint: Some(source_fingerprint.clone()),
        last_synced_source_fingerprint: Some(source_fingerprint),
        active_config_sha256: active_config_sha256.to_string(),
        runtime_identity: None,
        last_sync_state: "pending_runtime_verification".to_string(),
    }))
}

fn active_config_sha256() -> Result<String, String> {
    fs::read_to_string(active_config_path())
        .map(|content| stable_hash(&content))
        .map_err(|error| format!("Active route config could not be hashed: {error}"))
}

fn finalize_agent_auto_follow_authorization_value(
    request: FinalizeAgentAutoFollowRequest,
) -> Value {
    let mut authorization = load_agent_auto_follow_authorization();
    if authorization.last_sync_state != "pending_runtime_verification" {
        return json!({
            "ok": false,
            "state": "not_pending",
            "summary": "No reviewed Agent follow authorization is waiting for runtime verification",
        });
    }
    let current_sha256 = match active_config_sha256() {
        Ok(sha256) => sha256,
        Err(error) => {
            return json!({"ok":false, "state":"failed", "summary":sanitize_command_output(&error)})
        }
    };
    if request.expected_route_sha256 != authorization.active_config_sha256
        || current_sha256 != authorization.active_config_sha256
    {
        authorization.last_sync_state = "stale_route".to_string();
        let _ = save_agent_auto_follow_authorization(&authorization);
        return json!({
            "ok": false,
            "state": "stale",
            "summary": "The active d-token route changed before Agent follow authorization completed",
        });
    }
    let status = dtoken_service_status_value_fresh();
    if status.get("state").and_then(Value::as_str) != Some("running")
        || status.get("managed").and_then(Value::as_bool) != Some(true)
    {
        authorization.last_sync_state = "runtime_verification_required".to_string();
        let _ = save_agent_auto_follow_authorization(&authorization);
        return json!({
            "ok": false,
            "state": "runtime_verification_required",
            "summary": "Agent follow mode requires a healthy d-token-managed runtime",
        });
    }
    let discovery = enrich_upstream_discovery(upstream_discovery::discover());
    let agent_ids = authorization
        .last_synced_bindings
        .iter()
        .map(|binding| binding.agent_id.clone())
        .collect::<Vec<_>>();
    let fingerprints = match agent_config_fingerprints(&discovery, &agent_ids) {
        Ok(fingerprints) => fingerprints,
        Err(error) => {
            return json!({"ok":false, "state":"failed", "summary":sanitize_command_output(&error)})
        }
    };
    let source_fingerprint = match agent_config_source_fingerprint(&discovery, &agent_ids) {
        Ok(fingerprint) => fingerprint,
        Err(error) => {
            return json!({"ok":false, "state":"failed", "summary":sanitize_command_output(&error)})
        }
    };
    authorization.enabled = true;
    authorization.last_observed_config_fingerprints = fingerprints;
    authorization.last_observed_source_fingerprint = Some(source_fingerprint.clone());
    authorization.last_synced_source_fingerprint = Some(source_fingerprint.clone());
    authorization.runtime_identity = status
        .get("runtime_identity")
        .and_then(Value::as_str)
        .map(str::to_string);
    authorization.last_sync_state = "applied_verified".to_string();
    if let Err(error) = save_agent_auto_follow_authorization(&authorization) {
        return json!({"ok":false, "state":"failed", "summary":sanitize_command_output(&error)});
    }
    let _ = append_desktop_event(
        "route_sync_authorization",
        "authorized",
        json!({
            "source": "agent-config",
            "agent_ids": agent_ids,
            "source_fingerprint": source_fingerprint,
            "active_config_sha256": current_sha256,
            "runtime_identity": authorization.runtime_identity,
        }),
    );
    json!({
        "ok": true,
        "state": "authorized",
        "source_fingerprint": source_fingerprint,
        "agent_ids": agent_ids,
        "runtime_identity": authorization.runtime_identity,
    })
}

#[tauri::command]
async fn finalize_agent_auto_follow_authorization(
    request: FinalizeAgentAutoFollowRequest,
) -> Value {
    tauri::async_runtime::spawn_blocking(move || {
        finalize_agent_auto_follow_authorization_value(request)
    })
    .await
    .unwrap_or_else(|error| {
        json!({
            "ok": false,
            "state": "failed",
            "summary": format!("Agent follow authorization worker failed: {error}"),
        })
    })
}

fn resolve_agent_follow_bindings(
    authorization: &AgentAutoFollowAuthorization,
    discovery: &Value,
    changed_agents: &HashSet<String>,
) -> Result<Vec<AgentRouteFollowBinding>, String> {
    let discovered_bindings = discovery
        .get("agent_bindings")
        .and_then(Value::as_array)
        .ok_or_else(|| "Current Agent discovery result is unavailable".to_string())?;
    let mut resolved = Vec::with_capacity(authorization.last_synced_bindings.len());
    for stored in &authorization.last_synced_bindings {
        if !changed_agents.contains(&stored.agent_id) {
            resolved.push(stored.clone());
            continue;
        }
        let binding = discovered_bindings
            .iter()
            .find(|binding| {
                binding.get("agent_id").and_then(Value::as_str) == Some(stored.agent_id.as_str())
            })
            .ok_or_else(|| format!("Agent {} is no longer discoverable", stored.agent_id))?;
        let current = if let Some(candidate) = binding.get("resolved_candidate") {
            if candidate
                .get("source")
                .and_then(Value::as_str)
                .is_some_and(|source| source.starts_with("cc-switch:"))
            {
                return Err(format!(
                    "Agent {} now follows cc-switch and requires the separate cc-switch authorization",
                    stored.agent_id
                ));
            }
            let model = candidate
                .get("models")
                .and_then(Value::as_array)
                .and_then(|models| (models.len() == 1).then_some(models))
                .and_then(|models| models.first())
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    format!(
                        "The current model for {} is unavailable or ambiguous",
                        stored.agent_id
                    )
                })?;
            agent_follow_binding_from_candidate(&stored.agent_id, candidate, model)?
        } else if binding.get("already_connected").and_then(Value::as_bool) == Some(true) {
            let mut retained = stored.clone();
            if let Some(model) = current_dtoken_transport_model(discovery, &stored.agent_id) {
                retained.upstream.model = model;
            }
            retained
        } else {
            return Err(format!(
                "Agent {} changed to an ambiguous, unsupported, or credential-bearing upstream",
                stored.agent_id
            ));
        };
        let approved = authorization
            .authorized_upstream_signatures
            .get(&stored.agent_id)
            .ok_or_else(|| {
                format!(
                    "Agent {} has no persisted follow authorization",
                    stored.agent_id
                )
            })?;
        if !approved.contains(&agent_follow_upstream_signature(&current)) {
            return Err(format!(
                "Agent {} changed to a provider, endpoint, or protocol outside the approved follow set",
                stored.agent_id
            ));
        }
        resolved.push(current);
    }
    resolved.sort_by(|left, right| left.agent_id.cmp(&right.agent_id));
    Ok(resolved)
}

fn discovery_with_agent_follow_bindings(
    mut discovery: Value,
    bindings: &[AgentRouteFollowBinding],
) -> Result<Value, String> {
    let discovered_bindings = discovery
        .get_mut("agent_bindings")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| "Current Agent discovery result is unavailable".to_string())?;
    for followed in bindings {
        let binding = discovered_bindings
            .iter_mut()
            .find(|binding| {
                binding.get("agent_id").and_then(Value::as_str) == Some(followed.agent_id.as_str())
            })
            .ok_or_else(|| format!("Agent {} is no longer discoverable", followed.agent_id))?;
        let candidate = json!({
            "id": followed.candidate_id,
            "name": format!("{} followed upstream", followed.agent_id),
            "source": followed.candidate_source,
            "protocol": followed.upstream.protocol,
            "base_url": followed.upstream.normalized_endpoint,
            "models": [followed.upstream.model],
            "auth_mode": "none",
            "active": true,
            "state": "discovered",
            "agent_ids": [followed.agent_id],
            "confidence": 100,
            "reasons": ["Previously reviewed Agent follow identity"],
        });
        let object = binding
            .as_object_mut()
            .ok_or_else(|| "Agent binding is not a JSON object".to_string())?;
        object.insert("installed".to_string(), json!(true));
        object.insert("capability".to_string(), json!("confirm-required"));
        object.insert("candidates".to_string(), json!([candidate.clone()]));
        object.insert("resolved_candidate".to_string(), candidate);
        object.insert("resolution".to_string(), json!("auto-resolved"));
        object.insert("already_connected".to_string(), json!(false));
    }
    Ok(discovery)
}

fn agent_follow_plan(
    discovery: &Value,
    bindings: &[AgentRouteFollowBinding],
) -> Result<ConnectionRoutePlanRequest, String> {
    let agent_ids = bindings
        .iter()
        .map(|binding| binding.agent_id.clone())
        .collect::<Vec<_>>();
    let fingerprint = agent_config_source_fingerprint(discovery, &agent_ids)?;
    Ok(ConnectionRoutePlanRequest {
        discovery_revision: format!("agent-config-{fingerprint}"),
        source_fingerprint: Some(fingerprint),
        source_fingerprint_scope: Some("agent_configs".to_string()),
        bindings: bindings
            .iter()
            .map(|binding| ConnectionRouteBindingRequest {
                agent_id: binding.agent_id.clone(),
                candidate_id: binding.candidate_id.clone(),
                candidate_source: binding.candidate_source.clone(),
                model: binding.upstream.model.clone(),
            })
            .collect(),
    })
}

fn save_agent_route_sync_receipt(receipt: &AgentRouteSyncReceipt) {
    if let Ok(value) = serde_json::to_value(receipt) {
        let _ = write_json_app_state(&["routing", "agent-auto-follow-receipt.json"], &value);
    }
}

fn load_agent_route_sync_receipt() -> Option<AgentRouteSyncReceipt> {
    read_app_state(&["routing", "agent-auto-follow-receipt.json"])
        .ok()
        .and_then(|value| serde_json::from_value(value).ok())
        .filter(|receipt: &AgentRouteSyncReceipt| {
            receipt.schema_version == AGENT_AUTO_FOLLOW_SCHEMA_VERSION
        })
}

fn record_agent_auto_follow_receipt(
    state: &str,
    source_fingerprint: &str,
    changed_agents: Vec<String>,
    route_before_sha256: Option<String>,
    route_after_sha256: Option<String>,
    rollback_id: Option<String>,
    runtime_state: &str,
) {
    let receipt = AgentRouteSyncReceipt {
        schema_version: AGENT_AUTO_FOLLOW_SCHEMA_VERSION,
        state: state.to_string(),
        source_fingerprint: source_fingerprint.to_string(),
        changed_agents,
        route_before_sha256,
        route_after_sha256,
        rollback_id,
        runtime_state: runtime_state.to_string(),
        occurred_at_ms: epoch_ms(),
    };
    save_agent_route_sync_receipt(&receipt);
    let event = if state == "applied_verified" {
        "route_sync_applied"
    } else if state.contains("rolled_back") {
        "route_sync_rolled_back"
    } else {
        "route_sync_failed"
    };
    let _ = append_desktop_event(
        event,
        if state == "applied_verified" {
            "ok"
        } else {
            state
        },
        json!({
            "source": "agent-config",
            "source_fingerprint": source_fingerprint,
            "changed_agents": receipt.changed_agents,
            "route_before_sha256": receipt.route_before_sha256,
            "route_after_sha256": receipt.route_after_sha256,
            "rollback_id": receipt.rollback_id,
            "runtime_state": runtime_state,
            "auto_follow": true,
        }),
    );
}

fn active_config_matches_agent_follow_bindings(
    bindings: &[AgentRouteFollowBinding],
) -> Result<(), String> {
    let config = read_app_state(&["active-config", "dtoken-config.json"])
        .map_err(|error| format!("Active route verification failed: {error}"))?;
    let providers = config
        .get("providers")
        .and_then(Value::as_array)
        .ok_or_else(|| "Active config providers are unavailable".to_string())?;
    let routes = config
        .get("routes")
        .and_then(Value::as_array)
        .ok_or_else(|| "Active config routes are unavailable".to_string())?;
    let instances = config
        .get("instances")
        .and_then(Value::as_array)
        .ok_or_else(|| "Active config instances are unavailable".to_string())?;
    for binding in bindings {
        let route_id = safe_slug(
            &format!("auto-{}-route", binding.agent_id),
            &format!("auto-{}", binding.agent_id),
        );
        let route = routes
            .iter()
            .find(|route| route.get("id").and_then(Value::as_str) == Some(route_id.as_str()))
            .ok_or_else(|| format!("Active route for {} is missing", binding.agent_id))?;
        let provider_id = route
            .get("target_provider_id")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                format!(
                    "Active route for {} has no target provider",
                    binding.agent_id
                )
            })?;
        let provider = providers
            .iter()
            .find(|provider| provider.get("id").and_then(Value::as_str) == Some(provider_id))
            .ok_or_else(|| format!("Active provider for {} is unavailable", binding.agent_id))?;
        let endpoint = provider
            .get("base_url")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                format!(
                    "Active provider endpoint for {} is unavailable",
                    binding.agent_id
                )
            })
            .and_then(normalize_follow_endpoint)?;
        if endpoint != binding.upstream.normalized_endpoint
            || provider.get("protocol").and_then(Value::as_str)
                != Some(binding.upstream.protocol.as_str())
            || !provider
                .get("models")
                .and_then(Value::as_array)
                .is_some_and(|models| {
                    models
                        .iter()
                        .any(|model| model.as_str() == Some(binding.upstream.model.as_str()))
                })
        {
            return Err(format!(
                "Active provider identity for {} does not match the followed upstream",
                binding.agent_id
            ));
        }
        let instance_id = route
            .get("instance_id")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("Active route for {} has no instance", binding.agent_id))?;
        if !instances.iter().any(|instance| {
            instance.get("id").and_then(Value::as_str) == Some(instance_id)
                && instance.get("agent").and_then(Value::as_str) == Some(binding.agent_id.as_str())
                && instance.get("route_id").and_then(Value::as_str) == Some(route_id.as_str())
        }) {
            return Err(format!(
                "Active instance for {} does not match the followed route",
                binding.agent_id
            ));
        }
    }
    Ok(())
}

fn execute_agent_auto_follow() -> Value {
    let mut authorization = load_agent_auto_follow_authorization();
    if !authorization.enabled {
        return json!({"state":"not_authorized"});
    }
    let agent_ids = authorization
        .last_synced_bindings
        .iter()
        .map(|binding| binding.agent_id.clone())
        .collect::<Vec<_>>();
    if agent_ids.is_empty() {
        authorization.enabled = false;
        authorization.last_sync_state = "empty_authorization".to_string();
        let _ = save_agent_auto_follow_authorization(&authorization);
        return json!({"state":"empty_authorization"});
    }
    let discovery = enrich_upstream_discovery(upstream_discovery::discover());
    let fingerprints = match agent_config_fingerprints(&discovery, &agent_ids) {
        Ok(fingerprints) => fingerprints,
        Err(error) => {
            return json!({"state":"discovery_unavailable", "summary":sanitize_command_output(&error)})
        }
    };
    let source_fingerprint = match agent_config_source_fingerprint(&discovery, &agent_ids) {
        Ok(fingerprint) => fingerprint,
        Err(error) => {
            return json!({"state":"discovery_unavailable", "summary":sanitize_command_output(&error)})
        }
    };
    let changed_agents = agent_ids
        .iter()
        .filter(|agent_id| {
            authorization
                .last_observed_config_fingerprints
                .get(*agent_id)
                != fingerprints.get(*agent_id)
        })
        .cloned()
        .collect::<HashSet<_>>();
    if changed_agents.is_empty() {
        return json!({
            "state":"unchanged",
            "source_fingerprint":source_fingerprint,
            "route_signature":authorization.last_synced_route_signature,
        });
    }
    let mut changed_agents_list = changed_agents.iter().cloned().collect::<Vec<_>>();
    changed_agents_list.sort();
    authorization.last_observed_config_fingerprints = fingerprints.clone();
    authorization.last_observed_source_fingerprint = Some(source_fingerprint.clone());
    let bindings = match resolve_agent_follow_bindings(&authorization, &discovery, &changed_agents)
    {
        Ok(bindings) => bindings,
        Err(error) => {
            authorization.last_sync_state = "reauthorization_required".to_string();
            let _ = save_agent_auto_follow_authorization(&authorization);
            record_agent_auto_follow_receipt(
                "reauthorization_required",
                &source_fingerprint,
                changed_agents_list.clone(),
                None,
                None,
                None,
                "not_reloaded",
            );
            let _ = append_desktop_event(
                "route_sync_needed",
                "reauthorization_required",
                json!({
                    "source": "agent-config",
                    "source_fingerprint": source_fingerprint,
                    "changed_agents": changed_agents_list,
                    "auto_follow": true,
                }),
            );
            return json!({
                "state":"reauthorization_required",
                "summary":sanitize_command_output(&error),
            });
        }
    };
    let route_signature = agent_follow_route_signature(&bindings);
    if authorization.last_synced_route_signature.as_deref() == Some(route_signature.as_str()) {
        authorization.last_sync_state = "metadata_changed_route_unchanged".to_string();
        let _ = save_agent_auto_follow_authorization(&authorization);
        return json!({
            "state":"unchanged",
            "source_fingerprint":source_fingerprint,
            "route_signature":route_signature,
            "changed_agents":changed_agents_list,
        });
    }
    let current_active_sha256 = match active_config_sha256() {
        Ok(sha256) => sha256,
        Err(error) => {
            authorization.last_sync_state = "active_route_unavailable".to_string();
            let _ = save_agent_auto_follow_authorization(&authorization);
            return json!({"state":"active_route_unavailable","summary":sanitize_command_output(&error)});
        }
    };
    if current_active_sha256 != authorization.active_config_sha256 {
        authorization.last_sync_state = "active_route_changed_reauthorization_required".to_string();
        let _ = save_agent_auto_follow_authorization(&authorization);
        record_agent_auto_follow_receipt(
            "active_route_changed_reauthorization_required",
            &source_fingerprint,
            changed_agents_list,
            Some(current_active_sha256),
            None,
            None,
            "not_reloaded",
        );
        return json!({
            "state":"reauthorization_required",
            "summary":"The d-token active route changed outside this follow authorization",
        });
    }
    let runtime_status = dtoken_service_status_value_fresh();
    if runtime_status.get("state").and_then(Value::as_str) == Some("running")
        && authorization.runtime_identity.as_deref()
            != runtime_status
                .get("runtime_identity")
                .and_then(Value::as_str)
    {
        authorization.last_sync_state = "runtime_identity_changed".to_string();
        let _ = save_agent_auto_follow_authorization(&authorization);
        return json!({
            "state":"reauthorization_required",
            "summary":"The managed runtime identity changed and must be reverified before Agent follow can continue",
        });
    }
    authorization.last_sync_state = "evaluating".to_string();
    if let Err(error) = save_agent_auto_follow_authorization(&authorization) {
        return json!({"state":"authorization_persist_failed","summary":sanitize_command_output(&error)});
    }
    let _ = append_desktop_event(
        "external_config_changed",
        "observed",
        json!({
            "source": "agent-config",
            "source_fingerprint": source_fingerprint,
            "changed_agents": changed_agents_list,
            "auto_follow": true,
        }),
    );
    let synthetic = match discovery_with_agent_follow_bindings(discovery.clone(), &bindings) {
        Ok(synthetic) => synthetic,
        Err(error) => {
            authorization.last_sync_state = "blocked".to_string();
            let _ = save_agent_auto_follow_authorization(&authorization);
            return json!({"state":"blocked","summary":sanitize_command_output(&error)});
        }
    };
    let plan = match agent_follow_plan(&synthetic, &bindings) {
        Ok(plan) => plan,
        Err(error) => {
            authorization.last_sync_state = "blocked".to_string();
            let _ = save_agent_auto_follow_authorization(&authorization);
            return json!({"state":"blocked","summary":sanitize_command_output(&error)});
        }
    };
    let (revision, prepared, agents) =
        match prepare_connection_route_plan_from_discovery(&plan, &synthetic) {
            Ok(prepared) => prepared,
            Err(error) => {
                authorization.last_sync_state = "blocked".to_string();
                let _ = save_agent_auto_follow_authorization(&authorization);
                record_agent_auto_follow_receipt(
                    "blocked",
                    &source_fingerprint,
                    changed_agents_list,
                    None,
                    None,
                    None,
                    "not_reloaded",
                );
                return json!({"state":"blocked","summary":sanitize_command_output(&error)});
            }
        };
    let transaction = match config_patcher().apply_transaction(&[prepared]) {
        Ok(transaction) => transaction,
        Err(error) => {
            authorization.last_sync_state = "apply_failed".to_string();
            let _ = save_agent_auto_follow_authorization(&authorization);
            record_agent_auto_follow_receipt(
                "apply_failed",
                &source_fingerprint,
                changed_agents_list,
                None,
                None,
                None,
                "not_reloaded",
            );
            return json!({"state":"apply_failed","summary":sanitize_command_output(&format!("Agent route sync failed: {error:#}"))});
        }
    };
    let Some(change) = transaction.changes.first() else {
        authorization.last_sync_state = "apply_failed".to_string();
        let _ = save_agent_auto_follow_authorization(&authorization);
        return json!({"state":"apply_failed","summary":"Agent route sync returned no verification receipt"});
    };
    let before_sha256 = change.before.sha256.clone();
    let after_sha256 = change.after.sha256.clone();
    let rollback_id = change.backup.rollback_id.clone();
    let latest_discovery = enrich_upstream_discovery(upstream_discovery::discover());
    let latest_fingerprint = agent_config_source_fingerprint(&latest_discovery, &agent_ids);
    let latest_bindings =
        resolve_agent_follow_bindings(&authorization, &latest_discovery, &changed_agents);
    let source_still_matches = latest_fingerprint.as_deref() == Ok(source_fingerprint.as_str())
        && latest_bindings
            .as_ref()
            .is_ok_and(|latest| agent_follow_route_signature(latest) == route_signature);
    if !source_still_matches {
        let rollback_result = config_patcher().restore_backup(
            &change.backup,
            &ExpectedFileState::Hash(after_sha256.clone().unwrap_or_default()),
        );
        authorization.last_sync_state = if rollback_result.is_ok() {
            "source_changed_rolled_back".to_string()
        } else {
            "source_changed_rollback_failed".to_string()
        };
        let _ = save_agent_auto_follow_authorization(&authorization);
        record_agent_auto_follow_receipt(
            &authorization.last_sync_state,
            &source_fingerprint,
            changed_agents_list,
            before_sha256,
            after_sha256,
            Some(rollback_id),
            "not_reloaded",
        );
        return json!({
            "state":authorization.last_sync_state,
            "summary":"Agent configuration changed during apply; the d-token route was restored when possible",
            "agents":agents,
            "discovery_revision":revision,
        });
    }
    if let Err(error) = active_config_matches_agent_follow_bindings(&bindings) {
        let rollback_result = config_patcher().restore_backup(
            &change.backup,
            &ExpectedFileState::Hash(after_sha256.clone().unwrap_or_default()),
        );
        authorization.last_sync_state = if rollback_result.is_ok() {
            "verification_failed_rolled_back".to_string()
        } else {
            "verification_failed_rollback_failed".to_string()
        };
        let _ = save_agent_auto_follow_authorization(&authorization);
        record_agent_auto_follow_receipt(
            &authorization.last_sync_state,
            &source_fingerprint,
            changed_agents_list,
            before_sha256,
            after_sha256,
            Some(rollback_id),
            "not_reloaded",
        );
        return json!({
            "state":authorization.last_sync_state,
            "summary":sanitize_command_output(&error),
        });
    }
    let reload = reload_managed_runtime_after_route_sync();
    let runtime_state = reload
        .get("state")
        .and_then(Value::as_str)
        .unwrap_or("runtime_unknown");
    if !matches!(runtime_state, "runtime_reloaded" | "runtime_not_running") {
        let rollback_result = config_patcher().restore_backup(
            &change.backup,
            &ExpectedFileState::Hash(after_sha256.clone().unwrap_or_default()),
        );
        authorization.last_sync_state = if rollback_result.is_ok() {
            format!("rolled_back_{runtime_state}")
        } else {
            format!("{runtime_state}_rollback_failed")
        };
        let _ = save_agent_auto_follow_authorization(&authorization);
        record_agent_auto_follow_receipt(
            &authorization.last_sync_state,
            &source_fingerprint,
            changed_agents_list,
            before_sha256,
            after_sha256,
            Some(rollback_id),
            runtime_state,
        );
        return json!({
            "state":authorization.last_sync_state,
            "summary":"Runtime reload was unavailable; the d-token route snapshot was restored when possible",
            "runtime":reload,
        });
    }
    let current_runtime = dtoken_service_status_value_fresh();
    if runtime_state == "runtime_reloaded"
        && (current_runtime.get("state").and_then(Value::as_str) != Some("running")
            || current_runtime.get("managed").and_then(Value::as_bool) != Some(true))
    {
        let rollback_result = config_patcher().restore_backup(
            &change.backup,
            &ExpectedFileState::Hash(after_sha256.clone().unwrap_or_default()),
        );
        authorization.last_sync_state = if rollback_result.is_ok() {
            "runtime_verify_failed_rolled_back".to_string()
        } else {
            "runtime_verify_failed_rollback_failed".to_string()
        };
        let _ = save_agent_auto_follow_authorization(&authorization);
        record_agent_auto_follow_receipt(
            &authorization.last_sync_state,
            &source_fingerprint,
            changed_agents_list,
            before_sha256,
            after_sha256,
            Some(rollback_id),
            "runtime_verify_failed",
        );
        return json!({
            "state":authorization.last_sync_state,
            "summary":"The managed runtime did not verify the new route and the route file was restored when possible",
        });
    }
    authorization.last_synced_bindings = bindings;
    authorization.last_synced_route_signature = Some(route_signature.clone());
    authorization.last_synced_source_fingerprint = Some(source_fingerprint.clone());
    authorization.active_config_sha256 = after_sha256.clone().unwrap_or_default();
    authorization.runtime_identity = current_runtime
        .get("runtime_identity")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or(authorization.runtime_identity);
    authorization.last_sync_state = "applied_verified".to_string();
    let _ = save_agent_auto_follow_authorization(&authorization);
    record_agent_auto_follow_receipt(
        "applied_verified",
        &source_fingerprint,
        changed_agents_list.clone(),
        before_sha256.clone(),
        after_sha256.clone(),
        Some(rollback_id.clone()),
        runtime_state,
    );
    json!({
        "state":"applied_verified",
        "discovery_revision":revision,
        "source_fingerprint":source_fingerprint,
        "route_signature":route_signature,
        "before_sha256":before_sha256,
        "after_sha256":after_sha256,
        "rollback_id":rollback_id,
        "runtime":reload,
        "agents":agents,
        "changed_agents":changed_agents_list,
    })
}

/// Builds a d-token-only route plan from one unambiguous cc-switch active
/// provider. cc-switch remains read-only: the resulting plan changes only the
/// d-token active config and is still subject to the normal review/apply
/// transaction.
///
/// Stable, reproducible target list for a cc-switch sync plan, derived from
/// the plan bindings sorted by agent id. Both Doctor preview and apply
/// compute it from the same plan shape so the review digest is reproducible;
/// any cc-switch change between preview and apply changes the bindings and
/// therefore the digest, which is the stale-rejection signal.
fn cc_switch_plan_targets(plan: &ConnectionRoutePlanRequest) -> Vec<Value> {
    let mut bindings = plan.bindings.clone();
    bindings.sort_by(|left, right| left.agent_id.cmp(&right.agent_id));
    bindings
        .into_iter()
        .map(|binding| {
            json!({
                "agent_id": binding.agent_id,
                "candidate_id": binding.candidate_id,
                "candidate_source": binding.candidate_source,
                "model": binding.model,
            })
        })
        .collect()
}

fn prepare_cc_switch_sync_plan(
    request: &CcSwitchSyncPlanRequest,
) -> Result<ConnectionRoutePlanRequest, String> {
    let discovery = enrich_upstream_discovery(upstream_discovery::discover());
    prepare_cc_switch_sync_plan_from_discovery(request, &discovery)
}

fn prepare_cc_switch_sync_plan_from_discovery(
    request: &CcSwitchSyncPlanRequest,
    discovery: &Value,
) -> Result<ConnectionRoutePlanRequest, String> {
    let current_revision = discovery
        .get("external_config_snapshot")
        .and_then(|snapshot| snapshot.get("cc_switch"))
        .and_then(|cc_switch| cc_switch.get("revision"))
        .and_then(Value::as_str)
        .ok_or_else(|| "Current cc-switch revision is unavailable".to_string())?;
    if current_revision != request.discovery_revision {
        return Err("cc-switch changed after discovery; scan again before review".to_string());
    }
    let fingerprint = discovery
        .get("external_config_snapshot")
        .and_then(|snapshot| snapshot.get("cc_switch"))
        .and_then(|cc_switch| cc_switch.get("fingerprint"))
        .and_then(Value::as_str)
        .ok_or_else(|| "cc-switch source fingerprint is unavailable".to_string())?;
    if fingerprint != request.source_fingerprint {
        return Err(
            "cc-switch metadata changed after discovery; scan again before review".to_string(),
        );
    }
    let active_identity = cc_switch_active_route_identity(discovery)?;
    let authorizable = authorizable_cc_switch_upstream_signatures(discovery)?;
    if !authorizable.contains(&active_identity.upstream_signature) {
        return Err(
            "The active cc-switch upstream is not credential-free or has insufficient evidence"
                .to_string(),
        );
    }
    let upstreams = discovery
        .get("cc_switch_sync")
        .and_then(|sync| sync.get("active_upstreams"))
        .and_then(Value::as_array)
        .ok_or_else(|| "cc-switch has no safely readable active upstream".to_string())?;
    if upstreams.len() != 1 {
        return Err(
            "cc-switch has zero or multiple active upstreams; choose one before syncing"
                .to_string(),
        );
    }
    let upstream = &upstreams[0];
    let candidate_id = upstream
        .get("provider_id")
        .and_then(Value::as_str)
        .ok_or_else(|| "cc-switch active provider id is unavailable".to_string())?;
    let candidate_source = upstream
        .get("source")
        .and_then(Value::as_str)
        .ok_or_else(|| "cc-switch active provider source is unavailable".to_string())?;
    let model = upstream
        .get("models")
        .and_then(Value::as_array)
        .and_then(|models| (models.len() == 1).then_some(models))
        .and_then(|models| models.first())
        .and_then(Value::as_str)
        .ok_or_else(|| "cc-switch active provider model is ambiguous or unavailable".to_string())?;
    let agent_ids = upstream
        .get("agent_ids")
        .and_then(Value::as_array)
        .ok_or_else(|| "cc-switch active provider has no Agent mapping".to_string())?;
    let bindings = discovery
        .get("agent_bindings")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|binding| {
            let agent_id = binding.get("agent_id").and_then(Value::as_str)?;
            let mapped = agent_ids
                .iter()
                .any(|mapped| mapped.as_str() == Some(agent_id));
            let candidate_exists = binding
                .get("candidates")
                .and_then(Value::as_array)
                .is_some_and(|candidates| {
                    candidates.iter().any(|candidate| {
                        candidate.get("id").and_then(Value::as_str) == Some(candidate_id)
                            && candidate.get("source").and_then(Value::as_str)
                                == Some(candidate_source)
                            && candidate.get("auth_mode").and_then(Value::as_str) == Some("none")
                            && candidate
                                .get("models")
                                .and_then(Value::as_array)
                                .is_some_and(|models| {
                                    models.iter().any(|value| value.as_str() == Some(model))
                                })
                    })
                });
            (mapped
                && binding.get("installed").and_then(Value::as_bool) == Some(true)
                && candidate_exists)
                .then(|| ConnectionRouteBindingRequest {
                    agent_id: agent_id.to_string(),
                    candidate_id: candidate_id.to_string(),
                    candidate_source: candidate_source.to_string(),
                    model: model.to_string(),
                })
        })
        .collect::<Vec<_>>();
    if bindings.is_empty() {
        return Err("No installed Agent can safely use the current cc-switch upstream".to_string());
    }
    Ok(ConnectionRoutePlanRequest {
        discovery_revision: request.discovery_revision.clone(),
        source_fingerprint: Some(request.source_fingerprint.clone()),
        source_fingerprint_scope: Some("cc_switch".to_string()),
        bindings,
    })
}

fn cc_switch_sync_request_from_discovery(
    discovery: &Value,
) -> Result<CcSwitchSyncPlanRequest, String> {
    let cc_switch = discovery
        .get("external_config_snapshot")
        .and_then(|snapshot| snapshot.get("cc_switch"))
        .ok_or_else(|| "cc-switch snapshot is unavailable".to_string())?;
    let discovery_revision = cc_switch
        .get("revision")
        .and_then(Value::as_str)
        .ok_or_else(|| "Current cc-switch revision is unavailable".to_string())?
        .to_string();
    let source_fingerprint = cc_switch
        .get("fingerprint")
        .and_then(Value::as_str)
        .ok_or_else(|| "cc-switch source fingerprint is unavailable".to_string())?
        .to_string();
    Ok(CcSwitchSyncPlanRequest {
        discovery_revision,
        source_fingerprint,
    })
}

fn canonical_cc_switch_protocol(value: &str) -> Result<&'static str, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "anthropic" => Ok("anthropic"),
        "open_ai" | "openai" => Ok("open_ai"),
        _ => Err("cc-switch protocol is unknown and requires explicit review".to_string()),
    }
}

fn canonical_cc_switch_endpoint(value: &str) -> Result<String, String> {
    let mut parsed = url::Url::parse(value)
        .map_err(|_| "cc-switch endpoint is invalid and requires explicit review".to_string())?;
    if !matches!(parsed.scheme(), "http" | "https")
        || parsed.host_str().is_none()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
    {
        return Err(
            "cc-switch endpoint is unsafe or incomplete and requires explicit review".to_string(),
        );
    }
    parsed.set_query(None);
    parsed.set_fragment(None);
    let normalized_path = parsed.path().trim_end_matches('/').to_string();
    parsed.set_path(&normalized_path);
    Ok(parsed.as_str().trim_end_matches('/').to_string())
}

fn cc_switch_upstream_signature(
    provider_id: &str,
    source: &str,
    protocol: &str,
    endpoint: &str,
) -> Result<String, String> {
    let provider_id = provider_id.trim();
    let source = source.trim();
    if provider_id.is_empty()
        || source.is_empty()
        || provider_id.contains(['\n', '\r', '\0'])
        || source.contains(['\n', '\r', '\0'])
    {
        return Err("cc-switch provider identity is invalid".to_string());
    }
    let protocol = canonical_cc_switch_protocol(protocol)?;
    let endpoint = canonical_cc_switch_endpoint(endpoint)?;
    Ok(stable_hash(&format!(
        "cc-switch-upstream-v1\n{provider_id}\n{source}\n{protocol}\n{endpoint}"
    )))
}

fn cc_switch_active_route_identity(discovery: &Value) -> Result<CcSwitchRouteIdentity, String> {
    let upstreams = discovery
        .get("cc_switch_sync")
        .and_then(|sync| sync.get("active_upstreams"))
        .and_then(Value::as_array)
        .ok_or_else(|| "cc-switch active upstream evidence is unavailable".to_string())?;
    if upstreams.len() != 1 {
        return Err(
            "cc-switch has zero or multiple active upstreams; explicit selection is required"
                .to_string(),
        );
    }
    let upstream = &upstreams[0];
    let provider_id = upstream
        .get("provider_id")
        .and_then(Value::as_str)
        .ok_or_else(|| "cc-switch active provider id is unavailable".to_string())?;
    let source = upstream
        .get("source")
        .and_then(Value::as_str)
        .ok_or_else(|| "cc-switch active provider source is unavailable".to_string())?;
    let protocol = upstream
        .get("protocol")
        .and_then(Value::as_str)
        .ok_or_else(|| "cc-switch active protocol is unavailable".to_string())?;
    let endpoint = upstream
        .get("upstream")
        .and_then(Value::as_str)
        .ok_or_else(|| "cc-switch active endpoint is unavailable".to_string())?;
    let models = upstream
        .get("models")
        .and_then(Value::as_array)
        .ok_or_else(|| "cc-switch active model evidence is unavailable".to_string())?;
    let model = (models.len() == 1)
        .then(|| models[0].as_str())
        .flatten()
        .ok_or_else(|| "cc-switch active model is ambiguous or unavailable".to_string())?;
    if model.trim().is_empty() || model.len() > 256 || model.contains(['\n', '\r', '\0']) {
        return Err("cc-switch active model is invalid".to_string());
    }
    let upstream_signature = cc_switch_upstream_signature(provider_id, source, protocol, endpoint)?;
    let route_signature = stable_hash(&format!(
        "cc-switch-route-v1\n{upstream_signature}\n{}",
        model.trim()
    ));
    Ok(CcSwitchRouteIdentity {
        upstream_signature,
        route_signature,
    })
}

fn authorizable_cc_switch_upstream_signatures(discovery: &Value) -> Result<Vec<String>, String> {
    let candidates = discovery
        .get("candidates")
        .and_then(Value::as_array)
        .ok_or_else(|| "cc-switch provider candidates are unavailable".to_string())?;
    let mut signatures = candidates
        .iter()
        .filter(|candidate| {
            candidate
                .get("source")
                .and_then(Value::as_str)
                .is_some_and(|source| source.starts_with("cc-switch:"))
                && candidate.get("auth_mode").and_then(Value::as_str) == Some("none")
                && candidate
                    .get("confidence")
                    .and_then(Value::as_u64)
                    .is_some_and(|confidence| confidence >= 70)
        })
        .filter_map(|candidate| {
            cc_switch_upstream_signature(
                candidate.get("id").and_then(Value::as_str)?,
                candidate.get("source").and_then(Value::as_str)?,
                candidate.get("protocol").and_then(Value::as_str)?,
                candidate.get("base_url").and_then(Value::as_str)?,
            )
            .ok()
        })
        .collect::<Vec<_>>();
    signatures.sort();
    signatures.dedup();
    if signatures.is_empty() {
        return Err(
            "No credential-free cc-switch upstream identity is safe to authorize".to_string(),
        );
    }
    Ok(signatures)
}

fn cc_switch_follow_decision(
    authorization: &CcSwitchAutoFollowAuthorization,
    identity: &CcSwitchRouteIdentity,
) -> &'static str {
    if !authorization
        .authorized_upstream_signatures
        .contains(&identity.upstream_signature)
    {
        "reauthorization_required"
    } else if authorization.last_synced_route_signature.as_deref()
        == Some(identity.route_signature.as_str())
    {
        "unchanged"
    } else {
        "sync_required"
    }
}

fn default_cc_switch_auto_follow_authorization() -> CcSwitchAutoFollowAuthorization {
    CcSwitchAutoFollowAuthorization {
        schema_version: CC_SWITCH_AUTO_FOLLOW_SCHEMA_VERSION,
        enabled: false,
        authorized_at_ms: 0,
        authorized_source_fingerprint: String::new(),
        authorized_upstream_signatures: Vec::new(),
        last_observed_source_fingerprint: None,
        last_synced_source_fingerprint: None,
        last_observed_route_signature: None,
        last_synced_route_signature: None,
        last_sync_state: "not_authorized".to_string(),
    }
}

fn load_cc_switch_auto_follow_authorization() -> CcSwitchAutoFollowAuthorization {
    read_app_state(&["routing", "cc-switch-auto-follow.json"])
        .ok()
        .and_then(|value| serde_json::from_value(value).ok())
        .filter(|authorization: &CcSwitchAutoFollowAuthorization| {
            authorization.schema_version == CC_SWITCH_AUTO_FOLLOW_SCHEMA_VERSION
        })
        .unwrap_or_else(default_cc_switch_auto_follow_authorization)
}

fn save_cc_switch_auto_follow_authorization(
    authorization: &CcSwitchAutoFollowAuthorization,
) -> Result<(), String> {
    let value = serde_json::to_value(authorization)
        .map_err(|error| format!("cc-switch follow authorization serialize failed: {error}"))?;
    write_json_app_state(&["routing", "cc-switch-auto-follow.json"], &value).map(|_| ())
}

fn authorize_cc_switch_auto_follow(
    request: &CcSwitchSyncPlanRequest,
) -> Result<CcSwitchAutoFollowAuthorization, String> {
    let discovery = enrich_upstream_discovery(upstream_discovery::discover());
    // Preparing against the same snapshot performs the required revision,
    // fingerprint, unique-active-upstream, credential, model, protocol, and
    // Agent-mapping checks before any persistent opt-in is recorded.
    let _ = prepare_cc_switch_sync_plan_from_discovery(request, &discovery)?;
    let current_identity = cc_switch_active_route_identity(&discovery)?;
    let authorized_upstream_signatures = authorizable_cc_switch_upstream_signatures(&discovery)?;
    if !authorized_upstream_signatures.contains(&current_identity.upstream_signature) {
        return Err(
            "The active cc-switch upstream is outside the approved identity set".to_string(),
        );
    }
    let authorization = CcSwitchAutoFollowAuthorization {
        schema_version: CC_SWITCH_AUTO_FOLLOW_SCHEMA_VERSION,
        enabled: true,
        authorized_at_ms: epoch_ms(),
        authorized_source_fingerprint: request.source_fingerprint.clone(),
        authorized_upstream_signatures,
        last_observed_source_fingerprint: Some(request.source_fingerprint.clone()),
        last_synced_source_fingerprint: None,
        last_observed_route_signature: Some(current_identity.route_signature),
        last_synced_route_signature: None,
        last_sync_state: "authorized_waiting_for_change".to_string(),
    };
    save_cc_switch_auto_follow_authorization(&authorization)?;
    Ok(authorization)
}

/// A manual initial sync may leave the reviewed d-token route in place, but
/// it must not persist auto-follow authorization until the existing managed
/// runtime reload receipt proves the route is live (or that no runtime is
/// running and the route will be loaded on the next managed start).
fn runtime_reload_receipt_allows_initial_auto_follow(reload: &Value) -> bool {
    reload.get("ok").and_then(Value::as_bool) == Some(true)
        && matches!(
            reload.get("state").and_then(Value::as_str),
            Some("runtime_reloaded" | "runtime_not_running")
        )
}

fn authorize_initial_cc_switch_auto_follow_after_runtime_reload(
    request: &CcSwitchSyncPlanRequest,
    reload: &Value,
) -> Result<CcSwitchAutoFollowAuthorization, Value> {
    if !runtime_reload_receipt_allows_initial_auto_follow(reload) {
        return Err(json!({
            "enabled": false,
            "runtime": reload,
            "summary": "The reviewed route was applied, but auto-follow was not authorized because the managed runtime did not provide a valid reload receipt",
        }));
    }

    let mut authorization = authorize_cc_switch_auto_follow(request).map_err(|error| {
        json!({
            "enabled": false,
            "runtime": reload,
            "summary": sanitize_command_output(&error),
        })
    })?;
    authorization.last_synced_source_fingerprint = Some(request.source_fingerprint.clone());
    authorization.last_synced_route_signature = authorization.last_observed_route_signature.clone();
    authorization.last_sync_state = "applied_verified".to_string();
    save_cc_switch_auto_follow_authorization(&authorization).map_err(|error| {
        json!({
            "enabled": false,
            "runtime": reload,
            "summary": sanitize_command_output(&error),
        })
    })?;
    Ok(authorization)
}

fn save_cc_switch_route_sync_receipt(receipt: &CcSwitchRouteSyncReceipt) {
    if let Ok(value) = serde_json::to_value(receipt) {
        let _ = write_json_app_state(&["routing", "cc-switch-auto-follow-receipt.json"], &value);
    }
}

fn load_cc_switch_route_sync_receipt() -> Option<CcSwitchRouteSyncReceipt> {
    read_app_state(&["routing", "cc-switch-auto-follow-receipt.json"])
        .ok()
        .and_then(|value| serde_json::from_value(value).ok())
}

fn record_cc_switch_auto_follow_receipt(
    state: &str,
    source_fingerprint: &str,
    route_before_sha256: Option<String>,
    route_after_sha256: Option<String>,
    rollback_id: Option<String>,
    runtime_state: &str,
) {
    let receipt = CcSwitchRouteSyncReceipt {
        schema_version: CC_SWITCH_AUTO_FOLLOW_SCHEMA_VERSION,
        state: state.to_string(),
        source_fingerprint: source_fingerprint.to_string(),
        route_before_sha256,
        route_after_sha256,
        rollback_id,
        runtime_state: runtime_state.to_string(),
        occurred_at_ms: epoch_ms(),
    };
    save_cc_switch_route_sync_receipt(&receipt);
    let event = if state == "applied_verified" {
        "route_sync_applied"
    } else if state.contains("rolled_back") {
        "route_sync_rolled_back"
    } else {
        "route_sync_failed"
    };
    let _ = append_desktop_event(
        event,
        if state == "applied_verified" {
            "ok"
        } else {
            state
        },
        json!({
            "source_fingerprint": source_fingerprint,
            "route_before_sha256": receipt.route_before_sha256,
            "route_after_sha256": receipt.route_after_sha256,
            "rollback_id": receipt.rollback_id,
            "runtime_state": runtime_state,
            "auto_follow": true,
        }),
    );
}

/// Executes only after a user persisted an authorization. It re-discovers and
/// validates cc-switch immediately before each transaction; it never imports a
/// credential and never writes the cc-switch database, WAL, SHM, or profile.
fn execute_cc_switch_auto_follow() -> Value {
    let mut authorization = load_cc_switch_auto_follow_authorization();
    if !authorization.enabled {
        return json!({"state":"not_authorized"});
    }
    let discovery = enrich_upstream_discovery(upstream_discovery::discover());
    let request = match cc_switch_sync_request_from_discovery(&discovery) {
        Ok(request) => request,
        Err(error) => {
            return json!({"state":"discovery_unavailable", "summary": sanitize_command_output(&error)})
        }
    };
    let identity = match cc_switch_active_route_identity(&discovery) {
        Ok(identity) => identity,
        Err(error) => {
            authorization.last_observed_source_fingerprint =
                Some(request.source_fingerprint.clone());
            authorization.last_sync_state = "reauthorization_required".to_string();
            let _ = save_cc_switch_auto_follow_authorization(&authorization);
            record_cc_switch_auto_follow_receipt(
                "reauthorization_required",
                &request.source_fingerprint,
                None,
                None,
                None,
                "not_reloaded",
            );
            return json!({
                "state":"reauthorization_required",
                "summary":sanitize_command_output(&error),
            });
        }
    };
    if authorization.last_observed_source_fingerprint.as_deref()
        == Some(request.source_fingerprint.as_str())
        && authorization.last_observed_route_signature.as_deref()
            == Some(identity.route_signature.as_str())
    {
        return json!({
            "state":"unchanged",
            "source_fingerprint":request.source_fingerprint,
            "route_signature":identity.route_signature,
        });
    }

    // Record the observation before a potentially failing read/apply. This
    // prevents the polling fallback from repeatedly attempting an ambiguous or
    // credential-bearing selection; a new external fingerprint re-enables a
    // fresh evaluation.
    authorization.last_observed_source_fingerprint = Some(request.source_fingerprint.clone());
    authorization.last_observed_route_signature = Some(identity.route_signature.clone());
    let follow_decision = cc_switch_follow_decision(&authorization, &identity);
    if follow_decision == "reauthorization_required" {
        authorization.last_sync_state = "reauthorization_required".to_string();
        let _ = save_cc_switch_auto_follow_authorization(&authorization);
        record_cc_switch_auto_follow_receipt(
            "reauthorization_required",
            &request.source_fingerprint,
            None,
            None,
            None,
            "not_reloaded",
        );
        let _ = append_desktop_event(
            "route_sync_needed",
            "reauthorization_required",
            json!({
                "source_fingerprint": request.source_fingerprint,
                "route_signature": identity.route_signature,
                "auto_follow": true,
            }),
        );
        return json!({
            "state":"reauthorization_required",
            "summary":"The active cc-switch provider, endpoint, or protocol is outside the approved follow set",
        });
    }
    if follow_decision == "unchanged" {
        authorization.last_sync_state = "metadata_changed_route_unchanged".to_string();
        let _ = save_cc_switch_auto_follow_authorization(&authorization);
        return json!({
            "state":"unchanged",
            "source_fingerprint":request.source_fingerprint,
            "route_signature":identity.route_signature,
        });
    }
    authorization.last_sync_state = "evaluating".to_string();
    if let Err(error) = save_cc_switch_auto_follow_authorization(&authorization) {
        return json!({"state":"authorization_persist_failed", "summary":sanitize_command_output(&error)});
    }
    let _ = append_desktop_event(
        "external_config_changed",
        "observed",
        json!({"source_fingerprint": request.source_fingerprint, "auto_follow": true}),
    );

    let plan = match prepare_cc_switch_sync_plan_from_discovery(&request, &discovery) {
        Ok(plan) => plan,
        Err(error) => {
            authorization.last_sync_state = "blocked".to_string();
            let _ = save_cc_switch_auto_follow_authorization(&authorization);
            record_cc_switch_auto_follow_receipt(
                "blocked",
                &request.source_fingerprint,
                None,
                None,
                None,
                "not_reloaded",
            );
            return json!({"state":"blocked", "summary":sanitize_command_output(&error)});
        }
    };
    let (revision, prepared, agents) = match prepare_connection_route_plan(&plan) {
        Ok(prepared) => prepared,
        Err(error) => {
            authorization.last_sync_state = "blocked".to_string();
            let _ = save_cc_switch_auto_follow_authorization(&authorization);
            record_cc_switch_auto_follow_receipt(
                "blocked",
                &request.source_fingerprint,
                None,
                None,
                None,
                "not_reloaded",
            );
            return json!({"state":"blocked", "summary":sanitize_command_output(&error)});
        }
    };
    let transaction = match config_patcher().apply_transaction(&[prepared]) {
        Ok(transaction) => transaction,
        Err(error) => {
            authorization.last_sync_state = "apply_failed".to_string();
            let _ = save_cc_switch_auto_follow_authorization(&authorization);
            record_cc_switch_auto_follow_receipt(
                "apply_failed",
                &request.source_fingerprint,
                None,
                None,
                None,
                "not_reloaded",
            );
            return json!({"state":"apply_failed", "summary":sanitize_command_output(&format!("cc-switch route sync failed: {error:#}"))});
        }
    };
    let Some(change) = transaction.changes.first() else {
        authorization.last_sync_state = "apply_failed".to_string();
        let _ = save_cc_switch_auto_follow_authorization(&authorization);
        record_cc_switch_auto_follow_receipt(
            "apply_failed",
            &request.source_fingerprint,
            None,
            None,
            None,
            "not_reloaded",
        );
        return json!({"state":"apply_failed", "summary":"cc-switch route sync returned no verification receipt"});
    };
    let before_sha256 = change.before.sha256.clone();
    let after_sha256 = change.after.sha256.clone();
    let rollback_id = change.backup.rollback_id.clone();
    let latest_discovery = enrich_upstream_discovery(upstream_discovery::discover());
    let latest_request = cc_switch_sync_request_from_discovery(&latest_discovery);
    let latest_identity = cc_switch_active_route_identity(&latest_discovery);
    let source_still_matches = latest_identity
        .as_ref()
        .is_ok_and(|latest| latest == &identity)
        && latest_identity.as_ref().is_ok_and(|latest| {
            authorization
                .authorized_upstream_signatures
                .contains(&latest.upstream_signature)
        });
    if !source_still_matches {
        let rollback_result = config_patcher().restore_backup(
            &change.backup,
            &ExpectedFileState::Hash(after_sha256.clone().unwrap_or_default()),
        );
        if let Ok(latest) = &latest_request {
            authorization.last_observed_source_fingerprint =
                Some(latest.source_fingerprint.clone());
        }
        if let Ok(latest) = &latest_identity {
            authorization.last_observed_route_signature = Some(latest.route_signature.clone());
        }
        authorization.last_sync_state = if rollback_result.is_ok() {
            "source_changed_rolled_back".to_string()
        } else {
            "source_changed_rollback_failed".to_string()
        };
        let _ = save_cc_switch_auto_follow_authorization(&authorization);
        record_cc_switch_auto_follow_receipt(
            &authorization.last_sync_state,
            &request.source_fingerprint,
            before_sha256,
            after_sha256,
            Some(rollback_id),
            "not_reloaded",
        );
        return json!({
            "state":authorization.last_sync_state,
            "summary":"cc-switch changed during apply; the d-token route was restored when possible",
            "agents":agents,
            "discovery_revision":revision,
        });
    }
    let verified_request = latest_request.expect("validated latest cc-switch request");
    let verified_identity = latest_identity.expect("validated latest cc-switch identity");
    let reload = reload_managed_runtime_after_route_sync();
    let runtime_state = reload
        .get("state")
        .and_then(Value::as_str)
        .unwrap_or("runtime_unknown");

    // A route file is not a runtime reload receipt. Keep the last-known-good
    // d-token route snapshot whenever the managed reload fails *or* the port
    // belongs to an unmanaged compatible runtime that d-token may not reload.
    if !matches!(runtime_state, "runtime_reloaded" | "runtime_not_running") {
        let rollback_result = config_patcher().restore_backup(
            &change.backup,
            &ExpectedFileState::Hash(after_sha256.clone().unwrap_or_default()),
        );
        authorization.last_sync_state = if rollback_result.is_ok() {
            format!("rolled_back_{runtime_state}")
        } else {
            format!("{runtime_state}_rollback_failed")
        };
        let _ = save_cc_switch_auto_follow_authorization(&authorization);
        record_cc_switch_auto_follow_receipt(
            &authorization.last_sync_state,
            &request.source_fingerprint,
            before_sha256,
            after_sha256,
            Some(rollback_id),
            runtime_state,
        );
        return json!({
            "state": authorization.last_sync_state,
            "summary": "Runtime reload was unavailable; the d-token route snapshot was restored when possible",
            "agents": agents,
            "discovery_revision": revision,
        });
    }

    authorization.last_observed_source_fingerprint =
        Some(verified_request.source_fingerprint.clone());
    authorization.last_synced_source_fingerprint =
        Some(verified_request.source_fingerprint.clone());
    authorization.last_observed_route_signature = Some(verified_identity.route_signature.clone());
    authorization.last_synced_route_signature = Some(verified_identity.route_signature.clone());
    authorization.last_sync_state = "applied_verified".to_string();
    let _ = save_cc_switch_auto_follow_authorization(&authorization);
    record_cc_switch_auto_follow_receipt(
        "applied_verified",
        &verified_request.source_fingerprint,
        before_sha256.clone(),
        after_sha256.clone(),
        Some(rollback_id.clone()),
        runtime_state,
    );
    json!({
        "state":"applied_verified",
        "discovery_revision": revision,
        "source_fingerprint": verified_request.source_fingerprint,
        "route_signature": verified_identity.route_signature,
        "before_sha256": before_sha256,
        "after_sha256": after_sha256,
        "rollback_id": rollback_id,
        "runtime": reload,
        "agents": agents,
    })
}

fn schedule_cc_switch_auto_follow() {
    let coordinator = CC_SWITCH_AUTO_FOLLOW_COORDINATOR
        .get_or_init(|| {
            Arc::new(CcSwitchAutoFollowCoordinator {
                pending: AtomicBool::new(false),
                running: AtomicBool::new(false),
            })
        })
        .clone();
    coordinator.pending.store(true, Ordering::Release);
    if coordinator.running.swap(true, Ordering::AcqRel) {
        return;
    }
    thread::spawn(move || {
        loop {
            thread::sleep(CC_SWITCH_AUTO_FOLLOW_DEBOUNCE);
            if !coordinator.pending.swap(false, Ordering::AcqRel) {
                break;
            }
            let _ = execute_cc_switch_auto_follow();
            let _ = execute_agent_auto_follow();
            if !coordinator.pending.load(Ordering::Acquire) {
                break;
            }
        }
        coordinator.running.store(false, Ordering::Release);
        // Close the small race where a notification arrives after the final
        // pending check but before the worker releases `running`.
        if coordinator.pending.load(Ordering::Acquire) {
            schedule_cc_switch_auto_follow();
        }
    });
}

fn start_external_route_follow_observer() {
    let callback: Arc<dyn Fn() + Send + Sync> = Arc::new(schedule_cc_switch_auto_follow);
    match upstream_discovery::start_external_config_watcher(callback) {
        Ok(watcher) => {
            let _ = append_desktop_event(
                "external_config_watcher",
                &watcher.state,
                json!({
                    "mode": watcher.mode,
                    "watched_paths": watcher.watched_paths,
                    "polling_fallback": watcher.polling_fallback,
                }),
            );
        }
        Err(error) => {
            let _ = append_desktop_event(
                "external_config_watcher",
                "unavailable",
                json!({
                    "mode": "polling_fallback_only",
                    "summary": sanitize_command_output(&error),
                }),
            );
        }
    }
    thread::spawn(|| loop {
        thread::sleep(CC_SWITCH_AUTO_FOLLOW_POLL_INTERVAL);
        schedule_cc_switch_auto_follow();
    });
}

#[tauri::command]
async fn set_cc_switch_auto_follow(request: SetCcSwitchAutoFollowRequest) -> Value {
    tauri::async_runtime::spawn_blocking(move || {
        if !request.enabled {
            let mut authorization = load_cc_switch_auto_follow_authorization();
            authorization.enabled = false;
            authorization.last_sync_state = "revoked".to_string();
            return match save_cc_switch_auto_follow_authorization(&authorization) {
                Ok(()) => {
                    let _ = append_desktop_event(
                        "route_sync_authorization",
                        "revoked",
                        json!({"auto_follow": false}),
                    );
                    json!({"ok":true, "state":"revoked", "auto_follow":authorization})
                }
                Err(error) => json!({
                    "ok":false,
                    "state":"failed",
                    "summary":sanitize_command_output(&error),
                }),
            };
        }
        let (Some(discovery_revision), Some(source_fingerprint)) = (
            request.discovery_revision,
            request.source_fingerprint,
        ) else {
            return json!({
                "ok":false,
                "state":"blocked",
                "summary":"A current cc-switch discovery revision and source fingerprint are required for authorization",
            });
        };
        match authorize_cc_switch_auto_follow(&CcSwitchSyncPlanRequest {
            discovery_revision,
            source_fingerprint,
        }) {
            Ok(authorization) => {
                let _ = append_desktop_event(
                    "route_sync_authorization",
                    "authorized",
                    json!({
                        "source_fingerprint": authorization.authorized_source_fingerprint,
                        "auto_follow": true,
                    }),
                );
                json!({"ok":true, "state":"authorized", "auto_follow":authorization})
            }
            Err(error) => json!({
                "ok":false,
                "state":"blocked",
                "summary":sanitize_command_output(&error),
            }),
        }
    })
    .await
    .unwrap_or_else(|error| {
        json!({
            "ok":false,
            "state":"failed",
            "summary":format!("cc-switch auto-follow authorization worker failed: {error}"),
        })
    })
}

#[tauri::command]
async fn preview_cc_switch_sync_plan(request: CcSwitchSyncPlanRequest) -> Value {
    tauri::async_runtime::spawn_blocking(move || match prepare_cc_switch_sync_plan(&request) {
        Ok(plan) => {
            let mut review = preview_connection_route_plan_value(plan);
            if let Some(object) = review.as_object_mut() {
                object.insert(
                    "source".to_string(),
                    json!("dtoken-cc-switch-read-only-sync"),
                );
                object.insert(
                    "source_fingerprint".to_string(),
                    json!(request.source_fingerprint),
                );
            }
            review
        }
        Err(error) => json!({
            "ok": false,
            "state": "blocked",
            "source": "dtoken-cc-switch-read-only-sync",
            "summary": sanitize_command_output(&error),
            "requires_explicit_confirmation": true,
        }),
    })
    .await
    .unwrap_or_else(|error| {
        json!({
            "ok": false,
            "state": "failed",
            "source": "dtoken-cc-switch-read-only-sync",
            "summary": format!("cc-switch sync preview worker failed: {error}"),
        })
    })
}

#[tauri::command]
async fn apply_cc_switch_sync_plan(request: ApplyCcSwitchSyncPlanRequest) -> Value {
    tauri::async_runtime::spawn_blocking(move || match prepare_cc_switch_sync_plan(&request.plan) {
        Ok(plan) => {
            let mut result = apply_connection_route_plan_value(ApplyConnectionRoutePlanRequest {
                plan,
                reviewed_before_sha256: request.reviewed_before_sha256,
                reviewed_after_sha256: request.reviewed_after_sha256,
                enable_auto_follow: false,
            });
            if request.enable_auto_follow && result.get("ok").and_then(Value::as_bool) == Some(true)
            {
                let reload = reload_managed_runtime_after_route_sync();
                if let Some(object) = result.as_object_mut() {
                    object.insert("runtime".to_string(), reload.clone());
                }
                match authorize_initial_cc_switch_auto_follow_after_runtime_reload(
                    &request.plan,
                    &reload,
                ) {
                    Ok(authorization) => {
                        if let Some(object) = result.as_object_mut() {
                            object.insert("auto_follow".to_string(), json!(authorization));
                        }
                    }
                    Err(receipt) => {
                        if let Some(object) = result.as_object_mut() {
                            object.insert("auto_follow".to_string(), receipt);
                        }
                    }
                }
            }
            let event_state = if result.get("ok").and_then(Value::as_bool) == Some(true) {
                "ok"
            } else {
                "failed"
            };
            let _ = append_desktop_event(
                if event_state == "ok" {
                    "route_sync_applied"
                } else {
                    "route_sync_failed"
                },
                event_state,
                json!({
                    "source_fingerprint": request.plan.source_fingerprint,
                    "state": result.get("state"),
                    "after_sha256": result.get("after_sha256"),
                    "rollback_id": result.get("rollback_id"),
                }),
            );
            result
        }
        Err(error) => json!({
            "ok": false,
            "state": "stale",
            "source": "dtoken-cc-switch-read-only-sync",
            "summary": sanitize_command_output(&error),
        }),
    })
    .await
    .unwrap_or_else(|error| {
        json!({
            "ok": false,
            "state": "failed",
            "source": "dtoken-cc-switch-read-only-sync",
            "summary": format!("cc-switch sync apply worker failed: {error}"),
        })
    })
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
    let discovery = enrich_upstream_discovery(upstream_discovery::discover());
    let (revision, prepared, agents) =
        match prepare_connection_route_plan_from_discovery(&request.plan, &discovery) {
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
    let staged_auto_follow = if request.enable_auto_follow {
        match stage_agent_auto_follow_authorization(
            &request.plan,
            &discovery,
            &prepared.plan.after_hash,
        ) {
            Ok(authorization) => authorization,
            Err(error) => {
                return json!({
                    "ok": false,
                    "state": "blocked",
                    "summary": sanitize_command_output(&error),
                });
            }
        }
    } else {
        None
    };
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
    if let Some(authorization) = staged_auto_follow.as_ref() {
        if let Err(error) = save_agent_auto_follow_authorization(authorization) {
            let rollback = config_patcher().restore_backup(
                &receipt.backup,
                &ExpectedFileState::Hash(after_sha256.clone()),
            );
            return json!({
                "ok": false,
                "state": if rollback.is_ok() { "authorization_failed_rolled_back" } else { "authorization_failed_rollback_failed" },
                "summary": sanitize_command_output(&error),
            });
        }
    }
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
        "auto_follow": staged_auto_follow.as_ref().map(|authorization| json!({
            "enabled": authorization.enabled,
            "state": authorization.last_sync_state,
            "agent_ids": authorization.last_synced_bindings.iter().map(|binding| binding.agent_id.clone()).collect::<Vec<_>>(),
            "requires_runtime_verification": true,
        })),
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
        "restore" => {
            logs.push(log(
                "warn",
                "Legacy rollback manifests cannot prove the reviewed revision and current target hash",
            ));
            logs.push(log(
                "info",
                "Use preview_rollback followed by apply_rollback for a hash-bound restore",
            ));
            ActionResult {
                ok: false,
                summary:
                    "Legacy rollback restore is blocked; preview and apply a verified rollback point"
                        .to_string(),
                logs,
                tools: all_checks(),
            }
        }
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

/// Produce a no-write, hash-bound review for a d-token-owned Agent transport
/// change. The review has no credential or user-provider payload.
#[tauri::command]
fn preview_managed_agent_transport(
    request: ManagedAgentTransportRequest,
    operation: String,
) -> Value {
    let request = match resolve_managed_agent_path(request) {
        Ok(request) => request,
        Err(error) => return json!({ "ok": false, "state": "failed", "summary": error }),
    };
    let result = match operation.as_str() {
        "configure" => preview_configure_agent_transport(&request),
        "clear" => preview_clear_agent_transport(&request),
        _ => {
            return json!({ "ok": false, "state": "failed", "summary": "Unsupported Agent transport operation" })
        }
    };
    match result {
        Ok(review) => json!({ "ok": true, "review": review }),
        Err(error) => json!({
            "ok": false,
            "state": "failed",
            "summary": error.to_string(),
            "privacy": "Only d-token-owned transport metadata was considered; credentials were not read or written"
        }),
    }
}

/// Apply exactly the no-write review returned by
/// `preview_managed_agent_transport`. Any external configuration edit makes
/// the review stale and the operation fails before writing.
#[tauri::command]
fn apply_reviewed_managed_agent_transport(
    request: ManagedAgentTransportRequest,
    operation: String,
    reviewed_revision: String,
    reviewed_before_sha256: String,
    reviewed_after_sha256: String,
) -> Value {
    let request = match resolve_managed_agent_path(request) {
        Ok(request) => request,
        Err(error) => return json!({ "ok": false, "state": "failed", "summary": error }),
    };
    let operation = match operation.as_str() {
        "configure" => AgentTransportOperation::Configure,
        "clear" => AgentTransportOperation::Clear,
        _ => {
            return json!({ "ok": false, "state": "failed", "summary": "Unsupported Agent transport operation" })
        }
    };
    match apply_reviewed_agent_transport(
        &request,
        operation,
        &config_backup_root(),
        &reviewed_revision,
        &reviewed_before_sha256,
        &reviewed_after_sha256,
    ) {
        Ok(receipt) => json!(receipt),
        Err(error) => json!({
            "ok": false,
            "state": "failed",
            "summary": error.to_string(),
            "privacy": "Only d-token-owned transport metadata was considered; credentials were not read or written"
        }),
    }
}

#[tauri::command]
fn verify_managed_agent_transport(request: ManagedAgentTransportRequest) -> Value {
    let request = match resolve_managed_agent_path(request) {
        Ok(request) => request,
        Err(error) => return json!({ "ok": false, "detail": error }),
    };
    match verify_agent_transport(&request) {
        Ok(verification) => json!(verification),
        Err(error) => json!({ "ok": false, "detail": error.to_string() }),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ManagedAgentMcpRequest {
    agent_id: String,
    /// The absolute path to the MCP config file (resolved from the discovery).
    config_path: PathBuf,
    /// "configure" or "clear"
    operation: String,
}

fn resolve_managed_agent_mcp_request(
    request: ManagedAgentMcpRequest,
) -> Result<ManagedAgentMcpRequest, String> {
    let context = current_process_scan_context(epoch_ms())
        .map_err(|error| format!("Agent configuration roots are unavailable: {error}"))?;
    resolve_managed_agent_mcp_request_with_context(request, &context)
}

#[cfg(test)]
fn resolve_managed_agent_mcp_request_with_home(
    request: ManagedAgentMcpRequest,
    home_dir: &Path,
) -> Result<ManagedAgentMcpRequest, String> {
    let mut context = current_process_scan_context(epoch_ms())
        .map_err(|error| format!("Agent configuration roots are unavailable: {error}"))?;
    context.home_dir = home_dir.to_path_buf();
    resolve_managed_agent_mcp_request_with_context(request, &context)
}

/// Resolve a managed MCP request against the full platform scan context so
/// Windows agents (VS Code, Claude Desktop) resolve their `%APPDATA%` roaming
/// targets instead of the macOS home-relative layout, and Hook agents resolve
/// their verified hooks.json target.
fn resolve_managed_agent_mcp_request_with_context(
    mut request: ManagedAgentMcpRequest,
    context: &AgentScanContext,
) -> Result<ManagedAgentMcpRequest, String> {
    if !matches!(request.operation.as_str(), "configure" | "clear") {
        return Err("Unsupported managed MCP operation".to_string());
    }
    let raw = request.config_path.to_string_lossy();
    if let Some(relative) = raw.strip_prefix("~/") {
        request.config_path = context.home_dir.join(relative);
    } else if !request.config_path.is_absolute() {
        return Err("MCP config path is not absolute or home-relative".to_string());
    }
    request.config_path = normalize_managed_agent_target(&request.config_path)?;
    let expected = if is_verified_hook_agent(&request.agent_id) {
        resolve_hook_config_path(&request.agent_id, &context.home_dir)
    } else {
        resolve_mcp_config_path_for(
            &request.agent_id,
            context.platform,
            &context.home_dir,
            context.roaming_app_data.as_deref(),
        )
    }
    .ok_or_else(|| "Agent has no verified MCP configuration target".to_string())?;
    let expected = normalize_managed_agent_target(&expected)?;
    if request.config_path != expected {
        return Err("MCP config path is outside the registered Agent target".to_string());
    }
    reject_symlink_components(&request.config_path)?;
    if request.config_path.exists() {
        let metadata = fs::symlink_metadata(&request.config_path)
            .map_err(|error| format!("MCP config metadata is unavailable: {error}"))?;
        if !metadata.is_file() {
            return Err("MCP config target must be a regular file".to_string());
        }
        let resolved = fs::canonicalize(&request.config_path)
            .map_err(|error| format!("MCP config path cannot be canonicalized: {error}"))?;
        let resolved_expected = fs::canonicalize(&expected)
            .map_err(|error| format!("Registered MCP config cannot be canonicalized: {error}"))?;
        if resolved != resolved_expected {
            return Err("MCP config does not resolve to the registered target".to_string());
        }
    } else {
        // Clear is idempotent. Once the requested path exactly matches the
        // registered target and all existing path components are non-symlink,
        // an absent file (including an absent Agent config directory) already
        // satisfies the operation and must not require directory creation.
        if request.operation == "clear" {
            return Ok(request);
        }
        let parent = request
            .config_path
            .parent()
            .ok_or_else(|| "MCP config target has no parent directory".to_string())?;
        let expected_parent = expected
            .parent()
            .ok_or_else(|| "Registered MCP config target has no parent directory".to_string())?;
        let resolved_parent = fs::canonicalize(parent)
            .map_err(|_| "MCP config parent directory must already exist".to_string())?;
        let resolved_expected_parent = fs::canonicalize(expected_parent)
            .map_err(|_| "Registered MCP config parent directory must already exist".to_string())?;
        if resolved_parent != resolved_expected_parent || !resolved_parent.is_dir() {
            return Err("MCP config parent is outside the registered Agent target".to_string());
        }
    }
    Ok(request)
}

fn redacted_mcp_diff(request: &ManagedAgentMcpRequest, changed: bool) -> String {
    if !changed {
        return String::new();
    }
    format!(
        "{} the d-token-owned MCP entry in {}. Existing MCP providers and document contents are redacted.",
        if request.operation == "configure" {
            "Add"
        } else {
            "Remove"
        },
        request.config_path.display()
    )
}

/// Preview a d-token MCP entry add/remove for an MCP-capable agent.
/// Uses the resolved absolute dtoken-agent-bridge path to build the
/// exact MCP entry. No write occurs; returns a hash-bound diff.
#[tauri::command]
fn preview_managed_agent_mcp_integration(request: ManagedAgentMcpRequest) -> Value {
    let request = match resolve_managed_agent_mcp_request(request) {
        Ok(request) => request,
        Err(error) => return json!({ "ok": false, "state": "failed", "summary": error }),
    };
    let bridge = match resolve_dtoken_agent_bridge_exe() {
        Ok(path) => path,
        Err(error) => {
            return json!({
                "ok": false,
                "state": "failed",
                "summary": format!("dtoken-agent-bridge is unavailable: {error}")
            })
        }
    };
    let entry = json!({
        "command": bridge.to_string_lossy(),
        "args": ["mcp", "stdio"]
    });
    let config_text = match fs::read_to_string(&request.config_path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => {
            return json!({
                "ok": false,
                "state": "failed",
                "summary": format!("Failed to read MCP config: {error}")
            })
        }
    };
    let add = request.operation == "configure";
    let hook_agent = is_verified_hook_agent(&request.agent_id);
    let result = if hook_agent {
        transform_hook_json_document(&request.agent_id, &config_text, &bridge, add)
    } else {
        transform_mcp_json_document(&request.agent_id, &config_text, &entry, add)
    };
    match result {
        Ok(next_text) => {
            use sha2::{Digest, Sha256};
            let before_sha256 = format!("{:x}", Sha256::digest(config_text.as_bytes()));
            let after_sha256 = format!("{:x}", Sha256::digest(next_text.as_bytes()));
            let changed = before_sha256 != after_sha256;
            let diff = redacted_mcp_diff(&request, changed);
            json!({
                "ok": true,
                "state": "review",
                "review": {
                    "agent_id": request.agent_id,
                    "target_path": request.config_path.to_string_lossy(),
                    "revision": format!("mcp-review-{}", &before_sha256[..12]),
                    "before_sha256": before_sha256,
                    "after_sha256": after_sha256,
                    "changed": changed,
                    "redacted_diff": diff,
                    "bridge_path": bridge.to_string_lossy(),
                    "provides_route_evidence": false,
                    "integration_kind": if hook_agent { "hook" } else { "mcp" },
                }
            })
        }
        Err(error) => json!({
            "ok": false,
            "state": "failed",
            "summary": error.to_string()
        }),
    }
}

/// Apply a reviewed MCP integration change. The caller must pass the
/// exact before/after SHA256 from the preview. Fails on mismatch.
#[tauri::command]
fn apply_managed_agent_mcp_integration(
    request: ManagedAgentMcpRequest,
    reviewed_before_sha256: String,
    reviewed_after_sha256: String,
) -> Value {
    let request = match resolve_managed_agent_mcp_request(request) {
        Ok(request) => request,
        Err(error) => return json!({ "ok": false, "state": "failed", "summary": error }),
    };
    let bridge = match resolve_dtoken_agent_bridge_exe() {
        Ok(path) => path,
        Err(error) => {
            return json!({
                "ok": false,
                "state": "failed",
                "summary": format!("dtoken-agent-bridge is unavailable: {error}")
            })
        }
    };
    let entry = json!({
        "command": bridge.to_string_lossy(),
        "args": ["mcp", "stdio"]
    });
    let config_text = match fs::read_to_string(&request.config_path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => {
            return json!({
                "ok": false,
                "state": "failed",
                "summary": format!("Failed to read MCP config: {error}")
            })
        }
    };
    use sha2::{Digest, Sha256};
    let before_sha256 = format!("{:x}", Sha256::digest(config_text.as_bytes()));
    if before_sha256 != reviewed_before_sha256 {
        return json!({
            "ok": false,
            "state": "blocked",
            "summary": "MCP config changed after review; generate a new preview"
        });
    }
    let add = request.operation == "configure";
    let hook_agent = is_verified_hook_agent(&request.agent_id);
    let next_text = if hook_agent {
        transform_hook_json_document(&request.agent_id, &config_text, &bridge, add)
    } else {
        transform_mcp_json_document(&request.agent_id, &config_text, &entry, add)
    };
    let next_text = match next_text {
        Ok(text) => text,
        Err(error) => {
            return json!({
                "ok": false,
                "state": "failed",
                "summary": error.to_string()
            })
        }
    };
    let after_sha256 = format!("{:x}", Sha256::digest(next_text.as_bytes()));
    if after_sha256 != reviewed_after_sha256 {
        return json!({
            "ok": false,
            "state": "blocked",
            "summary": "MCP config after_sha256 does not match the reviewed value"
        });
    }
    if config_text == next_text {
        let verification =
            verify_managed_agent_mcp_integration(request.agent_id, request.config_path, add);
        return json!({
            "ok": verification.get("ok").and_then(Value::as_bool).unwrap_or(false),
            "state": "verified",
            "changed": false,
            "before_sha256": before_sha256,
            "after_sha256": after_sha256,
            "verification": verification,
            "provides_route_evidence": false,
        });
    }
    let patcher = config_patcher();
    let prepared = match patcher.prepare_change(
        &request.config_path,
        &next_text,
        ConfigToolKind::Agent,
        Some(request.agent_id.clone()),
        RiskLevel::Medium,
    ) {
        Ok(prepared) => prepared,
        Err(error) => {
            return json!({
                "ok": false,
                "state": "failed",
                "summary": format!("Failed to prepare MCP config transaction: {error}"),
            })
        }
    };
    let transaction = match patcher.apply_transaction(&[prepared]) {
        Ok(transaction) => transaction,
        Err(error) => {
            return json!({
                "ok": false,
                "state": "failed",
                "summary": format!("Failed to apply MCP config transaction: {error}"),
            })
        }
    };
    let Some(change) = transaction.changes.first() else {
        return json!({
            "ok": false,
            "state": "failed",
            "summary": "MCP config transaction returned no change receipt",
        });
    };
    let transaction_id = transaction.transaction_id.clone();
    let rollback_id = change.backup.rollback_id.clone();
    let verification =
        verify_managed_agent_mcp_integration(request.agent_id, request.config_path, add);
    if verification.get("ok").and_then(Value::as_bool) != Some(true) {
        let rollback = patcher.restore_backup(
            &change.backup,
            &ExpectedFileState::Hash(after_sha256.clone()),
        );
        return json!({
            "ok": false,
            "state": if rollback.is_ok() { "rolled_back" } else { "rollback_failed" },
            "summary": "MCP config verification failed after apply",
            "rollback_id": rollback_id,
            "restored": rollback.is_ok(),
            "verification": verification,
            "provides_route_evidence": false,
        });
    }
    json!({
        "ok": true,
        "state": "verified",
        "changed": true,
        "transaction_id": transaction_id,
        "rollback_id": rollback_id,
        "before_sha256": before_sha256,
        "after_sha256": after_sha256,
        "verification": verification,
        "provides_route_evidence": false,
    })
}

/// Verify a managed MCP integration by checking the config file.
#[tauri::command]
fn verify_managed_agent_mcp_integration(
    agent_id: String,
    config_path: PathBuf,
    configured: bool,
) -> Value {
    let request = match resolve_managed_agent_mcp_request(ManagedAgentMcpRequest {
        agent_id: agent_id.clone(),
        config_path: config_path.clone(),
        operation: if configured {
            "configure".to_string()
        } else {
            "clear".to_string()
        },
    }) {
        Ok(request) => request,
        Err(error) => return json!({ "ok": false, "detail": error }),
    };
    let bridge = match resolve_dtoken_agent_bridge_exe() {
        Ok(path) => path,
        Err(error) => {
            return json!({
                "ok": false,
                "detail": format!("dtoken-agent-bridge is unavailable: {error}")
            })
        }
    };
    let entry = json!({
        "command": bridge.to_string_lossy(),
        "args": ["mcp", "stdio"]
    });
    let config_text = match fs::read_to_string(&request.config_path) {
        Ok(text) => text,
        Err(error) if !configured && error.kind() == std::io::ErrorKind::NotFound => {
            return json!({
                "ok": true,
                "detail": "d-token MCP entry is absent because the MCP config file does not exist"
            })
        }
        Err(error) => {
            return json!({
                "ok": false,
                "detail": format!("Failed to read MCP config: {error}")
            })
        }
    };
    let result = if configured {
        if is_verified_hook_agent(&request.agent_id) {
            verify_hook_json_document(&request.agent_id, &config_text, &bridge)
        } else {
            verify_mcp_json_document(&request.agent_id, &config_text, &entry)
        }
    } else if is_verified_hook_agent(&request.agent_id) {
        verify_hook_json_cleared(&request.agent_id, &config_text, &bridge)
    } else {
        verify_mcp_json_cleared(&request.agent_id, &config_text, &entry)
    };
    match result {
        Ok(()) => json!({
            "ok": true,
            "detail": if configured { "d-token MCP entry is present and valid" } else { "d-token MCP entry has been removed" }
        }),
        Err(error) => json!({
            "ok": false,
            "detail": error.to_string()
        }),
    }
}

fn resolve_managed_agent_path(
    request: ManagedAgentTransportRequest,
) -> Result<ManagedAgentTransportRequest, String> {
    let context = current_process_scan_context(epoch_ms())
        .map_err(|error| format!("Agent configuration roots are unavailable: {error}"))?;
    resolve_managed_agent_path_with_context(request, &context)
}

fn resolve_managed_agent_path_with_context(
    mut request: ManagedAgentTransportRequest,
    context: &AgentScanContext,
) -> Result<ManagedAgentTransportRequest, String> {
    let raw = request.target_path.to_string_lossy();
    if let Some(relative) = raw.strip_prefix("~/") {
        request.target_path = context.home_dir.join(relative);
    } else if !request.target_path.is_absolute() {
        return Err("Agent config path is not an absolute or home-relative path".to_string());
    }
    request.target_path = normalize_managed_agent_target(&request.target_path)?;
    let allowed = allowed_agent_config_paths(&request.agent_id, context)
        .map_err(|_| "Agent config path is not registered for this Agent".to_string())?
        .into_iter()
        .map(|path| normalize_managed_agent_target(&path))
        .collect::<Result<Vec<_>, _>>()?;
    let Some(expected_path) = allowed
        .iter()
        .find(|candidate| *candidate == &request.target_path)
    else {
        return Err("Agent config path is outside the registered Agent targets".to_string());
    };
    reject_symlink_components(&request.target_path)?;
    if request.target_path.exists() {
        let metadata = fs::symlink_metadata(&request.target_path)
            .map_err(|error| format!("Agent config metadata is unavailable: {error}"))?;
        if !metadata.is_file() {
            return Err("Agent config target must be a regular file".to_string());
        }
        let resolved = fs::canonicalize(&request.target_path)
            .map_err(|error| format!("Agent config path cannot be canonicalized: {error}"))?;
        let expected = fs::canonicalize(expected_path).map_err(|error| {
            format!("Registered Agent config path cannot be canonicalized: {error}")
        })?;
        if resolved != expected {
            return Err("Agent config path does not resolve to the registered target".to_string());
        }
    } else {
        let parent = request
            .target_path
            .parent()
            .ok_or_else(|| "Agent config target has no parent directory".to_string())?;
        let expected_parent = expected_path
            .parent()
            .ok_or_else(|| "Registered Agent config target has no parent directory".to_string())?;
        let resolved_parent = fs::canonicalize(parent)
            .map_err(|_| "Agent config parent directory must already exist".to_string())?;
        let resolved_expected_parent = fs::canonicalize(expected_parent).map_err(|_| {
            "Registered Agent config parent directory must already exist".to_string()
        })?;
        if resolved_parent != resolved_expected_parent || !resolved_parent.is_dir() {
            return Err("Agent config parent is outside the registered Agent target".to_string());
        }
    }
    Ok(request)
}

fn normalize_managed_agent_target(path: &Path) -> Result<PathBuf, String> {
    use std::path::Component;

    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(_) | Component::RootDir | Component::Normal(_) => {
                normalized.push(component.as_os_str())
            }
            Component::CurDir => {}
            Component::ParentDir => {
                return Err("Agent config path cannot contain parent traversal".to_string())
            }
        }
    }
    if !normalized.is_absolute() {
        return Err("Agent config path must resolve to an absolute path".to_string());
    }
    Ok(normalized)
}

fn reject_symlink_components(path: &Path) -> Result<(), String> {
    let mut current = PathBuf::new();
    for component in path.components() {
        current.push(component.as_os_str());
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err("Agent config path cannot contain symbolic links".to_string())
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
            Err(error) => {
                return Err(format!(
                    "Agent config path metadata is unavailable: {error}"
                ))
            }
        }
    }
    Ok(())
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
    let active_providers = active
        .get("providers")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|provider| {
            json!({
                "endpoint": provider.get("base_url").and_then(Value::as_str).map(normalized_endpoint),
                "protocol": provider.get("protocol").and_then(Value::as_str),
                "models": provider.get("models").cloned().unwrap_or_else(|| json!([])),
            })
        })
        .collect::<Vec<_>>();
    if let Some(sync) = discovery
        .get_mut("cc_switch_sync")
        .and_then(Value::as_object_mut)
    {
        let detected = sync
            .get("detected")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let status = derive_cc_switch_status(detected, sync, &active_providers, active_state);
        sync.insert("status".to_string(), Value::String(status.to_string()));

        // Inject the route_snapshot_identity: the active upstream identity
        // that d-token is currently synced to, derived from the auto-follow
        // authorization's last-synced route signature. Populated regardless
        // of current detection state so the last-known-good identity is
        // visible when cc-switch becomes unavailable.
        let authorization = load_cc_switch_auto_follow_authorization();
        if let Some(route_sig) = authorization.last_synced_route_signature.as_deref() {
            sync.insert(
                "route_snapshot_identity".to_string(),
                json!({
                    "route_signature": route_sig,
                    "source_fingerprint": authorization.last_synced_source_fingerprint,
                }),
            );
        } else if let Some(route_sig) = authorization.last_observed_route_signature.as_deref() {
            // Fallback to the last observed (but not yet synced) identity
            // when no sync has completed yet.
            sync.insert(
                "route_snapshot_identity".to_string(),
                json!({
                    "route_signature": route_sig,
                    "source_fingerprint": authorization.last_observed_source_fingerprint,
                }),
            );
        }
        // Inject the last receipt from persistent storage, available
        // regardless of current detection so the UI always sees the last
        // sync outcome even when cc-switch is offline.
        if let Some(receipt) = load_cc_switch_route_sync_receipt() {
            sync.insert(
                "last_receipt".to_string(),
                json!({
                    "state": receipt.state,
                    "source_fingerprint": receipt.source_fingerprint,
                    "route_before_sha256": receipt.route_before_sha256,
                    "route_after_sha256": receipt.route_after_sha256,
                    "rollback_id": receipt.rollback_id,
                    "runtime_state": receipt.runtime_state,
                    "occurred_at_ms": receipt.occurred_at_ms,
                }),
            );
        }
    }
    let agent_follow_authorization = load_agent_auto_follow_authorization();
    let agent_follow_receipt = load_agent_route_sync_receipt();
    project_agent_follow_status(
        &mut discovery,
        &agent_follow_authorization,
        agent_follow_receipt.as_ref(),
    );
    if let Some(object) = discovery.as_object_mut() {
        let prior_revision = object
            .get("revision")
            .and_then(Value::as_str)
            .unwrap_or("setup-unavailable");
        let route_revision = stable_hash(
            &json!({
                "discovery_revision": prior_revision,
                "active_state": active_state,
                "active_providers": active_providers,
                "cc_switch_status": object.get("cc_switch_sync").and_then(|value| value.get("status")),
                "agent_route_follow": object.get("agent_route_follow"),
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
    record_external_config_refresh_evidence(&discovery);
    discovery
}

fn project_agent_follow_status(
    discovery: &mut Value,
    authorization: &AgentAutoFollowAuthorization,
    receipt: Option<&AgentRouteSyncReceipt>,
) {
    let Some(object) = discovery.as_object_mut() else {
        return;
    };
    let bindings = object
        .get_mut("agent_bindings")
        .and_then(Value::as_array_mut);
    if let Some(bindings) = bindings {
        for binding in bindings {
            let Some(agent_id) = binding.get("agent_id").and_then(Value::as_str) else {
                continue;
            };
            let Some(stored) = authorization
                .last_synced_bindings
                .iter()
                .find(|stored| stored.agent_id == agent_id)
            else {
                if let Some(binding) = binding.as_object_mut() {
                    binding.insert(
                        "follow".to_string(),
                        json!({
                            "authorized": false,
                            "enabled": false,
                            "state": "not_authorized",
                            "transport_out_of_sync": false,
                            "next_action": "enable_follow_in_connection",
                            "current_upstream": null,
                            "last_receipt": null,
                        }),
                    );
                }
                continue;
            };
            let already_connected = binding
                .get("already_connected")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let related_receipt = receipt.filter(|receipt| {
                receipt
                    .changed_agents
                    .iter()
                    .any(|changed| changed == agent_id)
            });
            let receipt_failure = related_receipt
                .map(|receipt| receipt.state.as_str())
                .filter(|state| *state != "applied_verified");
            let transport_out_of_sync = authorization.enabled && !already_connected;
            let state = if let Some(state) = receipt_failure {
                state
            } else if transport_out_of_sync {
                "transport_out_of_sync"
            } else if authorization.enabled {
                "following"
            } else if authorization.last_sync_state != "not_authorized" {
                authorization.last_sync_state.as_str()
            } else {
                "not_authorized"
            };
            let can_configure = binding
                .get("can_configure")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let next_action = match state {
                "following" | "applied_verified" => "none",
                "transport_out_of_sync" if can_configure => "configure",
                "transport_out_of_sync" => "advanced_configuration",
                "pending_runtime_verification"
                | "runtime_verification_required"
                | "runtime_identity_changed"
                | "runtime_verify_failed_rolled_back"
                | "runtime_verify_failed_rollback_failed" => "verify_runtime",
                "reauthorization_required"
                | "active_route_changed_reauthorization_required"
                | "stale_route" => "review_follow",
                "not_authorized" => "enable_follow_in_connection",
                _ => "retry_review",
            };
            if let Some(binding) = binding.as_object_mut() {
                binding.insert(
                    "follow".to_string(),
                    json!({
                        "authorized": true,
                        "enabled": authorization.enabled,
                        "state": state,
                        "transport_out_of_sync": transport_out_of_sync,
                        "next_action": next_action,
                        "current_upstream": stored.upstream,
                        "last_receipt": related_receipt,
                    }),
                );
            }
        }
    }
    object.insert(
        "agent_route_follow".to_string(),
        json!({
            "schema_version": authorization.schema_version,
            "enabled": authorization.enabled,
            "state": authorization.last_sync_state,
            "authorized_at_ms": authorization.authorized_at_ms,
            "runtime_identity": authorization.runtime_identity,
            "last_synced_source_fingerprint": authorization.last_synced_source_fingerprint,
            "last_receipt": receipt,
        }),
    );
}

/// Persist only the state transition and metadata fingerprint of an external
/// Agent/cc-switch configuration observation. The event intentionally never
/// includes external config content, credentials, or a mutation instruction.
fn record_external_config_refresh_evidence(discovery: &Value) {
    let Some(refresh) = discovery.get("external_config_refresh") else {
        return;
    };
    let state = refresh.get("state").and_then(Value::as_str);
    let event_payload = json!({
        "snapshot_fingerprint": refresh.get("snapshot_fingerprint"),
        "previous_fingerprint": refresh.get("previous_fingerprint"),
        "changed_sources": refresh.get("changed_sources"),
        "automatic_apply_allowed": false,
    });
    match state {
        Some("external_config_snapshot") => {
            let _ = append_desktop_event("external_config_detected", "ok", event_payload);
        }
        Some("external_config_review_required") => {
            let _ = append_desktop_event("external_config_changed", "ok", event_payload.clone());
            let _ = append_desktop_event("route_sync_needed", "review_required", event_payload);
        }
        _ => {}
    }
}

fn normalized_endpoint(value: &str) -> String {
    value.trim().trim_end_matches('/').to_ascii_lowercase()
}

fn derive_cc_switch_status(
    detected: bool,
    sync: &serde_json::Map<String, Value>,
    active_providers: &[Value],
    active_state: &str,
) -> &'static str {
    if !detected {
        // Check if the source state indicates an error condition
        // (locked/corrupt/unsupported schema) rather than simply missing.
        if sync
            .get("error")
            .and_then(Value::as_str)
            .is_some_and(|error| {
                error.starts_with("locked")
                    || error.starts_with("unsupported")
                    || error == "invalid_schema"
                    || error == "metadata_unavailable"
            })
        {
            return "error";
        }
        "unavailable"
    } else if sync
        .get("active_upstreams")
        .and_then(Value::as_array)
        .is_some_and(|upstreams| upstreams.len() > 1)
    {
        "needs-selection"
    } else if sync
        .get("active_upstreams")
        .and_then(Value::as_array)
        .and_then(|upstreams| upstreams.first())
        .is_some_and(|upstream| {
            let endpoint = upstream
                .get("upstream")
                .and_then(Value::as_str)
                .map(normalized_endpoint);
            let protocol = upstream.get("protocol").and_then(Value::as_str);
            let model = upstream
                .get("models")
                .and_then(Value::as_array)
                .and_then(|models| models.first())
                .and_then(Value::as_str);
            endpoint.is_some_and(|endpoint| {
                active_providers.iter().any(|provider| {
                    provider.get("endpoint").and_then(Value::as_str) == Some(endpoint.as_str())
                        && provider.get("protocol").and_then(Value::as_str) == protocol
                        && model.is_some_and(|model| {
                            provider
                                .get("models")
                                .and_then(Value::as_array)
                                .is_some_and(|models| {
                                    models.iter().any(|value| value.as_str() == Some(model))
                                })
                        })
                })
            })
        })
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
    let current = dtoken_service_status_value_fresh();
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

/// Reloads only a d-token-managed runtime after an already-reviewed route
/// sync. Unmanaged processes are never stopped; callers receive an explicit
/// restart-required receipt instead of a silent partial success.
#[tauri::command]
fn reload_managed_runtime_after_route_sync() -> Value {
    let status = dtoken_service_status_value_fresh();
    let state = status
        .get("state")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    match state {
        "running" | "restart_required" => {
            let result = restart_dtoken_service_impl();
            let event = if result.ok {
                "runtime_reloaded"
            } else {
                "route_sync_failed"
            };
            let _ = append_desktop_event(
                event,
                if result.ok { "ok" } else { "failed" },
                json!({ "summary": result.summary, "service_state": state }),
            );
            json!({
                "ok": result.ok,
                "state": if result.ok { "runtime_reloaded" } else { "runtime_reload_failed" },
                "managed": true,
                "summary": result.summary,
                "logs": result.logs,
            })
        }
        "unmanaged_running" => {
            let _ = append_desktop_event(
                "runtime_reload_required",
                "restart_required",
                json!({ "service_state": state, "managed": false }),
            );
            json!({
                "ok": false,
                "state": "restart_required",
                "managed": false,
                "summary": "An unmanaged d-token-compatible service owns the port; restart it manually to load the reviewed route",
            })
        }
        _ => json!({
            "ok": true,
            "state": "runtime_not_running",
            "managed": false,
            "summary": "The reviewed route is saved; the runtime will load it on the next managed start",
        }),
    }
}

fn start_dtoken_service_impl() -> ActionResult {
    let mut logs = vec![log(
        "info",
        "Starting d-token-owned local service lifecycle action",
    )];
    let current = dtoken_service_status_value_fresh();
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
    if selection.install_mcp {
        return Err(
            "Doctor cannot create or renew Managed SharedContext MCP capabilities; use Install & Connect so the project, route, provider, model, and capability review are bound in one transaction"
                .to_string(),
        );
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
    .map(|prepared| prepared.agent)
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
    let connection_discovery = enrich_upstream_discovery(upstream_discovery::discover());
    let cc_switch_sync = connection_discovery
        .get("cc_switch_sync")
        .cloned()
        .unwrap_or(Value::Null);
    let cc_switch_state = match cc_switch_sync.get("status").and_then(Value::as_str) {
        Some("synced") => "healthy",
        Some("out-of-sync") => "repairable",
        Some("needs-selection" | "needs-confirmation") => "manual",
        Some("error") => "manual",
        Some("unavailable") => "unknown",
        _ => "unknown",
    };
    let cc_switch_summary = match cc_switch_sync.get("status").and_then(Value::as_str) {
        Some("synced") => {
            "cc-switch 当前上游与 d-token route 的 endpoint、协议和模型一致".to_string()
        }
        Some("out-of-sync") => {
            "cc-switch 当前上游已变化；可预览并同步 d-token 自有 route".to_string()
        }
        Some("error") => {
            "cc-switch 数据库无法安全读取（锁定/损坏/schema 不兼容）；d-token 保持只读并停止同步"
                .to_string()
        }
        Some("needs-selection") => {
            "cc-switch 有多个活动上游，需要先选择唯一目标才能生成同步计划".to_string()
        }
        Some("needs-confirmation") => {
            "cc-switch 当前上游需要确认后才会写入 d-token route".to_string()
        }
        _ => "未检测到可安全验证的 cc-switch 同步证据".to_string(),
    };
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
        json!({ "module_id": "agent_cc_switch_sync", "state": cc_switch_state, "severity": if cc_switch_state == "repairable" { "warning" } else { "info" }, "summary": cc_switch_summary, "sync": cc_switch_sync }),
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
            module_id: DoctorModuleId::AgentCcSwitchSync,
            state: doctor_domain_state(cc_switch_state),
            observed_at_ms: collected_at_ms,
            evidence: vec![DoctorEvidence {
                kind: DoctorEvidenceKind::Configuration,
                state: doctor_domain_state(cc_switch_state),
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
            title: "未检测到当前可用的 Agent".to_string(),
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
        "agent_cc_switch_sync" => Some(DoctorModuleId::AgentCcSwitchSync),
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
    // AgentCcSwitchSync acts on the whole cc-switch-derived route, not on a
    // per-Agent selection. The Doctor selection contract requires both an
    // agent and a route id, so a fixed documented identity is used; the UI
    // round-trips the synthesized selection returned by preview.
    let cc_switch_module = matches!(module_id, DoctorModuleId::AgentCcSwitchSync);
    Ok(DoctorRepairSelection {
        scope,
        agent_ids: if cc_switch_module {
            vec!["cc-switch".to_string()]
        } else if agent_module {
            selection.agent_ids.clone()
        } else {
            Vec::new()
        },
        route_ids: if cc_switch_module {
            vec!["cc-switch".to_string()]
        } else {
            Vec::new()
        },
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
    if matches!(domain_module_id, DoctorModuleId::AgentCcSwitchSync) {
        // For AgentCcSwitchSync, generate a cc-switch sync plan preview.
        let discovery = enrich_upstream_discovery(upstream_discovery::discover());
        match cc_switch_sync_request_from_discovery(&discovery) {
            Ok(request) => match prepare_cc_switch_sync_plan(&request) {
                Ok(plan) => {
                    let targets = cc_switch_plan_targets(&plan);
                    let review_digest = stable_hash(
                        &json!({
                            "source_fingerprint": request.source_fingerprint,
                            "targets": targets,
                        })
                        .to_string(),
                    );
                    agent_review_digests.insert("cc-switch".to_string(), review_digest.clone());
                    agent_plans.push(json!({
                            "agent_id": "cc-switch",
                            "state": "review",
                            "summary": "cc-switch route sync plan: d-token route will follow cc-switch active upstream",
                            "review_digest": review_digest,
                            "targets": targets,
                            "source_fingerprint": request.source_fingerprint,
                        }));
                }
                Err(error) => {
                    agent_plans.push(json!({
                        "agent_id": "cc-switch",
                        "state": "blocked",
                        "summary": sanitize_command_output(&error),
                        "targets": [],
                    }));
                }
            },
            Err(error) => {
                agent_plans.push(json!({
                    "agent_id": "cc-switch",
                    "state": "blocked",
                    "summary": sanitize_command_output(&error),
                    "targets": [],
                }));
            }
        }
        if agent_review_digests.is_empty() {
            return json!({
                "ok": false,
                "schema_version": 3,
                "error": "cc-switch sync plan is not available; ensure cc-switch is detected and has one active upstream",
                "agents": agent_plans,
            });
        }
    } else if matches!(
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

    // The selection persisted and echoed back to the UI must be the one the
    // domain plan was built with. AgentCcSwitchSync uses a fixed synthetic
    // identity so the Doctor selection contract (agent + route) is satisfied
    // without implying a per-Agent cc-switch write.
    let stored_selection = if matches!(domain_module_id, DoctorModuleId::AgentCcSwitchSync) {
        let mut synthesized = request.selection.clone();
        synthesized.agent_ids = vec!["cc-switch".to_string()];
        synthesized.route_ids = vec!["cc-switch".to_string()];
        synthesized
    } else {
        request.selection.clone()
    };
    let mut digest_entries = agent_review_digests.iter().collect::<Vec<_>>();
    digest_entries.sort_by(|left, right| left.0.cmp(right.0));
    let review_digest = format!(
        "doctor-review-{}",
        stable_hash(
            &json!({
                "domain": domain_plan.review_digest,
                "module_id": request.module_id,
                "selection": stored_selection,
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
        selection: stored_selection.clone(),
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
        "selection": stored_selection,
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
    } else if stored.module_id == "agent_cc_switch_sync" {
        // AgentCcSwitchSync repair: sync d-token route to match cc-switch
        // without writing to cc-switch itself. The plan is re-derived fresh
        // from the current discovery and must reproduce the exact review
        // digest that was stored at preview time; any cc-switch state change
        // between preview and apply is rejected as stale.
        let discovery = enrich_upstream_discovery(upstream_discovery::discover());
        match cc_switch_sync_request_from_discovery(&discovery) {
            Ok(request) => match prepare_cc_switch_sync_plan(&request) {
                Ok(plan) => {
                    let targets = cc_switch_plan_targets(&plan);
                    let recomputed = stable_hash(
                        &json!({
                            "source_fingerprint": request.source_fingerprint,
                            "targets": targets,
                        })
                        .to_string(),
                    );
                    let reviewed = stored.agent_review_digests.get("cc-switch");
                    if reviewed != Some(&recomputed) {
                        agent_receipts.push(json!({
                            "agent_id": "cc-switch",
                            "outcome": "blocked",
                            "summary": "cc-switch discovery changed after review; scan and review again",
                        }));
                    } else {
                        // Compute the reviewed hashes from the current
                        // discovery so the apply transaction verifies against
                        // exactly what the preview digest bound.
                        let prepared =
                            match prepare_connection_route_plan_from_discovery(&plan, &discovery) {
                                Ok((_, prepared, _)) => Some(prepared),
                                Err(error) => {
                                    agent_receipts.push(json!({
                                        "agent_id": "cc-switch",
                                        "outcome": "blocked",
                                        "summary": sanitize_command_output(&error),
                                    }));
                                    None
                                }
                            };
                        if let Some(prepared) = prepared {
                            let result = apply_connection_route_plan_value(
                                ApplyConnectionRoutePlanRequest {
                                    plan: plan.clone(),
                                    reviewed_before_sha256: Some(prepared.plan.before_hash),
                                    reviewed_after_sha256: Some(prepared.plan.after_hash),
                                    enable_auto_follow: false,
                                },
                            );
                            let ok = result.get("ok").and_then(Value::as_bool).unwrap_or(false);
                            if ok {
                                verified += 1;
                                // Attempt runtime reload; on failure the applied
                                // route stays last-known-good and the rollback
                                // receipt is preserved for a safe revert.
                                let reload = reload_managed_runtime_after_route_sync();
                                let reload_ok =
                                    reload.get("ok").and_then(Value::as_bool).unwrap_or(false);
                                let runtime_state = reload
                                    .get("state")
                                    .and_then(Value::as_str)
                                    .unwrap_or(if reload_ok {
                                        "runtime_reloaded"
                                    } else {
                                        "runtime_reload_failed"
                                    });
                                if !reload_ok {
                                    let _ = append_desktop_event(
                                        "route_sync_failed",
                                        "runtime_reload_failed",
                                        json!({"summary": "Route applied but runtime reload failed; last-known-good preserved"}),
                                    );
                                }
                                // Bind the applied route as the current cc-switch
                                // route snapshot so enrichment exposes it as
                                // route_snapshot_identity.
                                let mut authorization = load_cc_switch_auto_follow_authorization();
                                if let Ok(identity) = cc_switch_active_route_identity(&discovery) {
                                    authorization.last_observed_route_signature =
                                        Some(identity.route_signature.clone());
                                    authorization.last_synced_route_signature =
                                        Some(identity.route_signature);
                                    authorization.last_observed_source_fingerprint =
                                        Some(request.source_fingerprint.clone());
                                    authorization.last_synced_source_fingerprint =
                                        Some(request.source_fingerprint.clone());
                                    authorization.last_sync_state = if reload_ok {
                                        "applied_verified".to_string()
                                    } else {
                                        "applied_reload_failed".to_string()
                                    };
                                    let _ =
                                        save_cc_switch_auto_follow_authorization(&authorization);
                                }
                                // Record a cc-switch auto-follow receipt for the enrichment.
                                let after_sha256 = result
                                    .get("after_sha256")
                                    .and_then(Value::as_str)
                                    .map(str::to_string);
                                let rollback_id = result
                                    .get("rollback_id")
                                    .and_then(Value::as_str)
                                    .map(str::to_string);
                                record_cc_switch_auto_follow_receipt(
                                    if reload_ok {
                                        "applied_verified"
                                    } else {
                                        "applied_reload_failed"
                                    },
                                    &request.source_fingerprint,
                                    result
                                        .get("before_sha256")
                                        .and_then(Value::as_str)
                                        .map(str::to_string),
                                    after_sha256,
                                    rollback_id.clone(),
                                    runtime_state,
                                );
                                agent_receipts.push(json!({
                                    "agent_id": "cc-switch",
                                    "outcome": if reload_ok { "verified" } else { "partial" },
                                    "summary": if reload_ok { "cc-switch route synced and runtime reloaded" } else { "Route applied but runtime reload failed; last-known-good route preserved" },
                                    "transaction_id": result.get("transaction_id"),
                                    "rollback_ids": [rollback_id],
                                }));
                            } else {
                                agent_receipts.push(json!({
                                    "agent_id": "cc-switch",
                                    "outcome": "failed",
                                    "summary": sanitize_command_output(result.get("summary").and_then(Value::as_str).unwrap_or("apply failed")),
                                }));
                            }
                        }
                    }
                }
                Err(error) => {
                    agent_receipts.push(json!({
                        "agent_id": "cc-switch",
                        "outcome": "blocked",
                        "summary": sanitize_command_output(&error),
                    }));
                }
            },
            Err(error) => {
                agent_receipts.push(json!({
                    "agent_id": "cc-switch",
                    "outcome": "blocked",
                    "summary": sanitize_command_output(&error),
                }));
            }
        }
        attempted += 1;
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

// ---------------------------------------------------------------------------
// tauri-specta type-safe IPC scaffolding
// ---------------------------------------------------------------------------
//
// New commands should be annotated with `#[specta::specta]` and registered in
// the `specta_commands!` macro below. The Builder exports TypeScript bindings
// to `src/app/lib/bindings.ts` in debug builds so the frontend gets compile-
// time type checking. Existing `#[tauri::command]` handlers continue to work
// unchanged and can be migrated incrementally.
//
// Migration recipe for an existing command:
//   1. Add `#[specta::specta]` next to `#[tauri::command]`.
//   2. Ensure all argument/return types implement `specta::Type`.
//   3. Register the command in `specta_commands!` below.
//   4. In `src/app/lib/native.ts`, switch to `commands.my_command(...)`.

/// Example type-safe command demonstrating the tauri-specta workflow.
/// Returns a structured snapshot that the frontend can consume with full
/// type checking via `bindings.ts`.
#[tauri::command]
#[specta::specta]
fn get_specta_ipc_status() -> SpectaIpcStatus {
    SpectaIpcStatus {
        available: true,
        contract_revision: dtoken_core_api::COMPRESSION_DECISION_CONTRACT_REVISION.to_string(),
        runtime: std::env::consts::OS.to_string(),
    }
}

/// Type-safe status payload exported to the frontend via tauri-specta.
#[derive(Debug, Clone, Serialize, Type)]
pub struct SpectaIpcStatus {
    pub available: bool,
    pub contract_revision: String,
    pub runtime: String,
}

/// Registers the subset of Tauri commands that carry `#[specta::specta]`.
/// Add new type-safe commands here as they are migrated.
macro_rules! specta_commands {
    () => {
        collect_commands![get_specta_ipc_status,]
    };
}

/// Exports the tauri-specta TypeScript bindings to the frontend in debug
/// builds. This is the only side-effect of the specta scaffolding at runtime:
/// the type-safe commands themselves are registered through the normal
/// `tauri::generate_handler!` macro (see `desktop_release_handlers!`) so they
/// share the same invoke handler as the rest of the IPC surface.
///
/// Failure to export bindings is logged but does not panic, so a missing
/// export path or filesystem error cannot break the desktop app.
#[cfg(debug_assertions)]
fn export_specta_bindings() {
    let builder = tauri_specta::Builder::<tauri::Wry>::new().commands(specta_commands!());
    let export_path =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../src/app/lib/bindings.ts");
    if let Err(error) = builder.export(specta_typescript::Typescript::default(), export_path) {
        eprintln!("warn: failed to export tauri-specta bindings: {error}");
    }
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
            finalize_agent_auto_follow_authorization,
            preview_cc_switch_sync_plan,
            apply_cc_switch_sync_plan,
            set_cc_switch_auto_follow,
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
            verify_managed_agent_transport,
            preview_managed_agent_transport,
            apply_reviewed_managed_agent_transport,
            preview_managed_agent_mcp_integration,
            apply_managed_agent_mcp_integration,
            verify_managed_agent_mcp_integration,
            discover_upstream_candidates,
            get_dtoken_service_status,
            get_autostart_status,
            enable_autostart,
            disable_autostart,
            start_dtoken_service,
            restart_dtoken_service,
            reload_managed_runtime_after_route_sync,
            stop_dtoken_service,
            get_managed_rtk_status,
            preview_managed_rtk_install,
            apply_managed_rtk_install,
            preview_managed_rtk_clear,
            apply_managed_rtk_clear,
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
            show_control_plane,
            get_specta_ipc_status,
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
            .setup(|app| {
                start_service_when_launched_by_autostart();
                start_external_route_follow_observer();
                tray::install(app.handle())?;
                tray::apply_panel_material(app.handle());
                Ok(())
            })
            .on_window_event(|window, event| tray::handle_window_event(window, event))
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
    export_specta_bindings();
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

    /// Temp root for path-sensitive tests. On macOS `/tmp` is a symlink to
    /// `/private/tmp`, so the anti-symlink walk needs the canonical root. On
    /// Windows `fs::canonicalize` returns `\\?\` extended-length paths whose
    /// drive prefix cannot be probed with reparse semantics
    /// (ERROR_INVALID_FUNCTION); `%TEMP%` is already a real absolute root.
    fn test_temp_root() -> PathBuf {
        #[cfg(windows)]
        {
            env::temp_dir()
        }
        #[cfg(not(windows))]
        {
            fs::canonicalize(env::temp_dir()).expect("canonical temporary root")
        }
    }

    /// Locate the staged sidecar in src-tauri/binaries whose SHA-256 exactly
    /// matches the digest build.rs embedded at compile time. build.rs hashes
    /// `binaries/dtoken-agent-bridge-<TARGET>[.exe]`, so this is the only file
    /// `verify_dtoken_agent_bridge` accepts on Linux/Windows; the raw cargo
    /// artifact under target/ is NOT the file the embedded hash covers. Falls
    /// back to any staged bridge when the digest is unavailable (macOS, where
    /// release authority is the codesign identity check, not the digest).
    fn staged_agent_bridge_matching_embedded_hash() -> Option<PathBuf> {
        use sha2::{Digest, Sha256};

        let expected = env!("DTOKEN_AGENT_BRIDGE_SHA256");
        let binaries_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("binaries");
        let mut any_staged = None;
        for entry in fs::read_dir(&binaries_dir).ok()? {
            let path = entry.ok()?.path();
            let name = path.file_name()?.to_string_lossy().into_owned();
            if !name.starts_with("dtoken-agent-bridge-") {
                continue;
            }
            let bytes = fs::read(&path).ok()?;
            if expected != "unavailable" {
                let digest = format!("{:x}", Sha256::digest(&bytes));
                if digest == expected {
                    return Some(path);
                }
            } else if any_staged.is_none() {
                any_staged = Some(path);
            }
        }
        any_staged
    }

    #[test]
    fn managed_mcp_clear_accepts_absent_registered_parent_without_creating_it() {
        let temp_root = test_temp_root();
        let home = tempfile::tempdir_in(temp_root).expect("temporary home");
        let expected = home.path().join(".cursor/mcp.json");
        assert!(!expected.exists());
        assert!(!expected.parent().expect("mcp parent").exists());

        let resolved = resolve_managed_agent_mcp_request_with_home(
            ManagedAgentMcpRequest {
                agent_id: "cursor".to_string(),
                config_path: PathBuf::from("~/.cursor/mcp.json"),
                operation: "clear".to_string(),
            },
            home.path(),
        )
        .expect("absent registered MCP target is already clear");

        assert_eq!(resolved.config_path, expected);
        assert!(!expected.parent().expect("mcp parent").exists());
    }

    #[test]
    fn managed_mcp_clear_resolves_windows_claude_desktop_via_roaming_app_data() {
        use dtoken_config_patcher::agent::{AgentEvidenceOverride, HostPlatform};
        let temp_root = test_temp_root();
        let temp = tempfile::tempdir_in(temp_root).expect("temporary root");
        let home = temp.path().join("home");
        let roaming = temp.path().join("roaming");
        fs::create_dir_all(&roaming).expect("roaming app data");
        let context = AgentScanContext {
            platform: HostPlatform::Windows,
            home_dir: home.clone(),
            path_dirs: Vec::new(),
            applications_dir: None,
            roaming_app_data: Some(roaming.clone()),
            local_app_data: None,
            additional_evidence: Vec::<AgentEvidenceOverride>::new(),
            observed_at_ms: 1,
        };
        let resolved = resolve_managed_agent_mcp_request_with_context(
            ManagedAgentMcpRequest {
                agent_id: "claude-desktop".to_string(),
                config_path: roaming.join("Claude").join("claude_desktop_config.json"),
                operation: "clear".to_string(),
            },
            &context,
        )
        .expect("Windows Claude Desktop clear must resolve to the roaming AppData target");

        assert_eq!(
            resolved.config_path,
            roaming.join("Claude").join("claude_desktop_config.json"),
            "expected the Windows %APPDATA% target, not the macOS home-relative layout"
        );
        assert!(!resolved.config_path.exists());
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

    /// P1 回归防护：get_specta_ipc_status 必须在 release 和 debug handler
    /// 列表中可达，且 specta_commands! 也注册了它。这锁定「specta 命令与
    /// 既有命令共用单一 generate_handler!」的不变量，防止未来有人重新引入
    /// 第二个 invoke_handler 调用。
    #[test]
    fn specta_ipc_status_command_is_reachable_in_all_modes() {
        const COMMAND_NAME: &str = "get_specta_ipc_status";

        assert!(
            RELEASE_HANDLER_NAMES.contains(&COMMAND_NAME),
            "get_specta_ipc_status must be reachable in release builds"
        );
        assert!(
            DEBUG_HANDLER_NAMES.contains(&COMMAND_NAME),
            "get_specta_ipc_status must be reachable in debug builds"
        );
    }

    /// P1 回归防护：一组既有命令必须在 release 和 debug 模式下都可达，
    /// 防止未来误把核心命令从 handler 列表删除或被第二个 invoke_handler 覆盖。
    #[test]
    fn existing_core_commands_remain_reachable() {
        const CORE_COMMANDS: &[&str] = &[
            "get_desktop_summary",
            "get_desktop_data",
            "run_doctor",
            "apply_rollback",
            "get_dtoken_dashboard_payload",
            "save_app_settings",
            "start_dtoken_service",
            "stop_dtoken_service",
        ];

        for command in CORE_COMMANDS {
            assert!(
                RELEASE_HANDLER_NAMES.contains(command),
                "existing command '{command}' must remain reachable in release builds"
            );
            assert!(
                DEBUG_HANDLER_NAMES.contains(command),
                "existing command '{command}' must remain reachable in debug builds"
            );
        }
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

    #[test]
    fn managed_mcp_capability_binding_is_project_and_route_bound() {
        with_isolated_app_state("managed-mcp-capability-binding", || {
            let project = app_state_dir().join("project");
            fs::create_dir_all(&project).expect("create project");
            let config: dtoken_config::DtokenConfig = serde_json::from_value(json!({
                "providers": [{
                    "id": "provider-codex",
                    "name": "Codex upstream",
                    "kind": "custom",
                    "protocol": "open_ai",
                    "base_url": "https://example.invalid/v1",
                    "api_key_ref": null,
                    "models": ["example/model"],
                    "timeout_ms": 120000,
                    "healthcheck_path": "/health"
                }],
                "routes": [{
                    "id": "route-codex",
                    "name": "Codex route",
                    "input_protocol": "open_ai",
                    "instance_id": "instance-codex",
                    "target_provider_id": "provider-codex",
                    "fallback_provider_id": null,
                    "chain_mode": "direct",
                    "compression_policy": "balanced",
                    "bind_host": "127.0.0.1",
                    "local_port": 18001
                }],
                "instances": [{
                    "id": "instance-codex",
                    "name": "Codex instance",
                    "agent": "codex",
                    "route_id": "route-codex",
                    "local_port": 18001,
                    "process_status": "stopped",
                    "log_path": "./logs/route-codex.jsonl"
                }]
            }))
            .expect("parse config");
            let draft = HybridAgentIntegrationDraftRequest {
                agent_ids: vec!["codex".to_string()],
                scope: "project".to_string(),
                project_root: Some(project.display().to_string()),
                install_instructions: false,
                install_hook_or_plugin: false,
                install_skill: false,
                install_mcp: true,
            };

            let bindings =
                resolve_managed_agent_capability_bindings(&draft, &config, &"a".repeat(64))
                    .expect("resolve capability binding");
            assert_eq!(bindings.len(), 1);
            let binding = &bindings[0];
            assert_eq!(binding.agent_id, "codex");
            assert_eq!(binding.scope.agent_id, "codex");
            assert!(binding.scope.project_id.starts_with("project-"));
            assert!(!binding
                .scope
                .project_id
                .contains(project.to_string_lossy().as_ref()));
            assert!(binding
                .scope
                .session_id
                .as_deref()
                .is_some_and(|value| value.starts_with("session-")));
            assert_eq!(binding.route_binding.route_id, "route-codex");
            assert_eq!(binding.route_binding.provider_id, "provider-codex");
            assert_eq!(binding.route_binding.model, "example/model");
            assert_eq!(binding.route_binding.config_revision, "a".repeat(64));
        });
    }

    #[test]
    fn managed_mcp_capability_binding_rejects_global_and_ambiguous_routes() {
        with_isolated_app_state("managed-mcp-capability-ambiguous", || {
            let project = app_state_dir().join("project");
            fs::create_dir_all(&project).expect("create project");
            let mut config: dtoken_config::DtokenConfig = serde_json::from_value(json!({
                "providers": [{
                    "id": "provider-a",
                    "name": "Provider A",
                    "kind": "custom",
                    "protocol": "open_ai",
                    "base_url": "https://example.invalid/v1",
                    "api_key_ref": null,
                    "models": ["example/model"],
                    "timeout_ms": 120000,
                    "healthcheck_path": null
                }],
                "routes": [{
                    "id": "route-a",
                    "name": "Route A",
                    "input_protocol": "open_ai",
                    "instance_id": "instance-a",
                    "target_provider_id": "provider-a",
                    "fallback_provider_id": null,
                    "chain_mode": "direct",
                    "compression_policy": "balanced",
                    "bind_host": "127.0.0.1",
                    "local_port": 18001
                }],
                "instances": [{
                    "id": "instance-a",
                    "name": "Instance A",
                    "agent": "codex",
                    "route_id": "route-a",
                    "local_port": 18001,
                    "process_status": "stopped",
                    "log_path": "./logs/route-a.jsonl"
                }]
            }))
            .expect("parse config");
            let mut draft = HybridAgentIntegrationDraftRequest {
                agent_ids: vec!["codex".to_string()],
                scope: "global".to_string(),
                project_root: None,
                install_instructions: false,
                install_hook_or_plugin: false,
                install_skill: false,
                install_mcp: true,
            };
            assert!(
                resolve_managed_agent_capability_bindings(&draft, &config, &"b".repeat(64),)
                    .unwrap_err()
                    .contains("project scope")
            );

            draft.scope = "project".to_string();
            draft.project_root = Some(project.display().to_string());
            let mut duplicate = config.instances[0].clone();
            duplicate.id = "instance-b".to_string();
            duplicate.route_id = "route-b".to_string();
            config.instances.push(duplicate);
            let mut route = config.routes[0].clone();
            route.id = "route-b".to_string();
            route.instance_id = "instance-b".to_string();
            config.routes.push(route);
            assert!(
                resolve_managed_agent_capability_bindings(&draft, &config, &"b".repeat(64),)
                    .unwrap_err()
                    .contains("multiple active routes")
            );
        });
    }

    #[test]
    fn hybrid_agent_mcp_transaction_applies_verifies_and_revokes_capability() {
        with_isolated_app_state("hybrid-agent-mcp-transaction", || {
            let home = app_state_dir().join("home");
            let project = app_state_dir().join("project");
            let bin = app_state_dir().join("bin");
            fs::create_dir_all(&home).expect("create home");
            fs::create_dir_all(&project).expect("create project");
            fs::create_dir_all(&bin).expect("create bin");
            fs::create_dir_all(service_data_dir()).expect("create runtime data");
            fs::create_dir_all(active_config_path().parent().expect("active config parent"))
                .expect("create active config parent");
            fs::write(
                active_config_path(),
                serde_json::to_vec_pretty(&json!({
                    "providers": [{
                        "id": "provider-codex",
                        "name": "Codex upstream",
                        "kind": "custom",
                        "protocol": "open_ai",
                        "base_url": "https://example.invalid/v1",
                        "api_key_ref": null,
                        "models": ["example/model"],
                        "timeout_ms": 120000,
                        "healthcheck_path": null
                    }],
                    "routes": [{
                        "id": "route-codex",
                        "name": "Codex route",
                        "input_protocol": "open_ai",
                        "instance_id": "instance-codex",
                        "target_provider_id": "provider-codex",
                        "fallback_provider_id": null,
                        "chain_mode": "direct",
                        "compression_policy": "balanced",
                        "bind_host": "127.0.0.1",
                        "local_port": 18001
                    }],
                    "instances": [{
                        "id": "instance-codex",
                        "name": "Codex instance",
                        "agent": "codex",
                        "route_id": "route-codex",
                        "local_port": 18001,
                        "process_status": "stopped",
                        "log_path": "./logs/route-codex.jsonl"
                    }]
                }))
                .expect("serialize active config"),
            )
            .expect("write active config");
            let proxy = bin.join(if cfg!(windows) {
                "dtoken-proxy.exe"
            } else {
                "dtoken-proxy"
            });
            fs::write(&proxy, b"test proxy").expect("write proxy");
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(&proxy, fs::Permissions::from_mode(0o755))
                    .expect("make proxy executable");
            }
            let previous_home = env::var_os("HOME");
            let previous_userprofile = env::var_os("USERPROFILE");
            let previous_proxy = env::var_os("DTOKEN_PROXY_EXE");
            let previous_bridge = env::var_os("DTOKEN_AGENT_BRIDGE_EXE");
            // The resolver checks DTOKEN_AGENT_BRIDGE_EXE first, so clear any stale value
            // before pointing it at the staged sidecar whose digest build.rs embedded
            // (src-tauri/binaries/dtoken-agent-bridge-<TARGET>[.exe]). The raw cargo
            // build artifact under target/ is NOT the file the embedded hash covers.
            env::remove_var("DTOKEN_AGENT_BRIDGE_EXE");
            let staged_bridge = staged_agent_bridge_matching_embedded_hash().expect(
                "staged Agent bridge sidecar matching the embedded hash must exist; run the platform sidecar prepare script first",
            );
            env::set_var("DTOKEN_AGENT_BRIDGE_EXE", &staged_bridge);
            let bridge = resolve_dtoken_agent_bridge_exe()
                .expect("staged bridge must pass embedded hash and identity verification");
            env::set_var("HOME", &home);
            env::set_var("USERPROFILE", &home);
            env::set_var("DTOKEN_PROXY_EXE", &proxy);
            env::set_var("DTOKEN_AGENT_BRIDGE_EXE", &bridge);

            let draft = HybridAgentIntegrationDraftRequest {
                agent_ids: vec!["codex".to_string()],
                scope: "project".to_string(),
                project_root: Some(project.display().to_string()),
                install_instructions: false,
                install_hook_or_plugin: false,
                install_skill: false,
                install_mcp: true,
            };
            let preview = preview_hybrid_agent_integration(draft.clone());
            assert_eq!(preview["ok"], true, "preview={preview}");
            assert_eq!(
                preview["plan"]["capabilities"].as_array().map(Vec::len),
                Some(1)
            );
            let review_digest = preview["plan"]["review_digest"]
                .as_str()
                .expect("review digest")
                .to_string();
            let capability_path = PathBuf::from(
                preview["plan"]["capabilities"][0]["target_path"]
                    .as_str()
                    .expect("capability path"),
            );
            assert!(!capability_path.exists());

            let applied = apply_hybrid_agent_integration(ApplyHybridAgentIntegrationRequest {
                draft,
                reviewed_digest: review_digest,
            });
            assert_eq!(applied["ok"], true, "applied={applied}");
            assert!(capability_path.is_file());
            let transaction_id = applied["receipt"]["transaction_id"]
                .as_str()
                .expect("transaction id")
                .to_string();
            let verified = verify_hybrid_agent_integration(VerifyHybridAgentIntegrationRequest {
                transaction_id: transaction_id.clone(),
            });
            assert_eq!(verified["ok"], true, "verified={verified}");

            let rolled_back =
                rollback_hybrid_agent_integration(RollbackHybridAgentIntegrationRequest {
                    transaction_id,
                });
            assert_eq!(rolled_back["ok"], true, "rolled_back={rolled_back}");
            assert!(!capability_path.exists());

            match previous_home {
                Some(value) => env::set_var("HOME", value),
                None => env::remove_var("HOME"),
            }
            match previous_userprofile {
                Some(value) => env::set_var("USERPROFILE", value),
                None => env::remove_var("USERPROFILE"),
            }
            match previous_proxy {
                Some(value) => env::set_var("DTOKEN_PROXY_EXE", value),
                None => env::remove_var("DTOKEN_PROXY_EXE"),
            }
            match previous_bridge {
                Some(value) => env::set_var("DTOKEN_AGENT_BRIDGE_EXE", value),
                None => env::remove_var("DTOKEN_AGENT_BRIDGE_EXE"),
            }
        });
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
    fn managed_agent_transport_rejects_unregistered_sensitive_paths() {
        use dtoken_config_patcher::agent::HostPlatform;

        let temp_root = test_temp_root();
        let root = tempfile::tempdir_in(temp_root).expect("temporary home");
        let codex_dir = root.path().join(".codex");
        fs::create_dir_all(&codex_dir).expect("create Codex directory");
        let registered = codex_dir.join("config.toml");
        fs::write(&registered, "model = \"existing\"\n").expect("write Codex config");
        let context = AgentScanContext {
            platform: HostPlatform::Macos,
            home_dir: root.path().to_path_buf(),
            path_dirs: vec![],
            applications_dir: Some(root.path().join("Applications")),
            roaming_app_data: None,
            local_app_data: None,
            additional_evidence: vec![],
            observed_at_ms: 1,
        };
        let request = |target_path: PathBuf| ManagedAgentTransportRequest {
            agent_id: "codex".to_string(),
            target_path,
            endpoint: "http://127.0.0.1:18001".to_string(),
            enabled: true,
            model: Some("test-model".to_string()),
        };

        let resolved =
            resolve_managed_agent_path_with_context(request(registered.clone()), &context)
                .expect("registered Codex target");
        assert_eq!(resolved.target_path, registered);
        let arbitrary_target = if cfg!(windows) {
            PathBuf::from("C:\\unregistered\\x.toml")
        } else {
            PathBuf::from("/tmp/x.toml")
        };
        let arbitrary =
            resolve_managed_agent_path_with_context(request(arbitrary_target), &context)
                .expect_err("arbitrary absolute path must fail closed");
        assert!(arbitrary.contains("outside the registered Agent targets"));
        let ssh = resolve_managed_agent_path_with_context(
            request(root.path().join(".ssh/config")),
            &context,
        )
        .expect_err("sensitive home path must fail closed");
        assert!(ssh.contains("outside the registered Agent targets"));
    }

    #[cfg(unix)]
    #[test]
    fn managed_agent_transport_rejects_symlinked_registered_target() {
        use dtoken_config_patcher::agent::HostPlatform;
        use std::os::unix::fs::symlink;

        let temp_root = test_temp_root();
        let root = tempfile::tempdir_in(temp_root).expect("temporary home");
        let codex_dir = root.path().join(".codex");
        fs::create_dir_all(&codex_dir).expect("create Codex directory");
        let outside = root.path().join("outside.toml");
        fs::write(&outside, "model = \"outside\"\n").expect("write outside target");
        let registered = codex_dir.join("config.toml");
        symlink(&outside, &registered).expect("create config symlink");
        let context = AgentScanContext {
            platform: HostPlatform::Macos,
            home_dir: root.path().to_path_buf(),
            path_dirs: vec![],
            applications_dir: Some(root.path().join("Applications")),
            roaming_app_data: None,
            local_app_data: None,
            additional_evidence: vec![],
            observed_at_ms: 1,
        };
        let error = resolve_managed_agent_path_with_context(
            ManagedAgentTransportRequest {
                agent_id: "codex".to_string(),
                target_path: registered,
                endpoint: "http://127.0.0.1:18001".to_string(),
                enabled: true,
                model: Some("test-model".to_string()),
            },
            &context,
        )
        .expect_err("symlinked Agent config must fail closed");
        assert!(error.contains("symbolic links"));
    }

    #[test]
    fn automatic_connection_route_plan_is_bound_to_discovery_and_uses_agent_models() {
        with_isolated_app_state("automatic-route-plan", || {
            let discovery = json!({
                "revision": "setup-reviewed",
                "external_config_snapshot": { "fingerprint": "external-a" },
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
                source_fingerprint: None,
                source_fingerprint_scope: None,
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
            assert!(prepared
                .plan
                .diff
                .contains("\"wire_api\": \"open_ai_chat\""));
            assert!(!prepared.plan.diff.contains("sk-"));
            assert!(!prepared.plan.diff.contains("secret-value"));

            let mut stale = request.clone();
            stale.discovery_revision = "setup-stale".to_string();
            let error = prepare_connection_route_plan_from_discovery(&stale, &discovery)
                .expect_err("stale discovery must fail");
            assert!(error.contains("changed after discovery"));

            let mut external_stale = request;
            external_stale.source_fingerprint = Some("external-b".to_string());
            let error = prepare_connection_route_plan_from_discovery(&external_stale, &discovery)
                .expect_err("external source drift must fail");
            assert!(error.contains("External Agent or cc-switch configuration changed"));
        });
    }

    #[test]
    fn codex_responses_upstream_forwarding_uses_dynamic_model_passthrough() {
        with_isolated_app_state("codex-local-responses-route", || {
            let discovery = json!({
                "revision": "setup-reviewed",
                "external_config_snapshot": { "fingerprint": "external-a" },
                "agent_bindings": [{
                    "agent_id": "codex",
                    "installed": true,
                    "capability": "one-click",
                    "candidates": [{
                        "id": "codex-ocxlocal",
                        "name": "Codex ocxlocal",
                        "source": "agent-config:codex",
                        "protocol": "responses",
                        "base_url": "http://127.0.0.1:10100/v1",
                        "models": [],
                        "auth_mode": "none"
                    }]
                }]
            });
            // Codex's local OpenAI Responses proxy advertises no static model
            // list: with an empty reviewed request model the provider keeps an
            // empty models array so the request model is passed through.
            let request = ConnectionRoutePlanRequest {
                discovery_revision: "setup-reviewed".to_string(),
                source_fingerprint: None,
                source_fingerprint_scope: None,
                bindings: vec![ConnectionRouteBindingRequest {
                    agent_id: "codex".to_string(),
                    candidate_id: "codex-ocxlocal".to_string(),
                    candidate_source: "agent-config:codex".to_string(),
                    model: String::new(),
                }],
            };

            let (revision, prepared, agents) =
                prepare_connection_route_plan_from_discovery(&request, &discovery)
                    .expect("Codex Responses upstream must be routable");
            assert_eq!(revision, "setup-reviewed");
            assert_eq!(agents, vec!["codex"]);
            assert!(prepared.plan.diff.contains("http://127.0.0.1:10100/v1"));
            assert!(prepared.plan.diff.contains("\"protocol\": \"open_ai\""));
            assert!(prepared.plan.diff.contains("\"kind\": \"local_chain\""));
            assert!(prepared.plan.diff.contains("\"models\": []"));
            assert!(prepared
                .plan
                .diff
                .contains("\"wire_api\": \"open_ai_responses\""));
            assert!(prepared
                .plan
                .diff
                .contains("\"input_protocol\": \"open_ai\""));
            assert!(prepared.plan.diff.contains("\"local_port\": 18001"));
        });
    }

    #[test]
    fn connection_route_merge_replaces_only_selected_agent_owned_routes() {
        let existing = json!({
            "providers": [{
                "id": "user-provider",
                "name": "User provider",
                "kind": "custom",
                "protocol": "open_ai",
                "base_url": "https://user.example.test/v1",
                "api_key_ref": null,
                "models": ["user-model"],
                "timeout_ms": 120000,
                "healthcheck_path": null
            }, {
                "id": "auto-opencode-old",
                "name": "Old OpenCode provider",
                "kind": "custom",
                "protocol": "open_ai",
                "base_url": "https://old.example.test/v1",
                "api_key_ref": null,
                "models": ["old-model"],
                "timeout_ms": 120000,
                "healthcheck_path": null
            }],
            "routes": [{
                "id": "user-route",
                "name": "User route",
                "input_protocol": "open_ai",
                "instance_id": "user-instance",
                "target_provider_id": "user-provider",
                "fallback_provider_id": null,
                "chain_mode": "direct",
                "compression_policy": "balanced",
                "bind_host": "127.0.0.1",
                "local_port": 18002
            }, {
                "id": "auto-opencode-route",
                "name": "Old OpenCode route",
                "input_protocol": "open_ai",
                "instance_id": "auto-opencode-route-instance",
                "target_provider_id": "auto-opencode-old",
                "fallback_provider_id": null,
                "chain_mode": "direct",
                "compression_policy": "balanced",
                "bind_host": "127.0.0.1",
                "local_port": 18001
            }],
            "instances": [{
                "id": "user-instance",
                "name": "User instance",
                "agent": "custom",
                "route_id": "user-route",
                "local_port": 18002,
                "process_status": "stopped",
                "log_path": "./logs/user.jsonl"
            }, {
                "id": "auto-opencode-route-instance",
                "name": "Old OpenCode instance",
                "agent": "opencode",
                "route_id": "auto-opencode-route",
                "local_port": 18001,
                "process_status": "stopped",
                "log_path": "./logs/old.jsonl"
            }],
            "custom_extension": {"preserve": true}
        });
        let content = merge_connection_route_document(
            Some(existing),
            &["opencode".to_string()],
            vec![json!({
                "id": "auto-opencode-new",
                "name": "New OpenCode provider",
                "kind": "official",
                "protocol": "open_ai",
                "base_url": "https://new.example.test/v1",
                "api_key_ref": null,
                "models": ["new-model"],
                "timeout_ms": 120000,
                "healthcheck_path": null
            })],
            vec![json!({
                "id": "auto-opencode-route",
                "name": "New OpenCode route",
                "input_protocol": "open_ai",
                "instance_id": "auto-opencode-route-instance",
                "target_provider_id": "auto-opencode-new",
                "fallback_provider_id": null,
                "chain_mode": "direct",
                "compression_policy": "balanced",
                "bind_host": "127.0.0.1",
                "local_port": 18001
            })],
            vec![json!({
                "id": "auto-opencode-route-instance",
                "name": "New OpenCode instance",
                "agent": "opencode",
                "route_id": "auto-opencode-route",
                "local_port": 18001,
                "process_status": "stopped",
                "log_path": "./logs/new.jsonl"
            })],
        )
        .expect("merge connection routes");
        let merged: Value = serde_json::from_str(&content).expect("parse merged config");

        assert_eq!(merged["custom_extension"]["preserve"], true);
        assert!(merged["providers"]
            .as_array()
            .is_some_and(|providers| providers
                .iter()
                .any(|provider| provider["id"] == "user-provider")));
        assert!(merged["providers"]
            .as_array()
            .is_some_and(|providers| providers
                .iter()
                .any(|provider| provider["id"] == "auto-opencode-new")));
        assert!(!merged["providers"]
            .as_array()
            .is_some_and(|providers| providers
                .iter()
                .any(|provider| provider["id"] == "auto-opencode-old")));
        assert!(merged["routes"]
            .as_array()
            .is_some_and(|routes| routes.iter().any(|route| route["id"] == "user-route")));
        assert_eq!(
            merged["routes"]
                .as_array()
                .and_then(|routes| routes
                    .iter()
                    .find(|route| route["id"] == "auto-opencode-route"))
                .and_then(|route| route["target_provider_id"].as_str()),
            Some("auto-opencode-new")
        );
    }

    #[test]
    fn agent_follow_allows_reviewed_provider_switch_and_rejects_new_identity() {
        let discovery = json!({
            "revision": "setup-reviewed",
            "external_config_snapshot": {
                "agent_configs": [{
                    "agent_id": "cline",
                    "path": "~/.cline/settings/providers.json",
                    "state": "present",
                    "fingerprint": "cline-a"
                }]
            },
            "agent_bindings": [{
                "agent_id": "cline",
                "installed": true,
                "capability": "confirm-required",
                "already_connected": false,
                "candidates": [{
                    "id": "cline-one",
                    "source": "agent-config:cline-cli-v1",
                    "protocol": "open_ai",
                    "base_url": "https://one.example.test/v1",
                    "models": ["model-a"],
                    "auth_mode": "none",
                    "confidence": 99
                }, {
                    "id": "cline-two",
                    "source": "agent-config:cline-cli-v1",
                    "protocol": "open_ai",
                    "base_url": "https://two.example.test/v1",
                    "models": ["model-b"],
                    "auth_mode": "none",
                    "confidence": 99
                }],
                "resolved_candidate": {
                    "id": "cline-one",
                    "source": "agent-config:cline-cli-v1",
                    "protocol": "open_ai",
                    "base_url": "https://one.example.test/v1",
                    "models": ["model-a"],
                    "auth_mode": "none",
                    "confidence": 99
                }
            }]
        });
        let plan = ConnectionRoutePlanRequest {
            discovery_revision: "setup-reviewed".to_string(),
            source_fingerprint: None,
            source_fingerprint_scope: None,
            bindings: vec![ConnectionRouteBindingRequest {
                agent_id: "cline".to_string(),
                candidate_id: "cline-one".to_string(),
                candidate_source: "agent-config:cline-cli-v1".to_string(),
                model: "model-a".to_string(),
            }],
        };
        let authorization =
            stage_agent_auto_follow_authorization(&plan, &discovery, &"a".repeat(64))
                .expect("stage follow")
                .expect("Agent follow authorization");
        assert_eq!(
            authorization
                .authorized_upstream_signatures
                .get("cline")
                .map(Vec::len),
            Some(2)
        );

        let mut switched = discovery.clone();
        switched["agent_bindings"][0]["resolved_candidate"] =
            switched["agent_bindings"][0]["candidates"][1].clone();
        switched["external_config_snapshot"]["agent_configs"][0]["fingerprint"] = json!("cline-b");
        let resolved = resolve_agent_follow_bindings(
            &authorization,
            &switched,
            &HashSet::from(["cline".to_string()]),
        )
        .expect("reviewed provider switch");
        assert_eq!(resolved[0].candidate_id, "cline-two");
        assert_eq!(resolved[0].upstream.model, "model-b");

        switched["agent_bindings"][0]["resolved_candidate"]["base_url"] =
            json!("https://new.example.test/v1");
        let error = resolve_agent_follow_bindings(
            &authorization,
            &switched,
            &HashSet::from(["cline".to_string()]),
        )
        .expect_err("new endpoint must require confirmation");
        assert!(error.contains("outside the approved follow set"));
    }

    #[test]
    fn agent_follow_projection_reports_transport_drift_and_next_action() {
        const SECRET: &str = "AGENT_FOLLOW_PROJECTION_SECRET";
        let mut discovery = json!({
            "revision": "setup-reviewed",
            "agent_bindings": [{
                "agent_id": "cline",
                "agent_name": "Cline",
                "already_connected": false,
                "can_configure": true
            }, {
                "agent_id": "codex",
                "agent_name": "Codex",
                "already_connected": true,
                "can_configure": true
            }, {
                "agent_id": "cursor",
                "agent_name": "Cursor",
                "already_connected": false,
                "can_configure": false
            }]
        });
        let authorization = AgentAutoFollowAuthorization {
            schema_version: AGENT_AUTO_FOLLOW_SCHEMA_VERSION,
            enabled: true,
            authorized_at_ms: 42,
            authorized_upstream_signatures: BTreeMap::new(),
            last_synced_bindings: vec![
                AgentRouteFollowBinding {
                    agent_id: "cline".to_string(),
                    candidate_id: "cline-upstream".to_string(),
                    candidate_source: "agent-config:cline".to_string(),
                    upstream: UpstreamSignature {
                        provider_id: "cline-upstream".to_string(),
                        model: "cline-model".to_string(),
                        protocol: "open_ai".to_string(),
                        normalized_endpoint: "https://cline.example.test/v1".to_string(),
                        profile_scope: Some("agent-config:cline".to_string()),
                    },
                },
                AgentRouteFollowBinding {
                    agent_id: "codex".to_string(),
                    candidate_id: "codex-upstream".to_string(),
                    candidate_source: "agent-config:codex".to_string(),
                    upstream: UpstreamSignature {
                        provider_id: "codex-upstream".to_string(),
                        model: "codex-model".to_string(),
                        protocol: "responses".to_string(),
                        normalized_endpoint: "https://codex.example.test/v1".to_string(),
                        profile_scope: Some("agent-config:codex".to_string()),
                    },
                },
            ],
            last_observed_config_fingerprints: BTreeMap::new(),
            last_observed_source_fingerprint: Some("source-b".to_string()),
            last_synced_source_fingerprint: Some("source-b".to_string()),
            last_synced_route_signature: Some("route-b".to_string()),
            active_config_sha256: "a".repeat(64),
            runtime_identity: Some("runtime-safe".to_string()),
            last_sync_state: "applied_verified".to_string(),
        };
        let receipt = AgentRouteSyncReceipt {
            schema_version: AGENT_AUTO_FOLLOW_SCHEMA_VERSION,
            state: "applied_verified".to_string(),
            source_fingerprint: "source-b".to_string(),
            changed_agents: vec!["cline".to_string()],
            route_before_sha256: Some("b".repeat(64)),
            route_after_sha256: Some("c".repeat(64)),
            rollback_id: Some("rollback-safe".to_string()),
            runtime_state: "runtime_reloaded".to_string(),
            occurred_at_ms: 43,
        };

        project_agent_follow_status(&mut discovery, &authorization, Some(&receipt));

        assert_eq!(
            discovery["agent_bindings"][0]["follow"]["state"],
            "transport_out_of_sync"
        );
        assert_eq!(
            discovery["agent_bindings"][0]["follow"]["next_action"],
            "configure"
        );
        assert_eq!(
            discovery["agent_bindings"][0]["follow"]["current_upstream"]["model"],
            "cline-model"
        );
        assert_eq!(
            discovery["agent_bindings"][0]["follow"]["last_receipt"]["runtime_state"],
            "runtime_reloaded"
        );
        assert_eq!(
            discovery["agent_bindings"][1]["follow"]["state"],
            "following"
        );
        assert_eq!(
            discovery["agent_bindings"][1]["follow"]["next_action"],
            "none"
        );
        assert!(discovery["agent_bindings"][1]["follow"]["last_receipt"].is_null());
        assert_eq!(
            discovery["agent_bindings"][2]["follow"]["state"],
            "not_authorized"
        );
        assert_eq!(discovery["agent_route_follow"]["enabled"], true);
        let encoded = discovery.to_string();
        assert!(!encoded.contains(SECRET));
        assert!(!encoded.contains("api_key"));
        assert!(!encoded.contains("Authorization"));
    }

    #[test]
    fn agent_follow_projection_prioritizes_failed_receipt_over_connected_state() {
        let mut discovery = json!({
            "agent_bindings": [{
                "agent_id": "codex",
                "already_connected": true,
                "can_configure": true
            }]
        });
        let authorization = AgentAutoFollowAuthorization {
            schema_version: AGENT_AUTO_FOLLOW_SCHEMA_VERSION,
            enabled: true,
            authorized_at_ms: 42,
            authorized_upstream_signatures: BTreeMap::new(),
            last_synced_bindings: vec![AgentRouteFollowBinding {
                agent_id: "codex".to_string(),
                candidate_id: "codex-upstream".to_string(),
                candidate_source: "agent-config:codex".to_string(),
                upstream: UpstreamSignature {
                    provider_id: "codex-upstream".to_string(),
                    model: "codex-model".to_string(),
                    protocol: "responses".to_string(),
                    normalized_endpoint: "https://codex.example.test/v1".to_string(),
                    profile_scope: None,
                },
            }],
            last_observed_config_fingerprints: BTreeMap::new(),
            last_observed_source_fingerprint: Some("source-b".to_string()),
            last_synced_source_fingerprint: Some("source-a".to_string()),
            last_synced_route_signature: Some("route-a".to_string()),
            active_config_sha256: "a".repeat(64),
            runtime_identity: Some("runtime-safe".to_string()),
            last_sync_state: "reauthorization_required".to_string(),
        };
        let receipt = AgentRouteSyncReceipt {
            schema_version: AGENT_AUTO_FOLLOW_SCHEMA_VERSION,
            state: "reauthorization_required".to_string(),
            source_fingerprint: "source-b".to_string(),
            changed_agents: vec!["codex".to_string()],
            route_before_sha256: None,
            route_after_sha256: None,
            rollback_id: None,
            runtime_state: "not_reloaded".to_string(),
            occurred_at_ms: 43,
        };

        project_agent_follow_status(&mut discovery, &authorization, Some(&receipt));

        assert_eq!(
            discovery["agent_bindings"][0]["follow"]["state"],
            "reauthorization_required"
        );
        assert_eq!(
            discovery["agent_bindings"][0]["follow"]["next_action"],
            "review_follow"
        );
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
                source_fingerprint: None,
                source_fingerprint_scope: None,
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
    fn verified_transport_adapter_overrides_legacy_prompt_only_route_label() {
        with_isolated_app_state("prompt-only-verified-transport", || {
            let discovery = json!({
                "revision": "setup-reviewed",
                "agent_bindings": [{
                    "agent_id": "cline",
                    "installed": true,
                    "capability": "prompt-only",
                    "can_configure": true,
                    "candidates": [{
                        "id": "cline-current",
                        "name": "Cline current upstream",
                        "source": "agent-config:cline-cli-v1",
                        "protocol": "open_ai",
                        "base_url": "https://cline.example.test/v1",
                        "models": ["cline-model"],
                        "auth_mode": "none"
                    }]
                }]
            });
            let request = ConnectionRoutePlanRequest {
                discovery_revision: "setup-reviewed".to_string(),
                source_fingerprint: None,
                source_fingerprint_scope: None,
                bindings: vec![ConnectionRouteBindingRequest {
                    agent_id: "cline".to_string(),
                    candidate_id: "cline-current".to_string(),
                    candidate_source: "agent-config:cline-cli-v1".to_string(),
                    model: "cline-model".to_string(),
                }],
            };

            let (_, prepared, agents) =
                prepare_connection_route_plan_from_discovery(&request, &discovery)
                    .expect("verified Cline transport must be routable");

            assert_eq!(agents, ["cline"]);
            assert!(prepared.plan.diff.contains("cline-model"));
        });
    }

    #[test]
    fn cc_switch_scoped_route_review_ignores_agent_only_snapshot_drift() {
        with_isolated_app_state("cc-switch-scoped-route-review", || {
            let discovery = json!({
                "revision": "setup-agent-config-b",
                "external_config_snapshot": {
                    "fingerprint": "aggregate-b",
                    "cc_switch": {
                        "fingerprint": "cc-switch-a",
                        "revision": "cc-switch-cc-switch-a"
                    }
                },
                "agent_bindings": [{
                    "agent_id": "opencode",
                    "installed": true,
                    "capability": "one-click",
                    "candidates": [{
                        "id": "cc-one",
                        "name": "cc-switch one",
                        "source": "cc-switch:opencode",
                        "protocol": "open_ai",
                        "base_url": "https://one.example.test/v1",
                        "models": ["model-a"],
                        "auth_mode": "none"
                    }]
                }]
            });
            let request = ConnectionRoutePlanRequest {
                discovery_revision: "cc-switch-cc-switch-a".to_string(),
                source_fingerprint: Some("cc-switch-a".to_string()),
                source_fingerprint_scope: Some("cc_switch".to_string()),
                bindings: vec![ConnectionRouteBindingRequest {
                    agent_id: "opencode".to_string(),
                    candidate_id: "cc-one".to_string(),
                    candidate_source: "cc-switch:opencode".to_string(),
                    model: "model-a".to_string(),
                }],
            };

            prepare_connection_route_plan_from_discovery(&request, &discovery)
                .expect("aggregate Agent drift must not invalidate a cc-switch-scoped review");

            let mut stale = request;
            stale.source_fingerprint = Some("cc-switch-b".to_string());
            let error = prepare_connection_route_plan_from_discovery(&stale, &discovery)
                .expect_err("cc-switch fingerprint drift must invalidate the review");
            assert!(error.contains("cc-switch configuration changed"));
        });
    }

    #[test]
    fn cc_switch_follow_allows_only_preapproved_identities_and_tracks_model_changes() {
        let mut discovery = json!({
            "candidates": [
                {
                    "id": "provider-one",
                    "source": "cc-switch:codex",
                    "protocol": "open_ai",
                    "base_url": "https://one.example.test/v1/",
                    "auth_mode": "none",
                    "confidence": 96
                },
                {
                    "id": "provider-two",
                    "source": "cc-switch:codex",
                    "protocol": "open_ai",
                    "base_url": "https://two.example.test/v1",
                    "auth_mode": "none",
                    "confidence": 72
                },
                {
                    "id": "credential-provider",
                    "source": "cc-switch:codex",
                    "protocol": "open_ai",
                    "base_url": "https://credential.example.test/v1",
                    "auth_mode": "unavailable",
                    "confidence": 96
                }
            ],
            "cc_switch_sync": {
                "active_upstreams": [{
                    "provider_id": "provider-one",
                    "source": "cc-switch:codex",
                    "protocol": "open_ai",
                    "upstream": "https://one.example.test/v1",
                    "models": ["model-a"]
                }]
            }
        });
        let approved =
            authorizable_cc_switch_upstream_signatures(&discovery).expect("approved identities");
        assert_eq!(
            approved.len(),
            2,
            "credential-bearing candidates stay excluded"
        );
        let initial = cc_switch_active_route_identity(&discovery).expect("initial identity");
        let authorization = CcSwitchAutoFollowAuthorization {
            schema_version: CC_SWITCH_AUTO_FOLLOW_SCHEMA_VERSION,
            enabled: true,
            authorized_at_ms: 1,
            authorized_source_fingerprint: "cc-a".to_string(),
            authorized_upstream_signatures: approved,
            last_observed_source_fingerprint: Some("cc-a".to_string()),
            last_synced_source_fingerprint: Some("cc-a".to_string()),
            last_observed_route_signature: Some(initial.route_signature.clone()),
            last_synced_route_signature: Some(initial.route_signature.clone()),
            last_sync_state: "applied_verified".to_string(),
        };
        assert_eq!(
            cc_switch_follow_decision(&authorization, &initial),
            "unchanged"
        );

        discovery["cc_switch_sync"]["active_upstreams"][0]["models"] = json!(["model-b"]);
        let model_change =
            cc_switch_active_route_identity(&discovery).expect("model-only identity");
        assert_eq!(
            model_change.upstream_signature, initial.upstream_signature,
            "model changes must keep the approved upstream identity"
        );
        assert_ne!(model_change.route_signature, initial.route_signature);
        assert_eq!(
            cc_switch_follow_decision(&authorization, &model_change),
            "sync_required"
        );

        discovery["cc_switch_sync"]["active_upstreams"][0] = json!({
            "provider_id": "provider-two",
            "source": "cc-switch:codex",
            "protocol": "open_ai",
            "upstream": "https://two.example.test/v1",
            "models": ["model-c"]
        });
        let known_switch =
            cc_switch_active_route_identity(&discovery).expect("known provider switch");
        assert_eq!(
            cc_switch_follow_decision(&authorization, &known_switch),
            "sync_required",
            "a provider present in the reviewed identity set may follow"
        );

        discovery["cc_switch_sync"]["active_upstreams"][0]["upstream"] =
            json!("https://attacker.example.test/v1");
        let new_endpoint =
            cc_switch_active_route_identity(&discovery).expect("new endpoint identity");
        assert_eq!(
            cc_switch_follow_decision(&authorization, &new_endpoint),
            "reauthorization_required"
        );

        discovery["cc_switch_sync"]["active_upstreams"][0] = json!({
            "provider_id": "provider-two",
            "source": "cc-switch:codex",
            "protocol": "anthropic",
            "upstream": "https://two.example.test/v1",
            "models": ["model-c"]
        });
        let new_protocol =
            cc_switch_active_route_identity(&discovery).expect("new protocol identity");
        assert_eq!(
            cc_switch_follow_decision(&authorization, &new_protocol),
            "reauthorization_required"
        );
    }

    #[test]
    fn cc_switch_sync_requires_endpoint_protocol_and_model_match() {
        let sync = serde_json::Map::from_iter([(
            "active_upstreams".to_string(),
            json!([{
                "provider_id": "cc-codex",
                "protocol": "open_ai",
                "upstream": "https://example.test/v1",
                "models": ["model-a"]
            }]),
        )]);
        let active = vec![json!({
            "endpoint": "https://example.test/v1",
            "protocol": "open_ai",
            "models": ["model-a"]
        })];
        assert_eq!(
            derive_cc_switch_status(true, &sync, &active, "loaded"),
            "synced"
        );

        let wrong_model = vec![json!({
            "endpoint": "https://example.test/v1",
            "protocol": "open_ai",
            "models": ["model-b"]
        })];
        assert_eq!(
            derive_cc_switch_status(true, &sync, &wrong_model, "loaded"),
            "out-of-sync"
        );
    }

    #[test]
    fn cc_switch_sync_requires_selection_for_multiple_active_upstreams() {
        let sync = serde_json::Map::from_iter([(
            "active_upstreams".to_string(),
            json!([
                {"protocol": "open_ai", "upstream": "https://one.test/v1", "models": ["one"]},
                {"protocol": "open_ai", "upstream": "https://two.test/v1", "models": ["two"]}
            ]),
        )]);
        assert_eq!(
            derive_cc_switch_status(true, &sync, &[], "loaded"),
            "needs-selection"
        );
    }

    #[test]
    fn cc_switch_auto_follow_is_explicit_and_persists_only_metadata() {
        with_isolated_app_state("cc-switch-auto-follow", || {
            const SECRET: &str = "cc-switch-auto-follow-secret";
            let default = load_cc_switch_auto_follow_authorization();
            assert!(!default.enabled);
            assert_eq!(execute_cc_switch_auto_follow()["state"], "not_authorized");
            assert!(!active_config_path().exists());

            let authorization = CcSwitchAutoFollowAuthorization {
                schema_version: CC_SWITCH_AUTO_FOLLOW_SCHEMA_VERSION,
                enabled: true,
                authorized_at_ms: 1,
                authorized_source_fingerprint: "external-config-safe".to_string(),
                authorized_upstream_signatures: vec!["approved-upstream".to_string()],
                last_observed_source_fingerprint: Some("external-config-safe".to_string()),
                last_synced_source_fingerprint: None,
                last_observed_route_signature: Some("route-current".to_string()),
                last_synced_route_signature: None,
                last_sync_state: "authorized_waiting_for_change".to_string(),
            };
            save_cc_switch_auto_follow_authorization(&authorization).expect("save authorization");
            let saved = read_app_state(&["routing", "cc-switch-auto-follow.json"])
                .expect("read authorization");
            assert_eq!(saved["enabled"], true);
            assert!(!saved.to_string().contains(SECRET));
            assert!(!saved.to_string().contains("api_key"));
            assert_eq!(
                load_cc_switch_auto_follow_authorization(),
                authorization,
                "the d-token-owned opt-in must round trip exactly"
            );
        });
    }

    #[test]
    fn manual_initial_auto_follow_requires_a_valid_runtime_reload_receipt() {
        with_isolated_app_state("cc-switch-initial-auto-follow-reload", || {
            let request = CcSwitchSyncPlanRequest {
                discovery_revision: "reviewed".to_string(),
                source_fingerprint: "external-config-safe".to_string(),
            };
            let invalid = json!({
                "ok": false,
                "state": "restart_required",
                "managed": false,
            });

            let receipt =
                authorize_initial_cc_switch_auto_follow_after_runtime_reload(&request, &invalid)
                    .expect_err("an unmanaged runtime cannot authorize auto-follow");
            assert_eq!(receipt["enabled"], false);
            assert_eq!(receipt["runtime"]["state"], "restart_required");
            assert!(!app_state_dir()
                .join("routing")
                .join("cc-switch-auto-follow.json")
                .exists());

            for valid in [
                json!({"ok": true, "state": "runtime_reloaded", "managed": true}),
                json!({"ok": true, "state": "runtime_not_running", "managed": false}),
            ] {
                assert!(runtime_reload_receipt_allows_initial_auto_follow(&valid));
            }
            assert!(!runtime_reload_receipt_allows_initial_auto_follow(&invalid));
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

                let shutdown = start_fake_dtoken_runtime();
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

    fn start_fake_dtoken_runtime() -> Arc<AtomicBool> {
        // A parallel workspace test binary or a real service can transiently own
        // the fixed product port. Retry briefly instead of silently skipping the
        // assertions; a persistent foreign owner must fail loudly so the
        // managed-runtime readiness contract is never masked.
        let retry_deadline = Instant::now() + Duration::from_secs(3);
        let listener = loop {
            match TcpListener::bind(("127.0.0.1", DTOKEN_SERVICE_PORT)) {
                Ok(listener) => break listener,
                Err(error) if error.kind() == std::io::ErrorKind::AddrInUse => {
                    if Instant::now() >= retry_deadline {
                        panic!(
                            "cannot bind fake d-token runtime on 127.0.0.1:{DTOKEN_SERVICE_PORT} \
                             after retries: {error}. Stop the external d-token service (or run \
                             workspace tests with the product port free) so the readiness \
                             assertions can actually execute."
                        );
                    }
                    thread::sleep(Duration::from_millis(50));
                }
                Err(error) => panic!("bind fake d-token runtime on product port: {error}"),
            }
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
        shutdown
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
    fn route_sync_reload_refuses_an_unmanaged_runtime() {
        with_isolated_app_state("route-sync-unmanaged-runtime", || {
            let shutdown = start_fake_dtoken_runtime();

            let receipt = reload_managed_runtime_after_route_sync();
            shutdown.store(true, Ordering::SeqCst);

            assert_eq!(receipt["ok"], false, "{receipt:#}");
            assert_eq!(receipt["state"], "restart_required", "{receipt:#}");
            assert_eq!(receipt["managed"], false, "{receipt:#}");
            assert!(receipt["summary"]
                .as_str()
                .is_some_and(|summary| summary.contains("unmanaged")));
        });
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
                managed_mcp_launches: BTreeMap::new(),
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
    fn legacy_rollback_manifest_cannot_bypass_hash_bound_preview_and_apply() {
        with_isolated_app_state("legacy-rollback-blocked", || {
            let active_path = active_config_path();
            fs::create_dir_all(active_path.parent().expect("active config parent"))
                .expect("create active config parent");
            fs::write(&active_path, r#"{"routes":[{"id":"current"}]}"#)
                .expect("write current config");
            write_json_app_state(
                &["rollback-points", "legacy-route.json"],
                &json!({
                    "id": "legacy-route",
                    "target": "d-token active route config",
                    "previous_config": r#"{"routes":[{"id":"stale"}]}"#,
                }),
            )
            .expect("write legacy rollback manifest");

            let result = manage_backup("restore".to_string(), "legacy-route".to_string());
            assert!(!result.ok);
            assert!(result
                .summary
                .contains("Legacy rollback restore is blocked"));
            assert_eq!(
                fs::read_to_string(&active_path).expect("read unchanged config"),
                r#"{"routes":[{"id":"current"}]}"#
            );
            assert_eq!(
                rollback_preview_value("legacy-route")["currently_restorable"],
                false
            );
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
    fn model_pricing_settings_round_trip_stamps_user_provenance() {
        with_isolated_app_state("model-pricing", || {
            let first = save_app_settings(
                "model-pricing".to_string(),
                json!({
                    "prices": [
                        {
                            "model": "  claude-sonnet-4-5 ",
                            "currency": "USD",
                            "inputPerMillion": 3.0,
                            "outputPerMillion": 15.0
                        },
                        {
                            "model": "gpt-4o",
                            "currency": "USD",
                            "inputPerMillion": 2.5,
                            "outputPerMillion": 10.0
                        }
                    ],
                    "unit": "per_1m_tokens"
                }),
            );
            assert!(first.ok, "{}", first.summary);

            let saved = get_app_settings("model-pricing".to_string());
            assert_eq!(saved["state"], "loaded");
            let prices = saved["payload"]["prices"].as_array().expect("prices array");
            assert_eq!(prices.len(), 2);
            // model ids are trimmed, and source/updatedAt are stamped by the backend.
            assert_eq!(prices[0]["model"], "claude-sonnet-4-5");
            assert_eq!(prices[0]["source"], "user");
            assert!(prices[0]["updatedAt"].as_str().is_some());
            assert_eq!(prices[0]["inputPerMillion"], 3.0);
            assert_eq!(prices[0]["outputPerMillion"], 15.0);
            assert_eq!(prices[1]["model"], "gpt-4o");
            assert_eq!(prices[1]["source"], "user");
        });
    }

    #[test]
    fn model_pricing_settings_rejects_invalid_entries_without_writing() {
        with_isolated_app_state("model-pricing-invalid", || {
            let cases: Vec<Value> = vec![
                // Empty model id.
                json!({ "prices": [{ "model": "   ", "currency": "USD", "inputPerMillion": 1.0, "outputPerMillion": 2.0 }] }),
                // Overlong model id (129 chars).
                json!({ "prices": [{ "model": "m".repeat(129), "currency": "USD", "inputPerMillion": 1.0, "outputPerMillion": 2.0 }] }),
                // Unsupported currency.
                json!({ "prices": [{ "model": "claude-sonnet-4-5", "currency": "EUR", "inputPerMillion": 1.0, "outputPerMillion": 2.0 }] }),
                // Negative price.
                json!({ "prices": [{ "model": "claude-sonnet-4-5", "currency": "USD", "inputPerMillion": -1.0, "outputPerMillion": 2.0 }] }),
                // Non-numeric price.
                json!({ "prices": [{ "model": "claude-sonnet-4-5", "currency": "USD", "inputPerMillion": "three", "outputPerMillion": 2.0 }] }),
                // Duplicate model+currency pair.
                json!({ "prices": [
                    { "model": "claude-sonnet-4-5", "currency": "USD", "inputPerMillion": 1.0, "outputPerMillion": 2.0 },
                    { "model": "claude-sonnet-4-5", "currency": "USD", "inputPerMillion": 3.0, "outputPerMillion": 4.0 }
                ] }),
                // Missing prices array entirely.
                json!({ "unit": "per_1m_tokens" }),
            ];
            for (index, case) in cases.iter().enumerate() {
                let result = save_app_settings("model-pricing".to_string(), case.clone());
                assert!(
                    !result.ok,
                    "case {index} should be rejected: {}",
                    result.summary
                );
            }

            // Nothing was written: the section is still missing.
            let saved = get_app_settings("model-pricing".to_string());
            assert_eq!(saved["state"], "missing");
        });
    }

    #[test]
    fn model_pricing_settings_empty_array_restores_default_empty_state() {
        with_isolated_app_state("model-pricing-empty", || {
            let first = save_app_settings(
                "model-pricing".to_string(),
                json!({
                    "prices": [
                        { "model": "gpt-4o", "currency": "USD", "inputPerMillion": 2.5, "outputPerMillion": 10.0 }
                    ]
                }),
            );
            assert!(first.ok);

            // Empty prices array is the verified empty state (not "missing").
            let cleared = save_app_settings("model-pricing".to_string(), json!({ "prices": [] }));
            assert!(cleared.ok, "{}", cleared.summary);
            let saved = get_app_settings("model-pricing".to_string());
            assert_eq!(saved["state"], "loaded");
            let prices = saved["payload"]["prices"].as_array().expect("prices array");
            assert!(prices.is_empty());
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
        let sync = serde_json::Map::from_iter([(
            "active_upstreams".to_string(),
            json!([{
                "protocol": "open_ai",
                "upstream": "http://127.0.0.1:15721",
                "models": ["model-a"]
            }]),
        )]);
        let active = vec![json!({
            "endpoint": normalized_endpoint("http://127.0.0.1:15721/"),
            "protocol": "open_ai",
            "models": ["model-a"]
        })];
        assert_eq!(
            derive_cc_switch_status(true, &sync, &active, "loaded"),
            "synced"
        );
        let diverged = vec![json!({
            "endpoint": "http://127.0.0.1:15722",
            "protocol": "open_ai",
            "models": ["model-a"]
        })];
        assert_eq!(
            derive_cc_switch_status(true, &sync, &diverged, "loaded"),
            "out-of-sync"
        );
        assert_eq!(
            derive_cc_switch_status(true, &sync, &[], "missing"),
            "needs-confirmation"
        );
    }

    #[test]
    fn derive_cc_switch_status_handles_error_state() {
        // Error state: locked_or_corrupt source with error field set
        let mut sync = serde_json::Map::from_iter([("active_upstreams".to_string(), json!([]))]);
        sync.insert("error".to_string(), json!("locked_or_corrupt"));
        assert_eq!(
            derive_cc_switch_status(false, &sync, &[], "unknown"),
            "error"
        );

        // Error state: unsupported_schema
        sync.insert("error".to_string(), json!("unsupported_schema"));
        assert_eq!(
            derive_cc_switch_status(false, &sync, &[], "unknown"),
            "error"
        );

        // Error state: metadata_unavailable
        sync.insert("error".to_string(), json!("metadata_unavailable"));
        assert_eq!(
            derive_cc_switch_status(false, &sync, &[], "unknown"),
            "error"
        );

        // Missing state: no error field, not detected
        sync.remove("error");
        assert_eq!(
            derive_cc_switch_status(false, &sync, &[], "unknown"),
            "unavailable"
        );
    }

    #[test]
    fn enrich_cc_switch_sync_injects_identity_and_receipt_even_when_not_detected() {
        with_isolated_app_state("enrich-cc-switch-undetected", || {
            // Set up a previous sync receipt and authorization so the
            // enrichment picks them up regardless of cc-switch detection.
            let authorization = CcSwitchAutoFollowAuthorization {
                schema_version: CC_SWITCH_AUTO_FOLLOW_SCHEMA_VERSION,
                enabled: true,
                authorized_at_ms: 1000,
                authorized_source_fingerprint: "prev-source".to_string(),
                authorized_upstream_signatures: vec!["sig-v1".to_string()],
                last_observed_source_fingerprint: Some("prev-source".to_string()),
                last_synced_source_fingerprint: Some("prev-source".to_string()),
                last_observed_route_signature: Some("route-prev".to_string()),
                last_synced_route_signature: Some("route-prev".to_string()),
                last_sync_state: "applied_verified".to_string(),
            };
            save_cc_switch_auto_follow_authorization(&authorization).expect("save auth");
            let receipt = CcSwitchRouteSyncReceipt {
                schema_version: CC_SWITCH_AUTO_FOLLOW_SCHEMA_VERSION,
                state: "applied_verified".to_string(),
                source_fingerprint: "prev-source".to_string(),
                route_before_sha256: Some("before-hash".to_string()),
                route_after_sha256: Some("after-hash".to_string()),
                rollback_id: Some("rollback-1".to_string()),
                runtime_state: "runtime_reloaded".to_string(),
                occurred_at_ms: 1000,
            };
            save_cc_switch_route_sync_receipt(&receipt);

            // Build a discovery where cc-switch is NOT detected (unavailable).
            let discovery = json!({
                "cc_switch_sync": {
                    "detected": false,
                    "source_state": "missing",
                    "status": "unavailable",
                    "error": Value::Null,
                    "profile": Value::Null,
                    "profile_source": "not_exposed_by_safe_schema",
                    "upstream": Value::Null,
                    "active_upstreams": [],
                    "connection_path": Value::Null,
                    "source": "cc-switch-read-only",
                    "writes_performed": false,
                    "source_fingerprint": "cc-switch-sync-abc",
                    "source_revision": "cc-switch-sync-cc-switch-sync-abc",
                    "route_snapshot_identity": Value::Null,
                    "last_receipt": Value::Null,
                },
                "agent_bindings": [],
            });
            let enriched = enrich_upstream_discovery(discovery);
            let sync = enriched
                .get("cc_switch_sync")
                .expect("cc_switch_sync present");

            // route_snapshot_identity must be populated from the last synced
            // authorization even though cc-switch is not currently detected.
            let identity = sync
                .get("route_snapshot_identity")
                .expect("route_snapshot_identity present");
            assert_eq!(
                identity["route_signature"].as_str(),
                Some("route-prev"),
                "last synced route signature visible even when cc-switch is unavailable"
            );
            assert_eq!(
                identity["source_fingerprint"].as_str(),
                Some("prev-source"),
                "source_fingerprint in route_snapshot_identity"
            );

            // last_receipt must be populated from persistent storage.
            let receipt_field = sync.get("last_receipt").expect("last_receipt present");
            assert_eq!(receipt_field["state"].as_str(), Some("applied_verified"));
            assert_eq!(
                receipt_field["source_fingerprint"].as_str(),
                Some("prev-source")
            );
            assert_eq!(receipt_field["rollback_id"].as_str(), Some("rollback-1"));
            assert!(receipt_field["occurred_at_ms"].as_u64() == Some(1000));
        });
    }

    #[test]
    fn enrich_cc_switch_sync_falls_back_to_observed_identity_when_no_sync_completed() {
        with_isolated_app_state("enrich-cc-switch-observed-fallback", || {
            // Authorization with observed but not synced route signature.
            let authorization = CcSwitchAutoFollowAuthorization {
                schema_version: CC_SWITCH_AUTO_FOLLOW_SCHEMA_VERSION,
                enabled: true,
                authorized_at_ms: 2000,
                authorized_source_fingerprint: "observed-source".to_string(),
                authorized_upstream_signatures: vec!["sig-v2".to_string()],
                last_observed_source_fingerprint: Some("observed-source".to_string()),
                last_synced_source_fingerprint: None,
                last_observed_route_signature: Some("route-observed".to_string()),
                last_synced_route_signature: None,
                last_sync_state: "authorized_waiting_for_change".to_string(),
            };
            save_cc_switch_auto_follow_authorization(&authorization).expect("save auth");

            let discovery = json!({
                "cc_switch_sync": {
                    "detected": false,
                    "source_state": "missing",
                    "status": "unavailable",
                    "error": Value::Null,
                    "active_upstreams": [],
                    "source": "cc-switch-read-only",
                    "writes_performed": false,
                    "source_fingerprint": "cc-switch-sync-def",
                    "source_revision": "cc-switch-sync-cc-switch-sync-def",
                    "route_snapshot_identity": Value::Null,
                    "last_receipt": Value::Null,
                },
                "agent_bindings": [],
            });
            let enriched = enrich_upstream_discovery(discovery);
            let sync = enriched
                .get("cc_switch_sync")
                .expect("cc_switch_sync present");
            let identity = sync
                .get("route_snapshot_identity")
                .expect("route_snapshot_identity present");
            assert_eq!(
                identity["route_signature"].as_str(),
                Some("route-observed"),
                "fallback to observed route signature when no sync completed"
            );
            assert_eq!(
                identity["source_fingerprint"].as_str(),
                Some("observed-source"),
                "observed source_fingerprint in fallback identity"
            );
        });
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
