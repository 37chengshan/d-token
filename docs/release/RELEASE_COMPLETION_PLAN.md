# d-token Release Completion Plan

Status: active release execution plan
Date: 2026-07-06
Truth source: `docs/PRD.md`

## 0. Whole-release execution rule: plan globally, then implement in batches

From this point, d-token release work must not continue as isolated one-off fixes. d-token is a complete desktop control plane plus local runtime plus agent routing/compression product. The whole scope must be planned first, reviewed, and then implemented in coherent batches.

The required execution flow is:

```text
Audit PRD / current code / current UI / current runtime
-> Define the complete implementation scope
-> Update release plan + UI capability matrix + runtime capability matrix
-> User review / approval checkpoint
-> Batched implementation, not single-issue patching
-> UI / desktop-window verification
-> Runtime / Tauri / Event Ledger / OpenCode real verification
-> Planned correction batch
-> Final Release Readiness Report
```

### 0.1 Final target

This release track is not just "fix a few pages". The target is a release-candidate quality d-token system:

```text
d-token release candidate
= polished desktop App
+ d-token-owned Headroom-derived runtime
+ all PRD-facing formal UI surfaces
+ UI data from d-token runtime / Tauri / Event Ledger when available
+ fixture/sample data only as explicit browser preview
+ OpenCode as the first real agent validation entry
+ install/connect/configure/verify/diagnose/log/export/rollback flow
+ developer tests kept in support/, CI, hidden dev mode, and reports
```

### 0.2 Full implementation tracks

| Track | Must be planned together | Not allowed | Evidence |
|---|---|---|---|
| UI product surface | Shell, Sidebar, Topbar, Overview, Install, Advanced, Analytics, Doctor, Logs, Changes, Settings, Compatibility | Random page-by-page fixes without global scope | Browser/Tauri visual checklist, DOM/style checks, forbidden-term scan |
| Frontend/backend data handoff | Every page must define preview / unavailable / real-runtime states and source rules | Showing fixture totals as real desktop runtime data | `useRuntimeDashboard`, Tauri command checks, runtime payload source checks |
| Install / Connect / Verify | Connect d-token, select agent, configure provider/route, diff/backup/apply, verify, navigate to Doctor/Dashboard/Logs | Visible workload/benchmark/smoke/test-center UI | Tauri action evidence, active config, rollback point, route verification |
| Headroom-derived runtime | Compression, CCR, memory/learn, output shaping, stats/history, dashboard primitives, integrated RTK, Doctor, export | Depending on external Headroom 8787 or standalone RTK as product runtime | Runtime endpoints, Event Ledger, redacted export, release scripts |
| Routing / Provider / cc-switch | Official/custom upstreams, OpenAI/Anthropic-compatible routes, cc-switch/chain bridge, local provider model | Making the product OpenCode-only | route decision, provider_forward, active config, no silent fallback |
| Event Ledger / Logs / Security | route decision, compression, provider_forward, config_patch, install_step, error, JSON/CSV export | Persisting raw prompt/response/output/key | artifact redaction scan, export content audit |
| Project policy / auto scan | scan -> recommendation -> saved local policy -> UI readback | React-only state or fixture-only rows as if saved | `recommend_project_policy`, `save_app_settings`, readback |
| OpenCode real validation | Use original OpenCode config to prove real traffic passes through d-token | Extending this permission to other agents or making OpenCode the UI center | redacted OpenCode report, ledger/metrics/dashboard evidence |
| Release readiness | build/test/gate/report, performance, red lines, known limitations, packaging gaps | Claiming completion without a final report | frontend build, Rust tests, release gate, readiness report |

### 0.3 Batch order

| Batch | Name | Goal | Completion standard |
|---|---|---|---|
| G0 | Plan freeze | Finalize the release plan, UI matrix, runtime matrix, and verification order | User-reviewed plan before more broad implementation |
| G1 | UI product hardening | Product language, visual quality, empty states, navigation, interaction consistency | UI checklist passes and no test-language drift |
| G2 | Frontend/backend contract | Every page has correct real-data source boundaries | desktop unavailable never displays fake real data |
| G3 | Install/connect/apply/verify | User flow maps to real Tauri/runtime evidence | Install flow checklist + route verification evidence |
| G4 | Runtime capability closure | Headroom-derived runtime capabilities have assertions | no-touch release gate covers PRD runtime capabilities |
| G5 | OpenCode real gate | First real agent path proves OpenCode -> d-token -> upstream | OpenCode report + redaction audit |
| G6 | Correction batch | Fix findings from G1-G5 as a grouped pass | focused gates pass, then full gate passes |
| G7 | Release hardening | Build/test/report/performance/known-limits/packaging gap closure | `docs/RELEASE_READINESS_REPORT.md` is reviewable |

### 0.4 Default product decisions

1. No developer-test concepts in formal UI: workload, benchmark, smoke, fixture, mock, fake-bin, and testing center stay in `support/`, CI, hidden dev mode, or reports.
2. OpenCode is the first real validation entry, not the product center. The formal UI remains a multi-agent control plane.
3. Browser or desktop-window checks prove UI only. Real integration proof requires d-token-owned runtime/API/Tauri command, Event Ledger, metrics/export, and routed OpenCode evidence.
4. External Headroom/RTK/cc-switch services are no-touch references only. Product completion must come from d-token-owned runtime.
5. Do not claim OS-level service registration unless it is implemented. App-managed d-token service start/stop is now identity-checked and verified; external Headroom/RTK/cc-switch remain no-touch.
6. Any new gap must first be assigned to this plan or a capability matrix row, then implemented in the relevant batch or correction batch.

