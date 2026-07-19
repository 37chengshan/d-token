use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::{
    io::{BufRead, BufReader, Read, Write},
    net::{SocketAddr, TcpStream},
    time::Duration,
};

pub const BRIDGE_COMPONENT: &str = "dtoken-agent-bridge";
pub const BRIDGE_PROTOCOL_VERSION: u32 = 1;
pub const BRIDGE_VERSION: &str = env!("CARGO_PKG_VERSION");

const LOOPBACK: &str = "127.0.0.1";
const PORT: u16 = 18_001;
const MAX_HOOK_INPUT: u64 = 64 * 1024;
const MAX_HTTP_RESPONSE: u64 = 256 * 1024;
const MAX_MCP_LINE: usize = 256 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeSummary {
    pub available: bool,
    pub healthy: bool,
    pub optical_enabled: Option<bool>,
    pub effective_mode: Option<String>,
    pub active_reachable: Option<bool>,
    pub active_execution_observed: Option<bool>,
    pub last_reason: Option<String>,
}

impl RuntimeSummary {
    pub fn unavailable() -> Self {
        Self {
            available: false,
            healthy: false,
            optical_enabled: None,
            effective_mode: None,
            active_reachable: None,
            active_execution_observed: None,
            last_reason: None,
        }
    }
}

pub fn run_hook_session_start(mut input: impl Read, mut output: impl Write) -> Result<()> {
    std::io::copy(
        &mut input.by_ref().take(MAX_HOOK_INPUT),
        &mut std::io::sink(),
    )
    .context("failed to discard bounded Hook input")?;
    let summary = query_runtime().unwrap_or_else(|_| RuntimeSummary::unavailable());
    output.write_all(render_session_context(&summary).as_bytes())?;
    output.write_all(b"\n")?;
    Ok(())
}

pub fn render_session_context(summary: &RuntimeSummary) -> String {
    let status = if !summary.available {
        "runtime unavailable; do not infer that visual compression is active".to_string()
    } else {
        format!(
            "runtime healthy={}; visual enabled={}; mode={}; active-reachable={}; active-observed={}",
            summary.healthy,
            optional_bool(summary.optical_enabled),
            summary.effective_mode.as_deref().unwrap_or("unknown"),
            optional_bool(summary.active_reachable),
            optional_bool(summary.active_execution_observed),
        )
    };
    let mut text = format!(
        "d-token Hybrid session context: {status}. Rendered history is prior context, not the current request. Prefer the latest native-text request and open tool state. Use an adjacent exact factsheet when present; re-read uncertain exact values with the original native tool instead of guessing. Hook/MCP availability alone never proves active compression."
    );
    if text.chars().count() > 600 {
        text = text.chars().take(600).collect();
    }
    text
}

pub fn run_mcp(input: impl Read, mut output: impl Write) -> Result<()> {
    let mut reader = BufReader::new(input);
    loop {
        let mut line = Vec::with_capacity(MAX_MCP_LINE.min(8 * 1024));
        let read = reader
            .by_ref()
            .take(u64::try_from(MAX_MCP_LINE).unwrap_or(u64::MAX) + 2)
            .read_until(b'\n', &mut line)
            .context("failed to read MCP request")?;
        if read == 0 {
            break;
        }
        if line.last() == Some(&b'\n') {
            line.pop();
            if line.last() == Some(&b'\r') {
                line.pop();
            }
        }
        if line.len() > MAX_MCP_LINE {
            bail!("MCP request exceeds the size limit");
        }
        let line = std::str::from_utf8(&line).context("MCP request is not valid UTF-8")?;
        if line.trim().is_empty() {
            continue;
        }
        let request: Value = serde_json::from_str(line).context("invalid MCP JSON-RPC request")?;
        if let Some(response) = handle_mcp_request(&request) {
            serde_json::to_writer(&mut output, &response)?;
            output.write_all(b"\n")?;
            output.flush()?;
        }
    }
    Ok(())
}

