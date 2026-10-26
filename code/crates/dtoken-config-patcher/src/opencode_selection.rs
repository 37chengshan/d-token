//! OpenCode's provider registration and default selection are one transaction.
//! The previous selection lives beside the config, never in OpenCode's schema.
use super::agent_transport::*;
use crate::{ConfigPatcher, ExpectedFileState, PreparedChange};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Selection {
    schema_version: u32,
    managed_by: String,
    config_name: String,
    model: String,
    endpoint: String,
    previous_model: Option<String>,
}

fn manifest_path(target: &Path) -> Result<PathBuf> {
    let name = target
        .file_name()
        .and_then(|v| v.to_str())
        .context("OpenCode config name is invalid")?;
    Ok(target.with_file_name(format!(".{name}.dtoken-selection.json")))
}

fn read(path: &Path) -> Result<String> {
    match std::fs::symlink_metadata(path) {
        Ok(meta) if !meta.is_file() || meta.file_type().is_symlink() => {
            bail!("OpenCode target must be a regular file")
        }
        Ok(_) => Ok(std::fs::read_to_string(path)?),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(error) => Err(error.into()),
    }
}

fn selection(text: &str, target: &Path) -> Result<Option<Selection>> {
    if text.trim().is_empty() || text.trim() == "null" {
        return Ok(None);
    }
    let value: Selection = serde_json::from_str(text)?;
    if value.schema_version != 1
        || value.managed_by != "d-token"
        || Some(value.config_name.as_str()) != target.file_name().and_then(|v| v.to_str())
    {
        bail!("OpenCode selection ownership record is invalid");
    }
    Ok(Some(value))
}

fn selected_model(model: &str) -> String {
    format!("{OPENCODE_PROVIDER_ID}/{model}")
}

struct Plan {
    review: AgentTransportReview,
    changes: Vec<PreparedChange>,
}

