use dtoken_types::{CcrCacheSummary, HeadroomStats, HeadroomStatus, ToolStatus};
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
