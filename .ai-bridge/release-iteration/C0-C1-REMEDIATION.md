# C0-C1 Remediation Report (T-C0A / T-C0B / T-C1A / T-C1B / T-C1C)

Candidate: `15f98cf` · Worktree: `final-c0-c1-remediation` · Date: 2026-08-10 · Owner: C0C1Remediation

## Scope

Canonical C0/C1 contracts across `dtoken-types`, `dtoken-router`, `dtoken-runtime-pipeline`, `dtoken-core-api` and their targeted tests. C2-C8 stage algorithms are out of scope. Every task below maps to concrete symbols and tests; no boundary/deferred/no-op substitutions.

---

## T-C0A schemas-policy — canonical IR, NodeAction, manifest, RouteDecision digest, global safety policy, four-dimension accounting

| Requirement | Symbol delivered | Test |
|---|---|---|
| Exclusive canonical NodeAction/IR | `dtoken_types::contract::{IrNode, NodePlan, StageArtifact, OptimizationManifest, BoundRequest}`; `dtoken_types::NodeAction` re-exported as `contract::NodeAction` (5 exclusive actions) | `dtoken-types/src/contract.rs` tests: `each_ir_node_carries_exactly_one_exclusive_action`, `node_plan_rejects_empty_and_duplicate_nodes`, `stage_artifact_must_belong_to_a_planned_node`, `optimization_manifest_binds_request_plan_and_stages` |
| RouteDecision stable digest (single authority) | `dtoken_types::RouteDecision::stable_digest()` (sha256 over canonical identity; `reason` excluded) | `route_decision_stable_digest_is_deterministic_and_excludes_reason` (dtoken-types) |
| Global SafetyPolicy (PRODUCT-8: default safe_lossless; enhanced/lossy require explicit confirmation; no engine chooser) | `dtoken_types::contract::SafetyPolicy` with `default/enhanced/lossy/requires_explicit_confirmation/from_compression_policy` | `safety_policy_default_is_safe_lossless_and_never_requires_confirmation`, `safety_policy_classifies_compression_policy_once` |
| Four-dimension accounting (RUNTIME-5/9: never summed) | `dtoken_types::contract::AccountingBuckets` — per-dimension reads only, no total | `accounting_buckets_are_independent_and_never_summed` |
| OptimizationReceipt (T-PR1 minimal receipt; Local/Provider/claimable separation) | `dtoken_types::OptimizationReceipt::from_request_log` — claimable only with paired Provider evidence | `types.optimization-receipt` registry pointer; covered by types unit build |

## T-C0B sealed-receipts — unforgeable Validation/Recovery/SendCommit receipts

| Requirement | Symbol | Test |
|---|---|---|
| Sealed terminal receipt, transport-observable | `dtoken_runtime_pipeline::SendCommitReceipt` (private fields; minted only by `ReadyToSend::mint_send_commit` which consumes the unit) | `post_commit_receipt_is_terminal_and_carries_the_optimized_evidence`, `one_terminal_receipt_per_request_is_structural`, `send_commit_receipt_binds_the_exact_sent_body_digest` (lifecycle.rs) |
| Sealed validation receipt bound to validated digest | `dtoken_runtime_pipeline::ValidationReceipt` minted by `ValidatedRequest::mint_validation_receipt` | `validation_receipt_is_minted_only_after_validation_and_binds_the_validated_digest`, `validation_and_commit_receipts_carry_the_same_sent_digest_for_optimized_units` |
| Recovery receipt binds request/source/decision + durable store; old `ccr:` prefix-sentinel forgery dead | `dtoken_runtime_pipeline::RecoveryReceipt` — `mint` is `pub(crate)`; public flow is `PreparedRequest::with_durable_candidate`; non-durable refs/expiry=0/foreign bindings rejected | `recovery_receipt_rejects_non_durable_refs_and_forged_bindings`, `recovery_receipt_expiry_is_enforced`, `optimized_send_requires_matching_source_validation_and_recovery_receipt` |
| Pre/post-commit failure semantics (Main review: commit receipt only at transport-observable point) | `ReadyToSend::mint_send_commit(0)` = `FailedBeforeCommit`; `mint_transport_send_commit` in core-api called only after reqwest accepted the body | `pre_send_fault_marks_failed_before_commit_and_keeps_the_original_variant`, `post_commit_is_terminal_no_retry_no_second_receipt`, `pre_send_fault_is_terminal_without_a_send` (recovery_state_machine.rs) |
| No caller can construct Optimized/Committed directly (QUALITY-7) | All receipt fields private; `RecoveryReceipt::mint` `pub(crate)`; `SendCommitReceipt`/`ValidationReceipt` have no public constructors | compile-fail-proof by construction + behavioral negatives above |

