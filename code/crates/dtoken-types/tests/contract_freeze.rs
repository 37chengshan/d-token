// CF-1 contract-freeze-types direct behavior tests.
//
// Scope: only the dtoken-types shared contract surface. These tests pin:
//  1. serialization round-trip for every frozen type
//  2. closed enum state transitions (ConfigTransactionState)
//  3. absence of sensitive fields (token/cookie/Authorization/auth config)
//  4. per-Agent independent transaction semantics (no cross-Agent atomics)

use dtoken_types::agent::HostPlatform;
use dtoken_types::{
    AgentCapability, AgentConfigOperation, AgentConfigPlan, AgentConfigReceipt,
    AgentConfigReceiptState, AgentTransactionEvidence, CacheReuseReceipt, CapabilityAspect,
    CapabilityTier, ConfigTransactionState, ContextIrNode, NodeAction, OptimizationReceipt,
    RecoveryReceipt, RecoveryStatus, RelayHitReceipt, SendCommitReceipt, UpstreamSignature,
    ValidationReceipt, ValidationStatus, WritePolicy,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

fn sha256_hex(c: char) -> String {
    c.to_string().repeat(64)
}

fn assert_round_trip<T>(value: &T)
where
    T: Serialize + for<'de> Deserialize<'de> + PartialEq + std::fmt::Debug,
{
    let encoded = serde_json::to_string(value).expect("serialize");
    let decoded: T = serde_json::from_str(&encoded).expect("deserialize");
    assert_eq!(&decoded, value, "round-trip must preserve the value");
}

/// Approved token usage/accounting fields (design §7.2 / CF-1): these count
/// local/provider tokens and are never credentials.
const APPROVED_TOKEN_ACCOUNTING_FIELDS: &[&str] = &[
    "provider_input_tokens",
    "provider_output_tokens",
    "local_original_tokens",
    "local_optimized_tokens",
    "estimated_local_tokens_avoided",
    "claimable_tokens_avoided",
];

/// A JSON key is credential-bearing if its name matches a credential shape.
/// Matching is structural (on field names), so legitimate values like the
/// project name "dtoken" or token accounting counters never trip the check.
fn is_forbidden_json_key(key: &str) -> bool {
    if APPROVED_TOKEN_ACCOUNTING_FIELDS.contains(&key) {
        return false;
    }
    let lower = key.to_ascii_lowercase();
    let normalized: String = lower
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .collect();
    lower == "token"
        || lower.ends_with("_token")
        || lower.starts_with("token_")
        || lower == "auth"
        || lower == "credentials"
        || lower == "credential"
        || normalized == "authconfig"
        || normalized == "fullauthconfig"
        || lower.contains("api_key")
        || lower.contains("apikey")
        || lower.contains("authorization")
        || lower.contains("auth_token")
        || lower.contains("cookie")
        || lower.contains("secret")
        || lower.contains("password")
}

fn walk_json_keys(value: &serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            for (key, child) in map {
                assert!(
                    !is_forbidden_json_key(key),
                    "serialized contract must not contain credential-bearing key {key:?}"
                );
                walk_json_keys(child);
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                walk_json_keys(item);
            }
        }
        _ => {}
    }
}

/// Asserts the serialized contract carries no credential-bearing JSON key.
/// Checks field names structurally instead of substring-scanning the encoded
/// text: a substring like `token"` would falsely match the project name
/// `dtoken` in values and cannot distinguish token accounting counters.
fn assert_no_sensitive_material(encoded: &str) {
    let value: serde_json::Value =
        serde_json::from_str(encoded).expect("serialized contract must parse as JSON");
    walk_json_keys(&value);
}

// ---------------------------------------------------------------------------
// 1. Serialization round-trips
// ---------------------------------------------------------------------------

