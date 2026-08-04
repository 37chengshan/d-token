use dtoken_context_store::{
    parse_memory_write_request, CcrMetadataProjection, ContextStore, FailureClass,
    FailurePatternObservation, MemoryAccessScope, MemoryCapabilityPermission,
    MemoryCapabilityRouteBinding, MemoryCapabilityTransactionRequest,
    MemoryCapabilityTransactionState, MemoryProvenance, MemoryShareGrantRequest,
    MemoryShareGrantScope, MemoryWriteRequest, RequestProjection, ScopedMemoryShareGrantRequest,
};
use dtoken_types::agent::{
    AgentDetectionEvidence, AgentDiscoverySnapshot, AgentEvidenceKind, AgentInstallCapability,
    AgentSessionCapability, DetectedAgent,
};
use dtoken_types::{
    CapabilityEvidence, Protocol, ProviderCacheObservation, ProviderCacheObservationKind,
};
use uuid::Uuid;

fn temp_db() -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("dtoken-context-store-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&root).expect("create isolated context-store test directory");
    root.join("runtime-state.sqlite3")
}

#[test]
fn migration_is_idempotent_and_schema_has_no_raw_content_columns() {
    let path = temp_db();
    let store = ContextStore::open(&path).expect("open store");
    assert_eq!(store.schema_version().unwrap(), 7);
    drop(store);
    let store = ContextStore::open(&path).expect("reopen store");
    assert_eq!(store.schema_version().unwrap(), 7);

    for table in store.table_names().unwrap() {
        for column in store.column_names(&table).unwrap() {
            let lower = column.to_ascii_lowercase();
            assert!(
                !["prompt", "response", "body", "content", "secret", "token"]
                    .iter()
                    .any(|forbidden| lower.contains(forbidden)),
                "forbidden raw-content column {table}.{column}"
            );
        }
    }
    let memory_columns = store.column_names("memory_entry").unwrap();
    assert!(memory_columns.contains(&"summary".to_string()));
    assert!(memory_columns.contains(&"keywords_json".to_string()));
    drop(store);
    let _ = std::fs::remove_file(path);
}

#[test]
fn agent_snapshot_round_trips_and_replaces_stale_rows_atomically() {
    let path = temp_db();
    let mut store = ContextStore::open(&path).expect("open store");
    let snapshot = AgentDiscoverySnapshot {
        observed_at_ms: 42,
        evidence_revision: "agent-revision-a".to_string(),
        agents: vec![DetectedAgent {
            id: "codex".to_string(),
            name: "Codex".to_string(),
            install_capability: AgentInstallCapability::ConfirmRequired,
            session_capability: AgentSessionCapability::Metadata,
            evidence: vec![AgentDetectionEvidence {
                kind: AgentEvidenceKind::SessionRoot,
                path: std::path::PathBuf::from("/safe/codex/sessions"),
            }],
        }],
    };

    store
        .replace_agent_snapshot(&snapshot)
        .expect("store snapshot");
    assert_eq!(
        store.agent_snapshot().expect("read snapshot"),
        Some(snapshot)
    );

    let empty = AgentDiscoverySnapshot {
        observed_at_ms: 84,
        evidence_revision: "agent-revision-empty".to_string(),
        agents: vec![],
    };
    store
        .replace_agent_snapshot(&empty)
        .expect("replace snapshot");
    assert_eq!(
        store.agent_snapshot().expect("read empty snapshot"),
        Some(empty)
    );
    drop(store);
    let _ = std::fs::remove_file(path);
}

#[test]
fn legacy_v1_store_is_upgraded_without_recreating_existing_tables() {
    let path = temp_db();
    let connection = rusqlite::Connection::open(&path).expect("legacy database");
    connection
        .execute_batch(
            "CREATE TABLE schema_migrations (
                 version INTEGER PRIMARY KEY,
                 applied_at_ms INTEGER NOT NULL
             );
             INSERT INTO schema_migrations (version, applied_at_ms) VALUES (1, 1);
             CREATE TABLE runtime_revisions (
                 domain TEXT PRIMARY KEY NOT NULL,
                 revision TEXT NOT NULL,
                 freshness TEXT NOT NULL,
                 collected_at_ms INTEGER NOT NULL
             );",
        )
        .expect("legacy v1 schema");
    drop(connection);

    let store = ContextStore::open(&path).expect("upgrade v1 store");
    assert_eq!(store.schema_version().unwrap(), 7);
    assert!(store
        .table_names()
        .unwrap()
        .contains(&"agent_detection".to_string()));
    drop(store);
    let _ = std::fs::remove_file(path);
}