### 0.5 Implementation method after review

After the plan is reviewed, each batch uses the same method:

1. Re-check the PRD and matrix rows for the batch scope.
2. Modify all relevant UI, Tauri, runtime, script, and doc files together.
3. Run UI/build checks first, then runtime/API/Tauri checks.
4. Run the real OpenCode gate only at planned checkpoints, not after every small UI edit.
5. Record findings in `findings.md` and the matrices, then fix them in G6.
6. Summarize final evidence and limits in `docs/RELEASE_READINESS_REPORT.md`.

## 1. Why this plan exists

Recent work has been too incremental: fixing one UI/runtime handoff gap at a time. That is useful for bug cleanup, but not enough for a release-grade d-token app. From this point, implementation should move in planned batches:

1. Audit the whole PRD-facing product surface.
2. Write the complete implementation and verification plan before broad code changes.
3. Review the plan before implementation.
4. Implement all approved items in coherent batches, not as isolated one-off fixes.
5. Run UI verification and runtime verification separately.
6. Fix findings from those verifications in one or more planned correction batches.
7. Produce a release-readiness report.

This plan keeps formal UI product-safe: no visible workload benchmark, smoke test, fake/fixture test, or testing center in the normal user flow.

## 1.1 Required execution protocol

This protocol is mandatory for the rest of the release work. It exists because d-token is now a whole desktop product plus local runtime, not a collection of disconnected UI fixes.

```text
PRD + current repo audit
↓
complete implementation scope
↓
reviewable plan and matrices
↓
user approval / product decision checkpoint
↓
batched implementation
↓
frontend build + UI verification
↓
runtime/API/Tauri verification
↓
OpenCode real validation at the approved checkpoint
↓
planned correction batch
↓
final release readiness report
```

Rules:

1. Do not start new broad feature implementation until the relevant plan section and matrix rows exist.
2. Do not implement “one bug at a time” when the issue belongs to a broader product surface. Group related UI, native command, runtime, docs, and verification work into a batch.
3. Every batch must define:
   - user-facing product job,
   - PRD capability,
   - frontend surface,
   - runtime/Tauri/API source,
   - verification command or UI check,
   - known release limitation if not fully complete.
4. UI verification and real capability verification are different gates:
   - Browser/Tauri window checks prove layout, copy, interaction, and visual quality.
   - Runtime checks prove d-token-owned service/API/Tauri commands, Event Ledger, metrics/export, and routed agent flow.
5. OpenCode real validation is not a UI feature. It is the first authorized real agent proof path and should run at planned checkpoints, not after every tiny UI edit.
6. Formal UI remains multi-agent. OpenCode is one selectable agent in the same install/connect/configure/verify model.
7. If implementation discovers a new gap, add it to this plan or the matrices first, then decide whether it joins the current batch or becomes a later batch.

## 1.2 Whole-product completion map

The release work is tracked as one integrated completion map, not separate random task lists:

| Track | Must be planned together with | Why |
|---|---|---|
| UI information architecture | PRD page list, product language, browser/Tauri visual checks | Avoid building test/dev concepts into the final app |
| Runtime/API payloads | Dashboard, Analytics, Logs, Doctor, export, Event Ledger | UI must show real d-token-owned data where available |
| Tauri/native commands | Install/connect/verify, route config, settings, backup/rollback | Desktop actions need safe local read/write loops |
| Headroom-derived capability migration | Compression, CCR, memory/learn, output shaping, integrated RTK, stats/history | The product promise is not just a shell around one dashboard |
| Agent/provider routing | Provider profiles, route profiles, cc-switch interface, no silent fallback | d-token must be the routing/compression layer |
| Verification | UI checks, no-touch runtime gate, redaction audit, OpenCode real validation | Release readiness needs evidence, not only screenshots |
| Docs/agent instructions | PRD, AGENTS, CLAUDE, matrices, release report | Future agents must not drift back to wrong architecture |

## 2. Non-negotiable product chain

```text
OpenCode / Claude Code / Codex / Cursor / Cline / other AI coding agent
↓
d-token local entry / managed wrap
↓
d-token local service
= Headroom-derived d-token-owned runtime
= compression / CCR / memory / learn / output shaping / metrics / stats / history / dashboard primitives
= integrated RTK internal capability
↓
d-token route adapter
↓
official upstream / custom upstream / cc-switch route interface / local provider
↓
d-token Event Ledger / Dashboard / Doctor / Logs / App
```

OpenCode is the first authorized real validation entry, not the final product center.

## 3. Formal UI information architecture

Normal users should see these surfaces, all using install/connect/configure/verify/diagnose/status/log/export language:

| UI surface | Product job | Must be backed by | Status target |
|---|---|---|---|
| Overview / Dashboard | See runtime status, savings, activity, route/provider health | `get_dtoken_dashboard_payload`, Event Ledger summaries | Real runtime when available; explicit browser preview otherwise |
| Install / Connect | Connect d-token runtime, choose agent, configure route, preview diff, apply, verify | install transaction, route draft/apply, route verification | No fake stop/start states; no test cards |
| Advanced / Providers & Routes | Manage provider profiles, route profiles, cc-switch preference, active config | route draft/apply/readback, app settings | Near-real-time refresh |
| Advanced / Project Policy | Scan project, recommend compression level, apply/read saved policy | `recommend_project_policy`, `get_app_settings`, `save_app_settings` | Saved policy readback |
| Analytics | Inspect savings, latency, throughput, route overhead, RTK/runtime split | runtime history/metrics payload | Empty waiting state in desktop unavailable |
| Doctor | Diagnose service, agent config, route, redaction, ledger/export health | `detect_tools`, dashboard payload, active config, route verification | Manual Run All plus auto-refresh evidence |
| Logs & Security | View redacted request logs, route decisions, Event Ledger, exports, credential boundaries | runtime logs/events/export commands | JSON/CSV redacted export |
| Changes / Rollback | Review diffs, approve route drafts, rollback d-token active config | `get_desktop_data`, `apply_route_profile_draft`, `manage_backup` | Polling real local state |
| Settings | Appearance, runtime mode preference, redaction, telemetry, language | `get_app_settings`, `save_app_settings`, global theme hydration | Saved state affects app startup |
| Compatibility Matrix | OS/shell/agent/provider support status and notes | d-token-owned canonical compatibility/protocol/chain model | Mostly covered; final Tauri visual pass remains |

## 4. Remaining implementation batches

The batches below are the execution backlog. They are meant to be implemented as coherent passes, not as isolated tickets. A batch may touch frontend, Tauri commands, runtime payloads, scripts, and docs together when that is what makes the product state true.

## 4.0 Default decisions for this release pass

Unless the user changes direction, proceed with these defaults:

1. **Persistent service lifecycle**
   - Do not claim full OS-level start/stop from the normal UI until identity-checked lifecycle commands are implemented and verified.
   - For this pass, the normal Install surface says connect/install/configure/verify.
   - If a temporary d-token-owned service is needed for verification, it stays in scripts/reports or a product-language verify action, not a visible benchmark/test center.
2. **Compatibility Matrix**
   - Keep it as a first-class visible page because PRD §16 requires a compatibility matrix and the user wants a release-grade desktop control plane.
3. **OpenCode real validation cadence**
   - Run no-touch runtime gates after each broad runtime batch.
   - Run the real user-authorized OpenCode validation once after the UI/runtime handoff is coherent, and again only if the validation batch changes routing/install/verification behavior.
4. **Packaging scope**
   - Current release-readiness pass requires desktop build, Tauri Rust tests, runtime verification, redaction audit, and artifact report.
   - Full signing/notarization/installer publishing is tracked as release hardening unless the user explicitly asks to ship installers now.
5. **Frontend button-driven real validation**
   - Product UI may have buttons such as `验证配置`, `诊断连接`, `应用配置`, `刷新状态`, `导出日志`.
   - Those buttons may call real Tauri/runtime commands and may internally reuse OpenCode validation/hook/config checks.
   - Normal UI must not call the feature `benchmark`, `smoke`, `workload`, `testing center`, `fixture`, or `fake-bin`.
   - The evidence from those actions is stored as redacted product status, Doctor results, Dashboard events, Logs/Event Ledger rows, and support artifacts.

## 4.1 Release execution board

| Batch | Scope | Product outcome | Primary files | Verification gate |
|---|---|---|---|---|
| R0 | Plan freeze and matrix alignment | One approved whole-product implementation plan | `docs/RELEASE_COMPLETION_PLAN.md`, `docs/UI_CAPABILITY_MATRIX.md`, `docs/RUNTIME_CAPABILITY_MATRIX.md`, `task_plan.md` | Plan/matrix self-check |
| R1 | Frontend shell and visual quality | Desktop window feels coherent, stable, and polished | `Sidebar.tsx`, `Topbar.tsx`, `StatusBar.tsx`, `theme.css`, `Settings.tsx` | Browser/Tauri visual checklist |
| R2 | UI capability coverage | Every PRD-facing capability has a visible product surface or explicit deferral | all `components/pages/*.tsx`, `nav.ts`, `i18n.ts` | UI matrix + forbidden-term audit |
| R3 | Real-time data contract | Pages use d-token runtime data when available and never fake desktop-unavailable data | `useRuntimeDashboard.ts`, page data adapters, Tauri `get_dtoken_dashboard_payload` | Runtime source check + UI empty-state check |
| R4 | Runtime payload completeness | Dashboard/Analytics/Logs/Doctor receive all required metrics and evidence fields | `dtoken-core-api/src/lib.rs`, release scripts | Rust tests + metrics/export assertions |
| R5 | Install/connect/configure/verify flow | User can connect d-token, select agent, apply route config, verify, and reach Doctor/Dashboard/Logs | `Install.tsx`, Tauri route/install commands, config patcher/runtime scripts | Install UI checklist + route verification mock |
| R6 | Headroom-derived capability surfacing | Compression, CCR, memory/learn, output shaping, integrated RTK, stats/history are visible as product state | `Overview.tsx`, `Analytics.tsx`, `Advanced.tsx`, `Doctor.tsx`, runtime endpoints | Capability matrix + runtime endpoint tests |
| R7 | Logs/security/export/rollback | User can inspect redacted events/logs, export JSON/CSV, and roll back d-token config | `LogsSecurity.tsx`, `Changes.tsx`, Tauri backup/export commands | Redaction audit + rollback tests |
| R8 | OpenCode real validation | First real agent path proves OpenCode -> d-token -> upstream with compression/ledger/dashboard evidence | `support/scripts/run_opencode_*`, release report, product verify action | Redacted OpenCode artifacts + no leaked raw output |
| R9 | Performance and release readiness | Build is performant, gates pass, report documents known limits | desktop build, Rust tests, release scripts | `docs/RELEASE_READINESS_REPORT.md` |

