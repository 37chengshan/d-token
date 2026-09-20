# Optical Retirement and Release Convergence Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove Optical image compression from every active product path, preserve safe multimodal pass-through and legacy readability, and produce release evidence for the refactored application.

**Architecture:** Retire Optical in dependency order. First migrate config to a read-only legacy form and preserve historical ledger deserialization. Then remove UI and Tauri controls, remove core-api endpoints and runtime producers, delete Optical-specific code/types/dependencies, archive superseded documentation, and run all release gates on one candidate commit. Generic secret/risk/inventory logic already extracted in phase 2 remains active.

**Tech Stack:** Rust/Serde migrations, Tauri, React/TypeScript, repository structure checks, full Rust/Desktop test suites, real routed OpenCode acceptance.

**Spec:** [Approved context engine design](../specs/2026-09-19-context-engine-five-page-refactor-design.md)

## Global Constraints

- Removing Optical must not remove ordinary image or multimodal forwarding.
- Legacy configs load deterministically, produce a migration receipt, and write back without active Optical fields.
- Rollback may restore the pre-migration file for data recovery but cannot reactivate an absent runtime capability.
- Historical Event Ledger records remain readable and labeled legacy; no new producer writes Optical stages.
- Remove dependencies only after rg and compiler evidence show no active use.
- Release evidence must come from the same candidate commit and must distinguish simulated, preview, local, and real routed checks.

## Review Focus

- No route, command, feature flag, deep link, or stale config can invoke Optical.
- Image bytes are byte-identical across OpenAI-compatible and Anthropic pass-through tests.
- Generic protection helpers remain neutral and are covered independently.
- A prior config and prior ledger fixture both load after retirement.
- Documentation, required-path checks, UI labels, package metadata, and release gates all agree that Optical is gone.

---

### Task 1: Migrate Optical config to read-only legacy compatibility

**Files:**
- Modify: code/crates/dtoken-config/src/lib.rs
- Create: code/crates/dtoken-config/src/legacy_optical.rs
- Create: code/crates/dtoken-config/tests/legacy_optical_migration.rs
- Modify: code/configs/config.example.json
- Modify: code/crates/dtoken-config-patcher/src/lib.rs
- Create: code/crates/dtoken-config-patcher/tests/optical_retirement.rs

- [ ] Add failing fixtures for enabled, disabled, malformed, partial, and future-version Optical config.

- [ ] Define migration behavior:
  - recognized legacy Optical fields load into LegacyOpticalConfig;
  - active runtime config omits Optical;
  - save writes no Optical field;
  - a redacted migration receipt records source digest, disposition, backup reference, verify status, and reason;
  - unsupported future schema returns an explicit error and leaves the file unchanged.

- [ ] Run tests and verify current config still exposes active optical.

~~~bash
cd code
cargo test -p dtoken-config --test legacy_optical_migration
cargo test -p dtoken-config-patcher --test optical_retirement
~~~

- [ ] Implement a custom deserialization/migration boundary. Do not keep find_optical_route_policy or normalize_optical_fail_closed on DtokenConfig.

~~~rust
pub struct ConfigLoadResult {
    pub config: DtokenConfig,
    pub legacy_migrations: Vec<LegacyConfigMigration>,
}
~~~

- [ ] Update config patching so previews remove legacy Optical keys and explain that the capability is retired. Apply must create backup, write, reload, verify absence, and return rollback evidence.

- [ ] Add a test that rollback restores exact file bytes but subsequent app load still cannot expose an Optical runtime.

- [ ] Run config tests.

~~~bash
cd code
cargo test -p dtoken-config
cargo test -p dtoken-config-patcher
~~~

- [ ] Audit and commit.

Commit: refactor(config): migrate Optical to legacy read compatibility

### Task 2: Remove Optical from React and Tauri surfaces

**Files:**
- Delete: code/apps/desktop/src/app/lib/optical/client.ts
- Delete: code/apps/desktop/src/app/lib/optical/types.ts
- Delete: code/apps/desktop/src/app/lib/visual-compression/VisualCompressionWorkbench.tsx
- Modify: code/apps/desktop/src/app/components/shell/AppShell.tsx
- Modify: code/apps/desktop/src/app/lib/nav.ts
- Modify: code/apps/desktop/src/app/lib/i18n.ts
- Modify: code/apps/desktop/src/app/components/pages/Configuration.tsx
- Modify: code/apps/desktop/src/app/components/pages/Settings.tsx
- Modify: code/apps/desktop/src-tauri/src/main.rs
- Modify: code/apps/desktop/src-tauri/README.md
- Modify: code/apps/desktop/e2e/desktop-control-plane.spec.ts

- [ ] Add failing tests that search rendered navigation, settings, configuration, command registry, and deep links for Optical/Visual Compression product actions.

