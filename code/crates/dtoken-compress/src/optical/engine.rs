use std::time::{Duration, Instant};

use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::{lower_hybrid_context, ResolvedOptimizationContext};

use super::applicability::evaluate_applicability_with_policies_and_registry;
use super::capability::{
    resolve_model_capability_record_for_route_with_registry, OpticalCapabilityRegistry,
};
use super::decision::build_shadow_plan_detailed_with_registry;
use super::patcher::{OpticalPatchOperation, OpticalPatchSet, PatchError};
use super::post_validate::{validate_patched_request, PostValidationError, ValidatedActiveRequest};
use super::render_geometry::{estimate_image_tokens, geometry_for_capability};
use super::renderer::{DeterministicPngRenderer, RenderCancellation, RenderError, RenderLimits};
use super::validate::{
    validate_active_plan_input, ActivePlanValidationInput, PlanValidationResult,
};
use dtoken_types::{
    CacheTokens, HybridContextAccounting, InputTokenUsage, ModalityTokens,
    OpticalApplicabilityInput, OpticalGlobalPolicy, OpticalMode, OpticalPlanStatus,
    OptimizationDisposition, PayloadByteUsage, ProviderActualUsage, RouteOpticalPolicy,
    SavingsAttribution, SentPayloadVariant, UpstreamSendState, UsageComparison, UsageEvidence,
    UsageEvidenceKind, UsageMeasurement, UsageVector,
};

const MIN_ACTIVE_CANDIDATE_CHARS: usize = 2_000;
const SIDECAR_TOKENS_PER_PAGE: u64 = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HybridPreparationKind {
    Passthrough,
    Semantic,
    Shadow,
    Active,
    FallbackPreSend,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HybridExecutionReport {
    pub request_id: String,
    pub kind: HybridPreparationKind,
    pub reason_code: String,
    pub candidate_count: usize,
    pub patched_count: usize,
    pub image_count: usize,
    pub source_chars: usize,
    pub png_bytes: u64,
    pub base64_bytes: u64,
    pub estimated_text_tokens_before: u64,
    pub estimated_input_tokens_after: u64,
    pub estimated_signed_input_savings: i64,
    pub capability_revision: Option<u64>,
    pub profile_digest: Option<String>,
    pub font_digest: Option<String>,
    pub capsule_digests: Vec<String>,
    pub artifact_validated: bool,
    pub raw_available: bool,
    pub transforms_applied: Vec<String>,
}

impl HybridExecutionReport {
    fn empty(
        request_id: &str,
        kind: HybridPreparationKind,
        reason_code: impl Into<String>,
        raw_available: bool,
    ) -> Self {
        Self {
            request_id: request_id.to_string(),
            kind,
            reason_code: reason_code.into(),
            candidate_count: 0,
            patched_count: 0,
            image_count: 0,
            source_chars: 0,
            png_bytes: 0,
            base64_bytes: 0,
            estimated_text_tokens_before: 0,
            estimated_input_tokens_after: 0,
            estimated_signed_input_savings: 0,
            capability_revision: None,
            profile_digest: None,
            font_digest: None,
            capsule_digests: Vec::new(),
            artifact_validated: false,
            raw_available,
            transforms_applied: Vec::new(),
        }
    }
}

#[derive(Clone)]
pub enum PreparedHybridRequest {
    Passthrough {
        body: Value,
        report: HybridExecutionReport,
        accounting: HybridContextAccounting,
    },
    Semantic {
        body: Value,
        report: HybridExecutionReport,
        accounting: HybridContextAccounting,
    },
    Shadow {
        body: Value,
        report: HybridExecutionReport,
        accounting: HybridContextAccounting,
    },
    Active {
        request: ValidatedActiveRequest,
        report: HybridExecutionReport,
        accounting: HybridContextAccounting,
    },
    FallbackPreSend {
        body: Value,
        report: HybridExecutionReport,
        accounting: HybridContextAccounting,
    },
}

impl std::fmt::Debug for PreparedHybridRequest {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PreparedHybridRequest")
            .field("kind", &self.kind())
            .field("report", self.report())
            .finish()
    }
}

impl PreparedHybridRequest {
    pub fn kind(&self) -> HybridPreparationKind {
        match self {
            Self::Passthrough { .. } => HybridPreparationKind::Passthrough,
            Self::Semantic { .. } => HybridPreparationKind::Semantic,
            Self::Shadow { .. } => HybridPreparationKind::Shadow,
            Self::Active { .. } => HybridPreparationKind::Active,
            Self::FallbackPreSend { .. } => HybridPreparationKind::FallbackPreSend,
        }
    }