#[test]
fn upstream_signature_round_trip_keeps_only_identity_fields() {
    let signature = UpstreamSignature {
        provider_id: "openai".to_string(),
        model: "gpt-5".to_string(),
        protocol: "responses".to_string(),
        normalized_endpoint: "https://api.example.test/v1".to_string(),
        profile_scope: Some("profile:a".to_string()),
    };
    assert_round_trip(&signature);
    let encoded = serde_json::to_string(&signature).expect("serialize");
    assert_no_sensitive_material(&encoded);
    // No credential-bearing key may ever exist. serde_json Maps may iterate
    // keys in sorted order (BTreeMap), so compare order-independently.
    let value: serde_json::Value = serde_json::from_str(&encoded).expect("parse");
    let mut keys: Vec<&str> = value
        .as_object()
        .expect("object")
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    let mut expected = [
        "provider_id",
        "model",
        "protocol",
        "normalized_endpoint",
        "profile_scope",
    ];
    expected.sort_unstable();
    assert_eq!(keys, expected);
}

#[test]
fn agent_config_snapshot_plan_receipt_round_trip() {
    let snapshot = dtoken_types::AgentConfigSnapshot {
        agent_id: "codex".to_string(),
        config_targets: vec!["~/.codex/config.toml".to_string()],
        current_upstream: Some(UpstreamSignature {
            provider_id: "openai".to_string(),
            model: "gpt-5".to_string(),
            protocol: "responses".to_string(),
            normalized_endpoint: "https://api.example.test/v1".to_string(),
            profile_scope: None,
        }),
        config_fingerprint: sha256_hex('a'),
        credential_state: dtoken_types::AgentCredentialState::ReusableReference,
        source_revision: "rev-1".to_string(),
    };
    assert_round_trip(&snapshot);

    let plan = AgentConfigPlan {
        operation: AgentConfigOperation::Configure,
        snapshot: snapshot.clone(),
        targets: vec![dtoken_types::AgentConfigTarget {
            display_path: "~/.codex/config.toml".to_string(),
            before_sha256: sha256_hex('b'),
            after_sha256: sha256_hex('c'),
            redacted_diff: "@@ -1 +1 @@\n-model_provider = \"openai\"".to_string(),
        }],
        backup_or_rollback_id: Some("backup-1".to_string()),
        runtime_reload_required: true,
    };
    assert_round_trip(&plan);

    let receipt = AgentConfigReceipt {
        operation: AgentConfigOperation::Configure,
        agent_id: "codex".to_string(),
        state: AgentConfigReceiptState::Verified,
        transaction_id: Some("tx-1".to_string()),
        rollback_ids: Vec::new(),
        verified_fingerprint: Some(sha256_hex('d')),
        runtime_reload_required: true,
        source_revision: "rev-1".to_string(),
        next_action: None,
        summary: "configured d-token-owned fields".to_string(),
    };
    assert_round_trip(&receipt);
}

#[test]
fn agent_transaction_evidence_round_trip() {
    let evidence = AgentTransactionEvidence {
        transaction_id: "tx-codex-1".to_string(),
        source_revision: "rev-1".to_string(),
        before_digest: sha256_hex('b'),
        redacted_diff_digest: sha256_hex('r'),
        backup_manifest: Some("backup-1".to_string()),
        planned_after_digest: sha256_hex('p'),
        readback_digest: Some(sha256_hex('q')),
        capability_evidence: Some("cap-1".to_string()),
        receipt_id: Some("receipt-1".to_string()),
        rollback_receipt_id: None,
    };
    assert_round_trip(&evidence);
    assert_no_sensitive_material(&serde_json::to_string(&evidence).expect("serialize"));
}

