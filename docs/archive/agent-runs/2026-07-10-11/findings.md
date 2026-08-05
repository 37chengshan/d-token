# Current findings

## Evidence baseline

- Canonical desktop app: `code/apps/desktop`.
- App shell already exposes six product pages: overview, install, analytics, doctor, changes, settings.
- Codebase Memory indexes project `Users-cc-Documents-d-token` and exposes Rust/Tauri/React call relationships.
- Optical Phase A planners and tests exist under `code/crates/dtoken-compress/src/optical`, but API/UI/renderer phases remain absent.

## Release blockers

- Tauri config is Windows-oriented: NSIS target, `.ico`, WebView2 resource, and PowerShell-only release preparation.
- macOS Keychain dependency exists, but the complete credential write and outbound short-lived resolution path is not proven.
- Install/configure/verify/rollback is not yet a durable transaction with failure recovery evidence.
- JSONL durability, rotation, corruption isolation, and long-running behavior need fault tests.
- No repository-native Playwright visual regression command currently exists.

## UI risks

- Runtime truth is not uniformly gated across all pages.
- Preview and dangerous actions need consistent disablement and source labels.
- Overview prioritization, Doctor/Changes safety review, Settings hierarchy, and reduced-motion/focus behavior need refinement.
- Existing reports are partly historical and must not override current source or dirty-worktree evidence.

## Worktree safety

The worktree contains pre-existing documentation moves/deletions and generated macOS schema files. Do not revert, stage, or rewrite those changes as part of implementation.

## Agent and tool discovery

- Project-local `.codex/` is currently absent even though the supplied AGENTS guidance describes sample project roles; usable global role definitions exist under `/Users/cc/.codex/agents/`.
- Relevant installed specialists include `code-mapper`, `architect-reviewer`, `docs-researcher`, `gsd-ui-auditor`, `test-engineer`, `test-automator`, `security-auditor`, `rust-engineer`, `frontend-developer`, and `ui-ux-tester`.
- The active collaboration runtime has three child slots. Initial read-only work is therefore grouped into product/code/Optical mapping, frontend UI/UX audit, and backend/runtime/test architecture.
- codebase-memory-mcp is callable in this session and exposes architecture, graph search, snippet, and path tracing tools. It remains the primary code discovery surface.
- RTK is installed and is the default shell-output proxy. Native `tree` is not installed, so directory discovery should use RTK list/find or the code graph.

## Current code graph baseline

- The indexed repository contains 5,830 nodes and 13,981 edges across 209 TypeScript and 37 Rust files. The de-facto core cluster centers on `start_dtoken_service`, `record_routed_request`, `dashboard_payload_for_state`, route forwarders, and Optical planning/validation.
- The graph exposes d-token-owned routes including `/dashboard/payload`, `/events`, `/config/patches/plan`, `/config/rollback-points`, `/projects/*`, and the new `/compression/optical/{status,capabilities,preview}` surfaces.
- Optical Phase B source currently includes safe-default status/capability handlers plus preview tests under `code/crates/dtoken-core-api/tests/optical_phase_b.rs`. Active mutation still requires separate renderer/equivalence proof; the existing surface is shadow/preview-oriented.
- The runtime graph contains explicit credential-resolution, loopback-guard, provider/route lookup, upstream forwarders, event append, dashboard payload, and service lifecycle symbols. These are necessary evidence anchors but do not by themselves prove live routed behavior or durability.
- The graph still indexes both `code/apps/desktop` and `code/UI design for project`. The latter appears to be a design/prototype duplicate and must not be confused with the canonical desktop app; final top-level cleanup/documentation must make ownership explicit before removal is considered.
- The new macOS real-test script is indexed and must be reviewed before use. Real-test success requires redacted ledger/runtime evidence, not merely a successful OpenCode exit.

## Authoritative document locations

