use dtoken_compress::codec_evidence::{
    candidate_codec_evidence_report, production_codec_evidence, CandidateCodecEvidenceError,
    CodecEvidenceState, PRODUCTION_CODEC_IDS,
};
use dtoken_compress::content_router::{route_content, CodecKind};
use dtoken_compress::{CompressionEngine, ResolvedOptimizationContext};
use dtoken_types::{CompressionPolicy, OpticalEndpoint, Protocol};
use serde_json::{json, Value};

fn context() -> ResolvedOptimizationContext {
    ResolvedOptimizationContext {
        route_id: "codec-evidence-route".to_string(),
        provider_id: "codec-evidence-provider".to_string(),
        model: "codec-evidence-model".to_string(),
        input_protocol: Protocol::Anthropic,
        output_protocol: Protocol::Anthropic,
        endpoint: OpticalEndpoint::AnthropicMessages,
    }
}

fn request_with_historical_result(result: &str) -> Value {
    let mut messages = vec![
        json!({"role":"assistant","content":[{"type":"tool_use","id":"old-call","name":"search","input":{}}]}),
        json!({"role":"user","content":[{"type":"tool_result","tool_use_id":"old-call","content":result}]}),
    ];
    for index in 0..6 {
        messages.push(json!({
            "role":"assistant",
            "content":[{"type":"tool_use","id":format!("recent-{index}"),"name":"read","input":{}}]
        }));
        messages.push(json!({
            "role":"user",
            "content":[{"type":"tool_result","tool_use_id":format!("recent-{index}"),"content":"recent"}]
        }));
    }
    messages.push(json!({"role":"user","content":[{"type":"text","text":"current"}]}));
    json!({
        "model": "codec-evidence-model",
        "system": "system remains exact",
        "messages": messages
    })
}

#[test]
fn production_codec_ids_cover_the_entire_compressor_registry() {
    use dtoken_compress::compressor_registry::production_compressor_registry;

    let registry_ids: Vec<&str> = production_compressor_registry()
        .descriptors()
        .iter()
        .map(|descriptor| descriptor.id)
        .collect();
    // The evidence list is a compile-time projection of the registry: every
    // registry codec must be measured, in registry order, and no codec may
    // be measured that the registry does not declare.
    assert_eq!(registry_ids, PRODUCTION_CODEC_IDS.to_vec());
    assert_eq!(PRODUCTION_CODEC_IDS.len(), 9);
    assert!(PRODUCTION_CODEC_IDS.contains(&"config-codec"));
}

#[test]
fn production_codec_evidence_is_measured_not_declared() {
    let evidence = production_codec_evidence();

    assert!(evidence.all_active(), "{evidence:#?}");
    assert_eq!(
        evidence
            .codecs()
            .iter()
            .map(|codec| codec.id())
            .collect::<Vec<_>>(),
        PRODUCTION_CODEC_IDS
    );
    assert_eq!(evidence.evidence_revision().len(), 64);
    assert!(evidence
        .evidence_revision()
        .bytes()
        .all(|byte| byte.is_ascii_hexdigit()));

    for codec in evidence.codecs() {
        assert_eq!(codec.state(), CodecEvidenceState::Active, "{}", codec.id());
        assert!(codec.corpus_cases() >= 1, "{}", codec.id());
        assert!(codec.property_cases() >= 128, "{}", codec.id());
        assert!(codec.semantic_cases() >= 128, "{}", codec.id());
        assert!(codec.negative_cases() >= 4, "{}", codec.id());
        assert!(codec.budget_or_cancel_cases() >= 1, "{}", codec.id());
        assert!(codec.rollback_cases() >= 128, "{}", codec.id());
        assert_eq!(codec.failures(), 0, "{}", codec.id());
        assert_eq!(codec.corpus_digest().len(), 64, "{}", codec.id());
        assert!(codec
            .corpus_digest()
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit()));
        assert!(codec.latency_p95_us() > 0, "{}", codec.id());
    }
}

#[test]
fn search_codec_preserves_exact_anchor_lines_while_folding_only_result_body_repeats() {
    let source = [
        "src/a.rs:10 result",
        "shared search context",
        "shared search context",
        "src/b.rs:20 other",
    ]
    .join("\n");

    assert_eq!(route_content(&source).codec, CodecKind::SearchFold);

    let result = CompressionEngine::new()
        .compress_resolved(
            &context(),
            CompressionPolicy::Balanced,
            request_with_historical_result(&source),
        )
        .expect("production compression");
    assert!(result
        .transforms_applied
        .iter()
        .any(|transform| transform == "search_fold_v1"));
    let candidate = result
        .body
        .pointer("/messages/1/content/0/content")
        .and_then(Value::as_str)
        .expect("compressed search result");
    assert!(candidate.contains("src/a.rs:10 result"));
    assert!(candidate.contains("src/b.rs:20 other"));
    assert!(candidate.contains("shared search context [repeated x2]"));
}

#[test]
fn candidate_report_is_hash_bound_metadata_only_and_fails_closed() {
    let candidate_id = "a".repeat(64);
    let revision = "b".repeat(40);
    let report = candidate_codec_evidence_report(&candidate_id, &revision, 50_000)
        .expect("valid candidate report");

    assert_eq!(report["candidate"]["candidate_id"], candidate_id);
    assert_eq!(report["candidate"]["git_head"], revision);
    assert_eq!(report["gate"]["ok"], true);
    assert_eq!(report["privacy"]["metadata_only"], true);
    assert_eq!(report["privacy"]["raw_corpus_stored"], false);
    assert_eq!(
        report["codecs"].as_array().map(Vec::len),
        Some(PRODUCTION_CODEC_IDS.len())
    );

    assert_eq!(
        candidate_codec_evidence_report("not-a-candidate", &revision, 50_000),
        Err(CandidateCodecEvidenceError::InvalidCandidateId)
    );
    assert_eq!(
        candidate_codec_evidence_report(&candidate_id, "not-a-revision", 50_000),
        Err(CandidateCodecEvidenceError::InvalidCandidateRevision)
    );
    let rejected =
        candidate_codec_evidence_report(&candidate_id, &revision, 0).expect("valid report");
    assert_eq!(rejected["gate"]["ok"], false);
    assert!(rejected["gate"]["failed_checks"]
        .as_array()
        .is_some_and(|checks| !checks.is_empty()));
}
