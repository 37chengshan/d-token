use std::collections::HashSet;
use std::time::{Duration, Instant};

use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::{lower_hybrid_context, ApprovedHybridAction, ResolvedOptimizationContext};

use super::applicability::evaluate_applicability_with_policies_and_registry;
use super::capability::{
    resolve_model_capability_record_for_route_with_registry, OpticalCapabilityRegistry,
};
use super::decision::build_shadow_plan_detailed_with_registry;
use super::economics::{
    economics_contract_from_attested, evaluate_materialized_capsule, CacheEvidenceState,
};
use super::history_collapse::plan_history_collapse;
use super::patcher::{OpticalPatchOperation, OpticalPatchSet, PatchError};
use super::post_validate::{validate_patched_request, PostValidationError, ValidatedActiveRequest};
use super::render_geometry::{
    estimate_image_tokens, estimate_image_tokens_for_dimensions, geometry_for_capability,
    images_required,
};
use super::renderer::{
    production_render_contract, DeterministicPngRenderer, RenderBytePermitPool, RenderCancellation,
    RenderError, RenderLimits,
};
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HybridPlanV2 {
    pub selected_node_ids: Vec<String>,
    pub selected_chars: usize,
    pub estimated_images: usize,
    pub estimated_vision_tokens: u64,
    pub rejected_by_budget: usize,
}

fn selected_targets_cache_stable(stability: &[bool]) -> bool {
    !stability.is_empty() && stability.iter().all(|stable| *stable)
}

fn active_cache_assumption(
    _cache_evidence_present: bool,
    prior_prefix_tokens: u64,
) -> (CacheEvidenceState, f64) {
    // A configured cache policy is not proof that this request is warm, and
    // missing telemetry is not proof that it is cold. Active economics always
    // prices the unobserved state conservatively as the maximum cache burn.
    (CacheEvidenceState::Unknown, prior_prefix_tokens as f64)
}

fn remaining_request_budget(
    request_started: Instant,
    request_budget: Duration,
) -> Result<Duration, HybridEngineError> {
    request_budget
        .checked_sub(request_started.elapsed())
        .filter(|remaining| !remaining.is_zero())
        .ok_or(HybridEngineError::PlanDeadlineExceeded)
}

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
    pub safe_text_candidate_count: usize,
    pub optical_candidate_count: usize,
    pub cache_stable_candidate_count: usize,
    pub exact_preserved_count: usize,
    pub decision_preserved_count: usize,
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
    /// Set when the history-collapse planner scoped the active candidates:
    /// the byte-stable static slab / freeze grid / cache anchor / billing
    /// boundary plan digest (PR-C5B). `None` means no collapse scope was
    /// applied and no frozen history was collapsed.
    pub history_collapse_digest: Option<String>,
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
            safe_text_candidate_count: 0,
            optical_candidate_count: 0,
            cache_stable_candidate_count: 0,
            exact_preserved_count: 0,
            decision_preserved_count: 0,
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
            history_collapse_digest: None,
            artifact_validated: false,
            raw_available,
            transforms_applied: Vec::new(),
        }
    }
}

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
    MissingEconomicsContract,
    NoApprovedCandidate,
    Unprofitable,
    Render(RenderError),
    Patch(PatchError),
    PostValidation(PostValidationError),
    ActiveGateRejected,
}

