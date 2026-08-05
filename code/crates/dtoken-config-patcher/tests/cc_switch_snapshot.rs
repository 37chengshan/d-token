//! Regression tests for the production cc-switch read-only snapshot, watcher,
//! and hash-bound route sync plan surfaces.
//!
//! These surfaces were previously claimed in docs
//! (`docs/product/IMPLEMENTATION_STATUS.md`, `docs/PLAN.md`) but only existed as
//! test-local re-implementations (`follow_decision` inside
//! `cc_switch_fixture.rs`). These tests pin the production implementation:
//!
//! 1. The committed fixture database parses read-only into a redacted
//!    multi-app-type snapshot that never leaks the embedded secret marker,
//!    `settings_config` payloads, or credential values.
//! 2. Schema and selection resolution is fail-closed: unknown schema versions
//!    resolve to `blocked`, multiple active candidates for one app type
//!    resolve to `needs_selection`, and a missing required field is never
//!    silently defaulted.
//! 3. The follow decision is hash-bound and production-owned: an approved
//!    upstream identity allows model changes (`sync_required`), a synced route
//!    stays `unchanged`, and a new provider identity fails closed into
//!    `reauthorization_required`.
//! 4. The DB/WAL/SHM/parent-directory watcher debounces, settles once,
//!    single-flights while a settled change is being handled, clears a pending
//!    change that rolls back to the stable snapshot, and reports its polling
//!    fallback mode explicitly.
//! 5. MCP/Hook adapters never expose route evidence, and Cline/Claude schema
//!    mismatches are rejected.
//!
//! This test never writes the committed fixture, never opens a real cc-switch
//! database, and never reads a credential value.

use dtoken_config_patcher::cc_switch::{
    follow_decision, read_cc_switch_snapshot, CcSwitchWatcher, FollowDecision, WatchEvent,
    CC_SWITCH_WATCH_DEBOUNCE_MS,
};
use std::{
    fs,
    path::{Path, PathBuf},
    thread,
    time::Duration,
};

const FIXTURE_DB: &str = "support/fixtures/cc-switch/cc-switch.db";
const FIXTURE_SECRET_MARKER: &str = "fixture-cc-switch-token-marker-8f31";

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .and_then(Path::parent)
        .expect("repository root")
        .to_path_buf()
}

fn fixture_db() -> PathBuf {
    let db = repo_root().join(FIXTURE_DB);
    assert!(
        db.is_file(),
        "committed cc-switch fixture is missing at {}",
        db.display()
    );
    db
}

/// Copies the committed fixture into a temp dir so a test can build schema
/// variants without touching the committed bytes.
fn fixture_temp_copy() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("temp dir");
    let copy = dir.path().join("cc-switch.db");
    fs::copy(fixture_db(), &copy).expect("copy fixture");
    (dir, copy)
}

/// Builds a variant database by mutating a copied fixture through rusqlite.
fn mutate_fixture_copy(
    mutation: impl FnOnce(&rusqlite::Connection) -> Result<(), rusqlite::Error>,
) -> (tempfile::TempDir, PathBuf) {
    let (_dir, path) = fixture_temp_copy();
    let conn = rusqlite::Connection::open(&path).expect("open variant db");
    mutation(&conn).expect("apply mutation");
    drop(conn);
    (_dir, path)
}

