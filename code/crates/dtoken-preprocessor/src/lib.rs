use dtoken_types::{
    HookStatus, IntegrityState, PreprocessorImplementation, PreprocessorProfile, RtkGainReport,
    RtkStatus, TelemetryStatus, ToolStatus,
};
use std::{env, fs, path::PathBuf};
use tokio::process::Command;

#[derive(Debug, Clone, Default)]
pub struct RtkAdapter {
    pub binary_path: Option<String>,
}

impl RtkAdapter {
    pub fn new(binary_path: Option<String>) -> Self {
        Self { binary_path }
    }

    pub async fn detect() -> RtkStatus {
        if is_fixture_profile() {
            return fixture_status();
        }

        let adapter = Self::default();
        match adapter.run_version().await {
            Some(version) => RtkStatus {
                status: ToolStatus::Found,
                binary_path: Some(adapter.binary_name()),
                version: Some(version),
                install_method: Some("external_path".to_string()),
                hook_status: HookStatus::Unknown,
                integrity_state: IntegrityState::Unknown,
                telemetry_status: TelemetryStatus::Unknown,
                warnings: vec!["M1 only detects RTK binary/version; hook and analytics import are skeletons".to_string()],
            },
            None => RtkStatus {
                status: ToolStatus::Missing,
                binary_path: None,
                version: None,
                install_method: None,
                hook_status: HookStatus::Missing,
                integrity_state: IntegrityState::NotInstalled,
                telemetry_status: TelemetryStatus::Unknown,
                warnings: vec!["rtk binary was not found in PATH".to_string()],
            },
        }
    }

    pub async fn gain_report(&self) -> RtkGainReport {
        if is_fixture_profile() {
            return fixture_gain_report();
        }

        let available = self.run_version().await.is_some();
        RtkGainReport {
            available,
            total_commands: 0,
            input_tokens: 0,
            output_tokens: 0,
            saved_tokens: 0,
            savings_percent: 0.0,
            source: if available {
                "m1_mock_after_binary_detection".to_string()
            } else {
                "m1_mock_missing_rtk".to_string()
            },
        }
    }

    pub fn profile_from_status(status: &RtkStatus) -> PreprocessorProfile {
        PreprocessorProfile {
            id: "rtk-default".to_string(),
            name: "RTK".to_string(),
            implementation: PreprocessorImplementation::RtkExternal,
            binary_path: status.binary_path.clone(),
            version: status.version.clone(),
            agent_hook_status: status.hook_status.clone(),
            telemetry_status: status.telemetry_status.clone(),
            config_path: None,
            tracking_db_path: None,
            tee_dir: None,
            supported_agents: vec![
                "claude-code".to_string(),
                "codex".to_string(),
                "cursor".to_string(),
                "windsurf".to_string(),
                "cline".to_string(),
            ],
        }
    }

    async fn run_version(&self) -> Option<String> {
        let output = Command::new(self.binary_name()).arg("--version").output().await.ok()?;
        if !output.status.success() {
            return None;
        }
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if stdout.is_empty() {
            Some("unknown".to_string())
        } else {
            Some(stdout)
        }
    }

    fn binary_name(&self) -> String {
        self.binary_path.clone().unwrap_or_else(|| "rtk".to_string())
    }
}

pub async fn detect_rtk_status() -> RtkStatus {
    RtkAdapter::detect().await
}

pub async fn rtk_gain_mock() -> RtkGainReport {
    RtkAdapter::default().gain_report().await
}

pub async fn list_preprocessors() -> Vec<PreprocessorProfile> {
    let status = detect_rtk_status().await;
    vec![RtkAdapter::profile_from_status(&status)]
}

fn is_fixture_profile() -> bool {
    matches!(env::var("DTOKEN_PROFILE").ok().as_deref(), Some("fixture"))
}

fn fixture_status() -> RtkStatus {
    let version = read_fixture_text("rtk/version.txt").unwrap_or_else(|| "rtk 0.0.0-fixture".to_string());
    RtkStatus {
        status: ToolStatus::Found,
        binary_path: Some("support/fake-bin/rtk".to_string()),
        version: Some(version.trim().to_string()),
        install_method: Some("fixture".to_string()),
        hook_status: HookStatus::Installed,
        integrity_state: IntegrityState::Verified,
        telemetry_status: TelemetryStatus::Disabled,
        warnings: vec!["fixture mode: real RTK was not called".to_string()],
    }
}

fn fixture_gain_report() -> RtkGainReport {
    read_fixture_text("rtk/gain-all.json")
        .and_then(|raw| serde_json::from_str::<RtkGainReport>(&raw).ok())
        .unwrap_or(RtkGainReport {
            available: true,
            total_commands: 0,
            input_tokens: 0,
            output_tokens: 0,
            saved_tokens: 0,
            savings_percent: 0.0,
            source: "fixture_fallback".to_string(),
        })
}

fn read_fixture_text(relative: &str) -> Option<String> {
    fixture_candidates(relative)
        .into_iter()
        .find_map(|path| fs::read_to_string(path).ok())
}

fn fixture_candidates(relative: &str) -> Vec<PathBuf> {
    let root = env::var("DTOKEN_FIXTURE_ROOT").ok();
    let mut candidates = Vec::new();
    if let Some(root) = root {
        candidates.push(PathBuf::from(root).join(relative));
    }
    candidates.push(PathBuf::from("../support/fixtures").join(relative));
    candidates.push(PathBuf::from("support/fixtures").join(relative));
    candidates
}
