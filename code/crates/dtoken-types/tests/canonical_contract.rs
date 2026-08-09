//! Canonical contract tests.
//!
//! Freezes `BoundRequest / IrNode / NodeAction / NodePlan / StageArtifact /
//! OptimizationManifest` plus the global safety policy and the four-dimension
//! accounting buckets. Tests are contract-first: every assertion pins a
//! contract invariant (digest derivation, action exclusivity, fail-closed
//! validation).

use dtoken_types::contract::{
    AccountingBuckets, BoundRequest, ContractError, IrNode, NodeAction, NodePlan,
    OptimizationManifest, SafetyPolicy, StageArtifact,
};
use dtoken_types::{OpticalBucket, Protocol};
use uuid::Uuid;

fn preserve_node(id: &str) -> IrNode {
    IrNode::new(
        id.to_string(),
        "/messages/0".to_string(),
        NodeAction::Preserve {
            reason: "exact".to_string(),
        },
    )
    .expect("valid preserve node")
}

fn ingress_digest_of(ingress: &[u8]) -> String {
    BoundRequest::new(
        Uuid::new_v4(),
        Protocol::OpenAi,
        None,
        None,
        ingress,
        "decision-a".to_string(),
    )
    .expect("valid bound request")
    .ingress_digest
    .clone()
}

#[test]
fn bound_request_digests_are_derived_from_bytes_not_supplied() {
    // MUST: ingress digest is computed from the request bytes, never supplied.
    let request = BoundRequest::new(
        Uuid::new_v4(),
        Protocol::Anthropic,
        Some("project-a".to_string()),
        Some("user-model".to_string()),
        b"hello world",
        "decision-a".to_string(),
    )
    .expect("valid bound request");

    assert_eq!(request.ingress_digest.len(), 64, "sha256 hex");
    assert!(request
        .ingress_digest
        .chars()
        .all(|c| c.is_ascii_hexdigit()));
    assert_eq!(request.decision_digest, "decision-a");
    assert_eq!(request.project_scope.as_deref(), Some("project-a"));
    assert_eq!(request.user_target_model.as_deref(), Some("user-model"));

    // Same bytes, same digest (deterministic).
    assert_eq!(
        ingress_digest_of(b"hello world"),
        ingress_digest_of(b"hello world")
    );
    // Different bytes, different digest.
    assert_ne!(
        ingress_digest_of(b"hello world"),
        ingress_digest_of(b"hello worlD")
    );
    // Empty ingress is rejected: no request without a body.
    assert!(matches!(
        BoundRequest::new(
            Uuid::new_v4(),
            Protocol::OpenAi,
            None,
            None,
            b"",
            "d".to_string()
        ),
        Err(ContractError::EmptyIngress)
    ));
}

#[test]
fn every_node_carries_exactly_one_action() {
    // MUST: single node, exactly one action (Preserve/Reuse/HeadroomSemantic/
    // RtkCommandOutput/Optical) — an enum, so no combination is representable.
    let node = IrNode::new(
        "n1".to_string(),
        "/messages/1".to_string(),
        NodeAction::Reuse {
            cache_identity: "cache-a".to_string(),
        },
    )
    .expect("valid node");
    assert!(matches!(node.action(), NodeAction::Reuse { .. }));

    for action in [
        NodeAction::Preserve {
            reason: "exact".to_string(),
        },
        NodeAction::Reuse {
            cache_identity: "cache-a".to_string(),
        },
        NodeAction::HeadroomSemantic {
            codec_id: "json-codec".to_string(),
        },
        NodeAction::RtkCommandOutput {
            command_id: "cmd-1".to_string(),
        },
        NodeAction::Optical {
            bucket: OpticalBucket::StaticSlab,
        },
    ] {
        let node = IrNode::new("n".to_string(), "/messages/0".to_string(), action.clone())
            .expect("valid node");
        assert_eq!(node.action(), &action, "exactly one action is stored");
        assert!(!node.action_kind().is_empty());
    }

    // Structural invariant: the action enum has exactly the five sealed variants.
    assert!(matches!(
        NodeAction::Preserve {
            reason: "x".to_string()
        }
        .kind(),
        "preserve"
    ));
    assert_eq!(
        NodeAction::HeadroomSemantic {
            codec_id: "json-codec".to_string()
        }
        .kind(),
        "headroom_semantic"
    );
    assert_eq!(
        NodeAction::RtkCommandOutput {
            command_id: "c".to_string()
        }
        .kind(),
        "rtk_command_output"
    );
    assert_eq!(
        NodeAction::Optical {
            bucket: OpticalBucket::Reminder
        }
        .kind(),
        "optical"
    );
    assert_eq!(
        NodeAction::Reuse {
            cache_identity: "c".to_string()
        }
        .kind(),
        "reuse"
    );
}