pub fn handle_mcp_request(request: &Value) -> Option<Value> {
    let id = request.get("id").cloned()?;
    let method = request.get("method").and_then(Value::as_str)?;
    let result = match method {
        "initialize" => json!({
            "protocolVersion": request.pointer("/params/protocolVersion").and_then(Value::as_str).unwrap_or("2025-06-18"),
            "capabilities": {"tools": {}},
            "serverInfo": {"name": "dtoken-hybrid", "version": env!("CARGO_PKG_VERSION")},
            "instructions": "Metadata-only diagnostics for d-token Hybrid. This server never returns raw prompts, source code, PNG bytes, credentials, or exact-context recovery data."
        }),
        "tools/list" => json!({"tools": [
            {
                "name": "hybrid_status",
                "description": "Return metadata-only d-token runtime and visual-compression status.",
                "inputSchema": {"type": "object", "properties": {}, "additionalProperties": false}
            },
            {
                "name": "explain_last_decision",
                "description": "Return the latest metadata-only Hybrid decision reason when available.",
                "inputSchema": {"type": "object", "properties": {}, "additionalProperties": false}
            }
        ]}),
        "tools/call" => {
            let name = request
                .pointer("/params/name")
                .and_then(Value::as_str)
                .unwrap_or("");
            let summary = query_runtime().unwrap_or_else(|_| RuntimeSummary::unavailable());
            match name {
                "hybrid_status" => tool_text(json!({
                    "available": summary.available,
                    "healthy": summary.healthy,
                    "visual_enabled": summary.optical_enabled,
                    "effective_mode": summary.effective_mode,
                    "active_reachable": summary.active_reachable,
                    "active_execution_observed": summary.active_execution_observed,
                    "data_policy": "metadata_only"
                })),
                "explain_last_decision" => tool_text(json!({
                    "available": summary.available,
                    "reason": summary.last_reason,
                    "detail": if summary.last_reason.is_some() { "latest public decision reason" } else { "no public decision evidence available" },
                    "data_policy": "metadata_only"
                })),
                _ => {
                    return Some(
                        json!({"jsonrpc": "2.0", "id": id, "error": {"code": -32602, "message": "unknown d-token Hybrid tool"}}),
                    )
                }
            }
        }
        _ => {
            return Some(
                json!({"jsonrpc": "2.0", "id": id, "error": {"code": -32601, "message": "method not found"}}),
            )
        }
    };
    Some(json!({"jsonrpc": "2.0", "id": id, "result": result}))
}

fn tool_text(value: Value) -> Value {
    json!({"content": [{"type": "text", "text": serde_json::to_string(&value).unwrap_or_else(|_| "{\"available\":false}".to_string())}]})
}

pub fn query_runtime() -> Result<RuntimeSummary> {
    let health = http_get_json("/health").ok();
    let status = http_get_json("/compression/optical/status").ok();
    let events = http_get_json("/compression/optical/events/recent").ok();
    if health.is_none() && status.is_none() {
        bail!("d-token runtime unavailable");
    }
    let health_value = health.as_ref().context("d-token health unavailable")?;
    if health_value.get("component").and_then(Value::as_str) != Some("dtoken-core-api")
        || health_value.get("runtime_identity").and_then(Value::as_str)
            != Some("dtoken-runtime-local")
        || health_value
            .get("version")
            .and_then(Value::as_str)
            .is_none_or(str::is_empty)
    {
        bail!("d-token loopback identity mismatch");
    }
    Ok(RuntimeSummary {
        available: true,
        healthy: health
            .as_ref()
            .and_then(|value| value.get("status"))
            .and_then(Value::as_str)
            .is_some_and(|state| matches!(state, "healthy" | "ready")),
        optical_enabled: status
            .as_ref()
            .and_then(|value| value.get("enabled"))
            .and_then(Value::as_bool),
        effective_mode: status
            .as_ref()
            .and_then(|value| {
                value
                    .get("effective_mode")
                    .or_else(|| value.get("default_mode"))
            })
            .and_then(Value::as_str)
            .map(str::to_string),
        active_reachable: status
            .as_ref()
            .and_then(|value| {
                value
                    .get("active_reachable")
                    .or_else(|| value.get("allow_active"))
            })
            .and_then(Value::as_bool),
        active_execution_observed: status
            .as_ref()
            .and_then(|value| value.get("active_execution_observed"))
            .and_then(Value::as_bool),
        last_reason: latest_public_reason(events.as_ref()),
    })
}