- The latest dated audits were moved, not deleted: backend and frontend reports are now `docs/history/BACKEND_AUDIT_AND_ITERATION_2026-07-10.md` and `docs/history/FRONTEND_UI_AUDIT_AND_ITERATION_2026-07-10.md`.
- Optical planning lives under `docs/experiments/` with a separate structured implementation set under `docs/optical/` (architecture, status, release gates, reviews, and test matrix).
- Formal release status lives under `docs/release/`; historical milestone evidence is under `docs/history/`. Current source/runtime evidence must override historical completion language.
- The mandatory PRD/architecture/security/audit/Optical reading set is roughly 4,800 lines, so conclusions must be persisted incrementally rather than held only in context.

## PRD and architecture requirements

- The release target is a stable Beta/release candidate, not a demo: every main flow needs real d-token-owned behavior plus loading/empty/error/success states.
- Product IA is fixed to five main pages plus Settings: Overview, Install & Connect workspace, Analytics, Doctor, Changes & Rollback, Settings. Install & Connect contains One-click Install, Smart Learning, Advanced Install, and Logs.
- The critical transaction is detect -> configure plan -> human summary/diff -> backup -> explicit apply -> verify -> Event Ledger/change record -> rollback. UI-window success alone cannot prove the transaction or routed runtime.
- UI copy must stay user-facing and must not expose benchmark/smoke/fixture/mock/testing-center/runtime-payload jargon. OpenCode is a real acceptance entry, not the product's information-architecture center.
- Security invariants are loopback-only by default, explicit routes with no silent fallback, local/redacted credentials and logs, no raw prompt/response persistence, process identity checks, visible provider/route/instance decisions, and reversible writes.
- `docs/TECH_STACK.md` and `docs/ARCHITECTURE.md` still describe a transitional Python Headroom-compatible sidecar. The user's current architecture requirement is stricter: Rust core business logic + Tauri 2 + React/TypeScript + minimal Swift/macOS bridge. Documentation and implementation boundaries must be reconciled explicitly rather than silently leaving conflicting architecture truth.
- Both macOS and Windows remain first-class; macOS receives real-window functional/visual validation first, while UI/product behavior must remain platform-parity. Platform-specific code should be confined to small OS adapters.
- The PRD requires complete Headroom Dashboard capability migration, but current stable-release IA presents those capabilities through user-task pages rather than retaining engineering-oriented dashboard navigation.

## Latest audit conclusions and current drift

- Both 2026-07-10 audits explicitly classify the candidate as release-blocked. Backend blockers remain credential-store end-to-end wiring, transaction-grade configuration/rollback, durable JSONL+SQLite behavior, concurrency/backpressure/retry policy, Doctor coverage, and package-level platform proof.
- The backend audit records one prior real OpenCode routed run and redacted artifact, but current work must still execute a fresh run against the current candidate; historical evidence cannot prove new dirty-worktree changes.
- The frontend audit's central rule is truthful state before visual polish. Its P0 list is Smart Learning fake completion, inconsistent runtime data boundaries, weak Install navigation/flow, non-conserving Analytics metrics, and preview data resembling live machine truth.
- Some frontend audit facts are already stale: the current worktree contains Vitest tests and a `RuntimeDataBoundary`-style evidence helper. Current tests/source must decide what remains, not the old score.
- The target UI refactor is medium-sized rather than a rewrite: retain the five-page shell and design language, then converge on one primary action per page, full-width four-way Install navigation, unified source/freshness state, Chinese task language, safe confirmations, and restrained desktop-control-plane visuals.

## Optical compression status

- Optical Phase A is a pure-Rust, plan-only, fail-closed foundation. It defaults disabled/shadow-only; preserves current turn/recent tail/exact/secret/schema/open-tool-call content as text; and never exposes raw prompt/image/base64 in public summaries.
- Formal Optical docs still say Phase B API visibility is pending, while current source already contains `/compression/optical/status`, `/capabilities`, `/preview` plus new tests. Documentation must be updated only after those tests pass.
- Renderer/request mutation remains deliberately absent. Active mode must stay disabled until a shared geometry renderer, protocol patcher, equivalence validation, cache stability, secret exclusion, budget limits, and renderer-failure original-body fallback are all proven.
- Optical UI should not become a developer-facing "Optical Lab" top-level test page in the release IA. The safe product shape is an advanced, clearly experimental visual-compression preview/status surface inside existing Analytics/Settings/Install advanced details, with no active toggle until renderer gates pass.
- Optical documentation contains historical inconsistencies in test counts (12 vs 42) and Phase B status. Fresh Cargo results must become the single evidence source.

