use anyhow::{bail, Result};
use dtoken_codebase_memory::{
    run_hook_session_start, run_index_controlled, run_stdio_proxy, run_ui_host, LaunchConfig,
    COMPONENT, POLICY_VERSION, PROTOCOL_VERSION, VERSION,
};
use std::{io, path::PathBuf};

fn main() -> Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    match args.as_slice() {
        [command] if command == "identity" => {
            println!(
                "{}",
                serde_json::json!({
                    "component": COMPONENT,
                    "protocol_version": PROTOCOL_VERSION,
                    "policy_version": POLICY_VERSION,
                    "version": VERSION,
                    "arch": std::env::consts::ARCH,
                    "os": std::env::consts::OS,
                    "data_policy": "pass_through_no_persistence"
                })
            );
            Ok(())
        }
        [area, command] if area == "hook" && command == "session-start" => {
            run_hook_session_start(io::stdin().lock(), io::stdout().lock())
        }
        [area, transport, upstream, repository_root, cache_dir]
            if area == "mcp" && transport == "stdio" =>
        {
            let config = LaunchConfig::validate(
                PathBuf::from(upstream),
                PathBuf::from(repository_root),
                PathBuf::from(cache_dir),
            )?;
            run_stdio_proxy(&config, io::stdin().lock(), io::stdout().lock())
        }
        [command, upstream, repository_root, cache_dir, mode] if command == "index" => {
            let config = LaunchConfig::validate(
                PathBuf::from(upstream),
                PathBuf::from(repository_root),
                PathBuf::from(cache_dir),
            )?;
            let receipt = run_index_controlled(&config, mode, io::stdin())?;
            serde_json::to_writer(io::stdout().lock(), &receipt)?;
            println!();
            Ok(())
        }
        [command, upstream, repository_root, cache_dir, parent_pid]
            if command == "ui-host" =>
        {
            run_host(upstream, repository_root, cache_dir, parent_pid, None)
        }
        [command, upstream, repository_root, cache_dir, parent_pid, ttl_secs]
            if command == "ui-host" =>
        {
            run_host(
                upstream,
                repository_root,
                cache_dir,
                parent_pid,
                Some(ttl_secs),
            )
        }
        _ => bail!(
            "usage: dtoken-codebase-memory-launcher identity | mcp stdio <upstream> <approved-repository-root> <isolated-cache-dir> | index <upstream> <approved-repository-root> <isolated-cache-dir> <full|moderate|fast> | ui-host <upstream> <approved-repository-root> <isolated-cache-dir> <parent-pid> [ttl-secs]"
        ),
    }
}

fn run_host(
    upstream: &str,
    repository_root: &str,
    cache_dir: &str,
    parent_pid: &str,
    ttl_secs: Option<&String>,
) -> Result<()> {
    let config = LaunchConfig::validate(
        PathBuf::from(upstream),
        PathBuf::from(repository_root),
        PathBuf::from(cache_dir),
    )?;
    let parent_pid = parent_pid.parse::<u32>()?;
    let ttl_secs = ttl_secs.map(|value| value.parse::<u64>()).transpose()?;
    run_ui_host(
        config,
        io::stdin(),
        io::stdout().lock(),
        parent_pid,
        ttl_secs,
    )
}