#[test]
fn node_plan_requires_unique_ids_and_stable_digest() {
    // MUST: plan digest is derived from the plan, changes when the plan changes.
    let plan = NodePlan::new(vec![
        preserve_node("a"),
        IrNode::new(
            "b".to_string(),
            "/messages/1".to_string(),
            NodeAction::HeadroomSemantic {
                codec_id: "log-codec".to_string(),
            },
        )
        .expect("valid node"),
    ])
    .expect("valid plan");

    assert_eq!(plan.plan_digest.len(), 64);

    let same = NodePlan::new(vec![
        preserve_node("a"),
        IrNode::new(
            "b".to_string(),
            "/messages/1".to_string(),
            NodeAction::HeadroomSemantic {
                codec_id: "log-codec".to_string(),
            },
        )
        .expect("valid node"),
    ])
    .expect("valid plan");
    assert_eq!(plan.plan_digest, same.plan_digest, "same plan, same digest");

    // A changed action changes the digest (field changes must change digest).
    let changed = NodePlan::new(vec![
        preserve_node("a"),
        IrNode::new(
            "b".to_string(),
            "/messages/1".to_string(),
            NodeAction::Preserve {
                reason: "keep".to_string(),
            },
        )
        .expect("valid node"),
    ])
    .expect("valid plan");
    assert_ne!(plan.plan_digest, changed.plan_digest);

    // Duplicate node ids are rejected: each node is uniquely addressable.
    assert!(matches!(
        NodePlan::new(vec![preserve_node("a"), preserve_node("a")]),
        Err(ContractError::DuplicateNodeId)
    ));
    // Empty plan rejected.
    assert!(matches!(
        NodePlan::new(vec![]),
        Err(ContractError::EmptyNodePlan)
    ));
}

#[test]
fn stage_artifact_digest_is_derived_from_body_bytes() {
    let artifact = StageArtifact::new(
        "transform".to_string(),
        "source-digest".to_string(),
        b"optimized body",
        vec!["log_fold_v1".to_string()],
    )
    .expect("valid stage artifact");
    assert_eq!(artifact.body_digest.len(), 64);
    assert_eq!(artifact.stage_id, "transform");
    assert_eq!(artifact.transforms, vec!["log_fold_v1".to_string()]);

    // Empty body / empty stage id / empty source digest are rejected.
    assert!(matches!(
        StageArtifact::new("t".to_string(), "s".to_string(), b"", vec![]),
        Err(ContractError::EmptyBody)
    ));
    assert!(matches!(
        StageArtifact::new("".to_string(), "s".to_string(), b"x", vec![]),
        Err(ContractError::EmptyStageId)
    ));
    assert!(matches!(
        StageArtifact::new("t".to_string(), "".to_string(), b"x", vec![]),
        Err(ContractError::EmptySourceDigest)
    ));
}