- [ ] Add a migration test proving legacy page value visual opens Configuration and does not render an Optical panel.

- [ ] Delete Optical UI client/types/workbench and remove all imports, translations, tabs, command invocations, status cards, and feature flags.

- [ ] Remove Tauri commands:
  - get_optical_observability
  - preview_optical_plan
  - get_optical_config_editor
  - preview_optical_config_change
  - apply_optical_config_change
  - rollback_optical_config_change

- [ ] Update the single release handler-list test so it proves the commands are absent in release and debug registrations.

- [ ] Run desktop checks.

~~~bash
cd code/apps/desktop
npm run test:run
npx tsc --noEmit
npm run build
npm run test:e2e
cd ../..
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml
~~~

Expected: all pass with no Optical user action.

- [ ] Audit and commit.

Commit: refactor(desktop): remove Optical product surfaces

### Task 3: Remove Optical API routes and runtime producers

**Files:**
- Modify: code/crates/dtoken-core-api/src/lib.rs
- Modify: code/crates/dtoken-core-api/src/context_runtime.rs
- Modify: code/crates/dtoken-core-api/tests/compress_gateway.rs
- Delete: code/crates/dtoken-core-api/tests/optical_phase_b.rs
- Create: code/crates/dtoken-core-api/tests/multimodal_passthrough.rs
- Modify: code/crates/dtoken-agent-bridge/src/lib.rs
- Modify: code/crates/dtoken-doctor/src/lib.rs

- [ ] Add failing HTTP tests asserting former Optical endpoints return the normal not-found contract and cannot mutate config.

- [ ] Add byte-level multimodal pass-through fixtures for image URLs, base64 image blocks, mixed text/image order, unknown media types, and large image payloads for OpenAI-compatible and Anthropic routes.

~~~rust
assert_eq!(forwarded_image_bytes, input_image_bytes);
assert_eq!(forwarded_part_order, input_part_order);
assert!(receipt.transformations.iter().all(|t| !t.contains("optical")));
~~~

- [ ] Remove direct Optical endpoints, registry/config lookup, observability producers, and prepare_direct_anthropic_hybrid Optical branch. Route text/code through ContextRuntime and pass unsupported media unchanged.

- [ ] Remove Optical capability advertisements and Doctor checks. Replace any image-compression health field with no field; do not show a permanently unavailable retired capability.

- [ ] Preserve generic multimodal request parsing required for pass-through.

- [ ] Run core route tests.

~~~bash
cd code
cargo test -p dtoken-core-api --test multimodal_passthrough
cargo test -p dtoken-core-api --test compress_gateway
cargo test -p dtoken-agent-bridge
cargo test -p dtoken-doctor
~~~

- [ ] Audit and commit.

Commit: refactor(runtime): remove Optical request paths

### Task 4: Remove Optical types, module, tests, and dependencies

**Files:**
- Delete: code/crates/dtoken-compress/src/optical/
- Delete: code/crates/dtoken-compress/tests/optical_phase_a.rs
- Modify: code/crates/dtoken-compress/src/lib.rs
- Modify: code/crates/dtoken-compress/src/hybrid.rs
- Modify: code/crates/dtoken-compress/Cargo.toml
- Modify: code/Cargo.lock
- Modify: code/crates/dtoken-types/src/lib.rs
- Modify: code/crates/dtoken-types/tests/contract_freeze.rs
- Delete: code/crates/dtoken-types/tests/optical_types.rs
- Modify: code/crates/dtoken-event-ledger/src/lib.rs
- Create: code/crates/dtoken-event-ledger/tests/legacy_optical_read.rs

- [ ] Add historical ledger fixtures for every Optical LayeredStage variant that must remain readable.

- [ ] Introduce a read-only legacy representation at the deserialization boundary if deleting public Optical structs would otherwise break historical data.

~~~rust
#[derive(Debug, Deserialize)]
#[serde(tag = "stage", rename_all = "snake_case")]
enum StoredLayeredStage {
    Headroom,
    LocalContext,
    JevDecision,
    OpticalInventory,
    OpticalRender,
    OpticalValidate,
}
~~~

Map Optical values to a public historical label; never expose a producer constructor.

- [ ] Run the legacy read test before deletion.

~~~bash
cd code
cargo test -p dtoken-event-ledger --test legacy_optical_read
~~~

- [ ] Remove the optical module, Optical runtime structs, render/patch/post-validation code, and tests.

- [ ] Remove base64, ed25519-dalek, font8x8, and png from dtoken-compress only after checking they have no non-Optical caller.

~~~bash
rg -n "base64|ed25519|font8x8|png::|optical::|Optical" \
  code/crates/dtoken-compress code/crates/dtoken-types code/crates/dtoken-event-ledger