#[test]
fn provider_cache_observation_round_trips_only_for_exact_identity() {
    let path = temp_db();
    let store = ContextStore::open(&path).expect("open store");
    let observation = ProviderCacheObservation::new(
        "route-a",
        "provider-a",
        "model-a",
        Protocol::Anthropic,
        "config-a",
        ProviderCacheObservationKind::AnthropicUsage,
        Some(21),
        Some(8),
        Some(5),
        Some(3),
        "provider-cache-observation-v1",
        42,
    )
    .expect("valid observation");

    store
        .upsert_provider_cache_observation(&observation)
        .expect("persist observation");
    assert_eq!(
        store
            .provider_cache_observation(
                "route-a",
                "provider-a",
                "model-a",
                &Protocol::Anthropic,
                "config-a",
            )
            .expect("read exact observation"),
        Some(observation)
    );
    assert!(store
        .provider_cache_observation(
            "route-a",
            "provider-a",
            "other-model",
            &Protocol::Anthropic,
            "config-a",
        )
        .expect("read mismatch")
        .is_none());
    assert!(store
        .provider_cache_observation(
            "route-a",
            "provider-a",
            "model-a",
            &Protocol::Anthropic,
            "other-config",
        )
        .expect("read stale config")
        .is_none());

    drop(store);
    let _ = std::fs::remove_file(path);
}

#[test]
fn provider_cache_observation_survives_restart_and_replaces_older_counters() {
    let path = temp_db();
    let first = ProviderCacheObservation::new(
        "route-a",
        "provider-a",
        "model-a",
        Protocol::OpenAi,
        "config-a",
        ProviderCacheObservationKind::OpenAiResponsesUsage,
        Some(0),
        None,
        None,
        None,
        "provider-cache-observation-v1",
        10,
    )
    .unwrap();
    let latest = ProviderCacheObservation::new(
        "route-a",
        "provider-a",
        "model-a",
        Protocol::OpenAi,
        "config-a",
        ProviderCacheObservationKind::OpenAiResponsesUsage,
        Some(55),
        None,
        None,
        None,
        "provider-cache-observation-v1",
        20,
    )
    .unwrap();
    {
        let store = ContextStore::open(&path).unwrap();
        store.upsert_provider_cache_observation(&first).unwrap();
        store.upsert_provider_cache_observation(&latest).unwrap();
    }
    let store = ContextStore::open(&path).unwrap();
    assert_eq!(
        store
            .provider_cache_observation(
                "route-a",
                "provider-a",
                "model-a",
                &Protocol::OpenAi,
                "config-a",
            )
            .unwrap(),
        Some(latest)
    );
    drop(store);
    let _ = std::fs::remove_file(path);
}

#[test]
fn provider_cache_observation_rejects_missing_cache_fields_and_protocol_mismatch() {
    assert!(ProviderCacheObservation::new(
        "route-a",
        "provider-a",
        "model-a",
        Protocol::OpenAi,
        "config-a",
        ProviderCacheObservationKind::OpenAiChatUsage,
        None,
        None,
        None,
        None,
        "provider-cache-observation-v1",
        42,
    )
    .is_err());
    assert!(ProviderCacheObservation::new(
        "route-a",
        "provider-a",
        "model-a",
        Protocol::Anthropic,
        "config-a",
        ProviderCacheObservationKind::OpenAiChatUsage,
        Some(1),
        None,
        None,
        None,
        "provider-cache-observation-v1",
        42,
    )
    .is_err());
}

#[test]
fn capability_evidence_round_trips_through_the_projection() {
    let path = temp_db();
    let store = ContextStore::open(&path).unwrap();
    let evidence = CapabilityEvidence::unavailable(
        "mcp",
        "headroom-backend-capabilities-v1",
        "mcp_transport_not_implemented",
    )
    .unwrap();
    store.upsert_capability(&evidence).unwrap();
    assert_eq!(store.capability("mcp").unwrap(), Some(evidence));
    store
        .upsert_request(&RequestProjection {
            request_id: "request-a".to_string(),
            route_id: "route-a".to_string(),
            provider_id: "provider-a".to_string(),
            agent_id: Some("claude-code".to_string()),
            project_id: None,
            status: "ok".to_string(),
            input_units: Some(10),
            output_units: Some(4),
            saved_units: Some(2),
            observed_at_ms: 10,
        })
        .unwrap();
    store
        .upsert_ccr_metadata(&CcrMetadataProjection {
            recovery_ref: "ccr:request-a".to_string(),
            request_id: "request-a".to_string(),
            expires_at_ms: Some(100),
            plaintext_bytes: Some(50),
            state: "prepared".to_string(),
            updated_at_ms: 10,
        })
        .unwrap();
    assert_eq!(store.projection_counts().unwrap(), (1, 1));
    drop(store);
    let _ = std::fs::remove_file(path);
}

