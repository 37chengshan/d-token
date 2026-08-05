use anyhow::{anyhow, bail, Context, Result};
use dtoken_types::{ConfigToolKind, RiskLevel};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
use uuid::Uuid;

use crate::{atomic_rename, sync_directory, ConfigPatcher, ExpectedFileState, PreparedChange};

const SCHEMA_VERSION: u32 = 1;
const ENTRY_JSON: &str = "dtoken-codebase-memory";
const OPENCODE_ENTRY_JSON: &str = "codebase-memory-mcp";
const ENTRY_TOML: &str = "dtoken_codebase_memory";
const HOOK_ENTRY_ID: &str = "dtoken-codebase-memory-session-start-v1";
const HOOK_MATCHER: &str = "startup|resume|compact";
const TRANSACTION_DIRECTORY: &str = "codebase-memory-integration-transactions";
const MAX_CONFIG_BYTES: u64 = 2 * 1024 * 1024;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CodebaseMemoryIntegrationScope {
    Global,
    Project,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CodebaseMemoryIntegrationSurface {
    Mcp,
    Hook,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CodebaseMemoryIntegrationRequest {
    pub agent_id: String,
    pub scope: CodebaseMemoryIntegrationScope,
    pub project_root: Option<PathBuf>,
    pub launcher_path: PathBuf,
    pub launcher_sha256: String,
    pub upstream_path: PathBuf,
    pub upstream_sha256: String,
    pub cache_dir: PathBuf,
    pub surface: CodebaseMemoryIntegrationSurface,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CodebaseMemoryIntegrationTargetPlan {
    pub target_path: PathBuf,
    pub before_sha256: String,
    pub after_sha256: String,
    pub diff: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CodebaseMemoryIntegrationPlan {
    pub schema_version: u32,
    pub agent_id: String,
    pub scope: CodebaseMemoryIntegrationScope,
    pub canonical_project_root: Option<PathBuf>,
    pub launcher_path: PathBuf,
    pub launcher_sha256: String,
    pub upstream_path: PathBuf,
    pub upstream_sha256: String,
    pub cache_dir: PathBuf,
    pub surface: CodebaseMemoryIntegrationSurface,
    pub target: CodebaseMemoryIntegrationTargetPlan,
    pub consent_summary: String,
    pub review_digest: String,
    pub trust_required: bool,
    pub new_session_required: bool,
}

#[derive(Debug, Clone)]
pub struct PreparedCodebaseMemoryIntegration {
    pub plan: CodebaseMemoryIntegrationPlan,
    change: Option<PreparedChange>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CodebaseMemoryIntegrationTransactionState {
    Applied,
    Verified,
    Changed,
    RolledBack,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CodebaseMemoryIntegrationTransactionManifest {
    pub schema_version: u32,
    pub transaction_id: String,
    pub plan: CodebaseMemoryIntegrationPlan,
    pub rollback_id: Option<String>,
    pub created_directories: Vec<PathBuf>,
    pub state: CodebaseMemoryIntegrationTransactionState,
    pub applied_at_ms: u64,
    pub verified_at_ms: Option<u64>,
    pub rolled_back_at_ms: Option<u64>,
    pub failure_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CodebaseMemoryIntegrationReceipt {
    pub transaction_id: String,
    pub review_digest: String,
    pub agent_id: String,
    pub surface: CodebaseMemoryIntegrationSurface,
    pub rollback_id: Option<String>,
    pub target_path: PathBuf,
    pub consent_summary: String,
    pub trust_required: bool,
    pub new_session_required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CodebaseMemoryIntegrationVerification {
    pub ok: bool,
    pub transaction_id: String,
    pub agent_id: String,
    pub surface: CodebaseMemoryIntegrationSurface,
    pub target_path: PathBuf,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CodebaseMemoryIntegrationRollbackReceipt {
    pub transaction_id: String,
    pub target_path: PathBuf,
    pub restored: bool,
    pub rolled_back_at_ms: u64,
}

#[derive(Serialize)]
struct ReviewBinding<'a> {
    schema_version: u32,
    agent_id: &'a str,
    scope: CodebaseMemoryIntegrationScope,
    canonical_project_root: &'a Option<PathBuf>,
    launcher_path: &'a Path,
    launcher_sha256: &'a str,
    upstream_path: &'a Path,
    upstream_sha256: &'a str,
    cache_dir: &'a Path,
    surface: CodebaseMemoryIntegrationSurface,
    target_path: &'a Path,
    before_sha256: &'a str,
    after_sha256: &'a str,
    consent_summary: &'a str,
    trust_required: bool,
    new_session_required: bool,
}

pub fn prepare_codebase_memory_integration(
    request: &CodebaseMemoryIntegrationRequest,
    home_dir: &Path,
    backup_root: &Path,
) -> Result<PreparedCodebaseMemoryIntegration> {
    let normalized = normalize_request(request, home_dir)?;
    let target_path = target_path(
        &normalized,
        &canonical_existing_directory(home_dir, "home")?,
    )?;
    let content = desired_content(&normalized, &target_path)?;
    let patcher = ConfigPatcher::new(backup_root);
    let change = patcher.prepare_change(
        &target_path,
        &content,
        ConfigToolKind::Agent,
        Some(normalized.agent_id.clone()),
        RiskLevel::High,
    )?;
    let target = CodebaseMemoryIntegrationTargetPlan {
        target_path: change.target_path.clone(),
        before_sha256: change.plan.before_hash.clone(),
        after_sha256: change.plan.after_hash.clone(),
        diff: change.plan.diff.clone(),
    };
    let mut plan = CodebaseMemoryIntegrationPlan {
        schema_version: SCHEMA_VERSION,
        agent_id: normalized.agent_id,
        scope: normalized.scope,
        canonical_project_root: normalized.project_root,
        launcher_path: normalized.launcher_path,
        launcher_sha256: normalized.launcher_sha256,
        upstream_path: normalized.upstream_path,
        upstream_sha256: normalized.upstream_sha256,
        cache_dir: normalized.cache_dir,
        surface: normalized.surface,
        target,
        consent_summary: consent_summary(normalized.surface).to_string(),
        review_digest: String::new(),
        trust_required: normalized.surface == CodebaseMemoryIntegrationSurface::Hook,
        new_session_required: true,
    };
    plan.review_digest = review_digest(&plan)?;
    let change = (change.plan.before_hash != change.plan.after_hash).then_some(change);
    Ok(PreparedCodebaseMemoryIntegration { plan, change })
}

pub fn apply_codebase_memory_integration(
    prepared: PreparedCodebaseMemoryIntegration,
    backup_root: &Path,
    reviewed_digest: &str,
) -> Result<CodebaseMemoryIntegrationReceipt> {
    validate_plan(&prepared.plan, true)?;
    if reviewed_digest != prepared.plan.review_digest {
        bail!("codebase-memory integration review is stale");
    }
    if let Some(change) = prepared.change.as_ref() {
        validate_prepared_change(&prepared.plan, change)?;
    } else if prepared.plan.target.before_sha256 != prepared.plan.target.after_sha256 {
        bail!("codebase-memory prepared change is missing");
    }

    let created_directories = if prepared.change.is_some() {
        create_missing_target_directories(&prepared.plan.target.target_path)?
    } else {
        Vec::new()
    };
    let patcher = ConfigPatcher::new(backup_root);
    let transaction_id = Uuid::new_v4().to_string();
    let rollback_id = match prepared.change.as_ref() {
        Some(change) => match patcher.apply_transaction(std::slice::from_ref(change)) {
            Ok(receipt) => receipt
                .changes
                .first()
                .map(|item| item.backup.rollback_id.clone()),
            Err(error) => {
                let _ = remove_created_directories(&created_directories);
                return Err(error);
            }
        },
        None => None,
    };
    let manifest = CodebaseMemoryIntegrationTransactionManifest {
        schema_version: SCHEMA_VERSION,
        transaction_id: transaction_id.clone(),
        plan: prepared.plan.clone(),
        rollback_id: rollback_id.clone(),
        created_directories: created_directories.clone(),
        state: CodebaseMemoryIntegrationTransactionState::Applied,
        applied_at_ms: now_ms(),
        verified_at_ms: None,
        rolled_back_at_ms: None,
        failure_reason: None,
    };
    if let Err(save_error) = save_manifest(backup_root, &manifest) {
        let restore_result = restore_applied_change(&patcher, &manifest)
            .and_then(|_| remove_created_directories(&created_directories));
        return match restore_result {
            Ok(()) => Err(save_error.context(
                "codebase-memory transaction manifest persistence failed; configuration restored",
            )),
            Err(restore_error) => Err(anyhow!(
                "codebase-memory manifest persistence failed and restore was incomplete: {restore_error:#}; manifest error: {save_error:#}"
            )),
        };
    }
    Ok(CodebaseMemoryIntegrationReceipt {
        transaction_id,
        review_digest: prepared.plan.review_digest,
        agent_id: prepared.plan.agent_id,
        surface: prepared.plan.surface,
        rollback_id,
        target_path: prepared.plan.target.target_path,
        consent_summary: prepared.plan.consent_summary,
        trust_required: prepared.plan.trust_required,
        new_session_required: prepared.plan.new_session_required,
    })
}

pub fn get_codebase_memory_integration_transaction(
    transaction_id: &str,
    backup_root: &Path,
) -> Result<CodebaseMemoryIntegrationTransactionManifest> {
    load_manifest(backup_root, transaction_id)
}

pub fn verify_codebase_memory_integration(
    transaction_id: &str,
    backup_root: &Path,
) -> Result<CodebaseMemoryIntegrationVerification> {
    let mut manifest = load_manifest(backup_root, transaction_id)?;
    if manifest.state == CodebaseMemoryIntegrationTransactionState::RolledBack {
        bail!("codebase-memory integration transaction was already rolled back");
    }
    let verification = match verify_semantics(&manifest.plan) {
        Ok(()) => CodebaseMemoryIntegrationVerification {
            ok: true,
            transaction_id: transaction_id.to_string(),
            agent_id: manifest.plan.agent_id.clone(),
            surface: manifest.plan.surface,
            target_path: manifest.plan.target.target_path.clone(),
            detail: "configuration hash, launcher identity, scope, and managed entry verified"
                .to_string(),
        },
        Err(error) => {
            manifest.state = CodebaseMemoryIntegrationTransactionState::Changed;
            manifest.failure_reason = Some(error.to_string());
            save_manifest(backup_root, &manifest)?;
            return Err(error);
        }
    };
    manifest.state = CodebaseMemoryIntegrationTransactionState::Verified;
    manifest.verified_at_ms = Some(now_ms());
    manifest.failure_reason = None;
    save_manifest(backup_root, &manifest)?;
    Ok(verification)
}

pub fn rollback_codebase_memory_integration(
    transaction_id: &str,
    backup_root: &Path,
) -> Result<CodebaseMemoryIntegrationRollbackReceipt> {
    let mut manifest = load_manifest(backup_root, transaction_id)?;
    if manifest.state == CodebaseMemoryIntegrationTransactionState::RolledBack {
        return Ok(CodebaseMemoryIntegrationRollbackReceipt {
            transaction_id: transaction_id.to_string(),
            target_path: manifest.plan.target.target_path,
            restored: false,
            rolled_back_at_ms: manifest.rolled_back_at_ms.unwrap_or_default(),
        });
    }
    let patcher = ConfigPatcher::new(backup_root);
    if let Err(error) = restore_applied_change(&patcher, &manifest)
        .and_then(|_| remove_created_directories(&manifest.created_directories))
    {
        manifest.state = CodebaseMemoryIntegrationTransactionState::Failed;
        manifest.failure_reason = Some(format!("codebase-memory rollback failed: {error:#}"));
        save_manifest(backup_root, &manifest)?;
        return Err(error);
    }
    let rolled_back_at_ms = now_ms();
    manifest.state = CodebaseMemoryIntegrationTransactionState::RolledBack;
    manifest.rolled_back_at_ms = Some(rolled_back_at_ms);
    manifest.failure_reason = None;
    save_manifest(backup_root, &manifest)?;
    Ok(CodebaseMemoryIntegrationRollbackReceipt {
        transaction_id: transaction_id.to_string(),
        target_path: manifest.plan.target.target_path,
        restored: manifest.rollback_id.is_some(),
        rolled_back_at_ms,
    })
}

#[derive(Debug)]
struct NormalizedRequest {
    agent_id: String,
    scope: CodebaseMemoryIntegrationScope,
    project_root: Option<PathBuf>,
    launcher_path: PathBuf,
    launcher_sha256: String,
    upstream_path: PathBuf,
    upstream_sha256: String,
    cache_dir: PathBuf,
    surface: CodebaseMemoryIntegrationSurface,
}

fn normalize_request(
    request: &CodebaseMemoryIntegrationRequest,
    home_dir: &Path,
) -> Result<NormalizedRequest> {
    canonical_existing_directory(home_dir, "home")?;
    if !matches!(
        request.agent_id.as_str(),
        "opencode" | "codex" | "claude-code"
    ) {
        bail!("unsupported codebase-memory Agent: {}", request.agent_id);
    }
    if request.surface == CodebaseMemoryIntegrationSurface::Hook
        && request.agent_id != "claude-code"
    {
        bail!("unsupported codebase-memory Agent/scope/surface combination")
    }
    if request.agent_id == "claude-code"
        && request.surface == CodebaseMemoryIntegrationSurface::Mcp
        && request.scope == CodebaseMemoryIntegrationScope::Global
    {
        bail!("global Claude MCP is unsupported because ~/.claude.json contains private client state; use project scope")
    }
    let project_root = Some(canonical_existing_directory(
        request.project_root.as_deref().context(
            "codebase-memory integration requires an explicit approved project root for every scope",
        )?,
        "approved project root",
    )?);
    validate_sha256(&request.launcher_sha256)?;
    let launcher_path = canonical_existing_file(&request.launcher_path, "launcher")?;
    let actual_launcher_hash = hash_file(&launcher_path)?;
    if actual_launcher_hash != request.launcher_sha256 {
        bail!("codebase-memory launcher hash mismatch");
    }
    validate_sha256(&request.upstream_sha256)?;
    let upstream_path = canonical_existing_file(&request.upstream_path, "upstream executable")?;
    let actual_upstream_hash = hash_file(&upstream_path)?;
    if actual_upstream_hash != request.upstream_sha256 {
        bail!("codebase-memory upstream hash mismatch");
    }
    let cache_dir = canonical_existing_directory(&request.cache_dir, "cache directory")?;
    Ok(NormalizedRequest {
        agent_id: request.agent_id.clone(),
        scope: request.scope,
        project_root,
        launcher_path,
        launcher_sha256: request.launcher_sha256.clone(),
        upstream_path,
        upstream_sha256: request.upstream_sha256.clone(),
        cache_dir,
        surface: request.surface,
    })
}

fn target_path(request: &NormalizedRequest, home: &Path) -> Result<PathBuf> {
    let project = request.project_root.as_deref();
    let project = || project.context("project-scoped integration is missing its project root");
    Ok(
        match (request.agent_id.as_str(), request.surface, request.scope) {
            (
                "opencode",
                CodebaseMemoryIntegrationSurface::Mcp,
                CodebaseMemoryIntegrationScope::Global,
            ) => home.join(".config/opencode/opencode.json"),
            (
                "opencode",
                CodebaseMemoryIntegrationSurface::Mcp,
                CodebaseMemoryIntegrationScope::Project,
            ) => project()?.join("opencode.json"),
            (
                "codex",
                CodebaseMemoryIntegrationSurface::Mcp,
                CodebaseMemoryIntegrationScope::Global,
            ) => home.join(".codex/config.toml"),
            (
                "codex",
                CodebaseMemoryIntegrationSurface::Mcp,
                CodebaseMemoryIntegrationScope::Project,
            ) => project()?.join(".codex/config.toml"),
            (
                "claude-code",
                CodebaseMemoryIntegrationSurface::Mcp,
                CodebaseMemoryIntegrationScope::Project,
            ) => project()?.join(".mcp.json"),
            (
                "claude-code",
                CodebaseMemoryIntegrationSurface::Hook,
                CodebaseMemoryIntegrationScope::Global,
            ) => home.join(".claude/settings.json"),
            (
                "claude-code",
                CodebaseMemoryIntegrationSurface::Hook,
                CodebaseMemoryIntegrationScope::Project,
            ) => project()?.join(".claude/settings.local.json"),
            _ => bail!("unsupported codebase-memory Agent/scope/surface combination"),
        },
    )
}

fn desired_content(request: &NormalizedRequest, path: &Path) -> Result<String> {
    let launcher = path_text(&request.launcher_path, "launcher")?;
    match (request.agent_id.as_str(), request.surface) {
        ("codex", CodebaseMemoryIntegrationSurface::Mcp) => {
            let args = normalized_mcp_arguments(request)?;
            merge_codex_mcp(path, launcher, &args)
        }
        ("opencode", CodebaseMemoryIntegrationSurface::Mcp) => {
            let args = normalized_mcp_arguments(request)?;
            let command = std::iter::once(launcher.to_string())
                .chain(args.iter().cloned())
                .collect::<Vec<_>>();
            let desired = json!({
                "type": "local",
                "command": command,
                "enabled": true,
                "timeout": 60000
            });
            // Use the upstream's canonical OpenCode server key so a project-scoped
            // managed entry replaces, rather than competes with, an existing direct
            // upstream entry. The transaction still preserves and exactly restores it.
            let merged = merge_json_entry(path, "mcp", OPENCODE_ENTRY_JSON, desired)?;
            let mut root: Value = serde_json::from_str(&merged)?;
            if root.get("$schema").is_none() {
                root.as_object_mut()
                    .context("OpenCode configuration root must be an object")?
                    .insert(
                        "$schema".to_string(),
                        Value::String("https://opencode.ai/config.json".to_string()),
                    );
            }
            Ok(serde_json::to_string_pretty(&root)? + "\n")
        }
        ("claude-code", CodebaseMemoryIntegrationSurface::Mcp) => {
            let args = normalized_mcp_arguments(request)?;
            let desired = json!({"command": launcher, "args": args});
            merge_json_entry(path, "mcpServers", ENTRY_JSON, desired)
        }
        ("claude-code", CodebaseMemoryIntegrationSurface::Hook) => {
            merge_session_start_hook(path, &request.launcher_path, false)
        }
        _ => bail!("unsupported codebase-memory integration surface"),
    }
}

fn normalized_mcp_arguments(request: &NormalizedRequest) -> Result<Vec<String>> {
    mcp_arguments(
        &request.upstream_path,
        request
            .project_root
            .as_deref()
            .context("approved project root is missing")?,
        &request.cache_dir,
    )
}

fn merge_session_start_hook(path: &Path, launcher: &Path, codex: bool) -> Result<String> {
    let mut root = read_json_object(path)?;
    let hooks = object_field(&mut root, "hooks")?;
    let sessions = hooks
        .entry("SessionStart".to_string())
        .or_insert_with(|| Value::Array(Vec::new()));
    let entries = sessions
        .as_array_mut()
        .context("hooks.SessionStart must be an array")?;
    entries.retain(|entry| {
        entry.get("dtoken_managed_id").and_then(Value::as_str) != Some(HOOK_ENTRY_ID)
    });
    entries.push(session_start_hook(launcher, codex)?);
    Ok(serde_json::to_string_pretty(&Value::Object(root))? + "\n")
}

fn session_start_hook(launcher: &Path, codex: bool) -> Result<Value> {
    let mut hook = json!({
        "dtoken_managed_id": HOOK_ENTRY_ID,
        "matcher": HOOK_MATCHER,
        "hooks": [{
            "type": "command",
            "command": hook_command(launcher)?,
            "timeout": 5
        }]
    });
    if codex {
        hook["hooks"][0]["statusMessage"] =
            json!("Loading optional d-token Codebase Memory guidance");
    }
    Ok(hook)
}

fn merge_json_entry(
    path: &Path,
    collection: &str,
    entry_name: &str,
    desired: Value,
) -> Result<String> {
    let mut root = read_json_object(path)?;
    let entries = object_field(&mut root, collection)?;
    match entries.get(entry_name) {
        Some(existing) if existing == &desired => {}
        Some(_) => bail!(
            "Agent entry {entry_name} already exists and is not the reviewed codebase-memory entry"
        ),
        None => {
            entries.insert(entry_name.to_string(), desired);
        }
    }
    Ok(serde_json::to_string_pretty(&Value::Object(root))? + "\n")
}

fn merge_codex_mcp(path: &Path, launcher: &str, arguments: &[String]) -> Result<String> {
    let text = read_text(path)?;
    let mut document = if text.trim().is_empty() {
        toml_edit::DocumentMut::new()
    } else {
        text.parse::<toml_edit::DocumentMut>()?
    };
    if document.get("mcp_servers").is_none() {
        document["mcp_servers"] = toml_edit::Item::Table(toml_edit::Table::new());
    }
    let servers = document["mcp_servers"]
        .as_table_mut()
        .context("mcp_servers must be a TOML table")?;
    let mut server = toml_edit::Table::new();
    server["command"] = toml_edit::value(launcher);
    let mut args = toml_edit::Array::new();
    for argument in arguments {
        args.push(argument.as_str());
    }
    server["args"] = toml_edit::value(args);
    server["enabled"] = toml_edit::value(true);
    server["startup_timeout_sec"] = toml_edit::value(60);
    server["tool_timeout_sec"] = toml_edit::value(120);
    let desired = toml_edit::Item::Table(server.clone()).to_string();
    match servers.get(ENTRY_TOML) {
        Some(existing) if existing.to_string() == desired => {}
        Some(_) => bail!(
            "Codex MCP entry {ENTRY_TOML} already exists and is not the reviewed codebase-memory entry"
        ),
        None => {
            servers[ENTRY_TOML] = toml_edit::Item::Table(server);
        }
    }
    Ok(document.to_string())
}

fn validate_plan(plan: &CodebaseMemoryIntegrationPlan, validate_live: bool) -> Result<()> {
    if plan.schema_version != SCHEMA_VERSION {
        bail!("unsupported codebase-memory integration plan schema");
    }
    if !matches!(plan.agent_id.as_str(), "opencode" | "codex" | "claude-code") {
        bail!("unsupported codebase-memory Agent in plan");
    }
    if plan.agent_id == "claude-code"
        && plan.surface == CodebaseMemoryIntegrationSurface::Mcp
        && plan.scope == CodebaseMemoryIntegrationScope::Global
    {
        bail!("global Claude codebase-memory MCP is unsupported");
    }
    if plan.surface == CodebaseMemoryIntegrationSurface::Hook && plan.agent_id != "claude-code" {
        bail!("unsupported codebase-memory Agent/scope/surface combination");
    }
    if plan.consent_summary != consent_summary(plan.surface)
        || plan.trust_required != (plan.surface == CodebaseMemoryIntegrationSurface::Hook)
        || !plan.new_session_required
    {
        bail!("codebase-memory integration consent metadata is inconsistent");
    }
    if plan.canonical_project_root.is_none() {
        bail!("codebase-memory integration plan is missing its approved project root");
    }
    validate_sha256(&plan.launcher_sha256)?;
    validate_sha256(&plan.upstream_sha256)?;
    validate_sha256(&plan.target.before_sha256)?;
    validate_sha256(&plan.target.after_sha256)?;
    if review_digest(plan)? != plan.review_digest {
        bail!("codebase-memory integration plan review digest is invalid");
    }
    if validate_live {
        verify_scope_and_launcher(plan)?;
    }
    Ok(())
}

fn validate_prepared_change(
    plan: &CodebaseMemoryIntegrationPlan,
    change: &PreparedChange,
) -> Result<()> {
    if change.target_path != plan.target.target_path
        || change.plan.before_hash != plan.target.before_sha256
        || change.plan.after_hash != plan.target.after_sha256
        || change.next_content_hash() != plan.target.after_sha256
    {
        bail!("prepared configuration change is not bound to the reviewed codebase-memory plan");
    }
    Ok(())
}

fn review_digest(plan: &CodebaseMemoryIntegrationPlan) -> Result<String> {
    let binding = ReviewBinding {
        schema_version: plan.schema_version,
        agent_id: &plan.agent_id,
        scope: plan.scope,
        canonical_project_root: &plan.canonical_project_root,
        launcher_path: &plan.launcher_path,
        launcher_sha256: &plan.launcher_sha256,
        upstream_path: &plan.upstream_path,
        upstream_sha256: &plan.upstream_sha256,
        cache_dir: &plan.cache_dir,
        surface: plan.surface,
        target_path: &plan.target.target_path,
        before_sha256: &plan.target.before_sha256,
        after_sha256: &plan.target.after_sha256,
        consent_summary: &plan.consent_summary,
        trust_required: plan.trust_required,
        new_session_required: plan.new_session_required,
    };
    Ok(hash_bytes(&serde_json::to_vec(&binding)?))
}

fn verify_semantics(plan: &CodebaseMemoryIntegrationPlan) -> Result<()> {
    validate_plan(plan, true)?;
    let bytes = fs::read(&plan.target.target_path).with_context(|| {
        format!(
            "failed to read codebase-memory Agent target {}",
            plan.target.target_path.display()
        )
    })?;
    if hash_bytes(&bytes) != plan.target.after_sha256 {
        bail!("codebase-memory Agent target no longer matches the reviewed hash");
    }
    let text = std::str::from_utf8(&bytes).context("Agent configuration is not UTF-8")?;
    let launcher = path_text(&plan.launcher_path, "launcher")?;
    match (plan.agent_id.as_str(), plan.surface) {
        ("codex", CodebaseMemoryIntegrationSurface::Mcp) => {
            let expected_args = plan_mcp_arguments(plan)?;
            let document = text.parse::<toml_edit::DocumentMut>()?;
            let server = document
                .get("mcp_servers")
                .and_then(|item| item.get(ENTRY_TOML))
                .context("reviewed Codex codebase-memory MCP entry is missing")?;
            if server.get("command").and_then(|item| item.as_str()) != Some(launcher)
                || server
                    .get("args")
                    .and_then(|item| item.as_array())
                    .map(|args| {
                        args.iter()
                            .filter_map(|value| value.as_str())
                            .collect::<Vec<_>>()
                    })
                    != Some(expected_args.iter().map(String::as_str).collect())
                || server.get("enabled").and_then(|item| item.as_bool()) != Some(true)
                || server
                    .get("startup_timeout_sec")
                    .and_then(|item| item.as_integer())
                    != Some(60)
                || server
                    .get("tool_timeout_sec")
                    .and_then(|item| item.as_integer())
                    != Some(120)
            {
                bail!("Codex codebase-memory MCP entry failed semantic verification");
            }
        }
        ("opencode", CodebaseMemoryIntegrationSurface::Mcp) => {
            let expected_args = plan_mcp_arguments(plan)?;
            let root: Value = serde_json::from_str(text)?;
            let command = std::iter::once(launcher.to_string())
                .chain(expected_args.iter().cloned())
                .collect::<Vec<_>>();
            let expected = json!({
                "type": "local",
                "command": command,
                "enabled": true,
                "timeout": 60000
            });
            if root.pointer(&format!("/mcp/{OPENCODE_ENTRY_JSON}")) != Some(&expected) {
                bail!("OpenCode codebase-memory MCP entry failed semantic verification");
            }
        }
        ("claude-code", CodebaseMemoryIntegrationSurface::Mcp) => {
            let expected_args = plan_mcp_arguments(plan)?;
            let root: Value = serde_json::from_str(text)?;
            let expected = json!({"command": launcher, "args": expected_args});
            if root.pointer(&format!("/mcpServers/{ENTRY_JSON}")) != Some(&expected) {
                bail!("Claude codebase-memory MCP entry failed semantic verification");
            }
        }
        ("claude-code", CodebaseMemoryIntegrationSurface::Hook) => {
            let root: Value = serde_json::from_str(text)?;
            let entries = root
                .pointer("/hooks/SessionStart")
                .and_then(Value::as_array)
                .context("reviewed codebase-memory SessionStart Hook is missing")?;
            let expected = session_start_hook(&plan.launcher_path, false)?;
            let managed = entries
                .iter()
                .filter(|entry| {
                    entry.get("dtoken_managed_id").and_then(Value::as_str) == Some(HOOK_ENTRY_ID)
                })
                .collect::<Vec<_>>();
            if managed != vec![&expected] {
                bail!("codebase-memory SessionStart Hook failed semantic verification");
            }
        }
        _ => bail!("unsupported codebase-memory semantic verification target"),
    }
    Ok(())
}

fn plan_mcp_arguments(plan: &CodebaseMemoryIntegrationPlan) -> Result<Vec<String>> {
    mcp_arguments(
        &plan.upstream_path,
        plan.canonical_project_root
            .as_deref()
            .context("approved project root is missing")?,
        &plan.cache_dir,
    )
}

fn verify_scope_and_launcher(plan: &CodebaseMemoryIntegrationPlan) -> Result<()> {
    let launcher = canonical_existing_file(&plan.launcher_path, "launcher")?;
    if launcher != plan.launcher_path || hash_file(&launcher)? != plan.launcher_sha256 {
        bail!("codebase-memory launcher identity changed after review");
    }
    let upstream = canonical_existing_file(&plan.upstream_path, "upstream executable")?;
    if upstream != plan.upstream_path || hash_file(&upstream)? != plan.upstream_sha256 {
        bail!("codebase-memory upstream identity changed after review");
    }
    let cache = canonical_existing_directory(&plan.cache_dir, "cache directory")?;
    if cache != plan.cache_dir {
        bail!("codebase-memory cache scope changed after review");
    }
    let root = plan
        .canonical_project_root
        .as_deref()
        .context("codebase-memory integration is missing its approved project root")?;
    if canonical_existing_directory(root, "approved project root")? != root {
        bail!("codebase-memory project scope changed after review");
    }
    Ok(())
}

fn restore_applied_change(
    patcher: &ConfigPatcher,
    manifest: &CodebaseMemoryIntegrationTransactionManifest,
) -> Result<()> {
    let Some(rollback_id) = manifest.rollback_id.as_deref() else {
        return Ok(());
    };
    let backup = patcher.load_backup(rollback_id)?;
    if backup.target_path != manifest.plan.target.target_path
        || backup.plan_before_sha256 != manifest.plan.target.before_sha256
        || backup.planned_after_sha256 != manifest.plan.target.after_sha256
    {
        bail!("codebase-memory rollback record is not bound to the reviewed target");
    }
    patcher.restore_backup(
        &backup,
        &ExpectedFileState::Hash(manifest.plan.target.after_sha256.clone()),
    )?;
    Ok(())
}

fn save_manifest(
    backup_root: &Path,
    manifest: &CodebaseMemoryIntegrationTransactionManifest,
) -> Result<()> {
    validate_manifest_structure(manifest, &manifest.transaction_id)?;
    let directory = ensure_manifest_directory(backup_root)?;
    let path = transaction_manifest_path(backup_root, &manifest.transaction_id)?;
    let bytes = serde_json::to_vec_pretty(manifest)?;
    if bytes.len() as u64 > MAX_CONFIG_BYTES {
        bail!("codebase-memory integration manifest exceeds 2 MiB");
    }
    let temporary = directory.join(format!(".dtoken-{}.tmp", Uuid::new_v4()));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&temporary)?;
    let result = (|| -> Result<()> {
        file.write_all(&bytes)?;
        file.sync_all()?;
        drop(file);
        atomic_rename(&temporary, &path)?;
        sync_directory(&directory)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn load_manifest(
    backup_root: &Path,
    transaction_id: &str,
) -> Result<CodebaseMemoryIntegrationTransactionManifest> {
    let path = transaction_manifest_path(backup_root, transaction_id)?;
    ensure_no_symlink_components(&path)?;
    let metadata = fs::symlink_metadata(&path)
        .with_context(|| format!("codebase-memory transaction {transaction_id} is unavailable"))?;
    if !metadata.is_file() || metadata.len() > MAX_CONFIG_BYTES {
        bail!("codebase-memory transaction manifest is not a bounded regular file");
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o077 != 0 {
            bail!("codebase-memory transaction manifest permissions are not private");
        }
    }
    let manifest: CodebaseMemoryIntegrationTransactionManifest =
        serde_json::from_slice(&fs::read(&path)?)?;
    validate_manifest_structure(&manifest, transaction_id)?;
    if let Some(rollback_id) = manifest.rollback_id.as_deref() {
        let patcher = ConfigPatcher::new(backup_root);
        let backup = patcher.load_backup(rollback_id)?;
        if backup.target_path != manifest.plan.target.target_path
            || backup.plan_before_sha256 != manifest.plan.target.before_sha256
            || backup.planned_after_sha256 != manifest.plan.target.after_sha256
        {
            bail!("codebase-memory transaction backup binding is invalid");
        }
    } else if manifest.plan.target.before_sha256 != manifest.plan.target.after_sha256 {
        bail!("changed codebase-memory transaction is missing its rollback record");
    }
    validate_created_directories(&manifest)?;
    Ok(manifest)
}

fn validate_manifest_structure(
    manifest: &CodebaseMemoryIntegrationTransactionManifest,
    transaction_id: &str,
) -> Result<()> {
    validate_transaction_id(transaction_id)?;
    if manifest.schema_version != SCHEMA_VERSION || manifest.transaction_id != transaction_id {
        bail!("codebase-memory transaction manifest identity mismatch");
    }
    validate_plan(&manifest.plan, false)
}

fn transaction_manifest_path(backup_root: &Path, transaction_id: &str) -> Result<PathBuf> {
    validate_transaction_id(transaction_id)?;
    Ok(backup_root
        .join(TRANSACTION_DIRECTORY)
        .join(format!("{transaction_id}.json")))
}

fn validate_transaction_id(transaction_id: &str) -> Result<()> {
    let parsed =
        Uuid::parse_str(transaction_id).context("invalid codebase-memory transaction id")?;
    if parsed.to_string() != transaction_id.to_ascii_lowercase() {
        bail!("codebase-memory transaction id is not canonical");
    }
    Ok(())
}

fn ensure_manifest_directory(backup_root: &Path) -> Result<PathBuf> {
    ensure_private_directory(backup_root)?;
    let directory = backup_root.join(TRANSACTION_DIRECTORY);
    match fs::create_dir(&directory) {
        Ok(()) => {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))?;
            }
            sync_directory(backup_root)?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(error.into()),
    }
    ensure_private_directory(&directory)?;
    Ok(directory)
}

fn ensure_private_directory(path: &Path) -> Result<()> {
    if !path.exists() {
        fs::create_dir(path)
            .with_context(|| format!("failed to create private directory {}", path.display()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
        }
        if let Some(parent) = path.parent() {
            sync_directory(parent)?;
        }
    }
    ensure_no_symlink_components(path)?;
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_dir() {
        bail!("private transaction path is not a directory");
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o077 != 0 {
            bail!("private transaction directory permissions are too broad");
        }
    }
    Ok(())
}

fn create_missing_target_directories(target: &Path) -> Result<Vec<PathBuf>> {
    let mut missing = Vec::new();
    let mut current = target.parent().context("Agent target has no parent")?;
    loop {
        match fs::symlink_metadata(current) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink() || !metadata.is_dir() {
                    bail!("Agent target ancestor is not a real directory");
                }
                break;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                missing.push(current.to_path_buf());
                current = current
                    .parent()
                    .context("Agent target has no existing ancestor")?;
            }
            Err(error) => return Err(error.into()),
        }
    }
    missing.reverse();
    let mut created = Vec::new();
    for directory in missing {
        if let Err(error) = fs::create_dir(&directory) {
            let _ = remove_created_directories(&created);
            return Err(error.into());
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Err(error) = fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)) {
                let _ = fs::remove_dir(&directory);
                let _ = remove_created_directories(&created);
                return Err(error.into());
            }
        }
        created.push(directory);
    }
    Ok(created)
}

fn remove_created_directories(directories: &[PathBuf]) -> Result<()> {
    for directory in directories.iter().rev() {
        match fs::remove_dir(directory) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) if error.kind() == std::io::ErrorKind::DirectoryNotEmpty => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

fn validate_created_directories(
    manifest: &CodebaseMemoryIntegrationTransactionManifest,
) -> Result<()> {
    if manifest.created_directories.len() > 32 {
        bail!("codebase-memory transaction created-directory list is too large");
    }
    let parent = manifest
        .plan
        .target
        .target_path
        .parent()
        .context("Agent target has no parent")?;
    for directory in &manifest.created_directories {
        if directory.parent().is_none()
            || !directory.is_absolute()
            || !parent.starts_with(directory)
        {
            bail!("codebase-memory transaction created directory is not target-bound");
        }
    }
    Ok(())
}

fn read_json_object(path: &Path) -> Result<Map<String, Value>> {
    let text = read_text(path)?;
    if text.trim().is_empty() {
        return Ok(Map::new());
    }
    serde_json::from_str::<Value>(&text)?
        .as_object()
        .cloned()
        .context("Agent JSON configuration must be an object")
}

fn object_field<'a>(
    root: &'a mut Map<String, Value>,
    key: &str,
) -> Result<&'a mut Map<String, Value>> {
    root.entry(key.to_string()).or_insert_with(|| json!({}));
    root.get_mut(key)
        .and_then(Value::as_object_mut)
        .with_context(|| format!("{key} must be an object"))
}

fn read_text(path: &Path) -> Result<String> {
    ensure_no_symlink_components(path)?;
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if !metadata.is_file() || metadata.len() > MAX_CONFIG_BYTES {
                bail!("Agent configuration is not a bounded regular file");
            }
            fs::read_to_string(path)
                .with_context(|| format!("failed to read Agent configuration {}", path.display()))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(error) => Err(error.into()),
    }
}

fn mcp_arguments(upstream: &Path, approved_root: &Path, cache_dir: &Path) -> Result<Vec<String>> {
    Ok(vec![
        "mcp".to_string(),
        "stdio".to_string(),
        path_text(upstream, "upstream executable")?.to_string(),
        path_text(approved_root, "approved project root")?.to_string(),
        path_text(cache_dir, "cache directory")?.to_string(),
    ])
}

fn hook_command(launcher: &Path) -> Result<String> {
    let launcher = path_text(launcher, "launcher")?;
    if launcher
        .chars()
        .any(|character| matches!(character, '"' | '\n' | '\r' | '\0'))
    {
        bail!("launcher path cannot be represented safely in an Agent Hook command");
    }
    #[cfg(unix)]
    if launcher
        .chars()
        .any(|character| matches!(character, '\\' | '$' | '`'))
    {
        bail!("launcher path contains shell-significant characters");
    }
    #[cfg(windows)]
    if launcher
        .chars()
        .any(|character| matches!(character, '%' | '!' | '^'))
    {
        bail!("launcher path contains shell-significant characters");
    }
    Ok(format!("\"{launcher}\" hook session-start"))
}

fn consent_summary(surface: CodebaseMemoryIntegrationSurface) -> &'static str {
    match surface {
        CodebaseMemoryIntegrationSurface::Mcp => {
            "Install one reviewed codebase-memory MCP launcher entry for the selected Agent and approved repository root."
        }
        CodebaseMemoryIntegrationSurface::Hook => {
            "Install an optional SessionStart Hook that emits bounded guidance only; it does not prove MCP availability, indexing, or readiness."
        }
    }
}

fn path_text<'a>(path: &'a Path, label: &str) -> Result<&'a str> {
    path.to_str()
        .with_context(|| format!("{label} path is not valid UTF-8"))
}

