use dtoken_config_patcher::agent::{
    known_agent_adapters, scan_agent_adapters, AgentAdapterDescriptor, AgentEvidenceKind,
    AgentInstallCapability, AgentPathBase, AgentPathSpec, AgentScanContext, AgentSessionCapability,
    HostPlatform,
};
use std::fs;
use tempfile::TempDir;

fn workspace_tempdir() -> TempDir {
    tempfile::tempdir_in(std::env::current_dir().expect("current directory"))
        .expect("temporary directory")
}

fn context(root: &TempDir, platform: HostPlatform) -> AgentScanContext {
    AgentScanContext {
        platform,
        home_dir: root.path().join("home"),
        path_dirs: vec![root.path().join("bin")],
        applications_dir: Some(root.path().join("Applications")),
        roaming_app_data: Some(root.path().join("AppData/Roaming")),
        local_app_data: Some(root.path().join("AppData/Local")),
        additional_evidence: vec![],
        observed_at_ms: 42,
    }
}

#[test]
fn known_registry_is_typed_unique_and_not_limited_to_first_release_agents() {
    let adapters = known_agent_adapters();
    assert!(adapters.len() >= 20);

    let mut ids = adapters
        .iter()
        .map(|adapter| adapter.id)
        .collect::<Vec<_>>();
    let original_len = ids.len();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), original_len);

    let opencode = adapters
        .iter()
        .find(|adapter| adapter.id == "opencode")
        .expect("OpenCode adapter");
    assert_eq!(
        opencode.install_capability,
        AgentInstallCapability::OneClick
    );
    assert_eq!(
        opencode.session_capability,
        AgentSessionCapability::Metadata
    );

    let codex = adapters
        .iter()
        .find(|adapter| adapter.id == "codex")
        .expect("Codex adapter");
    assert!(codex.session_paths.iter().all(|path| {
        !path.relative.ends_with("history.jsonl") && !path.relative.ends_with("session_index.jsonl")
    }));
    assert!(codex
        .session_paths
        .iter()
        .any(|path| path.relative.ends_with("archived_sessions")));
}

