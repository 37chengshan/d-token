# Jev Real-Time Context Workflows Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add Jev as an optional, bounded decision layer for five real-time context workflows while preserving a complete local-only product.

**Architecture:** Add a dtoken-decision crate containing workflow policies, a strict Jev client, response linting, sticky decision replay, and per-workflow circuit breakers. The context engine calls it at tool arrival, turn boundaries, pressure compaction, and selected ranking points. Provider-send paths normally replay prior sticky decisions. Every workflow owns its mode, timeout, confidence threshold, fallback, metrics, and kill switch.

**Tech Stack:** Rust, Tokio, Reqwest/Rustls, Serde, OS credential store through Tauri, SQLite sticky projections, wiremock-style local test server.

**Spec:** [Approved context engine design](../specs/2026-09-19-context-engine-five-page-refactor-design.md)

## Global Constraints

- No credential means Local mode, not a degraded or trial state.
- Jev receives only the minimum workflow input and identifiers necessary for one decision.
- Each response must parse against a versioned schema, cover all required units, stay within bounds, and pass local validation.
- Timeouts, HTTP errors, schema errors, low confidence, incomplete coverage, stale sticky state, and open circuit breakers fall back per workflow.
- Reserved workflows are serializable but have no dispatch mapping.
- Ordinary provider requests must not trigger one Jev request per message; arrival and turn-boundary decisions are reused by digest and TTL.
- Never log request bodies, Jev payloads, response bodies, or credentials.

## Review Focus

- Removing the key while the app runs leaves the next request safe and usable.
- A failure in one workflow does not disable the other four.
- Jev cannot directly produce a provider request body; it returns structured decisions that the local engine applies and validates.
- Sticky decisions are scoped, content-addressed, expiring, and invalidated by config/policy changes.
- Shadow mode records comparison evidence but never changes outbound content.

---

### Task 1: Add workflow policy and strict Jev response contracts

**Files:**
- Modify: code/Cargo.toml
- Create: code/crates/dtoken-decision/Cargo.toml
- Create: code/crates/dtoken-decision/src/lib.rs
- Create: code/crates/dtoken-decision/src/policy.rs
- Create: code/crates/dtoken-decision/src/contract.rs
- Create: code/crates/dtoken-decision/tests/contracts.rs
- Modify: code/crates/dtoken-config/src/lib.rs
- Create: code/crates/dtoken-config/tests/decision_config.rs

- [ ] Add failing tests for five executable workflows and five reserved workflows.

~~~rust
assert!(DecisionWorkflow::ToolOutputRetention.is_executable());
assert!(DecisionWorkflow::RuleSkillSelection.is_executable());
assert!(!DecisionWorkflow::MemoryWriteQualification.is_executable());
assert!(dispatch_for(DecisionWorkflow::ToolActionReview).is_none());
~~~

- [ ] Add a failing config test for independent workflow controls.

~~~rust
pub struct WorkflowPolicy {
    pub mode: WorkflowMode,
    pub timeout_ms: u64,
    pub minimum_confidence: f32,
    pub sticky_ttl_ms: u64,
    pub fallback: WorkflowFallback,
    pub kill_switch: bool,
}
~~~

- [ ] Run the tests and confirm missing crate/config fields.

~~~bash
cd code
cargo test -p dtoken-decision
cargo test -p dtoken-config --test decision_config
~~~

- [ ] Implement DecisionRuntimeConfig with global enabled, credential_ref, endpoint, total concurrency, and a map containing exactly the five executable workflow policies.

- [ ] Set safe defaults:
  - no credential: Local;
  - new credential: Shadow until explicitly changed;
  - kill_switch true: Local immediately;
  - timeout range: 50–2000 ms;
  - confidence range: 0.0–1.0;
  - endpoint must be HTTPS except an explicit loopback test endpoint in debug builds.

- [ ] Reject config that enables reserved workflows, contains a plaintext key, uses an unknown mode, or sets fallback to accept an invalid Jev result.

- [ ] Implement versioned request/response DTOs using digests and bounded metadata. A selection response must identify every selected unit by opaque unit_id and cannot carry replacement prose.

- [ ] Run tests and clippy.

~~~bash
cd code
cargo fmt --all -- --check
cargo clippy -p dtoken-decision -p dtoken-config --all-targets -- -D warnings
cargo test -p dtoken-decision
cargo test -p dtoken-config
~~~

- [ ] Audit and commit.

Commit: feat(jev): define workflow policies and contracts

### Task 2: Implement the bounded Jev client, linter, and circuit breakers

**Files:**
- Create: code/crates/dtoken-decision/src/jev_client.rs
- Create: code/crates/dtoken-decision/src/lint.rs
- Create: code/crates/dtoken-decision/src/circuit_breaker.rs
- Create: code/crates/dtoken-decision/tests/jev_client.rs
- Create: code/crates/dtoken-decision/tests/failure_isolation.rs