#[test]
fn manifest_digest_is_self_excluding_and_verifiable() {
    // MUST: manifest digest covers everything except itself; tampering breaks it.
    let manifest = OptimizationManifest::new(
        BoundRequest::new(
            Uuid::new_v4(),
            Protocol::OpenAi,
            None,
            None,
            b"ingress",
            "decision-a".to_string(),
        )
        .expect("bound request"),
        NodePlan::new(vec![preserve_node("a")]).expect("plan"),
        vec![
            StageArtifact::new("transform".to_string(), "s".to_string(), b"body", vec![])
                .expect("artifact"),
        ],
        SafetyPolicy::default(),
        AccountingBuckets::default(),
    )
    .expect("valid manifest");

    assert_eq!(manifest.schema_version, 1);
    assert!(manifest.verify_manifest_digest());

    // Tampering with any covered field invalidates the digest.
    let mut tampered = manifest.clone();
    tampered.stages[0].transforms.push("forged".to_string());
    assert!(!tampered.verify_manifest_digest());

    // The digest field itself is excluded from the digest computation.
    let mut relabeled = manifest.clone();
    relabeled.manifest_digest = "0".repeat(64);
    assert!(!relabeled.verify_manifest_digest());

    // A full JSON round trip preserves the digest: the canonical
    // serialization is deterministic, so a deserialized manifest still
    // re-verifies without re-minting.
    let value = serde_json::to_value(&manifest).expect("serializable");
    let restored: OptimizationManifest = serde_json::from_value(value).expect("deserializable");
    assert_eq!(restored, manifest);
    assert!(restored.verify_manifest_digest());
}

#[test]
fn safety_policy_defaults_to_safe_lossless() {
    // MUST: global safety policy defaults to `safe_lossless`.
    assert_eq!(SafetyPolicy::default(), SafetyPolicy::SafeLossless);
    assert!(SafetyPolicy::SafeLossless.is_lossless());
    assert_eq!(SafetyPolicy::SafeLossless.confirmation_revision(), None);
    assert!(SafetyPolicy::SafeLossless.validate().is_ok());
}

#[test]
fn enhanced_and_lossy_policies_require_confirmation_revision() {
    // MUST: enhanced/lossy policies are only constructible with an explicit,
    // non-empty confirmation revision — no silent escalation (no chooser).
    assert!(SafetyPolicy::enhanced("review-2026-08-09-a").is_ok());
    assert!(SafetyPolicy::lossy("review-2026-08-09-b").is_ok());

    let enhanced = SafetyPolicy::enhanced("review-2026-08-09-a").expect("valid enhanced");
    assert_eq!(
        enhanced.confirmation_revision(),
        Some("review-2026-08-09-a")
    );
    assert!(!enhanced.is_lossless());
    assert!(enhanced.validate().is_ok());

    assert!(matches!(
        SafetyPolicy::enhanced(""),
        Err(ContractError::MissingConfirmationRevision)
    ));
    assert!(matches!(
        SafetyPolicy::lossy("   "),
        Err(ContractError::MissingConfirmationRevision)
    ));
}

#[test]
fn accounting_buckets_keep_unknown_none_and_never_sum_naively() {
    // MUST: compression/command-output/provider-cache/relay are four
    // independent buckets; unknown stays None (never 0); no direct summation.
    let buckets = AccountingBuckets::default();
    assert_eq!(buckets.compression, None);
    assert_eq!(buckets.command_output, None);
    assert_eq!(buckets.provider_cache, None);
    assert_eq!(buckets.relay, None);
    assert_eq!(
        buckets.total_if_all_known(),
        None,
        "unknown buckets project to None"
    );

    let partial = AccountingBuckets {
        compression: Some(120),
        ..AccountingBuckets::default()
    };
    assert_eq!(
        partial.total_if_all_known(),
        None,
        "partially known stays None"
    );

    let full = AccountingBuckets {
        compression: Some(120),
        command_output: Some(40),
        provider_cache: Some(10),
        relay: Some(5),
    };
    assert_eq!(full.total_if_all_known(), Some(175));

    // Serialized form keeps unknown buckets as explicit null, never 0.
    let value = serde_json::to_value(AccountingBuckets::default()).unwrap();
    assert!(value["compression"].is_null());
    assert!(value["relay"].is_null());
    assert_eq!(value["compression"].as_i64(), None);

    // Round trip preserves None.
    let back: AccountingBuckets = serde_json::from_value(value).unwrap();
    assert_eq!(back.compression, None);
}
