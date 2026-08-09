pub mod applicability;
pub mod capability;
pub mod context_graph;
pub mod contract;
pub mod decision;
pub mod economics;
pub mod engine;
pub mod evidence;
pub mod eval;
pub mod eval_hybrid;
pub mod eval_profile;
pub mod history_collapse;
pub mod inventory;
pub mod patcher;
pub mod plan_ir;
pub mod post_validate;
pub mod render_geometry;
pub mod renderer;
pub mod risk;
pub mod schema_slimmer;
pub mod secret;
pub mod semantic_priority;
pub mod spleen5x8;
pub mod text_arena;
pub mod validate;
pub mod volatility;

pub use applicability::{
    evaluate_applicability, evaluate_applicability_with_policies,
    evaluate_applicability_with_policies_and_registry, evaluate_applicability_with_registry,
    resolve_endpoint,
};
pub use capability::{
    capability_attestation_payload, eval_artifact_digest, normalize_model_base,
    provider_family_for_protocol, resolve_model_capability,
    resolve_model_capability_record_for_route_with_registry,
    resolve_model_capability_record_with_registry, resolve_model_capability_with_registry,
    sign_capability_snapshot, CapabilityRegistryError, OpticalCapabilityRegistry,
    ResolvedOpticalCapability,
};
pub use context_graph::{
    build_context_graph, ContextEdge, ContextEdgeKind, ContextGraph, ContextGraphSummary,
    ContextNode, ProviderToolStateMachine,
};
pub use contract::{OpticalTransformContractV2, OPTICAL_TRANSFORM_CONTRACT_SCHEMA_VERSION};
pub use decision::{
    build_shadow_plan, build_shadow_plan_detailed, build_shadow_plan_detailed_with_registry,
    CandidateAction, CandidateDecision, ShadowPlanBuild,
};
pub use economics::{CacheEvidenceState, CostEstimateRange};
pub use engine::{
    HybridContextEngine, HybridEngineError, HybridExecutionReport, HybridPlanV2,
    HybridPreparationKind, PreparedHybridRequest,
};
pub use eval::{
    eval_corpus_digest, EvalBuildError, EvalSampleObservation, EvalThresholds, OpticalEvalBuilder,
};
pub use evidence::optical_exact_route_evidence;
pub use eval_hybrid::{
    partition_eval_hybrid, EvalHybridBlock, EvalHybridBlockKind, EvalHybridDecision,
    EvalHybridPartition,
};
pub use eval_profile::{
    build_exact_factsheet, extract_exact_facts, resolve_eval_profile, ExactFact, ExactFactKind,
    ExactFactSheet, ExactFactSheetPolicy, ModelRenderProfile, ProfileProvenance, RenderFontId,
    RenderLayoutPolicy, OPENAI_VISION_RULES_RETRIEVED_AT, OPENAI_VISION_RULES_URL,
    PXPIPE_SOURCE_COMMIT,
};
pub use history_collapse::{
    frozen_prefix_is_byte_identical, pin_reason_for, plan_history_collapse, BillingBoundary,
    CacheAnchor, CollapseCandidate, FreezeGrid, FrozenRow, HistoryCollapsePlan, PinReason,
    SlabProvenance, SlabRow, StaticSlab, HISTORY_COLLAPSE_PLANNER_REVISION,
};
pub use inventory::{
    build_anthropic_inventory, build_anthropic_inventory_with_arena, build_openai_chat_inventory,
    build_openai_chat_inventory_with_arena, build_openai_responses_inventory,
    build_openai_responses_inventory_with_arena, BlockKind, BlockRef, ConversationInventory,
    ConversationTurn, InitialProtection, RoleKind,
};
pub use patcher::{
    HistoryBoundaryGuard, OpticalPatchOperation, OpticalPatchSet, PatchError,
    RenderedContextPointer,
};
pub use plan_ir::{
    fingerprint_canonical_json, fingerprint_json, fingerprint_raw_bytes, BodyFingerprintEvidence,
    ContextOptimizationPlan, FingerprintKind, PlanCandidateEvidence,
};
pub use post_validate::{PostValidationError, PostValidationReport, ValidatedActiveRequest};
pub use render_geometry::{
    anthropic_dense_v1, anthropic_dense_v2, estimate_image_tokens,
    estimate_image_tokens_for_dimensions, geometry_for_capability, images_required,
    RenderGeometryProfile,
};
pub use renderer::{
    dereflow_rendered_text, font_digest, font_digest_for, platform_render_evidence,
    production_render_contract, profile_digest, render_contract_digest, validate_png,
    DeterministicPngRenderer, PlatformPageEvidence, PlatformRenderEvidence, RenderCancellation,
    RenderContract, RenderError, RenderLimits, RenderPermitPool, RenderedArtifact, RenderedPage,
    DETERMINISTIC_FONT_ID, PLATFORM_EVIDENCE_CORPUS, PLATFORM_EVIDENCE_CORPUS_REVISION,
};
pub use secret::{
    contains_secret_like, SecretClassifier, SecretFinding, SecretKind, SECRET_CLASSIFIER_REVISION,
};
pub use spleen5x8::{
    is_supported as spleen_glyph_is_supported, SPLEEN_5X8_AA_REVISION,
    SPLEEN_5X8_FALLBACK_REVISION, SPLEEN_5X8_SOURCE_REVISION, SPLEEN_5X8_UNICODE_FALLBACK_GLYPH,
};
pub use text_arena::{TextArena, TextRef, TextRefId, TextSensitivity};
pub use validate::{
    decide_final_status, validate_active_plan, validate_active_plan_input, validate_shadow_plan,
    validate_shadow_plan_input, ActivePlanValidationInput, CandidateValidationEvidence,
    PlanValidationResult, ShadowPlanValidationInput,
};