- [ ] Add test-server cases for success, timeout, 429, 500, malformed JSON, unknown schema, duplicate unit IDs, missing coverage, out-of-range score, excessive response size, and connection reset.

- [ ] Add a privacy assertion that captured logs contain only workflow, digest, timing, status class, and reason code.

- [ ] Run focused tests and verify failure.

~~~bash
cd code
cargo test -p dtoken-decision --test jev_client
cargo test -p dtoken-decision --test failure_isolation
~~~

- [ ] Implement the client boundary.

~~~rust
#[async_trait]
pub trait RealtimeDecisionClient: Send + Sync {
    async fn decide(
        &self,
        workflow: DecisionWorkflow,
        input: &DecisionInput,
        deadline: Instant,
    ) -> Result<LintedDecision, DecisionFailure>;
}
~~~

- [ ] Enforce request/response byte limits, one deadline, Rustls certificate validation, no redirects, and Authorization construction inside the client from an in-memory secret.

- [ ] Implement linting before confidence evaluation. Required order:
  1. transport status;
  2. response size;
  3. JSON/schema version;
  4. workflow match;
  5. digest match;
  6. complete unique unit coverage;
  7. numeric bounds;
  8. minimum confidence;
  9. local policy and protection validation.

- [ ] Add one circuit breaker per workflow. Opening tool_output_retention must not alter context_ranking state. Half-open probes are bounded to one in flight.

- [ ] Run tests.

~~~bash
cd code
cargo test -p dtoken-decision
~~~

Expected: all cases return typed outcomes and no panic or raw body logging.

- [ ] Audit and commit.

Commit: feat(jev): add bounded client and failure isolation

### Task 3: Add Jev credential storage and health controls

**Files:**
- Create: code/apps/desktop/src-tauri/src/decision_control.rs
- Modify: code/apps/desktop/src-tauri/src/main.rs
- Modify: code/apps/desktop/src-tauri/Cargo.toml
- Create: code/apps/desktop/src-tauri/tests/decision_credentials.rs
- Modify: code/crates/dtoken-doctor/src/lib.rs
- Create: code/crates/dtoken-doctor/tests/decision_health.rs

- [ ] Add failing command tests for store, replace, delete, missing credential, locked keychain, and redacted status.

~~~rust
assert_eq!(status.credential_state, CredentialState::Available);
assert!(status.credential_ref.starts_with("os-credential:"));
assert!(!serde_json::to_string(&status).unwrap().contains("jev_test_key"));
~~~

- [ ] Implement the keyring identity d-token/decision/typesafe-jev. Persist only the opaque credential_ref in DtokenConfig.

- [ ] Add Tauri commands:
  - get_decision_config
  - preview_decision_config_change
  - apply_decision_config_change
  - rollback_decision_config_change
  - store_decision_credential
  - delete_decision_credential
  - test_decision_connection
  - get_decision_health

- [ ] Use the existing config write contract: readable summary, diff, backup, authorization, verify, and rollback evidence.

- [ ] Add Doctor checks for configured/no-key, keychain locked, endpoint invalid, connection timeout, auth rejection, schema mismatch, circuit open, and stale sticky state. Health checks must not send user context.

- [ ] Run native and Doctor tests.

~~~bash
cd code
cargo test -p dtoken-doctor
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml decision
~~~

Expected: credential values never appear in command responses or logs.

- [ ] Audit and commit.

Commit: feat(desktop): add Jev credential and health controls

### Task 4: Implement tool-output and arrival-span retention

**Files:**
- Create: code/crates/dtoken-context-engine/src/realtime.rs
- Create: code/crates/dtoken-context-engine/src/workflows/tool_output.rs
- Create: code/crates/dtoken-context-engine/src/workflows/arrival_span.rs
- Create: code/crates/dtoken-context-engine/tests/realtime_arrival.rs
- Modify: code/crates/dtoken-core-api/src/context_runtime.rs
- Create: code/crates/dtoken-core-api/tests/jev_arrival_workflows.rs

- [ ] Add failing tests for tool completion and message arrival hooks. Verify one Jev call, a stored sticky decision, and replay without a second call.

~~~rust
runtime.on_tool_output(tool_event).await?;
runtime.prepare(next_provider_request).await?;
assert_eq!(jev_server.request_count(), 1);
assert_eq!(receipt.source, DecisionSource::StickyReplay);
~~~

- [ ] Add timeout, low-confidence, partial coverage, changed digest, expired TTL, kill switch, and no-key cases. Each must use local fallback and keep the request usable.

- [ ] Implement workflow inputs as opaque unit IDs plus bounded structural metadata. Archive eligible dropped tool bytes through CcrStore before a decision becomes applicable.

- [ ] Apply tool-output retention only to completed tool results; never drop the tool call, unresolved error, current-turn result, or user-pinned output.

