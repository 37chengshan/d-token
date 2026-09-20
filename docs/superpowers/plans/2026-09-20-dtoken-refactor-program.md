# d-token Context Engine and Five-Page Refactor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the fragmented compression and desktop surfaces with one auditable local-first context engine, an optional Jev real-time decision layer, five task-oriented desktop pages, and a complete retirement path for Optical image compression.

**Architecture:** Keep the Rust-owned request pipeline as the only production path. Build the program in five sequential increments: establish durable contracts and observability, complete the deterministic local engine, add isolated Jev workflows, converge the React/Tauri UI to five pages, then remove Optical producers and close release gates. Every increment must remain usable without a Jev key and must preserve baseline forwarding when an optimization cannot be proven safe.

**Tech Stack:** Rust workspace, Tokio/Reqwest, Serde, SQLite, Tauri 2, React 19, TypeScript, Vitest, Playwright.

**Spec:** [`docs/superpowers/specs/2026-09-19-context-engine-five-page-refactor-design.md`](../specs/2026-09-19-context-engine-five-page-refactor-design.md)

## Global Constraints

- The no-key product is complete: local deterministic optimization, recovery, analysis, diagnostics, and settings cannot depend on Jev.
- Jev is a real-time decision layer with five independently controlled workflows; it never becomes the upstream model router or a required sidecar.
- A failed, timed-out, low-confidence, invalid, or partially covered decision falls back conservatively for that workflow only.
- Raw prompts, responses, source code, tool output, credentials, and full command output never enter ordinary config, logs, analytics, exports, or SQLite projections.
- Recoverable payload bytes use the existing encrypted CCR store. SQLite stores metadata, digests, reasons, timing, and accounting only.
- Product services bind to `127.0.0.1`; Jev credentials live in the OS credential store and config contains an opaque reference.
- Optical image compression is removed as a product capability. Ordinary image and multimodal inputs remain byte-identical pass-through.
- Existing user changes in the worktree are preserved. Each implementation batch stages only its owned files and performs the required Git operation audit before commit.
- A phase cannot claim completion until its exact tests pass on one candidate commit. Preview, partial, unavailable, and unknown states remain explicit in the UI.

## Review Focus

- Verify that every optimized request can be traced from candidate creation through validation, recovery preparation, send commit, and redacted receipt projection.
- Verify that disabling Jev, deleting its credential, or forcing every Jev call to fail produces the same safe local behavior.
- Verify that each Jev workflow has its own mode, threshold, timeout, fallback, kill switch, metrics, and tests.
- Verify that legacy Optical configuration and ledger data remain readable while no active producer, route, command, or UI action can invoke Optical.
- Verify that the five-page information architecture reflects user tasks and that old deep links/session state migrate deterministically.
- Verify normal, boundary, invalid-input, timeout, crash/restart, and rollback paths rather than relying on build success alone.

---

## Program Sequence

### Phase 1: Contract and Observability Foundation

Execute [`2026-09-20-dtoken-refactor-01-contract-observability.md`](2026-09-20-dtoken-refactor-01-contract-observability.md).

Exit gate:

- Decision and sticky-decision contracts are frozen in `dtoken-types`.
- SQLite schema stores redacted decision projections without raw content.
- Optimization receipts expose decision source, workflow, confidence band, fallback, latency, recovery, validation, and accounting.
- Existing request behavior is unchanged.

### Phase 2: Deterministic Local Context Engine

Execute [`2026-09-20-dtoken-refactor-02-local-context-engine.md`](2026-09-20-dtoken-refactor-02-local-context-engine.md).

Exit gate:

- A single local engine classifies content, protects critical spans, creates candidates, validates them, prepares CCR recovery, and sends baseline or validated output.
- The local engine handles text, code, diffs, tool schemas, tool results, and mixed content; images pass through.
- OpenAI-compatible and Anthropic request paths use the same engine contract.
- No Jev key is required for any baseline product function.

### Phase 3: Jev Real-Time Workflows

Execute [`2026-09-20-dtoken-refactor-03-jev-realtime-workflows.md`](2026-09-20-dtoken-refactor-03-jev-realtime-workflows.md).

Exit gate:

- The five approved workflows are independently deployable: `tool_output_retention`, `arrival_span_retention`, `session_compaction`, `context_ranking`, and `rule_skill_selection`.
- Sticky decisions prevent repeated pre-provider Jev calls.
- Workflow failure or disablement degrades only that workflow.
- Reserved workflows exist only as disabled contract values and cannot execute.

### Phase 4: Five-Page Desktop Application

Execute [`2026-09-20-dtoken-refactor-04-five-page-desktop.md`](2026-09-20-dtoken-refactor-04-five-page-desktop.md).

Exit gate:

- The only top-level pages are Configuration, Advanced Configuration, Analysis & Logs, Diagnostics, and Settings.
- Existing animation and visual language remain, while page internals use a consistent task flow and source/freshness/action-result evidence.
- Legacy navigation/session state migrates to one of the five pages.
- Keyboard navigation, reduced motion, responsive layouts, and critical Playwright flows pass.

### Phase 5: Optical Retirement and Release Convergence

Execute [`2026-09-20-dtoken-refactor-05-optical-retirement-release.md`](2026-09-20-dtoken-refactor-05-optical-retirement-release.md).

Exit gate:

- Optical has no active runtime producer, config writer, Tauri command, API route, UI route, or release dependency.
- Generic secret/risk/inventory protections needed by text/code optimization remain available outside the Optical module.
- Legacy config and Event Ledger records remain readable and are clearly marked historical.
- Full workspace, desktop, structure, security, real routed OpenCode, and release evidence gates pass on the same commit.

## Commit Order

1. `feat(types): add context decision contracts`
2. `feat(store): persist redacted decision projections`
3. `feat(context): add deterministic local context engine`
4. `feat(runtime): route provider requests through context engine`
5. `feat(jev): add isolated realtime decision workflows`
6. `feat(desktop): converge navigation to five pages`
7. `refactor(optical): retire active image compression paths`
8. `docs(release): converge product contracts and evidence`

Each commit must be preceded by:

```bash
python3 support/scripts/git_operation_audit.py record   --operation commit   --summary '<exact commit purpose>'   --branch "$(git branch --show-current)"   --head-before "$(git rev-parse --short=12 HEAD)"
```

## Final Program Verification

- [ ] Run structure and audit validation.

```bash
python3 support/scripts/check_structure.py
python3 support/scripts/git_operation_audit.py validate
```

Expected: both commands exit `0`.

- [ ] Run Rust formatting, lint, and workspace tests.

```bash
cd code
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Expected: every command exits `0`; no ignored failure is used as release evidence.

- [ ] Run desktop unit, type, build, and end-to-end tests.

```bash
cd code/apps/desktop
npm run test:run
npx tsc --noEmit
npm run build
npm run test:e2e
```

Expected: all checks exit `0`; Playwright proves the five-page flows in the packaged-compatible app shell.

- [ ] Run the authorized OpenCode routed acceptance with transactional backup and exact restore.

```bash
./support/scripts/run_opencode_real_routed_test_macos.sh
```

Expected: the report proves `OpenCode -> d-token listener -> configured upstream`, includes correlated redacted receipt identifiers, and confirms exact config restoration without printing credentials or model content.

- [ ] Inspect the final candidate diff and confirm no raw content, credentials, archived research clones, or unrelated user changes are staged.

```bash
git diff --cached --check
git diff --cached --stat
git status --short
```

Expected: no whitespace errors; the staged set contains only the current implementation batch and its audit record.
