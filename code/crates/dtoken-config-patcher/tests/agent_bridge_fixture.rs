use dtoken_config_patcher::agent_integration::{
    apply_agent_integration, inspect_agent_integration_evidence, prepare_agent_integration,
    rollback_agent_integration, verify_agent_integration_transaction, AgentIntegrationRequest,
    AgentIntegrationScope, AgentIntegrationSurface, ManagedMcpLaunch,
};
use dtoken_config_patcher::agent_transport::{
    apply_reviewed_agent_transport, can_clear_agent_transport, preview_clear_agent_transport,
    preview_configure_agent_transport, verify_agent_transport, AgentTransportOperation,
    AgentTransportRequest,
};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};
use tempfile::TempDir;

#[derive(Debug, PartialEq, Eq)]
enum TreeEntry {
    Directory,
    File(Vec<u8>),
}

fn workspace_tempdir() -> TempDir {
    tempfile::tempdir_in(std::env::current_dir().expect("current directory"))
        .expect("workspace temporary directory")
}

fn write_text(path: &Path, text: &str) {
    fs::create_dir_all(path.parent().expect("fixture parent")).expect("create fixture parent");
    fs::write(path, text).expect("write fixture file");
}

fn snapshot_tree(root: &Path) -> BTreeMap<PathBuf, TreeEntry> {
    fn visit(root: &Path, current: &Path, snapshot: &mut BTreeMap<PathBuf, TreeEntry>) {
        let mut entries = fs::read_dir(current)
            .expect("read fixture directory")
            .collect::<Result<Vec<_>, _>>()
            .expect("collect fixture directory");
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let path = entry.path();
            let relative = path
                .strip_prefix(root)
                .expect("fixture path is below root")
                .to_path_buf();
            let file_type = entry.file_type().expect("fixture file type");
            if file_type.is_dir() {
                snapshot.insert(relative, TreeEntry::Directory);
                visit(root, &path, snapshot);
            } else {
                assert!(file_type.is_file(), "fixture contains no symlinks");
                snapshot.insert(
                    relative,
                    TreeEntry::File(fs::read(path).expect("fixture bytes")),
                );
            }
        }
    }

    let mut snapshot = BTreeMap::new();
    visit(root, root, &mut snapshot);
    snapshot
}

fn parse_generated_hook_command(command: &str) -> Vec<String> {
    let command = command.trim();
    assert!(
        command.starts_with('"'),
        "managed hook executable is quoted"
    );
    let closing = command[1..]
        .find('"')
        .map(|index| index + 1)
        .expect("closing executable quote");
    let mut argv = vec![command[1..closing].to_string()];
    argv.extend(
        command[closing + 1..]
            .split_whitespace()
            .map(str::to_string),
    );
    argv
}

fn managed_hook_argv(path: &Path) -> Vec<String> {
    let document: Value =
        serde_json::from_slice(&fs::read(path).expect("hook configuration")).expect("hook JSON");
    let entry = document["hooks"]["SessionStart"]
        .as_array()
        .expect("SessionStart array")
        .iter()
        .find(|entry| entry["dtoken_managed_id"].as_str() == Some("dtoken-hybrid-session-start-v1"))
        .expect("managed SessionStart hook");
    parse_generated_hook_command(
        entry["hooks"][0]["command"]
            .as_str()
            .expect("managed hook command"),
    )
}

fn assert_no_managed_bridge_reference(root: &Path) {
    for (relative, entry) in snapshot_tree(root) {
        let TreeEntry::File(bytes) = entry else {
            continue;
        };
        let text = String::from_utf8_lossy(&bytes);
        assert!(
            !text.contains("dtoken-hybrid-session-start-v1")
                && !text.contains("DtokenHybridContextPlugin")
                && !text.contains("dtoken_hybrid")
                && !text.contains("dtoken-hybrid"),
            "managed bridge reference remained in {}",
            relative.display()
        );
    }
}

