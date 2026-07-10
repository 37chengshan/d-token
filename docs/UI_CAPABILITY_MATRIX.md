# d-token UI Capability Matrix

Status: updated with 2026-07-07 product readiness command handoff
Date: 2026-07-07
Truth source: `docs/PRD.md`
Purpose: stop one-off UI fixes. This matrix defines the whole App surface before the next implementation batch.

## 1. UI product rule

正式 UI 只表达普通用户任务：

```text
install / connect / configure / apply / verify / diagnose / status / logs / export / backup / rollback
```

正式 UI 不表达开发验证词：

```text
workload / benchmark / smoke / fixture test / mock test / fake-bin / testing center / 真实链路测试 / 能力自检
```

OpenCode 是第一条真实验收入口，但不是产品中心。UI 必须保持多 agent 控制面。

## 2. Current page-to-capability matrix

| Page | User job | PRD capability | Current frontend source | Native/runtime source | Freshness | Status | Gaps / next implementation | Verification |
|---|---|---|---|---|---|---|---|---|
| Shell / Sidebar / Topbar / StatusBar | See global runtime state and navigate | Multi-agent desktop control plane, runtime state, product-safe status | `code/apps/desktop/src/app/components/shell/AppShell.tsx`, `Sidebar.tsx`, `Topbar.tsx`, `StatusBar.tsx` | `useRuntimeDashboard()` → `get_dtoken_dashboard_payload` | Polls about every 2.5s | Mostly covered | 2026-07-06 pass: topbar/sidebar outer radius is 0; collapsed selected item is fixed 40x40 and centered. Remaining: optional final visual screenshots across light/dark/compact widths | Browser/Tauri UI screenshot check; source text audit |
| Overview / Dashboard | Understand runtime health, savings, route/provider activity, activity trend | Dashboard primitives, stats/history, layered savings, route/provider health | `Overview.tsx`, `Heatmap.tsx`, `fixtures.ts` | `get_dtoken_dashboard_payload`, runtime `/dashboard/payload`, Event Ledger-derived summaries | Polls through shared hook | Mostly covered | 2026-07-06 pass: activity remains contribution calendar; Token is a trend/chart view; Tools is a ranking view; desktop unavailable stays empty/waiting | Browser check for layout; runtime check that source is `dtoken-owned-runtime` + `dtoken-local-service` |
| Install / Connect | Connect d-token runtime, choose agent, configure/apply route, verify config, go to Doctor/Dashboard/Logs | Setup Wizard, Agent Selection, Provider Setup, Route Builder, diff/backup/apply/verify, capability state | `Install.tsx`, `CapabilitySummary.tsx` | `run_install_transaction`, `detect_tools`, `save_route_profile_draft`, `apply_route_profile_draft`, `verify_agent_route_config`, `run_product_readiness_check`, `get_dtoken_dashboard_payload` | Mostly command-driven; dashboard status polls | Mostly covered | 2026-07-07: Install verify flow now invokes product readiness aggregation behind product language, covering install/connect, discovery, route config, project policy write, capability metadata, export redaction, and Doctor evidence. Remaining: final full OpenCode gate before public release | Tauri command result; Rust command tests; route verification mock; final OpenCode real validation |
| Advanced / Providers & Routes | Manage provider/route profiles and see active config | ProviderProfile, RouteProfile, cc-switch route preference, no silent fallback | `Advanced.tsx` | `save_route_profile_draft`, `apply_route_profile_draft`, `get_active_route_config_status`, `get_dtoken_dashboard_payload` | Active config polls about every 3s | Mostly covered | 2026-07-06 G2/G4: release gate now covers configured OpenAI chat, OpenAI Responses, Anthropic bridge, chain/cc-switch status, protocol profiles, and no-silent-fallback runtime evidence. Remaining: final Tauri visual pass for route-builder clarity | Tauri command round-trip; active config mock script; no-touch release gate; UI check |
| Advanced / Project Policy | Scan project signals, recommend compression level, apply/read saved policy | ProjectCompressionPolicy, recommendation engine, CCR/memory/learn/output shape/integrated RTK toggles | `Advanced.tsx`, `CapabilitySummary.tsx` | `recommend_project_policy`, `get_app_settings("project-policy")`, `save_app_settings`, `get_dtoken_dashboard_payload` | Polls saved policy about every 3s | Mostly covered | 2026-07-06 R6: Advanced Headroom tab shows runtime capability summary for compression/CCR/memory/output shaping/integrated RTK/dashboard primitives before policy scan/apply/readback. Browser check confirmed panel visible and no forbidden test wording | Rust unit tests; UI readback check |
| Advanced / Service & cc-switch preferences | Store local preferences without mutating external services | Runtime mode preference, cc-switch adapter preference | `Advanced.tsx` | `get_app_settings("advanced")`, `save_app_settings` | Local state + save/readback | Partial | Must keep wording as preference until real OS service / live cc-switch integration exists. If real lifecycle is approved, add identity-checked native lifecycle commands | Settings persistence tests; UI text audit |
| Analytics | Inspect savings, latency, provider errors, throughput, route overhead | Layered token model, metrics/history, provider/cc-switch metrics | `Analytics.tsx` | `useRuntimeDashboard()` → `/dashboard/payload`, `/stats-history`, `/metrics` through runtime payload | Polls through shared hook | Mostly covered | 2026-07-06 R4: Analytics now reads runtime `latency_breakdown` and `route_overhead`, adds a route-overhead metric card, and shows compression/route-adapter/cc-switch/provider latency breakdown. Remaining: browser/Tauri visual check after next app reload | Runtime matrix verification; browser/Tauri UI check |
| Doctor | Diagnose service, agent config, route, redaction, ledger/export health, capability health | Doctor, compatibility checks, port/process/config/provider/ledger diagnostics, Headroom-derived capability state | `Doctor.tsx`, `CapabilitySummary.tsx` | `run_doctor`, `run_product_readiness_check`, `get_dtoken_dashboard_payload`, `get_active_route_config_status`, `verify_agent_route_config` | Auto-refresh evidence; manual Run All | Mostly covered | 2026-07-07: Run All now has product readiness command coverage, so it is a real product verification aggregate rather than UI-only observation. Identity-checked app-managed service lifecycle commands now exist and are verified; OS service registration remains separate hardening if required | Tauri command results; Rust command tests; browser preview check; no fake repair buttons; runtime surface gate |
| Logs & Security | View redacted logs, Event Ledger, security status, export JSON/CSV | Event Ledger, route decisions, request logs, redacted export, credential boundary | `LogsSecurity.tsx` | `get_dtoken_dashboard_payload`, `verify_agent_route_config`, `export_dtoken_runtime("json"|"csv")` | Polls through runtime hook; export command on demand | Mostly covered | 2026-07-06 G2/G4: no-touch release gate now verifies provider_forward, config_patch, Responses, Anthropic bridge, CSV/JSON redaction, and Event Ledger persistence without raw content | Export redaction scripts; runtime event tests; UI export check |
| Changes / Rollback | Review pending diffs, approve route drafts, restore d-token active config | Config Diff, backup/rollback, rollback point | `Changes.tsx` | `get_desktop_data`, `apply_route_profile_draft`, `manage_backup` | Polls desktop data about every 3s | Partial | External agent config rollback remains gated/staged. Need explicit UX copy so users understand what is restored now vs later | Rust rollback tests; UI check |
| Settings | Appearance, glass/blur, flowing background palette/speed, runtime/redaction/telemetry/language | Settings, i18n, privacy defaults, desktop polish | `Settings.tsx`, `theme-context.tsx`, `theme.css` | `get_app_settings("app")`, `save_app_settings` | Hydrates at provider startup and page load | Partial | Visual polish pass remains: translucent blur readable in light/dark; top/sidebar square shell bars; saved theme/palette/speed verified in Tauri | Browser style check; startup hydration check |
| Compatibility Matrix | Show OS/shell/agent/provider/route support | PRD section 16 CompatibilityMatrix | `Compatibility.tsx`, `nav.ts`, `AppShell.tsx` | Tauri `get_desktop_data().runtimeCompatibility` backed by `/compatibility/agents`, `/protocol-injection`, `/chain/status`; browser preview rows are clearly labeled | Polls every 5s | Mostly covered | 2026-07-06 G2/G4: Compatibility page now prefers canonical d-token runtime compatibility/protocol/chain data when local service is available. Remaining: final Tauri visual pass | Runtime surface gate; UI page check |