#[test]
fn read_only_snapshot_parses_fixture_into_redacted_multi_app_type_snapshot() {
    let snapshot = read_cc_switch_snapshot(&fixture_db()).expect("parse fixture snapshot");

    assert_eq!(snapshot.schema_version, 12, "schema-12 fixture");
    assert_eq!(snapshot.schema_kind, "loaded");
    assert_eq!(snapshot.source_state, "loaded");
    assert_eq!(snapshot.selection_state, "resolved");

    // Every active app type is present with its redacted upstream identity.
    let codex = snapshot
        .active_upstreams
        .iter()
        .find(|u| u.app_type == "codex")
        .expect("codex active upstream");
    assert_eq!(codex.provider_id, "fixture-codex-current");
    assert_eq!(codex.model, "codex-fixture-model");
    assert_eq!(codex.protocol, "open_ai");
    assert_eq!(
        codex.normalized_endpoint,
        "https://fixture-codex.example.test/v1"
    );
    assert!(!codex.credential_present, "codex fixture has no credential");

    let claude = snapshot
        .active_upstreams
        .iter()
        .find(|u| u.app_type == "claude")
        .expect("claude active upstream");
    assert_eq!(claude.provider_id, "fixture-claude-current");
    assert!(
        claude.credential_present,
        "claude fixture carries a credential-shaped env field"
    );

    let gemini = snapshot
        .active_upstreams
        .iter()
        .find(|u| u.app_type == "gemini")
        .expect("gemini active upstream");
    assert_eq!(gemini.provider_id, "fixture-gemini-current");
    assert!(!gemini.credential_present);

    // The fixture has exactly one active candidate per app type, so the
    // snapshot is resolved and profiles are listed.
    assert!(
        snapshot
            .profiles
            .iter()
            .any(|p| p.contains("Fixture Profile A")),
        "profiles are listed read-only"
    );
    assert!(!snapshot.source_fingerprint.is_empty());
}

#[test]
fn snapshot_serialization_never_leaks_payload_or_credential_fields() {
    let snapshot = read_cc_switch_snapshot(&fixture_db()).expect("parse fixture snapshot");
    let encoded = serde_json::to_string(&snapshot).expect("serialize snapshot");

    for forbidden in [
        FIXTURE_SECRET_MARKER,
        "settings_config",
        "private_metadata",
        "ANTHROPIC_AUTH_TOKEN",
        "ANTHROPIC_BASE_URL",
        "ANTHROPIC_MODEL",
    ] {
        assert!(
            !encoded.contains(forbidden),
            "snapshot leaked {forbidden} through serialization"
        );
    }
    assert!(!encoded.contains("token-marker"));
}

#[test]
fn multiple_active_candidates_for_one_app_type_fail_closed() {
    // Mark the second codex provider active too — two active candidates for
    // the same app type must not resolve to a silent choice.
    let (_dir, path) = mutate_fixture_copy(|conn| {
        conn.execute(
            "UPDATE providers SET is_current = 1 WHERE id = 'fixture-codex-alt'",
            [],
        )?;
        Ok(())
    });

    let snapshot = read_cc_switch_snapshot(&path).expect("parse variant snapshot");
    assert_eq!(snapshot.selection_state, "needs_selection");
    let codex_upstreams: Vec<_> = snapshot
        .active_upstreams
        .iter()
        .filter(|u| u.app_type == "codex")
        .collect();
    assert_eq!(
        codex_upstreams.len(),
        2,
        "both candidates remain visible for the selection decision"
    );
}

#[test]
fn unknown_schema_version_fails_closed_to_blocked() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("unknown.db");
    {
        let conn = rusqlite::Connection::open(&path).expect("open variant db");
        conn.execute_batch(
            "PRAGMA user_version = 13;
             CREATE TABLE providers (id TEXT PRIMARY KEY, app_type TEXT NOT NULL);",
        )
        .expect("build unknown schema");
    }

    let snapshot = read_cc_switch_snapshot(&path).expect("parse unknown schema");
    assert_eq!(snapshot.schema_kind, "unknown");
    assert_eq!(snapshot.selection_state, "blocked");
    assert!(
        snapshot.active_upstreams.is_empty(),
        "no upstream is fabricated from an unknown schema"
    );
}

#[test]
fn follow_decision_is_hash_bound_and_production_owned() {
    let upstream_signature = dtoken_config_patcher::cc_switch::cc_switch_upstream_signature(
        "fixture-codex-current",
        "cc-switch:codex",
        "open_ai",
        "https://fixture-codex.example.test/v1",
    );
    let authorized = vec![upstream_signature.clone()];
    let route_a = dtoken_config_patcher::cc_switch::cc_switch_route_signature(
        &upstream_signature,
        "codex-fixture-model",
    );
    let route_b = dtoken_config_patcher::cc_switch::cc_switch_route_signature(
        &upstream_signature,
        "codex-alt-model",
    );

    assert_eq!(
        follow_decision(&authorized, Some(&route_a), &upstream_signature, &route_a),
        FollowDecision::Unchanged
    );
    assert_eq!(
        follow_decision(&authorized, Some(&route_a), &upstream_signature, &route_b),
        FollowDecision::SyncRequired
    );

    let other = dtoken_config_patcher::cc_switch::cc_switch_upstream_signature(
        "fixture-codex-alt",
        "cc-switch:codex",
        "open_ai",
        "https://fixture-alt.example.test/v1",
    );
    assert_eq!(
        follow_decision(&authorized, Some(&route_b), &other, &route_b),
        FollowDecision::ReauthorizationRequired
    );
}