### Batch A — Product surface audit and cleanup

Goal: prove every PRD-facing capability has a formal UI surface and that no normal UI exposes dev-test language.

Tasks:

1. Build a source-level UI text audit for forbidden terms: workload, benchmark, smoke, fixture test, fake-bin, testing center, real-chain test, capability self-test.
2. Verify formal pages use multi-agent language, not OpenCode-only product IA.
3. Check all interactive controls that look persistent are either backed by native/local state or clearly marked browser-preview.
4. Check all runtime values in desktop unavailable state are empty/waiting, not sample data.
5. Create/update a UI coverage matrix in docs with page → PRD capability → backend/native source → verification method.

Deliverable:

- `docs/UI_CAPABILITY_MATRIX.md`
- Source audit result in progress/findings

### Batch B — Runtime and data-source completion audit

Goal: identify exact runtime gaps before more UI work.

Tasks:

1. Inspect d-token runtime endpoints for PRD coverage:
   - `/health`
   - `/stats`
   - `/stats-history`
   - `/metrics`
   - `/providers`
   - `/routes`
   - `/instances`
   - `/route-decisions/recent`
   - `/logs/recent`
   - `/dashboard/payload`
   - `/export/json`
   - `/export/csv`
   - `/ccr/summary`
   - `/memory/summary`
   - `/output-shape`
2. Verify Event Ledger writes for route decision, compression, install/config patch, provider forward, error cases.
3. Verify runtime payload marks source as `dtoken-owned-runtime` and desktop source as `dtoken-local-service` before UI treats it as real.
4. Identify missing endpoints/models and decide whether to implement or mark blocked for release.

Deliverable:

- `docs/RUNTIME_CAPABILITY_MATRIX.md`

### Batch C — Install / connect / verify product flow

Goal: make the formal install flow coherent and backed by real d-token actions.

Tasks:

1. Confirm Install page primary flow:
   - connect/check d-token runtime
   - detect supported agents
   - select agent
   - configure provider/route
   - preview diff
   - backup/apply d-token-owned route config
   - verify config
   - navigate to Doctor/Dashboard/Logs
2. Ensure route verification uses d-token-owned active config and runtime evidence, not external Headroom.
3. Keep OpenCode as selectable first real agent while preserving multi-agent layout.
4. Ensure no UI says it starts/stops a persistent service unless the Tauri command actually does that.
5. If real persistent service lifecycle is in scope for this release, implement explicit service start/connect/stop with identity checks. If not, the UI must say connect/observe runtime only.

Deliverable:

- Install flow manual UI checklist
- Updated Tauri/native command coverage if needed

### Batch D — Headroom-derived capability surfacing

Goal: all Headroom-derived capabilities have a visible product surface and a script/runtime proof path.

Capabilities:

1. Compression pipeline and savings.
2. CCR metadata and retrieval summary.
3. Memory / learn metadata.
4. Output shaping / verbosity.
5. Integrated RTK status/metrics as runtime-internal capability.
6. Stats/history/dashboard primitives.
7. Provider/route/cc-switch adapters.
8. Config diff/backup/rollback.
9. Doctor diagnostics.
10. Logs/export redaction.

Tasks:

1. Map each capability to UI page and runtime/script evidence.
2. For any capability missing from UI, add a product-safe surface.
3. For any capability missing runtime proof, add/extend support script or runtime endpoint.
4. Keep dev evidence in `support/scripts` and `support/artifacts`, not normal UI.

Deliverable:

- Updated capability matrix with pass/incomplete status.

### Batch E — UI polish and desktop quality pass

Goal: make the app feel release-grade.

Tasks:

1. Verify desktop and compact widths for all pages.
2. Check sidebar expanded/collapsed selected states, hover motion, tooltips.
3. Check topbar/sidebar square shell bar requirement.
4. Check translucent blur/glass readability in light/dark.
5. Check animated flowing background and saved appearance hydration.
6. Check buttons, tabs, segmented controls, tables, logs, badges, empty states.
7. Remove visual clutter and ensure no text overflow.
8. Confirm lazy-loaded page chunks and no large unexpected bundle regression.

Deliverable:

- Browser screenshot/manual checklist.
- Build artifact size summary.

### Batch F — Real OpenCode validation track

Goal: prove the authorized real OpenCode path works through d-token.

Rules:

1. Use the user original OpenCode config by default.
2. If config modification is required, back up first, write minimal diff, do not print/store secrets.
3. Do not extend this permission to other agents.
4. Report only redacted artifacts and metrics.

Validation must prove:

1. OpenCode request enters d-token local service.
2. d-token route adapter selects expected provider/route.
3. Compression runs and records before/after token estimates.
4. Event Ledger records events/routes/logs.
5. Dashboard payload reflects the request.
6. Export is redacted; raw prompt/response/secrets are not stored.
7. Hook/wrap/config effect is evidenced by active config and ledger route hit.
8. Project policy scan/recommend/apply/readback works.

Deliverable:

- `support/artifacts/release-verification-*.json`
- `support/artifacts/opencode-*.json`
- Final redaction audit result.

