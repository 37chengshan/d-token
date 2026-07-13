pub mod applicability;
pub mod capability;
pub mod context_graph;
pub mod decision;
pub mod economics;
pub mod engine;
pub mod eval;
pub mod eval_hybrid;
pub mod eval_profile;
pub mod inventory;
pub mod patcher;
pub mod plan_ir;
pub mod post_validate;
pub mod render_geometry;
pub mod renderer;
pub mod risk;
pub mod schema_slimmer;
pub mod semantic_priority;
mod spleen5x8;
pub mod text_arena;
pub mod validate;
pub mod volatility;

pub use applicability::{
    evaluate_applicability, evaluate_applicability_with_policies,
    evaluate_applicability_with_policies_and_registry, evaluate_applicability_with_registry,
    resolve_endpoint,
};
pub use capability::{
    capability_attestation_payload, eval_artifact_digest, experimental_anthropic_active_capability,
    experimental_anthropic_active_snapshot, experimental_anthropic_active_snapshot_for_route,
    normalize_model_base, provider_family_for_protocol, resolve_model_capability,
    resolve_model_capability_record_for_route_with_registry,
    resolve_model_capability_record_with_registry, resolve_model_capability_with_registry,
    CapabilityRegistryError, OpticalCapabilityRegistry, ResolvedOpticalCapability,
};
pub use context_graph::{
    build_context_graph, ContextEdge, ContextEdgeKind, ContextGraph, ContextGraphSummary,
    ContextNode, ProviderToolStateMachine,
};
pub use decision::{
    build_shadow_plan, build_shadow_plan_detailed, build_shadow_plan_detailed_with_registry,
    CandidateAction, CandidateDecision, ShadowPlanBuild,
};
pub use engine::{
    HybridContextEngine, HybridEngineError, HybridExecutionReport, HybridPreparationKind,
    PreparedHybridRequest,
};
pub use eval::{
    eval_corpus_digest, EvalBuildError, EvalSampleObservation, EvalThresholds, OpticalEvalBuilder,
};
pub use eval_hybrid::{
    partition_eval_hybrid, EvalHybridBlock, EvalHybridBlockKind, EvalHybridDecision,
    EvalHybridPartition,
};
pub use eval_profile::{
    build_exact_factsheet, contains_secret_like, resolve_eval_profile, ExactFact, ExactFactSheet,
    ExactFactSheetPolicy, ModelRenderProfile, ProfileProvenance, RenderFontId, RenderLayoutPolicy,
    OPENAI_VISION_RULES_RETRIEVED_AT, OPENAI_VISION_RULES_URL, PXPIPE_SOURCE_COMMIT,
};
pub use inventory::{
    build_anthropic_inventory, build_anthropic_inventory_with_arena, build_openai_chat_inventory,
    BlockKind, BlockRef, ConversationInventory, ConversationTurn, InitialProtection, RoleKind,
};
pub use patcher::{OpticalPatchOperation, OpticalPatchSet, PatchError};
pub use plan_ir::{
    fingerprint_canonical_json, fingerprint_json, fingerprint_raw_bytes, BodyFingerprintEvidence,
    ContextOptimizationPlan, FingerprintKind, PlanCandidateEvidence,
};
pub use post_validate::{PostValidationError, PostValidationReport, ValidatedActiveRequest};
pub use render_geometry::{
    anthropic_dense_v1, estimate_image_tokens, geometry_for_capability, images_required,
    RenderGeometryProfile,
};
pub use renderer::{
    font_digest, font_digest_for, profile_digest, validate_png, DeterministicPngRenderer,
    RenderCancellation, RenderError, RenderLimits, RenderPermitPool, RenderedArtifact,
    RenderedPage, DETERMINISTIC_FONT_ID,
};
pub use text_arena::{TextArena, TextRef, TextRefId, TextSensitivity};
pub use validate::{
    decide_final_status, validate_active_plan, validate_active_plan_input, validate_shadow_plan,
    validate_shadow_plan_input, ActivePlanValidationInput, CandidateValidationEvidence,
    PlanValidationResult, ShadowPlanValidationInput,
};