#[test]
fn request_projection_rebuild_replaces_rows_atomically_and_heals_crash_gaps() {
    let path = temp_db();
    let mut store = ContextStore::open(&path).unwrap();
    let projection = |request_id: &str, route_id: &str| RequestProjection {
        request_id: request_id.to_string(),
        route_id: route_id.to_string(),
        provider_id: "provider-a".to_string(),
        agent_id: Some("opencode".to_string()),
        project_id: Some("project-a".to_string()),
        status: "ok".to_string(),
        input_units: Some(10),
        output_units: Some(4),
        saved_units: Some(2),
        observed_at_ms: 10,
    };
    // Simulate a pre-crash SQLite view that is missing the newest journal
    // rows (the crash happened between the JSONL append and the upsert).
    store
        .upsert_request(&projection("request-old", "route-a"))
        .unwrap();
    assert_eq!(store.projection_counts().unwrap(), (1, 0));

    // Restart reconciliation rebuilds the whole view from the durable journal.
    let journal = vec![
        projection("request-old", "route-a"),
        projection("request-new", "route-b"),
        projection("request-newest", "route-c"),
    ];
    store.replace_request_projections(&journal).unwrap();
    assert_eq!(store.projection_counts().unwrap(), (3, 0));

    // Rebuilding again replaces, never duplicates.
    let journal = vec![projection("request-old", "route-a")];
    store.replace_request_projections(&journal).unwrap();
    assert_eq!(store.projection_counts().unwrap(), (1, 0));

    drop(store);
    let _ = std::fs::remove_file(path);
}

fn memory_request(
    project_id: &str,
    agent_id: &str,
    session_id: Option<&str>,
    revision: &str,
    observed_at_ms: u64,
) -> MemoryWriteRequest {
    MemoryWriteRequest {
        project_id: project_id.to_string(),
        agent_id: Some(agent_id.to_string()),
        session_id: session_id.map(str::to_string),
        kind: "project-guideline".to_string(),
        summary: "Run the focused Rust tests before the workspace gate.".to_string(),
        keywords: vec!["rust".to_string(), "tests".to_string()],
        dedup_key: "testing-guideline".to_string(),
        entry_revision: revision.to_string(),
        provenance: MemoryProvenance {
            source: "smart-learning".to_string(),
            source_revision: "learning-scan-v1".to_string(),
            observed_at_ms,
        },
    }
}

fn memory_capability_request(
    issued_at_ms: u64,
    expires_at_ms: u64,
) -> MemoryCapabilityTransactionRequest {
    MemoryCapabilityTransactionRequest {
        scope: MemoryAccessScope {
            project_id: "project-a".to_string(),
            agent_id: "opencode".to_string(),
            session_id: Some("session-a".to_string()),
        },
        route_binding: MemoryCapabilityRouteBinding {
            route_id: "route-a".to_string(),
            provider_id: "provider-a".to_string(),
            model: "provider/model-a".to_string(),
            config_revision: "a".repeat(64),
        },
        permissions: vec![
            MemoryCapabilityPermission::Write,
            MemoryCapabilityPermission::List,
            MemoryCapabilityPermission::Write,
        ],
        source_revision: "managed-agent-integration-v1".to_string(),
        issued_at_ms,
        expires_at_ms,
    }
}

#[test]
fn memory_capability_preview_for_runtime_is_write_free() {
    let runtime = std::env::temp_dir().join(format!(
        "dtoken-memory-capability-preview-{}",
        uuid::Uuid::new_v4()
    ));
    let issued_at_ms = dtoken_types::time::now_ms();
    let prepared = dtoken_context_store::prepare_memory_capability_transaction_for_runtime(
        &runtime,
        &memory_capability_request(issued_at_ms, issued_at_ms + 60_000),
    )
    .expect("prepare write-free capability preview");

    assert_eq!(
        prepared.plan.target_path.parent(),
        Some(runtime.join("memory-capabilities").as_path())
    );
    assert!(
        !runtime.exists(),
        "preview must not create the runtime directory or SQLite projection"
    );
}