fn integration_request(project: &Path, bridge: &Path) -> AgentIntegrationRequest {
    let runtime_root = project.parent().expect("project parent").join("runtime");
    fs::create_dir_all(&runtime_root).expect("runtime root");
    let active_config_path = runtime_root.join("active-config.json");
    write_text(
        &active_config_path,
        r#"{"providers":[],"routes":[],"instances":[]}"#,
    );
    let capability_root = runtime_root.join("memory-capabilities");
    let managed_mcp_launches = ["claude-code", "codex", "opencode"]
        .into_iter()
        .map(|agent_id| {
            (
                agent_id.to_string(),
                ManagedMcpLaunch {
                    runtime_path: bridge.to_path_buf(),
                    active_config_path: active_config_path.clone(),
                    data_dir: runtime_root.clone(),
                    capability_path: capability_root.join(format!("memcap-{agent_id}.json")),
                    route_id: "fixture-route".to_string(),
                    model: "fixture/model".to_string(),
                },
            )
        })
        .collect();
    AgentIntegrationRequest {
        agent_ids: vec![
            "claude-code".to_string(),
            "codex".to_string(),
            "opencode".to_string(),
        ],
        scope: AgentIntegrationScope::Project,
        project_root: Some(project.to_path_buf()),
        bridge_path: bridge.to_path_buf(),
        managed_mcp_launches,
        managed_rtk_root: None,
        install_instructions: true,
        install_hook_or_plugin: true,
        install_skill: true,
        install_mcp: true,
    }
}