## 2.1 Latest all-page browser verification ? 2026-07-07

Checked pages: Overview, Install, Advanced, Analytics, Doctor, Changes/Rollback, Logs/Security, Compatibility, Settings.

Results:

- No visible forbidden testing/lifecycle terms were found.
- Every checked page had zero blank/unnamed buttons.
- Sidebar and topbar outer border radius measured as `0px`.
- Text overflow probe found only tiny empty icon/switch spans, not user-facing text overflow.
- The check confirms UI presentation only; runtime proof is tracked in `docs/RUNTIME_CAPABILITY_MATRIX.md` and `docs/RELEASE_READINESS_REPORT.md`.

## 3. Formal UI forbidden-term audit

Current source scan result:

- Active formal React pages do not show first-class workload/benchmark/smoke/test-center UI according to recent checks.
- Remaining `workload`, `benchmark`, `smoke`, `fixture`, `fake-bin` hits are acceptable only in:
  - `docs/PRD.md`, `AGENTS.md`, `CLAUDE.md` policy text.
  - `support/scripts` validation harnesses.
  - hidden/internal Tauri validation commands not wired into normal UI.
  - browser-preview fixture imports and labels that must be explicit as preview/sample.

Before release, run a final visible-text audit:

```powershell
rg -n "workload|benchmark|smoke|fixture test|mock test|fake-bin|testing center|测试中心|能力自检|真实链路测试|OpenCode Real Route|test_scope|routed test" code/apps/desktop/src/app
```