## Current dirty implementation assessment

- The in-progress Rust changes strengthen Optical safety through an explicit capability registry, protocol/endpoint matching, policy gates, over-budget rejection, request-local redaction, and shadow-plan validation; they are substantial and require workspace-level verification before further feature expansion.
- The in-progress frontend changes introduce evidence classification and apply it to Overview, Doctor, and Changes, with new Vitest/RTL tests. This directly addresses the audit's fake/live/stale/partial-state blocker, but Install, Analytics, Smart Learning, Settings, and global status still need the same truth model.
- `code/apps/desktop/package.json` now has `test` and `test:run`, Vitest 3.2.6, RTL, user-event, and jsdom. The old audit's "zero frontend tests" claim is no longer current, but coverage remains narrow.
- Base Tauri config remains Windows-specific (`nsis`, `.ico`, WebView2 resource). A new macOS overlay selects `app`/`dmg` and clears Windows-only icon/resources, but it must be proven via Tauri CLI/build; configuration presence is not packaging evidence.
- The macOS real OpenCode script safely uses a temporary d-token config/data directory and redacted summaries, but it does not itself modify or point the user's OpenCode provider to its selected test port. It assumes the requested `dtoken/...` model already resolves through that listener. The script must preflight and prove the actual OpenCode base URL/port without printing credentials, or perform a backed-up minimal config transaction when necessary.
- The current script asks for an exact `d-token-ok` marker and checks route/provider/agent plus four JSONL files. This is useful developer evidence, but the marker/output must never surface in formal App UI.

## First fresh verification results

- Frontend Vitest passes 40/40 tests across 11 files. This proves the new state/UX units, not Playwright or real-window behavior.
- TypeScript fails because `src/app/lib/state/operational.ts` imports a missing `../logs/event` module. The chained production build did not run after that failure.
- Rust workspace testing reaches the Tauri build script but fails before tests because `binaries/dtoken-proxy-aarch64-apple-darwin` is absent. The macOS preparation step is a required gate.
- The missing TypeScript import is from an older shared `AppOperationalState` model. No `lib/logs/` directory exists, and current page work uses the newer `state/evidence.ts` boundary. Before adding another model, confirm whether `operational.ts` is unused legacy code; removing dead parallel state is safer than inventing a duplicate log contract.
- Exact reference and history checks confirm `operational.ts` has no consumers and its `logs/event.ts` dependency never existed in the introducing commit. It is broken dead code, so it has been removed instead of creating a second runtime-state/log contract.
- macOS sidecar preparation now succeeds for `aarch64-apple-darwin` and emits a target-specific executable plus SHA-256. After removing the dead state file, TypeScript has zero errors and the Vite production build succeeds.
- The second workspace attempt passes the sidecar prerequisite but reaches a new Tauri build-macro blocker: required `src-tauri/icons/icon.png` is absent. This is a packaging-source issue, not a Rust test failure.
- After preparing the macOS sidecar and generating the required icon assets, the full Rust workspace passes: 146 tests across 34 suites.
- Rust formatting is mechanically out of date in the new Optical files. Clippy's only blockers are one collapsible Anthropic capability match and two manual zero-check divisions in render geometry; no broader static-analysis failure is currently reported.
- After targeted fixes and rustfmt, Rust format check and Clippy with `-D warnings` pass; the full workspace remains green at 146 tests.
- The repository tracks four root SVG logo candidates and only a Windows `src-tauri/icons/icon.ico`; macOS PNG/ICNS assets do not exist. Target-specific sidecar binaries and duplicate generated schemas are untracked and need commit-readiness classification.
- `icons/icon-2.svg` is the product-relevant coding-cat/laptop mark. The generic `logo.svg`/`icon-1.svg` contains an unrelated BSC-style coin, so it should not be used as the desktop application icon.
- Tauri icon generation from `icon-2.svg` now produces a valid 512px PNG, macOS ICNS, and Windows ICO. Visual inspection confirms the coding-cat mark is centered, legible, and product-relevant.
- The frontend bundle still has large lazy chunks (`AreaChart` about 386 KB and main vendor/index about 357 KB); this is a performance-polish item, not a current compile blocker.
- A one-off npm audit against the official registry reports 0 known vulnerabilities; the user's configured mirror was left unchanged.