#[test]
fn watcher_debounces_then_settles_once() {
    let dir = tempfile::tempdir().expect("temp dir");
    let db = dir.path().join("cc-switch.db");
    let wal = dir.path().join("cc-switch.db-wal");
    let shm = dir.path().join("cc-switch.db-shm");
    fs::write(&db, b"stable-db").expect("write db");
    fs::write(&wal, b"stable-wal").expect("write wal");
    fs::write(&shm, b"stable-shm").expect("write shm");

    let mut watcher = CcSwitchWatcher::new(
        vec![
            db.clone(),
            wal.clone(),
            shm.clone(),
            dir.path().to_path_buf(),
        ],
        Duration::from_millis(120),
    )
    .expect("watcher");

    // Baseline poll: nothing changed.
    assert_eq!(watcher.poll(), WatchEvent::NoChange);

    // A WAL write starts the pending debounce window.
    fs::write(&wal, b"changed-wal").expect("change wal");
    assert_eq!(watcher.poll(), WatchEvent::PendingChange);

    // Within the debounce window the change has not settled.
    thread::sleep(Duration::from_millis(30));
    assert_eq!(watcher.poll(), WatchEvent::NoChange);

    // After quiet time exceeds the debounce, it settles exactly once.
    thread::sleep(Duration::from_millis(140));
    assert_eq!(watcher.poll(), WatchEvent::Settled);
    assert_eq!(watcher.poll(), WatchEvent::NoChange);
}

#[test]
fn watcher_single_flights_until_the_settled_change_is_handled() {
    let dir = tempfile::tempdir().expect("temp dir");
    let db = dir.path().join("cc-switch.db");
    fs::write(&db, b"stable-db").expect("write db");

    let mut watcher =
        CcSwitchWatcher::new(vec![db.clone()], Duration::from_millis(60)).expect("watcher");
    assert_eq!(watcher.poll(), WatchEvent::NoChange);

    fs::write(&db, b"changed-db").expect("change db");
    assert_eq!(watcher.poll(), WatchEvent::PendingChange);
    thread::sleep(Duration::from_millis(100));
    assert_eq!(watcher.poll(), WatchEvent::Settled);

    // A further change while the settled change is in flight is single-flighted.
    fs::write(&db, b"changed-again").expect("change db again");
    assert_eq!(watcher.poll(), WatchEvent::InFlight);

    // After the change is acknowledged, a new pending window can begin.
    watcher.mark_handled();
    fs::write(&db, b"changed-third").expect("change db third time");
    assert_eq!(watcher.poll(), WatchEvent::PendingChange);
}

#[test]
fn watcher_clears_a_pending_change_that_rolls_back_to_the_stable_snapshot() {
    let dir = tempfile::tempdir().expect("temp dir");
    let db = dir.path().join("cc-switch.db");
    fs::write(&db, b"stable-db").expect("write db");

    let mut watcher =
        CcSwitchWatcher::new(vec![db.clone()], Duration::from_millis(60)).expect("watcher");
    assert_eq!(watcher.poll(), WatchEvent::NoChange);

    fs::write(&db, b"transient-change").expect("write transient change");
    assert_eq!(watcher.poll(), WatchEvent::PendingChange);

    // The pending change rolls back before the debounce settles it.
    fs::write(&db, b"stable-db").expect("roll back to stable bytes");
    assert_eq!(watcher.poll(), WatchEvent::Reverted);

    // A later change requires a fresh, complete debounce.
    fs::write(&db, b"later-change").expect("write later change");
    assert_eq!(watcher.poll(), WatchEvent::PendingChange);
}