#[test]
fn project_fixture_parses_real_agent_schemas_and_rolls_back_exactly() {
    let root = workspace_tempdir();
    let root = fs::canonicalize(root.path()).expect("canonical fixture root");
    let home = root.join("home");
    let project = root.join("project");
    let backups = root.join("backups");
    fs::create_dir_all(&home).expect("fixture home");
    fs::create_dir_all(&project).expect("fixture project");

    write_text(
        &project.join("CLAUDE.md"),
        "# Existing Claude instructions\n",
    );
    write_text(
        &project.join("AGENTS.md"),
        "# Existing shared instructions\n",
    );
    write_text(
        &project.join(".claude/settings.local.json"),
        r#"{"hooks":{"SessionStart":[{"matcher":"startup","hooks":[{"type":"command","command":"existing-claude-hook"}]}]},"theme":"dark"}"#,
    );
    write_text(
        &project.join(".codex/hooks.json"),
        r#"{"hooks":{"SessionStart":[{"matcher":"startup","hooks":[{"type":"command","command":"existing-codex-hook"}]}]}}"#,
    );
    write_text(
        &project.join(".mcp.json"),
        r#"{"mcpServers":{"existing":{"command":"existing-claude-mcp","args":["serve"]}}}"#,
    );
    write_text(
        &project.join(".codex/config.toml"),
        "# existing Codex MCP\n[mcp_servers.existing]\ncommand = \"existing-codex-mcp\"\nargs = [\"serve\"]\n",
    );
    write_text(
        &project.join("opencode.json"),
        r#"{"mcp":{"existing":{"type":"local","command":["existing-opencode-mcp","serve"],"enabled":true}}}"#,
    );

    let bridge_dir = root.join("fixture bin");
    fs::create_dir_all(&bridge_dir).expect("bridge fixture directory");
    let bridge = bridge_dir.join(if cfg!(windows) {
        "dtoken-agent-bridge.exe"
    } else {
        "dtoken-agent-bridge"
    });
    fs::write(&bridge, b"fixture bridge placeholder").expect("bridge fixture");
    let bridge = fs::canonicalize(bridge).expect("canonical bridge path");
    assert!(bridge.to_string_lossy().contains(' '));

    let before = snapshot_tree(&project);
    let request = integration_request(&project, &bridge);
    let prepared = prepare_agent_integration(&request, &home, &backups).expect("prepare");
    assert_eq!(prepared.plan.targets.len(), 10);
    assert!(prepared.plan.targets.iter().all(|target| {
        target.before_sha256.len() == 64
            && target.after_sha256.len() == 64
            && target.before_sha256 != target.after_sha256
    }));
    let digest = prepared.plan.review_digest.clone();
    let receipt = apply_agent_integration(prepared, &backups, &digest).expect("apply");
    let verification = verify_agent_integration_transaction(&receipt.transaction_id, &backups)
        .expect("verify transaction");
    assert!(verification.ok);
    assert_eq!(verification.verified_targets.len(), 10);

    let expected_hook = vec![
        bridge.display().to_string(),
        "hook".to_string(),
        "session-start".to_string(),
    ];
    assert_eq!(
        managed_hook_argv(&project.join(".claude/settings.local.json")),
        expected_hook
    );
    assert_eq!(
        managed_hook_argv(&project.join(".codex/hooks.json")),
        expected_hook
    );

    let claude_mcp: Value =
        serde_json::from_slice(&fs::read(project.join(".mcp.json")).expect("Claude project MCP"))
            .expect("Claude MCP JSON");
    assert_eq!(
        claude_mcp["mcpServers"]["dtoken-hybrid"]["command"].as_str(),
        Some(bridge.to_string_lossy().as_ref())
    );
    let runtime_root = root.join("runtime");
    let active_config_path = runtime_root.join("active-config.json");
    let capability_path = runtime_root
        .join("memory-capabilities")
        .join("memcap-claude-code.json");
    let expected_mcp_args = json!([
        "--mcp-stdio",
        "--config",
        active_config_path.display().to_string(),
        "--data-dir",
        runtime_root.display().to_string(),
        "--mcp-memory-capability-file",
        capability_path.display().to_string(),
        "--mcp-route-id",
        "fixture-route",
        "--mcp-model",
        "fixture/model"
    ]);
    assert_eq!(
        claude_mcp["mcpServers"]["dtoken-hybrid"]["args"],
        expected_mcp_args
    );

    let codex_mcp = fs::read_to_string(project.join(".codex/config.toml"))
        .expect("Codex MCP configuration")
        .parse::<toml_edit::DocumentMut>()
        .expect("Codex MCP TOML");
    let codex_server = &codex_mcp["mcp_servers"]["dtoken_hybrid"];
    let codex_capability_path = runtime_root
        .join("memory-capabilities")
        .join("memcap-codex.json");
    assert_eq!(
        codex_server["command"].as_str(),
        Some(bridge.to_string_lossy().as_ref())
    );
    assert_eq!(
        codex_server["args"]
            .as_array()
            .expect("Codex MCP args")
            .iter()
            .map(|value| value.as_str().expect("Codex string arg"))
            .collect::<Vec<_>>(),
        vec![
            "--mcp-stdio",
            "--config",
            active_config_path.to_str().expect("active config UTF-8"),
            "--data-dir",
            runtime_root.to_str().expect("runtime root UTF-8"),
            "--mcp-memory-capability-file",
            codex_capability_path
                .to_str()
                .expect("Codex capability UTF-8"),
            "--mcp-route-id",
            "fixture-route",
            "--mcp-model",
            "fixture/model",
        ]
    );

    let opencode: Value = serde_json::from_slice(
        &fs::read(project.join("opencode.json")).expect("OpenCode configuration"),
    )
    .expect("OpenCode JSON");
    assert_eq!(
        opencode["mcp"]["dtoken-hybrid"]["command"],
        json!([
            bridge.display().to_string(),
            "--mcp-stdio",
            "--config",
            active_config_path.display().to_string(),
            "--data-dir",
            runtime_root.display().to_string(),
            "--mcp-memory-capability-file",
            runtime_root
                .join("memory-capabilities")
                .join("memcap-opencode.json")
                .display()
                .to_string(),
            "--mcp-route-id",
            "fixture-route",
            "--mcp-model",
            "fixture/model"
        ])
    );
    let plugin = fs::read_to_string(project.join(".opencode/plugins/dtoken-hybrid-context.js"))
        .expect("OpenCode plugin");
    assert!(plugin.contains("experimental.session.compacting"));
    assert!(plugin.contains("rendered history is prior context"));
    assert!(!plugin.contains(&bridge.display().to_string()));

    for agent_id in ["claude-code", "codex", "opencode"] {
        let evidence = inspect_agent_integration_evidence(
            agent_id,
            AgentIntegrationScope::Project,
            &home,
            Some(&project),
        )
        .expect("managed Agent evidence");
        assert_eq!(
            evidence.managed_surfaces,
            vec![
                AgentIntegrationSurface::Instructions,
                AgentIntegrationSurface::HookOrPlugin,
                AgentIntegrationSurface::Skill,
                AgentIntegrationSurface::Mcp,
            ]
        );
    }

    rollback_agent_integration(&receipt.transaction_id, &backups).expect("exact rollback");
    assert_eq!(snapshot_tree(&project), before);
    assert_no_managed_bridge_reference(&project);
    for agent_id in ["claude-code", "codex", "opencode"] {
        assert!(inspect_agent_integration_evidence(
            agent_id,
            AgentIntegrationScope::Project,
            &home,
            Some(&project),
        )
        .expect("post-rollback evidence")
        .managed_surfaces
        .is_empty());
    }
}

