use dtoken_config_patcher::agent_transport::{
    ensure_owned_opencode_provider, opencode_base_url, opencode_provider_value_models,
    OPENCODE_PROVIDER_ID,
};
use dtoken_config_patcher::{ConfigPatcher, ExpectedFileState};
use dtoken_types::{ConfigToolKind, RiskLevel};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::{
    env, fs,
    io::ErrorKind,
    path::{Component, Path, PathBuf},
};

/// The dedicated desktop flow writes the same provider shape as the managed
/// transport through the Rust-owned canonical builder in
/// `dtoken_config_patcher::agent_transport`, so either flow can verify or
/// clear what the other wrote.

#[derive(Debug, Clone, Deserialize)]
pub struct OpenCodeConnectionRequest {
    pub models: Vec<String>,
    pub listener_port: u16,
    pub reviewed_revision: Option<String>,
    pub reviewed_before_sha256: Option<String>,
    pub reviewed_after_sha256: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct OpenCodeConnectionReview {
    pub ok: bool,
    pub state: String,
    pub source: &'static str,
    pub freshness: &'static str,
    pub target: String,
    pub models: Vec<String>,
    pub listener: String,
    pub transaction: String,
    pub revision: Option<String>,
    pub before_sha256: Option<String>,
    pub after_sha256: Option<String>,
    pub diff: Option<String>,
    pub summary: String,
    pub privacy: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct OpenCodeConnectionReceipt {
    pub ok: bool,
    pub state: String,
    pub source: &'static str,
    pub freshness: &'static str,
    pub target: String,
    pub models: Vec<String>,
    pub listener: String,
    pub transaction: String,
    pub revision: Option<String>,
    pub before_sha256: Option<String>,
    pub after_sha256: Option<String>,
    pub rollback_id: Option<String>,
    pub verified: bool,
    pub summary: String,
    pub privacy: &'static str,
}

pub fn preview(
    request: &OpenCodeConnectionRequest,
    backup_root: &Path,
) -> OpenCodeConnectionReview {
    match prepare(request, backup_root) {
        Ok((path, prepared, models)) => {
            let revision = review_revision(
                &models,
                request.listener_port,
                &prepared.plan.before_hash,
                &prepared.plan.after_hash,
            );
            OpenCodeConnectionReview {
                ok: true,
                state: "review_required".to_string(),
                source: "dtoken-desktop-opencode-adapter",
                freshness: "fresh",
                target: display_path(&path),
                models,
                listener: format!("127.0.0.1:{}", request.listener_port),
                transaction: "scan-plan-diff-backup-confirm-apply-verify-doctor-rollback"
                    .to_string(),
                revision: Some(revision),
                before_sha256: Some(prepared.plan.before_hash.clone()),
                after_sha256: Some(prepared.plan.after_hash.clone()),
                diff: Some(prepared.plan.diff.clone()),
                summary: "Review the exact OpenCode provider change before applying it".to_string(),
                privacy: privacy(),
            }
        }
        Err(error) => failed_review(request, error),
    }
}

pub fn apply(request: &OpenCodeConnectionRequest, backup_root: &Path) -> OpenCodeConnectionReceipt {
    let (path, prepared, models) = match prepare(request, backup_root) {
        Ok(value) => value,
        Err(error) => return failed_receipt(request, error),
    };
    let reviewed_before = match request.reviewed_before_sha256.as_deref() {
        Some(value) => value,
        None => return failed_receipt(request, "missing reviewed before hash".to_string()),
    };
    let reviewed_after = match request.reviewed_after_sha256.as_deref() {
        Some(value) => value,
        None => return failed_receipt(request, "missing reviewed after hash".to_string()),
    };
    let revision = review_revision(
        &models,
        request.listener_port,
        &prepared.plan.before_hash,
        &prepared.plan.after_hash,
    );
    if request.reviewed_revision.as_deref() != Some(revision.as_str())
        || reviewed_before != prepared.plan.before_hash
        || reviewed_after != prepared.plan.after_hash
    {
        return failed_receipt(
            request,
            "OpenCode configuration changed after review; generate a new preview".to_string(),
        );
    }

    let patcher = ConfigPatcher::new(backup_root);
    let backup = match patcher.backup_prepared(&prepared) {
        Ok(value) => value,
        Err(error) => return failed_receipt(request, format!("durable backup failed: {error:#}")),
    };
    let receipt = match patcher.apply_prepared(&prepared, &backup) {
        Ok(value) => value,
        Err(error) => return failed_receipt(request, format!("atomic apply failed: {error:#}")),
    };
    let verified = verify_provider(&path, &models, request.listener_port).is_ok();
    if !verified {
        let expected = ExpectedFileState::Hash(receipt.after.sha256.clone().unwrap_or_default());
        let restored = patcher.restore_backup(&receipt.backup, &expected);
        return failed_receipt(
            request,
            if restored.is_ok() {
                "post-write verification failed; exact original bytes were restored".to_string()
            } else {
                "post-write verification failed and automatic restore needs attention".to_string()
            },
        );
    }

    OpenCodeConnectionReceipt {
        ok: true,
        state: "applied_verified".to_string(),
        source: "dtoken-desktop-opencode-adapter",
        freshness: "fresh",
        target: display_path(&path),
        models,
        listener: format!("127.0.0.1:{}", request.listener_port),
        transaction: "scan-plan-diff-backup-confirm-apply-verify-doctor-rollback".to_string(),
        revision: Some(revision),
        before_sha256: receipt.before.sha256,
        after_sha256: receipt.after.sha256,
        rollback_id: Some(receipt.backup.rollback_id),
        verified: true,
        summary: "OpenCode now has a verified d-token local provider and a durable rollback point"
            .to_string(),
        privacy: privacy(),
    }
}

/// Pure preparation boundary usable by `main.rs` for the compound route +
/// OpenCode transaction: validates the request, discovers and guards the
/// target, and prepares the hash-bound change without writing anything.
/// `main.rs` feeds the returned `PreparedChange` into
/// `ConfigPatcher::apply_transaction` together with the route change, so
/// both files share one durable backup set and one all-or-nothing apply.
pub fn prepare(
    request: &OpenCodeConnectionRequest,
    backup_root: &Path,
) -> Result<(PathBuf, dtoken_config_patcher::PreparedChange, Vec<String>), String> {
    let models = validate_request(request)?;
    let path = discover_config_path()?;
    reject_unsafe_target(&path)?;
    // A missing final opencode.json is the one-click creation case: the
    // product configures an installed OpenCode before it has written any
    // config. `reject_unsafe_target` only admits a safe missing target, and
    // the ConfigPatcher snapshots the missing file so rollback deletes the
    // exact managed-created file. The missing read is planned from the same
    // empty object an existing strict `{}` document parses to, so the
    // created bytes match the managed transport exactly. A non-empty
    // existing file must already be strict JSON; an existing empty file is
    // still rejected by the ConfigPatcher's existing-content validation,
    // which is the fail-closed boundary for anything that is not strict
    // JSON today.
    let bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == ErrorKind::NotFound => Vec::new(),
        Err(error) => return Err(format!("read OpenCode config: {error}")),
    };
    let text = String::from_utf8(bytes).map_err(|_| "OpenCode config is not UTF-8".to_string())?;
    let mut root: Value = if text.trim().is_empty() {
        Value::Object(Map::new())
    } else {
        serde_json::from_str(&text)
            .map_err(|error| format!("OpenCode config is not strict JSON: {error}"))?
    };
    let object = root
        .as_object_mut()
        .ok_or_else(|| "OpenCode config root must be an object".to_string())?;
    patch_provider(object, &models, request.listener_port)?;
    let next = serde_json::to_string_pretty(&root)
        .map_err(|error| format!("serialize OpenCode config: {error}"))?
        + "\n";
    let patcher = ConfigPatcher::new(backup_root);
    let prepared = patcher
        .prepare_change(
            &path,
            &next,
            ConfigToolKind::Dtoken,
            Some("OpenCode".to_string()),
            RiskLevel::Medium,
        )
        .map_err(|error| format!("prepare guarded OpenCode change: {error:#}"))?;
    Ok((path, prepared, models))
}

fn patch_provider(
    root: &mut Map<String, Value>,
    models: &[String],
    port: u16,
) -> Result<(), String> {
    if root
        .get("disabled_providers")
        .and_then(Value::as_array)
        .is_some_and(|items| {
            items
                .iter()
                .any(|item| item.as_str() == Some(OPENCODE_PROVIDER_ID))
        })
    {
        return Err("dtoken-local-route is explicitly disabled in OpenCode".to_string());
    }
    {
        let providers = root
            .entry("provider")
            .or_insert_with(|| Value::Object(Map::new()))
            .as_object_mut()
            .ok_or_else(|| "OpenCode provider must be an object".to_string())?;
        if let Some(existing) = providers.get(OPENCODE_PROVIDER_ID) {
            ensure_owned_opencode_provider(existing).map_err(|_| {
                "OpenCode already contains a dtoken-local-route provider that is not provably d-token-owned; refusing to overwrite a foreign provider".to_string()
            })?;
        }
        let model_refs: Vec<&str> = models.iter().map(String::as_str).collect();
        providers.insert(
            OPENCODE_PROVIDER_ID.to_string(),
            opencode_provider_value_models(&format!("http://127.0.0.1:{port}"), &model_refs)
                .map_err(|error| format!("build canonical OpenCode provider: {error}"))?,
        );
    }
    if let Some(enabled) = root.get_mut("enabled_providers") {
        let enabled = enabled
            .as_array_mut()
            .ok_or_else(|| "OpenCode enabled_providers must be an array".to_string())?;
        if !enabled.iter().all(Value::is_string) {
            return Err("OpenCode enabled_providers must contain only strings".to_string());
        }
        if !enabled
            .iter()
            .any(|item| item.as_str() == Some(OPENCODE_PROVIDER_ID))
        {
            enabled.push(Value::String(OPENCODE_PROVIDER_ID.to_string()));
        }
    }
    Ok(())
}

/// Verification boundary for `main.rs`: the applied OpenCode config must
/// still be provably d-token-owned and must contain the exact reviewed
/// listener and model set before a compound transaction may commit.
pub fn verify_provider(path: &Path, models: &[String], port: u16) -> Result<(), String> {
    let value: Value = serde_json::from_slice(
        &fs::read(path).map_err(|error| format!("read applied config: {error}"))?,
    )
    .map_err(|error| format!("parse applied config: {error}"))?;
    let provider = value
        .pointer(&format!("/provider/{OPENCODE_PROVIDER_ID}"))
        .ok_or_else(|| "applied provider is missing".to_string())?;
    // The same canonical ownership predicate the managed transport uses: an
    // applied provider must still be provably d-token-owned before it passes.
    ensure_owned_opencode_provider(provider)
        .map_err(|error| format!("applied provider ownership check failed: {error:#}"))?;
    let expected_base_url = opencode_base_url(&format!("http://127.0.0.1:{port}"));
    let applied_models: Vec<&str> = provider
        .pointer("/models")
        .and_then(Value::as_object)
        .map(|models| models.keys().map(String::as_str).collect())
        .unwrap_or_default();
    let expected_models: Vec<&str> = models.iter().map(String::as_str).collect();
    if provider.pointer("/options/baseURL").and_then(Value::as_str)
        != Some(expected_base_url.as_str())
        || applied_models.len() != expected_models.len()
        || !expected_models
            .iter()
            .all(|model| applied_models.contains(model))
    {
        return Err("applied provider does not match the reviewed listener/models".to_string());
    }
    Ok(())
}

fn validate_request(request: &OpenCodeConnectionRequest) -> Result<Vec<String>, String> {
    if request.listener_port < 1024 {
        return Err("listener port must be between 1024 and 65535".to_string());
    }
    if request.models.is_empty() {
        return Err("at least one model is required".to_string());
    }
    let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
    let mut normalized = Vec::with_capacity(request.models.len());
    for model in &request.models {
        if model.trim().is_empty() || model.len() > 256 || model.contains(['\n', '\r', '\0']) {
            return Err("model is empty or contains unsupported characters".to_string());
        }
        if seen.insert(model.as_str()) {
            normalized.push(model.clone());
        }
    }
    Ok(normalized)
}

/// Discovery boundary for `main.rs`: the target the adapter guards, so a
/// compound rollback can reject backups that point at an unrelated file.
pub fn discover_config_path() -> Result<PathBuf, String> {
    if let Some(path) = env::var_os("DTOKEN_OPENCODE_CONFIG_PATH") {
        return Ok(PathBuf::from(path));
    }
    if let Some(path) = env::var_os("OPENCODE_CONFIG") {
        return Ok(PathBuf::from(path));
    }
    let home = env::var_os("HOME").ok_or_else(|| "HOME is unavailable".to_string())?;
    let root = env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(home).join(".config"));
    let json = root.join("opencode").join("opencode.json");
    if !json.exists() && root.join("opencode").join("opencode.jsonc").exists() {
        return Err(
            "OpenCode uses JSONC; guarded product writes require strict opencode.json".to_string(),
        );
    }
    Ok(json)
}

