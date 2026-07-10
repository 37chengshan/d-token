pub mod applicability;
pub mod capability;
pub mod decision;
pub mod economics;
pub mod inventory;
pub mod render_geometry;
pub mod risk;
pub mod schema_slimmer;
pub mod semantic_priority;
pub mod text_arena;
pub mod validate;
pub mod volatility;

pub use applicability::{evaluate_applicability, resolve_endpoint};
pub use capability::{
    normalize_model_base, provider_family_for_protocol, resolve_model_capability,
};
pub use decision::{build_shadow_plan, CandidateAction, CandidateDecision};
pub use render_geometry::{
    anthropic_dense_v1, estimate_image_tokens, geometry_for_capability, RenderGeometryProfile,
};
pub use text_arena::{TextArena, TextRef, TextRefId, TextSensitivity};
pub use validate::{
    decide_final_status, validate_active_plan, validate_shadow_plan, PlanValidationResult,
};