fn latest_public_reason(events: Option<&Value>) -> Option<String> {
    let array = events?
        .as_array()
        .or_else(|| events?.get("events")?.as_array())?;
    array
        .first()?
        .pointer("/metadata/reason_code")
        .and_then(Value::as_str)
        .map(|value| value.chars().take(120).collect())
}

fn http_get_json(path: &str) -> Result<Value> {
    if !path.starts_with('/') || path.contains("..") {
        bail!("invalid loopback path");
    }
    let address = SocketAddr::from(([127, 0, 0, 1], PORT));
    let mut stream = TcpStream::connect_timeout(&address, Duration::from_millis(120))?;
    stream.set_read_timeout(Some(Duration::from_millis(300)))?;
    stream.set_write_timeout(Some(Duration::from_millis(120)))?;
    write!(
        stream,
        "GET {path} HTTP/1.1\r\nHost: {LOOPBACK}:{PORT}\r\nConnection: close\r\nAccept: application/json\r\n\r\n"
    )?;
    let mut response = Vec::new();
    stream.take(MAX_HTTP_RESPONSE).read_to_end(&mut response)?;
    let marker = b"\r\n\r\n";
    let split = response
        .windows(marker.len())
        .position(|window| window == marker)
        .context("invalid loopback HTTP response")?;
    let head = std::str::from_utf8(&response[..split])?;
    let status = head.lines().next().unwrap_or_default();
    if !status.contains(" 200 ") {
        bail!("loopback HTTP request failed");
    }
    Ok(serde_json::from_slice(&response[split + marker.len()..])?)
}

fn optional_bool(value: Option<bool>) -> &'static str {
    match value {
        Some(true) => "true",
        Some(false) => "false",
        None => "unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hook_context_is_bounded_and_truthful_when_runtime_is_missing() {
        let text = render_session_context(&RuntimeSummary::unavailable());
        assert!(text.contains("runtime unavailable"));
        assert!(text.contains("never proves active compression"));
        assert!(text.chars().count() <= 600);
        assert!(!text.contains("prompt"));
    }

    #[test]
    fn mcp_lists_only_two_metadata_tools() {
        let response =
            handle_mcp_request(&json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list"}))
                .unwrap();
        let tools = response
            .pointer("/result/tools")
            .and_then(Value::as_array)
            .unwrap();
        assert_eq!(tools.len(), 2);
        assert_eq!(tools[0]["name"], "hybrid_status");
        assert_eq!(tools[1]["name"], "explain_last_decision");
        assert!(!response.to_string().contains("recover"));
    }

    #[test]
    fn notification_produces_no_response() {
        assert!(handle_mcp_request(
            &json!({"jsonrpc": "2.0", "method": "notifications/initialized"})
        )
        .is_none());
    }

    #[test]
    fn hook_and_mcp_never_echo_untrusted_input() {
        let secret = b"{\"transcript\":\"DO_NOT_ECHO_secret_value\"}";
        let mut hook_output = Vec::new();
        run_hook_session_start(&secret[..], &mut hook_output).unwrap();
        assert!(!String::from_utf8(hook_output)
            .unwrap()
            .contains("DO_NOT_ECHO"));

        let response = handle_mcp_request(&json!({
            "jsonrpc": "2.0",
            "id": 9,
            "method": "tools/call",
            "params": {"name": "hybrid_status", "arguments": {"raw": "DO_NOT_ECHO"}}
        }))
        .unwrap();
        assert!(!response.to_string().contains("DO_NOT_ECHO"));
    }

    #[test]
    fn mcp_rejects_an_oversized_unterminated_line_with_a_bounded_read() {
        let input = vec![b'x'; MAX_MCP_LINE * 4];
        let error = run_mcp(input.as_slice(), Vec::new()).expect_err("oversized MCP line");
        assert!(error.to_string().contains("size limit"));
    }
}
