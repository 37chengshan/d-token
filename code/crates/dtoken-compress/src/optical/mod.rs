pub mod applicability;
pub mod capability;
pub mod context_graph;
pub mod decision;
pub mod economics;
pub mod inventory;
pub mod plan_ir;
pub mod render_geometry;
pub mod risk;
pub mod schema_slimmer;
pub mod semantic_priority;
pub mod text_arena;
pub mod validate;
pub mod volatility;

pub use applicability::{
    evaluate_applicability, evaluate_applicability_with_policies,
    evaluate_applicability_with_policies_and_registry, evaluate_applicability_with_registry,
    resolve_endpoint,
};
pub use capability::{
    experimental_anthropic_active_capability, experimental_anthropic_active_snapshot,
    normalize_model_base, provider_family_for_protocol, resolve_model_capability,
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
pub use inventory::{
    build_anthropic_inventory, build_anthropic_inventory_with_arena, build_openai_chat_inventory,
    BlockKind, BlockRef, ConversationInventory, ConversationTurn, InitialProtection, RoleKind,
};
pub use plan_ir::{
    fingerprint_canonical_json, fingerprint_json, fingerprint_raw_bytes, BodyFingerprintEvidence,
    ContextOptimizationPlan, FingerprintKind, PlanCandidateEvidence,
};
pub use render_geometry::{
    anthropic_dense_v1, estimate_image_tokens, geometry_for_capability, images_required,
    RenderGeometryProfile,
};
pub use text_arena::{TextArena, TextRef, TextRefId, TextSensitivity};
pub use validate::{
    decide_final_status, validate_active_plan, validate_active_plan_input, validate_shadow_plan,
    validate_shadow_plan_input, ActivePlanValidationInput, CandidateValidationEvidence,
    PlanValidationResult, ShadowPlanValidationInput,
};