~~~

Expected before dependency removal: only known files in this task. Expected after: no active dtoken-compress Optical reference; event-ledger references are confined to legacy read compatibility.

- [ ] Run format, clippy, and affected tests.

~~~bash
cd code
cargo fmt --all -- --check
cargo clippy -p dtoken-compress -p dtoken-types -p dtoken-event-ledger --all-targets -- -D warnings
cargo test -p dtoken-compress
cargo test -p dtoken-types
cargo test -p dtoken-event-ledger
~~~

- [ ] Audit and commit.

Commit: refactor(optical): delete image compression implementation

### Task 5: Converge product documentation and archive superseded Optical material

**Files:**
- Modify: docs/PRD.md
- Modify: docs/PLAN.md
- Modify: docs/ARCHITECTURE.md
- Modify: docs/TECH_STACK.md
- Modify: docs/SECURITY_BOUNDARIES.md
- Modify: docs/APP_FUNCTIONS_AND_PAGES.md
- Modify: docs/product/IMPLEMENTATION_STATUS.md
- Modify: docs/index.md
- Modify: code/crates/dtoken-compress/README.md
- Modify: code/runtimes/headroom/README.md
- Modify: support/scripts/check_structure.py
- Move: docs/experiments/OPTICAL_HYBRID_CONTEXT_OPTIMIZATION_PLAN_V7.md -> docs/archive/optical/OPTICAL_HYBRID_CONTEXT_OPTIMIZATION_PLAN_V7.md
- Move: docs/optical/ -> docs/archive/optical/runtime-docs/

- [ ] Update PRD and page contracts to the five-page product, Local/Jev engine, code graph, logs/analysis, diagnostics, and settings.

- [ ] Mark Optical as retired with migration behavior and ordinary multimodal pass-through. Do not leave it as Beta, preview, planned, or unavailable navigation.

- [ ] Update PLAN with only remaining execution work and IMPLEMENTATION_STATUS with code-backed completed/partial/unverified facts.

- [ ] Archive Optical plans/reviews/status and update every active link plus the structure allow/required lists.

- [ ] Run structure and active-document reference scans.

~~~bash
python3 support/scripts/check_structure.py
rg -n "视觉压缩|Visual Compression|Optical" \
  README.md docs code --glob '*.md' \
  --glob '!docs/archive/**' \
  --glob '!docs/history/**'
~~~

Expected: active occurrences are limited to explicit retirement/migration/history notes and no current capability claim.

- [ ] Audit and commit.

Commit: docs(product): retire Optical and align context engine contracts

### Task 6: Run one-candidate release verification

**Files:**
- Modify only if a test exposes a directly related defect.
- Create or update the repository's existing release evidence location specified by docs/PLAN.md.
- Modify: support/git-operations.json for each resulting commit.

- [ ] Record the candidate SHA before testing and use it in every evidence artifact.

~~~bash
git rev-parse HEAD
git status --short
~~~

- [ ] Run repository structure, formatting, lint, and all Rust tests.

~~~bash
python3 support/scripts/check_structure.py
python3 support/scripts/git_operation_audit.py validate
cd code
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
~~~

Expected: all pass on the recorded candidate SHA.

- [ ] Run complete desktop verification.

~~~bash
cd code/apps/desktop
npm run test:run
npx tsc --noEmit
npm run build
npm run test:e2e
npm run tauri:build
~~~

Expected: all pass; packaged app exposes five top-level pages and no Optical command or asset dependency.

- [ ] Run repository scans.

~~~bash
cd /Users/cc/code/d-token
rg -n "get_optical|preview_optical|apply_optical|rollback_optical|VisualCompressionWorkbench|optical::" \
  code --glob '!target/**' --glob '!src-old/**'
rg -n "api[_-]?key|Authorization:|Bearer " \
  code docs support --glob '!target/**' --glob '!package-lock.json'
~~~

Expected: the first scan finds only deliberate legacy fixture names, if any; the second finds no embedded credential.

- [ ] Run the authorized real OpenCode route test with persistent backup and exact restore.

~~~bash
./support/scripts/run_opencode_real_routed_test_macos.sh
~~~

Expected: proves OpenCode -> d-token listener -> configured upstream, correlates request/decision/send receipts, and restores the exact original config without exposing auth material or model content.

- [ ] Manually exercise Local/no-key, Jev Shadow, Jev failure fallback, delete-key fallback, request inspection, diagnostics, rollback, and image pass-through in the packaged app.

- [ ] Record every unexecuted platform check as unverified. Do not convert macOS evidence into a Windows pass.

- [ ] If fixes were required, audit/commit them, record the new candidate SHA, and rerun every gate whose evidence was invalidated.

Commit when needed: fix(release): close context engine verification defects
