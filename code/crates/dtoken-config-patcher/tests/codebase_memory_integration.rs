use dtoken_config_patcher::codebase_memory_integration::{
    apply_codebase_memory_integration, get_codebase_memory_integration_transaction,
    prepare_codebase_memory_integration, rollback_codebase_memory_integration,
    verify_codebase_memory_integration, CodebaseMemoryIntegrationRequest,
    CodebaseMemoryIntegrationScope, CodebaseMemoryIntegrationSurface,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};
use tempfile::TempDir;

struct Fixture {
    _root: TempDir,
    home: PathBuf,
    project: PathBuf,
    cache: PathBuf,
    launcher: PathBuf,
    upstream: PathBuf,
    backups: PathBuf,
    launcher_sha256: String,
    upstream_sha256: String,
}

impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().expect("temporary workspace");
        let root_path = fs::canonicalize(root.path()).expect("canonical temporary workspace");
        let home = root_path.join("home");
        let project = root_path.join("project");
        let cache = root_path.join("cache");
        let launcher = root_path.join("dtoken-codebase-memory-launcher");
        let upstream = root_path.join("codebase-memory-mcp");
        let backups = root_path.join("backups");
        fs::create_dir_all(&home).expect("home directory");
        fs::create_dir_all(&project).expect("project directory");
        fs::create_dir_all(&cache).expect("cache directory");
        fs::write(&launcher, b"verified fake launcher v1").expect("fake launcher");
        fs::write(&upstream, b"verified fake upstream v1").expect("fake upstream");
        let launcher_sha256 = sha256(b"verified fake launcher v1");
        let upstream_sha256 = sha256(b"verified fake upstream v1");
        Self {
            _root: root,
            home,
            project,
            cache,
            launcher,
            upstream,
            backups,
            launcher_sha256,
            upstream_sha256,
        }
    }

    fn request(
        &self,
        agent_id: &str,
        scope: CodebaseMemoryIntegrationScope,
        surface: CodebaseMemoryIntegrationSurface,
    ) -> CodebaseMemoryIntegrationRequest {
        CodebaseMemoryIntegrationRequest {
            agent_id: agent_id.to_string(),
            scope,
            project_root: Some(self.project.clone()),
            launcher_path: self.launcher.clone(),
            launcher_sha256: self.launcher_sha256.clone(),
            upstream_path: self.upstream.clone(),
            upstream_sha256: self.upstream_sha256.clone(),
            cache_dir: self.cache.clone(),
            surface,
        }
    }
}

#[test]
fn opencode_project_mcp_preview_apply_verify_and_exact_rollback() {
    let fixture = Fixture::new();
    let target = fixture.project.join("opencode.json");
    let original = r#"{
  "mcp": {
    "existing": {
      "type": "remote",
      "url": "http://127.0.0.1:9000"
    }
  },
  "theme": "system"
}
"#;
    fs::write(&target, original).expect("existing OpenCode config");

    let prepared = prepare_codebase_memory_integration(
        &fixture.request(
            "opencode",
            CodebaseMemoryIntegrationScope::Project,
            CodebaseMemoryIntegrationSurface::Mcp,
        ),
        &fixture.home,
        &fixture.backups,
    )
    .expect("OpenCode MCP preview");
    assert_eq!(prepared.plan.surface, CodebaseMemoryIntegrationSurface::Mcp);
    assert_eq!(prepared.plan.agent_id, "opencode");
    assert_eq!(prepared.plan.launcher_sha256, fixture.launcher_sha256);
    assert_eq!(prepared.plan.upstream_sha256, fixture.upstream_sha256);
    assert_eq!(
        prepared.plan.canonical_project_root.as_deref(),
        Some(fixture.project.as_path())
    );
    let review_digest = prepared.plan.review_digest.clone();
    let receipt = apply_codebase_memory_integration(prepared, &fixture.backups, &review_digest)
        .expect("apply OpenCode MCP");
    assert_eq!(receipt.surface, CodebaseMemoryIntegrationSurface::Mcp);
    assert!(receipt.rollback_id.is_some());

    let config: Value =
        serde_json::from_slice(&fs::read(&target).expect("OpenCode config after apply"))
            .expect("valid OpenCode JSON");
    assert!(config.pointer("/mcp/existing").is_some());
    assert_eq!(
        config.pointer("/mcp/codebase-memory-mcp/enabled"),
        Some(&Value::Bool(true))
    );
    assert_eq!(
        config.pointer("/mcp/codebase-memory-mcp/timeout"),
        Some(&json!(60000))
    );
    assert_eq!(
        config.pointer("/mcp/codebase-memory-mcp/command"),
        Some(&json!([
            fixture.launcher,
            "mcp",
            "stdio",
            fixture.upstream,
            fixture.project,
            fixture.cache
        ]))
    );
    assert!(
        verify_codebase_memory_integration(&receipt.transaction_id, &fixture.backups)
            .expect("semantic verification")
            .ok
    );

    let rollback = rollback_codebase_memory_integration(&receipt.transaction_id, &fixture.backups)
        .expect("exact rollback");
    assert!(rollback.restored);
    assert_eq!(
        fs::read_to_string(&target).expect("restored config"),
        original
    );
}