## Updated delivery requirement

- The user explicitly authorizes remote submission and requires the entire existing change set to be committed once before later phase commits.
- Every commit and branch operation must be recorded with timestamp and one-line summary in a project JSON file, with a reusable query script and a rule documented at the repository top level.
- Before the first aggregate commit, the full dirty tree must be reviewed for secrets, generated binaries, incomplete moves, and failed gates; remote push must only follow a successful local commit and audit record.
- The active branch is optical, tracking origin/optical at cf65cc4.
- A targeted scan of all changed/untracked files found no high-confidence API key, GitHub token, AWS key, private-key block, or long Bearer-token patterns. This is path/pattern evidence, not a substitute for final diff review.
- The only files over 10 MB outside build/dependency directories are two already tracked Windows proxy binaries and the newly generated macOS proxy binary. The macOS binary must remain generated/ignored rather than enter the aggregate commit.

## Product/code mapper evidence update

- The newest Optical planning source is `docs/experiments/OPTICAL_HYBRID_CONTEXT_OPTIMIZATION_PLAN_V7.md`, with `docs/optical/architecture-v2-iteration.md` and `review-round-3.md`; V6/round-2 are historical.
- A read-only mapper freshly verified Optical tests: dtoken-types 2 passed, dtoken-compress 55 passed, and dtoken-core-api Phase B 4 passed. These focused results still need the full workspace gate after sidecar preparation.
- Current source advances beyond the backend audit: OS keyring write and short-lived credential resolution paths now exist. Real authenticated-provider proof on macOS/Windows is still absent, so the capability remains partial.
- Production request routing currently uses core-api's first protocol match while the separate dtoken-router decision engine is mainly test-only; this duplicate routing architecture must converge before richer health/model/priority/cc-switch selection rules.
- `get_desktop_data` remains the largest frontend truth-boundary debt because it contains hardcoded health/savings/routes/providers/changes. Pages must migrate to verified d-token-owned evidence rather than extending that payload.
- Smart Learning correctly fails closed in the current Tauri UI but lacks the PRD-required native scan/deep-scan/delegation/managed-block transaction.
- Tauri lifecycle still has a commit-boundary risk: manifest write failure can warn while reporting service start success. Installation/config patching also lacks a fully atomic write/verify/automatic-restore state machine.

## Frontend audit interim P0 evidence

- Install still generates an Agent-facing provider URL that points back to d-token's own listener, while the backend now correctly rejects that self-loop. The primary configuration flow is therefore internally contradictory and cannot be called complete.
- Install initial Agent and cc-switch facts still come from fixtures, so macOS/Tauri first paint can display `/usr/local/bin` and detected/synced states without native evidence.
- Changes accepts unsourced `get_desktop_data` content as desktop truth even though the Tauri command contains hardcoded facts; generic approval can reduce to saving settings plus hiding a frontend row rather than a durable config transaction.

## Backend audit interim release blockers

- macOS CI runs Clippy/workspace tests before preparing the external sidecar, so it reproduces the local sidecar failure; workflow ordering must be corrected.
- macOS signing credentials are imported after bundle creation while notarization is requested, so produced artifacts cannot yet be treated as correctly signed/notarized candidates.
- `app_state_dir()` falls back from Windows LOCALAPPDATA to system temp on macOS; config, manifest, ledger, and backup persistence is therefore unsafe across cleanup/restart.
- Fresh-start placeholder upstream configuration violates the fail-closed rule for an unconfigured provider.
- Production Tauri invoke registration still exposes test/benchmark/smoke/readiness commands, and one path retains a raw-output summary. These commands need dev gating/removal and stricter redaction.
- Service stop identity checks do not yet bind the manifest PID to the expected executable/port owner strongly enough to rule out PID reuse.
- Only the main events ledger has improved locking/corruption handling; route/request/CCR/memory persistence and SQLite query durability remain incomplete.

