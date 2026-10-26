//! Deterministic, provider-neutral local context preparation.
//!
//! The engine owns the decision boundary and lifecycle.  It delegates content
//! transformations to `dtoken-compress`, then uses `dtoken-runtime-pipeline`
//! to ensure a candidate is validated before it can be marked ready to send.
//! No network or Jev credential is required.

mod classify;
mod local;
mod validate;

pub use classify::{classify_unit, UnitClassification};
pub use local::{
    BaselineContext, ContextBudget, ContextEngine, ContextInput, ContextOutcome,
    LocalContextConfig, PreparedContextCandidate, RecoveryInput,
};
pub use validate::{validate_json_candidate, validate_unit_candidate, ValidationReport};

pub use dtoken_compress::protection::{
    inventory_context, protected_spans, ContextInventory, ContextInventoryEntry, ContextUnit,
    ContextUnitKind, ProtectedKind, ProtectedSpan, ProtectedSpanSet,
};