#[test]
fn opencode_same_scope_direct_entry_fails_closed_without_changes() {
    let fixture = Fixture::new();
    let target = fixture.project.join("opencode.json");
    let original = r#"{
  "$schema": "https://opencode.ai/config.json",
  "mcp": {
    "codebase-memory-mcp": {
      "type": "local",
      "command": ["/previous/direct/codebase-memory-mcp"],
      "enabled": true
    }
  }
}
"#;
    fs::write(&target, original).expect("existing direct OpenCode config");
    let error = prepare_codebase_memory_integration(
        &fixture.request(
            "opencode",
            CodebaseMemoryIntegrationScope::Project,
            CodebaseMemoryIntegrationSurface::Mcp,
        ),
        &fixture.home,
        &fixture.backups,
    )
    .expect_err("unreviewed same-scope direct entry must fail closed");
    assert!(error.to_string().contains("already exists"));
    assert_eq!(fs::read_to_string(target).unwrap(), original);
}

#[test]
fn codex_mcp_is_supported_but_unproven_codex_hook_fails_closed() {
    let fixture = Fixture::new();
    let mcp_prepared = prepare_codebase_memory_integration(
        &fixture.request(
            "codex",
            CodebaseMemoryIntegrationScope::Project,
            CodebaseMemoryIntegrationSurface::Mcp,
        ),
        &fixture.home,
        &fixture.backups,
    )
    .expect("Codex MCP preview");
    assert!(!mcp_prepared.plan.trust_required);
    assert!(mcp_prepared.plan.consent_summary.contains("MCP launcher"));

    let mcp_digest = mcp_prepared.plan.review_digest.clone();
    let mcp_receipt =
        apply_codebase_memory_integration(mcp_prepared, &fixture.backups, &mcp_digest)
            .expect("apply Codex MCP");
    assert_eq!(mcp_receipt.surface, CodebaseMemoryIntegrationSurface::Mcp);
    assert!(
        verify_codebase_memory_integration(&mcp_receipt.transaction_id, &fixture.backups)
            .expect("verify Codex MCP")
            .ok
    );

    let codex_toml =
        fs::read_to_string(fixture.project.join(".codex/config.toml")).expect("Codex MCP config");
    let codex = codex_toml.parse::<toml::Value>().expect("valid Codex TOML");
    assert_eq!(
        codex["mcp_servers"]["dtoken_codebase_memory"]["command"].as_str(),
        fixture.launcher.to_str()
    );
    let args = codex["mcp_servers"]["dtoken_codebase_memory"]["args"]
        .as_array()
        .expect("Codex MCP args")
        .iter()
        .map(|value| value.as_str().expect("string arg"))
        .collect::<Vec<_>>();
    assert_eq!(
        args,
        vec![
            "mcp",
            "stdio",
            fixture.upstream.to_str().unwrap(),
            fixture.project.to_str().unwrap(),
            fixture.cache.to_str().unwrap()
        ]
    );
    assert_eq!(
        codex["mcp_servers"]["dtoken_codebase_memory"]["startup_timeout_sec"].as_integer(),
        Some(60)
    );
    assert_eq!(
        codex["mcp_servers"]["dtoken_codebase_memory"]["tool_timeout_sec"].as_integer(),
        Some(120)
    );
    assert!(!fixture.project.join(".codex/hooks.json").exists());

    let hook_error = prepare_codebase_memory_integration(
        &fixture.request(
            "codex",
            CodebaseMemoryIntegrationScope::Project,
            CodebaseMemoryIntegrationSurface::Hook,
        ),
        &fixture.home,
        &fixture.backups,
    )
    .expect_err("Codex has no proven SessionStart Hook configuration contract");
    assert!(hook_error.to_string().contains("unsupported"));
    assert!(!fixture.project.join(".codex/hooks.json").exists());
    assert!(fixture.project.join(".codex/config.toml").exists());
    rollback_codebase_memory_integration(&mcp_receipt.transaction_id, &fixture.backups)
        .expect("rollback MCP");
    assert!(!fixture.project.join(".codex/config.toml").exists());
}

