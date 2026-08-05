use dtoken_preprocessor::{ManagedHookPaths, ManagedRtkLifecycle};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
};
use tempfile::tempdir;

fn bridge_binary() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_dtoken-agent-bridge"))
}

fn run_bridge(path: &Path, args: &[&str], input: &[u8]) -> Output {
    let mut child = Command::new(path)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn dtoken-agent-bridge");
    child
        .stdin
        .take()
        .expect("bridge stdin")
        .write_all(input)
        .expect("write bridge stdin");
    child.wait_with_output().expect("bridge output")
}

fn managed_command_binary() -> PathBuf {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .and_then(Path::parent)
        .expect("repo root")
        .to_path_buf();
    if cfg!(windows) {
        root.join("support/fake-bin/managed-rtk-command.cmd")
    } else {
        root.join("support/fake-bin/managed-rtk-command")
    }
}

fn private_bridge_copy(root: &Path) -> PathBuf {
    let file_name = if cfg!(windows) {
        "dtoken-agent-bridge.exe"
    } else {
        "dtoken-agent-bridge"
    };
    let copied = root.join(file_name);
    fs::copy(bridge_binary(), &copied).expect("copy bridge into isolated test root");
    copied
}

#[test]
fn hook_subprocess_has_a_bounded_non_echoing_stdio_contract() {
    let secret = b"{\"transcript\":\"DO_NOT_ECHO_fixture_secret\"}";
    let output = run_bridge(&bridge_binary(), &["hook", "session-start"], secret);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    let stdout = String::from_utf8(output.stdout).expect("Hook UTF-8 output");
    assert!(stdout.starts_with("d-token Hybrid session context:"));
    assert!(stdout.contains("Hook/MCP availability alone never proves active compression"));
    assert!(!stdout.contains("DO_NOT_ECHO"));
    assert_eq!(stdout.lines().count(), 1);
    assert!(stdout.chars().count() <= 601);
}

#[test]
fn mcp_subprocess_supports_handshake_tools_and_rejects_unknown_tools() {
    let requests = [
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"fixture","version":"1"}}}),
        json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
        json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"hybrid_status","arguments":{"untrusted":"DO_NOT_ECHO_fixture_secret"}}}),
        json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"unknown_fixture_tool","arguments":{}}}),
    ];
    let mut input = requests
        .iter()
        .map(Value::to_string)
        .collect::<Vec<_>>()
        .join("\n")
        .into_bytes();
    input.push(b'\n');
    let output = run_bridge(&bridge_binary(), &["mcp", "stdio"], &input);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());

    let responses = String::from_utf8(output.stdout)
        .expect("MCP UTF-8 output")
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).expect("MCP JSON response"))
        .map(|response| (response["id"].as_i64().expect("response id"), response))
        .collect::<BTreeMap<_, _>>();
    assert_eq!(
        responses.len(),
        4,
        "notification must not produce a response"
    );
    assert_eq!(
        responses[&1]
            .pointer("/result/protocolVersion")
            .and_then(Value::as_str),
        Some("2025-06-18")
    );
    assert_eq!(
        responses[&1]
            .pointer("/result/serverInfo/name")
            .and_then(Value::as_str),
        Some("dtoken-hybrid")
    );
    let tools = responses[&2]["result"]["tools"]
        .as_array()
        .expect("MCP tools array");
    assert_eq!(
        tools
            .iter()
            .map(|tool| tool["name"].as_str().expect("tool name"))
            .collect::<Vec<_>>(),
        vec!["hybrid_status", "explain_last_decision"]
    );
    let status_text = responses[&3]
        .pointer("/result/content/0/text")
        .and_then(Value::as_str)
        .expect("metadata-only tool text");
    let status: Value = serde_json::from_str(status_text).expect("tool text JSON");
    assert_eq!(status["data_policy"], "metadata_only");
    assert!(!status_text.contains("DO_NOT_ECHO"));
    assert_eq!(
        responses[&4].pointer("/error/code").and_then(Value::as_i64),
        Some(-32602)
    );
    assert_eq!(
        responses[&4]
            .pointer("/error/message")
            .and_then(Value::as_str),
        Some("unknown d-token Hybrid tool")
    );
}