fn reject_unsafe_target(path: &Path) -> Result<(), String> {
    if path.file_name().and_then(|name| name.to_str()) == Some("auth.json") {
        return Err("auth.json is never a valid configuration target".to_string());
    }
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                return Err(
                    "OpenCode config must be an existing regular non-symlink file".to_string(),
                );
            }
            Ok(())
        }
        Err(error) if error.kind() == ErrorKind::NotFound => reject_missing_target(path),
        Err(error) => Err(format!("inspect OpenCode config: {error}")),
    }
}

/// One-click creation guard for an absent final `opencode.json`. The missing
/// target is admitted only when the file name is exactly the strict-JSON
/// `opencode.json` (never auth.json, jsonc, or any other name), the path has
/// no parent-traversal components, every existing path component is a real
/// non-symlink directory, and the parent directory already exists. Parent
/// directories are never created implicitly: if the OpenCode config
/// directory is absent (for example before OpenCode has been installed or
/// run once), the operation fails truthfully instead of guessing where to
/// create it. The ConfigPatcher then snapshots the missing file, creates it
/// through the prepared change, and rollback deletes the exact
/// managed-created file.
fn reject_missing_target(path: &Path) -> Result<(), String> {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "OpenCode config target must end in opencode.json".to_string())?;
    if file_name != "opencode.json" {
        return Err(format!(
            "missing OpenCode target {file_name} is not the strict opencode.json that d-token may create"
        ));
    }
    if path
        .components()
        .any(|component| component == Component::ParentDir)
    {
        return Err(
            "parent-directory components are not allowed in configuration paths".to_string(),
        );
    }
    ensure_no_symlink_components(path)?;
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let metadata = match fs::symlink_metadata(parent) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == ErrorKind::NotFound => {
            return Err(format!(
                "OpenCode config directory {} does not exist; d-token never creates directories implicitly — install OpenCode (it creates the config directory) or create it first",
                parent.display()
            ));
        }
        Err(error) => {
            return Err(format!(
                "failed to inspect OpenCode config directory {}: {error}",
                parent.display()
            ));
        }
    };
    if !metadata.is_dir() {
        return Err(format!(
            "OpenCode config parent {} is not a directory",
            parent.display()
        ));
    }
    Ok(())
}

