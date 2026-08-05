use dtoken_types::{
    CcrCacheSummary, HeadroomMetricsReport, HeadroomStats, HeadroomStatus, RuntimeMode, ToolStatus,
};
use std::{
    env, fs,
    io::{Read, Write},
    net::TcpStream,
    path::PathBuf,
    time::Duration,
};
use tokio::process::Command;

#[derive(Debug, Clone, Default)]
pub struct HeadroomAdapter {
    pub binary_path: Option<String>,
    pub proxy_url: Option<String>,
}

impl HeadroomAdapter {
    pub fn new(binary_path: Option<String>, proxy_url: Option<String>) -> Self {
        Self {
            binary_path,
            proxy_url,
        }
    }

    pub async fn detect() -> HeadroomStatus {
        let mode = runtime_mode_from_env();
        if mode == RuntimeMode::Fixture {
            return fixture_status();
        }
        if mode == RuntimeMode::Native {
            return native_unavailable_status();
        }

        let adapter = Self::default();
        let proxy_running = read_headroom_http("/health").is_some();
        match adapter.run_version().await {
            Some(version) => HeadroomStatus {
                status: ToolStatus::Found,
                binary_path: Some(adapter.binary_name()),
                version: Some(version),
                python_runtime: None,
                proxy_running,
                mcp_configured: false,
                warnings: vec![format!(
                    "{mode:?} mode: Headroom CLI/version detected; HTTP status is read-only and only used when DTOKEN_HEADROOM_BASE_URL is set"
                )],
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
        let mode = runtime_mode_from_env();
        if mode == RuntimeMode::Fixture {
            return fixture_stats();
        }
        if mode == RuntimeMode::Native {
            return HeadroomStats {
                available: false,
                compressions: 0,
                retrievals: 0,
                tokens_saved: 0,
                savings_percent: 0.0,
                source: "native_unavailable".to_string(),
            };
        }

        if let Some(raw) = read_headroom_http("/stats") {
            if let Ok(stats) = serde_json::from_str::<HeadroomStats>(&raw) {
                return stats;
            }
        }

        let available = self.run_version().await.is_some();
        HeadroomStats {
            available,
            compressions: 0,
            retrievals: 0,
            tokens_saved: 0,
            savings_percent: 0.0,
            source: if available {
                format!("{mode:?}_cli_detected_no_stats").to_lowercase()
            } else {
                format!("{mode:?}_missing_headroom").to_lowercase()
            },
        }
    }

    pub fn cache_summary_mock() -> CcrCacheSummary {
        let mode = runtime_mode_from_env();
        if mode == RuntimeMode::Fixture {
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
            note: if mode == RuntimeMode::Native {
                "external Headroom CCR is unavailable in native mode".to_string()
            } else {
                "external Headroom CCR raw cache is not read; summary only".to_string()
            },
        }
    }

    pub async fn metrics_report(&self) -> HeadroomMetricsReport {
        let mode = runtime_mode_from_env();
        if mode == RuntimeMode::Fixture {
            return HeadroomMetricsReport {
                available: true,
                source: "fixture".to_string(),
                prometheus_text: read_fixture_text("headroom/metrics.txt").unwrap_or_default(),
            };
        }
        if mode == RuntimeMode::Native {
            return HeadroomMetricsReport {
                available: false,
                source: "native_unavailable".to_string(),
                prometheus_text: String::new(),
            };
        }

        HeadroomMetricsReport {
            available: read_headroom_http("/metrics").is_some(),
            source: "external_read_only_http".to_string(),
            prometheus_text: read_headroom_http("/metrics").unwrap_or_default(),
        }
    }

    pub fn stats_history_json() -> serde_json::Value {
        let mode = runtime_mode_from_env();
        if mode == RuntimeMode::Native {
            return serde_json::json!({
                "status": "unavailable",
                "history": [],
                "series": { "hourly": [], "daily": [], "weekly": [], "monthly": [] }
            });
        }
        if mode != RuntimeMode::Fixture {
            if let Some(raw) = read_headroom_http("/stats-history") {
                if let Ok(value) = serde_json::from_str(&raw) {
                    return value;
                }
            }
        }

        read_fixture_text("headroom/stats-history.json")
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_else(|| {
                serde_json::json!({
                    "status": "fixture_missing",
                    "history": [],
                    "series": { "hourly": [], "daily": [], "weekly": [], "monthly": [] }
                })
            })
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

    fn binary_name(&self) -> String {
        self.binary_path
            .clone()
            .unwrap_or_else(|| "headroom".to_string())
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

pub async fn headroom_metrics_report() -> HeadroomMetricsReport {
    HeadroomAdapter::default().metrics_report().await
}

pub fn headroom_stats_history_json() -> serde_json::Value {
    HeadroomAdapter::stats_history_json()
}

fn runtime_mode_from_env() -> RuntimeMode {
    let profile = env::var("DTOKEN_PROFILE").ok();
    RuntimeMode::from_profile(profile.as_deref(), cfg!(debug_assertions))
}

fn native_unavailable_status() -> HeadroomStatus {
    HeadroomStatus {
        status: ToolStatus::Missing,
        binary_path: None,
        version: None,
        python_runtime: None,
        proxy_running: false,
        mcp_configured: false,
        warnings: vec![
            "external Headroom is unavailable in native mode and is not a d-token runtime dependency"
                .to_string(),
        ],
    }
}

fn read_headroom_http(path: &str) -> Option<String> {
    let base = env::var("DTOKEN_HEADROOM_BASE_URL").ok()?;
    let target = parse_http_url(&base, path)?;
    simple_http_get(&target.host, target.port, &target.path).ok()
}

struct HttpTarget {
    host: String,
    port: u16,
    path: String,
}

fn parse_http_url(base: &str, path: &str) -> Option<HttpTarget> {
    let without_scheme = base.strip_prefix("http://")?;
    let authority = without_scheme.split('/').next().unwrap_or(without_scheme);
    let (host, port) = match authority.rsplit_once(':') {
        Some((host, port)) => (host.to_string(), port.parse().ok()?),
        None => (authority.to_string(), 80),
    };
    Some(HttpTarget {
        host,
        port,
        path: path.to_string(),
    })
}

fn simple_http_get(host: &str, port: u16, path: &str) -> std::io::Result<String> {
    let mut stream = TcpStream::connect((host, port))?;
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    stream.set_write_timeout(Some(Duration::from_secs(2)))?;
    write!(
        stream,
        "GET {path} HTTP/1.1\r\nHost: {host}:{port}\r\nConnection: close\r\n\r\n"
    )?;

    let mut response = String::new();
    stream.read_to_string(&mut response)?;
    let (_, body) = response.split_once("\r\n\r\n").unwrap_or(("", &response));
    Ok(body.to_string())
}

fn fixture_status() -> HeadroomStatus {
    let version = read_fixture_text("headroom/version.txt")
        .unwrap_or_else(|| "headroom 0.0.0-fixture".to_string());
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};

    fn environment_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
            .lock()
            .expect("Headroom test environment lock")
    }

    #[test]
    fn missing_or_unknown_profile_defaults_to_native_and_fixture_is_explicit() {
        let _guard = environment_lock();
        env::remove_var("DTOKEN_PROFILE");
        assert_eq!(runtime_mode_from_env(), RuntimeMode::Native);

        env::set_var("DTOKEN_PROFILE", "fixture");
        assert_eq!(
            runtime_mode_from_env(),
            if cfg!(debug_assertions) {
                RuntimeMode::Fixture
            } else {
                RuntimeMode::Native
            }
        );

        env::set_var("DTOKEN_PROFILE", "unexpected-profile");
        assert_eq!(runtime_mode_from_env(), RuntimeMode::Native);
        env::remove_var("DTOKEN_PROFILE");
    }

    #[test]
    fn native_history_is_unavailable_instead_of_falling_back_to_fixture() {
        let _guard = environment_lock();
        env::remove_var("DTOKEN_PROFILE");
        let value = HeadroomAdapter::stats_history_json();
        assert_eq!(value["status"], "unavailable");
        assert_eq!(value["history"], serde_json::json!([]));
    }

    #[test]
    fn fixture_history_has_required_top_level_shape() {
        let _guard = environment_lock();
        env::set_var("DTOKEN_PROFILE", "fixture");
        let value = HeadroomAdapter::stats_history_json();
        assert!(value.get("history").is_some());
        assert!(value.get("series").is_some());
        env::remove_var("DTOKEN_PROFILE");
    }

    #[test]
    fn http_url_parser_accepts_localhost_base_url_without_touching_service() {
        let parsed = parse_http_url("http://127.0.0.1:8787", "/stats").expect("valid url");
        assert_eq!(parsed.host, "127.0.0.1");
        assert_eq!(parsed.port, 8787);
        assert_eq!(parsed.path, "/stats");
    }
}
