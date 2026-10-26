use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, net::IpAddr};
use thiserror::Error;

pub use dtoken_types::DecisionWorkflow;

pub const EXECUTABLE_WORKFLOWS: [DecisionWorkflow; 5] = [
    DecisionWorkflow::ToolOutputRetention,
    DecisionWorkflow::ArrivalSpanRetention,
    DecisionWorkflow::SessionCompaction,
    DecisionWorkflow::ContextRanking,
    DecisionWorkflow::RuleSkillSelection,
];

pub const RESERVED_WORKFLOWS: [DecisionWorkflow; 5] = [
    DecisionWorkflow::MemoryWriteQualification,
    DecisionWorkflow::CompletionVerification,
    DecisionWorkflow::DiagnosticPriority,
    DecisionWorkflow::ToolActionReview,
    DecisionWorkflow::ModelEffortRouting,
];

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowMode {
    Local,
    #[default]
    Shadow,
    Live,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowFallback {
    /// Retain every eligible unit. This is safe for retention/selection
    /// workflows and keeps the request usable when no local engine applies.
    KeepAll,
    /// Let the local context engine apply its deterministic policy.
    #[default]
    Local,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkflowPolicy {
    #[serde(default)]
    pub mode: WorkflowMode,
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u64,
    #[serde(default = "default_minimum_confidence")]
    pub minimum_confidence: f32,
    #[serde(default = "default_sticky_ttl_ms")]
    pub sticky_ttl_ms: u64,
    #[serde(default)]
    pub fallback: WorkflowFallback,
    #[serde(default)]
    pub kill_switch: bool,
}

const fn default_timeout_ms() -> u64 {
    500
}

const fn default_minimum_confidence() -> f32 {
    0.70
}

const fn default_sticky_ttl_ms() -> u64 {
    30_000
}

impl Default for WorkflowPolicy {
    fn default() -> Self {
        Self {
            mode: WorkflowMode::default(),
            timeout_ms: default_timeout_ms(),
            minimum_confidence: default_minimum_confidence(),
            sticky_ttl_ms: default_sticky_ttl_ms(),
            fallback: WorkflowFallback::default(),
            kill_switch: false,
        }
    }
}

impl WorkflowPolicy {
    pub fn validate(&self, workflow: DecisionWorkflow) -> Result<(), DecisionConfigError> {
        if !workflow.is_executable() {
            return Err(DecisionConfigError::ReservedWorkflow(workflow));
        }
        if !(50..=2_000).contains(&self.timeout_ms) {
            return Err(DecisionConfigError::TimeoutOutOfRange {
                workflow,
                value: self.timeout_ms,
            });
        }
        if !self.minimum_confidence.is_finite()
            || !(0.0..=1.0).contains(&self.minimum_confidence)
        {
            return Err(DecisionConfigError::ConfidenceOutOfRange {
                workflow,
                value: self.minimum_confidence,
            });
        }
        if self.sticky_ttl_ms > 86_400_000 {
            return Err(DecisionConfigError::StickyTtlOutOfRange {
                workflow,
                value: self.sticky_ttl_ms,
            });
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DecisionRuntimeConfig {
    #[serde(default = "default_schema_version")]
    pub schema_version: u16,
    #[serde(default)]
    pub enabled: bool,
    /// Opaque OS keychain reference. The key itself is never serialized here.
    #[serde(default)]
    pub credential_ref: Option<String>,
    #[serde(default)]
    pub endpoint: Option<String>,
    #[serde(default = "default_max_concurrency")]
    pub max_concurrency: u16,
    #[serde(default = "default_workflow_policies")]
    pub workflows: BTreeMap<DecisionWorkflow, WorkflowPolicy>,
}

const fn default_schema_version() -> u16 {
    1
}

const fn default_max_concurrency() -> u16 {
    2
}

fn default_workflow_policies() -> BTreeMap<DecisionWorkflow, WorkflowPolicy> {
    EXECUTABLE_WORKFLOWS
        .into_iter()
        .map(|workflow| (workflow, WorkflowPolicy::default()))
        .collect()
}

impl Default for DecisionRuntimeConfig {
    fn default() -> Self {
        Self {
            schema_version: default_schema_version(),
            enabled: false,
            credential_ref: None,
            endpoint: None,
            max_concurrency: default_max_concurrency(),
            workflows: default_workflow_policies(),
        }
    }
}

impl DecisionRuntimeConfig {
    pub fn effective_mode(&self, workflow: DecisionWorkflow) -> WorkflowMode {
        let Some(policy) = self.workflows.get(&workflow) else {
            return WorkflowMode::Local;
        };
        if !self.enabled
            || self.credential_ref.is_none()
            || self.endpoint.is_none()
            || policy.kill_switch
        {
            WorkflowMode::Local
        } else {
            policy.mode
        }
    }

    pub fn validate(&self) -> Result<(), DecisionConfigError> {
        if self.schema_version != 1 {
            return Err(DecisionConfigError::UnsupportedSchemaVersion(
                self.schema_version,
            ));
        }
        if self.max_concurrency == 0 || self.max_concurrency > 32 {
            return Err(DecisionConfigError::ConcurrencyOutOfRange(
                self.max_concurrency,
            ));
        }
        if let Some(reference) = self.credential_ref.as_deref() {
            if !is_opaque_credential_ref(reference) {
                return Err(DecisionConfigError::CredentialReferenceNotOpaque);
            }
        }
        if let Some(endpoint) = self.endpoint.as_deref() {
            validate_endpoint(endpoint)?;
        }
        for workflow in RESERVED_WORKFLOWS {
            if self.workflows.contains_key(&workflow) {
                return Err(DecisionConfigError::ReservedWorkflow(workflow));
            }
        }
        if self.workflows.len() != EXECUTABLE_WORKFLOWS.len()
            || EXECUTABLE_WORKFLOWS
                .iter()
                .any(|workflow| !self.workflows.contains_key(workflow))
        {
            return Err(DecisionConfigError::ExecutableWorkflowSetMismatch);
        }
        for workflow in EXECUTABLE_WORKFLOWS {
            self.workflows
                .get(&workflow)
                .expect("validated executable workflow set")
                .validate(workflow)?;
        }
        Ok(())
    }
}

fn is_opaque_credential_ref(reference: &str) -> bool {
    [
        "os-credential:",
        "os-credential|",
        "wincred:",
        "wincred|",
        "keychain:",
        "keychain|",
    ]
    .iter()
    .any(|prefix| reference.starts_with(prefix))
        && reference.len() > 14
        && !reference.contains('\n')
        && !reference.contains('\r')
}

fn validate_endpoint(endpoint: &str) -> Result<(), DecisionConfigError> {
    let parsed = reqwest::Url::parse(endpoint)
        .map_err(|_| DecisionConfigError::InvalidEndpoint(endpoint.to_string()))?;
    match parsed.scheme() {
        "https" => Ok(()),
        "http" if cfg!(debug_assertions) && is_loopback_host(parsed.host_str()) => Ok(()),
        _ => Err(DecisionConfigError::EndpointMustUseHttps),
    }
}

fn is_loopback_host(host: Option<&str>) -> bool {
    let Some(host) = host else { return false };
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }
    host.parse::<IpAddr>().is_ok_and(|ip| ip.is_loopback())
}

#[derive(Debug, Error, Clone, PartialEq)]
pub enum DecisionConfigError {
    #[error("unsupported decision schema version: {0}")]
    UnsupportedSchemaVersion(u16),
    #[error("decision concurrency is out of range: {0}")]
    ConcurrencyOutOfRange(u16),
    #[error("decision credential reference must be an opaque OS credential reference")]
    CredentialReferenceNotOpaque,
    #[error("decision endpoint is invalid: {0}")]
    InvalidEndpoint(String),
    #[error("decision endpoint must use HTTPS outside loopback test endpoints")]
    EndpointMustUseHttps,
    #[error("reserved workflow cannot be enabled: {0}")]
    ReservedWorkflow(DecisionWorkflow),
    #[error("decision config must contain exactly the five executable workflows")]
    ExecutableWorkflowSetMismatch,
    #[error("workflow {workflow} timeout is out of range: {value}ms")]
    TimeoutOutOfRange { workflow: DecisionWorkflow, value: u64 },
    #[error("workflow {workflow} confidence is out of range: {value}")]
    ConfidenceOutOfRange { workflow: DecisionWorkflow, value: f32 },
    #[error("workflow {workflow} sticky TTL is out of range: {value}ms")]
    StickyTtlOutOfRange { workflow: DecisionWorkflow, value: u64 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkflowDispatch {
    ToolOutputRetention,
    ArrivalSpanRetention,
    SessionCompaction,
    ContextRanking,
    RuleSkillSelection,
}

pub const fn dispatch_for(workflow: DecisionWorkflow) -> Option<WorkflowDispatch> {
    match workflow {
        DecisionWorkflow::ToolOutputRetention => Some(WorkflowDispatch::ToolOutputRetention),
        DecisionWorkflow::ArrivalSpanRetention => Some(WorkflowDispatch::ArrivalSpanRetention),
        DecisionWorkflow::SessionCompaction => Some(WorkflowDispatch::SessionCompaction),
        DecisionWorkflow::ContextRanking => Some(WorkflowDispatch::ContextRanking),
        DecisionWorkflow::RuleSkillSelection => Some(WorkflowDispatch::RuleSkillSelection),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn executable_and_reserved_sets_are_disjoint() {
        assert_eq!(EXECUTABLE_WORKFLOWS.len(), 5);
        assert_eq!(RESERVED_WORKFLOWS.len(), 5);
        assert!(EXECUTABLE_WORKFLOWS
            .iter()
            .all(|workflow| workflow.is_executable()));
        assert!(RESERVED_WORKFLOWS
            .iter()
            .all(|workflow| workflow.is_reserved()));
    }

    #[test]
    fn no_credential_always_selects_local_mode() {
        let config = DecisionRuntimeConfig {
            enabled: true,
            endpoint: Some("https://jev.example.invalid".to_string()),
            ..Default::default()
        };
        assert_eq!(
            config.effective_mode(DecisionWorkflow::ContextRanking),
            WorkflowMode::Local
        );
    }
}
