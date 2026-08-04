use anyhow::Result;
use clap::Parser;
use dtoken_config::DtokenConfig;
use dtoken_core_api::{build_router, AppState, RuntimeConfigStatus};
use std::{
    net::{IpAddr, SocketAddr},
    path::PathBuf,
};
use tokio::net::TcpListener;

#[derive(Debug, Parser)]
#[command(name = "dtoken-proxy")]
#[command(about = "d-token local proxy and core API skeleton")]
struct Cli {
    /// Run the d-token-owned MCP server on stdio instead of binding the HTTP
    /// proxy listener. The managed launcher may provide a retrieval capability
    /// through its private process environment.
    #[arg(long)]
    mcp_stdio: bool,

    /// Private d-token-owned SharedContext capability document. The file is
    /// hash-bound to the exact route/model and is never emitted through MCP.
    #[arg(long, requires = "mcp_stdio")]
    mcp_memory_capability_file: Option<String>,

    #[arg(long, requires = "mcp_memory_capability_file")]
    mcp_route_id: Option<String>,

    #[arg(long, requires = "mcp_memory_capability_file")]
    mcp_model: Option<String>,

    #[arg(long, default_value = "127.0.0.1")]
    host: String,

    #[arg(long, default_value_t = 18001)]
    port: u16,

    #[arg(long)]
    config: Option<String>,

    #[arg(long)]
    data_dir: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();
    let (config, config_status) = load_runtime_config(cli.config.as_deref())?;

    if cli.mcp_stdio {
        let state = AppState::new_persistent_with_config_status(
            config,
            config_status,
            runtime_data_dir(cli.data_dir.as_deref()),
        );
        if let Some(path) = cli.mcp_memory_capability_file.as_deref() {
            let route_id = cli
                .mcp_route_id
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("--mcp-route-id is required"))?;
            let model = cli
                .mcp_model
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("--mcp-model is required"))?;
            let capability = state.resolve_memory_capability_for_route(
                &PathBuf::from(path),
                route_id,
                model,
                dtoken_types::time::now_ms(),
            )?;
            return dtoken_core_api::mcp::serve_stdio_with_memory_capability(state, capability)
                .await
                .map_err(Into::into);
        }
        return dtoken_core_api::mcp::serve_stdio(state)
            .await
            .map_err(Into::into);
    }

    let host = parse_loopback_host(&cli.host)?;

    let addr = SocketAddr::new(host, cli.port);
    let listener = TcpListener::bind(addr).await?;
    tracing::info!(%addr, "d-token M1 proxy/core-api listening");

    let state = AppState::new_persistent_with_config_status(
        config,
        config_status,
        runtime_data_dir(cli.data_dir.as_deref()),
    );
    axum::serve(listener, build_router(state)).await?;
    Ok(())
}

fn parse_loopback_host(value: &str) -> Result<IpAddr> {
    let host = value.parse::<IpAddr>().map_err(|_| {
        anyhow::anyhow!(
            "d-token bind host must be a loopback IP address (127.0.0.1 or ::1); got {value}"
        )
    })?;
    if !host.is_loopback() {
        anyhow::bail!(
            "d-token bind host must be a loopback IP address (127.0.0.1 or ::1); got {value}"
        );
    }
    Ok(host)
}

fn runtime_data_dir(override_dir: Option<&str>) -> PathBuf {
    if let Some(dir) = override_dir {
        return PathBuf::from(dir);
    }
    if let Ok(dir) = std::env::var("DTOKEN_DATA_DIR") {
        return PathBuf::from(dir);
    }
    if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
        return PathBuf::from(local_app_data)
            .join("d-token")
            .join("runtime");
    }
    if let Ok(home) = std::env::var("USERPROFILE").or_else(|_| std::env::var("HOME")) {
        return PathBuf::from(home).join(".dtoken").join("runtime");
    }
    PathBuf::from(".dtoken").join("runtime")
}

fn env_config_path() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("DTOKEN_CONFIG") {
        return Some(PathBuf::from(path));
    }
    None
}

fn default_active_config_path() -> Option<PathBuf> {
    if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
        return Some(
            PathBuf::from(local_app_data)
                .join("d-token")
                .join("active-config")
                .join("dtoken-config.json"),
        );
    }
    std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .ok()
        .map(|home| {
            PathBuf::from(home)
                .join(".dtoken")
                .join("active-config")
                .join("dtoken-config.json")
        })
}

fn load_runtime_config(explicit: Option<&str>) -> Result<(DtokenConfig, RuntimeConfigStatus)> {
    if let Some(path) = explicit {
        tracing::info!(config_path = %path, "loading explicit d-token config");
        let config = DtokenConfig::load_json(path)?;
        let status = RuntimeConfigStatus::from_config(
            "explicit_cli_config",
            Some(path.to_string()),
            &config,
        );
        return Ok((config, status));
    }

    if let Some(path) = env_config_path() {
        tracing::info!(config_path = %path.display(), "loading DTOKEN_CONFIG d-token config");
        let config = DtokenConfig::load_json(&path)?;
        let status = RuntimeConfigStatus::from_config(
            "env_dtoken_config",
            Some(path.display().to_string()),
            &config,
        );
        return Ok((config, status));
    }

    let Some(path) = default_active_config_path() else {
        tracing::info!("no d-token active config path is available; starting with empty config");
        return Ok((
            DtokenConfig::empty(),
            RuntimeConfigStatus::missing("no_config_path", None),
        ));
    };
    if path.exists() {
        tracing::info!(config_path = %path.display(), "loading d-token active config");
        let config = DtokenConfig::load_json(&path)?;
        let status = RuntimeConfigStatus::from_config(
            "desktop_active_config",
            Some(path.display().to_string()),
            &config,
        );
        Ok((config, status))
    } else {
        tracing::info!(config_path = %path.display(), "d-token active config not found; starting with empty config");
        Ok((
            DtokenConfig::empty(),
            RuntimeConfigStatus::missing(
                "desktop_active_config_missing",
                Some(path.display().to_string()),
            ),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loopback_host_validation_accepts_ipv4_and_ipv6_loopback() {
        assert_eq!(
            parse_loopback_host("127.0.0.1").expect("IPv4 loopback should be accepted"),
            "127.0.0.1".parse::<IpAddr>().unwrap()
        );
        assert_eq!(
            parse_loopback_host("::1").expect("IPv6 loopback should be accepted"),
            "::1".parse::<IpAddr>().unwrap()
        );
    }

    #[test]
    fn loopback_host_validation_rejects_non_loopback_and_invalid_hosts() {
        for host in ["0.0.0.0", "192.168.1.10", "8.8.8.8", "not-an-ip"] {
            let error = parse_loopback_host(host).expect_err("host must fail closed");
            assert!(error.to_string().contains("loopback"));
        }
    }

    #[test]
    fn explicit_data_dir_wins() {
        assert_eq!(
            runtime_data_dir(Some("D:/tmp/dtoken-data")),
            PathBuf::from("D:/tmp/dtoken-data")
        );
    }

    #[test]
    fn explicit_config_path_is_loaded() {
        let dir = std::env::temp_dir().join(format!("dtoken-config-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("config.json");
        std::fs::write(&path, r#"{"providers":[],"routes":[],"instances":[]}"#)
            .expect("write config");

        let (loaded, status) =
            load_runtime_config(Some(path.to_str().unwrap())).expect("load config");
        assert!(loaded.providers.is_empty());
        assert_eq!(status.source, "explicit_cli_config");
        assert!(status.loaded);
        assert!(status.exists);

        let _ = std::fs::remove_dir_all(dir);
    }
}