#[test]
fn memory_capability_transaction_is_private_route_bound_restart_safe_and_revocable() {
    let path = temp_db();
    let issued_at_ms = dtoken_types::time::now_ms();
    let expires_at_ms = issued_at_ms + 60_000;
    let request = memory_capability_request(issued_at_ms, expires_at_ms);
    let (target, transaction_id, document_sha256, review_digest) = {
        let store = ContextStore::open(&path).expect("open store");
        let prepared = store
            .prepare_memory_capability_transaction(&request)
            .expect("prepare capability");
        assert_eq!(
            prepared.plan.permissions,
            vec![
                MemoryCapabilityPermission::List,
                MemoryCapabilityPermission::Write
            ]
        );
        assert!(prepared.plan.requires_explicit_confirmation);
        let redacted_plan = serde_json::to_string(&prepared.plan).expect("serialize plan");
        assert!(!redacted_plan.contains("capability_nonce"));

        let target = prepared.plan.target_path.clone();
        let transaction_id = prepared.plan.transaction_id.clone();
        let document_sha256 = prepared.plan.document_sha256.clone();
        let review_digest = prepared.plan.review_digest.clone();
        let receipt = store
            .apply_memory_capability_transaction(prepared, &review_digest)
            .expect("apply and verify capability");
        assert_eq!(receipt.state, MemoryCapabilityTransactionState::Verified);
        assert!(target.is_file());
        assert!(!serde_json::to_string(&receipt)
            .expect("serialize receipt")
            .contains("capability_nonce"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&target)
                    .expect("capability metadata")
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
        (target, transaction_id, document_sha256, review_digest)
    };

    let store = ContextStore::open(&path).expect("reopen store");
    let resolved = store
        .resolve_memory_capability_file(&target, &request.route_binding, issued_at_ms + 1)
        .expect("resolve verified capability after restart");
    assert_eq!(resolved.scope, request.scope);
    assert!(resolved.permits(MemoryCapabilityPermission::List));
    assert!(resolved.permits(MemoryCapabilityPermission::Write));
    assert!(!resolved.permits(MemoryCapabilityPermission::Delete));

    let mut mismatched = request.route_binding.clone();
    mismatched.model = "provider/other-model".to_string();
    assert!(store
        .resolve_memory_capability_file(&target, &mismatched, issued_at_ms + 1)
        .is_err());

    let revoked = store
        .revoke_memory_capability_transaction(&transaction_id, &document_sha256, issued_at_ms + 2)
        .expect("revoke capability");
    assert_eq!(revoked.state, MemoryCapabilityTransactionState::Revoked);
    assert!(!target.exists());
    let repeated = store
        .revoke_memory_capability_transaction(&transaction_id, &document_sha256, issued_at_ms + 3)
        .expect("repeat revoke");
    assert_eq!(repeated.state, MemoryCapabilityTransactionState::Revoked);
    assert_eq!(repeated.review_digest, review_digest);
    assert!(store
        .resolve_memory_capability_file(&target, &request.route_binding, issued_at_ms + 4)
        .is_err());

    drop(store);
    let _ = std::fs::remove_dir_all(target.parent().expect("capability parent"));
    let _ = std::fs::remove_file(path);
}

#[test]
fn memory_capability_clear_fails_closed_when_the_private_document_was_modified() {
    let path = temp_db();
    let store = ContextStore::open(&path).expect("open store");
    let issued_at_ms = dtoken_types::time::now_ms();
    let request = memory_capability_request(issued_at_ms, issued_at_ms + 60_000);
    let prepared = store
        .prepare_memory_capability_transaction(&request)
        .expect("prepare capability");
    let target = prepared.plan.target_path.clone();
    let transaction_id = prepared.plan.transaction_id.clone();
    let document_sha256 = prepared.plan.document_sha256.clone();
    let review_digest = prepared.plan.review_digest.clone();
    store
        .apply_memory_capability_transaction(prepared, &review_digest)
        .expect("apply capability");

    let mut document: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&target).expect("read capability"))
            .expect("capability JSON");
    document["source_revision"] = serde_json::json!("externally-modified");
    std::fs::write(
        &target,
        serde_json::to_vec_pretty(&document).expect("tampered JSON"),
    )
    .expect("tamper capability");

    assert!(store
        .revoke_memory_capability_transaction(&transaction_id, &document_sha256, issued_at_ms + 1)
        .is_err());
    assert!(target.exists());
    assert!(store
        .resolve_memory_capability_file(&target, &request.route_binding, issued_at_ms + 1)
        .is_err());

    drop(store);
    let _ = std::fs::remove_dir_all(target.parent().expect("capability parent"));
    let _ = std::fs::remove_file(path);
}

