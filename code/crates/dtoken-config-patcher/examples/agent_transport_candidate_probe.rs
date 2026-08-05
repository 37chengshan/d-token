use anyhow::{bail, Context, Result};
use dtoken_config_patcher::{
    agent_transport::{
        apply_reviewed_agent_transport, preview_clear_agent_transport,
        preview_configure_agent_transport, verify_agent_transport, AgentTransportOperation,
        AgentTransportRequest,
    },
    ConfigPatcher, ExpectedFileState,
};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    env, fs,
    path::{Path, PathBuf},
};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

fn main() -> Result<()> {
    let args = env::args().skip(1).collect::<Vec<_>>();
    let [agent_id, target_path, backup_root, endpoint, model] = args.as_slice() else {
        bail!(
            "usage: agent_transport_candidate_probe AGENT_ID TARGET_PATH BACKUP_ROOT ENDPOINT MODEL"
        );
    };
    if !matches!(
        agent_id.as_str(),
        "claude-code" | "codex" | "cline" | "gemini-cli" | "roo-code" | "continue" | "aider"
    ) {
        bail!("candidate probe supports only verified single-file transports");
    }

    let target_path = PathBuf::from(target_path);
    let metadata = fs::symlink_metadata(&target_path)
        .context("candidate Agent configuration metadata is unavailable")?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        bail!("candidate Agent configuration must be a regular non-symlink file");
    }
    let original = fs::read(&target_path).context("read original Agent configuration")?;
    let original_sha256 = digest(&original);
    let original_mode = file_mode(&metadata);
    let request = AgentTransportRequest {
        agent_id: agent_id.clone(),
        target_path: target_path.clone(),
        endpoint: endpoint.clone(),
        enabled: true,
        model: Some(model.clone()),
    };

    let configure_review =
        preview_configure_agent_transport(&request).context("preview configure transport")?;
    if configure_review.current_upstream.is_some() {
        bail!("an existing d-token-owned Agent transport is already active");
    }
    if !configure_review.changed {
        bail!("candidate configure review produced no owned change");
    }
    let configure = apply_reviewed_agent_transport(
        &request,
        AgentTransportOperation::Configure,
        Path::new(backup_root),
        &configure_review.revision,
        &configure_review.before_sha256,
        &configure_review.after_sha256,
    )
    .context("apply reviewed configure transport")?;
    if !configure.changed || !configure.verification.ok {
        bail!("candidate configure transaction was not applied and verified");
    }
    let configure_change = configure
        .transaction
        .as_ref()
        .and_then(|transaction| transaction.changes.first())
        .context("candidate configure transaction has no durable backup")?;
    let original_backup = configure_change.backup.clone();
    let patcher = ConfigPatcher::new(backup_root);
    let configured_sha256 = configure.after_sha256.clone();
    let mut expected_owned_sha256 = configured_sha256.clone();

    let candidate_result = (|| -> Result<serde_json::Value> {
        let configured_verification =
            verify_agent_transport(&request).context("verify configured transport")?;
        if !configured_verification.ok
            || configured_verification
                .current_upstream
                .as_ref()
                .is_none_or(|upstream| {
                    upstream.endpoint.as_str() != endpoint
                        || upstream.model.as_deref() != Some(model.as_str())
                })
        {
            bail!("configured Agent transport did not match the reviewed endpoint and model");
        }

        let clear_review =
            preview_clear_agent_transport(&request).context("preview clear transport")?;
        if !clear_review.changed {
            bail!("candidate clear review did not identify the owned transport");
        }
        let clear = apply_reviewed_agent_transport(
            &request,
            AgentTransportOperation::Clear,
            Path::new(backup_root),
            &clear_review.revision,
            &clear_review.before_sha256,
            &clear_review.after_sha256,
        )
        .context("apply reviewed clear transport")?;
        expected_owned_sha256 = clear.after_sha256.clone();
        if !clear.changed || !clear.verification.ok || clear.verification.current_upstream.is_some()
        {
            bail!("candidate clear transaction did not remove only the owned transport");
        }

        let restore = patcher
            .restore_backup(
                &original_backup,
                &ExpectedFileState::Hash(expected_owned_sha256.clone()),
            )
            .context("restore exact pre-candidate Agent configuration")?;
        let restored = fs::read(&target_path).context("read restored Agent configuration")?;
        let restored_metadata =
            fs::metadata(&target_path).context("read restored Agent configuration metadata")?;
        if restored != original
            || restore.restored.sha256.as_deref() != Some(original_sha256.as_str())
            || file_mode(&restored_metadata) != original_mode
        {
            bail!("restored Agent configuration does not match the exact original bytes and mode");
        }

        Ok(json!({
            "source": "dtoken-config-patcher",
            "agent_id": agent_id,
            "target": match agent_id.as_str() {
                "claude-code" => "~/.claude/settings.json",
                "codex" => "<codex-home>/config.toml",
                "cline" => "~/.cline/data/settings/providers.json",
                "gemini-cli" => "~/.gemini/settings.json (+ .env + manifest)",
                "roo-code" => "~/.roo/settings.json (+ manifest)",
                "continue" => "~/.continue/config.json (+ manifest)",
                "aider" => "~/.aider.conf.yml (+ manifest)",
                _ => "<agent-config>",
            },
            "configure": {
                "previewed": true,
                "revision_bound": true,
                "before_sha256": configure.before_sha256,
                "after_sha256": configure.after_sha256,
                "backup_created": true,
                "verified": true,
            },
            "clear": {
                "previewed": true,
                "revision_bound": true,
                "before_sha256": clear.before_sha256,
                "after_sha256": clear.after_sha256,
                "verified": true,
                "owned_transport_removed": true,
            },
            "restore": {
                "state": "restored_exact",
                "sha256": original_sha256,
                "mode_preserved": true,
            },
            "network_request_made": false,
            "agent_process_started": false,
            "raw_config_returned": false,
            "credential_value_read": false,
            "model_returned": false,
        }))
    })();

    match candidate_result {
        Ok(report) => {
            println!("{report}");
            Ok(())
        }
        Err(candidate_error) => {
            let current_sha256 =
                digest(&fs::read(&target_path).context("read failed candidate target")?);
            if current_sha256 == expected_owned_sha256 {
                patcher
                    .restore_backup(
                        &original_backup,
                        &ExpectedFileState::Hash(expected_owned_sha256),
                    )
                    .context("recover exact Agent configuration after candidate failure")?;
                let restored =
                    fs::read(&target_path).context("read recovered Agent configuration")?;
                if restored != original {
                    bail!(
                        "candidate failed and recovery did not restore exact original bytes: {candidate_error:#}"
                    );
                }
                bail!(
                    "candidate Agent transaction failed after exact recovery: {candidate_error:#}"
                );
            }
            if current_sha256 == original_sha256 {
                bail!("candidate Agent transaction failed before changing final state: {candidate_error:#}");
            }
            bail!(
                "candidate Agent transaction failed and an external configuration change was preserved: {candidate_error:#}"
            );
        }
    }
}

fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(unix)]
fn file_mode(metadata: &fs::Metadata) -> Option<u32> {
    Some(metadata.permissions().mode() & 0o777)
}

#[cfg(not(unix))]
fn file_mode(_metadata: &fs::Metadata) -> Option<u32> {
    None
}