## T-C1A route-lowering — RouteDecision 唯一权威; 三协议 parse/lower once

| Requirement | Symbol | Test |
|---|---|---|
| Single route-decision authority | `dtoken_router::Router::decide` / `decide_for_request` (ambiguity fail-closed); `dtoken_types::RouteDecision` consumed everywhere via `stable_digest` | `decides_route_by_instance_and_protocol`, `rejects_ambiguous_routes_instead_of_selecting_first` (dtoken-router/tests/route_decision.rs) |
| Chat / Responses / Anthropic / MCP / TS / Proxy converge on one parse/lower/plan/validate/send path | `dtoken_core_api::prepare_json_pipeline_payload` (now `pub`); all three HTTP handlers + MCP + Proxy host the same sealed payload flow; memory injection pre-plan; wire body read from `ready.body()` | `responses_http_non_stream_returns_responses_output_from_chat_upstream`, `responses_http_stream_converts_chat_sse_to_responses_sse`, `native_responses_passthrough_seals_pipeline_payload_and_records_resolved_adapter` (wire body == validated body), `compression_decision_matches_http_and_mcp_surfaces` |

## T-C1B reuse-before-plan / send digest == validation digest

| Requirement | Symbol | Test |
|---|---|---|
| No post-validation mutation; production send body equals validated body | Wire body deserialized from sealed `ReadyToSend.body()`; `mint_transport_send_commit` compares `ValidationReceipt.validated_digest` vs `ReadyToSend.body_digest()` and aborts (500 `validated_body_mismatch`) on mismatch | `transport_mint_rejects_post_validation_mutation` (fail-closed digest refusal, no receipt evidence recorded); `native_responses_passthrough...` wire-capture equality |

## T-C1C single-dispatch — transport consumes SendCommitReceipt

| Requirement | Symbol | Test |
|---|---|---|
| Production transport consumes terminal SendCommitReceipt (RUNTIME-8; pending marker/no-op not enough) | `mint_transport_send_commit` called at all 12 production send points: OpenAI Chat (stream Ok/Err + non-stream), Native Responses (stream Ok/Err + non-stream), Responses→Chat bridge (stream Ok/Err + non-stream), Direct Anthropic (stream Ok/Err + non-stream). Receipt evidence (state/variant/digest/reason) lands in the Event Ledger metadata. | `native_responses_passthrough...` asserts `send_commit_state == send_committed` + `send_commit_variant` in ledger; lifecycle/recovery tests assert one-terminal-receipt semantics |
| Native Responses cannot bypass canonical pipeline (AGENT-10) | `openai_responses` (now `pub`) seals through `prepare_json_pipeline_payload` on every branch incl. Preserve | `native_responses_passthrough_seals_pipeline_payload_and_records_resolved_adapter` |
| Registry symbols machine-resolvable (C0/C1-owned entries) | `support/fixtures/unified-capability-registry-v1.json`: contract.*, aggregate.*, receipt.*, pipeline.send-commit-receipt, router.route-decision-digest, types.optimization-receipt, core-api.{prepare-json-pipeline-payload, native-responses, export-redaction} now point to the canonical symbols above | `python3 support/scripts/validate_unified_capability_registry.py` → "registry contract OK: exactly 117 unique complete entries" |
| Public redacted export + metrics symbols for registry | `dtoken_core_api::export_runtime_summary`, `dtoken_core_api::metrics` (pub) | covered by core-api lib tests |

## Verification evidence (targeted, no project-wide suites)

```
cargo test -p dtoken-types          -> 18 lib + 8 integration + 1 (optical) PASS
cargo test -p dtoken-router         -> 2 PASS
cargo test -p dtoken-runtime-pipeline -> 12 lifecycle + 6 recovery + 9 scheduler PASS
cargo test -p dtoken-core-api       -> 176 lib + 2 compress_gateway + 9 optical_phase_b + 18 surface_parity PASS
support/scripts/validate_unified_capability_registry.py -> OK (117 entries)
```

## Notes / boundaries

- `c6.bounded-concurrency` (`AppState::route_request`), `c2.ccr-memory-learning` (`CcrStoreReceipt`), `core-api.ccr-store`, `c6.ccr-full-durability` registry entries are owned by C6C7/C2C3 slices, not fixed here.
- `RouteDecision` fields remain public struct fields (core-api has ~130 accesses); the canonical construction authority is `dtoken_router::Router` and the digest is the sealed evidence binding. Full field sealing is a follow-up if reviewers require it.
- Scheduler re-export block intentionally matches the C6C7 baseline (no `AdmissionOutcome`/`StableSequence` in this worktree); combined commit takes C6C7's block.