#[test]
fn claude_project_mcp_emits_the_full_launcher_contract() {
    let fixture = Fixture::new();
    let prepared = prepare_codebase_memory_integration(
        &fixture.request(
            "claude-code",
            CodebaseMemoryIntegrationScope::Project,
            CodebaseMemoryIntegrationSurface::Mcp,
        ),
        &fixture.home,
        &fixture.backups,
    )
    .expect("Claude project MCP preview");
    assert_eq!(prepared.plan.launcher_sha256, fixture.launcher_sha256);
    assert_eq!(prepared.plan.upstream_sha256, fixture.upstream_sha256);
    let digest = prepared.plan.review_digest.clone();
    let receipt = apply_codebase_memory_integration(prepared, &fixture.backups, &digest)
        .expect("apply Claude project MCP");

    let config: Value = serde_json::from_slice(
        &fs::read(fixture.project.join(".mcp.json")).expect("Claude MCP config"),
    )
    .expect("valid Claude MCP JSON");
    assert_eq!(
        config.pointer("/mcpServers/dtoken-codebase-memory"),
        Some(&json!({
            "command": fixture.launcher,
            "args": [
                "mcp",
                "stdio",
                fixture.upstream,
                fixture.project,
                fixture.cache
            ]
        }))
    );
    assert!(
        verify_codebase_memory_integration(&receipt.transaction_id, &fixture.backups)
            .expect("verify Claude MCP")
            .ok
    );
    rollback_codebase_memory_integration(&receipt.transaction_id, &fixture.backups)
        .expect("rollback Claude MCP");
    assert!(!fixture.project.join(".mcp.json").exists());
}

#[test]
fn claude_project_hook_merges_verifies_and_rolls_back_exactly() {
    let fixture = Fixture::new();
    let target = fixture.project.join(".claude/settings.local.json");
    fs::create_dir_all(target.parent().expect("Claude config parent"))
        .expect("Claude config directory");
    let original = r#"{
  "hooks": {
    "SessionStart": [
      {
        "matcher": "startup",
        "hooks": [{"type": "command", "command": "existing-hook"}]
      }
    ]
  },
  "theme": "dark"
}
"#;
    fs::write(&target, original).expect("existing Claude settings");
    let prepared = prepare_codebase_memory_integration(
        &fixture.request(
            "claude-code",
            CodebaseMemoryIntegrationScope::Project,
            CodebaseMemoryIntegrationSurface::Hook,
        ),
        &fixture.home,
        &fixture.backups,
    )
    .expect("Claude Hook preview");
    assert!(prepared.plan.trust_required);
    let digest = prepared.plan.review_digest.clone();
    let receipt = apply_codebase_memory_integration(prepared, &fixture.backups, &digest)
        .expect("apply Claude Hook");
    assert!(
        verify_codebase_memory_integration(&receipt.transaction_id, &fixture.backups)
            .expect("verify Claude Hook")
            .ok
    );

    let config: Value = serde_json::from_slice(&fs::read(&target).expect("Claude Hook config"))
        .expect("valid Claude settings");
    let entries = config["hooks"]["SessionStart"]
        .as_array()
        .expect("SessionStart array");
    assert!(entries
        .iter()
        .any(|entry| entry.to_string().contains("existing-hook")));
    let managed = entries
        .iter()
        .find(|entry| entry["dtoken_managed_id"] == "dtoken-codebase-memory-session-start-v1")
        .expect("managed Codebase Memory Hook");
    assert_eq!(
        managed["hooks"][0]["command"],
        format!("\"{}\" hook session-start", fixture.launcher.display())
    );
    assert!(managed["hooks"][0].get("statusMessage").is_none());

    rollback_codebase_memory_integration(&receipt.transaction_id, &fixture.backups)
        .expect("rollback Claude Hook");
    assert_eq!(
        fs::read_to_string(target).expect("restored settings"),
        original
    );
}