fn reviewed_apply(
    request: &AgentTransportRequest,
    operation: AgentTransportOperation,
    backups: &Path,
) {
    let review = match operation {
        AgentTransportOperation::Configure => preview_configure_agent_transport(request),
        AgentTransportOperation::Clear => preview_clear_agent_transport(request),
    }
    .expect("transport preview");
    let receipt = apply_reviewed_agent_transport(
        request,
        operation,
        backups,
        &review.revision,
        &review.before_sha256,
        &review.after_sha256,
    )
    .expect("reviewed transport apply");
    assert!(receipt.verification.ok);
}

fn transport_request(agent_id: &str, target_path: PathBuf) -> AgentTransportRequest {
    AgentTransportRequest {
        agent_id: agent_id.to_string(),
        target_path,
        endpoint: "http://127.0.0.1:18001".to_string(),
        enabled: true,
        model: Some("fixture-model".to_string()),
    }
}

#[test]
#[test]
fn managed_rtk_command_hook_routes_tool_commands_through_bridge_wrap_and_rolls_back() {
    use dtoken_config_patcher::agent_integration::verify_pretool_wrap_hook;
    use dtoken_preprocessor::{ManagedHookPaths, ManagedRtkLifecycle};
    use std::process::{Command, Stdio};

    let root = workspace_tempdir();
    let root = fs::canonicalize(root.path()).expect("canonical fixture root");
    let home = root.join("home");
    let project = root.join("project");
    let backups = root.join("backups");
    fs::create_dir_all(&home).expect("fixture home");
    fs::create_dir_all(&project).expect("fixture project");

    // The real bridge binary from the current build is the managed wrapper.
    let bridge_bin = PathBuf::from(env!("CARGO_BIN_EXE_dtoken-agent-bridge"));
    let bridge = fs::canonicalize(&bridge_bin).expect("canonical bridge");

    // A fully verified d-token managed RTK root bound to that bridge.
    let managed_root = root.join("managed-rtk");
    let mut lifecycle = ManagedRtkLifecycle::plan(
        "rtk-hook-fixture.1",
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
            bridge_path: bridge.clone(),
            max_output_bytes: Some(1_024),
            filter_dsl: None,
        })
        .expect("install managed hook");
    lifecycle.verify_hook().expect("verify managed hook");

    // A user-owned PreToolUse hook must be preserved alongside the
    // d-token-owned wrap entry.
    write_text(
        &project.join(".claude/settings.local.json"),
        r#"{"hooks":{"PreToolUse":[{"matcher":"Bash","hooks":[{"type":"command","command":"user-hook"}]}]}}"#,
    );

    let mut request = integration_request(&project, &bridge);
    request.managed_rtk_root = Some(managed_root.clone());
    request.install_mcp = false;
    request.install_skill = false;
    request.install_instructions = false;
    let prepared = prepare_agent_integration(&request, &home, &backups).expect("prepare");
    let digest = prepared.plan.review_digest.clone();
    let receipt = apply_agent_integration(prepared, &backups, &digest).expect("apply");
    assert_eq!(receipt.verified_targets.len(), 1);

    let settings_path = project.join(".claude/settings.local.json");
    verify_pretool_wrap_hook(&settings_path, &bridge, &managed_root)
        .expect("generated PreToolUse wrap hook matches the canonical shim command");
    let document: Value = serde_json::from_slice(
        &fs::read(&settings_path).expect("settings after apply"),
    )
    .expect("settings JSON");
    let pretool = document["hooks"]["PreToolUse"].as_array().expect("array");
    assert_eq!(pretool.len(), 2, "user hook is preserved beside the wrap hook");
    assert!(pretool.iter().any(|entry| {
        entry["hooks"][0]["command"].as_str() == Some("user-hook")
    }));

    // EXECUTE the generated hook command against a fake tool command through
    // the real bridge: the shim must route it through the managed wrap,
    // filter the JSON output, and preserve the tool exit code (7).
    let wrap_entry = pretool
        .iter()
        .find(|entry| {
            entry["dtoken_managed_id"].as_str() == Some("dtoken-managed-rtk-wrap-v1")
        })
        .expect("wrap hook entry");
    let command = wrap_entry["hooks"][0]["command"]
        .as_str()
        .expect("wrap command");
    let argv = parse_generated_hook_command(command);
    let mut invocation = Command::new(&argv[0]);
    invocation
        .args(&argv[1..])
        .arg("--")
        .arg(if cfg!(windows) {
            "cmd"
        } else {
            "/usr/bin/printf"
        });
    if cfg!(windows) {
        invocation.arg("/c").arg("exit 7");
    } else {
        invocation.arg("{\\n  \\\"name\\\": \\\"tool\\\",\\n  \\\"ok\\\": true\\n}\\n");
    }
    let output = invocation
        .stdin(Stdio::null())
        .output()
        .expect("execute generated hook command");
    assert_eq!(
        output.status.code(),
        Some(7),
        "hook must preserve the tool exit code (stderr={})",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("\"ok\":true") || stdout.contains("\"name\":\"tool\""),
        "hook must route tool output through the managed wrap: {stdout}"
    );

    // The wrap produced one usage receipt under the verified root.
    let usage_dir = managed_root.join("usage");
    assert_eq!(
        fs::read_dir(&usage_dir).expect("usage dir").count(),
        1,
        "the generated hook invocation must tee a usage receipt"
    );

    // Rollback removes the d-token-owned wrap entry and restores the user's
    // original document exactly.
    rollback_agent_integration(&receipt.transaction_id, &backups).expect("rollback");
    let restored: Value = serde_json::from_slice(
        &fs::read(&settings_path).expect("settings after rollback"),
    )
    .expect("restored settings JSON");
    assert_eq!(
        restored,
        json!({"hooks":{"PreToolUse":[{"matcher":"Bash","hooks":[{"type":"command","command":"user-hook"}]}]}})
    );
}
    let root = workspace_tempdir();
    let root = fs::canonicalize(root.path()).expect("canonical fixture root");
    let backups = root.join("backups");
    let claude_path = root.join("claude-settings.json");
    let codex_path = root.join("codex-config.toml");
    let opencode_path = root.join("opencode.json");
    write_text(
        &claude_path,
        r#"{"theme":"dark","env":{"ANTHROPIC_BASE_URL":"https://prior.example/v1","ANTHROPIC_MODEL":"claude-prior","ANTHROPIC_API_KEY":"fixture-reference-only"}}"#,
    );
    write_text(
        &codex_path,
        "model_provider = \"prior\"\nmodel = \"gpt-prior\"\n[model_providers.prior]\nname = \"Prior\"\nbase_url = \"https://prior.example/v1\"\nwire_api = \"responses\"\n",
    );
    write_text(
        &opencode_path,
        r#"{"provider":{"prior":{"name":"Prior"}},"enabled_providers":["prior"]}"#,
    );

    for request in [
        transport_request("claude-code", claude_path.clone()),
        transport_request("codex", codex_path.clone()),
        transport_request("opencode", opencode_path.clone()),
    ] {
        reviewed_apply(&request, AgentTransportOperation::Configure, &backups);
        assert!(
            verify_agent_transport(&request)
                .expect("configured transport")
                .ok
        );
        assert!(can_clear_agent_transport(
            &request.agent_id,
            &request.target_path
        ));
        reviewed_apply(&request, AgentTransportOperation::Clear, &backups);
        assert!(!can_clear_agent_transport(
            &request.agent_id,
            &request.target_path
        ));
    }

    let claude: Value =
        serde_json::from_slice(&fs::read(&claude_path).expect("Claude prior route"))
            .expect("Claude prior JSON");
    assert_eq!(
        claude
            .pointer("/env/ANTHROPIC_BASE_URL")
            .and_then(Value::as_str),
        Some("https://prior.example/v1")
    );
    assert_eq!(
        claude
            .pointer("/env/ANTHROPIC_MODEL")
            .and_then(Value::as_str),
        Some("claude-prior")
    );
    assert!(claude.pointer("/dtoken/agent_transport").is_none());
    let codex = fs::read_to_string(&codex_path).expect("Codex prior route");
    assert!(codex.contains("model_provider = \"prior\""));
    assert!(codex.contains("model = \"gpt-prior\""));
    assert!(!codex.contains("[model_providers.dtoken]"));
    let opencode: Value =
        serde_json::from_slice(&fs::read(&opencode_path).expect("OpenCode prior route"))
            .expect("OpenCode prior JSON");
    assert_eq!(opencode["enabled_providers"], json!(["prior"]));
    assert_eq!(opencode["provider"]["prior"]["name"], "Prior");
    assert!(opencode.pointer("/provider/dtoken-local-route").is_none());

    let claude_request = transport_request("claude-code", claude_path.clone());
    reviewed_apply(
        &claude_request,
        AgentTransportOperation::Configure,
        &backups,
    );
    let clear_review = preview_clear_agent_transport(&claude_request).expect("clear preview");
    let mut externally_edited: Value =
        serde_json::from_slice(&fs::read(&claude_path).expect("configured Claude route"))
            .expect("configured Claude JSON");
    externally_edited["user_note"] = Value::String("keep this external edit".to_string());
    fs::write(
        &claude_path,
        serde_json::to_vec_pretty(&externally_edited).expect("external edit JSON"),
    )
    .expect("external edit");
    let error = apply_reviewed_agent_transport(
        &claude_request,
        AgentTransportOperation::Clear,
        &backups,
        &clear_review.revision,
        &clear_review.before_sha256,
        &clear_review.after_sha256,
    )
    .expect_err("external edit must invalidate clear review");
    assert!(error.to_string().contains("changed after review"));
    assert_eq!(
        serde_json::from_slice::<Value>(&fs::read(claude_path).expect("external edit remains"))
            .expect("external edit JSON")["user_note"],
        "keep this external edit"
    );
}