#[test]
fn optimization_receipt_round_trip_separates_buckets() {
    let receipt = OptimizationReceipt {
        schema_version: 1,
        request_id: Uuid::new_v4(),
        timestamp_ms: 42,
        source_agent: Some("codex".to_string()),
        project_scope: Some("proj-a".to_string()),
        user_target_provider: Some("openai".to_string()),
        user_target_model: Some("gpt-5".to_string()),
        actual_provider: Some("openai".to_string()),
        actual_model: Some("gpt-5".to_string()),
        provider_or_model_changed: false,
        transformations: vec!["frozen-prefix".to_string()],
        protected_content: "system_message,user_message".to_string(),
        local_original_tokens: Some(10_000),
        local_optimized_tokens: Some(9_000),
        provider_input_tokens: Some(9_100),
        provider_output_tokens: Some(800),
        estimated_local_tokens_avoided: Some(1_000),
        claimable_tokens_avoided: Some(900),
        recovery_status: Some(RecoveryStatus::PendingRetained),
        recovery_pointer: Some("recovery://ccr/1".to_string()),
        validation_status: ValidationStatus::Passed,
        skip_or_failure_reason: None,
    };
    assert_round_trip(&receipt);
    let encoded = serde_json::to_string(&receipt).expect("serialize");
    assert_no_sensitive_material(&encoded);
    // Local/Provider/claimable must stay separate in the wire format.
    assert!(encoded.contains("local_original_tokens"));
    assert!(encoded.contains("provider_input_tokens"));
    assert!(encoded.contains("claimable_tokens_avoided"));
}

#[test]
fn provider_usage_missing_stays_unavailable_not_zero() {
    let receipt = OptimizationReceipt {
        schema_version: 1,
        request_id: Uuid::new_v4(),
        timestamp_ms: 0,
        source_agent: None,
        project_scope: None,
        user_target_provider: None,
        user_target_model: None,
        actual_provider: None,
        actual_model: None,
        provider_or_model_changed: false,
        transformations: Vec::new(),
        protected_content: String::new(),
        local_original_tokens: None,
        local_optimized_tokens: None,
        provider_input_tokens: None,
        provider_output_tokens: None,
        estimated_local_tokens_avoided: None,
        claimable_tokens_avoided: None,
        recovery_status: None,
        recovery_pointer: None,
        validation_status: ValidationStatus::Unavailable,
        skip_or_failure_reason: Some("no_provider_usage".to_string()),
    };
    let encoded = serde_json::to_string(&receipt).expect("serialize");
    assert!(encoded.contains("\"provider_input_tokens\":null"));
    assert!(encoded.contains("\"validation_status\":\"unavailable\""));
}

#[test]
fn validation_recovery_send_commit_receipts_round_trip() {
    let validation = ValidationReceipt {
        schema_version: 1,
        artifact_sha256: sha256_hex('a'),
        status: ValidationStatus::Passed,
        reason_codes: Vec::new(),
        producer_revision: "producer-1".to_string(),
        validated_at_ms: 7,
    };
    assert_round_trip(&validation);

    let recovery = RecoveryReceipt {
        schema_version: 1,
        request_id: Uuid::new_v4(),
        source_digest: sha256_hex('s'),
        decision_digest: sha256_hex('d'),
        status: RecoveryStatus::Terminal,
        recovery_pointer: Some("recovery://ccr/2".to_string()),
        recorded_at_ms: 8,
    };
    assert_round_trip(&recovery);

    let commit = SendCommitReceipt {
        schema_version: 1,
        request_id: Uuid::new_v4(),
        sent_at_ms: 9,
        transport: "http".to_string(),
        sent_payload_sha256: sha256_hex('p'),
        upstream_state: dtoken_types::UpstreamSendState::SendCommitted,
        failure_reason: None,
    };
    assert_round_trip(&commit);
}

#[test]
fn relay_and_cache_receipts_are_independent_buckets() {
    let relay = RelayHitReceipt {
        schema_version: 1,
        request_id: Uuid::new_v4(),
        relay_id: "relay-1".to_string(),
        avoided_transfer_bytes: 4096,
        hit_at_ms: 10,
    };
    let cache = CacheReuseReceipt {
        schema_version: 1,
        request_id: Uuid::new_v4(),
        cache_key_fingerprint: sha256_hex('c'),
        reused_bytes: 2048,
        hit_at_ms: 11,
    };
    assert_round_trip(&relay);
    assert_round_trip(&cache);

    // Neither bucket may ever expose a compression-savings field.
    for encoded in [
        serde_json::to_string(&relay).expect("serialize"),
        serde_json::to_string(&cache).expect("serialize"),
    ] {
        assert!(!encoded.contains("compression"));
        assert!(!encoded.contains("savings"));
        assert_no_sensitive_material(&encoded);
    }
}