#[test]
fn scan_returns_only_agents_with_real_evidence_and_keeps_all_evidence() {
    let root = workspace_tempdir();
    let scan = context(&root, HostPlatform::Macos);
    fs::create_dir_all(&scan.path_dirs[0]).expect("bin directory");
    fs::create_dir_all(scan.home_dir.join(".codex/sessions")).expect("Codex sessions");
    fs::write(scan.path_dirs[0].join("opencode"), b"").expect("OpenCode executable");

    let snapshot = scan_agent_adapters(known_agent_adapters(), &scan).expect("scan succeeds");
    let ids = snapshot
        .agents
        .iter()
        .map(|agent| agent.id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(ids, vec!["codex", "opencode"]);
    assert_eq!(snapshot.observed_at_ms, 42);
    assert!(!snapshot.evidence_revision.is_empty());

    let codex = snapshot
        .agents
        .iter()
        .find(|agent| agent.id == "codex")
        .expect("Codex detected from session evidence");
    assert!(codex
        .evidence
        .iter()
        .any(|evidence| evidence.kind == AgentEvidenceKind::SessionRoot));
    assert!(snapshot
        .agents
        .iter()
        .all(|agent| agent.id != "claude-code"));
}

#[test]
fn scan_keeps_every_existing_config_for_one_adapter() {
    const CONFIGS: &[AgentPathSpec] = &[
        AgentPathSpec {
            platform: None,
            base: AgentPathBase::Home,
            relative: ".multi-agent/config.json",
        },
        AgentPathSpec {
            platform: None,
            base: AgentPathBase::Home,
            relative: ".multi-agent/profile.config.toml",
        },
    ];
    const ADAPTERS: &[AgentAdapterDescriptor] = &[AgentAdapterDescriptor {
        id: "multi-agent",
        name: "Multi Agent",
        executables: &[],
        install_capability: AgentInstallCapability::ManualOnly,
        session_capability: AgentSessionCapability::Unsupported,
        config_paths: CONFIGS,
        session_paths: &[],
        app_paths: &[],
        extension_prefixes: &[],
    }];
    let root = workspace_tempdir();
    let scan = context(&root, HostPlatform::Linux);
    for config in [
        scan.home_dir.join(".multi-agent/config.json"),
        scan.home_dir.join(".multi-agent/profile.config.toml"),
    ] {
        fs::create_dir_all(config.parent().expect("config parent")).expect("config directory");
        fs::write(config, b"{}").expect("config file");
    }

    let snapshot = scan_agent_adapters(ADAPTERS, &scan).expect("scan succeeds");
    let configs = snapshot.agents[0]
        .evidence
        .iter()
        .filter(|item| item.kind == AgentEvidenceKind::Config)
        .collect::<Vec<_>>();

    assert_eq!(configs.len(), 2);
}

#[test]
fn windows_scan_resolves_app_data_and_command_extensions() {
    let root = workspace_tempdir();
    let scan = context(&root, HostPlatform::Windows);
    fs::create_dir_all(&scan.path_dirs[0]).expect("bin directory");
    fs::create_dir_all(
        scan.roaming_app_data
            .as_ref()
            .expect("roaming app data")
            .join("Cursor/User/workspaceStorage"),
    )
    .expect("Cursor sessions");
    fs::write(scan.path_dirs[0].join("codex.cmd"), b"").expect("Codex command");

    let snapshot = scan_agent_adapters(known_agent_adapters(), &scan).expect("scan succeeds");
    assert_eq!(
        snapshot
            .agents
            .iter()
            .map(|agent| agent.id.as_str())
            .collect::<Vec<_>>(),
        vec!["codex", "cursor"]
    );
}

#[test]
fn macos_scan_finds_user_applications_and_vscode_insiders_extensions() {
    let root = workspace_tempdir();
    let scan = context(&root, HostPlatform::Macos);
    fs::create_dir_all(scan.home_dir.join("Applications/Cursor.app"))
        .expect("user Applications bundle");
    fs::create_dir_all(
        scan.home_dir
            .join(".vscode-insiders/extensions/cline.cline-3.0.0"),
    )
    .expect("VS Code Insiders extension");

    let snapshot = scan_agent_adapters(known_agent_adapters(), &scan).expect("scan succeeds");
    let ids = snapshot
        .agents
        .iter()
        .map(|agent| agent.id.as_str())
        .collect::<Vec<_>>();

    assert_eq!(ids, vec!["cline", "cursor"]);
    assert!(snapshot.agents.iter().any(|agent| {
        agent.id == "cursor"
            && agent
                .evidence
                .iter()
                .any(|item| item.kind == AgentEvidenceKind::AppBundle)
    }));
}

#[test]
fn scanner_accepts_validated_external_descriptors_instead_of_a_fixed_enum() {
    const CUSTOM_CONFIG: &[AgentPathSpec] = &[AgentPathSpec {
        platform: None,
        base: AgentPathBase::Home,
        relative: ".custom-agent/config.json",
    }];
    const CUSTOM: &[AgentAdapterDescriptor] = &[AgentAdapterDescriptor {
        id: "custom-agent",
        name: "Custom Agent",
        executables: &["custom-agent"],
        install_capability: AgentInstallCapability::ManualOnly,
        session_capability: AgentSessionCapability::Unsupported,
        config_paths: CUSTOM_CONFIG,
        session_paths: &[],
        app_paths: &[],
        extension_prefixes: &[],
    }];

    let root = workspace_tempdir();
    let scan = context(&root, HostPlatform::Linux);
    let config_path = scan.home_dir.join(".custom-agent/config.json");
    fs::create_dir_all(config_path.parent().expect("config parent")).expect("config directory");
    fs::write(&config_path, b"{}").expect("custom config");

    let snapshot = scan_agent_adapters(CUSTOM, &scan).expect("custom adapter scan");
    assert_eq!(snapshot.agents.len(), 1);
    assert_eq!(snapshot.agents[0].id, "custom-agent");
    assert_eq!(snapshot.agents[0].evidence[0].path, config_path);
}

#[test]
fn duplicate_or_invalid_descriptors_fail_closed() {
    const DUPLICATE: &[AgentAdapterDescriptor] = &[
        AgentAdapterDescriptor {
            id: "duplicate",
            name: "First",
            executables: &[],
            install_capability: AgentInstallCapability::ManualOnly,
            session_capability: AgentSessionCapability::Unsupported,
            config_paths: &[],
            session_paths: &[],
            app_paths: &[],
            extension_prefixes: &[],
        },
        AgentAdapterDescriptor {
            id: "duplicate",
            name: "Second",
            executables: &[],
            install_capability: AgentInstallCapability::ManualOnly,
            session_capability: AgentSessionCapability::Unsupported,
            config_paths: &[],
            session_paths: &[],
            app_paths: &[],
            extension_prefixes: &[],
        },
    ];
    let root = workspace_tempdir();

    let error = scan_agent_adapters(DUPLICATE, &context(&root, HostPlatform::Linux))
        .expect_err("duplicate descriptor IDs must be rejected");
    assert!(error.to_string().contains("duplicate Agent adapter id"));
}
