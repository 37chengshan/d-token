use dtoken_config_patcher::agent_transport::{
    ensure_owned_opencode_provider, opencode_base_url, opencode_provider_value,
    OPENCODE_PROVIDER_ID,
};
use dtoken_config_patcher::{ConfigPatcher, ExpectedFileState};
use dtoken_types::{ConfigToolKind, RiskLevel};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::{
    env, fs,
    path::{Path, PathBuf},
};

/// The dedicated desktop flow writes the same provider shape as the managed
/// transport through the Rust-owned canonical builder in
/// `dtoken_config_patcher::agent_transport`, so either flow can verify or
/// clear what the other wrote.

#[derive(Debug, Clone, Deserialize)]
pub struct OpenCodeConnectionRequest {
    pub model: String,
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
    pub model: String,
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
    pub model: String,
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
        Ok((path, prepared)) => OpenCodeConnectionReview {
            ok: true,
            state: "review_required".to_string(),
            source: "dtoken-desktop-opencode-adapter",
            freshness: "fresh",
            target: display_path(&path),
            model: request.model.clone(),
            listener: format!("127.0.0.1:{}", request.listener_port),
            transaction: "scan-plan-diff-backup-confirm-apply-verify-doctor-rollback".to_string(),
            revision: Some(review_revision(
                request,
                &prepared.plan.before_hash,
                &prepared.plan.after_hash,
            )),
            before_sha256: Some(prepared.plan.before_hash.clone()),
            after_sha256: Some(prepared.plan.after_hash.clone()),
            diff: Some(prepared.plan.diff.clone()),
            summary: "Review the exact OpenCode provider change before applying it".to_string(),
            privacy: privacy(),
        },
        Err(error) => failed_review(request, error),
    }
}