#[test]
fn invalid_cli_command_returns_a_nonzero_exit_code() {
    let output = run_bridge(&bridge_binary(), &["unknown-fixture-command"], b"");
    assert!(!output.status.success());
    assert_ne!(output.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&output.stderr).contains("usage: dtoken-agent-bridge"));
}

#[test]
fn managed_rtk_wrap_filters_output_tracks_metadata_and_preserves_failure_exit_code() {
    let root = tempdir().expect("managed RTK tempdir");
    let managed_root = root.path().join("managed-rtk");
    let isolated_bridge = private_bridge_copy(root.path());
    let mut lifecycle = ManagedRtkLifecycle::plan(
        "rtk-wrap-fixture.1",
        if cfg!(windows) {
            dtoken_types::agent::HostPlatform::Windows
        } else {
            dtoken_types::agent::HostPlatform::Macos
        },
        &"e".repeat(64),
    );
    lifecycle
        .install_hook(ManagedHookPaths {
            managed_root: managed_root.clone(),
            bridge_path: isolated_bridge.clone(),
            max_output_bytes: Some(1_024),
        })
        .expect("install managed hook");
    lifecycle.verify_hook().expect("verify managed hook");

    let command = managed_command_binary();
    let managed_root_text = managed_root.display().to_string();
    let command_text = command.display().to_string();
    let args = [
        "rtk",
        "wrap",
        "--managed-root",
        managed_root_text.as_str(),
        "--",
        command_text.as_str(),
        "7",
    ];
    let output = run_bridge(&isolated_bridge, &args, b"");
    assert!(!output.status.success());
    assert_eq!(
        output.status.code(),
        Some(7),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("managed stdout");
    let stderr = String::from_utf8(output.stderr).expect("managed stderr");
    assert!(stdout.contains("previous line repeated"));
    assert!(!stdout.contains("\u{1b}[31m"));
    assert!(stderr.contains("SECRET=must-not-persist"));

    let usage_dir = managed_root.join("usage");
    let mut entries = fs::read_dir(&usage_dir)
        .expect("usage dir")
        .map(|entry| entry.expect("usage entry").path())
        .collect::<Vec<_>>();
    entries.sort();
    assert_eq!(entries.len(), 1);
    let usage_text = fs::read_to_string(&entries[0]).expect("usage record");
    let usage: Value = serde_json::from_str(&usage_text).expect("usage JSON");
    assert_eq!(usage["disposition"], "filtered_verified_hook");
    assert_eq!(usage["exit_code"].as_i64(), Some(7));
    assert_eq!(
        usage["command_basename"].as_str(),
        command.file_name().and_then(|name| name.to_str())
    );
    assert!(!usage_text.contains("SECRET=must-not-persist"));
}

#[test]
fn managed_rtk_wrap_bounds_large_dual_stream_output_and_records_capture_evidence() {
    let root = tempdir().expect("managed RTK tempdir");
    let managed_root = root.path().join("managed-rtk");
    let isolated_bridge = private_bridge_copy(root.path());
    let mut lifecycle = ManagedRtkLifecycle::plan(
        "rtk-wrap-fixture.large",
        if cfg!(windows) {
            dtoken_types::agent::HostPlatform::Windows
        } else {
            dtoken_types::agent::HostPlatform::Macos
        },
        &"f".repeat(64),
    );
    lifecycle
        .install_hook(ManagedHookPaths {
            managed_root: managed_root.clone(),
            bridge_path: isolated_bridge.clone(),
            max_output_bytes: Some(1_024),
        })
        .expect("install managed hook");
    lifecycle.verify_hook().expect("verify managed hook");

    let command = managed_command_binary();
    let managed_root_text = managed_root.display().to_string();
    let command_text = command.display().to_string();
    let output = run_bridge(
        &isolated_bridge,
        &[
            "rtk",
            "wrap",
            "--managed-root",
            managed_root_text.as_str(),
            "--max-bytes",
            "1024",
            "--",
            command_text.as_str(),
            "large",
        ],
        b"",
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stdout.len() <= 1_024);
    assert!(output.stderr.len() <= 1_024);
    assert!(String::from_utf8_lossy(&output.stdout).contains("bounded capture truncated"));
    assert!(String::from_utf8_lossy(&output.stderr).contains("bounded capture truncated"));

    let entries = fs::read_dir(managed_root.join("usage"))
        .expect("usage dir")
        .map(|entry| entry.expect("usage entry").path())
        .collect::<Vec<_>>();
    assert_eq!(entries.len(), 1);
    let usage_text = fs::read_to_string(&entries[0]).expect("usage record");
    let usage: Value = serde_json::from_str(&usage_text).expect("usage JSON");
    assert!(
        usage["input_bytes"].as_u64().unwrap_or_default()
            > usage["output_bytes"].as_u64().unwrap_or_default()
    );
    assert!(usage["transforms"]
        .as_array()
        .expect("transforms")
        .iter()
        .any(|value| value == "stdout:bounded_capture"));
    assert!(!usage_text.contains("stdout-0000"));
}

#[test]
fn managed_rtk_wrap_rejects_a_cli_limit_that_is_not_hash_bound_to_the_manifest() {
    let root = tempdir().expect("managed RTK tempdir");
    let managed_root = root.path().join("managed-rtk");
    let isolated_bridge = private_bridge_copy(root.path());
    let mut lifecycle = ManagedRtkLifecycle::plan(
        "rtk-wrap-fixture.stale",
        dtoken_types::agent::HostPlatform::Macos,
        &"1".repeat(64),
    );
    lifecycle
        .install_hook(ManagedHookPaths {
            managed_root: managed_root.clone(),
            bridge_path: isolated_bridge.clone(),
            max_output_bytes: Some(1_024),
        })
        .expect("install managed hook");
    lifecycle.verify_hook().expect("verify managed hook");

    let command = managed_command_binary();
    let managed_root_text = managed_root.display().to_string();
    let command_text = command.display().to_string();
    let output = run_bridge(
        &isolated_bridge,
        &[
            "rtk",
            "wrap",
            "--managed-root",
            managed_root_text.as_str(),
            "--max-bytes",
            "2048",
            "--",
            command_text.as_str(),
            "7",
        ],
        b"",
    );
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("does not match"));
    assert!(output.stdout.is_empty());
    assert_eq!(
        fs::read_dir(managed_root.join("usage"))
            .expect("usage dir")
            .count(),
        0
    );
}

