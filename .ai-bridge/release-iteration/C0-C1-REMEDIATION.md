# C0/C1 Remediation Report — candidate 15f98cf

Branch: `codex/final/c0-c1-remediation` — commit `9722454a787fe35207cbc15df4ef81155a552a3d` (parent 15f98cf)
Scope: T-C0A (schemas-policy), T-C0B (sealed-receipts), T-C1A (route-lowering), T-C1C (single-dispatch, transport consumption)

## Canonical cross-slice types (C0/C1-owned)

| Task | Symbol | Location | Registry entry |
|---|---|---|---|
| T-C0A | `contract::NodeAction` (authority re-export) | `code/crates/dtoken-types/src/contract.rs` (`pub use crate::NodeAction`) | aggregate.headroom-semantic/reuse/rtk-command-output/optical/preserve, contract.node-action-* |
| T-C0A | `contract::SafetyPolicy` (+ `default`/`enhanced`/`lossy`/`requires_explicit_confirmation`/`from_compression_policy`) | contract.rs | aggregate.safety-policy, contract.safety-policy-* |
| T-C0A | `contract::AccountingBuckets` (four-dimension, no total) | contract.rs | aggregate.accounting, contract.accounting-bucket-* |
| T-C0A | `contract::{IrNode, BoundRequest, NodePlan, StageArtifact, OptimizationManifest}` | contract.rs | contract.ir-node, contract.bound-request, contract.node-plan, contract.stage-artifact, contract.optimization-manifest |
| T-C0A | `RouteDecision::stable_digest` (canonical, reason-excluded) | `dtoken-types/src/lib.rs` | aggregate.route-decision, router.route-decision-digest |
| T-C0A | `OptimizationReceipt::from_request_log` (PRD P0.0.7 minimal; Local/Provider/claimable split) | `dtoken-types/src/lib.rs` | types.optimization-receipt |
| T-C1A | `dtoken_router::Router::{decide, decide_for_request}` — single route-selection authority | `dtoken-router/src/lib.rs` | router.decide, router.ambiguous-reject, router.no-engine-chooser, c1.route-lowering |

## Sealed receipts (T-C0B / RUNTIME-8 / QUALITY-7)

| Symbol | Mint path | Anti-forgery |
|---|---|---|
| `ValidationReceipt` | `ValidatedRequest::mint_validation_receipt` | only after real validate(); binds exact validated candidate digest |
| `RecoveryReceipt` | `pub(crate) mint` via `PreparedRequest::with_durable_candidate` | binds request_id + source digest + decision digest + durable `ccr:` store ref; non-durable refs rejected; foreign identity rejected (`ReceiptIdentityMismatch`) |
| `SendCommitReceipt` | `ReadyToSend::mint_send_commit` (consumes unit) | one terminal receipt per request structurally; binds exact sent-body digest; `FailedBeforeCommit(0)` vs `SendCommitted` vs `FailedAfterCommit` |

Removed forgeable `RecoveryPrepareReceipt::new` and `mark_committed`; `SendReceipt` kept as deprecated alias.

## Production transport consumption (RUNTIME-8)

`dtoken-core-api` send handlers (Chat, Native Responses, Anthropic direct + bridged, streams) now own the sealed `ReadyToSend` and mint `SendCommitReceipt` at the transport-observable commit point (`mint_transport_send_commit`, 5-arg, validation-bound). Pre-commit failure → `FailedBeforeCommit` (0ms); post-commit upstream error → committed_at + `FailedAfterCommit`; stream guards mint on response start. `prepare_json_pipeline_payload` is now `pub`, decision-bound, and mints the `ValidationReceipt` per send; the wire body is read from `ready.body()` so sent == validated (no post-validation mutation). Native Responses (`/v1/responses`) flows through the same canonical pipeline (incl. Preserve).

## Tests

- `cargo test -p dtoken-types --lib` — 18 pass (contract module, digest, receipts-of-policy)
- `cargo test -p dtoken-runtime-pipeline` — lifecycle 11 + recovery_state_machine 6 + scheduler 9 pass (behavioral negatives: forged refs, foreign identity, expiry, double-mint, sent==validated digest)
- `cargo test -p dtoken-router` — 2 pass
- `cargo test -p dtoken-core-api --lib` — 175 pass (incl. ccr bare-marker guard, pipeline payload, transport commit points)
- `cargo check -p dtoken-core-api --all-targets` — clean (only pre-existing non-C0 warnings)

## Coordination

- C6C7: scheduler re-export block preserved exactly; no admission duplication.
- C2C3: RecoveryReceipt binding fields (request_id/source_digest/decision_digest) match bus RecoveryManifest binding; record site and bus modules untouched.
- C4: RTK stats/accounting untouched.
- C8: canonical symbols (contract::*, stable_digest, OptimizationReceipt, ValidationReceipt/RecoveryReceipt/SendCommitReceipt::mint, core-api prepare_json_pipeline_payload/export/metrics pub-ness) provided for registry binding; C8 owns registry edits.

## Boundaries honored

No C2-C8 stage algorithms changed; no registry rewrite (registry pointer fixes deferred to C8 binding); no project-wide validation run; loopback/secret-redaction/no-touch contracts untouched.