    pub fn body(&self) -> &Value {
        match self {
            Self::Passthrough { body, .. }
            | Self::Semantic { body, .. }
            | Self::Shadow { body, .. }
            | Self::FallbackPreSend { body, .. } => body,
            Self::Active { request, .. } => request.body(),
        }
    }

    pub fn into_body(self) -> Value {
        match self {
            Self::Passthrough { body, .. }
            | Self::Semantic { body, .. }
            | Self::Shadow { body, .. }
            | Self::FallbackPreSend { body, .. } => body,
            Self::Active { request, .. } => request.into_body(),
        }
    }

    pub fn report(&self) -> &HybridExecutionReport {
        match self {
            Self::Passthrough { report, .. }
            | Self::Semantic { report, .. }
            | Self::Shadow { report, .. }
            | Self::Active { report, .. }
            | Self::FallbackPreSend { report, .. } => report,
        }
    }

    pub fn accounting(&self) -> &HybridContextAccounting {
        match self {
            Self::Passthrough { accounting, .. }
            | Self::Semantic { accounting, .. }
            | Self::Shadow { accounting, .. }
            | Self::Active { accounting, .. }
            | Self::FallbackPreSend { accounting, .. } => accounting,
        }
    }

    pub fn accounting_mut(&mut self) -> &mut HybridContextAccounting {
        match self {
            Self::Passthrough { accounting, .. }
            | Self::Semantic { accounting, .. }
            | Self::Shadow { accounting, .. }
            | Self::Active { accounting, .. }
            | Self::FallbackPreSend { accounting, .. } => accounting,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HybridEngineError {
    PlanDeadlineExceeded,
    Cancelled,
    MissingContextIr,
    MissingGeometry,
    NoApprovedCandidate,
    Unprofitable,
    Render(RenderError),
    Patch(PatchError),
    PostValidation(PostValidationError),
    ActiveGateRejected,
}

#[derive(Clone)]
pub struct HybridContextEngine {
    renderer: DeterministicPngRenderer,
}

impl HybridContextEngine {
    pub fn new(global: &OpticalGlobalPolicy) -> Self {
        Self {
            renderer: DeterministicPngRenderer::new(
                global.max_concurrent_renders,
                Duration::from_millis(global.render_queue_timeout_ms.max(1)),
            ),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn prepare(
        &self,
        request_id: &str,
        context: &ResolvedOptimizationContext,
        compression_policy: dtoken_types::CompressionPolicy,
        original_body: Value,
        raw_body_bytes: Option<&[u8]>,
        global: &OpticalGlobalPolicy,
        route: &RouteOpticalPolicy,
        registry: &OpticalCapabilityRegistry,
        cancellation: &RenderCancellation,
    ) -> PreparedHybridRequest {
        let prepare_started = Instant::now();
        let raw_available = raw_body_bytes.is_some();
        let baseline_bytes = raw_body_bytes
            .map(|bytes| bytes.len())
            .unwrap_or_else(|| serde_json::to_vec(&original_body).unwrap_or_default().len());
        let baseline_tokens = estimate_json_tokens(&original_body);

        if !context.supports_safe_text_codec()
            || context.input_protocol != dtoken_types::Protocol::Anthropic
            || context.output_protocol != dtoken_types::Protocol::Anthropic
            || context.endpoint != dtoken_types::OpticalEndpoint::AnthropicMessages
        {
            return passthrough(
                request_id,
                original_body,
                "unsupported_or_cross_protocol_surface",
                raw_available,
                baseline_tokens,
                baseline_bytes,
            );
        }

        let applicability_input = OpticalApplicabilityInput {
            method: Some("POST".to_string()),
            endpoint_path: Some("/v1/messages".to_string()),
            body_bytes: Some(baseline_bytes),
            protocol: Some(context.input_protocol.clone()),
            provider_id: Some(context.provider_id.clone()),
            route_id: Some(context.route_id.clone()),
            model: Some(context.model.clone()),
            global_enabled: global.enabled,
            route_enabled: route.enabled,
        };
        let applicability = evaluate_applicability_with_policies_and_registry(
            &applicability_input,
            global,
            route,
            registry,
        );
        if !applicability.eligible_for_shadow {
            return semantic_or_passthrough(
                request_id,
                context,
                compression_policy,
                original_body,
                raw_available,
                baseline_tokens,
                baseline_bytes,
            );
        }

        let shadow = build_shadow_plan_detailed_with_registry(
            request_id,
            &original_body,
            global,
            route,
            &applicability_input,
            registry,
        );
        if prepare_started.elapsed() > Duration::from_millis(global.max_plan_time_ms.max(1))
            || cancellation.is_cancelled()
        {
            let reason = if cancellation.is_cancelled() {
                "plan_cancelled"
            } else {
                "plan_deadline_exceeded"
            };
            return PreparedHybridRequest::FallbackPreSend {
                body: original_body,
                report: HybridExecutionReport::empty(
                    request_id,
                    HybridPreparationKind::FallbackPreSend,
                    reason,
                    raw_available,
                ),
                accounting: base_accounting(
                    OptimizationDisposition::FallbackPreSend,
                    SentPayloadVariant::Original,
                    None,
                    Some(reason.to_string()),
                ),
            };
        }
        let resolved = resolve_model_capability_record_for_route_with_registry(
            Some(&context.output_protocol),
            Some(&context.provider_id),
            Some(&context.route_id),
            Some(&context.model),
            context.endpoint.clone(),
            registry,
        );
        let estimate = estimate_from_shadow(&shadow.summary, baseline_bytes);

        let active_requested = global.enabled
            && global.allow_active
            && route.enabled
            && route.mode == OpticalMode::ToolResultsOnly
            && route.allow_tool_result
            && route.confirmed_experimental_lossy;
        if !active_requested || !applicability.eligible_for_active {
            let mut report = HybridExecutionReport::empty(
                request_id,
                HybridPreparationKind::Shadow,
                "shadow_only",
                raw_available,
            );
            report.candidate_count = shadow.summary.counters.candidate_blocks;
            report.image_count = shadow.summary.counters.estimated_images;
            report.estimated_text_tokens_before = baseline_tokens;
            report.estimated_input_tokens_after = estimate
                .as_ref()
                .and_then(|comparison| comparison.candidate.usage.input.normalized_total)
                .unwrap_or(baseline_tokens);
            report.estimated_signed_input_savings = estimate
                .as_ref()
                .and_then(|comparison| comparison.signed_savings.input_tokens)
                .unwrap_or_default();
            report.capability_revision = resolved.revision;
            return PreparedHybridRequest::Shadow {
                body: original_body,
                accounting: base_accounting(
                    OptimizationDisposition::Shadow,
                    SentPayloadVariant::Original,
                    estimate,
                    None,
                ),
                report,
            };
        }

        match self.prepare_active(
            request_id,
            context,
            &original_body,
            baseline_bytes,
            baseline_tokens,
            raw_available,
            global,
            route,
            registry,
            resolved.revision,
            &shadow,
            cancellation,
        ) {
            Ok((request, report, comparison)) => PreparedHybridRequest::Active {
                request,
                accounting: base_accounting(
                    OptimizationDisposition::Active,
                    SentPayloadVariant::Optimized,
                    Some(comparison),
                    None,
                ),
                report,
            },
            Err(error) => {
                let reason = engine_error_code(&error).to_string();
                let report = HybridExecutionReport::empty(
                    request_id,
                    HybridPreparationKind::FallbackPreSend,
                    &reason,
                    raw_available,
                );
                PreparedHybridRequest::FallbackPreSend {
                    body: original_body,
                    accounting: base_accounting(
                        OptimizationDisposition::FallbackPreSend,
                        SentPayloadVariant::Original,
                        estimate,
                        Some(reason),
                    ),
                    report,
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn prepare_active(
        &self,
        request_id: &str,
        context: &ResolvedOptimizationContext,
        original_body: &Value,
        baseline_bytes: usize,
        baseline_tokens: u64,
        raw_available: bool,
        global: &OpticalGlobalPolicy,
        route: &RouteOpticalPolicy,
        registry: &OpticalCapabilityRegistry,
        capability_revision: Option<u64>,
        shadow: &super::decision::ShadowPlanBuild,
        cancellation: &RenderCancellation,
    ) -> Result<
        (
            ValidatedActiveRequest,
            HybridExecutionReport,
            UsageComparison,
        ),
        HybridEngineError,
    > {
        let plan_started = Instant::now();
        if cancellation.is_cancelled() {
            return Err(HybridEngineError::Cancelled);
        }
        let plan_deadline = Duration::from_millis(global.max_plan_time_ms.max(1));
        let ir = lower_hybrid_context(context, original_body)
            .ok_or(HybridEngineError::MissingContextIr)?;
        if ir.approved_text_targets().is_empty() {
            return Err(HybridEngineError::NoApprovedCandidate);
        }
        if plan_started.elapsed() > plan_deadline {
            return Err(HybridEngineError::PlanDeadlineExceeded);
        }

        let resolved = resolve_model_capability_record_for_route_with_registry(
            Some(&context.output_protocol),
            Some(&context.provider_id),
            Some(&context.route_id),
            Some(&context.model),
            context.endpoint.clone(),
            registry,
        );
        let geometry = geometry_for_capability(&resolved.capability)
            .ok_or(HybridEngineError::MissingGeometry)?;
        let limits = RenderLimits::from_policy(global);
        let mut operations = Vec::new();
        let mut source_chars = 0usize;
        let mut capsule_digests = Vec::new();
        let mut total_images = 0usize;

        for target in ir.approved_text_targets() {
            if cancellation.is_cancelled() {
                return Err(HybridEngineError::Cancelled);
            }
            let Some(text) = original_body
                .pointer(target.patch_path())
                .and_then(Value::as_str)
            else {
                continue;
            };
            let chars = text.chars().count();
            if chars < MIN_ACTIVE_CANDIDATE_CHARS {
                continue;
            }
            source_chars = source_chars.saturating_add(chars);
            if source_chars > global.max_candidate_chars_per_request {
                return Err(HybridEngineError::Render(RenderError::CandidateTooLarge));
            }
            let artifact = self
                .renderer
                .render(text, &geometry, &limits, cancellation)
                .map_err(HybridEngineError::Render)?;
            total_images = total_images.saturating_add(artifact.pages().len());
            if total_images > global.max_images_per_request {
                return Err(HybridEngineError::Render(RenderError::PageBudgetExceeded));
            }
            capsule_digests.push(capsule_digest(
                context,
                target.node_id(),
                artifact.source_digest(),
                artifact.profile_digest(),
            ));
            operations.push(
                OpticalPatchOperation::from_rendered(target, artifact)
                    .map_err(HybridEngineError::Patch)?,
            );
        }
        if operations.is_empty() {
            return Err(HybridEngineError::NoApprovedCandidate);
        }

        let patch_set =
            OpticalPatchSet::from_operations(operations).map_err(HybridEngineError::Patch)?;
        let total_pixels = u64::try_from(geometry.width_px)
            .unwrap_or(u64::MAX)
            .saturating_mul(u64::try_from(geometry.max_height_px).unwrap_or(u64::MAX))
            .saturating_mul(u64::try_from(patch_set.image_count()).unwrap_or(u64::MAX));
        if total_pixels > global.max_total_pixels {
            return Err(HybridEngineError::Render(RenderError::PixelBudgetExceeded));
        }
        if patch_set.png_bytes() > global.max_png_bytes {
            return Err(HybridEngineError::Render(RenderError::PngBudgetExceeded));
        }
        if patch_set.base64_bytes() > global.max_base64_bytes {
            return Err(HybridEngineError::Render(RenderError::Base64BudgetExceeded));
        }
        if total_pixels
            .saturating_add(patch_set.png_bytes())
            .saturating_add(patch_set.base64_bytes())
            > global.max_memory_bytes
        {
            return Err(HybridEngineError::Render(RenderError::MemoryBudgetExceeded));
        }
        let image_tokens = u64::try_from(estimate_image_tokens(&geometry, patch_set.image_count()))
            .unwrap_or(u64::MAX);
        let candidate_text_tokens = baseline_tokens
            .saturating_sub(u64::try_from(source_chars.div_ceil(4)).unwrap_or(u64::MAX));
        let candidate_input_tokens = candidate_text_tokens
            .saturating_add(image_tokens)
            .saturating_add(
                u64::try_from(patch_set.image_count())
                    .unwrap_or(u64::MAX)
                    .saturating_mul(SIDECAR_TOKENS_PER_PAGE),
            );
        let signed_savings = i128::from(baseline_tokens) - i128::from(candidate_input_tokens);
        if signed_savings < route.min_net_saved_tokens as i128 {
            return Err(HybridEngineError::Unprofitable);
        }

        let patched = patch_set
            .apply(original_body.clone())
            .map_err(HybridEngineError::Patch)?;
        let patched_bytes = serde_json::to_vec(patched.body()).unwrap_or_default().len();
        if u64::try_from(patched_bytes).unwrap_or(u64::MAX) > global.max_outbound_body_bytes {
            return Err(HybridEngineError::Render(RenderError::Base64BudgetExceeded));
        }
        let peak_validation_memory = total_pixels
            .saturating_add(patch_set.png_bytes().saturating_mul(2))
            .saturating_add(patch_set.base64_bytes())
            .saturating_add(u64::try_from(patched_bytes).unwrap_or(u64::MAX));
        if peak_validation_memory > global.max_memory_bytes {
            return Err(HybridEngineError::Render(RenderError::MemoryBudgetExceeded));
        }
        let comparison = estimated_comparison(
            baseline_tokens,
            candidate_input_tokens,
            candidate_text_tokens,
            image_tokens,
            baseline_bytes,
            patched_bytes,
            patch_set.png_bytes(),
            patch_set.base64_bytes(),
            geometry.profile_id.as_str(),
        );

        let shadow_validation = PlanValidationResult {
            valid: shadow.summary.final_status == OpticalPlanStatus::ValidatedShadowPlan,
            failures: if shadow.summary.final_status == OpticalPlanStatus::ValidatedShadowPlan {
                Vec::new()
            } else {
                vec!["shadow_plan_not_validated".to_string()]
            },
            mode: OpticalMode::ShadowPreview,
        };
        let active_validation = validate_active_plan_input(&ActivePlanValidationInput {
            shadow_validation: &shadow_validation,
            global_policy: global,
            route_policy: route,
            capability: &resolved.capability,
            cache_stable: true,
            secret_sidecar_duplication: false,
            patch_paths_inside_candidates: true,
            image_count: patch_set.image_count(),
            net_saved_tokens: i64::try_from(signed_savings)
                .unwrap_or(if signed_savings.is_negative() {
                    i64::MIN
                } else {
                    i64::MAX
                })
                .clamp(isize::MIN as i64, isize::MAX as i64) as isize,
        });
        if !active_validation.valid {
            return Err(HybridEngineError::ActiveGateRejected);
        }
        let validated = validate_patched_request(context, original_body, patched)
            .map_err(HybridEngineError::PostValidation)?;

        let first_artifact = validated
            .patch_set()
            .operations()
            .first()
            .map(|operation| operation.artifact());
        let mut report = HybridExecutionReport::empty(
            request_id,
            HybridPreparationKind::Active,
            "validated_active",
            raw_available,
        );
        report.candidate_count = ir.approved_text_targets().len();
        report.patched_count = validated.report().approved_patch_count;
        report.image_count = validated.report().image_count;
        report.source_chars = source_chars;
        report.png_bytes = validated.patch_set().png_bytes();
        report.base64_bytes = validated.patch_set().base64_bytes();
        report.estimated_text_tokens_before = baseline_tokens;
        report.estimated_input_tokens_after = candidate_input_tokens;
        report.estimated_signed_input_savings =
            comparison.signed_savings.input_tokens.unwrap_or_default();
        report.capability_revision = capability_revision;
        report.profile_digest =
            first_artifact.map(|artifact| artifact.profile_digest().to_string());
        report.font_digest = first_artifact.map(|artifact| artifact.font_digest().to_string());
        report.capsule_digests = capsule_digests;
        report.artifact_validated = validated.report().artifact_evidence_derived;
        Ok((validated, report, comparison))
    }
}

fn passthrough(
    request_id: &str,
    body: Value,
    reason: &str,
    raw_available: bool,
    baseline_tokens: u64,
    baseline_bytes: usize,
) -> PreparedHybridRequest {
    let mut report = HybridExecutionReport::empty(
        request_id,
        HybridPreparationKind::Passthrough,
        reason,
        raw_available,
    );
    report.estimated_text_tokens_before = baseline_tokens;
    report.estimated_input_tokens_after = baseline_tokens;
    PreparedHybridRequest::Passthrough {
        body,
        report,
        accounting: base_accounting(
            OptimizationDisposition::Passthrough,
            SentPayloadVariant::Original,
            Some(estimated_comparison(
                baseline_tokens,
                baseline_tokens,
                baseline_tokens,
                0,
                baseline_bytes,
                baseline_bytes,
                0,
                0,
                "passthrough",
            )),
            None,
        ),
    }
}

fn semantic_or_passthrough(
    request_id: &str,
    context: &ResolvedOptimizationContext,
    policy: dtoken_types::CompressionPolicy,
    body: Value,
    raw_available: bool,
    baseline_tokens: u64,
    baseline_bytes: usize,
) -> PreparedHybridRequest {
    let result = crate::CompressionEngine::new()
        .compress_resolved(context, policy, body.clone())
        .ok();
    let Some(result) = result.filter(|result| {
        result.compressed_tokens < result.original_tokens && !result.transforms_applied.is_empty()
    }) else {
        return passthrough(
            request_id,
            body,
            "no_profitable_safe_semantic_candidate",
            raw_available,
            baseline_tokens,
            baseline_bytes,
        );
    };
    let candidate_bytes = serde_json::to_vec(&result.body).unwrap_or_default().len();
    let comparison = estimated_comparison(
        u64::try_from(result.original_tokens).unwrap_or(u64::MAX),
        u64::try_from(result.compressed_tokens).unwrap_or(u64::MAX),
        u64::try_from(result.compressed_tokens).unwrap_or(u64::MAX),
        0,
        baseline_bytes,
        candidate_bytes,
        0,
        0,
        "semantic-safe-codec-v1",
    );
    let mut report = HybridExecutionReport::empty(
        request_id,
        HybridPreparationKind::Semantic,
        "safe_semantic_codec",
        raw_available,
    );
    report.estimated_text_tokens_before = u64::try_from(result.original_tokens).unwrap_or(u64::MAX);
    report.estimated_input_tokens_after =
        u64::try_from(result.compressed_tokens).unwrap_or(u64::MAX);
    report.estimated_signed_input_savings =
        comparison.signed_savings.input_tokens.unwrap_or_default();
    report.transforms_applied = result.transforms_applied;
    PreparedHybridRequest::Semantic {
        body: result.body,
        report,
        accounting: base_accounting(
            OptimizationDisposition::SemanticActive,
            SentPayloadVariant::Optimized,
            Some(comparison),
            None,
        ),
    }
}

fn base_accounting(
    disposition: OptimizationDisposition,
    sent_payload: SentPayloadVariant,
    estimated: Option<UsageComparison>,
    fallback_reason_code: Option<String>,
) -> HybridContextAccounting {
    HybridContextAccounting {
        schema_version: 1,
        disposition,
        sent_payload,
        upstream_state: UpstreamSendState::NotAttempted,
        estimated,
        provider_counterfactual_baseline: None,
        provider_actual_sent: ProviderActualUsage::default(),
        provider_reconciled_input_savings: None,
        paired_actual: None,
        attribution_basis: SavingsAttribution::None,
        fallback_reason_code,
    }
}

fn estimate_from_shadow(
    summary: &dtoken_types::OpticalPlanPublicSummary,
    baseline_bytes: usize,
) -> Option<UsageComparison> {
    if summary.economics.text_tokens_before == 0 {
        return None;
    }
    let baseline = u64::try_from(summary.economics.text_tokens_before).ok()?;
    let candidate = u64::try_from(
        summary
            .economics
            .image_tokens_after
            .saturating_add(summary.economics.sidecar_tokens)
            .saturating_add(summary.economics.cache_burn_tokens),
    )
    .ok()?;
    Some(estimated_comparison(
        baseline,
        candidate,
        0,
        u64::try_from(summary.economics.image_tokens_after).unwrap_or(u64::MAX),
        baseline_bytes,
        baseline_bytes,
        0,
        0,
        "shadow-estimate",
    ))
}

#[allow(clippy::too_many_arguments)]
fn estimated_comparison(
    baseline_tokens: u64,
    candidate_tokens: u64,
    candidate_text_tokens: u64,
    candidate_vision_tokens: u64,
    baseline_bytes: usize,
    candidate_bytes: usize,
    png_bytes: u64,
    base64_bytes: u64,
    source_revision: &str,
) -> UsageComparison {
    let measurement =
        |tokens, text_tokens, vision_tokens, request_bytes, png, base64| UsageMeasurement {
            evidence: UsageEvidence {
                kind: UsageEvidenceKind::LocalEstimate,
                source_revision: source_revision.to_string(),
            },
            usage: UsageVector {
                input: InputTokenUsage {
                    normalized_total: Some(tokens),
                    modality: ModalityTokens {
                        text: Some(text_tokens),
                        vision: (vision_tokens > 0).then_some(vision_tokens),
                        other: tokens
                            .saturating_sub(text_tokens.saturating_add(vision_tokens))
                            .gt(&0)
                            .then_some(
                                tokens.saturating_sub(text_tokens.saturating_add(vision_tokens)),
                            ),
                    },
                    cache: CacheTokens {
                        uncached: Some(tokens),
                        ..Default::default()
                    },
                },
                output_tokens: None,
                bytes: PayloadByteUsage {
                    request_json: Some(u64::try_from(request_bytes).unwrap_or(u64::MAX)),
                    png_binary: Some(png),
                    base64_encoded: Some(base64),
                    wire_body: Some(u64::try_from(request_bytes).unwrap_or(u64::MAX)),
                },
                cost_microusd: None,
            },
        };
    UsageComparison::from_measurements(
        measurement(baseline_tokens, baseline_tokens, 0, baseline_bytes, 0, 0),
        measurement(
            candidate_tokens,
            candidate_text_tokens,
            candidate_vision_tokens,
            candidate_bytes,
            png_bytes,
            base64_bytes,
        ),
    )
}

fn estimate_json_tokens(body: &Value) -> u64 {
    let chars = serde_json::to_string(body)
        .unwrap_or_default()
        .chars()
        .count();
    u64::try_from(chars.div_ceil(4)).unwrap_or(u64::MAX)
}

fn capsule_digest(
    context: &ResolvedOptimizationContext,
    node_id: &str,
    source_digest: &str,
    profile_digest: &str,
) -> String {
    format!(
        "{:x}",
        Sha256::digest(
            format!(
                "capsule-v1|{}|{}|{}|{}|{}|{}",
                context.route_id,
                context.provider_id,
                context.model,
                node_id,
                source_digest,
                profile_digest
            )
            .as_bytes()
        )
    )
}

fn engine_error_code(error: &HybridEngineError) -> &'static str {
    match error {
        HybridEngineError::PlanDeadlineExceeded => "plan_deadline_exceeded",
        HybridEngineError::Cancelled => "render_cancelled",
        HybridEngineError::MissingContextIr => "context_ir_unavailable",
        HybridEngineError::MissingGeometry => "render_geometry_unavailable",
        HybridEngineError::NoApprovedCandidate => "no_approved_candidate",
        HybridEngineError::Unprofitable => "estimated_savings_below_threshold",
        HybridEngineError::Render(RenderError::Cancelled) => "render_cancelled",
        HybridEngineError::Render(RenderError::DeadlineExceeded) => "render_deadline_exceeded",
        HybridEngineError::Render(RenderError::QueueTimeout) => "render_queue_timeout",
        HybridEngineError::Render(_) => "render_failed_closed",
        HybridEngineError::Patch(_) => "patch_failed_closed",
        HybridEngineError::PostValidation(_) => "post_validation_failed_closed",
        HybridEngineError::ActiveGateRejected => "active_gate_rejected",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::optical::capability::experimental_anthropic_active_snapshot_for_route;
    use dtoken_types::{CompressionPolicy, OpticalEndpoint, Protocol};
    use serde_json::json;

    fn context() -> ResolvedOptimizationContext {
        ResolvedOptimizationContext {
            route_id: "route-main".to_string(),
            provider_id: "anthropic-main".to_string(),
            model: "claude-fable-5".to_string(),
            input_protocol: Protocol::Anthropic,
            output_protocol: Protocol::Anthropic,
            endpoint: OpticalEndpoint::AnthropicMessages,
        }
    }

    fn request_body() -> Value {
        let dense_line = format!("INFO {}\n", "historical operation completed ".repeat(16));
        let log = dense_line.repeat(200);
        json!({
            "model": "claude-fable-5",
            "system": "protected system instruction",
            "tools": [{"name":"read","input_schema":{"type":"object"}}],
            "messages": [
                {"role":"assistant","content":[{"type":"tool_use","id":"tool-1","name":"read","input":{"path":"/tmp/a"}}]},
                {"role":"user","content":[{"type":"tool_result","tool_use_id":"tool-1","content":log}]},
                {"role":"assistant","content":[{"type":"text","text":"done"}]},
                {"role":"user","content":[{"type":"text","text":"next task"}]},
                {"role":"assistant","content":[{"type":"text","text":"working"}]},
                {"role":"user","content":[{"type":"text","text":"current turn"}]}
            ]
        })
    }

    fn active_policies() -> (OpticalGlobalPolicy, RouteOpticalPolicy) {
        (
            OpticalGlobalPolicy {
                enabled: true,
                allow_active: true,
                max_plan_time_ms: 500,
                max_render_time_ms: 5_000,
                ..Default::default()
            },
            RouteOpticalPolicy {
                enabled: true,
                mode: OpticalMode::ToolResultsOnly,
                allow_tool_result: true,
                min_net_saved_tokens: 1,
                confirmed_experimental_lossy: true,
                ..Default::default()
            },
        )
    }

    fn active_registry() -> OpticalCapabilityRegistry {
        OpticalCapabilityRegistry::from_snapshots([
            experimental_anthropic_active_snapshot_for_route(
                "engine-fixture-v1",
                1,
                "anthropic-main",
                "route-main",
                "claude-fable-5",
                100,
                "engine-fixture",
            ),
        ])
    }

    #[test]
    fn shadow_and_cancelled_fallback_preserve_the_original_request() {
        let body = request_body();
        let raw = serde_json::to_vec(&body).expect("fixture serializes");
        let shadow_global = OpticalGlobalPolicy {
            enabled: true,
            max_plan_time_ms: 5_000,
            ..Default::default()
        };
        let shadow_route = RouteOpticalPolicy {
            enabled: true,
            mode: OpticalMode::ShadowPreview,
            allow_tool_result: true,
            ..Default::default()
        };
        let shadow = HybridContextEngine::new(&shadow_global).prepare(
            "shadow-request",
            &context(),
            CompressionPolicy::Balanced,
            body.clone(),
            Some(&raw),
            &shadow_global,
            &shadow_route,
            &OpticalCapabilityRegistry::default(),
            &RenderCancellation::default(),
        );
        assert_eq!(shadow.kind(), HybridPreparationKind::Shadow);
        assert_eq!(shadow.body(), &body);
        assert!(shadow.report().raw_available);
        assert_eq!(
            shadow.accounting().sent_payload,
            SentPayloadVariant::Original
        );

        let (active_global, active_route) = active_policies();
        let cancellation = RenderCancellation::default();
        cancellation.cancel();
        let fallback = HybridContextEngine::new(&active_global).prepare(
            "cancelled-request",
            &context(),
            CompressionPolicy::Balanced,
            body.clone(),
            Some(&raw),
            &active_global,
            &active_route,
            &active_registry(),
            &cancellation,
        );
        assert_eq!(fallback.kind(), HybridPreparationKind::FallbackPreSend);
        assert_eq!(fallback.body(), &body);
        assert_eq!(fallback.report().reason_code, "plan_cancelled");
        assert_eq!(
            fallback.accounting().disposition,
            OptimizationDisposition::FallbackPreSend
        );
        assert_eq!(
            fallback.accounting().sent_payload,
            SentPayloadVariant::Original
        );
    }

    #[test]
    fn active_engine_emits_validated_images_and_preserves_protected_fields() {
        let body = request_body();
        let raw = serde_json::to_vec(&body).expect("fixture serializes");
        let (global, route) = active_policies();
        let prepared = HybridContextEngine::new(&global).prepare(
            "active-request",
            &context(),
            CompressionPolicy::Balanced,
            body.clone(),
            Some(&raw),
            &global,
            &route,
            &active_registry(),
            &RenderCancellation::default(),
        );

        assert_eq!(prepared.kind(), HybridPreparationKind::Active);
        assert_eq!(prepared.body()["model"], body["model"]);
        assert_eq!(prepared.body()["system"], body["system"]);
        assert_eq!(prepared.body()["tools"], body["tools"]);
        assert_eq!(prepared.body()["messages"][0], body["messages"][0]);
        let patched_content = prepared
            .body()
            .pointer("/messages/1/content/0/content")
            .and_then(Value::as_array)
            .expect("approved tool result is replaced by typed content blocks");
        assert_eq!(patched_content[0]["type"], "text");
        assert_eq!(patched_content[1]["type"], "image");
        assert!(prepared.report().artifact_validated);
        assert!(prepared.report().image_count > 0);
        assert!(prepared.report().png_bytes > 0);
        assert!(prepared.report().base64_bytes > prepared.report().png_bytes);
        assert_eq!(prepared.report().capability_revision, Some(1));
        assert_eq!(
            prepared.accounting().disposition,
            OptimizationDisposition::Active
        );
        assert_eq!(
            prepared.accounting().sent_payload,
            SentPayloadVariant::Optimized
        );
        assert!(prepared.report().estimated_signed_input_savings > 0);
    }

    #[test]
    fn appended_turns_keep_existing_optical_capsule_bytes_stable() {
        let original = request_body();
        let mut appended = original.clone();
        appended["messages"]
            .as_array_mut()
            .expect("fixture messages")
            .extend([
                json!({"role":"assistant","content":[{"type":"text","text":"follow-up"}]}),
                json!({"role":"user","content":[{"type":"text","text":"new current turn"}]}),
            ]);
        let (global, route) = active_policies();
        let engine = HybridContextEngine::new(&global);
        let first = engine.prepare(
            "prefix-first",
            &context(),
            CompressionPolicy::Balanced,
            original.clone(),
            Some(&serde_json::to_vec(&original).expect("first raw body")),
            &global,
            &route,
            &active_registry(),
            &RenderCancellation::default(),
        );
        let second = engine.prepare(
            "prefix-appended",
            &context(),
            CompressionPolicy::Balanced,
            appended.clone(),
            Some(&serde_json::to_vec(&appended).expect("appended raw body")),
            &global,
            &route,
            &active_registry(),
            &RenderCancellation::default(),
        );

        assert_eq!(first.kind(), HybridPreparationKind::Active);
        assert_eq!(second.kind(), HybridPreparationKind::Active);
        assert_eq!(
            first.body().pointer("/messages/1/content/0/content"),
            second.body().pointer("/messages/1/content/0/content")
        );
        assert_eq!(
            first.report().capsule_digests,
            second.report().capsule_digests
        );
        assert_eq!(
            &first.body()["messages"].as_array().expect("first messages")[..6],
            &second.body()["messages"]
                .as_array()
                .expect("second messages")[..6]
        );
    }
}