### Batch G — Release readiness gate

Goal: produce a publishable project state.

Commands/checks:

1. `npm run build` in `code/apps/desktop`.
2. `cargo test -p d-token-desktop` in `code` after frontend build completes.
3. Runtime fixture tests.
4. Mock routed runtime tests.
5. Active config runtime tests.
6. Agent route config verification.
7. Real OpenCode validation when explicitly running final real gate.
8. Port/process cleanup verification.
9. Artifact redaction verification.
10. Optional packaging check if Tauri packaging is in current scope.

Deliverable:

- `docs/RELEASE_READINESS_REPORT.md`

### Batch H — Planned correction batch after verification

Goal: convert verification findings into one planned cleanup pass instead of returning to ad hoc fixes.

Inputs:

1. Browser/Tauri UI screenshots and DOM checks.
2. Frontend build and bundle-size output.
3. Rust unit/integration test output.
4. Runtime release verification report.
5. OpenCode real validation report.
6. Redaction scan results.
7. Port/process cleanup report.

Rules:

1. Classify each finding as UI, runtime, Tauri/native, script, documentation, or release blocker.
2. Fix release blockers first.
3. Keep cosmetic findings grouped by page/shell, not scattered.
4. Re-run only the relevant focused gate first, then the full gate once the batch is complete.
5. Update `findings.md`, `progress.md`, and the matrices with what changed and what remains deferred.

Deliverable:

- Updated matrices with final status.
- Final focused verification output.
- Final full release gate output.

## 5. Verification order

Do not run frontend build and Tauri Rust tests in parallel. Vite can replace hashed dist assets while `tauri::generate_context!()` reads them.

Recommended order:

```powershell
cd D:\D-TOKEN\d-token\code\apps\desktop
npm run build

cd D:\D-TOKEN\d-token\code
cargo test -p d-token-desktop

cd D:\D-TOKEN\d-token
support\scripts\run_release_verification.ps1 -SkipOpenCode
# final real gate only when ready:
support\scripts\run_release_verification.ps1
```

## 6. Detailed implementation blueprint

### 6.1 Frontend polish blueprint

| Area | Required work | Acceptance evidence |
|---|---|---|
| Shell | Sidebar/topbar/status bar square outer frame, stable collapsed state, no selected indicator drift, subtle icon hover motion | Browser measurement and screenshots at expanded/collapsed widths |
| Overview | Activity calendar only for activity; token trend is chart; tools view is ranking/usage; runtime unavailable shows waiting state | Browser check plus dashboard payload mapping |
| Install | One coherent connect/configure/apply/verify flow; OpenCode is one agent; no fake service stop/start | UI checklist + `verify_agent_route_config` evidence |
| Advanced | Provider/route builder covers official/custom/cc-switch/local provider modes; project policy scan/apply/readback is clear | Tauri settings/config round-trip |
| Analytics | Shows layered savings, latency breakdown, route overhead, provider/route performance, RTK/runtime split | Runtime metrics payload and UI card check |
| Doctor | Runs diagnostics with product wording; shows route/config/redaction/ledger/export health; links to compatibility/logs | Tauri `run_doctor` + route verification result |
| Logs/Security | Redacted logs, route decisions, Event Ledger, JSON/CSV export, credential boundary | Export artifacts and redaction scan |
| Changes/Rollback | Pending diffs, approve route draft, rollback d-token active config, explain external config limits | Rollback unit test and UI copy check |
| Compatibility | OS/shell/agent/provider/route matrix from d-token-owned model; browser preview is labeled | Runtime/desktop compatibility source check |
| Settings | Theme, glass blur, flowing background palettes/speed, language, runtime/redaction preferences persist and hydrate on startup | Saved settings readback and startup hydration check |

### 6.2 Frontend/backend handoff blueprint

| UI need | Runtime/Tauri source | Update cadence | Must prove |
|---|---|---:|---|
| Global runtime state | `get_dtoken_dashboard_payload` → `/dashboard/payload` | ~2.5s polling | Payload source is `dtoken-owned-runtime` and desktop source is `dtoken-local-service` |
| Active route/provider | `get_active_route_config_status`, dashboard route fields | ~3s or action refresh | Provider/route shown matches active d-token config |
| Route draft/apply | `save_route_profile_draft`, `apply_route_profile_draft` | action + refresh | Draft creates pending change; apply writes active config and rollback point |
| Project policy | `recommend_project_policy`, `save_app_settings`, `get_app_settings` | action + ~3s readback | Saved level/features are visible after refresh |
| Install verification | `verify_agent_route_config` | action | Runtime health, active config, route loader, fallback policy, dashboard source, export redaction |
| Doctor | `run_doctor`, route verification, dashboard payload | action + auto refresh | Diagnostics are product-safe and no-touch unless explicitly managed |
| Logs/Event Ledger | dashboard payload + `/events`, `/logs/recent`, `/route-decisions/recent` | shared runtime poll | Events/logs are redacted and not sample data in desktop-unavailable mode |
| Export | `export_dtoken_runtime("json"|"csv")` | action | Artifacts contain no raw prompt/response/secrets |
| Compatibility | `get_desktop_data` now; `/compatibility/agents` target canonical model | ~5s | Rows are real desktop/runtime data or labeled preview |

### 6.3 Runtime/API completion blueprint

