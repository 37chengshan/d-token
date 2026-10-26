use crate::{
    contract::{ContractError, DecisionRequest, DecisionResponse, LintedDecision},
    DecisionWorkflow, WorkflowPolicy, DECISION_SCHEMA_VERSION,
};
use std::collections::BTreeSet;

#[derive(Debug, thiserror::Error, Clone, PartialEq)]
pub enum LintError {
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
    #[error("response must cover every input unit exactly once")]
    CoverageMismatch,
    #[error("response contains duplicate unit id: {0}")]
    DuplicateUnitId(String),
    #[error("response confidence is out of range")]
    ConfidenceOutOfRange,
    #[error("response score is out of range for unit {0}")]
    ScoreOutOfRange(String),
    #[error("response confidence is below minimum")]
    LowConfidence,
    #[error("request contract is invalid: {0}")]
    InvalidRequest(String),
}

pub fn lint_response(
    response: &DecisionResponse,
    workflow: DecisionWorkflow,
    request: &DecisionRequest,
    policy: &WorkflowPolicy,
) -> Result<LintedDecision, LintError> {
    request
        .validate()
        .map_err(|error| LintError::InvalidRequest(error.to_string()))?;
    policy
        .validate(workflow)
        .map_err(|error| LintError::InvalidRequest(error.to_string()))?;
    if response.schema_version != DECISION_SCHEMA_VERSION {
        return Err(LintError::UnsupportedSchemaVersion(response.schema_version));
    }
    if response.workflow != workflow {
        return Err(LintError::WorkflowMismatch);
    }
    if response.request_id != request.request_id {
        return Err(LintError::RequestIdMismatch);
    }
    if response.input_digest != request.input_digest {
        return Err(LintError::DigestMismatch);
    }
    if response.candidate_digest != request.candidate_digest {
        return Err(LintError::CandidateDigestMismatch);
    }
    let expected = request
        .units
        .iter()
        .map(|unit| unit.unit_id.as_str())
        .collect::<BTreeSet<_>>();
    let mut seen = BTreeSet::new();
    for unit in &response.units {
        if !seen.insert(unit.unit_id.as_str()) {
            return Err(LintError::DuplicateUnitId(unit.unit_id.clone()));
        }
        if !expected.contains(unit.unit_id.as_str()) {
            return Err(LintError::CoverageMismatch);
        }
        if !unit.score.is_finite() || !(0.0..=1.0).contains(&unit.score) {
            return Err(LintError::ScoreOutOfRange(unit.unit_id.clone()));
        }
    }
    if seen != expected {
        return Err(LintError::CoverageMismatch);
    }
    if !response.confidence.is_finite() || !(0.0..=1.0).contains(&response.confidence) {
        return Err(LintError::ConfidenceOutOfRange);
    }
    if response.confidence < policy.minimum_confidence {
        return Err(LintError::LowConfidence);
    }
    Ok(LintedDecision {
        schema_version: response.schema_version,
        workflow: response.workflow,
        request_id: response.request_id.clone(),
        input_digest: response.input_digest.clone(),
        candidate_digest: response.candidate_digest.clone(),
        confidence: response.confidence,
        units: response.units.clone(),
    })
}

impl From<LintError> for ContractError {
    fn from(value: LintError) -> Self {
        match value {
            LintError::UnsupportedSchemaVersion(version) => {
                ContractError::UnsupportedSchemaVersion(version)
            }
            LintError::WorkflowMismatch => ContractError::WorkflowMismatch,
            LintError::DigestMismatch => ContractError::DigestMismatch,
            LintError::RequestIdMismatch => ContractError::RequestIdMismatch,
            LintError::CandidateDigestMismatch => ContractError::CandidateDigestMismatch,
            LintError::CoverageMismatch => ContractError::CoverageMismatch,
            LintError::DuplicateUnitId(unit_id) => ContractError::ResponseDuplicateUnitId(unit_id),
            LintError::ConfidenceOutOfRange | LintError::ScoreOutOfRange(_) => {
                ContractError::NumericOutOfRange("score_or_confidence".to_string())
            }
            LintError::LowConfidence => ContractError::LowConfidence,
            LintError::InvalidRequest(message) => ContractError::Lint(message),
        }
    }
}