fn canonical_existing_file(path: &Path, label: &str) -> Result<PathBuf> {
    if !path.is_absolute() {
        bail!("{label} path must be absolute");
    }
    reject_parent_components(path)?;
    ensure_no_symlink_components(path)?;
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("{label} is unavailable: {}", path.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        bail!("{label} must be a real regular file");
    }
    fs::canonicalize(path).with_context(|| format!("failed to canonicalize {label}"))
}

fn canonical_existing_directory(path: &Path, label: &str) -> Result<PathBuf> {
    if !path.is_absolute() {
        bail!("{label} path must be absolute");
    }
    reject_parent_components(path)?;
    ensure_no_symlink_components(path)?;
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("{label} is unavailable: {}", path.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        bail!("{label} must be a real directory");
    }
    fs::canonicalize(path).with_context(|| format!("failed to canonicalize {label}"))
}

fn ensure_no_symlink_components(path: &Path) -> Result<()> {
    reject_parent_components(path)?;
    let mut current = PathBuf::new();
    for component in path.components() {
        current.push(component.as_os_str());
        // Windows cannot inspect a bare drive root through this API, but every
        // normal component below it must still be checked before use.
        if !matches!(component, Component::Normal(_)) {
            continue;
        }
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                bail!("symbolic links are not allowed in codebase-memory paths")
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

fn reject_parent_components(path: &Path) -> Result<()> {
    if path
        .components()
        .any(|component| component == Component::ParentDir)
    {
        bail!("parent path components are not allowed");
    }
    Ok(())
}

fn validate_sha256(hash: &str) -> Result<()> {
    if hash.len() != 64
        || !hash
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        bail!("expected a lowercase SHA-256 value");
    }
    Ok(())
}

fn hash_file(path: &Path) -> Result<String> {
    let mut file = File::open(path)?;
    let mut digest = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

fn hash_bytes(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn windows_drive_root_is_not_inspected_as_a_symlink_component() {
        let temp_dir = std::env::temp_dir();
        let mut components = temp_dir.components();
        let mut drive_root = PathBuf::new();
        drive_root.push(components.next().expect("drive prefix").as_os_str());
        drive_root.push(components.next().expect("drive root").as_os_str());

        ensure_no_symlink_components(&drive_root.join("dtoken-codebase-memory-test"))
            .expect("a Windows drive root should not be probed as a symlink component");
    }
}