#[test]
fn stale_review_digest_writes_nothing() {
    let fixture = Fixture::new();
    let target = fixture.project.join("opencode.json");
    let prepared = prepare_codebase_memory_integration(
        &fixture.request(
            "opencode",
            CodebaseMemoryIntegrationScope::Project,
            CodebaseMemoryIntegrationSurface::Mcp,
        ),
        &fixture.home,
        &fixture.backups,
    )
    .expect("OpenCode preview");
    let error = apply_codebase_memory_integration(prepared, &fixture.backups, &"0".repeat(64))
        .expect_err("stale review must fail");
    assert!(error.to_string().contains("review is stale"));
    assert!(!target.exists());
    assert!(!fixture.backups.exists());
}

#[test]
fn review_digest_binds_cache_and_surface_context() {
    let fixture = Fixture::new();
    let target = fixture.project.join("opencode.json");
    let mut prepared = prepare_codebase_memory_integration(
        &fixture.request(
            "opencode",
            CodebaseMemoryIntegrationScope::Project,
            CodebaseMemoryIntegrationSurface::Mcp,
        ),
        &fixture.home,
        &fixture.backups,
    )
    .expect("OpenCode preview");
    let reviewed_digest = prepared.plan.review_digest.clone();
    prepared.plan.cache_dir = fixture.project.clone();
    let error = apply_codebase_memory_integration(prepared, &fixture.backups, &reviewed_digest)
        .expect_err("mutated security context must invalidate the review");
    assert!(error.to_string().contains("review digest is invalid"));
    assert!(!target.exists());
    assert!(!fixture.backups.exists());
}

#[test]
fn request_and_persisted_manifest_reject_unknown_fields() {
    let fixture = Fixture::new();
    let mut request = json!({
        "agent_id": "codex",
        "scope": "project",
        "project_root": fixture.project,
        "launcher_path": fixture.launcher,
        "launcher_sha256": fixture.launcher_sha256,
        "upstream_path": fixture.upstream,
        "upstream_sha256": fixture.upstream_sha256,
        "cache_dir": fixture.cache,
        "surface": "mcp",
        "agent_ids": ["codex", "opencode"]
    });
    assert!(serde_json::from_value::<CodebaseMemoryIntegrationRequest>(request.clone()).is_err());
    request
        .as_object_mut()
        .expect("request object")
        .remove("agent_ids");
    request
        .as_object_mut()
        .expect("request object")
        .remove("surface");
    assert!(serde_json::from_value::<CodebaseMemoryIntegrationRequest>(request).is_err());

    let prepared = prepare_codebase_memory_integration(
        &fixture.request(
            "codex",
            CodebaseMemoryIntegrationScope::Project,
            CodebaseMemoryIntegrationSurface::Mcp,
        ),
        &fixture.home,
        &fixture.backups,
    )
    .expect("Codex preview");
    let digest = prepared.plan.review_digest.clone();
    let receipt = apply_codebase_memory_integration(prepared, &fixture.backups, &digest)
        .expect("apply Codex transaction");
    let manifest_path = fixture
        .backups
        .join("codebase-memory-integration-transactions")
        .join(format!("{}.json", receipt.transaction_id));
    let mut manifest: Value =
        serde_json::from_slice(&fs::read(&manifest_path).expect("transaction manifest"))
            .expect("manifest JSON");
    manifest["unknown_security_override"] = Value::Bool(true);
    fs::write(
        &manifest_path,
        serde_json::to_vec_pretty(&manifest).expect("tampered manifest JSON"),
    )
    .expect("tamper manifest");
    assert!(
        get_codebase_memory_integration_transaction(&receipt.transaction_id, &fixture.backups)
            .is_err()
    );
}