#[test]
fn v3_store_migrates_to_v7_without_losing_provider_cache_observations() {
    let path = temp_db();
    {
        let connection = rusqlite::Connection::open(&path).expect("v3 database");
        connection
            .execute_batch(
                "CREATE TABLE schema_migrations (
                   version INTEGER PRIMARY KEY,
                   applied_at_ms INTEGER NOT NULL
                 );
                 INSERT INTO schema_migrations (version, applied_at_ms) VALUES (1, 1), (2, 2), (3, 3);
                 CREATE TABLE provider_cache_observation (
                   route_id TEXT NOT NULL,
                   provider_id TEXT NOT NULL,
                   model TEXT NOT NULL,
                   protocol TEXT NOT NULL,
                   config_revision TEXT NOT NULL,
                   observation_kind TEXT NOT NULL,
                   read_units INTEGER,
                   creation_units INTEGER,
                   creation_5m_units INTEGER,
                   creation_1h_units INTEGER,
                   source_revision TEXT NOT NULL,
                   observed_at_ms INTEGER NOT NULL,
                   PRIMARY KEY (route_id, provider_id, model, protocol, config_revision)
                 );
                 INSERT INTO provider_cache_observation (
                   route_id, provider_id, model, protocol, config_revision, observation_kind,
                   read_units, creation_units, source_revision, observed_at_ms
                 ) VALUES (
                   'route-a', 'provider-a', 'model-a', '\"open_ai\"', 'config-a',
                   '\"open_ai_chat_usage\"', 7, 2, 'cache-revision-a', 42
                 );",
            )
            .expect("create v3 fixture");
    }

    let store = ContextStore::open(&path).expect("migrate v3 store");

    assert_eq!(store.schema_version().unwrap(), 7);
    let count = rusqlite::Connection::open(&path)
        .unwrap()
        .query_row(
            "SELECT COUNT(*) FROM provider_cache_observation WHERE route_id = 'route-a'",
            [],
            |row| row.get::<_, u64>(0),
        )
        .unwrap();
    assert_eq!(count, 1);
    assert!(store
        .table_names()
        .unwrap()
        .contains(&"memory_entry".to_string()));
    assert!(store
        .table_names()
        .unwrap()
        .contains(&"memory_share_grant".to_string()));
    drop(store);
    let _ = std::fs::remove_file(path);
}

#[test]
fn v4_project_wide_share_grants_are_revoked_during_session_scope_migration() {
    let path = temp_db();
    {
        let connection = rusqlite::Connection::open(&path).expect("v4 database");
        connection
            .execute_batch(
                "CREATE TABLE schema_migrations (
                   version INTEGER PRIMARY KEY,
                   applied_at_ms INTEGER NOT NULL
                 );
                 INSERT INTO schema_migrations (version, applied_at_ms) VALUES (4, 4);
                 CREATE TABLE memory_share_grant (
                   grant_id TEXT PRIMARY KEY NOT NULL,
                   project_id TEXT NOT NULL,
                   owner_agent_id TEXT NOT NULL,
                   grantee_agent_id TEXT NOT NULL,
                   grant_revision TEXT NOT NULL,
                   granted_at_ms INTEGER NOT NULL,
                   revoked_at_ms INTEGER,
                   UNIQUE(project_id, owner_agent_id, grantee_agent_id),
                   CHECK (owner_agent_id <> grantee_agent_id)
                 );
                 INSERT INTO memory_share_grant (
                   grant_id, project_id, owner_agent_id, grantee_agent_id,
                   grant_revision, granted_at_ms, revoked_at_ms
                 ) VALUES (
                   'grant-legacy', 'project-a', 'codex', 'claude-code',
                   'grant-r1', 10, NULL
                 );",
            )
            .expect("create v4 grant fixture");
    }

    let store = ContextStore::open(&path).expect("migrate v4 store");
    assert_eq!(store.schema_version().unwrap(), 7);
    let connection = rusqlite::Connection::open(&path).unwrap();
    let (session_id, revoked_at_ms): (String, Option<u64>) = connection
        .query_row(
            "SELECT session_id, revoked_at_ms FROM memory_share_grant WHERE grant_id = 'grant-legacy'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert!(session_id.is_empty());
    assert!(revoked_at_ms.is_some());
    drop(store);
    let _ = std::fs::remove_file(path);
}

