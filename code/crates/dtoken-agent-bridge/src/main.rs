use anyhow::{bail, Result};
use dtoken_agent_bridge::{
    run_hook_session_start, run_mcp, run_rtk_wrap, BRIDGE_COMPONENT, BRIDGE_PROTOCOL_VERSION,
    BRIDGE_VERSION,
};
use std::io;

fn main() -> Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    match args.as_slice() {
        [area, command, rest @ ..] if area == "rtk" && command == "wrap" => {
            let code = run_rtk_wrap(rest, io::stdout(), io::stderr())?;
            std::process::exit(code);
        }
        [area, command] if area == "hook" && command == "session-start" => {
            run_hook_session_start(io::stdin().lock(), io::stdout().lock())
        }
        [area, transport] if area == "mcp" && transport == "stdio" => {
            run_mcp(io::stdin().lock(), io::stdout().lock())
        }
        [command] if command == "identity" => {
            println!(
                "{}",
                serde_json::json!({
                    "component": BRIDGE_COMPONENT,
                    "protocol_version": BRIDGE_PROTOCOL_VERSION,
                    "version": BRIDGE_VERSION,
                    "arch": std::env::consts::ARCH,
                    "os": std::env::consts::OS,
                })
            );
            Ok(())
        }
        _ => bail!(
            "usage: dtoken-agent-bridge identity | hook session-start | mcp stdio | rtk wrap --managed-root <path> [--max-bytes <n>] -- <command> [args...]"
        ),
    }
}