fn select_hybrid_plan_v2(
    ir: &crate::HybridContextIr,
    body: &Value,
    geometry: &super::render_geometry::RenderGeometryProfile,
    global: &OpticalGlobalPolicy,
    chars_per_text_token: f64,
    collapse_scope: Option<&HashSet<String>>,
) -> HybridPlanV2 {
    let mut candidates = ir
        .approved_text_targets()
        .iter()
        .filter(|target| {
            target.action() == ApprovedHybridAction::OpticalImage && target.cache_stable()
        })
        .filter(|target| {
            // PR-C5B: when history collapse is enabled, only blocks the
            // planner approved (frozen history below the recent tail, outside
            // the static slab, freeze grid and billing boundary) may collapse.
            // A missing/empty scope means nothing may collapse.
            collapse_scope.is_none_or(|scope| scope.contains(target.node_id()))
        })
        .filter_map(|target| {
            let text = body.pointer(target.patch_path()).and_then(Value::as_str)?;
            let chars = text.chars().count();
            if chars < MIN_ACTIVE_CANDIDATE_CHARS {
                return None;
            }
            let images = images_required(geometry, chars)?;
            let vision_tokens = estimate_image_tokens(geometry, images);
            let text_tokens = ((chars as f64) / chars_per_text_token)
                .ceil()
                .min(usize::MAX as f64) as usize;
            let gross_savings = text_tokens.saturating_sub(vision_tokens);
            Some((
                target.node_id(),
                chars,
                images,
                vision_tokens,
                gross_savings,
            ))
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| right.4.cmp(&left.4).then_with(|| left.0.cmp(right.0)));

    let mut plan = HybridPlanV2 {
        selected_node_ids: Vec::new(),
        selected_chars: 0,
        estimated_images: 0,
        estimated_vision_tokens: 0,
        rejected_by_budget: 0,
    };
    for (node_id, chars, images, vision_tokens, gross_savings) in candidates {
        let within_chars =
            plan.selected_chars.saturating_add(chars) <= global.max_candidate_chars_per_request;
        let within_images =
            plan.estimated_images.saturating_add(images) <= global.max_images_per_request;
        if gross_savings == 0 || !within_chars || !within_images {
            plan.rejected_by_budget = plan.rejected_by_budget.saturating_add(1);
            continue;
        }
        plan.selected_node_ids.push(node_id.to_string());
        plan.selected_chars = plan.selected_chars.saturating_add(chars);
        plan.estimated_images = plan.estimated_images.saturating_add(images);
        plan.estimated_vision_tokens = plan
            .estimated_vision_tokens
            .saturating_add(u64::try_from(vision_tokens).unwrap_or(u64::MAX));
    }
    plan
}

#[derive(Clone)]
pub struct HybridContextEngine {
    renderer: DeterministicPngRenderer,
    request_bytes: RenderBytePermitPool,
    queue_timeout: Duration,
}

impl HybridContextEngine {
    pub fn new(global: &OpticalGlobalPolicy) -> Self {
        let queue_timeout = Duration::from_millis(global.render_queue_timeout_ms.max(1));
        let request_bytes = RenderBytePermitPool::new(global.max_memory_bytes);
        Self {
            renderer: DeterministicPngRenderer::new_with_memory_pool(
                global.max_concurrent_renders,
                queue_timeout,
                request_bytes.clone(),
            ),
            request_bytes,
            queue_timeout,
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
        self.prepare_with_cache_evidence(
            request_id,
            context,
            compression_policy,
            original_body,
            raw_body_bytes,
            global,
            route,
            registry,
            cancellation,
            None,
            None,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn prepare_with_cache_evidence(
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
        cache_evidence: Option<&crate::context_pipeline::ProviderCacheEvidence>,
        frozen_message_count: Option<usize>,
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
                cache_evidence,
                frozen_message_count,
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
            apply_shadow_evidence(&mut report, &shadow);
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
            prepare_started,
            cache_evidence,
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
        request_started: Instant,
        cache_evidence: Option<&crate::context_pipeline::ProviderCacheEvidence>,
    ) -> Result<
        (
            ValidatedActiveRequest,
            HybridExecutionReport,
            UsageComparison,
        ),
        HybridEngineError,
    > {
        if cancellation.is_cancelled() {
            return Err(HybridEngineError::Cancelled);
        }
        let plan_deadline = Duration::from_millis(global.max_plan_time_ms.max(1));
        let ir = lower_hybrid_context(context, original_body)
            .ok_or(HybridEngineError::MissingContextIr)?;
        if !ir.approved_text_targets().iter().any(|target| {
            target.action() == ApprovedHybridAction::OpticalImage && target.cache_stable()
        }) {
            return Err(HybridEngineError::NoApprovedCandidate);
        }
        remaining_request_budget(request_started, plan_deadline)?;

        let resolved = resolve_model_capability_record_for_route_with_registry(
            Some(&context.output_protocol),
            Some(&context.provider_id),
            Some(&context.route_id),
            Some(&context.model),
            context.endpoint.clone(),
            registry,
        );
        // Active economics must come from the signature-bound snapshot; a
        // wildcard, unattested or malformed economics contract stays
        // fail-closed (PR-C5C).
        let economics_contract = resolved
            .economics
            .as_ref()
            .and_then(economics_contract_from_attested)
            .ok_or(HybridEngineError::MissingEconomicsContract)?;
        let geometry = geometry_for_capability(&resolved.capability)
            .ok_or(HybridEngineError::MissingGeometry)?;
        let render_contract = production_render_contract(&geometry);
        // PR-C5B: history collapse is scoped by the d-token-owned planner.
        // When the route allows it, the active candidate set is restricted to
        // the planner's approved frozen-history blocks; a plan that fails its
        // own integrity (freeze grid not byte-stable, protected text not
        // native) collapses nothing (empty scope) so the request preserves
        // original bytes. `None` keeps the legacy cache-stable selection.
        let (collapse_scope, history_collapse_digest) = if route.allow_history {
            match plan_history_collapse(&ir, original_body, &render_contract.geometry, route) {
                Some(plan)
                    if plan.freeze_grid.byte_stable
                        && plan.current_recent_open_exact_secret_native_text =>
                {
                    let scope = plan
                        .collapse_candidates
                        .iter()
                        .map(|candidate| candidate.block_id.clone())
                        .collect::<HashSet<_>>();
                    (Some(scope), Some(plan.digest().to_string()))
                }
                _ => (Some(HashSet::new()), None),
            }
        } else {
            (None, None)
        };
        let hybrid_plan = select_hybrid_plan_v2(
            &ir,
            original_body,
            &render_contract.geometry,
            global,
            economics_contract.chars_per_text_token,
            collapse_scope.as_ref(),
        );
        if hybrid_plan.selected_node_ids.is_empty() {
            return Err(HybridEngineError::NoApprovedCandidate);
        }
        let base_limits = RenderLimits::from_policy(global);
        let mut operations = Vec::new();
        let mut source_chars = 0usize;
        let mut capsule_digests = Vec::new();
        let mut total_images = 0usize;
        let mut selected_cache_stability = Vec::new();

        for target in ir.approved_text_targets().iter().filter(|target| {
            hybrid_plan
                .selected_node_ids
                .iter()
                .any(|node_id| node_id == target.node_id())
        }) {
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
            let remaining_deadline = remaining_request_budget(request_started, plan_deadline)?;
            let mut limits = base_limits.clone();
            limits.deadline = limits.deadline.min(remaining_deadline);
            let artifact = self
                .renderer
                .render_with_contract(text, &render_contract, &limits, cancellation)
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
            selected_cache_stability.push(target.cache_stable());
            operations.push(
                OpticalPatchOperation::from_rendered(target, artifact, text)
                    .map_err(HybridEngineError::Patch)?,
            );
        }
        if operations.is_empty() {
            return Err(HybridEngineError::NoApprovedCandidate);
        }

        let patch_set =
            OpticalPatchSet::from_operations(operations).map_err(HybridEngineError::Patch)?;
        let total_pixels = patch_set.total_pixels();
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
        let image_tokens = patch_set
            .operations()
            .iter()
            .flat_map(|operation| operation.artifact().pages())
            .map(|page| {
                estimate_image_tokens_for_dimensions(
                    &geometry,
                    usize::try_from(page.width()).unwrap_or(usize::MAX),
                    usize::try_from(page.height()).unwrap_or(usize::MAX),
                )
            })
            .try_fold(0u64, |total, tokens| {
                total.checked_add(u64::try_from(tokens).unwrap_or(u64::MAX))
            })
            .unwrap_or(u64::MAX);
        let source_text_tokens = ((source_chars as f64) / economics_contract.chars_per_text_token)
            .ceil()
            .min(u64::MAX as f64) as u64;
        let candidate_text_tokens = baseline_tokens.saturating_sub(source_text_tokens);
        let sidecar_tokens = patch_set.estimated_sidecar_tokens();
        let candidate_input_tokens = candidate_text_tokens
            .saturating_add(image_tokens)
            .saturating_add(sidecar_tokens);
        let signed_savings = i128::from(baseline_tokens) - i128::from(candidate_input_tokens);
        if signed_savings < route.min_net_saved_tokens as i128 {
            return Err(HybridEngineError::Unprofitable);
        }
        let (cache_state, prior_prefix_tokens) =
            active_cache_assumption(cache_evidence.is_some(), candidate_text_tokens);
        let materialized_cost = evaluate_materialized_capsule(
            source_text_tokens as f64,
            image_tokens as f64,
            sidecar_tokens as f64,
            prior_prefix_tokens,
            economics_contract.cache_create_rate,
            economics_contract.cache_read_rate,
            cache_state,
        );
        let conservative_total =
            candidate_text_tokens as f64 + materialized_cost.conservative_input_tokens;
        if !conservative_total.is_finite() || conservative_total >= baseline_tokens as f64 {
            return Err(HybridEngineError::Unprofitable);
        }

        let patched = patch_set
            .apply(original_body.clone())
            .map_err(HybridEngineError::Patch)?;
        remaining_request_budget(request_started, plan_deadline)?;
        let patched_bytes = serde_json::to_vec(patched.body()).unwrap_or_default().len();
        if u64::try_from(patched_bytes).unwrap_or(u64::MAX) > global.max_outbound_body_bytes {
            return Err(HybridEngineError::Render(RenderError::Base64BudgetExceeded));
        }
        let duplicated_patched_bytes = u64::try_from(patched_bytes)
            .unwrap_or(u64::MAX)
            .saturating_mul(2);
        let retained_request_memory = patched
            .patch_set()
            .png_bytes()
            .saturating_add(patched.patch_set().base64_bytes())
            .saturating_add(duplicated_patched_bytes);
        let peak_validation_memory = patched
            .patch_set()
            .png_bytes()
            .saturating_add(retained_request_memory);
        if peak_validation_memory > global.max_memory_bytes {
            return Err(HybridEngineError::Render(RenderError::MemoryBudgetExceeded));
        }
        // Rendered artifacts retain their PNG leases from the same global pool.
        // Reserve the remaining validation/send footprint, including the owned
        // patched body and the transport copy created by the core adapter.
        let request_memory = self
            .request_bytes
            .acquire(retained_request_memory.max(1), self.queue_timeout)
            .map_err(HybridEngineError::Render)?;
        let comparison = estimated_comparison(
            baseline_tokens,
            candidate_input_tokens,
            candidate_text_tokens,
            image_tokens,
            baseline_bytes,
            patched_bytes,
            patched.patch_set().png_bytes(),
            patched.patch_set().base64_bytes(),
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
            cache_stable: selected_targets_cache_stable(&selected_cache_stability),
            secret_sidecar_duplication: false,
            patch_paths_inside_candidates: true,
            operation_page_counts: &patched
                .patch_set()
                .operations()
                .iter()
                .map(|operation| operation.artifact().pages().len())
                .collect::<Vec<_>>(),
            image_count: patched.patch_set().image_count(),
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
        remaining_request_budget(request_started, plan_deadline)?;
        let validated = validate_patched_request(context, original_body, patched)
            .map_err(HybridEngineError::PostValidation)?
            .with_memory_lease(request_memory);

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
        report.candidate_count = ir
            .approved_text_targets()
            .iter()
            .filter(|target| {
                target.action() == ApprovedHybridAction::OpticalImage && target.cache_stable()
            })
            .count();
        report.safe_text_candidate_count = ir
            .representation_targets()
            .iter()
            .filter(|target| target.action() == ApprovedHybridAction::SafeTextCodec)
            .count();
        report.optical_candidate_count = ir
            .representation_targets()
            .iter()
            .filter(|target| target.action() == ApprovedHybridAction::OpticalImage)
            .count();
        report.cache_stable_candidate_count = ir
            .representation_targets()
            .iter()
            .filter(|target| target.cache_stable())
            .count();
        report.exact_preserved_count = shadow
            .candidates
            .iter()
            .filter(|candidate| {
                candidate.risk_tier == Some(crate::optical::risk::RiskTier::ExactTextRequired)
            })
            .count();
        report.decision_preserved_count = shadow
            .candidates
            .iter()
            .filter(|candidate| {
                candidate.risk_tier == Some(crate::optical::risk::RiskTier::DecisionSensitive)
            })
            .count();
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
        report.history_collapse_digest = history_collapse_digest;
        if report.history_collapse_digest.is_some() {
            report
                .transforms_applied
                .push("history_collapse_v1".to_string());
        }
        report.artifact_validated = validated.report().artifact_evidence_derived;
        Ok((validated, report, comparison))
    }
}

fn apply_shadow_evidence(
    report: &mut HybridExecutionReport,
    shadow: &super::decision::ShadowPlanBuild,
) {
    report.optical_candidate_count = shadow
        .candidates
        .iter()
        .filter(|candidate| {
            candidate.action == super::decision::CandidateAction::ShadowEstimateOnly
        })
        .count();
    report.cache_stable_candidate_count = shadow
        .candidates
        .iter()
        .filter(|candidate| candidate.cache_stable)
        .count();
    report.exact_preserved_count = shadow
        .candidates
        .iter()
        .filter(|candidate| {
            candidate.risk_tier == Some(crate::optical::risk::RiskTier::ExactTextRequired)
        })
        .count();
    report.decision_preserved_count = shadow
        .candidates
        .iter()
        .filter(|candidate| {
            candidate.risk_tier == Some(crate::optical::risk::RiskTier::DecisionSensitive)
        })
        .count();
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

#[allow(clippy::too_many_arguments)]
fn semantic_or_passthrough(
    request_id: &str,
    context: &ResolvedOptimizationContext,
    policy: dtoken_types::CompressionPolicy,
    body: Value,
    raw_available: bool,
    baseline_tokens: u64,
    baseline_bytes: usize,
    cache_evidence: Option<&crate::context_pipeline::ProviderCacheEvidence>,
    frozen_message_count: Option<usize>,
) -> PreparedHybridRequest {
    let result = crate::CompressionEngine::new()
        .compress_resolved_with_cache_evidence(
            context,
            policy,
            body.clone(),
            cache_evidence,
            frozen_message_count,
        )
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
        HybridEngineError::MissingEconomicsContract => "economics_contract_unavailable",
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
    use crate::optical::capability::{
        experimental_anthropic_active_snapshot_for_route, signed_runtime_config_fixture,
    };
    use dtoken_types::{CompressionPolicy, OpticalCapabilityLifecycle, OpticalEndpoint, Protocol};
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

    #[test]
    fn active_cache_evidence_requires_nonempty_all_stable_selection() {
        assert!(!selected_targets_cache_stable(&[]));
        assert!(!selected_targets_cache_stable(&[true, false]));
        assert!(selected_targets_cache_stable(&[true, true]));

        assert_eq!(
            active_cache_assumption(true, 500),
            (CacheEvidenceState::Unknown, 500.0)
        );
        assert_eq!(
            active_cache_assumption(false, 500),
            (CacheEvidenceState::Unknown, 500.0)
        );
        assert!(matches!(
            remaining_request_budget(
                Instant::now() - Duration::from_millis(10),
                Duration::from_millis(1)
            ),
            Err(HybridEngineError::PlanDeadlineExceeded)
        ));
    }

    fn request_body() -> Value {
        let dense_line = format!("INFO {}\n", "historical operation completed ".repeat(16));
        let log = dense_line.repeat(40);
        let mut messages = vec![
            json!({"role":"assistant","content":[{"type":"tool_use","id":"tool-1","name":"read","input":{"path":"/tmp/a"}}]}),
            json!({"role":"user","content":[{"type":"tool_result","tool_use_id":"tool-1","content":log}]}),
        ];
        for index in 0..6 {
            messages.push(json!({"role":"assistant","content":[{"type":"tool_use","id":format!("recent-{index}"),"name":"read","input":{}}]}));
            messages.push(json!({"role":"user","content":[{"type":"tool_result","tool_use_id":format!("recent-{index}"),"content":"recent"}]}));
        }
        messages.push(json!({"role":"user","content":[{"type":"text","text":"current turn"}]}));
        json!({
            "model": "claude-fable-5",
            "system": "protected system instruction",
            "tools": [{"name":"read","input_schema":{"type":"object"}}],
            "messages": messages
        })
    }

    fn active_policies() -> (OpticalGlobalPolicy, RouteOpticalPolicy) {
        (
            OpticalGlobalPolicy {
                enabled: true,
                allow_active: true,
                max_plan_time_ms: 5_000,
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

        assert_eq!(
            prepared.kind(),
            HybridPreparationKind::Active,
            "{}",
            prepared.report().reason_code
        );
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
        assert_eq!(prepared.report().optical_candidate_count, 1);
        assert_eq!(prepared.report().cache_stable_candidate_count, 1);
        assert_eq!(prepared.report().safe_text_candidate_count, 0);
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

        assert_eq!(
            first.kind(),
            HybridPreparationKind::Active,
            "{}",
            first.report().reason_code
        );
        assert_eq!(
            second.kind(),
            HybridPreparationKind::Active,
            "{}",
            second.report().reason_code
        );
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

    #[test]
    fn unsupported_protocol_never_activates_even_with_full_active_policies() {
        // OpenAI Chat with allow_active + a signed-looking fixture registry
        // must still preserve text: the Anthropic-only contract keeps every
        // unsupported-protocol surface on the passthrough path (OPTICAL-2).
        let openai_context = ResolvedOptimizationContext {
            route_id: "route-openai".to_string(),
            provider_id: "openai-main".to_string(),
            model: "gpt-5.6-sol".to_string(),
            input_protocol: dtoken_types::Protocol::OpenAi,
            output_protocol: dtoken_types::Protocol::OpenAi,
            endpoint: OpticalEndpoint::OpenAiChatCompletions,
        };
        let body = json!({
            "model": "gpt-5.6-sol",
            "messages": [
                {"role":"system","content":"instructions"},
                {"role":"user","content":"current turn"}
            ]
        });
        let raw = serde_json::to_vec(&body).expect("fixture serializes");
        let (global, route) = active_policies();
        let prepared = HybridContextEngine::new(&global).prepare(
            "openai-request",
            &openai_context,
            CompressionPolicy::Balanced,
            body.clone(),
            Some(&raw),
            &global,
            &route,
            &active_registry(),
            &RenderCancellation::default(),
        );
        assert_eq!(prepared.kind(), HybridPreparationKind::Passthrough);
        assert_eq!(prepared.body(), &body);
        assert_eq!(
            prepared.report().reason_code,
            "unsupported_or_cross_protocol_surface"
        );
        assert_eq!(
            prepared.accounting().sent_payload,
            SentPayloadVariant::Original
        );
    }

    #[test]
    fn active_text_is_preserved_until_attestation_gates_are_complete() {
        // allow_active without a signed capability must keep the original
        // body: Optical planning is evidence-only until PR-C5C attestation.
        let body = request_body();
        let raw = serde_json::to_vec(&body).expect("fixture serializes");
        let (global, route) = active_policies();
        let prepared = HybridContextEngine::new(&global).prepare(
            "unsigned-request",
            &context(),
            CompressionPolicy::Balanced,
            body.clone(),
            Some(&raw),
            &global,
            &route,
            &OpticalCapabilityRegistry::default(),
            &RenderCancellation::default(),
        );
        assert_eq!(prepared.kind(), HybridPreparationKind::Shadow);
        assert_eq!(prepared.body(), &body);
        assert_eq!(
            prepared.accounting().sent_payload,
            SentPayloadVariant::Original
        );
        assert!(prepared.report().optical_candidate_count > 0);
        assert_eq!(
            prepared.report().image_count,
            prepared.report().optical_candidate_count
        );
    }

    #[test]
    fn attested_production_registry_drives_automatic_active_without_ui_switch() {
        // PR-C5D: a config-loaded, Ed25519-attested snapshot (not the
        // from_snapshots test helper) plus green gates makes the exact direct
        // Anthropic route go automatic active. No UI switch exists.
        let config = signed_runtime_config_fixture();
        let registry = OpticalCapabilityRegistry::from_runtime_config(&config, 200);
        assert_eq!(registry.verified_rule_count(), 1);

        let body = request_body();
        let raw = serde_json::to_vec(&body).expect("fixture serializes");
        let (global, route) = active_policies();
        let prepared = HybridContextEngine::new(&global).prepare(
            "attested-active-request",
            &context(),
            CompressionPolicy::Balanced,
            body.clone(),
            Some(&raw),
            &global,
            &route,
            &registry,
            &RenderCancellation::default(),
        );
        assert_eq!(
            prepared.kind(),
            HybridPreparationKind::Active,
            "{}",
            prepared.report().reason_code
        );
        assert_eq!(prepared.report().capability_revision, Some(1));
        assert!(prepared.report().artifact_validated);
        assert_eq!(
            prepared.accounting().disposition,
            OptimizationDisposition::Active
        );
    }

    #[test]
    fn expired_attestation_preserves_text_on_the_same_request() {
        let config = signed_runtime_config_fixture();
        let registry = OpticalCapabilityRegistry::from_runtime_config(&config, 1_001);
        assert_eq!(registry.verified_rule_count(), 0);
        assert_eq!(registry.rejected_rule_count(), 1);

        let body = request_body();
        let raw = serde_json::to_vec(&body).expect("fixture serializes");
        let (global, route) = active_policies();
        let prepared = HybridContextEngine::new(&global).prepare(
            "expired-request",
            &context(),
            CompressionPolicy::Balanced,
            body.clone(),
            Some(&raw),
            &global,
            &route,
            &registry,
            &RenderCancellation::default(),
        );
        assert_eq!(prepared.kind(), HybridPreparationKind::Shadow);
        assert_eq!(prepared.body(), &body);
        assert_eq!(
            prepared.accounting().sent_payload,
            SentPayloadVariant::Original
        );
    }

    #[test]
    fn revoked_or_superseded_snapshot_preserves_text_on_the_same_request() {
        for lifecycle in [
            OpticalCapabilityLifecycle::Revoked,
            OpticalCapabilityLifecycle::Superseded,
        ] {
            let mut snapshot = experimental_anthropic_active_snapshot_for_route(
                "lifecycle-fixture",
                1,
                "anthropic-main",
                "route-main",
                "claude-fable-5",
                100,
                "engine-fixture",
            );
            snapshot.lifecycle = lifecycle;
            if lifecycle == OpticalCapabilityLifecycle::Revoked {
                snapshot.revoked_at_ms = Some(250);
                snapshot.revocation_reason = Some("fixture revocation".to_string());
            } else {
                snapshot.supersedes_snapshot_id = Some("older-snapshot".to_string());
            }
            let registry = OpticalCapabilityRegistry::from_snapshots([snapshot]);
            assert_eq!(registry.verified_rule_count(), 0);

            let body = request_body();
            let raw = serde_json::to_vec(&body).expect("fixture serializes");
            let (global, route) = active_policies();
            let prepared = HybridContextEngine::new(&global).prepare(
                "lifecycle-request",
                &context(),
                CompressionPolicy::Balanced,
                body.clone(),
                Some(&raw),
                &global,
                &route,
                &registry,
                &RenderCancellation::default(),
            );
            assert_eq!(prepared.kind(), HybridPreparationKind::Shadow);
            assert_eq!(prepared.body(), &body);
            assert_eq!(
                prepared.accounting().sent_payload,
                SentPayloadVariant::Original
            );
        }
    }

    #[test]
    fn missing_attested_economics_preserves_text_on_the_same_request() {
        let mut snapshot = experimental_anthropic_active_snapshot_for_route(
            "no-economics-fixture",
            1,
            "anthropic-main",
            "route-main",
            "claude-fable-5",
            100,
            "engine-fixture",
        );
        snapshot.economics = None;
        let registry = OpticalCapabilityRegistry::from_snapshots([snapshot]);

        let body = request_body();
        let raw = serde_json::to_vec(&body).expect("fixture serializes");
        let (global, route) = active_policies();
        let prepared = HybridContextEngine::new(&global).prepare(
            "no-economics-request",
            &context(),
            CompressionPolicy::Balanced,
            body.clone(),
            Some(&raw),
            &global,
            &route,
            &registry,
            &RenderCancellation::default(),
        );
        assert_eq!(prepared.kind(), HybridPreparationKind::FallbackPreSend);
        assert_eq!(
            prepared.report().reason_code,
            "economics_contract_unavailable"
        );
        assert_eq!(prepared.body(), &body);
        assert_eq!(
            prepared.accounting().sent_payload,
            SentPayloadVariant::Original
        );
    }

    #[test]
    fn wildcard_snapshot_identity_preserves_text_on_the_same_request() {
        for field in ["provider", "route", "model"] {
            let mut snapshot = experimental_anthropic_active_snapshot_for_route(
                "wildcard-fixture",
                1,
                "anthropic-main",
                "route-main",
                "claude-fable-5",
                100,
                "engine-fixture",
            );
            match field {
                "provider" => {
                    snapshot.provider_id = Some("*".to_string());
                }
                "route" => {
                    snapshot.route_id = Some("*".to_string());
                }
                _ => {
                    snapshot.model_base = "*".to_string();
                }
            }
            let registry = OpticalCapabilityRegistry::from_snapshots([snapshot]);
            assert_eq!(registry.verified_rule_count(), 0, "wildcard {field}");

            let body = request_body();
            let raw = serde_json::to_vec(&body).expect("fixture serializes");
            let (global, route) = active_policies();
            let prepared = HybridContextEngine::new(&global).prepare(
                "wildcard-request",
                &context(),
                CompressionPolicy::Balanced,
                body.clone(),
                Some(&raw),
                &global,
                &route,
                &registry,
                &RenderCancellation::default(),
            );
            assert_eq!(prepared.kind(), HybridPreparationKind::Shadow);
            assert_eq!(prepared.body(), &body);
            assert_eq!(
                prepared.accounting().sent_payload,
                SentPayloadVariant::Original
            );
        }
    }

    #[test]
    fn ordinary_user_image_never_creates_an_optical_render_entry() {
        // OPTICAL-2: Optical only renders approved code/context text targets
        // from the closed historical tool results. An ordinary user-supplied
        // image block must never become an Optical image entry.
        let body = json!({
            "model": "claude-fable-5",
            "system": "protected system instruction",
            "tools": [{"name":"read","input_schema":{"type":"object"}}],
            "messages": [
                {"role":"user","content":[
                    {"type":"image","source":{"type":"base64","media_type":"image/png","data":"iVBORw0KGgo="}}
                ]},
                {"role":"user","content":[{"type":"text","text":"current turn"}]}
            ]
        });
        let raw = serde_json::to_vec(&body).expect("fixture serializes");
        let (global, route) = active_policies();
        let prepared = HybridContextEngine::new(&global).prepare(
            "ordinary-image-request",
            &context(),
            CompressionPolicy::Balanced,
            body.clone(),
            Some(&raw),
            &global,
            &route,
            &active_registry(),
            &RenderCancellation::default(),
        );
        assert_ne!(prepared.kind(), HybridPreparationKind::Active);
        assert_eq!(prepared.body(), &body);
        assert_eq!(prepared.report().image_count, 0);
        assert!(prepared.report().capsule_digests.is_empty());
        assert_eq!(prepared.report().optical_candidate_count, 0);
        assert_eq!(
            prepared.accounting().sent_payload,
            SentPayloadVariant::Original
        );
    }
}