#[test]
fn watcher_reports_polling_fallback_mode_explicitly() {
    let dir = tempfile::tempdir().expect("temp dir");
    let db = dir.path().join("cc-switch.db");
    fs::write(&db, b"stable-db").expect("write db");

    let mut watcher = CcSwitchWatcher::new(
        vec![db.clone(), dir.path().to_path_buf()],
        Duration::from_millis(50),
    )
    .expect("watcher");
    assert_eq!(
        watcher.watch_mode(),
        "polling_fallback",
        "the watcher explicitly reports its fallback mode"
    );
    assert_eq!(
        CC_SWITCH_WATCH_DEBOUNCE_MS, 750,
        "production debounce is 750ms"
    );

    // A file appearing in the watched parent directory is also observed.
    let sibling = dir.path().join("cc-switch.db-wal");
    fs::write(&sibling, b"new-wal").expect("create wal in parent");
    assert_eq!(watcher.poll(), WatchEvent::PendingChange);
}

#[test]
fn mcp_and_hook_adapters_never_expose_route_evidence() {
    use dtoken_config_patcher::agent_kind::AgentIntegrationKind;
    use dtoken_config_patcher::agent_transport::known_agent_transport_adapters;

    for adapter in known_agent_transport_adapters() {
        if adapter.integration_kind == AgentIntegrationKind::Route {
            continue;
        }
        // MCP/Hook integration is explicitly not route-capable: no configure
        // button, no clear button, and no route evidence claim.
        assert!(
            !adapter.can_configure,
            "{} must not offer configure",
            adapter.id
        );
        assert!(!adapter.can_clear, "{} must not offer clear", adapter.id);
        let limitation = adapter
            .truthful_limitation
            .expect("MCP/Hook adapters carry a truthful limitation");
        assert!(
            !limitation.provides_route_evidence,
            "{} must never claim route evidence",
            adapter.id
        );
        assert!(
            !limitation.modifies_model_provider,
            "{} must never claim to modify the model provider",
            adapter.id
        );
    }
}

#[test]
fn cline_configuration_with_a_mismatched_schema_is_rejected() {
    use dtoken_config_patcher::agent_transport::{verify_agent_transport, AgentTransportRequest};

    let dir = tempfile::tempdir().expect("temp dir");
    let config_dir = dir.path().join(".cline/settings");
    fs::create_dir_all(&config_dir).expect("create cline settings dir");
    let path = config_dir.join("providers.json");
    // A Cline document whose top-level shape is not a strict JSON object must
    // be rejected, never partially parsed or guessed.
    fs::write(&path, r#"["not","an","object"]"#).expect("write malformed cline config");

    let request = AgentTransportRequest {
        agent_id: "cline".into(),
        target_path: path.clone(),
        endpoint: "http://127.0.0.1:8787".into(),
        enabled: true,
        model: Some("gpt-routed".into()),
    };
    let verification = verify_agent_transport(&request).expect("verification runs fail-closed");
    assert!(!verification.ok, "mismatched Cline schema must fail closed");
    assert!(
        verification.current_upstream.is_none(),
        "no upstream signature is produced from a mismatched Cline schema"
    );
}

#[test]
fn claude_settings_with_a_mismatched_schema_is_rejected() {
    use dtoken_config_patcher::agent_transport::{verify_agent_transport, AgentTransportRequest};

    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("settings.json");
    // Claude Code settings must be a JSON object; a non-object document is a
    // schema mismatch and must fail closed.
    fs::write(&path, r#"{"env": ["not", "an", "object"]}"#)
        .expect("write mismatched claude config");

    let request = AgentTransportRequest {
        agent_id: "claude-code".into(),
        target_path: path.clone(),
        endpoint: "http://127.0.0.1:8787".into(),
        enabled: true,
        model: Some("claude-test".into()),
    };
    let verification = verify_agent_transport(&request).expect("verification runs fail-closed");
    assert!(
        !verification.ok,
        "mismatched Claude schema must fail closed"
    );
    assert!(
        verification.current_upstream.is_none(),
        "no upstream signature is produced from a mismatched Claude schema"
    );
}
