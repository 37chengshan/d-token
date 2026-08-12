# CF-1 Contract Freeze Types Report

## Scope

- Baseline: `c84ba9a52d8396ddc8a7d54a69cd627cb7c5d394`
- Owned implementation: `code/crates/dtoken-types/src/{agent.rs,lib.rs}`
- Direct contract tests: `code/crates/dtoken-types/tests/contract_freeze.rs`
- No other crate, desktop source, docs, support script, Cargo manifest, or workflow was modified.

## Frozen contracts

- Existing configuration contracts were reused from `dtoken_types::agent` and re-exported from the crate root: `AgentConfigSnapshot`, `UpstreamSignature`, `AgentConfigPlan`, `AgentConfigReceipt` and supporting enums/target types. No second configuration type set was introduced. The existing naming is an explicit compatibility mapping: design `provider` → `provider_id`, `profile_or_app_scope` → `profile_scope`, and snapshot provider/model/protocol/endpoint identity → nested `current_upstream: UpstreamSignature`.
- Completed the design-required configuration metadata: `AgentConfigPlan.backup_or_rollback_id` and `AgentConfigReceipt.{source_revision,next_action}` now bind review/backup identity and user-facing continuation without carrying config bytes.
- Added the closed per-Agent transaction state machine `ConfigTransactionState` with the approved forward chain, rollback only after backup, failure from non-terminal states, and absorbing terminal states.
- Added `WritePolicy::{AlwaysReadOnly, UserSelectedManaged, Unsupported}` and capability tier/aspect/Agent capability contracts.
- Added the exact ten-field `AgentTransactionEvidence` cross-section.
- Added receipt contracts: `OptimizationReceipt`, `ValidationReceipt`, `RecoveryReceipt`, `SendCommitReceipt`, `RelayHitReceipt`, `CacheReuseReceipt`; Local, Provider, claimable, relay, and cache buckets remain distinct.
- Added canonical type skeletons `ContextIrNode` and `NodeAction::{Preserve, Reuse, HeadroomSemantic, RtkCommandOutput, Optical}`. Wiring remains assigned to C0A/C1.
- Credential-bearing fields are absent. Tests parse JSON keys recursively, reject token/auth/credential/cookie/secret/password key shapes, allow only approved accounting fields, and exercise every frozen serializable contract.

## Root-cause correction after first test run

The first direct test run exposed two test-design defects, not contract implementation defects:

1. A raw substring check for `token\"` falsely classified the approved token-accounting keys as credential fields. The assertion now parses JSON recursively and rejects credential-bearing key names only.
2. A JSON object key vector was compared in declaration order, but `serde_json::Map` ordering is not a field-order contract. The test now compares an order-independent key set.

No security gate was weakened: credential key detection remains explicit and applies recursively to every object.

## Independent review closure

- Initial review verdict: `CHANGES_REQUIRED` — one High and two Low findings.
- High closed: plan/receipt backup, source revision, and next-action fields were added and directly tested.
- Low closed: compatibility field-name mappings are documented above; the structural credential tripwire now covers auth/credential shapes and every frozen contract, including a populated `AgentConfigTarget` nested in the plan.
- Independent re-review verdict: `APPROVE`; all original findings closed. Its sole new Low (empty target vector in the credential battery) was also fixed before the final verification run.

## Verification

- `cargo test -p dtoken-types`: PASS — 38 tests across 6 suites, 0 failures.
- `cargo fmt --all -- --check`: PASS.
- `cargo clippy --workspace --all-targets -- -D warnings`: PASS after running the repository-defined macOS debug-sidecar preparation and producing the frontend `dist` consumed by Tauri's compile-time context. The initial attempts failed only because a fresh worktree lacked generated `dtoken-proxy-aarch64-apple-darwin` and `apps/desktop/dist`; after those declared prerequisites were generated, Clippy finished without warnings.
- `git diff --check`: PASS.

## Risk and next gate

CF-1's owned crate, direct contract tests, formatter, and workspace Clippy gate are green. Generated sidecars, frontend `dist`, and the local `node_modules` symlink are ignored build prerequisites and are not part of the slice diff.
