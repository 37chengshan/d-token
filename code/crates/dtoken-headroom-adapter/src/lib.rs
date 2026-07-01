use dtoken_types::{CcrCacheSummary, HeadroomStats, HeadroomStatus, ToolStatus};
use std::{env, fs, path::PathBuf};
use tokio::process::Command;

#[derive(Debug, Clone, Default)]
pub struct HeadroomAdapter {
    pub binary_path: Option<String>,
    pub proxy_url: Option<String>,
}

impl HeadroomAdapter {
    pub fn new(binary_path: Option<String>, proxy_url: Option<String>) -> Self {
        Self { binary_path, proxy_url }
    }

    pub async fn detect() -> HeadroomStatus {
        if is_fixture_profile() {
            return fixture_status();
        }

        let adapter = Self::default();
        match adapter.run_version().await {
            Some(version) => HeadroomStatus {
                status: ToolStatus::Found,
                binary_path: Some(adapter.binary_name()),
                version: Some(version),
                python_runtime: None,
                proxy_running: false,
                mcp_configured: false,
                warnings: vec!["M1 only detects Headroom CLI/version; proxy/MCP checks are skeletons".to_string()],
            },
            None => HeadroomStatus {
                status: ToolStatus::Missing,
                binary_path: None,
                version: None,
                python_runtime: None,
                proxy_running: false,
                mcp_configured: false,
                warnings: vec!["headroom CLI was not found in PATH".to_string()],
            },
        }
    }

    pub async fn stats_mock(&self) -> HeadroomStats {
        if is_fixture_profile() {
            return fixture_stats();
        }

        let available = self.run_version().await.is_some();
        HeadroomStats {
            available,
            compressions: 0,
            retrievals: 0,
            tokens_saved: 0,
            savings_percent: 0.0,
            source: if available {
                "m1_mock_after_cli_detection".to_string()
            } else {
                "m1_mock_missing_headroom".to_string()
            },
        }
    }

    pub fn cache_summary_mock() -> CcrCacheSummary {
        if is_fixture_profile() {
            return CcrCacheSummary {
                available: true,
                entries: 24,
                bytes: 1048576,
                retrievals: 3,
                note: "fixture mode: summary only; raw CCR cache was not read".to_string(),
            };
        }

        CcrCacheSummary {
            available: false,
            entries: 0,
            bytes: 0,
            retrievals: 0,
            note: "M1 does not read Headroom CCR raw cache; summary only".to_string(),
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
        self.binary_path.clone().unwrap_or_else(|| "headroom".to_string())
    }
}

pub async fn detect_headroom_status() -> HeadroomStatus {
    HeadroomAdapter::detect().await
}

pub async fn headroom_stats_mock() -> HeadroomStats {
    HeadroomAdapter::default().stats_mock().await
}

pub fn headroom_cache_summary_mock() -> CcrCacheSummary {
    HeadroomAdapter::cache_summary_mock()
}

fn is_fixture_profile() -> bool {
    matches!(env::var("DTOKEN_PROFILE").ok().as_deref(), Some("fixture"))
}

fn fixture_status() -> HeadroomStatus {
    let version = read_fixture_text("headroom/version.txt").unwrap_or_else(|| "headroom 0.0.0-fixture".to_string());
    HeadroomStatus {
        status: ToolStatus::Found,
        binary_path: Some("support/fake-bin/headroom".to_string()),
        version: Some(version.trim().to_string()),
        python_runtime: Some("fixture-python".to_string()),
        proxy_running: true,
        mcp_configured: true,
        warnings: vec!["fixture mode: real Headroom was not called".to_string()],
    }
}

fn fixture_stats() -> HeadroomStats {
    read_fixture_text("headroom/stats.json")
        .and_then(|raw| serde_json::from_str::<HeadroomStats>(&raw).ok())
        .unwrap_or(HeadroomStats {
            available: true,
            compressions: 0,
            retrievals: 0,
            tokens_saved: 0,
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