#[cfg(windows)]
#[test]
fn windows_hook_schema_quotes_a_bridge_path_with_spaces() {
    let root = workspace_tempdir();
    let root = fs::canonicalize(root.path()).expect("canonical fixture root");
    let home = root.join("home");
    let project = root.join("project");
    fs::create_dir_all(&home).expect("fixture home");
    fs::create_dir_all(&project).expect("fixture project");
    let bridge = root.join("Program Files/d-token/dtoken-agent-bridge.exe");
    write_text(&bridge, "fixture");
    let bridge = fs::canonicalize(bridge).expect("canonical Windows bridge");
    let mut request = integration_request(&project, &bridge);
    request.agent_ids = vec!["codex".to_string()];
    request.install_instructions = false;
    request.install_skill = false;
    request.install_mcp = false;
    let prepared = prepare_agent_integration(&request, &home, &root.join("backups"))
        .expect("Windows hook prepare");
    let digest = prepared.plan.review_digest.clone();
    let receipt = apply_agent_integration(prepared, &root.join("backups"), &digest)
        .expect("Windows hook apply");
    assert_eq!(
        managed_hook_argv(&project.join(".codex/hooks.json")),
        vec![
            bridge.display().to_string(),
            "hook".to_string(),
            "session-start".to_string(),
        ]
    );
    rollback_agent_integration(&receipt.transaction_id, &root.join("backups"))
        .expect("Windows hook rollback");
}
