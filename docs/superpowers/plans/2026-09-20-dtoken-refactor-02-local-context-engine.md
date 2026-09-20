# Deterministic Local Context Engine Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace scattered compression branches with one safe local engine that works fully without Jev.

**Architecture:** Introduce a dtoken-context-engine crate that orchestrates existing codecs through the runtime-pipeline lifecycle. The engine classifies each content unit, marks protected spans, creates a local candidate, validates invariants, prepares encrypted CCR recovery through the existing core-api CcrStore, and returns either a validated candidate or an explicit baseline decision. Provider adapters call one core-api context_runtime facade.

**Tech Stack:** Rust, existing dtoken-compress codecs, dtoken-runtime-pipeline, dtoken-types, core-api CcrStore, property tests.

**Spec:** [Approved context engine design](../specs/2026-09-19-context-engine-five-page-refactor-design.md)

## Global Constraints

- Local mode is a complete product and is the mandatory safe fallback for all Jev workflows.
- Never compress secrets, credentials, opaque/binary content, images, or content already compressed.
- Protect system instructions, recent turns, unresolved errors, tool call/result pairs, code fences, diffs, schemas, file paths, identifiers, citations, and user-pinned content.
- Candidate validation and recovery preparation happen before ReadyToSend.
- Any classifier ambiguity, candidate expansion beyond budget, validator failure, or CCR failure returns baseline with a reason code.
- OpenAI-compatible and Anthropic routes must produce equivalent decisions for equivalent normalized content.

## Review Focus

- The engine owns orchestration; codecs remain focused and provider adapters do not choose codecs.
- Generic protection logic is independent of Optical.
- Baseline output is byte-identical when optimization is skipped or fails.
- Crash/restart recovery uses the existing encrypted CCR store and deletes data according to current retention policy.
- Token claims are based on provider-observed or local exact accounting and never on image estimates.

---

### Task 1: Extract reusable protection and content inventory primitives

**Files:**
- Create: code/crates/dtoken-compress/src/protection.rs
- Create: code/crates/dtoken-compress/src/inventory.rs
- Create: code/crates/dtoken-compress/src/secret.rs
- Modify: code/crates/dtoken-compress/src/lib.rs
- Modify: code/crates/dtoken-compress/src/content_router.rs
- Modify: code/crates/dtoken-compress/src/hybrid.rs
- Create: code/crates/dtoken-compress/tests/protection_contract.rs

- [ ] Write failing tests that protect secrets, system instructions, tool pairs, code fences, unresolved errors, file paths, and citations while allowing redundant prose to be a candidate.

~~~rust
let inventory = inventory_context(&fixture);
let protected = protected_spans(&fixture, &inventory);
assert!(protected.contains_kind(ProtectedKind::SecretLike));
assert!(protected.contains_kind(ProtectedKind::ToolPair));
assert!(protected.contains_kind(ProtectedKind::UnresolvedError));
assert!(!protected.overlaps(redundant_prose_range));
~~~

- [ ] Add a byte-preservation test for image and opaque binary parts.

- [ ] Run focused tests and confirm missing modules.

~~~bash
cd code
cargo test -p dtoken-compress --test protection_contract
~~~

- [ ] Move generic secret detection, risk classification, volatility, and inventory logic out of optical into the three new modules. Keep APIs content-oriented and free of image vocabulary.

~~~rust
pub fn inventory_context(input: &[ContextUnit]) -> ContextInventory;
pub fn protected_spans(
    input: &[ContextUnit],
    inventory: &ContextInventory,
) -> ProtectedSpanSet;
pub fn contains_secret_like(bytes: &[u8]) -> bool;
~~~

- [ ] Update content_router and hybrid imports. Do not delete Optical yet; temporarily re-export moved helpers from optical if needed to keep callers compiling.

- [ ] Run dtoken-compress tests.

~~~bash
cd code
cargo test -p dtoken-compress
~~~

Expected: existing safe-baseline tests and the new protection contract pass.

- [ ] Audit and commit.

Commit: refactor(compress): extract generic context protection

### Task 2: Add the dtoken-context-engine crate and deterministic planner

