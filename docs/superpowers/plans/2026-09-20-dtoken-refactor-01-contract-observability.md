# Contract and Observability Foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add stable, redacted contracts for local and Jev decisions before changing request behavior.

**Architecture:** Extend dtoken-types with provider-neutral decision records, persist redacted projections in dtoken-context-store schema v8, and project them through dtoken-core-api into the existing OptimizationReceipt and analytics surfaces. This phase records no raw content and does not alter which request body is sent.

**Tech Stack:** Rust, Serde, UUID, SQLite/rusqlite, existing Event Ledger and CCR metadata projections.

**Spec:** [Approved context engine design](../specs/2026-09-19-context-engine-five-page-refactor-design.md)

## Global Constraints

- Keep all existing request transformation behavior unchanged in this phase.
- Store digests, byte/token counts, enum values, latency, confidence bands, and reason codes; never store candidate text.
- Keep historical LayeredStage Optical enum values readable.
- New serialized contracts require explicit schema versions and contract-freeze tests.
- Reuse request_id, trace_id, and recovery_ref as correlation keys.

## Review Focus

- A serialized decision can be read after restart and correlated to one request without revealing its content.
- Unknown enum values or schema versions fail explicitly rather than being interpreted as success.
- Every fallback has a reason code and every decision source is local, Jev, sticky replay, or baseline.
- Existing logs created before schema v8 still migrate and query correctly.

---

### Task 1: Freeze decision contracts in dtoken-types

**Files:**
- Modify: code/crates/dtoken-types/src/lib.rs
- Modify: code/crates/dtoken-types/tests/contract_freeze.rs
- Create: code/crates/dtoken-types/tests/context_decision_contract.rs

- [ ] Add a failing round-trip test for the decision contract.

~~~rust
use dtoken_types::{
    ConfidenceBand, ContextDecisionReceipt, DecisionFallback, DecisionSource,
    DecisionWorkflow,
};

#[test]
fn context_decision_receipt_round_trips_without_content() {
    let receipt = ContextDecisionReceipt::new(
        "req-1",
        DecisionWorkflow::ToolOutputRetention,
        DecisionSource::Local,
        ConfidenceBand::High,
        14,
    )
    .with_fallback(DecisionFallback::NotRequired);

    let json = serde_json::to_string(&receipt).unwrap();
    assert!(!json.contains("secret output"));
    assert_eq!(
        serde_json::from_str::<ContextDecisionReceipt>(&json).unwrap(),
        receipt
    );
}
~~~

- [ ] Run the focused test and verify it fails because the new types do not exist.

~~~bash
cd code
cargo test -p dtoken-types --test context_decision_contract
~~~

Expected: compilation fails on unresolved decision contract imports.

- [ ] Implement the typed contract with explicit serialized names.

