use crate::content_router::{route_content, CodecKind, ContentKind};
use crate::hybrid::{
    lower_hybrid_context, ApprovedHybridTarget, HybridContextIr, HybridProtectionReason,
    ResolvedOptimizationContext,
};
use dtoken_types::{CompressionPolicy, Protocol};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZoneDisposition {
    FrozenPrefix,
    TransformableWindow,
    ProtectedTail,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextZone {
    node_id: String,
    pointer: String,
    source_digest: String,
    disposition: ZoneDisposition,
    reason_code: &'static str,
}

impl ContextZone {
    pub fn node_id(&self) -> &str {
        &self.node_id
    }

    pub fn pointer(&self) -> &str {
        &self.pointer
    }

    pub fn source_digest(&self) -> &str {
        &self.source_digest
    }

    pub fn disposition(&self) -> ZoneDisposition {
        self.disposition
    }

    pub fn reason_code(&self) -> &str {
        self.reason_code
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheAlignmentPlan {
    resolved_identity_digest: String,
    stable_prefix_digest: Option<String>,
    cache_key_digest: Option<String>,
    frozen_prefix_bytes: usize,
    frozen_zone_count: usize,
    breakpoint_zone_index: usize,
    evidence_status: CacheEvidenceStatus,
    provider_strategy: Option<ProviderCacheStrategy>,
    evidence_revision: Option<String>,
    ttl_seconds: Option<u64>,
    min_prefix_bytes: Option<usize>,
}

impl CacheAlignmentPlan {
    pub fn resolved_identity_digest(&self) -> &str {
        &self.resolved_identity_digest
    }

    pub fn stable_prefix_digest(&self) -> Option<&str> {
        self.stable_prefix_digest.as_deref()
    }

    pub fn frozen_zone_count(&self) -> usize {
        self.frozen_zone_count
    }

    pub fn frozen_prefix_bytes(&self) -> usize {
        self.frozen_prefix_bytes
    }

    pub fn cache_key_digest(&self) -> Option<&str> {
        self.cache_key_digest.as_deref()
    }

    pub fn breakpoint_zone_index(&self) -> usize {
        self.breakpoint_zone_index
    }

    pub fn evidence_status(&self) -> CacheEvidenceStatus {
        self.evidence_status
    }

    pub fn provider_strategy(&self) -> Option<ProviderCacheStrategy> {
        self.provider_strategy
    }

    pub fn evidence_revision(&self) -> Option<&str> {
        self.evidence_revision.as_deref()
    }

    pub fn ttl_seconds(&self) -> Option<u64> {
        self.ttl_seconds
    }

    pub fn min_prefix_bytes(&self) -> Option<usize> {
        self.min_prefix_bytes
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderCacheStrategy {
    AnthropicExplicit,
    OpenAiAutomatic,
    GoogleExplicit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheEvidenceStatus {
    Unavailable,
    VerifiedBelowMinimum,
    VerifiedEligible,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderCacheEvidence {
    pub route_id: String,
    pub provider_id: String,
    pub model: String,
    pub strategy: ProviderCacheStrategy,
    pub ttl_seconds: Option<u64>,
    pub min_prefix_bytes: usize,
    pub source_revision: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextPlan {
    zones: Vec<ContextZone>,
    cache_alignment: CacheAlignmentPlan,
}

impl ContextPlan {
    pub fn zones(&self) -> &[ContextZone] {
        &self.zones
    }

    pub fn transformable_zones(&self) -> Vec<&ContextZone> {
        self.zones
            .iter()
            .filter(|zone| zone.disposition == ZoneDisposition::TransformableWindow)
            .collect()
    }

    pub fn cache_alignment(&self) -> &CacheAlignmentPlan {
        &self.cache_alignment
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransformManifestEntry {
    pointer: String,
    source_digest: String,
    candidate_digest: String,
    content_kind: ContentKind,
    codec: CodecKind,
    codec_revision: &'static str,
    reason_code: &'static str,
    original_bytes: usize,
    candidate_bytes: usize,
}

impl TransformManifestEntry {
    pub fn pointer(&self) -> &str {
        &self.pointer
    }

    pub fn source_digest(&self) -> &str {
        &self.source_digest
    }

    pub fn candidate_digest(&self) -> &str {
        &self.candidate_digest
    }

    pub fn content_kind(&self) -> ContentKind {
        self.content_kind
    }

    pub fn codec(&self) -> CodecKind {
        self.codec
    }

    pub fn codec_revision(&self) -> &str {
        self.codec_revision
    }

    pub fn reason_code(&self) -> &str {
        self.reason_code
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextTransformManifest {
    original_body_digest: String,
    candidate_body_digest: String,
    stable_prefix_digest: Option<String>,
    entries: Vec<TransformManifestEntry>,
    transforms: Vec<String>,
    bytes_saved: usize,
}

impl ContextTransformManifest {
    pub fn original_body_digest(&self) -> &str {
        &self.original_body_digest
    }

    pub fn candidate_body_digest(&self) -> &str {
        &self.candidate_body_digest
    }

    pub fn stable_prefix_digest(&self) -> Option<&str> {
        self.stable_prefix_digest.as_deref()
    }

    pub fn entries(&self) -> &[TransformManifestEntry] {
        &self.entries
    }

    pub fn transforms(&self) -> &[String] {
        &self.transforms
    }

    pub fn bytes_saved(&self) -> usize {
        self.bytes_saved
    }
}

#[derive(Clone)]
pub struct PreparedContextTransform {
    original_body: Value,
    candidate_body: Value,
    plan: ContextPlan,
    manifest: ContextTransformManifest,
}

impl fmt::Debug for PreparedContextTransform {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PreparedContextTransform")
            .field("plan", &self.plan)
            .field("manifest", &self.manifest)
            .finish_non_exhaustive()
    }
}

impl PreparedContextTransform {
    pub fn original_body(&self) -> &Value {
        &self.original_body
    }

    pub fn candidate_body(&self) -> &Value {
        &self.candidate_body
    }

    pub fn plan(&self) -> &ContextPlan {
        &self.plan
    }

    pub fn manifest(&self) -> &ContextTransformManifest {
        &self.manifest
    }

    pub fn validate(self) -> Result<ValidatedContextTransform, ContextTransformError> {
        validate_context_transform(&self.original_body, &self.candidate_body, &self.manifest)?;
        Ok(ValidatedContextTransform {
            body: self.candidate_body,
            manifest: self.manifest,
        })
    }
}

#[derive(Clone)]
pub struct ValidatedContextTransform {
    body: Value,
    manifest: ContextTransformManifest,
}

impl fmt::Debug for ValidatedContextTransform {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ValidatedContextTransform")
            .field("manifest", &self.manifest)
            .finish_non_exhaustive()
    }
}

impl ValidatedContextTransform {
    pub fn body(&self) -> &Value {
        &self.body
    }

    pub fn into_body(self) -> Value {
        self.body
    }

    pub fn manifest(&self) -> &ContextTransformManifest {
        &self.manifest
    }

    pub fn into_parts(self) -> (Value, ContextTransformManifest) {
        (self.body, self.manifest)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContextTransformError {
    UnsupportedContext,
    MissingSourceValue,
    SourceDigestMismatch,
    CandidateDigestMismatch,
    MutationOutsideManifest,
    MissingDeclaredMutation,
    CodecSemanticMismatch,
    NonBeneficialCandidate,
    InvalidProviderCacheEvidence,
    ProviderCacheEvidenceMismatch,
}

pub fn plan_context(
    context: &ResolvedOptimizationContext,
    body: &Value,
) -> Result<ContextPlan, ContextTransformError> {
    plan_context_with_cache_evidence(context, body, None)
}

pub fn plan_context_with_cache_evidence(
    context: &ResolvedOptimizationContext,
    body: &Value,
    evidence: Option<&ProviderCacheEvidence>,
) -> Result<ContextPlan, ContextTransformError> {
    let ir =
        lower_hybrid_context(context, body).ok_or(ContextTransformError::UnsupportedContext)?;
    apply_cache_evidence(context, build_context_plan(&ir, body), evidence)
}

fn apply_cache_evidence(
    context: &ResolvedOptimizationContext,
    mut plan: ContextPlan,
    evidence: Option<&ProviderCacheEvidence>,
) -> Result<ContextPlan, ContextTransformError> {
    if let Some(evidence) = evidence {
        validate_provider_cache_evidence(context, evidence)?;
        plan.cache_alignment.evidence_status =
            if plan.cache_alignment.stable_prefix_digest.is_some()
                && plan.cache_alignment.frozen_prefix_bytes >= evidence.min_prefix_bytes
            {
                CacheEvidenceStatus::VerifiedEligible
            } else {
                CacheEvidenceStatus::VerifiedBelowMinimum
            };
        plan.cache_alignment.provider_strategy = Some(evidence.strategy);
        plan.cache_alignment.evidence_revision = Some(evidence.source_revision.clone());
        plan.cache_alignment.ttl_seconds = evidence.ttl_seconds;
        plan.cache_alignment.min_prefix_bytes = Some(evidence.min_prefix_bytes);
    }
    Ok(plan)
}

pub fn prepare_context_transform(
    context: &ResolvedOptimizationContext,
    policy: CompressionPolicy,
    original_body: Value,
) -> Result<Option<PreparedContextTransform>, ContextTransformError> {
    prepare_context_transform_with_cache_evidence(context, policy, original_body, None)
}

pub fn prepare_context_transform_with_cache_evidence(
    context: &ResolvedOptimizationContext,
    policy: CompressionPolicy,
    original_body: Value,
    evidence: Option<&ProviderCacheEvidence>,
) -> Result<Option<PreparedContextTransform>, ContextTransformError> {
    if policy == CompressionPolicy::Off || !context.supports_safe_text_codec() {
        return Ok(None);
    }
    let ir = lower_hybrid_context(context, &original_body)
        .ok_or(ContextTransformError::UnsupportedContext)?;
    let plan = apply_cache_evidence(context, build_context_plan(&ir, &original_body), evidence)?;
    let mut transforms = Vec::new();
    let candidate_body = crate::hybrid::apply_safe_text_codec(
        context,
        policy,
        original_body.clone(),
        &mut transforms,
    );
    if candidate_body == original_body {
        return Ok(None);
    }

    let entries = build_manifest_entries(&ir, &original_body, &candidate_body)?;
    if entries.is_empty() {
        return Err(ContextTransformError::MutationOutsideManifest);
    }
    let original_bytes = json_bytes(&original_body).len();
    let candidate_bytes = json_bytes(&candidate_body).len();
    if candidate_bytes >= original_bytes {
        return Err(ContextTransformError::NonBeneficialCandidate);
    }
    let manifest = ContextTransformManifest {
        original_body_digest: digest_value(&original_body),
        candidate_body_digest: digest_value(&candidate_body),
        stable_prefix_digest: plan.cache_alignment.stable_prefix_digest.clone(),
        entries,
        transforms,
        bytes_saved: original_bytes - candidate_bytes,
    };
    Ok(Some(PreparedContextTransform {
        original_body,
        candidate_body,
        plan,
        manifest,
    }))
}

pub fn validate_context_transform(
    original_body: &Value,
    candidate_body: &Value,
    manifest: &ContextTransformManifest,
) -> Result<(), ContextTransformError> {
    if digest_value(original_body) != manifest.original_body_digest {
        return Err(ContextTransformError::SourceDigestMismatch);
    }
    if json_bytes(candidate_body).len() >= json_bytes(original_body).len() {
        return Err(ContextTransformError::NonBeneficialCandidate);
    }

    let declared = manifest
        .entries
        .iter()
        .map(|entry| entry.pointer.as_str())
        .collect::<BTreeSet<_>>();
    let mut changed = BTreeSet::new();
    collect_changed_pointers("", original_body, candidate_body, &mut changed);
    if changed
        .iter()
        .any(|pointer| !declared.contains(pointer.as_str()))
    {
        return Err(ContextTransformError::MutationOutsideManifest);
    }
    if declared.iter().any(|pointer| !changed.contains(*pointer)) {
        return Err(ContextTransformError::MissingDeclaredMutation);
    }
    if digest_value(candidate_body) != manifest.candidate_body_digest {
        return Err(ContextTransformError::CandidateDigestMismatch);
    }

    for entry in &manifest.entries {
        let source = original_body
            .pointer(&entry.pointer)
            .ok_or(ContextTransformError::MissingSourceValue)?;
        let candidate = candidate_body
            .pointer(&entry.pointer)
            .ok_or(ContextTransformError::MissingSourceValue)?;
        if digest_value(source) != entry.source_digest {
            return Err(ContextTransformError::SourceDigestMismatch);
        }
        if digest_value(candidate) != entry.candidate_digest {
            return Err(ContextTransformError::CandidateDigestMismatch);
        }
        validate_codec_semantics(entry.codec, source, candidate)?;
    }
    Ok(())
}

fn build_context_plan(ir: &HybridContextIr, body: &Value) -> ContextPlan {
    let first_transformable = ir
        .nodes()
        .iter()
        .position(|node| target_for_node(ir, node.node_id()).is_some());
    let first_recent = ir.nodes().iter().position(|node| {
        node.turn_index()
            .is_some_and(|turn| turn >= ir.recent_tail_start())
    });
    let breakpoint = first_transformable
        .into_iter()
        .chain(first_recent)
        .min()
        .unwrap_or(ir.nodes().len());

    let zones = ir
        .nodes()
        .iter()
        .enumerate()
        .map(|(index, node)| {
            let target = target_for_node(ir, node.node_id());
            let pointer = target
                .map(ApprovedHybridTarget::patch_path)
                .unwrap_or_else(|| node.source_path())
                .to_string();
            let source_digest = body
                .pointer(&pointer)
                .map_or_else(|| digest_bytes(pointer.as_bytes()), digest_value);
            let disposition = if index < breakpoint {
                ZoneDisposition::FrozenPrefix
            } else if target.is_some() {
                ZoneDisposition::TransformableWindow
            } else {
                ZoneDisposition::ProtectedTail
            };
            ContextZone {
                node_id: node.node_id().to_string(),
                pointer,
                source_digest,
                disposition,
                reason_code: protection_reason_code(node.reason()),
            }
        })
        .collect::<Vec<_>>();

    let stable_prefix_digest = (breakpoint > 0).then(|| {
        let mut hasher = Sha256::new();
        for zone in zones.iter().take(breakpoint) {
            hasher.update(zone.node_id.as_bytes());
            hasher.update([0]);
            hasher.update(zone.pointer.as_bytes());
            hasher.update([0]);
            hasher.update(zone.source_digest.as_bytes());
            hasher.update([0xff]);
        }
        format!("{:x}", hasher.finalize())
    });
    let resolved_identity_digest = ir.resolved_identity_digest().to_string();
    let cache_key_digest = stable_prefix_digest
        .as_ref()
        .map(|prefix| digest_bytes(format!("{resolved_identity_digest}\0{prefix}").as_bytes()));
    let frozen_prefix_bytes = zones
        .iter()
        .take(breakpoint)
        .filter_map(|zone| body.pointer(&zone.pointer))
        .map(|value| json_bytes(value).len())
        .sum();
    ContextPlan {
        zones,
        cache_alignment: CacheAlignmentPlan {
            resolved_identity_digest,
            stable_prefix_digest,
            cache_key_digest,
            frozen_prefix_bytes,
            frozen_zone_count: breakpoint,
            breakpoint_zone_index: breakpoint,
            evidence_status: CacheEvidenceStatus::Unavailable,
            provider_strategy: None,
            evidence_revision: None,
            ttl_seconds: None,
            min_prefix_bytes: None,
        },
    }
}

fn validate_provider_cache_evidence(
    context: &ResolvedOptimizationContext,
    evidence: &ProviderCacheEvidence,
) -> Result<(), ContextTransformError> {
    if evidence.route_id != context.route_id
        || evidence.provider_id != context.provider_id
        || evidence.model != context.model
        || !strategy_matches_protocol(evidence.strategy, &context.input_protocol)
    {
        return Err(ContextTransformError::ProviderCacheEvidenceMismatch);
    }
    if evidence.min_prefix_bytes == 0
        || evidence.ttl_seconds == Some(0)
        || evidence.source_revision.len() != 64
        || !evidence
            .source_revision
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(ContextTransformError::InvalidProviderCacheEvidence);
    }
    Ok(())
}

fn strategy_matches_protocol(strategy: ProviderCacheStrategy, protocol: &Protocol) -> bool {
    matches!(
        (strategy, protocol),
        (
            ProviderCacheStrategy::AnthropicExplicit,
            Protocol::Anthropic
        ) | (ProviderCacheStrategy::OpenAiAutomatic, Protocol::OpenAi)
            | (ProviderCacheStrategy::GoogleExplicit, Protocol::OpenAi)
    )
}

fn build_manifest_entries(
    ir: &HybridContextIr,
    original_body: &Value,
    candidate_body: &Value,
) -> Result<Vec<TransformManifestEntry>, ContextTransformError> {
    let mut entries = Vec::new();
    for target in ir.approved_text_targets() {
        let source = original_body
            .pointer(target.patch_path())
            .ok_or(ContextTransformError::MissingSourceValue)?;
        let candidate = candidate_body
            .pointer(target.patch_path())
            .ok_or(ContextTransformError::MissingSourceValue)?;
        if source == candidate {
            continue;
        }
        if digest_value(source) != target.precondition_digest() {
            return Err(ContextTransformError::SourceDigestMismatch);
        }
        let source_text = source
            .as_str()
            .ok_or(ContextTransformError::CodecSemanticMismatch)?;
        let route = route_content(source_text);
        if route.codec == CodecKind::Passthrough {
            return Err(ContextTransformError::CodecSemanticMismatch);
        }
        entries.push(TransformManifestEntry {
            pointer: target.patch_path().to_string(),
            source_digest: target.precondition_digest().to_string(),
            candidate_digest: digest_value(candidate),
            content_kind: route.content_kind,
            codec: route.codec,
            codec_revision: codec_revision(route.codec),
            reason_code: route.reason_code,
            original_bytes: json_bytes(source).len(),
            candidate_bytes: json_bytes(candidate).len(),
        });
    }
    Ok(entries)
}

fn target_for_node<'a>(ir: &'a HybridContextIr, node_id: &str) -> Option<&'a ApprovedHybridTarget> {
    ir.approved_text_targets()
        .iter()
        .find(|target| target.node_id() == node_id)
}

fn validate_codec_semantics(
    codec: CodecKind,
    source: &Value,
    candidate: &Value,
) -> Result<(), ContextTransformError> {
    let source = source
        .as_str()
        .ok_or(ContextTransformError::CodecSemanticMismatch)?;
    let candidate = candidate
        .as_str()
        .ok_or(ContextTransformError::CodecSemanticMismatch)?;
    match codec {
        CodecKind::JsonMinify => {
            let source: Value = serde_json::from_str(source)
                .map_err(|_| ContextTransformError::CodecSemanticMismatch)?;
            let candidate: Value = serde_json::from_str(candidate)
                .map_err(|_| ContextTransformError::CodecSemanticMismatch)?;
            if source != candidate {
                return Err(ContextTransformError::CodecSemanticMismatch);
            }
        }
        CodecKind::LogFold => {
            for line in source
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
            {
                let normalized = normalize_log_line_for_validation(line);
                if !candidate.contains(line) && !candidate.contains(&normalized) {
                    return Err(ContextTransformError::CodecSemanticMismatch);
                }
            }
        }
        CodecKind::BuildFold
        | CodecKind::SearchFold
        | CodecKind::DiffFold
        | CodecKind::TextRepeatFold
        | CodecKind::HtmlRepeatFold => {
            if expand_exact_folded_lines(candidate).as_deref() != Some(source) {
                return Err(ContextTransformError::CodecSemanticMismatch);
            }
        }
        CodecKind::Passthrough => return Err(ContextTransformError::CodecSemanticMismatch),
    }
    Ok(())
}

fn expand_exact_folded_lines(candidate: &str) -> Option<String> {
    let mut expanded = Vec::new();
    for line in candidate.lines() {
        let Some((base, suffix)) = line.rsplit_once(" [repeated x") else {
            expanded.push(line.to_string());
            continue;
        };
        let Some(count) = suffix
            .strip_suffix(']')
            .and_then(|value| value.parse::<usize>().ok())
        else {
            expanded.push(line.to_string());
            continue;
        };
        if !(2..=1_000_000).contains(&count) {
            return None;
        }
        expanded.extend(std::iter::repeat_n(base.to_string(), count));
    }
    Some(expanded.join("\n"))
}

fn normalize_log_line_for_validation(line: &str) -> String {
    let bytes = line.as_bytes();
    if bytes.len() >= 20
        && bytes.get(4) == Some(&b'-')
        && bytes.get(7) == Some(&b'-')
        && bytes.get(10) == Some(&b'T')
        && bytes.get(13) == Some(&b':')
        && bytes.get(16) == Some(&b':')
    {
        return format!("<ts> {}", line[20..].trim_start());
    }
    line.to_string()
}

fn codec_revision(codec: CodecKind) -> &'static str {
    match codec {
        CodecKind::JsonMinify => "json_minify_v1",
        CodecKind::LogFold => "log_fold_v1",
        CodecKind::SearchFold => "search_fold_v1",
        CodecKind::BuildFold => "build_fold_v1",
        CodecKind::DiffFold => "diff_fold_v1",
        CodecKind::TextRepeatFold => "text_repeat_fold_v1",
        CodecKind::HtmlRepeatFold => "html_repeat_fold_v1",
        CodecKind::Passthrough => "passthrough",
    }
}

fn collect_changed_pointers(
    path: &str,
    original: &Value,
    candidate: &Value,
    changed: &mut BTreeSet<String>,
) {
    if original == candidate {
        return;
    }
    match (original, candidate) {
        (Value::Object(before), Value::Object(after)) => {
            let keys = before.keys().chain(after.keys()).collect::<BTreeSet<_>>();
            for key in keys {
                let escaped = key.replace('~', "~0").replace('/', "~1");
                let next = format!("{path}/{escaped}");
                collect_changed_pointers(
                    &next,
                    before.get(key).unwrap_or(&Value::Null),
                    after.get(key).unwrap_or(&Value::Null),
                    changed,
                );
            }
        }
        (Value::Array(before), Value::Array(after)) if before.len() == after.len() => {
            for (index, (before, after)) in before.iter().zip(after).enumerate() {
                collect_changed_pointers(&format!("{path}/{index}"), before, after, changed);
            }
        }
        _ => {
            changed.insert(path.to_string());
        }
    }
}

fn protection_reason_code(reason: HybridProtectionReason) -> &'static str {
    match reason {
        HybridProtectionReason::System => "system",
        HybridProtectionReason::Developer => "developer",
        HybridProtectionReason::CurrentTurn => "current-turn",
        HybridProtectionReason::RecentTail => "recent-tail",
        HybridProtectionReason::OpenToolState => "open-tool-state",
        HybridProtectionReason::StructuralToolState => "structural-tool-state",
        HybridProtectionReason::ToolSchema => "tool-schema",
        HybridProtectionReason::ToolArguments => "tool-arguments",
        HybridProtectionReason::CodeOrDiff => "code-or-diff",
        HybridProtectionReason::Secret => "secret",
        HybridProtectionReason::Exact => "exact",
        HybridProtectionReason::ExistingImage => "existing-image",
        HybridProtectionReason::Identifier => "identifier",
        HybridProtectionReason::UnknownNode => "unknown-node",
        HybridProtectionReason::GeneralPreserve => "general-preserve",
        HybridProtectionReason::ClosedHistoricalLowRiskToolResult => {
            "closed-historical-low-risk-tool-result"
        }
    }
}

fn json_bytes(value: &Value) -> Vec<u8> {
    serde_json::to_vec(value).unwrap_or_default()
}

fn digest_value(value: &Value) -> String {
    digest_bytes(&json_bytes(value))
}

fn digest_bytes(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
