# BASE-1 Baseline Absorption Decision

- P0 freeze commit: `0553168847c323c13d7855dfebd9941295f29f8e`
- Source state after P0: `0` staged, `22` tracked unstaged, `13` untracked (`35` current dirty paths; `support/git-operations.json` was absorbed by P0 after its audit record, so the pre-P0 total was `36`).
- Rule: no dirty path is deleted, reset, overwritten, or silently treated as release evidence. Paths assigned to a later slice remain physically in the shared worktree until that slice imports and reviews the diff. Explicit exclusions remain outside the implementation baseline.

## Slice-owned diffs

| Path | Ownership decision | Required handling |
|---|---|---|
| `code/apps/desktop/src-tauri/icons/32x32.png` | PR-6 macOS/sidebar + app icon slice | Review provenance and visual output; import only through PR-6. |
| `code/apps/desktop/src-tauri/icons/icon.icns` | PR-6 | Same as above; package verification required. |
| `code/apps/desktop/src-tauri/icons/icon.ico` | PR-6 | Same as above; Windows package verification required. |
| `code/apps/desktop/src-tauri/icons/icon.png` | PR-6 | Same as above. |
| `support/scripts/generate_app_icons.py` | PR-6 | Review generator input/provenance and deterministic output before import. |
| `code/apps/desktop/src-tauri/src/main.rs` | PR-13 agent/route native bridge slice | Review existing native command/test diff before PR-13; do not absorb into contract-freeze baseline. |
| `code/apps/desktop/src/app/components/shell/AnimatedNavIcon.test.tsx` | PR-4 motion/polish slice | Review existing test diff in PR-4; no new product capability claim. |
| `code/apps/desktop/src/app/components/shell/AnimatedNavIcon.tsx` | PR-4 | Review existing animation diff in PR-4. |
| `code/apps/desktop/src/app/components/shell/Sidebar.test.tsx` | PR-4 | Review existing test diff in PR-4. |
| `code/apps/desktop/src/app/components/shell/Sidebar.tsx` | PR-4 | Review existing shell-local polish only; eight-page IA must not change. |
| `code/apps/desktop/src/app/components/shell/Topbar.tsx` | PR-3 onboarding/i18n slice, not PR-4 | PR-3 owns Topbar onboarding re-entry; PR-4 remains forbidden from editing it. |
| `code/apps/desktop/src/styles/shell.css` | PR-4 | Review only shell-local motion/style changes; no design-system rewrite. |
| `docs/PRD.md` | DOC-1 shared product-contract reconciliation | Main-agent review after implementation facts are proven; current diff is not authoritative release evidence. |
| `docs/d-token_AI_product_target_spec_v2.md` | DOC-1 target-contract reconciliation | Main-agent review after implementation facts are proven. |
| `docs/releases/v0.0.1/EVIDENCE.md` | DOC-1/release closure | Reconcile only against same-commit N0-N10 evidence. |
| `docs/releases/v0.0.1/EXECUTION.md` | DOC-1/release closure | Reconcile after implementation and evidence DAG close. |
| `docs/releases/v0.0.1/GOALS.md` | DOC-1/release closure | Review against approved unified design. |
| `docs/releases/v0.0.1/OSS_MIGRATION.md` | DOC-1/provenance closure | Review licenses/NOTICE/source pins before import. |
| `docs/releases/v0.0.1/README.md` | DOC-1/release closure | Update last, from verified release state. |
| `docs/releases/v0.0.1/REQUIREMENTS.md` | DOC-1/release closure | Reconcile requirement IDs with final matrix. |
| `docs/releases/v0.0.1/RISKS_DECISIONS.md` | DOC-1/release closure | Preserve unresolved risks; no greenwashing. |
| `docs/releases/v0.0.1/TASKS.md` | DOC-1/release closure | Reconcile only after slice states are real. |
| `docs/releases/v0.0.1/VERIFICATION.md` | DOC-1/release closure | Bind commands/results to `C_final`; current text is not evidence. |

## Explicitly excluded local artifacts

| Path | Reason |
|---|---|
| `.mimosa/` | Local tool state; no product/release ownership. |
| `.omp/` | Local harness state; no product/release ownership. |
| `plans/` | Session/tool output outside the approved `.ai-bridge/release-iteration` plan source; not imported. |
| `code/apps/desktop/src/app/components/shell/Sidebar.performance.test 2.tsx` | Duplicate-suffixed local copy; preserved but excluded pending explicit owner review. |
| `code/apps/desktop/src/app/lib/install/selection-controls.test 2.ts` | Duplicate-suffixed local copy; preserved but excluded. |
| `code/apps/desktop/src/app/lib/optical/client.test 2.ts` | Duplicate-suffixed local copy; preserved but excluded. |
| `code/apps/desktop/src/app/lib/overview/savings.pricing.test 2.ts` | Duplicate-suffixed local copy; preserved but excluded. |
| `code/apps/desktop/src/app/lib/visual-compression/VisualCompressionWorkbench.test 2.tsx` | Duplicate-suffixed local copy; preserved but excluded. |
| `docs/archive/reviews/2026-07-01/APP_PAGE_UX_SPEC_MULTIDIMENSIONAL_REVIEW_2026-07-02 2.md` | Duplicate historical document; preserved, not a current truth source. |
| `docs/history/OPTICAL_RELEASE_ITERATION_REPORT_2026-08-06 2.md` | Duplicate historical report; preserved, not current evidence. |
| `docs/history/PROTOCOL_NORMALIZATION_REPORT_2026-08-06 2.md` | Duplicate historical report; preserved, not current evidence. |
| `code/crates/dtoken-compress/examples/_diag_tmp.rs` | Ad-hoc diagnostic source; preserved outside release implementation until the owning compression slice explicitly reviews it. |

## Worktree and branch reconciliation

- Existing worktrees observed at P0: 57.
- All pre-existing `final-*`, release, detached, Supacode, Cline, and temporary worktrees are classified `pre-design/superseded` by default.
- They are not implementation baselines and must not be reused merely because a planned branch name resembles an existing branch.
- Before every planned branch/worktree creation, the main agent must check whether the branch already exists, compare its base and ownership, record the decision in `support/git-operations.json`, and either approve explicit reuse or select a fresh audited branch name. No branch is deleted by BASE-1.

## Frozen implementation baseline

The implementation baseline is the BASE-1 commit containing this decision report on top of P0 commit `0553168847c323c13d7855dfebd9941295f29f8e`. The dirty slice-owned and excluded paths listed above are not silently included in that commit. Every later slice must explicitly review any assigned pre-P0 diff before claiming completion.
