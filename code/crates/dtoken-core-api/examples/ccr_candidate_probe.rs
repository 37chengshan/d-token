use anyhow::{bail, Context, Result};
use dtoken_config::DtokenConfig;
use dtoken_core_api::{mcp::McpServer, AppState};
use serde_json::{json, Value};
use std::{env, path::PathBuf};

fn main() -> Result<()> {
    let args = env::args().skip(1).collect::<Vec<_>>();
    let [config_path, data_dir, recovery_ref] = args.as_slice() else {
        bail!("usage: ccr_candidate_probe CONFIG_PATH DATA_DIR RECOVERY_REF");
    };
    if recovery_ref.trim().is_empty() || !recovery_ref.starts_with("ccr:") {
        bail!("recovery ref is invalid");
    }

    let config = DtokenConfig::load_json(config_path).context("load candidate config")?;
    let state = AppState::new_persistent(config, PathBuf::from(data_dir));
    let capability = "dtoken-candidate-ccr-capability".repeat(2);
    let server = McpServer::new_with_retrieve_capability(state, Some(capability.clone()));

    let initialized = server
        .handle_jsonrpc(request(
            1,
            "initialize",
            json!({"protocolVersion": "2024-11-05"}),
        ))
        .context("MCP initialize produced no response")?;
    if initialized
        .pointer("/result/protocolVersion")
        .and_then(Value::as_str)
        != Some("2024-11-05")
    {
        bail!("MCP initialize protocol mismatch");
    }

    let listed = server
        .handle_jsonrpc(request(2, "tools/list", json!({})))
        .context("MCP tools/list produced no response")?;
    let tools = listed
        .pointer("/result/tools")
        .and_then(Value::as_array)
        .context("MCP tools list is unavailable")?;
    for required in ["headroom_retrieve", "headroom_delete"] {
        if !tools
            .iter()
            .any(|tool| tool.get("name").and_then(Value::as_str) == Some(required))
        {
            bail!("MCP required CCR tool is unavailable");
        }
    }

    let retrieved = server
        .handle_jsonrpc(tool_call(3, "headroom_retrieve", recovery_ref, &capability))
        .context("MCP retrieve produced no response")?;
    let retrieved = tool_payload(&retrieved)?;
    if retrieved.get("state").and_then(Value::as_str) != Some("retrieved") {
        bail!("MCP retrieve did not return a retrieved state");
    }
    let body = retrieved
        .get("body")
        .context("MCP retrieve body is absent")?;
    if !body.is_object() {
        bail!("MCP retrieve body is not the original JSON request object");
    }
    let retrieved_body_bytes = serde_json::to_vec(body)?.len();
    if retrieved_body_bytes == 0 {
        bail!("MCP retrieve body is empty");
    }

    let deleted = server
        .handle_jsonrpc(tool_call(4, "headroom_delete", recovery_ref, &capability))
        .context("MCP delete produced no response")?;
    let deleted = tool_payload(&deleted)?;
    if deleted.get("state").and_then(Value::as_str) != Some("deleted")
        || deleted.get("raw_available").and_then(Value::as_bool) != Some(false)
        || deleted.get("raw_returned").and_then(Value::as_bool) != Some(false)
    {
        bail!("MCP delete receipt is invalid");
    }

    let repeated = server
        .handle_jsonrpc(tool_call(5, "headroom_delete", recovery_ref, &capability))
        .context("MCP repeated delete produced no response")?;
    let repeated = tool_payload(&repeated)?;
    if repeated.get("state").and_then(Value::as_str) != Some("already_deleted") {
        bail!("MCP delete is not idempotent");
    }

    let unavailable = server
        .handle_jsonrpc(tool_call(6, "headroom_retrieve", recovery_ref, &capability))
        .context("MCP post-delete retrieve produced no response")?;
    let unavailable_text = unavailable
        .pointer("/result/content/0/text")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if unavailable
        .pointer("/result/isError")
        .and_then(Value::as_bool)
        != Some(true)
        || !unavailable_text.contains("recovery_unavailable")
    {
        bail!("MCP post-delete retrieve did not fail closed");
    }

    println!(
        "{}",
        json!({
            "source": "dtoken-owned-runtime",
            "initialized": true,
            "raw_available_before_delete": true,
            "retrieve_state": "retrieved",
            "retrieved_body_bytes": retrieved_body_bytes,
            "delete_state": "deleted",
            "repeat_delete_state": "already_deleted",
            "post_delete_retrieve": "recovery_unavailable",
            "raw_available_after_delete": false,
            "raw_returned": false,
            "capability_returned": false,
            "recovery_ref_returned": false
        })
    );
    Ok(())
}

fn request(id: u64, method: &str, params: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params})
}

fn tool_call(id: u64, name: &str, recovery_ref: &str, capability: &str) -> Value {
    request(
        id,
        "tools/call",
        json!({
            "name": name,
            "arguments": {
                "recovery_ref": recovery_ref,
                "authorization": capability
            }
        }),
    )
}

fn tool_payload(response: &Value) -> Result<Value> {
    if response.pointer("/result/isError").and_then(Value::as_bool) == Some(true) {
        bail!("MCP tool returned an error");
    }
    let text = response
        .pointer("/result/content/0/text")
        .and_then(Value::as_str)
        .context("MCP tool text is unavailable")?;
    serde_json::from_str(text).context("MCP tool payload is not JSON")
}
