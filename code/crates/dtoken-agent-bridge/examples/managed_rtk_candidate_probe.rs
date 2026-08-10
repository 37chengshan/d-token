use anyhow::{bail, Context, Result};
use dtoken_preprocessor::{
    inspect_managed_rtk_runtime, uninstall_managed_rtk_runtime, ManagedHookPaths,
    ManagedRtkLifecycle,
};
use dtoken_types::agent::HostPlatform;
use serde_json::json;
use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

fn main() -> Result<()> {
    let args = env::args().skip(1).collect::<Vec<_>>();
    let [bridge_path, managed_root, host_config_fingerprint] = args.as_slice() else {
        bail!(
            "usage: managed_rtk_candidate_probe BRIDGE_PATH MANAGED_ROOT HOST_CONFIG_FINGERPRINT"
        );
    };
    let bridge_path = fs::canonicalize(bridge_path).context("canonicalize candidate bridge")?;
    let managed_root = PathBuf::from(managed_root);
    if managed_root.exists() {
        bail!("candidate managed RTK root already exists");
    }

    let platform = if cfg!(target_os = "macos") {
        HostPlatform::Macos
    } else if cfg!(windows) {
        HostPlatform::Windows
    } else {
        bail!("managed RTK candidate probe requires macOS or Windows");
    };
    let mut lifecycle =
        ManagedRtkLifecycle::plan("rtk-candidate.1", platform, host_config_fingerprint);
    lifecycle
        .install_hook(ManagedHookPaths {
            managed_root: managed_root.clone(),
            bridge_path: bridge_path.clone(),
            max_output_bytes: Some(1_024),
            filter_dsl: None,
        })
        .context("install candidate managed RTK hook")?;
    lifecycle
        .verify_hook()
        .context("verify candidate managed RTK hook")?;

    let candidate_result = run_candidate(&bridge_path, &managed_root);
    match candidate_result {
        Ok(report) => {
            let uninstall =
                uninstall_managed_rtk_runtime(&managed_root).context("uninstall candidate RTK")?;
            if managed_root.exists() {
                bail!("candidate managed RTK root remained after verified uninstall");
            }
            println!(
                "{}",
                json!({
                    "source": "dtoken-managed-rtk",
                    "install_state": "installed_verified",
                    "usage": report,
                    "uninstall_state": format!("{:?}", uninstall.state).to_lowercase(),
                    "root_removed": true,
                    "raw_command_returned": false,
                    "raw_output_returned": false,
                    "command_argv_persisted": false,
                    "credential_value_read": false,
                })
            );
            Ok(())
        }
        Err(error) => {
            if inspect_managed_rtk_runtime(&managed_root).verified {
                let _ = uninstall_managed_rtk_runtime(&managed_root);
            }
            bail!("managed RTK candidate failed: {error:#}");
        }
    }
}

fn run_candidate(bridge_path: &Path, managed_root: &Path) -> Result<serde_json::Value> {
    // The success command emits a complete pretty-printed JSON document so
    // the managed wrapper's structured JSON family (PR-C4A) minifies it
    // losslessly. macOS uses /usr/bin/printf; Windows uses cmd /c with a
    // bounded echo sequence.
    #[cfg(windows)]
    let success_command = {
        let mut command = vec![
            "cmd".to_string(),
            "/c".to_string(),
            "echo".to_string(),
            "{\"name\": \"candidate\", \"ok\": true, \"count\": 3}".to_string(),
        ];
        command
    };
    #[cfg(not(windows))]
    let success_command = vec![
        "/usr/bin/printf".to_string(),
        "{\\n  \"name\": \"candidate\",\\n  \"ok\": true,\\n  \"count\": 3\\n}\\n".to_string(),
    ];
    let mut success_args = vec![
        "rtk".to_string(),
        "wrap".to_string(),
        "--managed-root".to_string(),
        managed_root
            .to_str()
            .context("managed RTK root is not UTF-8")?
            .to_string(),
        "--max-bytes".to_string(),
        "1024".to_string(),
        "--".to_string(),
    ];
    success_args.extend(success_command);
    let success = Command::new(bridge_path)
        .args(&success_args)
        .output()
        .context("invoke candidate managed RTK success command")?;
    if !success.status.success() {
        bail!("managed RTK filtered candidate output but lost the exit status");
    }
    let success_stdout = String::from_utf8_lossy(&success.stdout);
    // The JSON family minifies the pretty document into a single line.
    let minified = serde_json::from_str::<serde_json::Value>(&success_stdout.trim())
        .context("candidate RTK did not return minified JSON output")?;
    if minified
        != serde_json::json!({"count": 3, "name": "candidate", "ok": true})
    {
        bail!("managed RTK did not minify the successful candidate output");
    }

    // The failure command exits 1; the wrapper must propagate it exactly.
    #[cfg(windows)]
    let failure_args = vec![
        "rtk".to_string(),
        "wrap".to_string(),
        "--managed-root".to_string(),
        managed_root
            .to_str()
            .context("managed RTK root is not UTF-8")?
            .to_string(),
        "--max-bytes".to_string(),
        "1024".to_string(),
        "--".to_string(),
        "cmd".to_string(),
        "/c".to_string(),
        "exit 1".to_string(),
    ];
    #[cfg(not(windows))]
    let failure_args = vec![
        "rtk".to_string(),
        "wrap".to_string(),
        "--managed-root".to_string(),
        managed_root
            .to_str()
            .context("managed RTK root is not UTF-8")?
            .to_string(),
        "--max-bytes".to_string(),
        "1024".to_string(),
        "--".to_string(),
        "/usr/bin/false".to_string(),
    ];
    let failure = Command::new(bridge_path)
        .args(&failure_args)
        .output()
        .context("invoke candidate managed RTK failure command")?;
    if failure.status.code() != Some(1) {
        bail!("managed RTK did not preserve the wrapped failure exit code");
    }

    let evidence = inspect_managed_rtk_runtime(managed_root);
    if !evidence.installed
        || !evidence.verified
        || evidence.usage_count != 2
        || evidence.saved_bytes == 0
        || evidence
            .last_receipt
            .as_ref()
            .and_then(|receipt| receipt.exit_code)
            != Some(1)
    {
        bail!("managed RTK usage evidence is incomplete or unverified");
    }
    Ok(json!({
        "verified": evidence.verified,
        "usage_count": evidence.usage_count,
        "saved_bytes": evidence.saved_bytes,
        "success_exit_propagated": true,
        "failure_exit_code": 1,
        "failure_exit_propagated": true,
        "filtered_output_observed": true,
        "reason": evidence.reason,
    }))
}