Allowed hits must be non-visible comments/types or explicit browser-preview sample boundaries. Any visible formal UI hit fails the release gate.

## 4. Browser preview vs desktop runtime rule

| State | Allowed UI data | Required label |
|---|---|---|
| Browser preview | Sample/preview data for layout only | `浏览器预览` / `Preview data` / `示例数据` |
| Desktop runtime unavailable | Empty/waiting/error state, no fake totals | `d-token local service unavailable` / waiting copy |
| Desktop runtime available | Real d-token-owned runtime data only | source must be `dtoken-owned-runtime` and `desktop_data_source=dtoken-local-service` |

Browser screenshots are UI evidence only. They do not prove real capability integration.

## 5. Implementation batches after review

The release plan now uses R-batches. The UI batches below map into that execution board:

| UI batch | Release batch | Implementation meaning |
|---|---|---|
| UI-A Navigation and shell polish | R1 | Desktop shell, sidebar/topbar/status bar visual stability |
| UI-B Overview data presentation | R2/R3 | Dashboard-facing presentation and real/preview/unavailable data states |
| UI-C Install/connect state machine | R5 | Product-safe connect/configure/apply/verify flow |
| UI-D Compatibility and capability surfacing | R2/R6 | All PRD capabilities have visible surfaces or documented deferrals |
| UI-E Final polish | R1/R7/R9 | Accessibility, compact layouts, glass blur, flowing background, export UX, release visual pass |

### Batch UI-A — Navigation and shell polish

- Fix sidebar expanded/collapsed selected indicator once for all nav items and Settings.
- Keep topbar/sidebar shell bars square/no rounded outer shell if that remains the desired direction.
- Keep hover animation subtle and consistent.
- Verify no layout jump in collapsed mode.

### Batch UI-B — Overview data presentation

- Replace confusing token calendar behavior with approved product model:
  - Active trend.
  - Token trend.
  - Tools trend.
- If calendar remains, it is only active/session contribution style; token/tool views should use chart/list styles that match the data.

### Batch UI-C — Install/connect state machine

