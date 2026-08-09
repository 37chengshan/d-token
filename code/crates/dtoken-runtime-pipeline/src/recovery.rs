//! Per-request recovery state machine (PR-C6C).
//!
//! The lifecycle types already make most guarantees structurally (a
//! [`PreparedRequest`](crate::PreparedRequest) is consumed into exactly one
//! sendable unit, and a [`ReadyToSend`](crate::ReadyToSend) is consumed into
//! exactly one terminal receipt).  This module adds an explicit, observable
//! per-request tracker that records the recovery phase so the transport and
//! its evidence can prove the QUALITY-5 invariants at runtime:
//!
//! - a pre-commit candidate failure dispatches the baseline **exactly once**;
//! - after commit the request is terminal: no retry, no provider switch, no
//!   second receipt;
//! - every request reaches at most one terminal receipt.

use uuid::Uuid;

use crate::{PipelineError, SendReceipt};

/// Observable recovery phase of a single request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryPhase {
    /// Ingress accepted; no sendable unit has been dispatched yet.
    Preparing,
    /// A pre-commit preserve path dispatched the original baseline once.
    BaselineDispatched,
    /// An optimized candidate unit was dispatched.
    CandidateDispatched,
    /// A terminal receipt exists; this phase is absorbing.
    Terminal,
}

impl RecoveryPhase {
    pub fn is_terminal(self) -> bool {
        self == Self::Terminal
    }
}

/// Transition error returned when the machine would violate a MUST.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryTransitionError {
    /// A second baseline dispatch was attempted for the same request.
    BaselineAlreadyDispatched,
    /// A dispatch or receipt was attempted after the request was terminal.
    AlreadyTerminal,
    /// A receipt was recorded for a different request identity.
    RequestIdentityMismatch,
}

/// Per-request recovery tracker.  Cheap, cloneable, and owned by the
/// transport for the request lifetime.
#[derive(Debug, Clone)]
pub struct RecoveryTracker {
    request_id: Uuid,
    phase: RecoveryPhase,
    baseline_dispatched: bool,
}

impl RecoveryTracker {
    pub fn new(request_id: Uuid) -> Self {
        Self {
            request_id,
            phase: RecoveryPhase::Preparing,
            baseline_dispatched: false,
        }
    }

    pub fn request_id(&self) -> Uuid {
        self.request_id
    }

    pub fn phase(&self) -> RecoveryPhase {
        self.phase
    }

    /// Record that the pre-commit preserve path produced the original
    /// baseline.  At most one baseline dispatch is ever allowed; a second
    /// attempt is rejected so a fault loop cannot re-send the baseline.
    pub fn record_baseline_dispatch(&mut self) -> Result<RecoveryPhase, RecoveryTransitionError> {
        if self.phase.is_terminal() {
            return Err(RecoveryTransitionError::AlreadyTerminal);
        }
        if self.baseline_dispatched {
            return Err(RecoveryTransitionError::BaselineAlreadyDispatched);
        }
        self.baseline_dispatched = true;
        self.phase = RecoveryPhase::BaselineDispatched;
        Ok(self.phase)
    }

    /// Record that the optimized candidate unit was dispatched.
    pub fn record_candidate_dispatch(&mut self) -> Result<RecoveryPhase, RecoveryTransitionError> {
        if self.phase.is_terminal() {
            return Err(RecoveryTransitionError::AlreadyTerminal);
        }
        self.phase = RecoveryPhase::CandidateDispatched;
        Ok(self.phase)
    }

    /// Record the terminal receipt.  This is the only transition into
    /// `Terminal`, and it is absorbing: every later dispatch or receipt
    /// attempt is rejected, which makes retry-after-commit and
    /// double-receipt structurally impossible.
    ///
    /// The receipt must belong to this request; a foreign receipt is
    /// rejected so a transport can never settle another request's identity.
    pub fn record_terminal(
        &mut self,
        receipt: &SendReceipt,
    ) -> Result<RecoveryPhase, RecoveryTransitionError> {
        if self.phase.is_terminal() {
            return Err(RecoveryTransitionError::AlreadyTerminal);
        }
        if receipt.request_id() != self.request_id {
            return Err(RecoveryTransitionError::RequestIdentityMismatch);
        }
        self.phase = RecoveryPhase::Terminal;
        Ok(self.phase)
    }
}

/// Map a transition error to the shared pipeline error vocabulary.
pub fn recovery_error_to_pipeline(error: RecoveryTransitionError) -> PipelineError {
    match error {
        RecoveryTransitionError::BaselineAlreadyDispatched => {
            PipelineError::BaselineAlreadyDispatched
        }
        RecoveryTransitionError::AlreadyTerminal => PipelineError::AlreadyTerminal,
        RecoveryTransitionError::RequestIdentityMismatch => PipelineError::ReceiptIdentityMismatch,
    }
}