- [ ] Apply arrival-span retention only after the full incoming span is available; preserve system/user role boundaries and exact protected spans.

- [ ] Store sticky decisions keyed by scope + workflow + source_digest + policy_digest. Invalidate when any key component changes.

- [ ] Run tests.

~~~bash
cd code
cargo test -p dtoken-context-engine --test realtime_arrival
cargo test -p dtoken-core-api --test jev_arrival_workflows
~~~

- [ ] Audit and commit.

Commit: feat(jev): add realtime arrival retention workflows

### Task 5: Implement session compaction with transactional recovery

**Files:**
- Create: code/crates/dtoken-context-engine/src/workflows/session_compaction.rs
- Create: code/crates/dtoken-context-engine/tests/session_compaction.rs
- Create: code/crates/dtoken-core-api/tests/jev_session_compaction.rs

- [ ] Add failing tests for pressure thresholds, boundary equality, all-protected sessions, multi-turn tool dependencies, compaction candidate validation, CCR failure, and restart recovery.

- [ ] Define pressure from normalized provider budget, reserved output budget, current context tokens, and a deterministic safety margin. Never use a percentage without the absolute token evidence.

~~~rust
pub struct ContextPressure {
    pub context_tokens: u64,
    pub provider_limit: u64,
    pub reserved_output_tokens: u64,
    pub safety_margin_tokens: u64,
}
~~~

- [ ] Invoke Jev only when the configured pressure threshold crosses from below to at/above, or when the source digest changes after a failed/expired decision.

- [ ] Make compaction two-phase: prepare candidate and CCR; validate; commit sticky decision; then permit provider send. Any failure returns the baseline session.

- [ ] Add crash/restart tests that prove stored sticky metadata without a live CCR cannot be replayed.

- [ ] Run tests.

~~~bash
cd code
cargo test -p dtoken-context-engine --test session_compaction
cargo test -p dtoken-core-api --test jev_session_compaction
~~~

- [ ] Audit and commit.

Commit: feat(jev): add recoverable session compaction

### Task 6: Implement context ranking and rule/skill selection in shadow-first mode

**Files:**
- Create: code/crates/dtoken-context-engine/src/workflows/context_ranking.rs
- Create: code/crates/dtoken-context-engine/src/workflows/rule_skill_selection.rs
- Create: code/crates/dtoken-context-engine/tests/ranking_workflows.rs
- Create: code/crates/dtoken-core-api/tests/jev_shadow_mode.rs

- [ ] Add tests comparing local and Jev rankings while Shadow mode leaves outbound bytes unchanged.

- [ ] Protect mandatory repository and project rules before ranking. Rule/skill selection may reorder/select optional context but cannot remove globally required or path-matched instructions.

- [ ] Require complete scoring coverage and stable tie-breaking. Equal scores use original order.

- [ ] Add an activation gate: a workflow may move from Shadow to Active only through config and only when comparison metrics meet configured minimum sample count, maximum invalid rate, maximum p95 latency, and zero protected-span violations.

- [ ] Record comparison deltas as counts, bands, and reason codes; never record the compared text.

- [ ] Run tests.

~~~bash
cd code
cargo test -p dtoken-context-engine --test ranking_workflows
cargo test -p dtoken-core-api --test jev_shadow_mode
~~~

- [ ] Audit and commit.

Commit: feat(jev): add shadow-first ranking workflows

### Task 7: Expose workflow status, analytics, and emergency disable

**Files:**
- Modify: code/crates/dtoken-core-api/src/decision_projection.rs
- Modify: code/crates/dtoken-core-api/src/lib.rs
- Modify: code/apps/desktop/src-tauri/src/decision_control.rs
- Create: code/crates/dtoken-core-api/tests/decision_analytics.rs
- Modify: docs/SECURITY_BOUNDARIES.md
- Modify: docs/product/IMPLEMENTATION_STATUS.md

- [ ] Add failing analytics tests for per-workflow call count, fallback rate, invalid rate, p50/p95 latency, sticky hit rate, circuit state, local-vs-Jev selection delta, and recovery failure count.

- [ ] Implement a global emergency disable plus independent workflow kill switches. A switch change applies to the next decision without restart and invalidates affected sticky rows.

- [ ] Add command tests proving UI status cannot report Healthy when the credential is missing, the circuit is open, or freshness is unknown.

- [ ] Run phase verification.

~~~bash
cd code
cargo fmt --all -- --check
cargo clippy -p dtoken-decision -p dtoken-context-engine -p dtoken-core-api -p dtoken-doctor --all-targets -- -D warnings
cargo test -p dtoken-decision
cargo test -p dtoken-context-engine
cargo test -p dtoken-core-api
cargo test -p dtoken-doctor
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml decision
~~~

Expected: all pass with tests explicitly covering no-key and all forced-failure modes.

- [ ] Audit and commit.

Commit: feat(jev): expose workflow evidence and kill switches