#[test]
fn typed_memory_deduplicates_by_scope_and_revision_and_survives_restart() {
    let path = temp_db();
    let first = memory_request("project-a", "codex", Some("session-a"), "memory-r1", 10);
    let mut updated = first.clone();
    updated.summary = "Run crate tests first, then the full workspace gate.".to_string();
    updated.entry_revision = "memory-r2".to_string();
    updated.provenance.observed_at_ms = 20;

    let memory_id = {
        let store = ContextStore::open(&path).expect("open store");
        let stored = store.upsert_memory(&first).expect("insert memory");
        let idempotent = store.upsert_memory(&first).expect("idempotent memory");
        assert_eq!(stored, idempotent);
        let mut conflicting = first.clone();
        conflicting.summary = "Different data with the same revision must fail.".to_string();
        assert!(store.upsert_memory(&conflicting).is_err());
        let replaced = store.upsert_memory(&updated).expect("update memory");
        assert_eq!(replaced.memory_id, stored.memory_id);
        assert_eq!(replaced.entry_revision, "memory-r2");
        assert_eq!(replaced.summary, updated.summary);
        stored.memory_id
    };

    let reopened = ContextStore::open(&path).expect("reopen store");
    let visible = reopened
        .list_visible_memory(&MemoryAccessScope {
            project_id: "project-a".to_string(),
            agent_id: "codex".to_string(),
            session_id: Some("session-a".to_string()),
        })
        .expect("read memory after restart");
    assert_eq!(visible.len(), 1);
    assert_eq!(visible[0].memory_id, memory_id);
    assert_eq!(visible[0].summary, updated.summary);
    drop(reopened);
    let _ = std::fs::remove_file(path);
}

#[test]
fn memory_is_project_agent_and_session_isolated_until_explicit_grant() {
    let path = temp_db();
    let store = ContextStore::open(&path).expect("open store");
    let session_a_memory = store
        .upsert_memory(&memory_request(
            "project-a",
            "codex",
            Some("session-a"),
            "memory-r1",
            10,
        ))
        .unwrap();
    store
        .upsert_memory(&memory_request(
            "project-a",
            "codex",
            Some("session-b"),
            "memory-r1",
            10,
        ))
        .unwrap();

    let claude_same_session = MemoryAccessScope {
        project_id: "project-a".to_string(),
        agent_id: "claude-code".to_string(),
        session_id: Some("session-a".to_string()),
    };
    assert!(store
        .list_visible_memory(&claude_same_session)
        .unwrap()
        .is_empty());
    assert!(store
        .list_visible_memory(&MemoryAccessScope {
            project_id: "project-b".to_string(),
            agent_id: "codex".to_string(),
            session_id: Some("session-a".to_string()),
        })
        .unwrap()
        .is_empty());
    assert!(store
        .list_visible_memory(&MemoryAccessScope {
            project_id: "project-a".to_string(),
            agent_id: "codex".to_string(),
            session_id: Some("session-c".to_string()),
        })
        .unwrap()
        .is_empty());

    let grant_scope = MemoryShareGrantScope {
        project_id: "project-a".to_string(),
        owner_agent_id: "codex".to_string(),
        grantee_agent_id: "claude-code".to_string(),
        session_id: "session-a".to_string(),
    };
    store
        .grant_memory_share_for_scope(&ScopedMemoryShareGrantRequest {
            scope: grant_scope.clone(),
            grant_revision: "grant-r1".to_string(),
            granted_at_ms: 20,
        })
        .expect("grant cross-Agent visibility");

    let shared = store
        .list_visible_memory(&claude_same_session)
        .expect("read shared memory");
    assert_eq!(shared.len(), 1);
    assert_eq!(shared[0].agent_id.as_deref(), Some("codex"));
    assert_eq!(
        store
            .visible_memory_metadata(&session_a_memory.memory_id, &claude_same_session)
            .expect("retrieve authorized metadata")
            .expect("shared metadata"),
        shared[0]
    );
    let claude_session_b = MemoryAccessScope {
        project_id: "project-a".to_string(),
        agent_id: "claude-code".to_string(),
        session_id: Some("session-b".to_string()),
    };
    assert!(store
        .list_visible_memory(&claude_session_b)
        .unwrap()
        .is_empty());
    assert!(store
        .visible_memory_metadata(&session_a_memory.memory_id, &claude_session_b)
        .unwrap()
        .is_none());
    assert!(store
        .export_project_memory(&claude_session_b, false)
        .unwrap()
        .is_empty());

    assert!(store
        .revoke_memory_share_for_scope(&grant_scope, "grant-r1", 30)
        .expect("revoke cross-Agent visibility"));
    assert!(store
        .list_visible_memory(&claude_same_session)
        .unwrap()
        .is_empty());

    drop(store);
    let _ = std::fs::remove_file(path);
}