/// Walks every path component and rejects any existing component that is a
/// symbolic link. Missing components are skipped: the final missing target
/// itself is the creation case, and deeper absent directories are rejected
/// by the parent-existence check in `reject_missing_target`.
fn ensure_no_symlink_components(path: &Path) -> Result<(), String> {
    let mut current = PathBuf::new();
    for component in path.components() {
        current.push(component.as_os_str());
        if !matches!(component, Component::Normal(_)) {
            continue;
        }
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err("symbolic links are not allowed in configuration paths".to_string());
            }
            Ok(_) => {}
            Err(error) if error.kind() == ErrorKind::NotFound => continue,
            Err(error) => {
                return Err(format!(
                    "failed to inspect path {}: {error}",
                    current.display()
                ));
            }
        }
    }
    Ok(())
}

/// Display boundary for `main.rs`: a home-relative label for the guarded
/// target so receipts never leak absolute user paths.
pub fn display_path(path: &Path) -> String {
    if let Some(home) = env::var_os("HOME").map(PathBuf::from) {
        if let Ok(relative) = path.strip_prefix(home) {
            return format!("~/{}", relative.display());
        }
    }
    format!(
        "<user-config>/{}",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("opencode.json")
    )
}

pub fn privacy() -> &'static str {
    "Normal strict JSON config only; auth.json, credentials, prompts, and responses are never read or returned"
}

/// Revision boundary for `main.rs`: the deterministic OpenCode-side digest
/// that the compound revision folds in, binding models, listener, and both
/// hashes into one review token.
pub fn review_revision(models: &[String], listener_port: u16, before: &str, after: &str) -> String {
    dtoken_config_patcher::stable_hash(&format!(
        "opencode-v1\n{}\n{}\n{}\n{}",
        models.join("\n"),
        listener_port,
        before,
        after
    ))
}

fn failed_review(request: &OpenCodeConnectionRequest, summary: String) -> OpenCodeConnectionReview {
    OpenCodeConnectionReview {
        ok: false,
        state: "unavailable".to_string(),
        source: "dtoken-desktop-opencode-adapter",
        freshness: "fresh",
        target: "<normal-opencode-config>".to_string(),
        models: request.models.clone(),
        listener: format!("127.0.0.1:{}", request.listener_port),
        transaction: "scan-plan-diff-backup-confirm-apply-verify-doctor-rollback".to_string(),
        revision: None,
        before_sha256: None,
        after_sha256: None,
        diff: None,
        summary,
        privacy: privacy(),
    }
}

fn failed_receipt(
    request: &OpenCodeConnectionRequest,
    summary: String,
) -> OpenCodeConnectionReceipt {
    OpenCodeConnectionReceipt {
        ok: false,
        state: "failed".to_string(),
        source: "dtoken-desktop-opencode-adapter",
        freshness: "fresh",
        target: "<normal-opencode-config>".to_string(),
        models: request.models.clone(),
        listener: format!("127.0.0.1:{}", request.listener_port),
        transaction: "scan-plan-diff-backup-confirm-apply-verify-doctor-rollback".to_string(),
        revision: request.reviewed_revision.clone(),
        before_sha256: None,
        after_sha256: None,
        rollback_id: None,
        verified: false,
        summary,
        privacy: privacy(),
    }
}

/// Shared guard for tests that mutate `DTOKEN_OPENCODE_CONFIG_PATH`.
/// `main.rs` compound-transaction tests acquire the same lock so adapter
/// env tests and compound tests never race on the env var in one binary.
#[cfg(test)]
pub(crate) fn config_env_lock() -> &'static std::sync::Mutex<()> {
    static LOCK: std::sync::LazyLock<std::sync::Mutex<()>> =
        std::sync::LazyLock::new(|| std::sync::Mutex::new(()));
    &LOCK
}

#[cfg(test)]
mod tests {
    use super::*;
    use dtoken_config_patcher::agent_transport::{
        clear_agent_transport_document, configure_opencode_transport_document_models,
    };
    use tempfile::tempdir;