#[test]
fn semantic_verification_detects_upstream_change_but_rollback_remains_available() {
    let fixture = Fixture::new();
    let target = fixture.project.join("opencode.json");
    fs::write(&target, "{\"mcp\":{}}\n").expect("initial OpenCode config");
    let prepared = prepare_codebase_memory_integration(
        &fixture.request(
            "opencode",
            CodebaseMemoryIntegrationScope::Project,
            CodebaseMemoryIntegrationSurface::Mcp,
        ),
        &fixture.home,
        &fixture.backups,
    )
    .expect("OpenCode preview");
    let digest = prepared.plan.review_digest.clone();
    let receipt = apply_codebase_memory_integration(prepared, &fixture.backups, &digest)
        .expect("apply OpenCode integration");
    fs::write(&fixture.upstream, b"different upstream bytes").expect("replace upstream");
    let error = verify_codebase_memory_integration(&receipt.transaction_id, &fixture.backups)
        .expect_err("upstream identity change must fail verification");
    assert!(error.to_string().contains("upstream identity changed"));
    rollback_codebase_memory_integration(&receipt.transaction_id, &fixture.backups)
        .expect("rollback must not depend on current upstream bytes");
    assert_eq!(
        fs::read_to_string(&target).expect("restored OpenCode config"),
        "{\"mcp\":{}}\n"
    );
}

#[test]
fn unsupported_opencode_hook_fails_closed() {
    let fixture = Fixture::new();
    let error = prepare_codebase_memory_integration(
        &fixture.request(
            "opencode",
            CodebaseMemoryIntegrationScope::Project,
            CodebaseMemoryIntegrationSurface::Hook,
        ),
        &fixture.home,
        &fixture.backups,
    )
    .expect_err("OpenCode Hook support is not established");
    assert!(error.to_string().contains("unsupported"));
}

#[test]
fn global_scope_still_requires_and_emits_an_explicit_approved_root() {
    let fixture = Fixture::new();
    let mut missing_root = fixture.request(
        "codex",
        CodebaseMemoryIntegrationScope::Global,
        CodebaseMemoryIntegrationSurface::Mcp,
    );
    missing_root.project_root = None;
    let error = prepare_codebase_memory_integration(&missing_root, &fixture.home, &fixture.backups)
        .expect_err("global integration without an approved root must fail");
    assert!(error.to_string().contains("explicit approved project root"));

    let prepared = prepare_codebase_memory_integration(
        &fixture.request(
            "codex",
            CodebaseMemoryIntegrationScope::Global,
            CodebaseMemoryIntegrationSurface::Mcp,
        ),
        &fixture.home,
        &fixture.backups,
    )
    .expect("global Codex preview with approved root");
    assert_eq!(
        prepared.plan.canonical_project_root.as_deref(),
        Some(fixture.project.as_path())
    );
    let digest = prepared.plan.review_digest.clone();
    let receipt = apply_codebase_memory_integration(prepared, &fixture.backups, &digest)
        .expect("apply global Codex MCP");
    let config =
        fs::read_to_string(fixture.home.join(".codex/config.toml")).expect("global Codex config");
    assert!(config.contains(fixture.project.to_str().unwrap()));
    rollback_codebase_memory_integration(&receipt.transaction_id, &fixture.backups)
        .expect("rollback global Codex MCP");
}

