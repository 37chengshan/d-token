//! Bounded, optional Jev decision layer.
//!
//! This crate deliberately does not compress text, choose a provider, or
//! construct a provider request.  It accepts opaque unit metadata and returns
//! a typed selection decision that a local context engine can validate and
//! apply.  A missing credential always results in local mode at the policy
//! boundary; it is never treated as a remote failure.

mod circuit_breaker;
mod contract;
mod jev_client;
mod lint;
mod policy;

pub use circuit_breaker::{BreakerState, CircuitBreaker, CircuitBreakerSet};
pub use contract::{
    DecisionInput, DecisionMetadata, DecisionRequest, DecisionResponse, DecisionUnit,
    DecisionUnitDecision, LintedDecision, StickyDecision, DECISION_SCHEMA_VERSION,
};
pub use jev_client::{
    ClientLimits, DecisionClient, DecisionEvent, DecisionFailure, DecisionFailureKind,
    DecisionHttpStatus, JevClient, SecretString,
};
pub use lint::{lint_response, LintError};
pub use policy::{
    dispatch_for, DecisionConfigError, DecisionRuntimeConfig, DecisionWorkflow, WorkflowFallback,
    WorkflowMode, WorkflowPolicy, EXECUTABLE_WORKFLOWS, RESERVED_WORKFLOWS,
};