#[test]
fn legacy_unscoped_share_methods_fail_closed_and_scoped_grants_reject_invalid_scope() {
    let path = temp_db();
    let store = ContextStore::open(&path).expect("open store");
    assert!(store
        .grant_memory_share(&MemoryShareGrantRequest {
            project_id: "project-a".to_string(),
            owner_agent_id: "codex".to_string(),
            grantee_agent_id: "claude-code".to_string(),
            grant_revision: "grant-r1".to_string(),
            granted_at_ms: 10,
        })
        .is_err());
    assert!(store
        .revoke_memory_share("project-a", "codex", "claude-code", "grant-r1", 20)
        .is_err());
    assert!(store
        .grant_memory_share_for_scope(&ScopedMemoryShareGrantRequest {
            scope: MemoryShareGrantScope {
                project_id: "project-a".to_string(),
                owner_agent_id: "codex".to_string(),
                grantee_agent_id: "claude-code".to_string(),
                session_id: " ".to_string(),
            },
            grant_revision: "grant-r1".to_string(),
            granted_at_ms: 10,
        })
        .is_err());
    assert!(store
        .grant_memory_share_for_scope(&ScopedMemoryShareGrantRequest {
            scope: MemoryShareGrantScope {
                project_id: "project-a".to_string(),
                owner_agent_id: "codex".to_string(),
                grantee_agent_id: "codex".to_string(),
                session_id: "session-a".to_string(),
            },
            grant_revision: "grant-r1".to_string(),
            granted_at_ms: 10,
        })
        .is_err());
    drop(store);
    let _ = std::fs::remove_file(path);
}

#[test]
fn strict_memory_boundary_rejects_unknown_raw_fields() {
    let malicious = serde_json::json!({
        "project_id": "project-a",
        "agent_id": "codex",
        "session_id": "session-a",
        "kind": "project-guideline",
        "summary": "Safe summary",
        "keywords": ["safe"],
        "dedup_key": "safe-key",
        "entry_revision": "memory-r1",
        "provenance": {
            "source": "smart-learning",
            "source_revision": "scan-r1",
            "observed_at_ms": 10
        },
        "raw_prompt": "must never enter the store"
    });
    assert!(parse_memory_write_request(&malicious).is_err());

    let nested_malicious = serde_json::json!({
        "project_id": "project-a",
        "agent_id": "codex",
        "kind": "project-guideline",
        "summary": "Safe summary",
        "keywords": [],
        "dedup_key": "safe-key",
        "entry_revision": "memory-r1",
        "provenance": {
            "source": "smart-learning",
            "source_revision": "scan-r1",
            "observed_at_ms": 10,
            "response": "must never enter the store"
        }
    });
    assert!(parse_memory_write_request(&nested_malicious).is_err());
}

#[test]
fn tombstone_delete_is_revision_guarded_and_export_contains_no_summary() {
    let path = temp_db();
    let store = ContextStore::open(&path).expect("open store");
    let scope = MemoryAccessScope {
        project_id: "project-a".to_string(),
        agent_id: "codex".to_string(),
        session_id: Some("session-a".to_string()),
    };
    let stored = store
        .upsert_memory(&memory_request(
            "project-a",
            "codex",
            Some("session-a"),
            "memory-r1",
            10,
        ))
        .unwrap();

    assert!(store
        .tombstone_memory(
            &stored.memory_id,
            &scope,
            "stale-revision",
            "memory-deleted-r1",
            20,
        )
        .is_err());
    let tombstone = store
        .tombstone_memory(
            &stored.memory_id,
            &scope,
            "memory-r1",
            "memory-deleted-r1",
            20,
        )
        .expect("delete memory exactly");
    assert_eq!(tombstone.memory_id, stored.memory_id);
    assert_eq!(tombstone.entry_revision, "memory-deleted-r1");
    assert!(store.list_visible_memory(&scope).unwrap().is_empty());

    let exported = store
        .export_project_memory(&scope, true)
        .expect("export tombstone");
    assert_eq!(exported.len(), 1);
    assert_eq!(exported[0].state, "deleted");
    assert!(exported[0].summary.is_none());
    assert!(exported[0].keywords.is_empty());
    assert!(!serde_json::to_string(&serde_json::json!({
        "memory_id": exported[0].memory_id,
        "summary": exported[0].summary,
        "keywords": exported[0].keywords,
    }))
    .unwrap()
    .contains("Run the focused Rust tests"));

    drop(store);
    let _ = std::fs::remove_file(path);
}