| Runtime area | Required implementation/verification | Evidence |
|---|---|---|
| Metrics and latency | `/stats`, `/stats-history`, `/metrics`, `/dashboard/payload` expose compression latency, route adapter latency, provider latency, cc-switch latency/disabled state, route overhead | Rust tests + metrics text assertions |
| Event Ledger | Route decisions, compression, install/config patch, provider forward, errors, CCR/memory metadata events are persisted without raw content | JSONL row assertions + redaction scan |
| Routing | Official/custom OpenAI/custom Anthropic/cc-switch/local provider modes have visible route decision and no silent fallback | Mock route tests + active config tests |
| Protocols | OpenAI chat, OpenAI responses, Anthropic messages bridge are either covered by release tests or explicitly deferred | Route tests + matrix status |
| CCR | `/ccr/summary` and retrieve metadata return `raw_available=false` by default | Endpoint test |
| Memory/learn | `/memory/learn` strips raw prompt/response/messages/content and `/memory/summary` summarizes metadata only | Unit/runtime test |
| Output shaping | `/output-shape` and project policy readback expose verbosity/output-shaper state | Endpoint + UI check |
| Integrated RTK | Runtime-internal RTK status/savings are visible as aggregate capability, not standalone user path | Runtime payload + UI wording audit |
| Compatibility | `/compatibility/agents` becomes canonical source for OS/shell/agent/provider cases or release deferral is explicit | Endpoint + Compatibility UI |
| Install scan/plan | `/install/scan`, `/install/plan`, Tauri transaction, diff/backup/apply/verify converge into one product flow | Install flow script + Tauri unit tests |

### 6.4 Product-button-driven real validation blueprint

The user wants real validation driven from the app flow. The product-safe interpretation is:

```text
Install page / Doctor page user actions
→ product-language buttons: Connect / Apply / Verify / Diagnose
→ Tauri command
→ d-token-owned runtime/API/script harness
→ redacted evidence
→ Dashboard / Logs / Doctor / support artifact
```

Required validations:

| Product action | Internal proof path | Required evidence |
|---|---|---|
| Connect OpenCode | Active route config points OpenCode traffic to d-token local service where needed | Backup path and diff summary if config is changed; no secrets |
| Verify route | `verify_agent_route_config` and route verification script | Health/config/dashboard/export/fallback checks |
| Prove compression | Routed request records before/after token estimates and transforms | Event Ledger + dashboard savings |
| Prove data updates | Dashboard payload changes after routed request | Request count, route decision count, updated timestamp |
| Prove hook/wrap/config effect | Active config and ledger route hit show agent traffic passed through d-token | Route hit evidence, not raw prompt |
| Auto scan policy | Project policy recommendation uses runtime/project inputs | Recommendation payload + saved settings readback |
| Write config | d-token-owned active config and rollback point are written | Active config status + rollback point |
| Headroom-derived capability coverage | CCR/memory/output shaping/integrated RTK/status endpoints are exercised | Runtime script report + UI summaries |

### 6.5 Release report blueprint

`docs/RELEASE_READINESS_REPORT.md` must include:

1. Exact commit/worktree summary.
2. UI verification summary with screenshots/checklist locations.
3. Runtime verification commands and pass/fail status.
4. OpenCode real validation artifact paths.
5. Redaction audit result.
6. Performance/build size summary.
7. Known release limitations and explicit deferrals.
8. Confirmation that external Headroom/RTK/cc-switch services were not mutated.
9. Final PRD acceptance checklist result.

## 7. Immediate next step from the current state

Current state as of 2026-07-07:

1. R4 Analytics/runtime metrics handoff is complete.
2. R5/R6 Install flow and Headroom-derived capability surfacing are complete for the current milestone.
3. G2/G4 canonical compatibility/protocol/runtime surface coverage is complete for the current milestone.
4. Product readiness command is in the release gate and proves 9/9 readiness checks against d-token active config plus d-token runtime metadata.
5. Headroom-derived capability coverage audit is in the release gate and proves 11/11 capability groups across runtime, frontend, and verification evidence.
6. Latest no-touch release gate `support/artifacts/release-verification-20260707-184716.json` passed and includes desktop packaging, bundle artifact, installed-dependency, signing-status distribution audit, Windows signing readiness audit, cross-platform release readiness audit, release artifact redaction audit, release cleanup audit, and release handoff manifest.
7. Latest full OpenCode gate `support/artifacts/release-verification-20260707-172419.json` is the current real-agent evidence and includes installer artifact verification.

Proceed in this order:

1. Actual Tauri/NSIS packaging has now succeeded and installer path/size/hash verification is part of the release gate.
2. The latest full release gate with OpenCode and installer artifact verification passed: `support/artifacts/release-verification-20260707-172419.json`.
3. Identity-checked app-managed d-token service lifecycle is now implemented and verified. UI may expose start/stop only for d-token-managed service state, while OS-level service registration remains a later hardening item if required.
4. Public distribution still requires signing/notarization/reputation hardening; current Windows signing readiness shows `can_sign_now=false` because `signtool.exe` is unavailable and no certificate was provided.
5. Any next broad implementation should target signing/release hardening or a planned product correction batch, then rerun no-touch and full OpenCode gates only when routing/runtime behavior changes.

## 6. Latest execution checkpoint - 2026-07-07 17:24

The current hard evidence checkpoint is `support/artifacts/release-verification-20260707-172419.json`. It passed with real OpenCode enabled after the service lifecycle and bundled runtime sidecar changes.