#[test]
fn unverified_managed_rtk_wrap_is_exact_passthrough_and_does_not_create_evidence() {
    let root = tempdir().expect("managed RTK tempdir");
    let isolated_bridge = private_bridge_copy(root.path());
    let missing_root = root.path().join("missing-managed-rtk");
    let command = managed_command_binary();
    let missing_root_text = missing_root.display().to_string();
    let command_text = command.display().to_string();
    let output = run_bridge(
        &isolated_bridge,
        &[
            "rtk",
            "wrap",
            "--managed-root",
            missing_root_text.as_str(),
            "--",
            command_text.as_str(),
            "7",
        ],
        b"",
    );
    assert_eq!(output.status.code(), Some(7));
    assert!(String::from_utf8_lossy(&output.stdout).contains("\u{1b}[31m"));
    assert!(String::from_utf8_lossy(&output.stderr).contains("SECRET=must-not-persist"));
    assert!(!missing_root.exists());
}

#[cfg(windows)]
#[test]
fn windows_binary_path_with_spaces_preserves_success_and_failure_exit_codes() {
    use std::fs;

    let root = tempfile::tempdir().expect("Windows bridge tempdir");
    let directory = root.path().join("Program Files/d-token");
    fs::create_dir_all(&directory).expect("Windows bridge directory");
    let copied = directory.join("dtoken-agent-bridge.exe");
    fs::copy(bridge_binary(), &copied).expect("copy Windows bridge fixture");

    let identity = run_bridge(&copied, &["identity"], b"");
    assert!(identity.status.success());
    let identity: Value = serde_json::from_slice(&identity.stdout).expect("bridge identity JSON");
    assert_eq!(identity["component"], "dtoken-agent-bridge");
    assert_eq!(identity["os"], "windows");

    let failure = run_bridge(&copied, &["unknown-fixture-command"], b"");
    assert!(!failure.status.success());
    assert_ne!(failure.status.code(), Some(0));
}
