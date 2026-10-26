use crate::{DecisionConfigError, DecisionWorkflow, WorkflowPolicy};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const DECISION_SCHEMA_VERSION: u16 = 1;
const MAX_UNIT_ID_LEN: usize = 128;
const MAX_UNITS: usize = 4096;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DecisionMetadata {
    pub scope_digest: String,
    pub policy_digest: String,
    #[serde(default)]
    pub pressure_ratio_basis_points: Option<u16>,
}

impl DecisionMetadata {
    pub fn validate(&self) -> Result<(), ContractError> {
        validate_sha256_digest(&self.scope_digest, "scope_digest")?;
        validate_sha256_digest(&self.policy_digest, "policy_digest")?;
        if self
            .pressure_ratio_basis_points
            .is_some_and(|value| value > 10_000)
        {
            return Err(ContractError::NumericOutOfRange(
                "pressure_ratio_basis_points".to_string(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DecisionUnit {
    pub unit_id: String,
    pub kind: String,
    pub byte_len: u32,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub protected: bool,
    #[serde(default)]
    pub age_ms: Option<u64>,
}

impl DecisionUnit {
    pub fn validate(&self) -> Result<(), ContractError> {
        validate_unit_id(&self.unit_id)?;
        if self.kind.is_empty() || self.kind.len() > 64 {
            return Err(ContractError::InvalidUnitId(self.unit_id.clone()));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DecisionInput {
    pub request_id: String,
    pub input_digest: String,
    pub candidate_digest: String,
    pub metadata: DecisionMetadata,
    pub units: Vec<DecisionUnit>,
}

impl DecisionInput {
    pub fn validate(&self) -> Result<(), ContractError> {
        validate_request_id(&self.request_id)?;
        validate_sha256_digest(&self.input_digest, "input_digest")?;
        validate_sha256_digest(&self.candidate_digest, "candidate_digest")?;
        self.metadata.validate()?;
        if self.units.is_empty() || self.units.len() > MAX_UNITS {
            return Err(ContractError::UnitCountOutOfRange(self.units.len()));
        }
        let mut ids = BTreeSet::new();
        for unit in &self.units {
            unit.validate()?;
            if !ids.insert(unit.unit_id.as_str()) {
                return Err(ContractError::DuplicateUnitId(unit.unit_id.clone()));
            }
        }
        Ok(())
    }
}

pub type DecisionRequest = DecisionInput;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DecisionUnitDecision {
    pub unit_id: String,
    pub selected: bool,
    pub score: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DecisionResponse {
    pub schema_version: u16,
    pub workflow: DecisionWorkflow,
    pub request_id: String,
    pub input_digest: String,
    pub candidate_digest: String,
    pub confidence: f32,
    pub units: Vec<DecisionUnitDecision>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct LintedDecision {
    pub schema_version: u16,
    pub workflow: DecisionWorkflow,
    pub request_id: String,
    pub input_digest: String,
    pub candidate_digest: String,
    pub confidence: f32,
    pub units: Vec<DecisionUnitDecision>,
}

impl DecisionResponse {
    pub fn lint(
        &self,
        workflow: DecisionWorkflow,
        request: &DecisionRequest,
        policy: &WorkflowPolicy,
    ) -> Result<LintedDecision, ContractError> {
        crate::lint::lint_response(self, workflow, request, policy)
            .map_err(ContractError::from)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct StickyDecision {
    pub scope_digest: String,
    pub workflow: DecisionWorkflow,
    pub source_digest: String,
    pub policy_digest: String,
    pub decision: LintedDecision,
    pub created_at_ms: u64,
    pub expires_at_ms: u64,
}

impl StickyDecision {
    pub fn is_reusable(
        &self,
        now_ms: u64,
        scope_digest: &str,
        workflow: DecisionWorkflow,
        source_digest: &str,
        policy_digest: &str,
    ) -> bool {
        now_ms <= self.expires_at_ms
            && self.scope_digest == scope_digest
            && self.workflow == workflow
            && self.source_digest == source_digest
            && self.policy_digest == policy_digest
            && self.decision.workflow == workflow
            && self.decision.input_digest == source_digest
    }

    pub fn from_decision(
        decision: LintedDecision,
        metadata: &DecisionMetadata,
        created_at_ms: u64,
        policy: &WorkflowPolicy,
    ) -> Result<Self, ContractError> {
        metadata.validate()?;
        let expires_at_ms = created_at_ms
            .checked_add(policy.sticky_ttl_ms)
            .ok_or(ContractError::TimestampOverflow)?;
        Ok(Self {
            scope_digest: metadata.scope_digest.clone(),
            workflow: decision.workflow,
            source_digest: decision.input_digest.clone(),
            policy_digest: metadata.policy_digest.clone(),
            decision,
            created_at_ms,
            expires_at_ms,
        })
    }
}

fn validate_sha256_digest(value: &str, field: &str) -> Result<(), ContractError> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
    {
        return Err(ContractError::InvalidDigest(field.to_string()));
    }
    Ok(())
}

fn validate_request_id(value: &str) -> Result<(), ContractError> {
    if value.is_empty()
        || value.len() > 128
        || value
            .bytes()
            .any(|byte| !matches!(byte, b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'_' | b'.' | b':' | b'-'))
    {
        return Err(ContractError::InvalidRequestId);
    }
    Ok(())
}

fn validate_unit_id(value: &str) -> Result<(), ContractError> {
    if value.is_empty() || value.len() > MAX_UNIT_ID_LEN || value.bytes().any(|byte| byte < 0x20) {
        return Err(ContractError::InvalidUnitId(value.to_string()));
    }
    Ok(())
}

#[derive(Debug, thiserror::Error, Clone, PartialEq)]
pub enum ContractError {
    #[error("invalid digest field: {0}")]
    InvalidDigest(String),
    #[error("invalid request id")]
    InvalidRequestId,
    #[error("invalid unit id: {0}")]
    InvalidUnitId(String),
    #[error("unit count is out of range: {0}")]
    UnitCountOutOfRange(usize),
    #[error("duplicate unit id: {0}")]
    DuplicateUnitId(String),
    #[error("response schema version is unsupported: {0}")]
    UnsupportedSchemaVersion(u16),
    #[error("response workflow does not match request")]
    WorkflowMismatch,
    #[error("response digest does not match request")]
    DigestMismatch,
    #[error("response request id does not match request")]
    RequestIdMismatch,
    #[error("response candidate digest does not match request")]
    CandidateDigestMismatch,
    #[error("response does not cover every input unit")]
    CoverageMismatch,
    #[error("response has a duplicate unit id: {0}")]
    ResponseDuplicateUnitId(String),
    #[error("response score/confidence is out of range: {0}")]
    NumericOutOfRange(String),
    #[error("response confidence is below workflow minimum")]
    LowConfidence,
    #[error("timestamp overflow")]
    TimestampOverflow,
    #[error("{0}")]
    Lint(String),
    #[error("{0}")]
    Config(#[from] DecisionConfigError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DecisionWorkflow, WorkflowPolicy};

    fn input() -> DecisionInput {
        DecisionInput {
            request_id: "request-1".to_string(),
            input_digest: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                .to_string(),
            candidate_digest: "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
                .to_string(),
            metadata: DecisionMetadata {
                scope_digest: "c".repeat(64),
                policy_digest: "d".repeat(64),
                pressure_ratio_basis_points: None,
            },
            units: vec![DecisionUnit {
                unit_id: "unit-a".to_string(),
                kind: "tool_result".to_string(),
                byte_len: 100,
                required: false,
                protected: false,
                age_ms: Some(1),
            }],
        }
    }

    #[test]
    fn sticky_decision_is_scoped_and_expires() {
        let request = input();
        let decision = LintedDecision {
            schema_version: DECISION_SCHEMA_VERSION,
            workflow: DecisionWorkflow::ToolOutputRetention,
            request_id: request.request_id.clone(),
            input_digest: request.input_digest.clone(),
            candidate_digest: request.candidate_digest.clone(),
            confidence: 0.9,
            units: vec![DecisionUnitDecision {
                unit_id: "unit-a".to_string(),
                selected: true,
                score: 0.9,
            }],
        };
        let sticky = StickyDecision::from_decision(
            decision,
            &request.metadata,
            10,
            &WorkflowPolicy::default(),
        )
        .expect("sticky decision");
        assert!(sticky.is_reusable(
            10,
            &"c".repeat(64),
            DecisionWorkflow::ToolOutputRetention,
            &request.input_digest,
            &"d".repeat(64)
        ));
        assert!(!sticky.is_reusable(
            40_011,
            &"c".repeat(64),
            DecisionWorkflow::ToolOutputRetention,
            &request.input_digest,
            &"d".repeat(64)
        ));
        assert!(!sticky.is_reusable(
            10,
            &"e".repeat(64),
            DecisionWorkflow::ToolOutputRetention,
            &request.input_digest,
            &"d".repeat(64)
        ));
    }
}