**Files:**
- Modify: code/Cargo.toml
- Create: code/crates/dtoken-context-engine/Cargo.toml
- Create: code/crates/dtoken-context-engine/src/lib.rs
- Create: code/crates/dtoken-context-engine/src/classify.rs
- Create: code/crates/dtoken-context-engine/src/local.rs
- Create: code/crates/dtoken-context-engine/src/validate.rs
- Create: code/crates/dtoken-context-engine/tests/local_engine.rs
- Create: code/crates/dtoken-context-engine/tests/failure_matrix.rs

- [ ] Write a failing public-contract test for the engine.

~~~rust
let result = ContextEngine::local(Default::default()).prepare(ContextInput {
    request_id: "req-1".into(),
    scope: "project-a".into(),
    budget: ContextBudget::tokens(8_000),
    units: fixture_units(),
})?;

match result {
    ContextOutcome::Candidate(candidate) => {
        assert!(candidate.validation.is_valid());
        assert_eq!(candidate.decision.source, DecisionSource::Local);
    }
    ContextOutcome::Baseline(_) => panic!("safe fixture should produce a candidate"),
}
~~~

- [ ] Run the crate test and confirm the crate is absent.

~~~bash
cd code
cargo test -p dtoken-context-engine
~~~

- [ ] Add the crate to the workspace and implement the public engine boundary.

~~~rust
#[async_trait]
pub trait DecisionLayer: Send + Sync {
    async fn decide(
        &self,
        input: &DecisionInput,
    ) -> Result<DecisionOutput, DecisionError>;
}

pub struct ContextEngine<D: DecisionLayer> {
    decision_layer: D,
    registry: ProductionCompressorRegistry,
    policy: LocalContextPolicy,
}

pub enum ContextOutcome {
    Candidate(PreparedContextCandidate),
    Baseline(BaselineContext),
}
~~~

- [ ] Implement classification for prose, code, diff, tool schema, tool result, mixed text, image, and opaque content. Route to existing codecs through ProductionCompressorRegistry.

- [ ] Implement deterministic local selection using stable ordering: protected status, recency, unresolved dependencies, structural role, local relevance signals, then original position as tie-breaker.

- [ ] Implement validation:
  - protected bytes and ordering unchanged;
  - tool call/result references remain paired;
  - code/diff/schema decodes or validates;
  - candidate is smaller than baseline by configured minimum;
  - candidate stays within budget;
  - manifests cover every changed range;
  - no unsupported content kind changed.

- [ ] Add the failure matrix: empty input, invalid UTF-8 in text part, all-protected input, candidate expansion, codec error, validator error, already-compressed input, image-only input, mixed image/text, and maximum configured size.

- [ ] Add deterministic property tests: same normalized input/config yields the same digests, selection, manifest, and reason codes.

- [ ] Run crate tests and clippy.

~~~bash
cd code
cargo fmt --all -- --check
cargo clippy -p dtoken-context-engine --all-targets -- -D warnings
cargo test -p dtoken-context-engine
~~~

Expected: all pass.

- [ ] Audit and commit.

Commit: feat(context): add deterministic local context engine

### Task 3: Integrate encrypted recovery with the existing CCR store

**Files:**
- Create: code/crates/dtoken-core-api/src/context_recovery.rs
- Modify: code/crates/dtoken-core-api/src/ccr_store.rs
- Modify: code/crates/dtoken-core-api/src/mcp.rs
- Create: code/crates/dtoken-core-api/tests/context_recovery.rs

- [ ] Add a failing test proving candidate preparation cannot reach ReadyToSend before a valid ccr: recovery reference exists.

~~~rust
let candidate = engine.prepare(input)?.expect_candidate();
let error = recovery.prepare(candidate, &failing_store).unwrap_err();
assert_eq!(error.reason_code(), "recovery_prepare_failed");
assert_eq!(error.into_baseline().body(), original_body);
~~~

- [ ] Add restart tests: store encrypted recovery bytes, reopen the store, retrieve through the existing capability-bound MCP tool, verify digest, delete, and confirm absence.

- [ ] Run focused tests.

~~~bash
cd code
cargo test -p dtoken-core-api --test context_recovery
~~~

- [ ] Implement ContextRecoveryCoordinator as a thin adapter over CcrStore. Do not create a second payload store.

~~~rust
pub trait RecoveryStore {
    fn store_context(
        &self,
        request_id: &str,
        original: &[u8],
        ttl: Duration,
    ) -> Result<RecoveryPrepareReceipt>;
}
~~~