#[test]
fn agent_capability_round_trip() {
    let aspect = CapabilityAspect {
        official_source: "official-docs".to_string(),
        version: "1.0".to_string(),
        config_path: Some("~/.codex/config.toml".to_string()),
        field_ownership: "dtoken-owned".to_string(),
        write_policy: WritePolicy::UserSelectedManaged,
        auth_owner: "user".to_string(),
        watcher: "file-watcher".to_string(),
        failure_recovery_acceptance: "verified".to_string(),
        ui_status: "connected".to_string(),
    };
    let capability = AgentCapability {
        agent_id: "codex".to_string(),
        display_name: "Codex".to_string(),
        official_capability: CapabilityTier::Route,
        discovery: aspect.clone(),
        configuration: aspect.clone(),
        authentication: aspect.clone(),
        change_detection: aspect.clone(),
        verification: aspect.clone(),
        rollback: aspect.clone(),
        platforms: vec![HostPlatform::Macos, HostPlatform::Windows],
        provenance: "registry-v1".to_string(),
    };
    assert_round_trip(&capability);
    let encoded = serde_json::to_string(&capability).expect("serialize");
    assert_no_sensitive_material(&encoded);
}

#[test]
fn context_ir_node_skeleton_round_trip() {
    for action in [
        NodeAction::Preserve,
        NodeAction::Reuse,
        NodeAction::HeadroomSemantic,
        NodeAction::RtkCommandOutput,
        NodeAction::Optical,
    ] {
        let node = ContextIrNode {
            node_id: "node-1".to_string(),
            action,
        };
        assert_round_trip(&node);
    }
}

// ---------------------------------------------------------------------------
// 2. Closed enum transitions
// ---------------------------------------------------------------------------

#[test]
fn config_transaction_state_forward_chain_is_closed() {
    let chain = [
        ConfigTransactionState::Discovered,
        ConfigTransactionState::Selected,
        ConfigTransactionState::Planned,
        ConfigTransactionState::BackedUp,
        ConfigTransactionState::Applied,
        ConfigTransactionState::ReadbackVerified,
        ConfigTransactionState::CapabilityVerified,
        ConfigTransactionState::Completed,
    ];
    for pair in chain.windows(2) {
        assert!(
            pair[0].can_transition_to(pair[1]),
            "{pair:?} must be a valid forward transition"
        );
    }
}

#[test]
fn config_transaction_state_terminal_states_absorb() {
    for terminal in [
        ConfigTransactionState::Completed,
        ConfigTransactionState::RolledBack,
        ConfigTransactionState::Failed,
    ] {
        assert!(terminal.is_terminal());
        for other in [
            ConfigTransactionState::Discovered,
            ConfigTransactionState::Selected,
            ConfigTransactionState::Planned,
            ConfigTransactionState::BackedUp,
            ConfigTransactionState::Applied,
            ConfigTransactionState::ReadbackVerified,
            ConfigTransactionState::CapabilityVerified,
            ConfigTransactionState::Completed,
            ConfigTransactionState::RolledBack,
            ConfigTransactionState::Failed,
        ] {
            assert!(
                !terminal.can_transition_to(other),
                "terminal {terminal:?} must not transition to {other:?}"
            );
        }
    }
}