fn plan(
    request: &AgentTransportRequest,
    operation: AgentTransportOperation,
    backup_root: &Path,
) -> Result<Plan> {
    let before = read(&request.target_path)?;
    let manifest = manifest_path(&request.target_path)?;
    let manifest_before = read(&manifest)?;
    let owned = selection(&manifest_before, &request.target_path)?;
    let mut root: Value = if before.trim().is_empty() {
        serde_json::json!({})
    } else {
        serde_json::from_str(&before)?
    };
    let current = root.get("model");
    if current.is_some_and(|v| !v.is_string()) {
        bail!("OpenCode default model must be a string");
    }
    if let Some(owned) = &owned {
        if current.and_then(Value::as_str) != Some(selected_model(&owned.model).as_str()) {
            bail!("OpenCode default model changed outside d-token; restore or review that change first");
        }
        let expected = opencode_provider_value(&owned.endpoint, &owned.model);
        if root.pointer("/provider/dtoken-local-route") != Some(&expected) {
            bail!("OpenCode provider changed outside d-token; automatic update is unsafe");
        }
    }
    let (after, manifest_after) = match operation {
        AgentTransportOperation::Configure => {
            if !request.enabled {
                bail!("OpenCode default connection must be enabled");
            }
            let model = request
                .model
                .as_deref()
                .context("OpenCode model is required")?;
            let previous_model = match owned {
                Some(ref owned) => owned.previous_model.clone(),
                None => current.and_then(Value::as_str).map(str::to_owned),
            };
            if owned.is_none()
                && previous_model
                    .as_deref()
                    .is_some_and(|model| model.starts_with("dtoken-local-route/"))
            {
                bail!("OpenCode previous model is unknown; restore its earlier rollback point before reconnecting");
            }
            // Do not record credentials accidentally pasted into a model field.
            if previous_model
                .as_deref()
                .is_some_and(dtoken_compress::optical::contains_secret_like)
            {
                bail!("OpenCode model selection contains sensitive content");
            }
            let configured =
                configure_opencode_transport_document(&before, &request.endpoint, model, true)?;
            root = serde_json::from_str(&configured)?;
            root["model"] = Value::String(selected_model(model));
            let state = Selection {
                schema_version: 1,
                managed_by: "d-token".into(),
                config_name: request
                    .target_path
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .into_owned(),
                model: model.into(),
                endpoint: request.endpoint.clone(),
                previous_model,
            };
            (
                serde_json::to_string_pretty(&root)? + "\n",
                serde_json::to_string_pretty(&state)? + "\n",
            )
        }
        AgentTransportOperation::Clear => {
            let cleared = clear_agent_transport_document("opencode", &before)?;
            if let Some(owned) = owned {
                root = serde_json::from_str(&cleared)?;
                match owned.previous_model {
                    Some(previous) => {
                        root["model"] = Value::String(previous);
                    }
                    None => {
                        root.as_object_mut()
                            .context("OpenCode config must be an object")?
                            .remove("model");
                    }
                }
                (serde_json::to_string_pretty(&root)? + "\n", "null\n".into())
            } else {
                // Legacy registrations did not own the default selection.
                if current
                    .and_then(Value::as_str)
                    .is_some_and(|v| v.starts_with("dtoken-local-route/"))
                {
                    bail!("OpenCode previous model is unknown; use its exact rollback point");
                }
                (cleared, manifest_before.clone())
            }
        }
    };
    let patcher = ConfigPatcher::new(backup_root);
    let mut changes = Vec::new();
    for (path, old, new) in [
        (&request.target_path, &before, &after),
        (&manifest, &manifest_before, &manifest_after),
    ] {
        if old != new {
            let change = patcher.prepare_change(
                path,
                new,
                dtoken_types::ConfigToolKind::Agent,
                Some("opencode".into()),
                dtoken_types::RiskLevel::Medium,
            )?;
            if change.plan.before_hash != crate::stable_hash(old) {
                bail!("OpenCode configuration changed during planning");
            }
            changes.push(change);
        }
    }
    let before_sha256 = crate::stable_hash(&before);
    let after_sha256 = crate::stable_hash(&after);
    Ok(Plan {
        review: AgentTransportReview {
            operation,
            agent_id: "opencode".into(),
            target_path: request.target_path.clone(),
            revision: crate::stable_hash(&format!(
                "opencode-selection-v1\n{operation:?}\n{before_sha256}\n{after_sha256}\n{}\n{}",
                crate::stable_hash(&manifest_before),
                crate::stable_hash(&manifest_after)
            )),
            changed: !changes.is_empty(),
            before_sha256,
            after_sha256,
            redacted_diff: crate::simple_diff(&before, &after),
            current_upstream: None,
        },
        changes,
    })
}

pub(crate) fn preview(
    request: &AgentTransportRequest,
    operation: AgentTransportOperation,
) -> Result<AgentTransportReview> {
    Ok(plan(
        request,
        operation,
        &request.target_path.with_extension("dtoken-backups"),
    )?
    .review)
}

pub(crate) fn verify(request: &AgentTransportRequest) -> Result<AgentTransportVerification> {
    let text = read(&request.target_path)?;
    let result: Result<()> = (|| {
        let state = selection(
            &read(&manifest_path(&request.target_path)?)?,
            &request.target_path,
        )?
        .context("OpenCode default connection has not been activated")?;
        let root: Value = serde_json::from_str(&text)?;
        if root.get("model").and_then(Value::as_str) != Some(selected_model(&state.model).as_str())
            || state.endpoint != request.endpoint
            || request.model.as_deref() != Some(&state.model)
        {
            bail!("OpenCode default connection does not match the requested route");
        }
        let expected = opencode_provider_value(&state.endpoint, &state.model);
        if root.pointer("/provider/dtoken-local-route") != Some(&expected) {
            bail!("OpenCode connection changed after configuration");
        }
        let configured =
            configure_opencode_transport_document(&text, &state.endpoint, &state.model, true)?;
        if serde_json::from_str::<Value>(&configured)? != root {
            bail!("OpenCode connection is not enabled");
        }
        Ok(())
    })();
    let ok = result.is_ok();
    Ok(AgentTransportVerification {
        agent_id: "opencode".into(),
        kind: AgentTransportAdapterKind::OpenCodeProvider,
        ok: result.is_ok(),
        detail: result
            .err()
            .map(|e| e.to_string())
            .unwrap_or_else(|| "OpenCode default connection verified".into()),
        current_upstream: ok.then(|| AgentTransportUpstreamSignature {
            provider_id: OPENCODE_PROVIDER_ID.into(),
            endpoint: request.endpoint.clone(),
            model: request.model.clone(),
            enabled: true,
            signature_sha256: crate::stable_hash(&format!(
                "agent-upstream-v1\nopencode\n{}\n{}\ntrue",
                request.endpoint,
                request.model.as_deref().unwrap_or_default()
            )),
        }),
    })
}

