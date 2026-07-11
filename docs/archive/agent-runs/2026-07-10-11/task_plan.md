# d-token macOS-first implementation plan

## Objective

Deliver a truthful, polished d-token desktop control plane aligned with `docs/PRD.md`, using Rust for core business logic, Tauri 2 for the desktop shell, and React/TypeScript for the UI. macOS is the first execution and visual-validation platform; Windows remains a supported target with the same product behavior.

## Execution order

1. Preserve the existing dirty documentation reorganization and establish macOS build/runtime truth.
2. Complete d-token-owned service lifecycle, runtime data boundaries, and safe configuration transaction evidence.
3. Polish the six-page product IA, state semantics, Chinese UI copy, focus behavior, and dangerous-action safety.
4. Expose Optical shadow-only status and preview inside the existing product IA as an advanced context-optimization surface; keep active mutation disabled until renderer, patcher, post-validation, rollback, and fidelity gates are proven.
5. Add layered Rust, Vitest/RTL, Playwright, Tauri-window, and real OpenCode routed verification.
6. Re-audit every PRD and release gate against current source, tests, screenshots, and runtime evidence.

## Non-negotiable constraints

- Never expose raw prompt, response, image, base64, secret, API key, cookie, or auth material.
- Preview/demo data must be visibly distinct from live d-token-owned runtime data.
- Every write path requires a human-readable summary, diff, backup, verification, and rollback evidence.
- Optical shadow mode must preserve the original request body byte-for-byte and fail closed.
- Do not use external Headroom as an App runtime dependency.
- Real OpenCode tests use the user's existing configuration and never print or persist secrets.

## Phases

| Phase | Status | Completion evidence |
| --- | --- | --- |
| 1. Recover current state and parallel evidence audit | complete | PRD/progress/Optical/audit reports mapped to current source; dirty worktree ownership recorded |
| 2. Converge architecture, implementation, and test plan | complete | Requirement-to-code/test matrix with macOS-first and Windows-parity decisions |
| 3. Implement Rust/Tauri/runtime and Optical improvements | in_progress | P0 route, persistence, lifecycle, and runtime-provenance hardening is green; real routed evidence remains |
| 4. Polish React/TypeScript product UI and UX | in_progress | Install/runtime evidence is truthful and unit-tested; Playwright and real-window polish remain |
| 5. Verify builds and automated quality gates | in_progress | Cargo, TypeScript, Vitest, Playwright, Tauri/macOS build results |
| 6. Run real OpenCode routed verification | pending | Redacted evidence proving OpenCode -> d-token-owned runtime -> configured upstream |
| 7. Run macOS App visual and functional acceptance | pending | Real Tauri window walkthrough, screenshots, interaction/UX findings and fixes |
| 8. Final PRD completion audit and documentation | pending | Requirement-by-requirement evidence, updated architecture/top-level docs, no unproven claims |
| 9. Git operation audit trail and precise delivery | in_progress | Aggregate baseline `ad276cc` pushed; subsequent P0 slices require separate audited commits and pushes |

## Current phase

The aggregate delivery gate is complete on `codex/prd-macos-polish`: commit `ad276cc` is pushed to `origin/codex/prd-macos-polish`. The three Phase 3/4 P0 slices have converged: Tauri runtime truth/persistence, backend routing/durability, and Install/runtime-evidence UI all pass their combined gates. They are being split into audited scope commits before Playwright, current App rebuild, real-window acceptance, and fresh OpenCode routed verification.

## Errors encountered

| Error | Attempt | Resolution |
| --- | --- | --- |
| `rtk tree` could not run because the native `tree` command is absent | 1 | Use `rtk ls`, `rtk find`, and codebase-memory-mcp instead; do not install new tooling for discovery |
| `cargo test --workspace` failed because the macOS target sidecar was not prepared | 1 | Validate/run the macOS sidecar preparation script before retrying the workspace gate |
| TypeScript cannot resolve `../logs/event` from `src/app/lib/state/operational.ts` | 1 | Locate the intended event-log module and repair the scoped import/module before rerunning typecheck |
| Workspace tests reached Tauri but failed because `src-tauri/icons/icon.png` is missing | 1 | Inspect existing icon assets and configure/generate the required macOS/Tauri icon source before the next workspace attempt |
| Rust format check reported only mechanical formatting drift; Clippy reported 3 Optical warnings-as-errors | 1 | Run rustfmt, then patch one collapsible match and two manual checked divisions before re-running both gates |
| Local image viewer could not process the root SVG icon sources | 1 | Inspect SVG source/metadata and use Tauri's icon generator; validate generated PNG/ICNS outputs instead of retrying the same viewer action |
| First SVG source read used paths relative to the desktop package instead of repository root | 1 | Re-read with absolute paths; selected the coding-cat asset for desktop icon generation |
| First planning-file patch used a context that did not apply atomically | 1 | Re-read the plan and apply smaller exact patches |
| First Git-audit patch assembly had a JavaScript template-literal syntax error | 1 | Removed embedded Markdown backticks from the patch transport and applied the files successfully |
| First mapper-findings patch used an out-of-order heading context | 1 | Appended a dedicated evidence section using a stable end-of-file anchor |
| `npm audit` failed because the configured npmmirror registry does not implement the npm security audit endpoint | 1 | Retry once against the official npm registry explicitly; do not alter the user's global npm registry configuration |
| First OpenCode script redesign used `OPENCODE_CONFIG_CONTENT` plus isolated XDG data/state | 1 | Reject that approach because it violates `AGENTS.md` rule 9 and PRD real-test policy; redesign around the normal config with exact backup, minimal temporary provider addition, conflict-safe restore, and redacted report |
| Late Optical ContextGraph/PlanIR/config edits failed `cargo fmt --check`, then Clippy found missing `DtokenConfig.optical` initializers and an incomplete recent-events integration | 1 | Preserve the work, repair the test initializers and complete the metadata-only endpoint under the dedicated Optical subtask before rerunning full Rust gates |
| `python3 -m unittest support/scripts/test_git_operation_audit.py` discovered zero pytest-style tests | 1 | Use `python3 -m pytest -q`; both audit-script tests pass |
| Planning update failed because concurrent documentation cleanup moved root planning files | 1 | Located the intact set under `docs/archive/agent-runs/2026-07-10-11/`, preserved the archival move, and continued updates at the relocated paths |
| Combined log-truth patch did not match one fixture line and was rejected atomically | 1 | Confirmed no partial changes, then split the work into small component/type/fixture patches with exact local contexts |
| New LogsSecurity test assumed Radix inactive tab content was visible after click | 2 | Switched to semantic user-event activation and asserted the truthful row is mounted while avoiding a jsdom visibility claim Radix does not satisfy |
| A scoped frontend check used repository-relative paths while already inside the desktop package | 1 | Re-ran with `src/...` paths; TypeScript then exposed one real implicit-any issue and it was fixed |
| A fixture-import search used a mismatched shell quote | 1 | Replaced the complex quoted regex with a simple `lib/fixtures` search and continued without repeating the command |
| A second complex fixture regex also hit shell quote parsing | 1 | Stopped using nested quote expressions entirely and used a plain `fixtures` search scoped by path/glob |
| A follow-up fixture command accidentally reintroduced nested shell quotes | 1 | Re-ran only simple semicolon-separated `sed` and plain `rg fixtures`; no further complex shell regexes will be used |