    fn env_lock() -> std::sync::MutexGuard<'static, ()> {
        config_env_lock()
            .lock()
            .unwrap_or_else(|error| error.into_inner())
    }

    fn request() -> OpenCodeConnectionRequest {
        OpenCodeConnectionRequest {
            models: vec!["deepseek/model-v1".to_string()],
            listener_port: 18001,
            reviewed_revision: None,
            reviewed_before_sha256: None,
            reviewed_after_sha256: None,
        }
    }

    fn request_two_models() -> OpenCodeConnectionRequest {
        OpenCodeConnectionRequest {
            models: vec![
                "deepseek/model-v1".to_string(),
                "opencode-coder".to_string(),
            ],
            listener_port: 18001,
            reviewed_revision: None,
            reviewed_before_sha256: None,
            reviewed_after_sha256: None,
        }
    }

    #[test]
    fn preview_apply_and_exact_rollback_are_hash_bound() {
        let _guard = env_lock();
        let dir = tempdir().expect("temp dir");
        let root = fs::canonicalize(dir.path()).expect("canonical temp dir");
        let config = dir.path().join("opencode.json");
        let original = b"{\"provider\":{\"existing\":{\"name\":\"Existing\"}},\"enabled_providers\":[\"existing\"]}\n";
        fs::write(&config, original).expect("write config");
        env::set_var(
            "DTOKEN_OPENCODE_CONFIG_PATH",
            fs::canonicalize(&config).expect("canonical config"),
        );

        let review = preview(&request(), &root.join("backups"));
        assert!(review.ok, "{}", review.summary);
        assert!(!review.diff.as_deref().unwrap_or_default().contains("auth"));
        let mut apply_request = request();
        apply_request.reviewed_before_sha256 = review.before_sha256;
        apply_request.reviewed_after_sha256 = review.after_sha256;
        apply_request.reviewed_revision = review.revision;
        let receipt = apply(&apply_request, &root.join("backups"));
        assert!(receipt.ok, "{}", receipt.summary);
        let patcher = ConfigPatcher::new(root.join("backups"));
        let backup = patcher
            .load_backup(receipt.rollback_id.as_deref().expect("rollback id"))
            .expect("load backup");
        patcher
            .restore_backup(
                &backup,
                &ExpectedFileState::Hash(receipt.after_sha256.expect("after hash")),
            )
            .expect("restore");
        assert_eq!(fs::read(&config).expect("read restored"), original);
        env::remove_var("DTOKEN_OPENCODE_CONFIG_PATH");
    }

    #[test]
    fn stale_review_is_rejected_without_overwriting_concurrent_edit() {
        let _guard = env_lock();
        let dir = tempdir().expect("temp dir");
        let root = fs::canonicalize(dir.path()).expect("canonical temp dir");
        let config = dir.path().join("opencode.json");
        fs::write(&config, "{\"provider\":{}}\n").expect("write config");
        env::set_var(
            "DTOKEN_OPENCODE_CONFIG_PATH",
            fs::canonicalize(&config).expect("canonical config"),
        );
        let review = preview(&request(), &root.join("backups"));
        fs::write(&config, "{\"provider\":{},\"theme\":\"system\"}\n").expect("concurrent edit");
        let concurrent = fs::read(&config).expect("concurrent bytes");
        let mut apply_request = request();
        apply_request.reviewed_before_sha256 = review.before_sha256;
        apply_request.reviewed_after_sha256 = review.after_sha256;
        apply_request.reviewed_revision = review.revision;
        let receipt = apply(&apply_request, &root.join("backups"));
        assert!(!receipt.ok);
        assert_eq!(fs::read(&config).expect("read current"), concurrent);
        env::remove_var("DTOKEN_OPENCODE_CONFIG_PATH");
    }

    /// End-to-end fixture: the dedicated desktop flow applies the provider, then
    /// the managed transport clears it. User providers and their credentials are
    /// preserved, the second clear is a no-op, and a user edit to the owned
    /// provider makes the managed clear fail closed.
    #[test]
    fn dedicated_apply_then_managed_clear_preserves_user_providers_and_is_idempotent() {
        let _guard = env_lock();
        let dir = tempdir().expect("temp dir");
        let root = fs::canonicalize(dir.path()).expect("canonical temp dir");
        let config = dir.path().join("opencode.json");
        let original = b"{\"provider\":{\"existing\":{\"name\":\"Existing\",\"options\":{\"apiKey\":\"user-credential-placeholder\"}}},\"enabled_providers\":[\"existing\"]}\n";
        fs::write(&config, original).expect("write config");
        env::set_var(
            "DTOKEN_OPENCODE_CONFIG_PATH",
            fs::canonicalize(&config).expect("canonical config"),
        );

        // Dedicated flow writes the provider through the canonical builder.
        let review = preview(&request(), &root.join("backups"));
        assert!(review.ok, "{}", review.summary);
        let mut apply_request = request();
        apply_request.reviewed_before_sha256 = review.before_sha256;
        apply_request.reviewed_after_sha256 = review.after_sha256;
        apply_request.reviewed_revision = review.revision;
        let receipt = apply(&apply_request, &root.join("backups"));
        assert!(receipt.ok, "{}", receipt.summary);
        let request = request();
        verify_provider(&config, &request.models, request.listener_port)
            .expect("dedicated verify after dedicated apply");

        // The managed transport recognizes and clears the same ownership evidence.
        let applied = fs::read_to_string(&config).expect("read applied config");
        let cleared = clear_agent_transport_document("opencode", &applied).expect("managed clear");
        let cleared_value: Value = serde_json::from_str(&cleared).expect("cleared JSON");
        assert!(
            cleared_value.pointer("/provider/existing").is_some(),
            "user provider must be preserved"
        );
        assert_eq!(
            cleared_value
                .pointer("/provider/existing/options/apiKey")
                .and_then(Value::as_str),
            Some("user-credential-placeholder"),
            "user credential must be preserved"
        );
        assert!(
            cleared_value
                .pointer("/provider/dtoken-local-route")
                .is_none(),
            "owned dtoken provider must be removed"
        );
        let enabled = cleared_value
            .get("enabled_providers")
            .and_then(Value::as_array)
            .expect("enabled_providers array");
        assert!(enabled
            .iter()
            .any(|entry| entry.as_str() == Some("existing")));
        assert!(
            !enabled
                .iter()
                .any(|entry| entry.as_str() == Some("dtoken-local-route")),
            "owned entry must leave the allowlist"
        );

        // Clear is idempotent when no d-token transport is present.
        let cleared_again =
            clear_agent_transport_document("opencode", &cleared).expect("idempotent clear");
        assert_eq!(cleared_again, cleared);

        // A user edit to the owned provider fails closed instead of deleting it.
        let mut edited: Value = serde_json::from_str(&applied).expect("applied JSON");
        *edited
            .pointer_mut("/provider/dtoken-local-route/options/apiKey")
            .expect("owned apiKey") = Value::String("user-changed".to_string());
        let edited_text = serde_json::to_string_pretty(&edited).expect("serialize edit") + "\n";
        let error = clear_agent_transport_document("opencode", &edited_text)
            .expect_err("clear must fail closed on user-edited provider");
        assert!(
            error.to_string().contains("not provably d-token-owned"),
            "unexpected error: {error}"
        );
        env::remove_var("DTOKEN_OPENCODE_CONFIG_PATH");
    }

    /// End-to-end fixture: the managed transport configures the provider, the
    /// dedicated desktop flow verifies it, and the managed clear restores the
    /// user-owned providers and credentials untouched.
    #[test]
    fn managed_configure_then_dedicated_verify_and_managed_clear_preserve_user_providers() {
        let original = "{\"provider\":{\"existing\":{\"name\":\"Existing\",\"options\":{\"apiKey\":\"user-credential-placeholder\"}}},\"enabled_providers\":[\"existing\"]}\n";
        let configured = configure_opencode_transport_document_models(
            original,
            "http://127.0.0.1:18001",
            &["deepseek/model-v1"],
            true,
        )
        .expect("managed configure");
        let configured_value: Value = serde_json::from_str(&configured).expect("configured JSON");
        ensure_owned_opencode_provider(
            configured_value
                .pointer("/provider/dtoken-local-route")
                .expect("managed provider"),
        )
        .expect("canonical ownership predicate accepts managed configure");

        let dir = tempdir().expect("temp dir");
        let config = dir.path().join("opencode.json");
        fs::write(&config, configured).expect("write configured config");
        let models = vec!["deepseek/model-v1".to_string()];
        verify_provider(&config, &models, 18001)
            .expect("dedicated verify accepts managed configure");

        let current = fs::read_to_string(&config).expect("read config");
        let cleared = clear_agent_transport_document("opencode", &current).expect("managed clear");
        let cleared_value: Value = serde_json::from_str(&cleared).expect("cleared JSON");
        assert!(
            cleared_value.pointer("/provider/existing").is_some(),
            "user provider must be preserved"
        );
        assert_eq!(
            cleared_value
                .pointer("/provider/existing/options/apiKey")
                .and_then(Value::as_str),
            Some("user-credential-placeholder"),
            "user credential must be preserved"
        );
        assert!(
            cleared_value
                .pointer("/provider/dtoken-local-route")
                .is_none(),
            "owned dtoken provider must be removed"
        );
        let enabled = cleared_value
            .get("enabled_providers")
            .and_then(Value::as_array)
            .expect("enabled_providers array");
        assert!(enabled
            .iter()
            .any(|entry| entry.as_str() == Some("existing")));
        assert!(
            !enabled
                .iter()
                .any(|entry| entry.as_str() == Some("dtoken-local-route")),
            "owned entry must leave the allowlist"
        );
    }

    /// Byte-level lock: for the same input document, endpoint, and model list,
    /// the dedicated desktop flow and the managed transport must serialize the
    /// exact same bytes. Byte drift would let one path rewrite what the other
    /// wrote, or fail mutual verification of the provider shape.
    #[test]
    fn dedicated_apply_and_managed_configure_write_byte_identical_provider_json() {
        let _guard = env_lock();
        let dir = tempdir().expect("temp dir");
        let root = fs::canonicalize(dir.path()).expect("canonical temp dir");
        let config = dir.path().join("opencode.json");
        let original = b"{\"provider\":{\"existing\":{\"name\":\"Existing\",\"options\":{\"apiKey\":\"user-credential-placeholder\"}}},\"enabled_providers\":[\"existing\"]}\n";
        fs::write(&config, original).expect("write config");
        env::set_var(
            "DTOKEN_OPENCODE_CONFIG_PATH",
            fs::canonicalize(&config).expect("canonical config"),
        );

        let managed = configure_opencode_transport_document_models(
            std::str::from_utf8(original).expect("original is UTF-8"),
            "http://127.0.0.1:18001",
            &["deepseek/model-v1"],
            true,
        )
        .expect("managed configure");

        let review = preview(&request(), &root.join("backups"));
        assert!(review.ok, "{}", review.summary);
        let mut apply_request = request();
        apply_request.reviewed_before_sha256 = review.before_sha256;
        apply_request.reviewed_after_sha256 = review.after_sha256;
        apply_request.reviewed_revision = review.revision;
        let receipt = apply(&apply_request, &root.join("backups"));
        assert!(receipt.ok, "{}", receipt.summary);
        let applied = fs::read_to_string(&config).expect("read applied config");

        assert_eq!(
            applied, managed,
            "dedicated and managed OpenCode writes must be byte-identical"
        );
        env::remove_var("DTOKEN_OPENCODE_CONFIG_PATH");
    }

    /// Semantic lock: a provider written by the dedicated desktop flow stays
    /// owned, so the managed transport may reconfigure it without touching user
    /// providers, and the dedicated flow still recognizes the result. Because
    /// the dedicated flow now accepts any provably owned provider, it can also
    /// reconfigure the managed-written provider back to its own binding.
    #[test]
    fn dedicated_apply_then_managed_reconfigure_stays_mutually_recognized() {
        let _guard = env_lock();
        let dir = tempdir().expect("temp dir");
        let root = fs::canonicalize(dir.path()).expect("canonical temp dir");
        let config = dir.path().join("opencode.json");
        let original = "{\"provider\":{\"existing\":{\"name\":\"Existing\",\"options\":{\"apiKey\":\"user-credential-placeholder\"}}},\"enabled_providers\":[\"existing\"]}\n";
        fs::write(&config, original).expect("write config");
        env::set_var(
            "DTOKEN_OPENCODE_CONFIG_PATH",
            fs::canonicalize(&config).expect("canonical config"),
        );

        let review = preview(&request(), &root.join("backups"));
        assert!(review.ok, "{}", review.summary);
        let mut apply_request = request();
        apply_request.reviewed_before_sha256 = review.before_sha256;
        apply_request.reviewed_after_sha256 = review.after_sha256;
        apply_request.reviewed_revision = review.revision;
        let receipt = apply(&apply_request, &root.join("backups"));
        assert!(receipt.ok, "{}", receipt.summary);
        let applied = fs::read_to_string(&config).expect("read applied config");

        // The managed transport may reconfigure the dedicated-owned provider.
        let reconfigured = configure_opencode_transport_document_models(
            &applied,
            "http://127.0.0.1:18002",
            &["deepseek/model-v2"],
            true,
        )
        .expect("managed reconfigure of dedicated-owned provider");
        let reconfigured_value: Value =
            serde_json::from_str(&reconfigured).expect("reconfigured JSON");
        assert!(
            reconfigured_value.pointer("/provider/existing").is_some(),
            "user provider must be preserved across managed reconfigure"
        );
        fs::write(&config, reconfigured).expect("write reconfigured config");
        let managed_models = vec!["deepseek/model-v2".to_string()];
        verify_provider(&config, &managed_models, 18002)
            .expect("dedicated verify recognizes managed reconfigure");

        // The dedicated flow accepts the owned provider and reconfigures it
        // back to its own binding idempotently.
        let again = preview(&request(), &root.join("backups"));
        assert!(again.ok, "{}", again.summary);
        let mut again_request = request();
        again_request.reviewed_before_sha256 = again.before_sha256;
        again_request.reviewed_after_sha256 = again.after_sha256;
        again_request.reviewed_revision = again.revision;
        let again_receipt = apply(&again_request, &root.join("backups"));
        assert!(again_receipt.ok, "{}", again_receipt.summary);
        verify_provider(&config, &request().models, 18001)
            .expect("dedicated reconfigure restores the dedicated binding");
        env::remove_var("DTOKEN_OPENCODE_CONFIG_PATH");
    }

    /// Two-model fixture: the review and receipt expose the normalized full
    /// list, apply writes both canonical model entries, and verify requires
    /// the exact model set before passing.
    #[test]
    fn two_models_review_apply_and_verify_round_trip() {
        let _guard = env_lock();
        let dir = tempdir().expect("temp dir");
        let root = fs::canonicalize(dir.path()).expect("canonical temp dir");
        let config = dir.path().join("opencode.json");
        let original = b"{\"provider\":{\"existing\":{\"name\":\"Existing\"}},\"enabled_providers\":[\"existing\"]}\n";
        fs::write(&config, original).expect("write config");
        env::set_var(
            "DTOKEN_OPENCODE_CONFIG_PATH",
            fs::canonicalize(&config).expect("canonical config"),
        );

        let review = preview(&request_two_models(), &root.join("backups"));
        assert!(review.ok, "{}", review.summary);
        assert_eq!(
            review.models,
            vec![
                "deepseek/model-v1".to_string(),
                "opencode-coder".to_string()
            ],
            "review must expose the full normalized model list"
        );
        let mut apply_request = request_two_models();
        apply_request.reviewed_before_sha256 = review.before_sha256;
        apply_request.reviewed_after_sha256 = review.after_sha256;
        apply_request.reviewed_revision = review.revision;
        let receipt = apply(&apply_request, &root.join("backups"));
        assert!(receipt.ok, "{}", receipt.summary);
        assert_eq!(
            receipt.models,
            vec![
                "deepseek/model-v1".to_string(),
                "opencode-coder".to_string()
            ],
            "receipt must expose the full normalized model list"
        );

        let applied: Value =
            serde_json::from_str(&fs::read_to_string(&config).expect("read applied config"))
                .expect("applied JSON");
        let applied_models = applied
            .pointer("/provider/dtoken-local-route/models")
            .and_then(Value::as_object)
            .expect("applied models object");
        assert_eq!(applied_models.len(), 2, "both models must be present");
        assert_eq!(
            applied_models
                .get("deepseek/model-v1")
                .and_then(|value| value.get("name"))
                .and_then(Value::as_str),
            Some("d-token routed deepseek/model-v1"),
            "canonical model definition"
        );
        assert_eq!(
            applied_models
                .get("opencode-coder")
                .and_then(|value| value.get("name"))
                .and_then(Value::as_str),
            Some("d-token routed opencode-coder"),
            "canonical model definition"
        );

        verify_provider(&config, &request_two_models().models, 18001)
            .expect("verify accepts the exact two-model set");
        let partial = vec!["deepseek/model-v1".to_string()];
        assert!(
            verify_provider(&config, &partial, 18001).is_err(),
            "verify must fail when a reviewed model is missing"
        );
        let extra = vec![
            "deepseek/model-v1".to_string(),
            "opencode-coder".to_string(),
            "extra/model".to_string(),
        ];
        assert!(
            verify_provider(&config, &extra, 18001).is_err(),
            "verify must fail when an unreviewed model is present"
        );
        env::remove_var("DTOKEN_OPENCODE_CONFIG_PATH");
    }

    /// Idempotent reconfigure: re-applying the same request over the owned
    /// provider succeeds and rewrites byte-identical config, because the
    /// canonical multi-model builder replaces the owned provider deterministically.
    #[test]
    fn owned_reconfigure_is_idempotent_and_byte_stable() {
        let _guard = env_lock();
        let dir = tempdir().expect("temp dir");
        let root = fs::canonicalize(dir.path()).expect("canonical temp dir");
        let config = dir.path().join("opencode.json");
        let original = b"{\"provider\":{\"existing\":{\"name\":\"Existing\"}},\"enabled_providers\":[\"existing\"]}\n";
        fs::write(&config, original).expect("write config");
        env::set_var(
            "DTOKEN_OPENCODE_CONFIG_PATH",
            fs::canonicalize(&config).expect("canonical config"),
        );

        let first = request_two_models();
        let review = preview(&first, &root.join("backups"));
        assert!(review.ok, "{}", review.summary);
        let mut apply_request = first;
        apply_request.reviewed_before_sha256 = review.before_sha256;
        apply_request.reviewed_after_sha256 = review.after_sha256;
        apply_request.reviewed_revision = review.revision;
        let receipt = apply(&apply_request, &root.join("backups"));
        assert!(receipt.ok, "{}", receipt.summary);
        let applied_once = fs::read(&config).expect("read applied config");

        // A second preview over the owned provider is a successful reconfigure
        // plan, and applying it rewrites the exact same bytes.
        let review_again = preview(&request_two_models(), &root.join("backups"));
        assert!(review_again.ok, "{}", review_again.summary);
        let mut again_request = request_two_models();
        again_request.reviewed_before_sha256 = review_again.before_sha256;
        again_request.reviewed_after_sha256 = review_again.after_sha256;
        again_request.reviewed_revision = review_again.revision;
        let receipt_again = apply(&again_request, &root.join("backups"));
        assert!(receipt_again.ok, "{}", receipt_again.summary);
        assert_eq!(
            fs::read(&config).expect("read reapplied config"),
            applied_once,
            "idempotent reconfigure must be byte-identical"
        );
        verify_provider(&config, &request_two_models().models, 18001)
            .expect("verify after idempotent reconfigure");
        env::remove_var("DTOKEN_OPENCODE_CONFIG_PATH");
    }

    /// Foreign collision: a provider under the reserved d-token id that is not
    /// provably owned must fail closed without touching the user's config.
    #[test]
    fn foreign_provider_collision_fails_closed_without_touching_config() {
        let _guard = env_lock();
        let dir = tempdir().expect("temp dir");
        let root = fs::canonicalize(dir.path()).expect("canonical temp dir");
        let config = dir.path().join("opencode.json");
        let foreign = b"{\"provider\":{\"dtoken-local-route\":{\"npm\":\"@impostor/opencode\",\"name\":\"Impostor\",\"options\":{\"baseURL\":\"http://127.0.0.1:9999/v1\",\"apiKey\":\"env:IMPOSTOR\"},\"models\":{\"impostor/model\":{\"name\":\"Impostor model\"}}}},\"enabled_providers\":[\"dtoken-local-route\"]}\n";
        fs::write(&config, foreign).expect("write config");
        env::set_var(
            "DTOKEN_OPENCODE_CONFIG_PATH",
            fs::canonicalize(&config).expect("canonical config"),
        );
        let before = fs::read(&config).expect("read before");

        let review = preview(&request(), &root.join("backups"));
        assert!(!review.ok, "foreign collision must fail closed");
        assert!(
            review.summary.contains("not provably d-token-owned"),
            "unexpected refusal summary: {}",
            review.summary
        );
        assert_eq!(
            fs::read(&config).expect("read after"),
            before,
            "foreign provider config must be untouched"
        );
        env::remove_var("DTOKEN_OPENCODE_CONFIG_PATH");
    }

    /// Validation lock: duplicate models are deduplicated in stable
    /// first-occurrence order, an empty list fails, and control characters in
    /// any model fail.
    #[test]
    fn duplicate_models_are_deduped_and_invalid_lists_fail() {
        let mut duplicate = request();
        duplicate.models = vec![
            "deepseek/model-v1".to_string(),
            "opencode-coder".to_string(),
            "deepseek/model-v1".to_string(),
        ];
        let normalized = validate_request(&duplicate).expect("validate request");
        assert_eq!(
            normalized,
            vec![
                "deepseek/model-v1".to_string(),
                "opencode-coder".to_string()
            ],
            "stable first-occurrence dedupe"
        );

        let mut empty = request();
        empty.models = Vec::new();
        let error = validate_request(&empty).expect_err("empty model list must fail");
        assert!(
            error.contains("at least one model"),
            "unexpected error: {error}"
        );

        let mut invalid = request();
        invalid.models = vec!["deepseek/model-v1".to_string(), "bad\nmodel".to_string()];
        let error = validate_request(&invalid).expect_err("control characters must fail");
        assert!(
            error.contains("unsupported characters"),
            "unexpected error: {error}"
        );

        let mut oversized = request();
        oversized.models = vec!["x".repeat(257)];
        let error = validate_request(&oversized).expect_err("oversized model must fail");
        assert!(
            error.contains("unsupported characters"),
            "unexpected error: {error}"
        );
    }

    /// One-click creation fixture: with no opencode.json present but a real
    /// parent directory, preview plans the creation from the empty-object
    /// snapshot, apply writes the exact canonical owned provider (byte
    /// identical to the managed transport configuring an empty document),
    /// and the receipt rollback deletes the exact managed-created file,
    /// restoring the missing state.
    #[test]
    fn missing_config_preview_apply_and_rollback_restores_missing() {
        let _guard = env_lock();
        let dir = tempdir().expect("temp dir");
        let root = fs::canonicalize(dir.path()).expect("canonical temp dir");
        let config_dir = dir.path().join("opencode");
        fs::create_dir(&config_dir).expect("create OpenCode config directory");
        let config = config_dir.join("opencode.json");
        assert!(!config.exists(), "fixture must start without a config file");
        env::set_var(
            "DTOKEN_OPENCODE_CONFIG_PATH",
            fs::canonicalize(&config_dir)
                .expect("canonical config directory")
                .join("opencode.json"),
        );

        let review = preview(&request(), &root.join("backups"));
        assert!(review.ok, "{}", review.summary);
        assert_eq!(
            review.before_sha256.as_deref(),
            Some(dtoken_config_patcher::stable_hash("").as_str()),
            "a missing config plans from the empty-object snapshot hash"
        );
        assert!(
            review
                .diff
                .as_deref()
                .unwrap_or_default()
                .contains("dtoken-local-route"),
            "the review diff must show the provider to be created"
        );
        assert!(!config.exists(), "preview must not write");

        let mut apply_request = request();
        apply_request.reviewed_before_sha256 = review.before_sha256;
        apply_request.reviewed_after_sha256 = review.after_sha256;
        apply_request.reviewed_revision = review.revision;
        let receipt = apply(&apply_request, &root.join("backups"));
        assert!(receipt.ok, "{}", receipt.summary);
        let applied = fs::read_to_string(&config).expect("read created config");

        let managed = configure_opencode_transport_document_models(
            "{}",
            "http://127.0.0.1:18001",
            &["deepseek/model-v1"],
            true,
        )
        .expect("managed configure of an empty document");
        assert_eq!(
            applied, managed,
            "created config must be byte-identical to the managed configure of an empty document"
        );
        verify_provider(&config, &request().models, request().listener_port)
            .expect("verify the created provider");

        let patcher = ConfigPatcher::new(root.join("backups"));
        let backup = patcher
            .load_backup(receipt.rollback_id.as_deref().expect("rollback id"))
            .expect("load backup");
        assert!(
            !backup.original.exists,
            "backup manifest must record the original as missing"
        );
        patcher
            .restore_backup(
                &backup,
                &ExpectedFileState::Hash(receipt.after_sha256.expect("after hash")),
            )
            .expect("restore");
        assert!(
            !config.exists(),
            "rollback must delete the exact managed-created file, restoring missing"
        );
        env::remove_var("DTOKEN_OPENCODE_CONFIG_PATH");
    }

    /// Default-path fixture: the guarded default
    /// `$XDG_CONFIG_HOME/opencode/opencode.json` is created when only the
    /// directory exists, and a missing config directory fails truthfully
    /// instead of being created implicitly.
    #[test]
    fn default_path_creation_and_absent_parent_fails_truthfully() {
        let _guard = env_lock();
        let dir = tempdir().expect("temp dir");
        let root = fs::canonicalize(dir.path()).expect("canonical temp dir");
        let previous_xdg = env::var_os("XDG_CONFIG_HOME");
        env::set_var("XDG_CONFIG_HOME", &root);
        let opencode_dir = root.join("opencode");
        let config = opencode_dir.join("opencode.json");

        // No config directory yet: preview fails truthfully and creates
        // nothing. OpenCode's own installation is expected to create the
        // directory; d-token never creates parent directories implicitly.
        let review = preview(&request(), &root.join("backups"));
        assert!(!review.ok, "absent parent must fail closed");
        assert!(
            review.summary.contains("does not exist"),
            "unexpected refusal summary: {}",
            review.summary
        );
        assert!(!config.exists());

        // Once the directory exists (as OpenCode's install leaves it), the
        // one-click flow may create the missing strict JSON config.
        fs::create_dir(&opencode_dir).expect("create OpenCode config directory");
        let review = preview(&request(), &root.join("backups"));
        assert!(review.ok, "{}", review.summary);
        let mut apply_request = request();
        apply_request.reviewed_before_sha256 = review.before_sha256;
        apply_request.reviewed_after_sha256 = review.after_sha256;
        apply_request.reviewed_revision = review.revision;
        let receipt = apply(&apply_request, &root.join("backups"));
        assert!(receipt.ok, "{}", receipt.summary);
        assert!(config.is_file(), "default-path config must be created");
        verify_provider(&config, &request().models, request().listener_port)
            .expect("verify the default-path created provider");

        match previous_xdg {
            Some(value) => env::set_var("XDG_CONFIG_HOME", value),
            None => env::remove_var("XDG_CONFIG_HOME"),
        }
    }

    /// Path-safety lock: a symlinked final target, a symlinked parent with a
    /// missing final target, a missing auth.json, and parent-traversal
    /// components all fail closed without touching anything.
    #[cfg(unix)]
    #[test]
    fn symlinked_parent_and_final_target_and_unsafe_names_are_rejected() {
        use std::os::unix::fs::symlink;

        let _guard = env_lock();
        let dir = tempdir().expect("temp dir");
        let root = fs::canonicalize(dir.path()).expect("canonical temp dir");
        let real_dir = dir.path().join("real-config");
        fs::create_dir(&real_dir).expect("create real directory");
        let outside = real_dir.join("outside.json");
        fs::write(&outside, "{\"provider\":{}}\n").expect("write outside config");

        // Symlinked final target: an existing symlink is rejected.
        let symlink_config = root.join("opencode-symlink.json");
        symlink(&outside, &symlink_config).expect("symlink final target");
        env::set_var("DTOKEN_OPENCODE_CONFIG_PATH", &symlink_config);
        let review = preview(&request(), &root.join("backups"));
        assert!(!review.ok, "symlinked final target must fail closed");
        assert!(
            review.summary.contains("non-symlink"),
            "unexpected refusal summary: {}",
            review.summary
        );
        env::remove_var("DTOKEN_OPENCODE_CONFIG_PATH");

        // Symlinked parent with a missing final target: creation is refused.
        let parent_symlink = root.join("opencode-parent-symlink");
        symlink(&real_dir, &parent_symlink).expect("symlink parent");
        env::set_var(
            "DTOKEN_OPENCODE_CONFIG_PATH",
            parent_symlink.join("opencode.json"),
        );
        let review = preview(&request(), &root.join("backups"));
        assert!(!review.ok, "symlinked parent must fail closed");
        assert!(
            review.summary.contains("symbolic links"),
            "unexpected refusal summary: {}",
            review.summary
        );
        env::remove_var("DTOKEN_OPENCODE_CONFIG_PATH");

        // Missing auth.json is never a creation target.
        env::set_var(
            "DTOKEN_OPENCODE_CONFIG_PATH",
            fs::canonicalize(&real_dir)
                .expect("canonical real dir")
                .join("auth.json"),
        );
        let review = preview(&request(), &root.join("backups"));
        assert!(!review.ok, "auth.json must fail closed");
        assert!(
            review.summary.contains("auth.json"),
            "unexpected refusal summary: {}",
            review.summary
        );
        env::remove_var("DTOKEN_OPENCODE_CONFIG_PATH");

        // Parent traversal is rejected for the missing-target creation case.
        env::set_var(
            "DTOKEN_OPENCODE_CONFIG_PATH",
            root.join("no-such-dir").join("..").join("opencode.json"),
        );
        let review = preview(&request(), &root.join("backups"));
        assert!(!review.ok, "parent traversal must fail closed");
        assert!(
            review.summary.contains("parent-directory"),
            "unexpected refusal summary: {}",
            review.summary
        );
        env::remove_var("DTOKEN_OPENCODE_CONFIG_PATH");
    }

    /// JSONC fail-closed lock: an existing opencode.jsonc on the default
    /// path blocks creation of opencode.json, and an explicit missing jsonc
    /// target is never admitted as a creation target.
    #[test]
    fn jsonc_only_config_and_missing_jsonc_target_are_rejected() {
        let _guard = env_lock();
        let dir = tempdir().expect("temp dir");
        let root = fs::canonicalize(dir.path()).expect("canonical temp dir");
        let previous_xdg = env::var_os("XDG_CONFIG_HOME");
        env::set_var("XDG_CONFIG_HOME", &root);
        let opencode_dir = root.join("opencode");
        fs::create_dir(&opencode_dir).expect("create OpenCode config directory");
        fs::write(opencode_dir.join("opencode.jsonc"), "{ /* user JSONC */ }")
            .expect("write JSONC config");

        let review = preview(&request(), &root.join("backups"));
        assert!(!review.ok, "JSONC-only config must fail closed");
        assert!(
            review.summary.contains("JSONC"),
            "unexpected refusal summary: {}",
            review.summary
        );
        assert!(
            !opencode_dir.join("opencode.json").exists(),
            "opencode.json must not be created next to user JSONC"
        );

        match previous_xdg {
            Some(value) => env::set_var("XDG_CONFIG_HOME", value),
            None => env::remove_var("XDG_CONFIG_HOME"),
        }

        // An explicit missing jsonc target is refused by the name guard.
        let custom = root.join("custom");
        fs::create_dir(&custom).expect("create custom directory");
        let explicit = custom.join("opencode.jsonc");
        env::set_var("DTOKEN_OPENCODE_CONFIG_PATH", &explicit);
        let review = preview(&request(), &root.join("backups"));
        assert!(!review.ok, "missing jsonc target must fail closed");
        assert!(
            review.summary.contains("not the strict opencode.json"),
            "unexpected refusal summary: {}",
            review.summary
        );
        assert!(!explicit.exists());
        env::remove_var("DTOKEN_OPENCODE_CONFIG_PATH");
    }
}