~~~rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecisionWorkflow {
    ToolOutputRetention,
    ArrivalSpanRetention,
    SessionCompaction,
    ContextRanking,
    RuleSkillSelection,
    MemoryWriteQualification,
    CompletionVerification,
    DiagnosticPriority,
    ToolActionReview,
    ModelEffortRouting,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecisionSource {
    Baseline,
    Local,
    Jev,
    StickyReplay,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextDecisionReceipt {
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
}
~~~

The five reserved workflow values are contract-only. Do not add an executor for them.

- [ ] Extend OptimizationReceipt with optional redacted decision fields rather than a second analytics model.

~~~rust
pub decision: Option<ContextDecisionReceipt>,
pub decision_recovery_ref: Option<String>,
pub decision_validation_status: Option<ValidationStatus>,
~~~

- [ ] Add contract-freeze assertions for exact JSON field and enum names, schema_version = 1, and absence of content-bearing fields.

- [ ] Run the type tests.

~~~bash
cd code
cargo test -p dtoken-types --test context_decision_contract
cargo test -p dtoken-types --test contract_freeze
~~~

Expected: both pass.

- [ ] Audit and commit only the type contract files plus support/git-operations.json.

Commit: feat(types): add context decision contracts

### Task 2: Add schema v8 redacted decision projections

**Files:**
- Modify: code/crates/dtoken-context-store/src/lib.rs
- Modify: code/crates/dtoken-context-store/tests/migrations.rs
- Create: code/crates/dtoken-context-store/tests/decision_projections.rs

- [ ] Add migration tests that open schema v7 fixtures, migrate to v8, and assert the two new tables and indexes.

Expected tables:

~~~sql
CREATE TABLE decision_receipts (
  request_id TEXT NOT NULL,
  workflow TEXT NOT NULL,
  source TEXT NOT NULL,
  confidence_band TEXT NOT NULL,
  latency_ms INTEGER NOT NULL,
  input_digest TEXT NOT NULL,
  candidate_digest TEXT,
  selected_units INTEGER NOT NULL,
  dropped_units INTEGER NOT NULL,
  fallback TEXT NOT NULL,
  reason_code TEXT NOT NULL,
  created_at_ms INTEGER NOT NULL,
  PRIMARY KEY (request_id, workflow)
);

CREATE TABLE sticky_decisions (
  scope TEXT NOT NULL,
  workflow TEXT NOT NULL,
  source_digest TEXT NOT NULL,
  decision_digest TEXT NOT NULL,
  expires_at_ms INTEGER NOT NULL,
  created_at_ms INTEGER NOT NULL,
  PRIMARY KEY (scope, workflow, source_digest)
);
~~~

- [ ] Run migration tests and verify the expected schema-version failure.

~~~bash
cd code
cargo test -p dtoken-context-store --test migrations
cargo test -p dtoken-context-store --test decision_projections
~~~

Expected before implementation: tests fail because SCHEMA_VERSION is 7 and tables are absent.

- [ ] Raise SCHEMA_VERSION to 8 and implement one transaction-safe migration.

- [ ] Add DecisionProjection and StickyDecisionProjection APIs:

~~~rust
pub fn record_decision(&self, projection: &DecisionProjection) -> Result<()>;
pub fn decision_for(
    &self,
    request_id: &str,
    workflow: DecisionWorkflow,
) -> Result<Option<DecisionProjection>>;
pub fn put_sticky_decision(&self, projection: &StickyDecisionProjection) -> Result<()>;
pub fn get_live_sticky_decision(
    &self,
    scope: &str,
    workflow: DecisionWorkflow,
    source_digest: &str,
    now_ms: u64,
) -> Result<Option<StickyDecisionProjection>>;
pub fn purge_expired_sticky_decisions(&self, now_ms: u64) -> Result<u64>;
~~~

- [ ] Add invalid-input tests for empty digests, empty reason codes, expired rows, duplicate upserts, and values that exceed SQLite signed integer range.

- [ ] Add a privacy test that scans every decision table column and stored value for a sentinel raw payload; it must find none.

- [ ] Run all store tests.

~~~bash
cd code
cargo test -p dtoken-context-store
~~~

Expected: all pass, including migrations from every supported prior schema.

- [ ] Audit and commit the storage batch.

Commit: feat(store): persist redacted decision projections

### Task 3: Project decisions through core-api and Event Ledger

**Files:**
- Create: code/crates/dtoken-core-api/src/decision_projection.rs
- Modify: code/crates/dtoken-core-api/src/lib.rs
- Modify: code/crates/dtoken-core-api/tests/compress_gateway.rs
- Modify: code/crates/dtoken-event-ledger/src/lib.rs
- Create: code/crates/dtoken-event-ledger/tests/context_decision_events.rs

- [ ] Add failing tests for a redacted ContextDecisionRecorded event and OptimizationReceipt projection.

~~~rust
let event = ContextDecisionRecorded::from_receipt(&decision);
let json = serde_json::to_string(&event).unwrap();
assert!(json.contains(r#""workflow":"tool_output_retention""#));
assert!(!json.contains("sentinel raw tool output"));
~~~

- [ ] Run focused tests and confirm the new module/event is missing.

~~~bash
cd code
cargo test -p dtoken-event-ledger --test context_decision_events
cargo test -p dtoken-core-api --test compress_gateway decision_projection
~~~

- [ ] Implement decision_projection.rs as the only mapping between runtime receipts, SQLite projections, Event Ledger events, and public OptimizationReceipt fields.

- [ ] Add a producer-neutral ledger event carrying request_id, workflow, source, confidence band, latency, fallback, reason, validation status, and recovery presence.

- [ ] Preserve old Optical LayeredStage deserialization. Add a test that reads an old Optical event but prove no new decision projection writes an Optical stage.

- [ ] Add projection tests for local, Jev, sticky replay, timeout fallback, low-confidence fallback, validation failure, and baseline.

- [ ] Run the affected crates.

~~~bash
cd code
cargo test -p dtoken-event-ledger
cargo test -p dtoken-core-api --test compress_gateway
~~~

Expected: all pass with unchanged outbound request bodies.

- [ ] Audit and commit the projection batch.

Commit: feat(observability): project context decisions without raw content

### Task 4: Document the frozen contract and verify no behavior change

**Files:**
- Modify: docs/ARCHITECTURE.md
- Modify: docs/SECURITY_BOUNDARIES.md
- Modify: docs/product/IMPLEMENTATION_STATUS.md
- Modify: docs/index.md
- Modify: support/scripts/check_structure.py

- [ ] Document the decision lifecycle and explicitly state that phase 1 adds observability contracts only.

- [ ] Document permitted and forbidden decision fields, sticky-decision TTL behavior, and CCR correlation.

- [ ] Add links and required-path entries for any new active contract document if one is created.

- [ ] Run phase verification.

~~~bash
python3 support/scripts/check_structure.py
cd code
cargo fmt --all -- --check
cargo clippy -p dtoken-types -p dtoken-context-store -p dtoken-event-ledger -p dtoken-core-api --all-targets -- -D warnings
cargo test -p dtoken-types
cargo test -p dtoken-context-store
cargo test -p dtoken-event-ledger
cargo test -p dtoken-core-api --test compress_gateway
~~~

Expected: all commands pass; existing baseline payload assertions remain byte-for-byte unchanged.

- [ ] Inspect staged content for forbidden payloads and unrelated worktree changes.

~~~bash
git diff --cached --check
git diff --cached --name-only
rg -n "Authorization:|api[_-]?key|secret output|sentinel raw" \
  code/crates/dtoken-context-store code/crates/dtoken-event-ledger
~~~

Expected: no credential or raw-content fixture appears in production storage/logging code.

- [ ] Audit and commit documentation.

Commit: docs(context): define decision observability contract