## Aggregate delivery blocker resolution

- The documentation move is now structurally complete. The checker requires canonical history/product/release/testing/experiments/notes paths, rejects the old root copies and parallel PRDs, and all current Markdown relative links pass. Historical bodies remain untouched except the intentionally current Optical Phase A/B report.
- macOS CI now prepares a debug sidecar before workspace checks and a release sidecar before bundling. When notarization is requested, the signing certificate and resolved codesign identity are available before `tauri build`; notarization remains after bundle creation.
- The generated coding-cat assets are sufficient for current desktop targets: macOS uses tracked PNG/ICNS and Windows uses the matching ICO. Android/iOS and unused Windows-store raster variants are generated artifacts, not source.
- The first provider-agnostic OpenCode script revision still violated the product contract by injecting `OPENCODE_CONFIG_CONTENT` and isolating XDG data/state. Those mechanisms are supported by OpenCode but explicitly prohibited for this acceptance path; the correct test must run normal OpenCode state and use a backed-up minimal real-config transaction when adding the temporary d-token provider.
- The local OpenCode CLI exposes the normal config directory through `opencode debug paths`. The real-test harness can use that path without reading or printing `auth.json`, and must keep any config backup beside the user's config with mode 0600 rather than inside the repository.
- The active delivery branch is now `codex/prd-macos-polish`; the dirty worktree was carried across intact after recording both branch operations. No further implementation should occur on `optical`.
- The current macOS `.app` bundle was produced before late ContextGraph/PlanIR/config edits and is therefore stale evidence. It proves the packaging path once worked, but a new build and real-window run are required after the Rust tree is green.
- Frontend unit/type/build evidence remains green on the new branch (40 tests, TypeScript, Vite). This still does not substitute for Playwright or a real Tauri window walkthrough.
- The aggregate baseline is now remote at `ad276cc`; all later claims and real tests must identify a newer exact commit if P0 slices change runtime behavior.
- Current Tauri P0 debt is concentrated in one file: macOS falls back to temp storage, default startup creates a placeholder provider/route, `get_desktop_data` mixes hardcoded facts with live calls, and release invokes still expose test/benchmark commands. These are now a single owned implementation slice to avoid concurrent edits to `src-tauri/src/main.rs`.
- Current frontend fail-closed safety is intentional: Changes stays locked until Tauri emits the full runtime identity/freshness/transaction-provenance contract. The next Tauri slice must produce that contract from real service/draft/backup evidence rather than weakening frontend validation.

## P0 convergence evidence

- Tauri now uses persistent macOS, Windows, and Linux state roots; settings, drafts, active config, rollback points, and lifecycle manifests use synchronized same-directory atomic replacement. Windows uses `MoveFileExW` with replace and write-through flags, but Windows CI/real-machine compilation remains required evidence.
- Service start now requires a non-empty applied provider/route/instance config, loopback-owned local binding, and a non-self-loop upstream. Start success requires durable manifest read-back plus PID, executable, listener, and health identity evidence.
- `get_desktop_data` now emits complete/fresh provenance only when both health and dashboard responses prove the d-token-owned runtime. Unavailable or corrupt local state produces empty operational collections and no synthetic health, savings, routes, logs, or changes.
- Runtime route selection no longer chooses the first matching route. Explicit route/instance evidence wins; a uniquely matching model can disambiguate; conflicts and ambiguity fail closed.
- The JSONL durability improvement is still process-local and performs `sync_data` per record. Cross-process locking, batched durability, multi-file atomic transactions, persistence-error propagation, and long-running rotation/backpressure remain later hardening work.
- Install now derives Agent, cc-switch, service, provider, and route presentation from desktop/runtime evidence. It provides manual connection fields without claiming third-party config writes. Browser preview and Tauri-unavailable states are visibly distinct.