/// Uses the caller's normal OpenCode configuration and provider/model. It is
/// ignored by default because it launches the real Agent and performs a
/// temporary, fully rolled-back project-scoped configuration transaction.
#[test]
#[ignore = "requires explicit real OpenCode acceptance authorization and pinned local artifacts"]
fn real_opencode_codebase_memory_acceptance_restores_exact_config() {
    assert_eq!(
        std::env::var("DTOKEN_RUN_REAL_OPENCODE_CBM")
            .ok()
            .as_deref(),
        Some("1"),
        "explicit acceptance environment gate is required"
    );
    let project = canonical_env_path("DTOKEN_CBM_ACCEPTANCE_PROJECT");
    let launcher = canonical_env_path("DTOKEN_CBM_ACCEPTANCE_LAUNCHER");
    let upstream = canonical_env_path("DTOKEN_CBM_ACCEPTANCE_UPSTREAM");
    let cache = canonical_env_path("DTOKEN_CBM_ACCEPTANCE_CACHE");
    let backups = PathBuf::from(
        std::env::var("DTOKEN_CBM_ACCEPTANCE_BACKUPS")
            .expect("DTOKEN_CBM_ACCEPTANCE_BACKUPS is required"),
    );
    let home = PathBuf::from(std::env::var("HOME").expect("normal HOME is required"));
    let request = CodebaseMemoryIntegrationRequest {
        agent_id: "opencode".to_string(),
        scope: CodebaseMemoryIntegrationScope::Project,
        project_root: Some(project.clone()),
        launcher_path: launcher.clone(),
        launcher_sha256: hash_file(&launcher),
        upstream_path: upstream.clone(),
        upstream_sha256: hash_file(&upstream),
        cache_dir: cache.clone(),
        surface: CodebaseMemoryIntegrationSurface::Mcp,
    };
    let prepared = prepare_codebase_memory_integration(&request, &home, &backups)
        .expect("real OpenCode integration preview");
    let digest = prepared.plan.review_digest.clone();
    let receipt = apply_codebase_memory_integration(prepared, &backups, &digest)
        .expect("real OpenCode integration apply");
    let acceptance = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        assert!(
            verify_codebase_memory_integration(&receipt.transaction_id, &backups)
                .expect("real OpenCode integration verification")
                .ok
        );
        let mcp_probe = Command::new("opencode")
            .args(["mcp", "list"])
            .current_dir(&project)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .expect("probe real OpenCode MCP configuration");
        assert!(mcp_probe.success(), "real OpenCode MCP probe failed");
        let mut command = Command::new("opencode");
        command.arg("run").arg("--agent").arg("build");
        if let Ok(model) = std::env::var("DTOKEN_CBM_ACCEPTANCE_MODEL") {
            assert!(!model.trim().is_empty(), "acceptance model cannot be empty");
            command.arg("--model").arg(model);
        }
        let mut child = command
            .arg("You must call the codebase-memory-mcp get_architecture tool for this project before replying. Do not use shell or file-search tools. If the tool is unavailable, fail instead of answering from context.")
            .current_dir(&project)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("launch real OpenCode");
        let deadline = Instant::now() + Duration::from_secs(240);
        let status = loop {
            if let Some(status) = child.try_wait().expect("poll real OpenCode") {
                break status;
            }
            if Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                panic!("real OpenCode Codebase Memory acceptance timed out");
            }
            thread::sleep(Duration::from_millis(250));
        };
        assert!(status.success(), "real OpenCode exited unsuccessfully");
        let session: Value = serde_json::from_slice(
            &fs::read(cache.join(".dtoken-agent-session.json"))
                .expect("metadata-only Agent session receipt"),
        )
        .expect("valid metadata-only Agent session receipt");
        assert_eq!(session["owner"], "agent");
        assert!(session["initialize_count"].as_u64().unwrap_or(0) > 0);
        assert!(session["tool_call_count"].as_u64().unwrap_or(0) > 0);
        assert!(
            session["tool_call_count"].as_u64().unwrap_or(0)
                > session["denied_count"].as_u64().unwrap_or(0)
        );
    }));
    let rollback = rollback_codebase_memory_integration(&receipt.transaction_id, &backups)
        .expect("exact real OpenCode integration rollback");
    assert!(rollback.restored);
    if let Err(payload) = acceptance {
        std::panic::resume_unwind(payload);
    }
}

fn canonical_env_path(name: &str) -> PathBuf {
    fs::canonicalize(std::env::var(name).unwrap_or_else(|_| panic!("{name} is required")))
        .unwrap_or_else(|error| panic!("{name} must identify an existing path: {error}"))
}

fn hash_file(path: &Path) -> String {
    sha256(&fs::read(path).expect("read acceptance artifact"))
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