pub fn apply(request: &OpenCodeConnectionRequest, backup_root: &Path) -> OpenCodeConnectionReceipt {
    let (path, prepared) = match prepare(request, backup_root) {
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
        request,
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

    if prepared.plan.before_hash == prepared.plan.after_hash {
        if let Err(error) = verify_provider(&path, &request.model, request.listener_port) {
            return failed_receipt(request, error);
        }
        return OpenCodeConnectionReceipt {
            ok: true,
            state: "already_verified".to_string(),
            source: "dtoken-desktop-opencode-adapter",
            freshness: "fresh",
            target: display_path(&path),
            model: request.model.clone(),
            listener: format!("127.0.0.1:{}", request.listener_port),
            transaction: "review-verify-no-change".to_string(),
            revision: Some(revision),
            before_sha256: Some(prepared.plan.before_hash),
            after_sha256: Some(prepared.plan.after_hash),
            rollback_id: None,
            verified: true,
            summary: "Existing OpenCode connection verified without rewriting configuration"
                .to_string(),
            privacy: privacy(),
        };
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
    let verified = verify_provider(&path, &request.model, request.listener_port).is_ok();
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
        model: request.model.clone(),
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

fn prepare(
    request: &OpenCodeConnectionRequest,
    backup_root: &Path,
) -> Result<(PathBuf, dtoken_config_patcher::PreparedChange), String> {
    validate_request(request)?;
    let path = discover_config_path()?;
    reject_unsafe_target(&path)?;
    let bytes = fs::read(&path).map_err(|error| format!("read OpenCode config: {error}"))?;
    let text = String::from_utf8(bytes).map_err(|_| "OpenCode config is not UTF-8".to_string())?;
    let mut root: Value = serde_json::from_str(&text)
        .map_err(|error| format!("OpenCode config is not strict JSON: {error}"))?;
    let object = root
        .as_object_mut()
        .ok_or_else(|| "OpenCode config root must be an object".to_string())?;
    patch_provider(object, &request.model, request.listener_port)?;
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
    Ok((path, prepared))
}

fn patch_provider(root: &mut Map<String, Value>, model: &str, port: u16) -> Result<(), String> {
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
            ensure_owned_opencode_provider(existing).map_err(|error| {
                format!("existing OpenCode provider ownership check failed: {error:#}")
            })?;
        }
        providers.insert(
            OPENCODE_PROVIDER_ID.to_string(),
            opencode_provider_value(&format!("http://127.0.0.1:{port}"), model),
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

fn verify_provider(path: &Path, model: &str, port: u16) -> Result<(), String> {
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
    if provider.pointer("/options/baseURL").and_then(Value::as_str)
        != Some(expected_base_url.as_str())
        || provider
            .pointer("/models")
            .and_then(Value::as_object)
            .is_none_or(|models| !models.contains_key(model))
    {
        return Err("applied provider does not match the reviewed listener/model".to_string());
    }
    Ok(())
}

fn validate_request(request: &OpenCodeConnectionRequest) -> Result<(), String> {
    if request.listener_port < 1024 {
        return Err("listener port must be between 1024 and 65535".to_string());
    }
    let model = request.model.trim();
    if model.is_empty() || model.len() > 256 || model.contains(['\n', '\r', '\0']) {
        return Err("model is empty or contains unsupported characters".to_string());
    }
    Ok(())
}

fn discover_config_path() -> Result<PathBuf, String> {
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
    let metadata =
        fs::symlink_metadata(path).map_err(|error| format!("inspect OpenCode config: {error}"))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("OpenCode config must be an existing regular non-symlink file".to_string());
    }
    Ok(())
}

fn display_path(path: &Path) -> String {
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

fn privacy() -> &'static str {
    "Normal strict JSON config only; auth.json, credentials, prompts, and responses are never read or returned"
}

fn review_revision(request: &OpenCodeConnectionRequest, before: &str, after: &str) -> String {
    dtoken_config_patcher::stable_hash(&format!(
        "opencode-v1\n{}\n{}\n{}\n{}",
        request.model, request.listener_port, before, after
    ))
}

fn failed_review(request: &OpenCodeConnectionRequest, summary: String) -> OpenCodeConnectionReview {
    OpenCodeConnectionReview {
        ok: false,
        state: "unavailable".to_string(),
        source: "dtoken-desktop-opencode-adapter",
        freshness: "fresh",
        target: "<normal-opencode-config>".to_string(),
        model: request.model.clone(),
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
        model: request.model.clone(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use dtoken_config_patcher::agent_transport::{
        clear_agent_transport_document, configure_opencode_transport_document,
    };
    use std::sync::{Mutex, OnceLock};
    use tempfile::tempdir;

    fn env_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|error| error.into_inner())
    }

    fn request() -> OpenCodeConnectionRequest {
        OpenCodeConnectionRequest {
            model: "deepseek/model-v1".to_string(),
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
        verify_provider(&config, &request.model, request.listener_port)
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
        let configured = configure_opencode_transport_document(
            original,
            "http://127.0.0.1:18001",
            "deepseek/model-v1",
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
        verify_provider(&config, "deepseek/model-v1", 18001)
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

    /// Byte-level lock: for the same input document, endpoint, and model, the
    /// dedicated desktop flow and the managed transport must serialize the
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

        let managed = configure_opencode_transport_document(
            std::str::from_utf8(original).expect("original is UTF-8"),
            "http://127.0.0.1:18001",
            "deepseek/model-v1",
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

    #[test]
    fn reconfiguration_preserves_foreign_provider_collisions() {
        let original =
            serde_json::json!({"provider":{"dtoken-local-route":{"name":"user managed"}}});
        let mut document = original.as_object().unwrap().clone();
        assert!(patch_provider(&mut document, "mimo-v2.5-free", 18001).is_err());
        assert_eq!(Value::Object(document), original);
    }

    /// Semantic lock: a provider written by the dedicated desktop flow stays
    /// owned, so the managed transport may reconfigure it without touching user
    /// providers, and the dedicated flow still recognizes the result. The
    /// dedicated flow refuses foreign providers and updates only verified ownership.
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
        let reconfigured = configure_opencode_transport_document(
            &applied,
            "http://127.0.0.1:18002",
            "deepseek/model-v2",
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
        verify_provider(&config, "deepseek/model-v2", 18002)
            .expect("dedicated verify recognizes managed reconfigure");

        // One-click configuration also updates a provider proven to be ours.
        let again = preview(&request(), &root.join("backups"));
        assert!(
            again.ok,
            "owned reconfiguration should be reviewable: {}",
            again.summary
        );
        let mut update = request();
        update.reviewed_revision = again.revision;
        update.reviewed_before_sha256 = again.before_sha256;
        update.reviewed_after_sha256 = again.after_sha256;
        let updated = apply(&update, &root.join("backups"));
        assert!(updated.ok && updated.verified);
        verify_provider(&config, &update.model, update.listener_port).unwrap();
        let after: Value = serde_json::from_slice(&fs::read(&config).unwrap()).unwrap();
        assert_eq!(
            after.pointer("/provider/existing"),
            reconfigured_value.pointer("/provider/existing")
        );
        let before_retry = fs::read(&config).unwrap();
        let same = preview(&update, &root.join("backups"));
        update.reviewed_revision = same.revision;
        update.reviewed_before_sha256 = same.before_sha256;
        update.reviewed_after_sha256 = same.after_sha256;
        let verified = apply(&update, &root.join("backups"));
        assert!(verified.ok && verified.verified);
        assert_eq!(verified.state, "already_verified");
        assert!(verified.rollback_id.is_none());
        assert_eq!(fs::read(&config).unwrap(), before_retry);
        env::remove_var("DTOKEN_OPENCODE_CONFIG_PATH");
    }
}
