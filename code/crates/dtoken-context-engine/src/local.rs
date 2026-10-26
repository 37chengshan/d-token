use crate::classify::classify_unit;
use crate::validate::{validate_json_candidate, validate_unit_candidate, ValidationReport};
use dtoken_compress::content_router::apply_routed_codec;
use dtoken_compress::context_pipeline;
use dtoken_compress::hybrid::ResolvedOptimizationContext;
use dtoken_compress::protection::{
    inventory_context, protected_spans, ContextUnit, ContextUnitKind,
};
use dtoken_runtime_pipeline::{
    PipelineError, PipelineInput, RecoveryPrepareReceipt, TransformArtifact,
};
use dtoken_types::{
    CompressionPolicy, EvidenceSource, EvidenceValue, Protocol, RequestScope, SentPayloadVariant,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContextBudget {
    pub max_bytes: usize,
    pub min_savings_bytes: usize,
}

impl ContextBudget {
    pub fn bytes(max_bytes: usize) -> Self {
        Self {
            max_bytes,
            min_savings_bytes: 1,
        }
    }

    pub fn tokens(tokens: usize) -> Self {
        Self::bytes(tokens.saturating_mul(4))
    }
}

impl Default for ContextBudget {
    fn default() -> Self {
        Self {
            max_bytes: 8 * 1024 * 1024,
            min_savings_bytes: 8,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveryInput {
    pub recovery_ref: String,
    pub expires_at_unix: u64,
}

impl RecoveryInput {
    pub fn new(recovery_ref: impl Into<String>, expires_at_unix: u64) -> Self {
        Self {
            recovery_ref: recovery_ref.into(),
            expires_at_unix,
        }
    }
}

#[derive(Debug, Clone)]
pub struct LocalContextConfig {
    pub budget: ContextBudget,
    pub policy: CompressionPolicy,
    pub recovery: Option<RecoveryInput>,
}

impl Default for LocalContextConfig {
    fn default() -> Self {
        Self {
            budget: ContextBudget::default(),
            policy: CompressionPolicy::Balanced,
            recovery: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ContextInput {
    pub request_id: String,
    pub scope: String,
    pub budget: ContextBudget,
    pub units: Vec<ContextUnit>,
}

impl ContextInput {
    pub fn new(
        request_id: impl Into<String>,
        scope: impl Into<String>,
        budget: ContextBudget,
        units: Vec<ContextUnit>,
    ) -> Self {
        Self {
            request_id: request_id.into(),
            scope: scope.into(),
            budget,
            units,
        }
    }
}

#[derive(Debug, Clone)]
pub struct PreparedContextCandidate {
    pub body: Value,
    pub original_digest: String,
    pub candidate_digest: String,
    pub validation: ValidationReport,
    pub transforms: Vec<String>,
    pub recovery_ref: Option<String>,
    pub source: &'static str,
}

#[derive(Debug, Clone)]
pub struct BaselineContext {
    pub body: Value,
    pub digest: String,
    pub reason_code: String,
}

#[derive(Debug, Clone)]
pub enum ContextOutcome {
    Candidate(PreparedContextCandidate),
    Baseline(BaselineContext),
}

impl ContextOutcome {
    pub fn body(&self) -> &Value {
        match self {
            Self::Candidate(candidate) => &candidate.body,
            Self::Baseline(baseline) => &baseline.body,
        }
    }

    pub fn is_candidate(&self) -> bool {
        matches!(self, Self::Candidate(_))
    }

    pub fn reason_code(&self) -> &str {
        match self {
            Self::Candidate(_) => "validated_optimized_candidate",
            Self::Baseline(baseline) => &baseline.reason_code,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ContextEngine {
    config: LocalContextConfig,
}

impl ContextEngine {
    pub fn local(config: LocalContextConfig) -> Self {
        Self { config }
    }

    pub fn config(&self) -> &LocalContextConfig {
        &self.config
    }

    /// Prepare normalized content units.  This path is provider-neutral and is
    /// useful before a provider adapter serializes a request body.
    pub fn prepare(&self, input: ContextInput) -> ContextOutcome {
        if input.units.is_empty() {
            return baseline_units(&input.units, "empty_input");
        }
        if let Some(recovery) = &self.config.recovery {
            if RecoveryPrepareReceipt::new(recovery.recovery_ref.clone(), recovery.expires_at_unix)
                .is_err()
            {
                return baseline_units(&input.units, "recovery_prepare_failed");
            }
        }
        let inventory = inventory_context(&input.units);
        let protected = protected_spans(&input.units, &inventory);
        let mut candidate = input.units.clone();
        let mut transforms = Vec::new();
        for unit in &mut candidate {
            let classification = classify_unit(unit);
            if classification.protected || unit.kind() != ContextUnitKind::Prose {
                continue;
            }
            let Ok(text) = std::str::from_utf8(unit.bytes()) else {
                continue;
            };
            let mut local_transforms = Vec::new();
            let compressed = apply_routed_codec(self.config.policy.clone(), text, &mut local_transforms);
            if compressed.len() < text.len() {
                *unit = ContextUnit::new(unit.id(), unit.kind(), compressed.into_bytes())
                    .protected(false);
                transforms.extend(local_transforms);
            }
        }
        transforms.sort();
        transforms.dedup();
        if candidate == input.units {
            return baseline_units(&input.units, "no_safe_candidate");
        }
        let validation = match validate_unit_candidate(
            &input.units,
            &candidate,
            &protected,
            input.budget.max_bytes.min(self.config.budget.max_bytes),
            input
                .budget
                .min_savings_bytes
                .max(self.config.budget.min_savings_bytes),
        ) {
            Ok(report) => report,
            Err(reason) => return baseline_units(&input.units, reason),
        };
        let body = units_to_value(&candidate);
        ContextOutcome::Candidate(PreparedContextCandidate {
            body,
            original_digest: validation.original_digest.clone(),
            candidate_digest: validation.candidate_digest.clone(),
            validation,
            transforms,
            recovery_ref: self
                .config
                .recovery
                .as_ref()
                .map(|value| value.recovery_ref.clone()),
            source: "local",
        })
    }

    /// Prepare an already normalized provider body.  Codec and manifest
    /// validation are delegated to dtoken-compress, while the runtime
    /// pipeline enforces source digest, recovery and ReadyToSend invariants.
    pub fn prepare_body(
        &self,
        scope: RequestScope,
        context: &ResolvedOptimizationContext,
        body: Value,
        policy: CompressionPolicy,
        recovery: Option<RecoveryInput>,
    ) -> ContextOutcome {
        let original_body = body.clone();
        let original_bytes = match serde_json::to_vec(&body) {
            Ok(bytes) => bytes,
            Err(_) => return baseline_body(original_body, "input_serialization_failed"),
        };
        let pipeline = match PipelineInput::new(
            scope.clone(),
            original_bytes.clone(),
            self.config.budget.max_bytes,
        ) {
            Ok(input) => input.prepare(),
            Err(error) => return baseline_body(original_body, pipeline_reason(error)),
        };
        let prepared =
            match context_pipeline::prepare_context_transform(context, policy, body.clone()) {
                Ok(Some(value)) => value,
                Ok(None) => return baseline_body(original_body, "no_safe_candidate"),
                Err(error) => return baseline_body(original_body, context_reason(error)),
            };
        let validated = match prepared.validate() {
            Ok(value) => value,
            Err(error) => return baseline_body(original_body, context_reason(error)),
        };
        let (candidate_body, manifest) = validated.into_parts();
        let candidate_bytes = match serde_json::to_vec(&candidate_body) {
            Ok(bytes) => bytes,
            Err(_) => return baseline_body(original_body, "candidate_serialization_failed"),
        };
        let report = match validate_json_candidate(
            &original_bytes,
            &candidate_bytes,
            self.config.budget.max_bytes,
            self.config.budget.min_savings_bytes,
        ) {
            Ok(report) => report,
            Err(reason) => return baseline_body(original_body, reason),
        };
        let recovery = recovery.or_else(|| self.config.recovery.clone());
        let recovery = recovery.and_then(|value| {
            RecoveryPrepareReceipt::new(value.recovery_ref, value.expires_at_unix).ok()
        });
        let recovery_ref = recovery
            .as_ref()
            .map(|value| value.recovery_ref().to_string());
        let artifact = match TransformArtifact::new(
            pipeline.baseline_digest(),
            candidate_bytes,
            manifest.transforms().to_vec(),
        ) {
            Ok(artifact) => artifact,
            Err(error) => return baseline_body(original_body, pipeline_reason(error)),
        };
        let candidate = match pipeline.with_candidate(artifact, recovery) {
            Ok(candidate) => candidate,
            Err(error) => return baseline_body(original_body, pipeline_reason(error)),
        };
        let validated = match candidate.validate(|baseline, candidate| {
            baseline.len() > candidate.len() && serde_json::from_slice::<Value>(candidate).is_ok()
        }) {
            Ok(value) => value,
            Err(error) => return baseline_body(original_body, pipeline_reason(error)),
        };
        let transforms = manifest.transforms().to_vec();
        let ready = validated.into_ready_to_send();
        if ready.variant() != SentPayloadVariant::Optimized {
            return baseline_body(original_body, "runtime_baseline");
        }
        let output_body = match serde_json::from_slice::<Value>(ready.body()) {
            Ok(value) => value,
            Err(_) => return baseline_body(original_body, "candidate_deserialization_failed"),
        };
        ContextOutcome::Candidate(PreparedContextCandidate {
            body: output_body,
            original_digest: report.original_digest.clone(),
            candidate_digest: report.candidate_digest.clone(),
            validation: report,
            transforms,
            recovery_ref,
            source: "local",
        })
    }
}

fn baseline_units(units: &[ContextUnit], reason_code: &str) -> ContextOutcome {
    let body = units_to_value(units);
    baseline_body(body, reason_code)
}

fn baseline_body(body: Value, reason_code: &str) -> ContextOutcome {
    let digest = serde_json::to_vec(&body)
        .map(|bytes| format!("{:x}", Sha256::digest(bytes)))
        .unwrap_or_default();
    ContextOutcome::Baseline(BaselineContext {
        body,
        digest,
        reason_code: reason_code.to_string(),
    })
}

fn units_to_value(units: &[ContextUnit]) -> Value {
    Value::Array(
        units
            .iter()
            .map(|unit| {
                json!({
                    "id": unit.id(),
                    "kind": format!("{:?}", unit.kind()),
                    "bytes": unit.bytes(),
                })
            })
            .collect(),
    )
}

fn pipeline_reason(error: PipelineError) -> &'static str {
    match error {
        PipelineError::EmptyIngress => "empty_input",
        PipelineError::IngressTooLarge => "input_over_budget",
        PipelineError::EmptyCandidate => "empty_candidate",
        PipelineError::EmptyTransformList => "empty_transform_list",
        PipelineError::EmptySourceDigest => "source_digest_missing",
        PipelineError::SourceDigestMismatch => "source_digest_mismatch",
        PipelineError::MissingRecoveryReceipt => "recovery_prepare_failed",
        PipelineError::InvalidRecoveryReceipt => "recovery_prepare_failed",
        PipelineError::ValidatorRejected => "validator_rejected",
        PipelineError::InvalidCommitTimestamp => "invalid_commit_timestamp",
    }
}

fn context_reason(error: context_pipeline::ContextTransformError) -> &'static str {
    match error {
        context_pipeline::ContextTransformError::UnsupportedContext => "unsupported_context",
        context_pipeline::ContextTransformError::MissingSourceValue => "missing_source_value",
        context_pipeline::ContextTransformError::SourceDigestMismatch => "source_digest_mismatch",
        context_pipeline::ContextTransformError::CandidateDigestMismatch => {
            "candidate_digest_mismatch"
        }
        context_pipeline::ContextTransformError::MutationOutsideManifest => {
            "mutation_outside_manifest"
        }
        context_pipeline::ContextTransformError::MissingDeclaredMutation => {
            "missing_declared_mutation"
        }
        context_pipeline::ContextTransformError::CodecSemanticMismatch => "codec_semantic_mismatch",
        context_pipeline::ContextTransformError::NonBeneficialCandidate => "candidate_expansion",
        context_pipeline::ContextTransformError::InvalidProviderCacheEvidence => {
            "invalid_cache_evidence"
        }
        context_pipeline::ContextTransformError::ProviderCacheEvidenceMismatch => {
            "cache_evidence_mismatch"
        }
    }
}

fn digest_scope_id(request_id: &str) -> Uuid {
    Uuid::parse_str(request_id).unwrap_or_else(|_| Uuid::nil())
}

#[allow(dead_code)]
fn synthetic_scope(request_id: &str, route_id: &str) -> RequestScope {
    RequestScope::new(
        digest_scope_id(request_id),
        EvidenceValue::known("dtoken-context-engine".to_string(), EvidenceSource::Derived),
        EvidenceValue::known(route_id.to_string(), EvidenceSource::Derived),
        EvidenceValue::unknown("local_context_engine"),
        EvidenceValue::unknown("local_context_engine"),
        route_id,
        "local",
        "local",
        Protocol::OpenAi,
        Protocol::OpenAi,
        "local-v1",
    )
    .expect("synthetic local scope is valid")
}

#[allow(dead_code)]
fn _now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |value| value.as_secs())
}
