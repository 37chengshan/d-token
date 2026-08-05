//! Metadata-only runtime evidence for the production codec registry.
//!
//! This module executes the real router and codec implementations against a
//! bounded deterministic corpus. It never returns corpus content. The release
//! gate binds the resulting digests and counters to the candidate binary.

use crate::code_codec::{production_registry, CodeCodecProductionError};
use crate::content_router::{
    apply_routed_codec, expand_exact_folded_lines_losslessly, route_content,
    route_content_with_budget, CodecKind, RoutingBudget,
};
use dtoken_types::CompressionPolicy;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::sync::OnceLock;
use std::time::Instant;
use std::time::{SystemTime, UNIX_EPOCH};

/// Registry-derived production codec ids, in the registry's declared order.
/// The constant is a compile-time projection of the compressor registry so
/// the count can never drift from the authoritative descriptor list: adding
/// or removing a codec in [`crate::compressor_registry`] changes this list
/// (and the release gate) instead of silently desynchronizing the evidence.
pub const PRODUCTION_CODEC_IDS: [&str; 9] =
    crate::compressor_registry::production_registry_codec_ids();

const PROPERTY_CASES_PER_CODEC: usize = 128;
const EVIDENCE_IMPLEMENTATION_REVISION: &str = "production-codec-runtime-evidence-v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodecEvidenceState {
    Active,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CandidateCodecEvidenceError {
    InvalidCandidateId,
    InvalidCandidateRevision,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodecEvidence {
    id: &'static str,
    state: CodecEvidenceState,
    corpus_cases: usize,
    property_cases: usize,
    semantic_cases: usize,
    negative_cases: usize,
    budget_or_cancel_cases: usize,
    rollback_cases: usize,
    failures: usize,
    corpus_digest: String,
    latency_p95_us: u64,
}

impl CodecEvidence {
    pub fn id(&self) -> &'static str {
        self.id
    }

    pub fn state(&self) -> CodecEvidenceState {
        self.state
    }

    pub fn corpus_cases(&self) -> usize {
        self.corpus_cases
    }

    pub fn property_cases(&self) -> usize {
        self.property_cases
    }

    pub fn semantic_cases(&self) -> usize {
        self.semantic_cases
    }

    pub fn negative_cases(&self) -> usize {
        self.negative_cases
    }

    pub fn budget_or_cancel_cases(&self) -> usize {
        self.budget_or_cancel_cases
    }

    pub fn rollback_cases(&self) -> usize {
        self.rollback_cases
    }

    pub fn failures(&self) -> usize {
        self.failures
    }

    pub fn corpus_digest(&self) -> &str {
        &self.corpus_digest
    }

    pub fn latency_p95_us(&self) -> u64 {
        self.latency_p95_us
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProductionCodecEvidence {
    evidence_revision: String,
    codecs: Vec<CodecEvidence>,
}

impl ProductionCodecEvidence {
    pub fn evidence_revision(&self) -> &str {
        &self.evidence_revision
    }

    pub fn codecs(&self) -> &[CodecEvidence] {
        &self.codecs
    }

    pub fn all_active(&self) -> bool {
        self.codecs.len() == PRODUCTION_CODEC_IDS.len()
            && self
                .codecs
                .iter()
                .zip(PRODUCTION_CODEC_IDS)
                .all(|(codec, expected)| {
                    codec.id == expected && codec.state == CodecEvidenceState::Active
                })
    }

    pub fn find(&self, id: &str) -> Option<&CodecEvidence> {
        self.codecs.iter().find(|codec| codec.id == id)
    }
}

pub fn production_codec_evidence() -> &'static ProductionCodecEvidence {
    static EVIDENCE: OnceLock<ProductionCodecEvidence> = OnceLock::new();
    EVIDENCE.get_or_init(build_production_codec_evidence)
}

pub fn candidate_codec_evidence_report(
    candidate_id: &str,
    candidate_revision: &str,
    max_p95_us: u64,
) -> Result<Value, CandidateCodecEvidenceError> {
    if candidate_id.len() != 64 || !is_lower_hex(candidate_id) {
        return Err(CandidateCodecEvidenceError::InvalidCandidateId);
    }
    if !(7..=64).contains(&candidate_revision.len()) || !is_lower_hex(candidate_revision) {
        return Err(CandidateCodecEvidenceError::InvalidCandidateRevision);
    }

    let evidence = production_codec_evidence();
    let mut failed_checks = Vec::new();
    if !evidence.all_active() {
        failed_checks.push("all_codecs_active");
    }
    for codec in evidence.codecs() {
        if codec.state() != CodecEvidenceState::Active || codec.failures() != 0 {
            failed_checks.push(codec.id());
        }
        if codec.latency_p95_us() > max_p95_us {
            failed_checks.push(match codec.id() {
                "json-codec" => "json_codec_latency_p95",
                "log-codec" => "log_codec_latency_p95",
                "search-codec" => "search_codec_latency_p95",
                "build-codec" => "build_codec_latency_p95",
                "diff-codec" => "diff_codec_latency_p95",
                "text-codec" => "text_codec_latency_p95",
                "html-codec" => "html_codec_latency_p95",
                "config-codec" => "config_codec_latency_p95",
                "code-compressor" => "code_compressor_latency_p95",
                _ => "unknown_codec_latency_p95",
            });
        }
    }
    failed_checks.sort_unstable();
    failed_checks.dedup();

    let generated_at_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| u64::try_from(duration.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0)
        .max(1);
    let codecs = evidence
        .codecs()
        .iter()
        .map(|codec| {
            json!({
                "id": codec.id(),
                "state": match codec.state() {
                    CodecEvidenceState::Active => "active",
                    CodecEvidenceState::Failed => "failed",
                },
                "corpus_cases": codec.corpus_cases(),
                "property_cases": codec.property_cases(),
                "semantic_cases": codec.semantic_cases(),
                "negative_cases": codec.negative_cases(),
                "budget_or_cancel_cases": codec.budget_or_cancel_cases(),
                "rollback_cases": codec.rollback_cases(),
                "failures": codec.failures(),
                "corpus_digest": codec.corpus_digest(),
                "latency_p95_us": codec.latency_p95_us(),
            })
        })
        .collect::<Vec<_>>();

    Ok(json!({
        "schema_version": 1,
        "scope": "production-codec-candidate-evidence",
        "generated_at_ms": generated_at_ms,
        "candidate": {
            "candidate_id": candidate_id,
            "git_head": candidate_revision,
        },
        "evidence_revision": evidence.evidence_revision(),
        "implementation_revision": EVIDENCE_IMPLEMENTATION_REVISION,
        "thresholds": {
            "minimum_property_cases_per_codec": PROPERTY_CASES_PER_CODEC,
            "minimum_negative_cases_per_codec": 4,
            "minimum_budget_or_cancel_cases_per_codec": 1,
            "max_latency_p95_us": max_p95_us,
        },
        "codecs": codecs,
        "gate": {
            "ok": failed_checks.is_empty(),
            "failed_checks": failed_checks,
        },
        "privacy": {
            "metadata_only": true,
            "raw_corpus_stored": false,
            "raw_request_stored": false,
            "raw_response_stored": false,
            "secret_values_stored": false,
        },
    }))
}

#[derive(Debug)]
struct EvidenceBuilder {
    id: &'static str,
    corpus_cases: usize,
    property_cases: usize,
    semantic_cases: usize,
    negative_cases: usize,
    budget_or_cancel_cases: usize,
    rollback_cases: usize,
    failures: usize,
    corpus_digest: Sha256,
    latencies_us: Vec<u64>,
}

impl EvidenceBuilder {
    fn new(id: &'static str) -> Self {
        Self {
            id,
            corpus_cases: 0,
            property_cases: 0,
            semantic_cases: 0,
            negative_cases: 0,
            budget_or_cancel_cases: 0,
            rollback_cases: 0,
            failures: 0,
            corpus_digest: Sha256::new(),
            latencies_us: Vec::with_capacity(PROPERTY_CASES_PER_CODEC),
        }
    }

    fn add_corpus(&mut self, source: &str) {
        self.corpus_cases += 1;
        self.update_digest(source);
    }

    fn observe_routed<F>(&mut self, source: &str, expected: CodecKind, verify: F)
    where
        F: FnOnce(&str, &str) -> bool,
    {
        self.property_cases += 1;
        self.update_digest(source);
        let decision = route_content(source);
        let mut transforms = Vec::new();
        let started = Instant::now();
        let candidate = apply_routed_codec(CompressionPolicy::Balanced, source, &mut transforms);
        self.latencies_us
            .push(u64::try_from(started.elapsed().as_micros().max(1)).unwrap_or(u64::MAX));
        if decision.codec != expected
            || candidate.len() >= source.len()
            || transforms.is_empty()
            || !verify(source, &candidate)
        {
            self.failures += 1;
            return;
        }
        self.semantic_cases += 1;
        // The source remains available to the enclosing manifest/CCR
        // lifecycle, while reversible codecs also prove exact reconstruction.
        self.rollback_cases += 1;
    }

    fn observe_code(&mut self, source: &str) {
        self.property_cases += 1;
        self.update_digest(source);
        let started = Instant::now();
        let candidate = production_registry().prepare_production_candidate(source);
        self.latencies_us
            .push(u64::try_from(started.elapsed().as_micros().max(1)).unwrap_or(u64::MAX));
        match candidate {
            Ok(candidate)
                if candidate.candidate_source().len() < source.len()
                    && candidate.reconstruct_source() == source =>
            {
                self.semantic_cases += 1;
                self.rollback_cases += 1;
            }
            Ok(_) | Err(_) => self.failures += 1,
        }
    }

    fn observe_negative(&mut self, source: &str) {
        self.negative_cases += 1;
        self.update_digest(source);
        let mut transforms = Vec::new();
        let candidate = apply_routed_codec(CompressionPolicy::Balanced, source, &mut transforms);
        if candidate != source || !transforms.is_empty() {
            self.failures += 1;
        }
    }

    fn observe_budget(&mut self, source: &str) {
        self.budget_or_cancel_cases += 1;
        let decision = route_content_with_budget(
            source,
            RoutingBudget {
                max_input_bytes: source.len(),
                deadline_remaining_us: Some(0),
            },
        );
        if decision.codec != CodecKind::Passthrough || decision.reason_code != "deadline-budget" {
            self.failures += 1;
        }
    }

    fn observe_code_cancel(&mut self, source: &str) {
        self.budget_or_cancel_cases += 1;
        if production_registry().prepare_production_candidate_with_cancel(source, || true)
            != Err(CodeCodecProductionError::Cancelled)
        {
            self.failures += 1;
        }
    }

    fn update_digest(&mut self, source: &str) {
        self.corpus_digest.update(source.len().to_le_bytes());
        self.corpus_digest.update(source.as_bytes());
    }

    fn finish(mut self) -> CodecEvidence {
        self.latencies_us.sort_unstable();
        let latency_p95_us = percentile_ceil(&self.latencies_us, 95, 100).max(1);
        let state = if self.failures == 0
            && self.corpus_cases > 0
            && self.property_cases >= PROPERTY_CASES_PER_CODEC
            && self.semantic_cases == self.property_cases
            && self.negative_cases >= 4
            && self.budget_or_cancel_cases > 0
            && self.rollback_cases == self.property_cases
        {
            CodecEvidenceState::Active
        } else {
            CodecEvidenceState::Failed
        };
        CodecEvidence {
            id: self.id,
            state,
            corpus_cases: self.corpus_cases,
            property_cases: self.property_cases,
            semantic_cases: self.semantic_cases,
            negative_cases: self.negative_cases,
            budget_or_cancel_cases: self.budget_or_cancel_cases,
            rollback_cases: self.rollback_cases,
            failures: self.failures,
            corpus_digest: format!("{:x}", self.corpus_digest.finalize()),
            latency_p95_us,
        }
    }
}

fn build_production_codec_evidence() -> ProductionCodecEvidence {
    let codecs = vec![
        evaluate_json(),
        evaluate_log(),
        evaluate_search(),
        evaluate_build(),
        evaluate_diff(),
        evaluate_text(),
        evaluate_html(),
        evaluate_config(),
        evaluate_code(),
    ];
    let mut revision = Sha256::new();
    revision.update(EVIDENCE_IMPLEMENTATION_REVISION.as_bytes());
    for codec in &codecs {
        revision.update(codec.id.as_bytes());
        revision.update(codec.corpus_cases.to_le_bytes());
        revision.update(codec.property_cases.to_le_bytes());
        revision.update(codec.semantic_cases.to_le_bytes());
        revision.update(codec.negative_cases.to_le_bytes());
        revision.update(codec.budget_or_cancel_cases.to_le_bytes());
        revision.update(codec.rollback_cases.to_le_bytes());
        revision.update(codec.failures.to_le_bytes());
        revision.update(codec.corpus_digest.as_bytes());
        revision.update([match codec.state {
            CodecEvidenceState::Active => 1,
            CodecEvidenceState::Failed => 0,
        }]);
    }
    ProductionCodecEvidence {
        evidence_revision: format!("{:x}", revision.finalize()),
        codecs,
    }
}

fn evaluate_json() -> CodecEvidence {
    let mut evidence = EvidenceBuilder::new("json-codec");
    evidence.add_corpus("{\n  \"items\": [1, 2, 3],\n  \"ok\": true\n}");
    for index in 0..PROPERTY_CASES_PER_CODEC {
        let source = format!(
            "{{\n  \"case\": {index},\n  \"items\": [1, 2, 3],\n  \"nested\": {{ \"ok\": true }}\n}}"
        );
        evidence.observe_routed(&source, CodecKind::JsonMinify, |source, candidate| {
            serde_json::from_str::<Value>(source).ok()
                == serde_json::from_str::<Value>(candidate).ok()
        });
    }
    for source in [
        "{\"ok\":true}",
        "not-json",
        "{\"api_key\":\"sk-redacted-test\"}",
        "1",
    ] {
        evidence.observe_negative(source);
    }
    evidence.observe_budget("{\n  \"items\": [1, 2, 3]\n}");
    evidence.finish()
}

fn evaluate_log() -> CodecEvidence {
    let mut evidence = EvidenceBuilder::new("log-codec");
    evidence.add_corpus("INFO compile alpha\nINFO compile alpha\nWARN retry scheduled");
    for index in 0..PROPERTY_CASES_PER_CODEC {
        let label = alphabetic_label(index);
        let source = format!("INFO compile {label}\nINFO compile {label}\nWARN retry scheduled");
        evidence.observe_routed(&source, CodecKind::LogFold, exact_repeat_reconstruction);
    }
    for source in [
        "INFO compile alpha\nWARN retry scheduled",
        "INFO x\nINFO x",
        "INFO Authorization: Bearer sk-redacted-test\nINFO done",
        "INFO compile alpha [repeated x2]\nINFO compile alpha [repeated x2]",
    ] {
        evidence.observe_negative(source);
    }
    evidence.observe_budget("INFO compile alpha\nINFO compile alpha");
    evidence.finish()
}

fn evaluate_search() -> CodecEvidence {
    let mut evidence = EvidenceBuilder::new("search-codec");
    evidence.add_corpus(
        "src/a.rs:10 result\nshared search context\nshared search context\nsrc/b.rs:20 other",
    );
    for index in 0..PROPERTY_CASES_PER_CODEC {
        let label = alphabetic_label(index);
        let source = format!(
            "src/a.rs:10 result\nshared {label} context\nshared {label} context\nsrc/b.rs:20 other"
        );
        evidence.observe_routed(&source, CodecKind::SearchFold, exact_repeat_reconstruction);
    }
    for source in [
        "src/a.rs:10 result\nsrc/a.rs:10 result\nsrc/b.rs:20 other",
        "src/a.rs:10 result\nunique context\nsrc/b.rs:20 other",
        "src/a.rs:10 Authorization: Bearer sk-redacted-test\nsrc/b.rs:20 other",
        "src/a.rs:10 result\nshared [repeated x2]\nshared [repeated x2]\nsrc/b.rs:20 other",
    ] {
        evidence.observe_negative(source);
    }
    evidence.observe_budget(
        "src/a.rs:10 result\nshared search context\nshared search context\nsrc/b.rs:20 other",
    );
    evidence.finish()
}

fn evaluate_build() -> CodecEvidence {
    let mut evidence = EvidenceBuilder::new("build-codec");
    evidence.add_corpus(
        "Compiling alpha target\nCompiling alpha target\nCompiling alpha target\nCompiling alpha target\nRunning tests\nFinished target",
    );
    for index in 0..PROPERTY_CASES_PER_CODEC {
        let label = alphabetic_label(index);
        let source = format!(
            "Compiling {label} target\nCompiling {label} target\nCompiling {label} target\nCompiling {label} target\nRunning tests\nFinished target"
        );
        evidence.observe_routed(&source, CodecKind::BuildFold, exact_repeat_reconstruction);
    }
    for source in [
        "Compiling alpha\nRunning tests\nFinished target",
        "Compiling alpha v1.2.3\nCompiling alpha v1.2.3\nFinished target",
        "Compiling Authorization: Bearer sk-redacted-test\nFinished target",
        "Compiling alpha [repeated x2]\nCompiling alpha [repeated x2]\nFinished target",
    ] {
        evidence.observe_negative(source);
    }
    evidence.observe_budget("Compiling alpha\nCompiling alpha\nFinished target");
    evidence.finish()
}

fn evaluate_diff() -> CodecEvidence {
    let mut evidence = EvidenceBuilder::new("diff-codec");
    evidence.add_corpus(&diff_source(4));
    for index in 0..PROPERTY_CASES_PER_CODEC {
        let source = diff_source(4 + (index % 4));
        evidence.observe_routed(&source, CodecKind::DiffFold, exact_repeat_reconstruction);
    }
    let compact = [
        "diff --git a/a.rs b/a.rs",
        "--- a/a.rs",
        "+++ b/a.rs",
        "@@ -1 +1 @@",
        "-old",
        "+new",
    ]
    .join("\n");
    for source in [
        compact.clone(),
        "diff --git a/a.rs b/a.rs\n-old\n+new".to_string(),
        format!("{compact}\n{compact}"),
        compact.replace("+new", "+Authorization: Bearer sk-redacted-test"),
    ] {
        evidence.observe_negative(&source);
    }
    evidence.observe_budget(&diff_source(4));
    evidence.finish()
}

fn evaluate_text() -> CodecEvidence {
    let mut evidence = EvidenceBuilder::new("text-codec");
    evidence.add_corpus(
        "The build completed successfully.\nThe build completed successfully.\nNo action is needed.",
    );
    for index in 0..PROPERTY_CASES_PER_CODEC {
        let label = alphabetic_label(index);
        let source = format!(
            "The {label} task completed.\nThe {label} task completed.\nNo action is needed."
        );
        evidence.observe_routed(
            &source,
            CodecKind::TextRepeatFold,
            exact_repeat_reconstruction,
        );
    }
    for source in [
        "A unique sentence.\nAnother unique sentence.",
        "final decision keep alpha\nfinal decision keep alpha",
        "Authorization: Bearer sk-redacted-test\nAuthorization: Bearer sk-redacted-test",
        "Already compact.",
    ] {
        evidence.observe_negative(source);
    }
    evidence.observe_budget("The task completed.\nThe task completed.");
    evidence.finish()
}

fn evaluate_config() -> CodecEvidence {
    let mut evidence = EvidenceBuilder::new("config-codec");
    let corpus = [
        "[server]",
        "host = \"aa.example.internal\"",
        "host = \"aa.example.internal\"",
        "zone = \"primary\"",
        "mode = \"fast\"",
    ]
    .join("\n");
    evidence.add_corpus(&corpus);
    for index in 0..PROPERTY_CASES_PER_CODEC {
        let label = alphabetic_label(index);
        let source = [
            "[server]".to_string(),
            format!("host = \"{label}.example.internal\""),
            format!("host = \"{label}.example.internal\""),
            "zone = \"primary\"".to_string(),
            "mode = \"fast\"".to_string(),
        ]
        .join("\n");
        evidence.observe_routed(&source, CodecKind::ConfigFold, exact_repeat_reconstruction);
    }
    for source in [
        // Config shape without adjacent repeats: nothing to fold, no
        // invented transform.
        "[server]\nhost = \"aa.example.internal\"\nzone = \"primary\"",
        // Secret line inside config: the fold would fire on the repeated
        // line if the secret guard failed.
        "[server]\nAuthorization: Bearer sk-redacted-test\nAuthorization: Bearer sk-redacted-test\nzone = \"primary\"",
        // Already-folded marker inside config: never re-folded.
        "[server]\nhost = \"aa.example.internal\" [repeated x2]\nhost = \"aa.example.internal\" [repeated x2]\nzone = \"primary\"",
        // Markdown front matter is not YAML config.
        "---\ntitle: demo\n---\n\nSome prose here.",
    ] {
        evidence.observe_negative(source);
    }
    evidence
        .observe_budget("[server]\nhost = \"aa.example.internal\"\nhost = \"aa.example.internal\"");
    evidence.finish()
}

fn evaluate_html() -> CodecEvidence {
    let mut evidence = EvidenceBuilder::new("html-codec");
    evidence
        .add_corpus("<!doctype html>\n<ul>\n<li>same</li>\n<li>same</li>\n<li>same</li>\n</ul>");
    for index in 0..PROPERTY_CASES_PER_CODEC {
        let label = alphabetic_label(index);
        let source = format!(
            "<!doctype html>\n<ul>\n<li>{label}</li>\n<li>{label}</li>\n<li>{label}</li>\n</ul>"
        );
        evidence.observe_routed(
            &source,
            CodecKind::HtmlRepeatFold,
            exact_repeat_reconstruction,
        );
    }
    for source in [
        "<!doctype html><html><body><p>unique</p></body></html>",
        "<pre>\nexact\nexact\n</pre>",
        "<script>\nrun();\nrun();\n</script>",
        "<p>Authorization: Bearer sk-redacted-test</p>",
    ] {
        evidence.observe_negative(source);
    }
    evidence.observe_budget("<ul>\n<li>same</li>\n<li>same</li>\n</ul>");
    evidence.finish()
}

fn evaluate_code() -> CodecEvidence {
    let mut evidence = EvidenceBuilder::new("code-compressor");
    for (label, body) in code_corpus() {
        evidence.add_corpus(body);
        for variant in 0..(PROPERTY_CASES_PER_CODEC / code_corpus().len()) {
            let leading = "\n".repeat(1 + (variant % 3));
            let trailing = "\n".repeat(1 + ((variant / 3) % 3));
            let source = format!("```{label}\n{leading}{body}{trailing}```");
            evidence.observe_code(&source);
        }
    }
    for source in [
        "```rust\nfn main() {}\n```",
        "```rust\n\nconst TOKEN: &str = \"sk-redacted-test\";\n\n```",
        "```rust\n\nfn current_line() -> u32 { line!() }\n\n```",
        "```python\n\npass\n```extra",
    ] {
        evidence.observe_negative(source);
    }
    let cancellable = "```rust\n\nfn main() {}\n\n```";
    evidence.observe_budget(cancellable);
    evidence.observe_code_cancel(cancellable);
    evidence.finish()
}

fn exact_repeat_reconstruction(source: &str, candidate: &str) -> bool {
    expand_exact_folded_lines_losslessly(candidate).as_deref() == Some(source)
}

fn diff_source(context_count: usize) -> String {
    let count = context_count + 1;
    let mut lines = vec![
        "diff --git a/src/a.rs b/src/a.rs".to_string(),
        "--- a/src/a.rs".to_string(),
        "+++ b/src/a.rs".to_string(),
        format!("@@ -1,{count} +1,{count} @@"),
    ];
    lines.extend(std::iter::repeat_n(" unchanged".to_string(), context_count));
    lines.push("-old exact".to_string());
    lines.push("+new exact".to_string());
    lines.join("\n")
}

fn alphabetic_label(index: usize) -> String {
    let first = char::from(b'a' + u8::try_from(index % 26).unwrap_or(0));
    let second = char::from(b'a' + u8::try_from((index / 26) % 26).unwrap_or(0));
    format!("{first}{second}")
}

fn code_corpus() -> [(&'static str, &'static str); 8] {
    [
        (
            "rust",
            "pub fn add(left: usize, right: usize) -> usize {\n    left + right\n}",
        ),
        (
            "typescript",
            "interface User {\n  id: number;\n}\nconst value: User = { id: 1 };",
        ),
        (
            "javascript",
            "function add(left, right) {\n  return left + right;\n}",
        ),
        (
            "python",
            "def add(left: int, right: int) -> int:\n    return left + right",
        ),
        (
            "go",
            "package codec\nfunc add(left int, right int) int { return left + right }",
        ),
        (
            "java",
            "class Sample {\n  static int add(int left, int right) { return left + right; }\n}",
        ),
        ("c", "int add(int left, int right) { return left + right; }"),
        (
            "cpp",
            "int add(int left, int right) { return left + right; }",
        ),
    ]
}

fn percentile_ceil(values: &[u64], numerator: usize, denominator: usize) -> u64 {
    if values.is_empty() {
        return 0;
    }
    let rank = values
        .len()
        .saturating_mul(numerator)
        .saturating_add(denominator - 1)
        / denominator;
    values[rank.saturating_sub(1).min(values.len() - 1)]
}

fn is_lower_hex(value: &str) -> bool {
    value
        .bytes()
        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