#[test]
fn typed_failure_patterns_deduplicate_expire_and_survive_restart() {
    let path = temp_db();
    let observation = FailurePatternObservation {
        failure_class: FailureClass::ProviderTimeout,
        route_id: "route-a".to_string(),
        provider_id: "provider-a".to_string(),
        model: Some("opencode/deepseek-v4-flash-free".to_string()),
        agent_id: Some("opencode".to_string()),
        project_id: Some("project-a".to_string()),
        observed_at_ms: 10,
        evidence_revision: "request-00000000-0000-0000-0000-000000000001".to_string(),
    };

    let pattern_id = {
        let store = ContextStore::open(&path).expect("open store");
        let first = store
            .record_failure_pattern(&observation, 100)
            .expect("record first failure");
        assert_eq!(first.occurrences, 1);
        assert_eq!(first.first_observed_at_ms, 10);
        assert_eq!(first.last_observed_at_ms, 10);
        assert_eq!(first.expires_at_ms, 110);

        let replay = store
            .record_failure_pattern(&observation, 100)
            .expect("replay is idempotent");
        assert_eq!(replay.occurrences, 1);

        let mut later = observation.clone();
        later.observed_at_ms = 20;
        later.evidence_revision = "request-00000000-0000-0000-0000-000000000002".to_string();
        let updated = store
            .record_failure_pattern(&later, 100)
            .expect("record later failure");
        assert_eq!(updated.pattern_id, first.pattern_id);
        assert_eq!(updated.occurrences, 2);
        assert_eq!(updated.last_observed_at_ms, 20);
        assert_eq!(updated.expires_at_ms, 120);
        updated.pattern_id
    };

    let store = ContextStore::open(&path).expect("reopen store");
    assert_eq!(
        store
            .failure_pattern(&pattern_id)
            .expect("read exact pattern")
            .expect("pattern survives restart")
            .occurrences,
        2
    );
    assert_eq!(
        store
            .list_failure_patterns(Some("project-a"), Some("opencode"), 20, 10)
            .expect("list scoped patterns")
            .len(),
        1
    );
    assert!(store
        .list_failure_patterns(Some("project-b"), Some("opencode"), 20, 10)
        .expect("isolate project scope")
        .is_empty());
    assert!(store
        .list_failure_patterns(Some("project-a"), Some("opencode"), 120, 10)
        .expect("exclude expired patterns")
        .is_empty());
    assert_eq!(
        store
            .prune_failure_patterns(120)
            .expect("prune expired patterns"),
        1
    );
    assert!(store
        .failure_pattern(&pattern_id)
        .expect("read pruned pattern")
        .is_none());

    drop(store);
    let _ = std::fs::remove_file(path);
}

#[test]
fn failure_patterns_reject_raw_paths_secret_shapes_and_unknown_reason_codes() {
    assert!(FailureClass::from_reason_code("raw upstream error text").is_none());
    let path = temp_db();
    let store = ContextStore::open(&path).expect("open store");
    let mut observation = FailurePatternObservation {
        failure_class: FailureClass::ProviderNetwork,
        route_id: "route-a".to_string(),
        provider_id: "provider-a".to_string(),
        model: Some("model-a".to_string()),
        agent_id: Some("codex".to_string()),
        project_id: Some("project-a".to_string()),
        observed_at_ms: 10,
        evidence_revision: "request-safe".to_string(),
    };

    observation.project_id = Some("/Users/example/private-project".to_string());
    assert!(store.record_failure_pattern(&observation, 100).is_err());
    observation.project_id = Some("project-a".to_string());
    observation.provider_id = "sk-secret-shaped-value".to_string();
    assert!(store.record_failure_pattern(&observation, 100).is_err());
    observation.provider_id = "provider-a".to_string();
    observation.evidence_revision = "Authorization:Bearer-token".to_string();
    assert!(store.record_failure_pattern(&observation, 100).is_err());

    drop(store);
    let _ = std::fs::remove_file(path);
}