## 2026-07-11 continuation context

- `.planning/.active_plan` currently points at an independent Hybrid Context Optimization Engine task whose scope explicitly excludes frontend work. The user's current request is the macOS-first PRD/UI delivery tracked by the root `task_plan.md`, `findings.md`, and `progress.md`; do not mix the two plans or let the unrelated pointer override this task.
- The current continuation priority is Overview live-data truth, unified top window layer, restrained Claude-inspired typography/color/motion, full frontend verification, real macOS-window acceptance, then the installation transaction and real OpenCode route closure.
- Re-inspection of the user's Overview screenshot confirms the original issue was structural, not cosmetic: the title appeared in both the window bar and page body; a white top strip, hard vertical brand divider, full-width page separator, and detached bottom status bar created four competing horizontal layers. The current shell pass must leave one continuous top/window surface and one quiet status surface.
- The Install screenshot shows semantic-color overload: selected navigation used success green across background, icon, text accent, and underline. Selection should use the warm coral interaction accent; green must remain reserved for verified success so the UI does not imply installation has already succeeded.
- Current Playwright coverage is behavior-oriented only: four scenarios run in Chromium and macOS WebKit at 1180×780 with reduced motion, yielding 8 cases. It does not yet cover the required compact 920×640 layout, normal-motion appearance, dark theme, keyboard traversal, or screenshot assertions.
- Existing E2E correctly proves browser preview fail-closed behavior and scans all primary pages/workbench tabs for forbidden test vocabulary, but real Tauri window evidence still needs a separate launch/capture/interaction procedure because Playwright is serving the browser build and explicitly asserts `__TAURI_INTERNALS__` is absent.
- The macOS build/run skill prefers one stable build-and-launch entrypoint. This project already has an established Tauri-aware entrypoint (`npm run tauri:dev` / `npm run tauri:build`) that prepares the target sidecar; use it instead of creating a conflicting root `script/` directory outside the repository's `support/` boundary.
- GUI acceptance must fetch fresh accessibility state after each action and pair it with screenshots; claims about window hierarchy, keyboard focus, theme, and native-only controls cannot be inferred from browser Playwright alone.
- Appearance controls are safe to exercise in browser E2E: preview mode updates the local React/theme context but explicitly reports that it was not persisted. Theme/accent controls expose `aria-pressed`, so dark/light and selection-state checks can use semantic locators without triggering native configuration writes.
- The macOS OpenCode real-route harness is intentionally stricter than a smoke command: it requires an explicitly selected existing `PROVIDER/MODEL`, an explicit matching OpenAI-compatible upstream URL, and either an OS credential reference or explicit no-auth declaration. It refuses `auth.json`, literal keys, config-content overrides, temporary HOME/XDG isolation, symlinks, JSONC, duplicate keys, and provider conflicts.
- Its dry-run contract is safe to run before real traffic: it validates the normal config path and planned route without editing config, creating a backup/report, starting d-token, executing OpenCode, or contacting upstream. A real run will still require exact provider/protocol selection from non-secret local evidence.
- A concurrent, unrelated documentation archival move appeared while the UI agents were working (`docs/experiments`, `docs/optical`, product matrices, research, reviews and notes moved under `docs/archive/**`). It also moved this macOS/PRD planning set from the root into `docs/archive/agent-runs/2026-07-10-11/`. Preserve the move, do not revert or mix it into scoped UI/Rust edits, and keep recording current work at the relocated paths.
- Sol's focused graph audit separates immediate UI truth fixes from backend-required work: Analytics/LogsSecurity/Terminal/SmartLearning/Compatibility/Changes still have explicit fixture or preview fallback paths; the first-run deadlock requires a desktop-owned transaction snapshot plus plan-bound backup/apply/verify/rollback rather than more shell polish.
- Smart Learning currently has no native scan call at all: browser mode starts a 900ms timer and fabricates three Windows/macOS project paths, timestamps, scores, languages and Agent-hit explanations, while Tauri mode disables the same action. Until a real scanner contract exists, the only truthful release behavior is one consistent unavailable state with no scan timer, candidate list, fake score, fake path, or pseudo-output.
- The existing Smart Learning unit test explicitly required the fake browser path and candidate (`D:/D-TOKEN/d-token`), so removing production mock semantics also requires replacing that test with a cross-runtime invariant: scan remains disabled, no preview/sample copy or candidate path exists, and no completed-write language is rendered.
- Logs currently violate the same truth boundary twice: browser preview substitutes `fixtures.logs`, and native export failure silently falls back to a frontend-built download that bypasses the runtime's authoritative redaction/export path. Browser/offline must show zero real rows and disabled export; native export failure must remain a visible failure.
- The shared `Terminal` is a visual simulation rather than a log viewer: it invents a `tail -f ~/.dtoken/events.log` path, CLI command, prompt/cursor, and 220ms replay on every refresh. It should render the supplied redacted rows immediately under a plain “脱敏运行日志 · N 条” header, with a truthful zero-row empty state.
- LogsSecurity and Terminal currently have no direct component tests; only InstallWorkbench mocks the entire Logs page. The fixture cleanup therefore needs new focused tests for zero browser rows, disabled browser export, native export failure staying failed, and Terminal rendering only supplied lines without shell/path/prompt text.
- `LogEntry` itself is defined beside twelve fabricated log rows in `fixtures.ts`, forcing truthful production components to import a mock-data module even when they only need the type. Move the shape to a runtime-owned type module, then remove both LogsSecurity and Terminal imports from `fixtures.ts`.
- LogsSecurity also paints an unverified “默认脱敏策略 / 已启用” success row in browser/offline mode, treats a missing credential reference as healthy, substitutes the current clock for missing log timestamps, and prints `0 tok` when token evidence is absent. These must become warning/unknown/`—` states rather than synthetic healthy evidence.
- Changes still imports fixture `changes/backups/ownership`: browser preview renders fake pending changes and rollback rows, `buildChanges` defaults missing route ports to 18001 and fabricates before/after prose, while `buildBackups` hardcodes port 18001 and process count 0. Immediate truth cleanup should make preview/empty arrays empty and preserve only fields explicitly supplied by desktop evidence.
- Changes currently requires runtime identity as well as transaction provenance to classify desktop evidence, which contributes to the first-run/runtime-offline deadlock. The immediate UI fix can remove mock rows and fake ownership, but the final action gate must move to the forthcoming desktop-control-plane snapshot rather than weakening the existing fail-closed classifier prematurely.
- Changes tests currently institutionalize browser fixture rows by asserting disabled Approve/Reject/Rollback buttons exist in preview. Those assertions must become truthful empty-state assertions; real-payload safety tests remain valuable and should be updated to supply explicit backup ports/process evidence rather than relying on hardcoded UI values.
- Current `get_desktop_data` exposes `active_config` plus route-draft paths but no verified external Agent config locations or desktop state-root field. Therefore Config Ownership may list only those actual d-token paths; Agent, cc-switch, and invented `~/.dtoken/events.db` paths must remain unknown/absent until the control-plane snapshot adds evidence.
- After removing page-level mock consumers, production still imports `Health` and `DoctorCheck` types from `fixtures.ts`, so the entire fixture module remains reachable in the release bundle despite no longer needing its values. Move those pure types into runtime/doctor type modules, then remove the compatibility re-export and fixture file once no production/test imports remain.
- Doctor's remaining browser rows are warning-only disclosure statements (desktop window required, no local config read, loopback/redaction boundaries), not fabricated health results. They can remain after moving the type; deleting `fixtures.ts` will not remove those truthful product safeguards.
- The post-cleanup production scan found one residual `runtime_route` fabrication inside LogsSecurity. The same mapper also hardcodes OpenCode as Agent, `fallback=disabled`, `status=ok`, and `0ms` when those fields are absent. These are evidence-label bugs even though source rows are real and must be converted to `未知/—` or omitted fields before claiming the log truth phase complete.