Completed in this checkpoint:

- App-managed d-token service lifecycle: implemented and verified.
- d-token runtime sidecar packaging: configured through Tauri `externalBin`, prepared for Windows GNU/MSVC targets, and verified by artifact audit.
- Regenerated NSIS installer after sidecar preparation: `code/target/release/bundle/nsis/d-token_0.2.0_x64-setup.exe`, SHA-256 `8a3ddecee780cfb2db2f1823305a62b878a43010d29da4891d2913e8a41cac22`.
- Full OpenCode route validation still passes after the lifecycle/package change.

Next release-hardening track:

- Signing/notarization/reputation hardening for public distribution.
- Optional OS-level service registration if the product decision changes from app-managed local process lifecycle to a true installed Windows/macOS service.

## 7. Installed Windows startup checkpoint - 2026-07-07 17:45

User startup evidence showed a real installed-app blocker: `d-token-desktop.exe` failed before UI load because `WebView2Loader.dll` was not found.

Resolution:

- `WebView2Loader.dll` is now prepared into `code/apps/desktop/src-tauri/binaries/WebView2Loader.dll`.
- Tauri `bundle.resources` now installs that file at the app root, next to `d-token-desktop.exe`.
- Packaging audit now checks WebView2Loader resource configuration and prepared file presence.
- Bundle artifact audit now checks WebView2Loader release/prepared hashes and requires the installer to be newer than WebView2Loader preparation.

Evidence:

- Focused packaging audit: `support/artifacts/desktop-packaging-audit-20260707-174051.json`, 13/13 checks passed.
- Focused bundle artifact audit: `support/artifacts/desktop-bundle-artifact-audit-20260707-174052.json`, 12/12 checks passed.
- Latest no-touch release gate: `support/artifacts/release-verification-20260707-174527.json`, passed.
- Rebuilt installer: `code/target/release/bundle/nsis/d-token_0.2.0_x64-setup.exe`, 9,956,611 bytes, SHA-256 `bec7724cbc3c99a0794a1e4f4e4423c1794a7e9d27adb5872683953171391d41`.
- Real silent install placed `d-token-desktop.exe`, `dtoken-proxy.exe`, and `WebView2Loader.dll` under `C:\Users\10777\AppData\Local\d-token`.
- Installed app launched successfully and stayed alive with window title `d-token Control Plane`.

OpenCode real routing was not rerun at this checkpoint because only installer startup packaging changed. Current real-agent routing evidence remains `support/artifacts/release-verification-20260707-172419.json`.

## 8. Release distribution manifest checkpoint - 2026-07-07 17:56

The release gate now includes a read-only distribution manifest audit.

Added:

- `support/scripts/run_release_distribution_audit.ps1`
- `release_distribution_audit` step in `support/scripts/run_release_verification.ps1`

Evidence:

- Focused distribution manifest: `support/artifacts/release-distribution-manifest-20260707-175305.json`
- Latest no-touch release gate: `support/artifacts/release-verification-20260707-175620.json`

Current status:

- `release_candidate_ready = true`
- `distribution_ready_public = false`
- `d-token-desktop.exe`: `NotSigned`
- `dtoken-proxy.exe`: `NotSigned`
- `d-token_0.2.0_x64-setup.exe`: `NotSigned`
- `WebView2Loader.dll`: valid Microsoft signature
- Installed dependency check passed when the installed directory existed: `d-token-desktop.exe`, `dtoken-proxy.exe`, and `WebView2Loader.dll` were all present under `C:\Users\10777\AppData\Local\d-token`.

Boundary:

- This checkpoint did not sign artifacts, install certificates, touch signing credentials, or mutate external Headroom/RTK/cc-switch services.
- The product is a local release candidate, not public-distribution ready until signing/reputation hardening is completed.

## 9. Release artifact redaction checkpoint - 2026-07-07 18:05

The release gate now includes a machine-enforced artifact privacy check.

Added:

- `support/scripts/run_release_artifact_redaction_audit.ps1`
- `release_artifact_redaction_audit` step in `support/scripts/run_release_verification.ps1`

Evidence:

- Focused redaction audit: `support/artifacts/release-artifact-redaction-audit-20260707-180220.json`
- Latest no-touch release gate: `support/artifacts/release-verification-20260707-180515.json`

Current status:

- Scanned latest release JSON artifacts: 8
- Raw prompt/response/output field hits: 0
- Credential-shaped value hits: 0
- JSON parse errors: 0

Boundary:

- The audit records only file paths, JSON paths, and pattern names.
- It does not store matched secret values.
- It does not read or mutate external Headroom/RTK/cc-switch services.

## 10. Release cleanup checkpoint - 2026-07-07 18:13

The release gate now includes a machine-enforced cleanup check after validation.

Added:

- `support/scripts/run_release_cleanup_audit.ps1`
- `release_cleanup_audit` step in `support/scripts/run_release_verification.ps1`

Evidence:

- Focused cleanup audit: `support/artifacts/release-cleanup-audit-20260707-180956.json`
- Latest no-touch release gate: `support/artifacts/release-verification-20260707-181333.json`

Current status:

- Checked release/test ports: `18001`, `38101`, `38113`, `38117`, `39122`
- Remaining listeners on those ports: 0
- Residual validation processes: 0
- Generated mock-upstream Python processes: 0

Boundary:

- The audit is read-only and does not kill processes.
- It intentionally does not fail on a normal installed/preview `d-token-desktop.exe` window.
- It does not start, stop, or reconfigure external Headroom/RTK/cc-switch/OpenCode services.

## 11. Installed desktop startup checkpoint - 2026-07-07 18:17

The installed-app startup evidence is now repeatable through a focused script.

Added:

- `support/scripts/run_installed_desktop_startup_audit.ps1`

Evidence:

- Focused installed startup audit: `support/artifacts/installed-desktop-startup-audit-20260707-181725.json`

Current status:

- Installed directory: `C:\Users\10777\AppData\Local\d-token`
- Installed `d-token-desktop.exe`: present
- Installed `dtoken-proxy.exe`: present
- Installed `WebView2Loader.dll`: present
- Installed `uninstall.exe`: present
- Installed app launch: passed
- Window title: `d-token Control Plane`

Boundary:

- This is a focused installer/startup gate, not part of every no-touch release run, because it briefly launches a desktop window.
- It closes the process it started.
- It does not start, stop, or reconfigure external Headroom/RTK/cc-switch/OpenCode services and does not read secrets.



## 12. Windows signing readiness checkpoint - 2026-07-07 18:30

The no-touch release gate now includes a read-only Windows signing readiness audit. This turns public-distribution signing from a vague caveat into a machine-checked boundary.

Added/integrated:

- `support/scripts/run_windows_signing_readiness_audit.ps1`
- `support/scripts/sign_windows_release_artifacts.ps1` dry-run plan, with real signing gated behind `-Execute`
- `windows_signing_readiness` step in `support/scripts/run_release_verification.ps1`

Evidence:

- Focused readiness audit: `support/artifacts/windows-signing-readiness-20260707-182612.json`
- Dry-run signing plan: `support/artifacts/windows-release-signing-20260707-182612.json`
- Latest no-touch release gate: `support/artifacts/release-verification-20260707-183020.json`
- Release-gate readiness subreport: `support/artifacts/windows-signing-readiness-20260707-183017.json`

Current status:

- Release artifacts exist and hashes match the current installer generation.
- `can_sign_now = false`.
- `distribution_ready_public = false`.
- `signtool.exe` was not found on this machine.
- No signing certificate path was provided.
- App exe, runtime sidecar, and NSIS installer remain `NotSigned`.

Boundary:

- No files were signed.
- No certificates were installed or loaded.
- No certificate password value was read or written.
- The dry-run signing plan was run without `-Execute`.
- Real signing requires explicit user action with signing toolchain and credentials intentionally available.


## 13. Release handoff manifest checkpoint - 2026-07-07 18:38

The no-touch release gate now ends with a compact release handoff manifest. This gives the project a single machine-readable delivery summary rather than requiring the reader to stitch together many audit files.

Added/integrated:

- `support/scripts/run_release_handoff_manifest.ps1`
- `release_handoff_manifest` step in `support/scripts/run_release_verification.ps1`

Evidence:

- Focused manifest: `support/artifacts/release-handoff-manifest-20260707-183504.json`
- Latest release-gate manifest: `support/artifacts/release-handoff-manifest-20260707-183847.json`
- Latest no-touch release gate: `support/artifacts/release-verification-20260707-183847.json`

Current handoff status:

- `local_release_candidate_ready = true`
- `public_distribution_ready = false`
- Public blockers are explicit:
  - app/runtime/installer Authenticode signatures are not valid for public distribution,
  - Windows signing toolchain/certificate is not ready on this machine.
- The manifest records app/runtime/WebView2/installer paths, sizes, SHA-256 hashes, signing status, and latest evidence report paths.

Boundary:

- The manifest does not sign files.
- It does not access certificates, certificate passwords, prompts, responses, API keys, cookies, bearer tokens, external service state, or agent auth files.
- It does not start, stop, or reconfigure external Headroom/RTK/cc-switch/OpenCode services.


## 14. Cross-platform release readiness checkpoint - 2026-07-07 18:47

The release gate now explicitly tracks the Windows/macOS release boundary required by PRD and TECH_STACK.

Added/integrated:

- `support/scripts/run_cross_platform_release_readiness_audit.ps1`
- `cross_platform_release_readiness` step in `support/scripts/run_release_verification.ps1`
- Cross-platform readiness fields in `support/scripts/run_release_handoff_manifest.ps1`

Evidence:

- Focused cross-platform audit: `support/artifacts/cross-platform-release-readiness-20260707-184255.json`
- Latest release-gate cross-platform audit: `support/artifacts/cross-platform-release-readiness-20260707-184712.json`
- Latest release-gate handoff manifest: `support/artifacts/release-handoff-manifest-20260707-184716.json`
- Latest no-touch release gate: `support/artifacts/release-verification-20260707-184716.json`

Current platform status:

- Windows local release candidate: ready.
- macOS local release candidate: not ready on this Windows host.
- Cross-platform local release candidate: false.
- Public distribution: false.

Current macOS blockers:

- Tauri bundle targets currently include `nsis`, not macOS `dmg`/`app` artifacts.
- No macOS artifact exists under the expected release bundle paths.
- macOS package generation requires a macOS runner.
- macOS code signing/notarization is not complete.

Boundary:

- The audit is read-only.
- It does not try to build macOS artifacts on Windows.
- It does not sign, notarize, install certificates, read credentials, or touch external Headroom/RTK/cc-switch/OpenCode services.