- One coherent flow:
  1. Observe/connect runtime.
  2. Select agent.
  3. Configure provider/route.
  4. Preview diff and backup.
  5. Apply.
  6. Verify config.
  7. Open Doctor/Dashboard/Logs.
- No fake start/stop.
- Service lifecycle only if native command truly exists and verifies d-token process identity.

### Batch UI-D — Compatibility and capability surfacing

- Either create first-class Compatibility Matrix page or fold it visibly into Doctor/Advanced with clear matrix rows.
- Surface Headroom-derived runtime capabilities as product state, not test results.

### Batch UI-E — Final polish

- Light/dark translucent blur readability.
- Flowing background color/speed persistence.
- Text overflow and compact widths.
- JSON/CSV export and Event Ledger UX.

## 6. Acceptance checklist

UI release candidate passes only if:

- Every PRD page has a surface or an explicit release deferral.
- No normal UI exposes development testing concepts.
- OpenCode appears as one supported agent, not the product center.
- Desktop unavailable state does not show fake runtime facts.
- Browser preview is clearly labeled as preview.
- Persistent-looking controls have local/native readback.
- UI verification and runtime verification are reported separately.

## 7. Release-batch UI ownership

This table turns the matrix into an implementation queue. If a page changes, update the matching row before or during the batch, not after the fact.

| Release batch | UI surfaces | Required UI proof |
|---|---|---|
| R1 Frontend shell and visual quality | Shell, Sidebar, Topbar, StatusBar, Settings appearance | Expanded/collapsed sidebar screenshots, square shell measurement, light/dark glass blur screenshots, saved theme/palette/speed hydration |
| R2 UI capability coverage | All pages and nav entries | Forbidden-term audit, PRD page coverage checklist, OpenCode shown only as one agent |
| R3 Real-time data contract | Overview, Analytics, Doctor, Logs, Advanced, StatusBar | Desktop unavailable has empty/waiting states; browser preview is labeled; runtime source badge appears only for d-token-owned payload |
| R5 Install/connect/configure/verify | Install, Doctor, Changes, Advanced routes | Select agent, save/apply route, verify config, view Doctor/Dashboard/Logs without test/benchmark wording |
| R6 Headroom-derived capability surfacing | Overview, Analytics, Advanced project policy, Doctor, Logs/Security | Compression/CCR/memory/learn/output-shape/integrated RTK summaries visible as product state |
| R7 Logs/security/export/rollback | Logs/Security, Changes/Rollback | Event Ledger tab, JSON/CSV export buttons, redaction copy, rollback result visible |
| R9 Performance/release readiness | App shell and all route chunks | Build size summary, no large unexpected main bundle regression, final UI checklist |

## 8. Frontend real-data rule by page

| Page | Real desktop data required before claiming integrated | Browser preview allowed? | Desktop unavailable behavior |
|---|---|---:|---|
| Overview | Runtime dashboard payload with `source=dtoken-owned-runtime` and `desktop_data_source=dtoken-local-service` | Yes, labeled | Waiting/empty cards, no fake totals |
| Install | Tauri command results for detect/connect/apply/verify and shared runtime status | Yes, labeled | Runtime unavailable, actions remain connect/configure/verify |
| Advanced | Route draft/apply/readback, active config, saved settings, saved project policy | Yes, labeled | Saved-state unavailable/error indicators |
| Analytics | Runtime stats/history/metrics with latency/overhead fields | Yes, labeled | Empty waiting charts, no sample metrics |
| Doctor | `run_doctor`, active config, route verification, dashboard payload | Yes, labeled | Diagnostics explain unavailable runtime |
| Logs/Security | Runtime events/logs/routes/export command | Yes, labeled | Empty ledger/log state, export only browser-visible redacted preview |
| Changes/Rollback | `get_desktop_data`, route drafts, rollback points, manage backup | Yes, labeled | Empty pending/rollback state |
| Settings | `get_app_settings`, `save_app_settings`, startup hydration | Yes, local UI | Persist error or browser-preview label |
| Compatibility | `get_desktop_data` now, canonical `/compatibility/agents` target | Yes, labeled | Empty/waiting compatibility state |
