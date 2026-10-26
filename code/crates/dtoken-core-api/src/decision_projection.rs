//! Redacted context-decision projections shared by the runtime and desktop
//! analytics surfaces.
//!
//! This module is intentionally a pure mapping layer.  It does not inspect or
//! persist request bodies, and it does not call Jev or a provider.  Runtime
//! callers can use the typed projections here to update the context store and
//! append the dedicated decision event stream in one terminal commit.

use anyhow::{bail, Result};
use dtoken_context_store::DecisionProjection;
use dtoken_event_ledger::ContextDecisionRecorded;
use dtoken_types::{
    ConfidenceBand, ContextDecisionReceipt, DecisionFallback, DecisionSource, DecisionWorkflow,
};
use serde::{Deserialize, Serialize};

/// Public analytics shape.  Only redacted identifiers, digests, counts and
/// fixed vocabulary are exposed; the CCR reference is represented as a
/// presence bit so the UI cannot accidentally request or display raw content.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RedactedContextDecisionProjection {
    pub schema_version: u8,
    pub request_id: String,
    pub workflow: DecisionWorkflow,
    pub source: DecisionSource,
    pub confidence_band: ConfidenceBand,
    pub latency_ms: u64,
    pub input_digest: String,
    pub candidate_digest: Option<String>,
    pub selected_units: u32,
    pub dropped_units: u32,
    pub fallback: DecisionFallback,
    pub reason_code: String,
    pub recovery_present: bool,
}

impl RedactedContextDecisionProjection {
    pub fn from_receipt(receipt: &ContextDecisionReceipt) -> Result<Self> {
        receipt
            .validate()
            .map_err(|error| anyhow::anyhow!(error.to_string()))?;
        Ok(Self {
            schema_version: receipt.schema_version,
            request_id: receipt.request_id.clone(),
            workflow: receipt.workflow,
            source: receipt.source,
            confidence_band: receipt.confidence_band,
            latency_ms: receipt.latency_ms,
            input_digest: receipt.input_digest.clone(),
            candidate_digest: receipt.candidate_digest.clone(),
            selected_units: receipt.selected_units,
            dropped_units: receipt.dropped_units,
            fallback: receipt.fallback,
            reason_code: receipt.reason_code.clone(),
            recovery_present: receipt.recovery_ref.is_some(),
        })
    }
}

/// Maps one runtime receipt into its SQLite projection.  This is the only
/// mapping used by core-api so storage cannot grow a second, less restrictive
/// representation accidentally.
pub fn project_store_decision(
    receipt: &ContextDecisionReceipt,
    created_at_ms: u64,
) -> Result<DecisionProjection> {
    DecisionProjection::from_receipt(receipt, created_at_ms)
}

/// Maps one runtime receipt into the dedicated redacted ledger event.
pub fn project_decision_event(receipt: &ContextDecisionReceipt) -> Result<ContextDecisionRecorded> {
    if !receipt.is_valid() {
        bail!("invalid context decision receipt");
    }
    Ok(ContextDecisionRecorded::from_receipt(receipt))
}

/// Returns all three terminal projections for callers that need one explicit
/// write boundary: SQLite metadata, JSONL event, and UI response shape.
pub fn project_decision(
    receipt: &ContextDecisionReceipt,
    created_at_ms: u64,
) -> Result<(
    DecisionProjection,
    ContextDecisionRecorded,
    RedactedContextDecisionProjection,
)> {
    Ok((
        project_store_decision(receipt, created_at_ms)?,
        project_decision_event(receipt)?,
        RedactedContextDecisionProjection::from_receipt(receipt)?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn receipt() -> ContextDecisionReceipt {
        ContextDecisionReceipt::new(
            "req-1",
            DecisionWorkflow::ContextRanking,
            DecisionSource::Local,
            ConfidenceBand::Medium,
            8,
        )
        .with_digests(
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            Some("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".into()),
        )
        .with_counts(10, 1)
        .with_fallback(DecisionFallback::NotRequired)
        .with_reason_code("local_selected")
    }

    #[test]
    fn project_decision_keeps_only_redacted_fields() {
        let (store, event, public) = project_decision(&receipt(), 42).expect("project");
        let encoded = serde_json::to_string(&(store, event, public)).expect("serialize");
        assert!(encoded.contains("context_ranking"));
        assert!(encoded.contains("local_selected"));
        assert!(!encoded.contains("prompt"));
        assert!(!encoded.contains("tool output"));
        assert!(!encoded.contains("recovery_ref"));
    }

    #[test]
    fn invalid_receipt_is_rejected_before_any_projection() {
        let mut receipt = receipt();
        receipt.input_digest = "raw tool output".to_string();
        assert!(project_decision(&receipt, 42).is_err());
    }
}
