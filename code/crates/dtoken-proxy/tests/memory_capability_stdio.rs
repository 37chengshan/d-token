use dtoken_config::DtokenConfig;
use dtoken_context_store::{
    MemoryAccessScope, MemoryCapabilityPermission, MemoryCapabilityTransactionRequest,
};
use dtoken_core_api::AppState;
use dtoken_types::{
    ChainMode, CompressionPolicy, Protocol, ProviderKind, ProviderProfile, RouteProfile,
};
use serde_json::{json, Value};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
};

fn proxy_binary() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_dtoken-proxy"))
}

fn config() -> DtokenConfig {
    DtokenConfig {
        providers: vec![ProviderProfile {
            id: "memory-provider".to_string(),
            name: "Memory provider".to_string(),
            kind: ProviderKind::Custom,
            protocol: Protocol::OpenAi,
            base_url: "http://127.0.0.1:9/v1".to_string(),
            api_key_ref: None,
            models: vec!["memory-model".to_string()],
            timeout_ms: 100,
            healthcheck_path: None,
        }],
        routes: vec![RouteProfile {
            id: "memory-route".to_string(),
            name: "Memory route".to_string(),
            input_protocol: Protocol::OpenAi,
            instance_id: "memory-instance".to_string(),
            target_provider_id: "memory-provider".to_string(),
            fallback_provider_id: None,
            chain_mode: ChainMode::Direct,
            compression_policy: CompressionPolicy::Balanced,
            bind_host: "127.0.0.1".to_string(),
            local_port: 18_001,
        }],
        instances: vec![],
        provider_cache_policies: vec![],
        output_shaping_policies: vec![],
        optical: Default::default(),
    }
}

fn run_proxy(
    config_path: &Path,
    data_dir: &Path,
    capability_path: &Path,
    model: &str,
    requests: &[Value],
) -> Output {
    let mut child = Command::new(proxy_binary())
        .args([
            "--mcp-stdio",
            "--config",
            config_path.to_str().expect("config UTF-8"),
            "--data-dir",
            data_dir.to_str().expect("data dir UTF-8"),
            "--mcp-memory-capability-file",
            capability_path.to_str().expect("capability path UTF-8"),
            "--mcp-route-id",
            "memory-route",
            "--mcp-model",
            model,
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn dtoken-proxy MCP");
    let mut stdin = child.stdin.take().expect("proxy stdin");
    for request in requests {
        writeln!(stdin, "{request}").expect("write MCP request");
    }
    drop(stdin);
    child.wait_with_output().expect("proxy MCP output")
}

fn tool_payload(response: &Value) -> Value {
    serde_json::from_str(
        response
            .pointer("/result/content/0/text")
            .and_then(Value::as_str)
            .expect("tool text"),
    )
    .expect("tool payload JSON")
}

#[test]
fn private_route_bound_capability_is_distributed_to_real_mcp_stdio_process() {
    let root = tempfile::tempdir().expect("MCP capability tempdir");
    let data_dir = root.path().join("runtime");
    let config_path = root.path().join("config.json");
    let config = config();
    fs::write(
        &config_path,
        serde_json::to_vec_pretty(&config).expect("config JSON"),
    )
    .expect("write config");

    let state = AppState::new_persistent(config, &data_dir);
    let route_binding = state
        .memory_capability_route_binding("memory-route", "memory-model")
        .expect("route binding");
    let observed_at_ms = dtoken_types::time::now_ms();
    let prepared = state
        .prepare_memory_capability_transaction(&MemoryCapabilityTransactionRequest {
            scope: MemoryAccessScope {
                project_id: "memory-project".to_string(),
                agent_id: "opencode".to_string(),
                session_id: Some("memory-session".to_string()),
            },
            route_binding,
            permissions: vec![
                MemoryCapabilityPermission::List,
                MemoryCapabilityPermission::Write,
            ],
            source_revision: "proxy-stdio-integration-v1".to_string(),
            issued_at_ms: observed_at_ms,
            expires_at_ms: observed_at_ms + 60_000,
        })
        .expect("prepare capability");
    let capability_path = prepared.plan.target_path.clone();
    let transaction_id = prepared.plan.transaction_id.clone();
    let document_sha256 = prepared.plan.document_sha256.clone();
    let review_digest = prepared.plan.review_digest.clone();
    state
        .apply_memory_capability_transaction(prepared, &review_digest)
        .expect("apply capability");
    let private_document =
        fs::read_to_string(&capability_path).expect("read private capability in test");
    let private_nonce = serde_json::from_str::<Value>(&private_document)
        .expect("capability JSON")
        .get("capability_nonce")
        .and_then(Value::as_str)
        .expect("capability nonce")
        .to_string();

    let output = run_proxy(
        &config_path,
        &data_dir,
        &capability_path,
        "memory-model",
        &[
            json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}),
            json!({"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}),
            json!({
                "jsonrpc":"2.0","id":3,"method":"tools/call","params":{
                    "name":"headroom_memory_write",
                    "arguments":{"record":{
                        "project_id":"memory-project",
                        "agent_id":"opencode",
                        "session_id":"memory-session",
                        "kind":"project-guideline",
                        "summary":"Run focused tests before the workspace gate.",
                        "keywords":["tests"],
                        "dedup_key":"focused-tests",
                        "entry_revision":"a".repeat(64),
                        "provenance":{
                            "source":"managed-agent",
                            "source_revision":"b".repeat(64),
                            "observed_at_ms":observed_at_ms
                        }
                    }}
                }
            }),
            json!({
                "jsonrpc":"2.0","id":4,"method":"tools/call","params":{
                    "name":"headroom_memory_list","arguments":{}
                }
            }),
        ],
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).expect("MCP stdout UTF-8");
    assert!(!stdout.contains(&private_nonce));
    assert!(!stdout.contains("capability_nonce"));
    assert!(!stdout.contains("authorization"));
    let responses = stdout
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).expect("MCP response JSON"))
        .collect::<Vec<_>>();
    assert_eq!(responses.len(), 4);
    let tools = responses[1]
        .pointer("/result/tools")
        .and_then(Value::as_array)
        .expect("tools");
    assert!(tools
        .iter()
        .any(|tool| tool["name"] == "headroom_memory_write"));
    let written = tool_payload(&responses[2]);
    assert_eq!(written["project_id"], "memory-project");
    let listed = tool_payload(&responses[3]);
    assert_eq!(listed["count"], 1);
    assert_eq!(
        listed["entries"][0]["summary"],
        "Run focused tests before the workspace gate."
    );

    let mismatch = run_proxy(
        &config_path,
        &data_dir,
        &capability_path,
        "other-model",
        &[],
    );
    assert!(!mismatch.status.success());
    assert!(!String::from_utf8_lossy(&mismatch.stderr).contains(&private_nonce));

    state
        .revoke_memory_capability_transaction(&transaction_id, &document_sha256, observed_at_ms + 1)
        .expect("revoke capability");
    let revoked = run_proxy(
        &config_path,
        &data_dir,
        &capability_path,
        "memory-model",
        &[],
    );
    assert!(!revoked.status.success());
    assert!(!String::from_utf8_lossy(&revoked.stderr).contains(&private_nonce));
}
