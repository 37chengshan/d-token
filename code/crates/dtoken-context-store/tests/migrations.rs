use dtoken_context_store::{
    parse_memory_write_request, CcrMetadataProjection, ContextStore, MemoryAccessScope,
    MemoryProvenance, MemoryShareGrantRequest, MemoryWriteRequest, RequestProjection,
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
    std::env::temp_dir().join(format!("dtoken-context-store-{}.sqlite3", Uuid::new_v4()))
}

#[test]
fn migration_is_idempotent_and_schema_has_no_raw_content_columns() {
    let path = temp_db();
    let store = ContextStore::open(&path).expect("open store");
    assert_eq!(store.schema_version().unwrap(), 4);
    drop(store);
    let store = ContextStore::open(&path).expect("reopen store");
    assert_eq!(store.schema_version().unwrap(), 4);

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
    assert_eq!(store.schema_version().unwrap(), 4);
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

#[test]
fn v3_store_migrates_to_v4_without_losing_provider_cache_observations() {
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

    assert_eq!(store.schema_version().unwrap(), 4);
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
    store
        .upsert_memory(&memory_request(
            "project-a",
            "codex",
            Some("session-a"),
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
            session_id: Some("session-b".to_string()),
        })
        .unwrap()
        .is_empty());

    store
        .grant_memory_share(&MemoryShareGrantRequest {
            project_id: "project-a".to_string(),
            owner_agent_id: "codex".to_string(),
            grantee_agent_id: "claude-code".to_string(),
            grant_revision: "grant-r1".to_string(),
            granted_at_ms: 20,
        })
        .expect("grant cross-Agent visibility");

    let shared = store
        .list_visible_memory(&claude_same_session)
        .expect("read shared memory");
    assert_eq!(shared.len(), 1);
    assert_eq!(shared[0].agent_id.as_deref(), Some("codex"));
    assert!(store
        .list_visible_memory(&MemoryAccessScope {
            project_id: "project-a".to_string(),
            agent_id: "claude-code".to_string(),
            session_id: Some("session-b".to_string()),
        })
        .unwrap()
        .is_empty());

    assert!(store
        .revoke_memory_share("project-a", "codex", "claude-code", "grant-r1", 30,)
        .expect("revoke cross-Agent visibility"));
    assert!(store
        .list_visible_memory(&claude_same_session)
        .unwrap()
        .is_empty());

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
