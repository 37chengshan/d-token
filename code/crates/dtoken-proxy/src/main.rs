use anyhow::Result;
use clap::Parser;
use dtoken_config::DtokenConfig;
use dtoken_core_api::{build_router, AppState};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use tokio::net::TcpListener;

#[derive(Debug, Parser)]
#[command(name = "dtoken-proxy")]
#[command(about = "d-token local proxy and core API skeleton")]
struct Cli {
    #[arg(long, default_value = "127.0.0.1")]
    host: String,

    #[arg(long, default_value_t = 18001)]
    port: u16,

    #[arg(long)]
    config: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();
    let config = match cli.config.as_deref() {
        Some(path) => DtokenConfig::load_json(path)?,
        None => DtokenConfig::empty(),
    };

    let host: IpAddr = cli.host.parse().unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST));

    if host != IpAddr::V4(Ipv4Addr::LOCALHOST) {
        tracing::warn!(%host, "M1 only supports local-first binding by default");
    }

    let addr = SocketAddr::new(host, cli.port);
    let listener = TcpListener::bind(addr).await?;
    tracing::info!(%addr, "d-token M1 proxy/core-api listening");

    let state = AppState::new(config);
    axum::serve(listener, build_router(state)).await?;
    Ok(())
}