pub(crate) fn can_clear(target: &Path) -> bool {
    let request = AgentTransportRequest {
        agent_id: "opencode".into(),
        target_path: target.into(),
        endpoint: String::new(),
        model: None,
        enabled: false,
    };
    preview(&request, AgentTransportOperation::Clear).is_ok_and(|v| v.changed)
}

pub(crate) fn apply(
    request: &AgentTransportRequest,
    operation: AgentTransportOperation,
    backup_root: &Path,
) -> Result<AgentTransportReceipt> {
    let plan = plan(request, operation, backup_root)?;
    apply_plan(request, operation, backup_root, plan)
}

pub(crate) fn apply_reviewed(
    request: &AgentTransportRequest,
    operation: AgentTransportOperation,
    backup_root: &Path,
    revision: &str,
    before: &str,
    after: &str,
) -> Result<AgentTransportReceipt> {
    let plan = plan(request, operation, backup_root)?;
    if plan.review.revision != revision
        || plan.review.before_sha256 != before
        || plan.review.after_sha256 != after
    {
        bail!("OpenCode configuration or selection record changed after review");
    }
    apply_plan(request, operation, backup_root, plan)
}

fn apply_plan(
    request: &AgentTransportRequest,
    operation: AgentTransportOperation,
    backup_root: &Path,
    plan: Plan,
) -> Result<AgentTransportReceipt> {
    let patcher = ConfigPatcher::new(backup_root);
    let transaction = if plan.changes.is_empty() {
        None
    } else {
        Some(patcher.apply_transaction(&plan.changes)?)
    };
    let verification = if operation == AgentTransportOperation::Configure {
        verify(request)
    } else {
        preview(request, AgentTransportOperation::Clear).map(|v| AgentTransportVerification {
            agent_id: "opencode".into(),
            kind: AgentTransportAdapterKind::OpenCodeProvider,
            ok: !v.changed,
            detail: "OpenCode previous selection restored".into(),
            current_upstream: None,
        })
    };
    if verification.as_ref().map_or(true, |v| !v.ok) {
        if let Some(tx) = &transaction {
            for change in tx.changes.iter().rev() {
                patcher.restore_backup(
                    &change.backup,
                    &ExpectedFileState::Hash(change.after.sha256.clone().unwrap_or_default()),
                )?;
            }
        }
        bail!("OpenCode connection verification failed; exact configuration restored");
    }
    Ok(AgentTransportReceipt {
        operation,
        agent_id: "opencode".into(),
        target_path: request.target_path.clone(),
        before_sha256: plan.review.before_sha256,
        after_sha256: plan.review.after_sha256,
        changed: transaction.is_some(),
        transaction,
        verification: verification?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(path: &Path) -> AgentTransportRequest {
        AgentTransportRequest {
            agent_id: "opencode".into(),
            target_path: path.into(),
            endpoint: "http://127.0.0.1:18001".into(),
            enabled: true,
            model: Some("mimo-v2.5-free".into()),
        }
    }

    #[test]
    fn opencode_default_activation_update_and_clear_preserve_original_selection() {
        for original_model in [None, Some("existing/original")] {
            let dir =
                tempfile::tempdir_in(std::fs::canonicalize(std::env::temp_dir()).unwrap()).unwrap();
            let path = dir.path().join("opencode.json");
            let mut original = serde_json::json!({"provider":{"existing":{"name":"User provider"}},"snapshot":true});
            if let Some(model) = original_model {
                original["model"] = model.into();
            }
            std::fs::write(&path, serde_json::to_string(&original).unwrap()).unwrap();
            let mut req = request(&path);
            let receipt = apply(
                &req,
                AgentTransportOperation::Configure,
                &dir.path().join("backups"),
            )
            .unwrap();
            assert!(receipt.verification.ok);
            assert_eq!(receipt.transaction.unwrap().changes.len(), 2);
            let configured: Value = serde_json::from_str(&read(&path).unwrap()).unwrap();
            assert_eq!(configured["model"], selected_model("mimo-v2.5-free"));
            assert_eq!(
                configured["provider"]["existing"],
                original["provider"]["existing"]
            );
            assert!(
                !apply(
                    &req,
                    AgentTransportOperation::Configure,
                    &dir.path().join("backups")
                )
                .unwrap()
                .changed
            );
            req.model = Some("next-model".into());
            apply(
                &req,
                AgentTransportOperation::Configure,
                &dir.path().join("backups"),
            )
            .unwrap();
            apply(
                &req,
                AgentTransportOperation::Clear,
                &dir.path().join("backups"),
            )
            .unwrap();
            let restored: Value = serde_json::from_str(&read(&path).unwrap()).unwrap();
            assert_eq!(restored, original);
            assert!(
                !apply(
                    &req,
                    AgentTransportOperation::Clear,
                    &dir.path().join("backups")
                )
                .unwrap()
                .changed
            );
        }
    }

    #[test]
    fn opencode_selection_drift_stale_review_and_symlink_block_writes() {
        let dir =
            tempfile::tempdir_in(std::fs::canonicalize(std::env::temp_dir()).unwrap()).unwrap();
        let path = dir.path().join("opencode.json");
        std::fs::write(&path, "{}").unwrap();
        let req = request(&path);
        let backups = dir.path().join("backups");
        let review = preview(&req, AgentTransportOperation::Configure).unwrap();
        std::fs::write(manifest_path(&path).unwrap(), "null\n").unwrap();
        assert!(apply_reviewed(
            &req,
            AgentTransportOperation::Configure,
            &backups,
            &review.revision,
            &review.before_sha256,
            &review.after_sha256
        )
        .is_err());
        assert_eq!(read(&path).unwrap(), "{}");
        apply(&req, AgentTransportOperation::Configure, &backups).unwrap();
        let mut value: Value = serde_json::from_str(&read(&path).unwrap()).unwrap();
        value["model"] = "user/new-choice".into();
        std::fs::write(&path, serde_json::to_string(&value).unwrap()).unwrap();
        let before = read(&path).unwrap();
        assert!(!verify(&req).unwrap().ok);
        assert!(apply(&req, AgentTransportOperation::Clear, &backups).is_err());
        assert!(apply(&req, AgentTransportOperation::Configure, &backups).is_err());
        assert_eq!(read(&path).unwrap(), before);
        #[cfg(unix)]
        {
            std::fs::remove_file(manifest_path(&path).unwrap()).unwrap();
            std::os::unix::fs::symlink(dir.path().join("absent"), manifest_path(&path).unwrap())
                .unwrap();
            assert!(preview(&req, AgentTransportOperation::Configure).is_err());
        }
    }

    #[test]
    fn opencode_legacy_owned_provider_can_be_activated() {
        let dir =
            tempfile::tempdir_in(std::fs::canonicalize(std::env::temp_dir()).unwrap()).unwrap();
        let path = dir.path().join("opencode.json");
        let legacy = configure_opencode_transport_document(
            "{}",
            "http://127.0.0.1:18002",
            "old-model",
            true,
        )
        .unwrap();
        std::fs::write(&path, legacy).unwrap();
        let req = request(&path);
        assert!(
            apply(
                &req,
                AgentTransportOperation::Configure,
                &dir.path().join("backups")
            )
            .unwrap()
            .verification
            .ok
        );
        assert!(can_clear(&path));
    }
}