#[test]
fn config_transaction_state_rejects_illegal_edges() {
    // No skipping of the forward chain.
    assert!(!ConfigTransactionState::Discovered.can_transition_to(ConfigTransactionState::Planned));
    assert!(!ConfigTransactionState::Selected.can_transition_to(ConfigTransactionState::BackedUp));
    assert!(!ConfigTransactionState::Planned.can_transition_to(ConfigTransactionState::Applied));
    // Rollback requires an existing backup.
    assert!(
        !ConfigTransactionState::Discovered.can_transition_to(ConfigTransactionState::RolledBack)
    );
    assert!(!ConfigTransactionState::Selected.can_transition_to(ConfigTransactionState::RolledBack));
    assert!(!ConfigTransactionState::Planned.can_transition_to(ConfigTransactionState::RolledBack));
    // Rollback is allowed once a backup exists.
    assert!(ConfigTransactionState::BackedUp.can_transition_to(ConfigTransactionState::RolledBack));
    assert!(ConfigTransactionState::Applied.can_transition_to(ConfigTransactionState::RolledBack));
    // Failure is reachable from any non-terminal state.
    for state in [
        ConfigTransactionState::Discovered,
        ConfigTransactionState::Selected,
        ConfigTransactionState::Planned,
        ConfigTransactionState::BackedUp,
        ConfigTransactionState::Applied,
        ConfigTransactionState::ReadbackVerified,
        ConfigTransactionState::CapabilityVerified,
    ] {
        assert!(state.can_transition_to(ConfigTransactionState::Failed));
    }
}

#[test]
fn write_policy_is_closed_three_way() {
    let policies = [
        WritePolicy::AlwaysReadOnly,
        WritePolicy::UserSelectedManaged,
        WritePolicy::Unsupported,
    ];
    for policy in policies {
        assert_round_trip(&policy);
    }
    let unknown = r#"{"write_policy":"silent_fallback"}"#;
    assert!(
        serde_json::from_str::<WritePolicy>(unknown).is_err(),
        "unapproved write policy must be rejected"
    );
}

// ---------------------------------------------------------------------------
// 3. Sensitive-field absence across the whole frozen surface
// ---------------------------------------------------------------------------

