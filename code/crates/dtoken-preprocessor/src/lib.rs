use dtoken_types::{
    HookStatus, IntegrityState, PreprocessorImplementation, PreprocessorProfile, RtkDiscoverReport,
    RtkGainReport, RtkSessionReport, RtkStatus, RuntimeMode, TelemetryStatus, ToolStatus,
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
        let mode = runtime_mode_from_env();
        if mode == RuntimeMode::Fixture {
            return fixture_status("fixture");
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
                warnings: vec![
                    "M1 only detects RTK binary/version; hook and analytics import are skeletons"
                        .to_string(),
                ],
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
        let mode = runtime_mode_from_env();
        if mode == RuntimeMode::Fixture {
            return fixture_gain_report();
        }

        match self
            .run_json_command(&["gain", "--all", "--format", "json"])
            .await
        {
            Some(raw) => {
                serde_json::from_str::<RtkGainReport>(&raw).unwrap_or_else(|_| RtkGainReport {
                    available: true,
                    total_commands: 0,
                    input_tokens: 0,
                    output_tokens: 0,
                    saved_tokens: 0,
                    savings_percent: 0.0,
                    source: format!("{:?}_unparsed_gain", mode).to_lowercase(),
                })
            }
            None => RtkGainReport {
                available: false,
                total_commands: 0,
                input_tokens: 0,
                output_tokens: 0,
                saved_tokens: 0,
                savings_percent: 0.0,
                source: format!("{:?}_missing_rtk", mode).to_lowercase(),
            },
        }
    }

    pub async fn discover_report(&self) -> RtkDiscoverReport {
        if runtime_mode_from_env() == RuntimeMode::Fixture {
            return fixture_discover_report();
        }

        match self.run_text_command(&["discover", "--all"]).await {
            Some(raw) => parse_discover_text("external", &raw),
            None => RtkDiscoverReport {
                available: false,
                source: "external_missing_rtk".to_string(),
                missed_opportunities: vec![],
                raw_summary: None,
            },
        }
    }

    pub async fn session_report(&self) -> RtkSessionReport {
        if runtime_mode_from_env() == RuntimeMode::Fixture {
            return fixture_session_report();
        }

        match self.run_text_command(&["session"]).await {
            Some(raw) => parse_session_text("external", &raw),
            None => RtkSessionReport {
                available: false,
                source: "external_missing_rtk".to_string(),
                sessions: vec![],
                raw_summary: None,
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
        let output = Command::new(self.binary_name())
            .arg("--version")
            .output()
            .await
            .ok()?;
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

    async fn run_json_command(&self, args: &[&str]) -> Option<String> {
        self.run_text_command(args).await
    }

    async fn run_text_command(&self, args: &[&str]) -> Option<String> {
        let output = Command::new(self.binary_name())
            .args(args)
            .output()
            .await
            .ok()?;
        if !output.status.success() {
            return None;
        }
        Some(String::from_utf8_lossy(&output.stdout).to_string())
    }

    fn binary_name(&self) -> String {
        self.binary_path
            .clone()
            .unwrap_or_else(|| "rtk".to_string())
    }
}

pub async fn detect_rtk_status() -> RtkStatus {
    RtkAdapter::detect().await
}

pub async fn rtk_gain_mock() -> RtkGainReport {
    RtkAdapter::default().gain_report().await
}

pub async fn rtk_discover_report() -> RtkDiscoverReport {
    RtkAdapter::default().discover_report().await
}

pub async fn rtk_session_report() -> RtkSessionReport {
    RtkAdapter::default().session_report().await
}

pub async fn list_preprocessors() -> Vec<PreprocessorProfile> {
    let status = detect_rtk_status().await;
    vec![RtkAdapter::profile_from_status(&status)]
}

fn runtime_mode_from_env() -> RuntimeMode {
    match env::var("DTOKEN_PROFILE").ok().as_deref() {
        Some("fake-binary") | Some("fake_binary") => RuntimeMode::FakeBinary,
        Some("shadow") => RuntimeMode::Shadow,
        Some("external") => RuntimeMode::External,
        Some("managed") => RuntimeMode::Managed,
        Some("builtin-fallback") | Some("builtin_fallback") => RuntimeMode::BuiltinFallback,
        _ => RuntimeMode::Fixture,
    }
}

fn fixture_status(source: &str) -> RtkStatus {
    let version =
        read_fixture_text("rtk/version.txt").unwrap_or_else(|| "rtk 0.0.0-fixture".to_string());
    RtkStatus {
        status: ToolStatus::Found,
        binary_path: Some("support/fake-bin/rtk".to_string()),
        version: Some(version.trim().to_string()),
        install_method: Some(source.to_string()),
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

fn fixture_discover_report() -> RtkDiscoverReport {
    let raw = read_fixture_text("rtk/discover.txt").unwrap_or_default();
    parse_discover_text("fixture", &raw)
}

fn fixture_session_report() -> RtkSessionReport {
    let raw = read_fixture_text("rtk/session.txt").unwrap_or_default();
    parse_session_text("fixture", &raw)
}

fn parse_discover_text(source: &str, raw: &str) -> RtkDiscoverReport {
    let missed_opportunities = raw
        .lines()
        .filter_map(|line| line.trim().strip_prefix("- "))
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    RtkDiscoverReport {
        available: true,
        source: source.to_string(),
        missed_opportunities,
        raw_summary: Some(raw.to_string()),
    }
}

fn parse_session_text(source: &str, raw: &str) -> RtkSessionReport {
    let sessions = raw
        .lines()
        .filter_map(|line| line.trim().strip_prefix("- "))
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    RtkSessionReport {
        available: true,
        source: source.to_string(),
        sessions,
        raw_summary: Some(raw.to_string()),
    }
}

fn read_fixture_text(relative: &str) -> Option<String> {
    fixture_candidates(relative)
        .into_iter()
        .find_map(|path| fs::read_to_string(path).ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discover_fixture_text_extracts_missed_opportunities() {
        let report = parse_discover_text("fixture", "- cargo test --workspace\nsource=fixture");
        assert_eq!(report.source, "fixture");
        assert_eq!(report.missed_opportunities, vec!["cargo test --workspace"]);
    }

    #[test]
    fn unknown_profile_defaults_to_fixture_for_no_touch_development() {
        env::remove_var("DTOKEN_PROFILE");
        assert_eq!(runtime_mode_from_env(), RuntimeMode::Fixture);
    }
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