- [ ] Store only the opaque ccr: reference and metadata in decisions/receipts. Preserve the existing capability check for retrieve/delete.

- [ ] Add TTL expiry, corrupted ciphertext, wrong key, missing reference, duplicate request, and delete-after-expiry tests.

- [ ] Run CCR and MCP tests.

~~~bash
cd code
cargo test -p dtoken-core-api --test context_recovery
cargo test -p dtoken-core-api mcp
~~~

Expected: all pass without raw recovery bytes in output.

- [ ] Audit and commit.

Commit: feat(recovery): prepare context candidates through encrypted CCR

### Task 4: Create one core-api context runtime facade

**Files:**
- Create: code/crates/dtoken-core-api/src/context_runtime.rs
- Modify: code/crates/dtoken-core-api/src/lib.rs
- Modify: code/crates/dtoken-core-api/Cargo.toml
- Create: code/crates/dtoken-core-api/tests/context_runtime.rs

- [ ] Add a failing test for the full local lifecycle.

~~~rust
let prepared = runtime.prepare(request).await?;
assert_eq!(prepared.decision().source, DecisionSource::Local);
assert!(prepared.recovery_ref().starts_with("ccr:"));

let ready = prepared.validate()?.into_ready_to_send();
let receipt = ready.mark_committed(provider_receipt)?;
assert_eq!(receipt.request_id(), "req-1");
~~~

- [ ] Add a baseline test for every failure point and assert exact original bytes.

- [ ] Implement ContextRuntime with dependencies injected through AppState: ContextEngine, ContextRecoveryCoordinator, ContextStore, and Event Ledger writer.

- [ ] Keep lib.rs entry points as delegation only. All new orchestration lives in context_runtime.rs.

- [ ] Record decision metadata only after candidate validation; record send commit only after the provider accepts the request.

- [ ] Run focused core-api tests.

~~~bash
cd code
cargo test -p dtoken-core-api --test context_runtime
cargo test -p dtoken-core-api --test compress_gateway
~~~

Expected: lifecycle tests pass and pre-existing gateway tests remain unchanged.

- [ ] Audit and commit.

Commit: feat(runtime): add context engine facade

### Task 5: Route OpenAI-compatible and Anthropic traffic through the local engine

**Files:**
- Modify: code/crates/dtoken-core-api/src/lib.rs
- Modify: code/crates/dtoken-core-api/tests/compress_gateway.rs
- Modify: code/crates/dtoken-proxy/src/main.rs
- Create: code/crates/dtoken-core-api/tests/provider_context_parity.rs

- [ ] Add failing parity fixtures for equivalent OpenAI-compatible and Anthropic conversations. Assert equal normalized workflow decisions, protected spans, fallback reasons, and accounting.

- [ ] Add byte-identical pass-through tests for image/multimodal parts, unsupported payloads, disabled optimization, and engine failure.

- [ ] Run focused tests and capture the expected old split-path mismatch.

~~~bash
cd code
cargo test -p dtoken-core-api --test provider_context_parity
~~~

- [ ] Replace direct codec/Optical selection in compress_for_route*, anthropic_route_payload, and prepare_direct_anthropic_hybrid with calls to ContextRuntime.

- [ ] Keep provider serialization in provider-specific adapters. Normalize into ContextInput before decision and serialize the chosen body after validation.

- [ ] Ensure a local-engine error never turns a valid provider request into a d-token HTTP failure; return baseline and record the redacted reason.

- [ ] Run route tests.

~~~bash
cd code
cargo test -p dtoken-core-api --test provider_context_parity
cargo test -p dtoken-core-api --test compress_gateway
cargo test -p dtoken-proxy
~~~

Expected: all pass; image bytes and baseline bodies are unchanged.

- [ ] Run phase verification.

~~~bash
cd code
cargo fmt --all -- --check
cargo clippy -p dtoken-compress -p dtoken-context-engine -p dtoken-core-api -p dtoken-proxy --all-targets -- -D warnings
cargo test -p dtoken-compress
cargo test -p dtoken-context-engine
cargo test -p dtoken-core-api
cargo test -p dtoken-proxy
~~~

- [ ] Audit and commit.

Commit: feat(runtime): use local context engine for provider requests