#[test]
fn no_frozen_type_carries_credential_fields() {
    let snapshot = dtoken_types::AgentConfigSnapshot {
        agent_id: "codex".into(),
        config_targets: vec!["~/.codex/config.toml".into()],
        current_upstream: None,
        config_fingerprint: sha256_hex('f'),
        credential_state: dtoken_types::AgentCredentialState::ReusableReference,
        source_revision: "rev-1".into(),
    };
    let aspect = CapabilityAspect {
        official_source: "s".into(),
        version: "1".into(),
        config_path: None,
        field_ownership: "dtoken".into(),
        write_policy: WritePolicy::AlwaysReadOnly,
        auth_owner: "user".into(),
        watcher: "watcher".into(),
        failure_recovery_acceptance: "accept".into(),
        ui_status: "detected".into(),
    };
    let docs: Vec<String> = vec![
        serde_json::to_string(&UpstreamSignature {
            provider_id: "p".into(),
            model: "m".into(),
            protocol: "proto".into(),
            normalized_endpoint: "https://e/v1".into(),
            profile_scope: None,
        })
        .expect("serialize"),
        serde_json::to_string(&snapshot).expect("serialize"),
        serde_json::to_string(&AgentConfigPlan {
            operation: AgentConfigOperation::Configure,
            snapshot: snapshot.clone(),
            targets: vec![dtoken_types::AgentConfigTarget {
                display_path: "~/.codex/config.toml".into(),
                before_sha256: sha256_hex('b'),
                after_sha256: sha256_hex('a'),
                redacted_diff: "@@ redacted @@".into(),
            }],
            backup_or_rollback_id: Some("backup-1".into()),
            runtime_reload_required: false,
        })
        .expect("serialize"),
        serde_json::to_string(&AgentConfigReceipt {
            operation: AgentConfigOperation::Configure,
            agent_id: "codex".into(),
            state: AgentConfigReceiptState::Verified,
            transaction_id: Some("tx-1".into()),
            rollback_ids: Vec::new(),
            verified_fingerprint: Some(sha256_hex('v')),
            runtime_reload_required: false,
            source_revision: "rev-1".into(),
            next_action: None,
            summary: "verified".into(),
        })
        .expect("serialize"),
        serde_json::to_string(&AgentTransactionEvidence {
            transaction_id: "tx-1".into(),
            source_revision: "rev-1".into(),
            before_digest: sha256_hex('b'),
            redacted_diff_digest: sha256_hex('r'),
            backup_manifest: Some("backup-1".into()),
            planned_after_digest: sha256_hex('p'),
            readback_digest: Some(sha256_hex('q')),
            capability_evidence: Some("cap-1".into()),
            receipt_id: Some("receipt-1".into()),
            rollback_receipt_id: None,
        })
        .expect("serialize"),
        serde_json::to_string(&OptimizationReceipt {
            schema_version: 1,
            request_id: Uuid::new_v4(),
            timestamp_ms: 0,
            source_agent: None,
            project_scope: None,
            user_target_provider: None,
            user_target_model: None,
            actual_provider: None,
            actual_model: None,
            provider_or_model_changed: false,
            transformations: Vec::new(),
            protected_content: String::new(),
            local_original_tokens: None,
            local_optimized_tokens: None,
            provider_input_tokens: None,
            provider_output_tokens: None,
            estimated_local_tokens_avoided: None,
            claimable_tokens_avoided: None,
            recovery_status: None,
            recovery_pointer: None,
            validation_status: ValidationStatus::Unavailable,
            skip_or_failure_reason: None,
        })
        .expect("serialize"),
        serde_json::to_string(&ValidationReceipt {
            schema_version: 1,
            artifact_sha256: sha256_hex('a'),
            status: ValidationStatus::Passed,
            reason_codes: Vec::new(),
            producer_revision: "producer-1".into(),
            validated_at_ms: 1,
        })
        .expect("serialize"),
        serde_json::to_string(&RecoveryReceipt {
            schema_version: 1,
            request_id: Uuid::new_v4(),
            source_digest: sha256_hex('s'),
            decision_digest: sha256_hex('d'),
            status: RecoveryStatus::Terminal,
            recovery_pointer: None,
            recorded_at_ms: 1,
        })
        .expect("serialize"),
        serde_json::to_string(&SendCommitReceipt {
            schema_version: 1,
            request_id: Uuid::new_v4(),
            sent_at_ms: 1,
            transport: "http".into(),
            sent_payload_sha256: sha256_hex('p'),
            upstream_state: dtoken_types::UpstreamSendState::SendCommitted,
            failure_reason: None,
        })
        .expect("serialize"),
        serde_json::to_string(&RelayHitReceipt {
            schema_version: 1,
            request_id: Uuid::new_v4(),
            relay_id: "relay-1".into(),
            avoided_transfer_bytes: 1,
            hit_at_ms: 1,
        })
        .expect("serialize"),
        serde_json::to_string(&CacheReuseReceipt {
            schema_version: 1,
            request_id: Uuid::new_v4(),
            cache_key_fingerprint: sha256_hex('c'),
            reused_bytes: 1,
            hit_at_ms: 1,
        })
        .expect("serialize"),
        serde_json::to_string(&AgentCapability {
            agent_id: "codex".into(),
            display_name: "Codex".into(),
            official_capability: CapabilityTier::Route,
            discovery: aspect.clone(),
            configuration: aspect.clone(),
            authentication: aspect.clone(),
            change_detection: aspect.clone(),
            verification: aspect.clone(),
            rollback: aspect,
            platforms: vec![HostPlatform::Macos],
            provenance: "registry-v1".into(),
        })
        .expect("serialize"),
        serde_json::to_string(&ContextIrNode {
            node_id: "node-1".into(),
            action: NodeAction::Preserve,
        })
        .expect("serialize"),
    ];
    for doc in &docs {
        assert_no_sensitive_material(doc);
    }
}

// ---------------------------------------------------------------------------
// 4. Per-Agent independent semantics
// ---------------------------------------------------------------------------

#[test]
fn per_agent_transactions_advance_independently() {
    // Agent A completes; Agent B rolls back. The evidence cross-sections are
    // per-Agent records: no shared atomic transaction state exists.
    let agent_a = AgentTransactionEvidence {
        transaction_id: "tx-a".to_string(),
        source_revision: "rev-a".to_string(),
        before_digest: sha256_hex('a'),
        redacted_diff_digest: sha256_hex('r'),
        backup_manifest: Some("backup-a".to_string()),
        planned_after_digest: sha256_hex('p'),
        readback_digest: Some(sha256_hex('q')),
        capability_evidence: Some("cap-a".to_string()),
        receipt_id: Some("receipt-a".to_string()),
        rollback_receipt_id: None,
    };
    let agent_b = AgentTransactionEvidence {
        transaction_id: "tx-b".to_string(),
        source_revision: "rev-b".to_string(),
        before_digest: sha256_hex('b'),
        redacted_diff_digest: sha256_hex('s'),
        backup_manifest: Some("backup-b".to_string()),
        planned_after_digest: sha256_hex('t'),
        readback_digest: None,
        capability_evidence: None,
        receipt_id: Some("receipt-b".to_string()),
        rollback_receipt_id: Some("rollback-b".to_string()),
    };

    // A's success must not be affected by B's rollback fields.
    assert_eq!(agent_a.rollback_receipt_id, None);
    assert_eq!(agent_b.rollback_receipt_id.as_deref(), Some("rollback-b"));
    assert_ne!(agent_a.transaction_id, agent_b.transaction_id);
    assert_round_trip(&agent_a);
    assert_round_trip(&agent_b);
}

#[test]
fn config_state_is_per_agent_not_a_shared_transaction() {
    // A's terminal state does not change what B may do next.
    let mut agent_a = ConfigTransactionState::Discovered;
    let mut agent_b = ConfigTransactionState::Discovered;

    // A advances through the whole chain to Completed.
    let chain_a = [
        ConfigTransactionState::Selected,
        ConfigTransactionState::Planned,
        ConfigTransactionState::BackedUp,
        ConfigTransactionState::Applied,
        ConfigTransactionState::ReadbackVerified,
        ConfigTransactionState::CapabilityVerified,
        ConfigTransactionState::Completed,
    ];
    for next in chain_a {
        assert!(agent_a.can_transition_to(next));
        agent_a = next;
    }

    // B independently fails at the same time.
    assert!(agent_b.can_transition_to(ConfigTransactionState::Failed));
    agent_b = ConfigTransactionState::Failed;

    // A stays Completed regardless of B's failure (independent progression).
    assert_eq!(agent_a, ConfigTransactionState::Completed);
    assert_eq!(agent_b, ConfigTransactionState::Failed);
    assert!(agent_a.is_terminal());
    assert!(agent_b.is_terminal());
}

#[test]
fn per_agent_receipts_keep_agent_bound_state() {
    let receipt_a = AgentConfigReceipt {
        operation: AgentConfigOperation::Configure,
        agent_id: "codex".to_string(),
        state: AgentConfigReceiptState::Verified,
        transaction_id: Some("tx-a".to_string()),
        rollback_ids: Vec::new(),
        verified_fingerprint: Some(sha256_hex('v')),
        runtime_reload_required: false,
        source_revision: "rev-a".to_string(),
        next_action: None,
        summary: "verified".to_string(),
    };
    let receipt_b = AgentConfigReceipt {
        operation: AgentConfigOperation::Clear,
        agent_id: "cline".to_string(),
        state: AgentConfigReceiptState::RolledBack,
        transaction_id: Some("tx-b".to_string()),
        rollback_ids: vec!["rollback-b".to_string()],
        verified_fingerprint: None,
        runtime_reload_required: true,
        source_revision: "rev-b".to_string(),
        next_action: Some("retry".to_string()),
        summary: "rolled back".to_string(),
    };
    assert_ne!(receipt_a.agent_id, receipt_b.agent_id);
    assert_ne!(receipt_a.state, receipt_b.state);
    assert_round_trip(&receipt_a);
    assert_round_trip(&receipt_b);
}
