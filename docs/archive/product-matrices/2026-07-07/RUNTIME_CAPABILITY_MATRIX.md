# d-token Runtime Capability Matrix

Status: updated with 2026-07-07 full OpenCode release gate, service lifecycle, packaging audit, installer sidecar artifact audit, distribution manifest audit, Windows signing readiness audit, cross-platform readiness audit, handoff manifest, artifact redaction audit, cleanup audit, and installed startup audit
Date: 2026-07-06
Truth source: `docs/PRD.md`
Purpose: define the full runtime/API/native verification surface before the next implementation batch.

## 1. Required runtime chain

```text
Agent
鈫?
d-token local service
鈫?
Headroom-derived d-token-owned runtime
鈫?
d-token route adapter
鈫?
official upstream / custom upstream / cc-switch route interface / local provider
鈫?
Event Ledger / Dashboard / Doctor / Logs / App
```

Runtime proof must come from d-token-owned service/API/Tauri command evidence, Event Ledger, metrics/export, and routed agent flow. Browser UI inspection is not runtime proof.

## 2. Runtime endpoint matrix

| PRD capability | Runtime endpoint / function | Tauri/native surface | Script/test evidence | Status | Gaps / next implementation | Verification method |
|---|---|---|---|---|---|---|
| Health / service identity | `/health` | `get_dtoken_dashboard_payload`, `verify_agent_route_config`, Doctor probes | `run_fixture_tests.ps1`, release verification | Mostly covered | App-managed identity-checked service lifecycle is implemented and verified; OS-level service registration remains release hardening only if required | Runtime health response + process/port cleanup report |
| Dashboard payload | `/dashboard/payload`, `dashboard_payload_for_state` | `get_dtoken_dashboard_payload`, `useRuntimeDashboard` | fixture, mock routed, active config, agent route verification | Mostly covered | Ensure every UI metric needed by PRD exists in payload, especially route overhead and latency breakdown | Payload source must be `dtoken-owned-runtime`; desktop source `dtoken-local-service` |
| OpenAI chat route | `/v1/chat/completions` | Hidden validation scripts; route config surfaces in UI | routed mock, OpenCode workload | Covered for current validation | Need final multi-agent/provider matrix beyond OpenCode first path | Mock upstream + Event Ledger route hit |
| OpenAI responses route | `/v1/responses` | Hidden validation scripts / route config | `run_dtoken_responses_mock_test.ps1`, release verification | Mostly covered | 2026-07-06 G4: Responses-compatible requests now honor configured OpenAI routes, forward to upstream, record provider_forward, and verify redacted export | Route request + provider_forward ledger + export redaction |
| Anthropic bridge | `/v1/messages`, `anthropic_messages` | Hidden validation scripts | `run_dtoken_anthropic_bridge_mock_test.ps1`, release verification | Mostly covered | 2026-07-06 G4: no-touch release gate now includes Anthropic-shaped route bridge through configured OpenAI-compatible/cc-switch-style upstream | Mock Anthropic-shaped request through configured upstream |
| Stats | `/stats` | Dashboard/Analytics payload | fixture/release tests | Partial | Confirm source is d-token-owned and no external Headroom dependency | API response + dashboard consistency |
| History | `/stats-history` | Dashboard/Analytics payload | fixture tests updated for d-token-owned history | Partial | Confirm enough history points for UI charts without fixture fallback | API response contains history field |
| Metrics | `/metrics` | Analytics / release scripts | fixture/release tests | Mostly covered | 2026-07-06 R4: stats, metrics payload, and metrics text expose compression latency, route adapter latency, provider latency, route overhead tokens, and route overhead percent. Current per-stage latency is deterministic/derived until per-stage timers are persisted | Metrics scrape + parsed report |
| JSON export | `/export/json` | `export_dtoken_runtime("json")`, Logs/Security | active config, route verification, release scripts | Mostly covered | Final redaction audit should scan report/export artifacts | Export has no raw prompt/response/secrets |
| CSV export | `/export/csv` | `export_dtoken_runtime("csv")`, Logs/Security | active config script + release verification | Mostly covered | 2026-07-06 G4: active config gate asserts CSV export includes config status and no credential-shaped content; artifact redaction scan passed | CSV downloaded/read; no secrets/raw text |
| Provider profiles | `/providers` | Advanced provider/route UI, dashboard payload | fixture/release tests | Partial | Need complete official/custom/cc-switch/local provider modes | Provider rows + active route evidence |
| Route profiles | `/routes`, `/config/active` | route draft/apply/readback commands | active config runtime, agent route verification | Mostly covered | Need no-silent-fallback assertions in every route mode | Active config status + route decision |
| Instance profiles | `/instances` | Dashboard/Analytics future surface | fixture/release tests | Partial | UI surface for instances is not clearly first-class yet | API response + UI mapping |
| Chain status | `/chain/status` | Compatibility/Doctor/route surfaces | `run_dtoken_runtime_surface_mock_test.ps1`, release verification | Mostly covered | 2026-07-06 G4: chain bridge route status is exposed from d-token config with external cc-switch no-mutation boundary | API response + UI status |
| Route decisions | `/route-decisions/recent` | Logs/Security Event Ledger tab, Dashboard | routed mock, OpenCode workload | Mostly covered | Ensure error/fallback/cc-switch decisions are logged, not only success | JSONL/Event Ledger row verification |
| Request logs | `/logs/recent` | Logs/Security | release scripts | Mostly covered | Ensure no raw prompt/response; include error categories | Redaction scan |
| Doctor / product readiness runtime | `/doctor/run`, dashboard/export/config probes | `run_doctor`, `run_product_readiness_check`, Doctor page, Install verify flow | fixture/release tests, Rust command tests | Mostly covered | Product readiness now aggregates install/connect, discovery, route config, policy write, capability metadata, export redaction, and Doctor evidence. Persistent OS service lifecycle remains deferred until identity-checked commands exist | Tauri command tests + release gate + UI diagnostic view |
| Preprocessors | `/preprocessors` | Dashboard/Analytics potential surface | fixture tests | Partial | Decide whether visible UI needs preprocessor details or just aggregate RTK/runtime split | API response + Analytics mapping |
| Integrated RTK status | `/rtk/status`, `/rtk/gain`, `/rtk/discover`, `/rtk/session` | Dashboard/Analytics/Doctor planned surfaces | fixture tests | Partial | RTK is internal capability; UI should show aggregate status/savings, not standalone product path | Runtime metrics + no standalone RTK UI path |
| Headroom-derived status | `/headroom/status`, `/headroom/proxy`, `/headroom/mcp`, `/headroom/stats`, `/headroom/cache-summary` | Dashboard/Doctor/Advanced capability state | fixture tests | Partial | Wording must be d-token-owned Headroom-derived runtime, not external Headroom service dependency | API shows external dependency false |
| Projects | `/projects`, `/projects/default-policy`, `/projects/recommend-compression` | Advanced Project Policy | project policy UI and Rust tests | Partial | Recommendation engine factors need full PRD coverage or release limitation | Apply/readback settings + runtime input report |
| Config plan | `/config/patches/plan` | Install diff/backup flow, Changes page | fixture/release tests | Partial | Need product-safe preview diff for all supported agents, not OpenCode-only | Diff output + backup point |
| Rollback points | `/config/rollback-points` | Changes/Rollback | rollback Rust tests | Partial | External agent config restore remains staged/gated; must be transparent | Restore d-token active config test |
| Event Ledger | `/events`, JSONL persistence | Logs/Security Event Ledger tab | routed mock, Responses mock, runtime surface, OpenCode workload | Mostly covered | 2026-07-06 G4: release gate now asserts provider_forward and config_patch event rows in addition to route/compression rows. Remaining: richer install_step coverage for future OS-level service registration if required | Persisted JSONL rows + redaction scan |
| CCR | `/ccr/summary`, `/ccr/retrieve/{recovery_ref}` | Dashboard/Analytics/Logs/Install/Doctor/Advanced capability summaries | routed mock / release fixture artifacts | Covered for current release gate | 2026-07-06 R6: CapabilitySummary surfaces CCR metadata entries and raw-availability boundary in Install, Doctor runtime tab, and Advanced Headroom tab. No-touch release gate now asserts CCR source, entries, and `raw_available=false` | CCR metadata only; `raw_available=false` |
| Memory / learn | `/memory/summary`, `/memory/learn` | Advanced/Analytics/Install/Doctor capability summaries | Rust tests + release fixture artifacts | Covered for current release gate | 2026-07-06 R6: CapabilitySummary surfaces memory/learn metadata entries and raw boundary; no-touch release gate now posts memory learn metadata, asserts summary entry, output verbosity readback, and no raw prompt/response leak | Learn metadata write/read; raw text stripped |
| Output shaping | `/output-shape` | Advanced Project Policy + capability summaries | fixture/runtime endpoint + release fixture artifacts | Covered for current release gate | 2026-07-06 R6: CapabilitySummary exposes output shaping as a product capability driven by project policy and learn metadata. No-touch release gate now asserts `/output-shape` source, `raw_available=false`, and learned verbosity | Endpoint + saved setting readback |
| Compatibility | `/compatibility/agents`, desktop `get_desktop_data().runtimeCompatibility` | First-class Compatibility page | `run_dtoken_runtime_surface_mock_test.ps1`, release verification | Mostly covered | 2026-07-06 G2/G4: desktop Compatibility page now prefers canonical runtime compatibility/protocol/chain payload when available | API response + visible matrix |
| Protocol injection | `/protocol-injection` | Compatibility page and route/Doctor surfaces | unit tests + `run_dtoken_runtime_surface_mock_test.ps1` | Mostly covered | 2026-07-06 G4: OpenAI and Anthropic protocol profiles are checked in the no-touch release gate and shown in Compatibility | API response + route decision protocol fields |
| Install scan/plan | `/install/scan`, `/install/plan` | Install page through Tauri scan/plan/transaction | install transaction tests | Partial | Runtime endpoint and Tauri transaction should be reconciled into one product flow | Scan/plan/diff/backup/apply/verify logs |



## 2.1 Latest no-touch runtime verification ? 2026-07-07

Command: `support/scripts/run_release_verification.ps1 -SkipOpenCode`
Report: `support/artifacts/release-verification-20260707-184716.json`
Result: passed.

Covered runtime gates:

- desktop build
- desktop Rust tests
- fixture runtime
- mock routed runtime
- active config runtime
- agent route config verification
- product readiness command with d-token active config and runtime metadata
- OpenAI Responses-compatible runtime
- compatibility / protocol injection / chain status runtime surface
- Anthropic-shaped bridge runtime
- product readiness command focused Rust coverage: 9 desktop tests, including a runtime-metadata-backed 9/9 readiness integration check
- Headroom-derived capability coverage audit: 11/11 capability groups covered across runtime, frontend, and verification evidence
- Desktop packaging audit: 13/13 package/bundle/configuration checks passed; installer artifact audit: 12/12 artifact path/size/hash/WebView2 checks passed; distribution manifest audit passed with `release_candidate_ready=true`; Windows signing readiness is now checked and reports `can_sign_now=false` / `distribution_ready_public=false` because `signtool.exe` and signing credentials are unavailable and app/runtime/installer are unsigned
- Release artifact redaction audit scanned 8 latest JSON artifacts with 0 raw prompt/response/output field hits, 0 credential-shaped value hits, and 0 parse errors
- Release cleanup audit passed with no listeners on release/test ports and no residual validation processes
- Cross-platform release readiness audit passed as a boundary check and records Windows local RC ready, macOS local RC not ready on this Windows host, and public distribution false
- Release handoff manifest passed and records `local_release_candidate_ready=true`, `cross_platform_local_release_candidate_ready=false`, `public_distribution_ready=false`, artifact hashes, evidence paths, and public-distribution blockers

Full OpenCode real validation was rerun after adding installer artifact verification. Latest full report: `support/artifacts/release-verification-20260707-172419.json`; OpenCode sub-report: `support/artifacts/opencode-workload-benchmark-20260707-172418.json`.

## 3. Tauri command matrix

| Command | Product meaning | Current UI use | Status | Must not do |
|---|---|---|---|---|
| `get_dtoken_dashboard_payload` | Read runtime Dashboard payload | Shell, Overview, Analytics, Advanced, Doctor, Logs | Core path | Must not treat local empty fallback as real runtime |
| `export_dtoken_runtime` | Export redacted runtime data | Logs/Security | Core path | Must not export raw prompt/response/secrets |
| `get_app_settings` / `save_app_settings` | Local App settings read/write | Settings, Advanced, project policy | Core path | Must not overwrite whole section on partial save |
| `save_route_profile_draft` | Save route/provider draft | Install, Advanced | Core path | Must not write third-party config directly |
| `apply_route_profile_draft` | Apply d-token-owned active route config | Install, Advanced, Changes | Core path | Must not silently enable fallback |
| `get_active_route_config_status` | Read active d-token config | Advanced, Doctor | Core path | Must not expose secrets |
| `verify_agent_route_config` | Verify product config path | Install, Doctor, Logs security | Core path | Must remain product-language config verification, not benchmark UI |
| `get_desktop_data` | Read local desktop state, drafts, rollback, compatibility | Changes, potential Compatibility Matrix | Core path | Must distinguish real local state from preview/sample |
| `manage_backup` | Backup/rollback actions | Changes/Rollback | Partial | Must not restore external agent configs without explicit safe implementation |
| `detect_tools` | Detect agent/tool presence | Install, Doctor | Partial | Must not mutate PATH/hooks/config |
| `preview_install_plan` | Preview install/config diff | Install planned/available | Partial | Must produce user-understandable diff and backup plan |
| `run_install_transaction` | Execute install/connect/apply/verify transaction | Install | Partial | Must not imply persistent service start unless implemented |
| `run_doctor` | Run diagnostics | Doctor | Core path | Must not be framed as smoke test |
| `run_product_readiness_check` | Product-level install/connect/config/diagnose/export readiness aggregation | Install verify flow, Doctor full-check flow | Core path | Must not depend on external Headroom/RTK/cc-switch service state or expose workload/benchmark/smoke UI wording |
| `recommend_project_policy` | Recommend compression policy | Advanced | Partial | Must not store raw project content |
| Hidden/internal validation commands | Development verification | Not normal UI | Allowed only hidden | Must not appear in final formal UI |

Hidden/internal validation commands currently include `run_opencode_workload_benchmark` and `run_headroom_capability_smoke`. They can remain as command-level harnesses or dev-only tools, but normal UI must not expose them as user actions.

## 4. Support script verification matrix

| Script | Purpose | Uses external Headroom/RTK/cc-switch? | Uses real OpenCode config? | Release role |
|---|---|---:|---:|---|
| `support/scripts/run_release_verification.ps1` | Unified release gate | No external Headroom/RTK/cc-switch mutation | Optional, unless `-SkipOpenCode` | Final report |
| `support/scripts/run_fixture_tests.ps1` | No-touch fixture runtime and structure checks | No | No | Safe CI/dev gate |
| `support/scripts/run_dtoken_routed_mock_test.ps1` | d-token routed mock upstream with compression/persistence | No | No | Runtime route proof |
| `support/scripts/run_dtoken_active_config_mock_test.ps1` | Active config load/route/export redaction | No | No | Config proof |
| `support/scripts/run_dtoken_agent_route_verification_mock.ps1` | Product route config verification | No | No | Mirrors Install Verify config evidence |
| `support/scripts/run_dtoken_product_readiness_command_test.ps1` | App product readiness command verification with d-token active config and runtime metadata | No | No | Mirrors Install/Doctor full verification evidence |
| `support/scripts/run_headroom_capability_coverage_audit.ps1` | Static/report audit that every Headroom-derived capability group has runtime, UI, and verification evidence | No | No | Release coverage guard for "Headroom capabilities cannot be dropped" |
| `support/scripts/run_desktop_packaging_audit.ps1` | Desktop package/bundle readiness audit | No | No | Release packaging guard; does not sign/notarize |
| `support/scripts/run_desktop_bundle_artifact_audit.ps1` | Verifies generated app executable and NSIS installer path, size, and SHA-256 hash | No | No | Release artifact guard after `npm run tauri:build`; does not sign/notarize |
| `support/scripts/run_release_distribution_audit.ps1` | Records release-candidate hashes, installed dependency presence, and Authenticode signing status | No | No | Local RC/public distribution boundary guard; read-only and does not sign |
| `support/scripts/run_windows_signing_readiness_audit.ps1` | Checks artifacts, `signtool.exe`, optional certificate path, unsigned artifacts, and `can_sign_now` | No | No | Public distribution readiness guard; read-only and does not sign/read credentials |
| `support/scripts/sign_windows_release_artifacts.ps1` | Plans or executes Windows signing for app/runtime/installer artifacts | No | No | Dry-run is safe evidence; real signing requires explicit `-Execute` and credentials |
| `support/scripts/run_release_artifact_redaction_audit.ps1` | Scans latest release JSON artifacts for raw prompt/response/output fields and credential-shaped values | No | No | Machine-enforced artifact privacy guard; records only paths/pattern names |
| `support/scripts/run_release_cleanup_audit.ps1` | Checks for residual validation processes and listening release/test ports after verification | No | No | Machine-enforced cleanup guard; read-only and does not kill processes |
| `support/scripts/run_cross_platform_release_readiness_audit.ps1` | Records Windows/macOS release boundary, platform artifacts, and macOS runner/signing requirements | No | No | Cross-platform release truth guard; read-only and does not build/sign |
| `support/scripts/run_release_handoff_manifest.ps1` | Generates compact release handoff manifest with artifacts, hashes, evidence paths, and public blockers | No | No | Final local-RC delivery summary; read-only and no secrets |
| `support/scripts/run_installed_desktop_startup_audit.ps1` | Checks installed desktop dependencies and launches installed app to verify startup window | No | No | Focused installer/startup guard; launches and closes the installed app, so not part of every no-touch run |
| `support/scripts/run_dtoken_anthropic_bridge_mock_test.ps1` | Anthropic-shaped route bridge | No | No | Needed if Anthropic agents are release scope |
| `support/scripts/run_opencode_workload_benchmark.ps1` | Real OpenCode routed validation | No external Headroom/RTK/cc-switch mutation | Yes, user-authorized | Final real OpenCode evidence only; not UI |
| `support/scripts/run_opencode_real_routed_test.ps1` | Real routed OpenCode path | No external Headroom/RTK/cc-switch mutation | Yes | Optional focused validation |
| `support/scripts/run_opencode_real_streaming_route_test.ps1` | Real streaming OpenCode path | No external Headroom/RTK/cc-switch mutation | Yes | Optional focused validation |
| `support/scripts/run_compression_benchmark.ps1` | Compression measurement | No | No | Development evidence/report only |

## 5. Evidence requirements by capability

| Capability | UI evidence | Runtime evidence | Required before release |
|---|---|---|---|
| Agent route works | Install/Doctor shows config verified | Route decision + Event Ledger + dashboard request count | Yes |
| Compression works | Overview/Analytics shows savings | request before/after tokens + compression event | Yes |
| Provider/route visible | Advanced/Logs route target visible | active config + route decision provider/route | Yes |
| No silent fallback | UI shows fallback disabled/visible | route decision and config assert fallback policy | Yes |
| Export redaction | Logs/Security export buttons | JSON/CSV artifacts contain no secrets/raw prompt/response | Yes |
| Event Ledger | Logs/Security Event Ledger tab | `events.jsonl`, route decisions, request logs | Yes |
| Project policy | Advanced policy saved/readback | local settings + recommendation input | Yes |
| Backup/rollback | Changes/Rollback page | rollback point restores d-token active config | Yes |
| Doctor | Doctor page | `run_doctor` + route verification evidence | Yes |
| Compatibility | Compatibility page or Doctor/Advanced matrix | `/compatibility/agents` or desktop data | Decision needed |
| Persistent service lifecycle | Install runtime state | identity-checked start/stop/connect commands | Decision needed |
| OpenCode real validation | UI treats OpenCode as one selectable agent | real OpenCode report with redacted artifacts | Covered for current milestone: latest full release gate passed on 2026-07-07 with `support/artifacts/release-verification-20260707-135631.json` and `support/artifacts/opencode-workload-benchmark-20260707-135631.json` |
| Other agents real validation | Multi-agent UI model present | command/runtime proof per agent | Not required for first OpenCode milestone unless user expands scope |

## 6. Known runtime gaps to resolve after review

1. **Persistent service lifecycle**
   - Current App observes/connects/verifies d-token runtime and runs install/config transactions.
   - It should not claim real start/stop until there are identity-checked native lifecycle commands.

2. **Compatibility Matrix**
   - Runtime endpoint exists.
   - UI needs either a first-class page or a clear Doctor/Advanced matrix section.

3. **Latency and route overhead metrics**
   - PRD requires compression latency, route adapter latency, provider final latency, cc-switch double-hop latency, provider error classes.
   - Need verify whether runtime payload exposes all fields; if not, add them.

4. **CCR / memory / output shaping surfaces**
   - Runtime endpoints exist.
   - UI should show product-safe summaries without raw prompt/response/CCR content.

5. **Anthropic and Responses API explicit release coverage**
   - Endpoints exist.
   - Need decide whether first release gate must include explicit Anthropic bridge and Responses API scripts.

6. **Install scan/plan unification**
   - Runtime has `/install/scan` and `/install/plan`.
   - Tauri has its own install transaction commands.
   - Need reconcile into one coherent product flow and one verification story.

## 7. Proposed implementation batches after review

The release plan now uses R-batches. Runtime work maps to those batches as follows:

| Runtime batch | Release batch | Implementation meaning |
|---|---|---|
| Runtime-A Metrics and payload completeness | R4 | Complete stats/history/metrics/dashboard fields used by Overview and Analytics |
| Runtime-B Compatibility and protocol coverage | R4/R6 | Canonical compatibility and OpenAI/Anthropic protocol proof |
| Runtime-C Capability summaries | R6 | Product-safe CCR, memory/learn, output-shape, integrated RTK summaries |
| Runtime-D Install/connect lifecycle decision | R5 | Keep connect/verify honest now; add identity-checked lifecycle only when implemented |
| Runtime-E Final OpenCode proof | R8 | Real OpenCode validation with redacted artifacts |

### Runtime-A 鈥?Metrics and payload completeness

- Audit `/dashboard/payload`, `/stats`, `/stats-history`, `/metrics`.
- Add missing latency/overhead/error fields or mark deferred.
- Add release assertions for the fields used by UI.

### Runtime-B 鈥?Compatibility and protocol coverage

- Wire `/compatibility/agents` into UI.
- Confirm `/protocol-injection` explains OpenAI/Anthropic adaptation and fallback policy.
- Include Anthropic bridge and Responses API in release gate if in current release scope.

### Runtime-C 鈥?Capability summaries

- Add product-safe UI/runtime summaries for CCR, memory/learn, output shaping, integrated RTK.
- Ensure no raw content is stored or displayed.

### Runtime-D 鈥?Install/connect lifecycle decision

- If approved: implement real d-token-owned service start/connect/stop with process identity checks.
- If not approved: keep App as install/connect/verify and make copy unambiguous.

### Runtime-E 鈥?Final OpenCode proof

- Run final user-authorized OpenCode validation only after UI/runtime batch work.
- Store redacted artifacts only.
- Confirm no external Headroom/RTK/cc-switch mutation.

## 8. Runtime release-batch ownership

| Release batch | Runtime/API scope | Required proof |
|---|---|---|
| R3 Real-time data contract | `get_dtoken_dashboard_payload`, `/dashboard/payload`, payload source fields | UI receives real runtime only when source is `dtoken-owned-runtime` + `dtoken-local-service` |
| R4 Runtime payload completeness | `/stats`, `/stats-history`, `/metrics`, `/dashboard/payload` | Latency breakdown, route overhead, provider/route performance, errors, throughput in JSON and metrics text |
| R5 Install/connect/configure/verify | Tauri install transaction, route draft/apply, active config, `verify_agent_route_config`, `/config/active` | Config verify result proves health/config/dashboard/export/fallback checks |
| R6 Headroom-derived capabilities | `/ccr/summary`, `/memory/summary`, `/memory/learn`, `/output-shape`, `/rtk/*`, `/headroom/*`, `/preprocessors` | Metadata-only summaries, no raw prompt/response/CCR content |
| R7 Logs/security/export/rollback | `/events`, `/route-decisions/recent`, `/logs/recent`, `/export/json`, `/export/csv`, rollback commands | Redacted export artifacts and rollback unit/script evidence |
| R8 OpenCode real validation | d-token temporary runtime + user-authorized OpenCode command/config path | Redacted OpenCode report with route hit, compression, dashboard update, ledger rows |
| R9 Release readiness | Unified release verification script and build/test gates | Final self-contained report and process/port cleanup proof |

## 9. Required assertions to add or preserve

| Capability | Assertion |
|---|---|
| Runtime source | Dashboard payload source is `dtoken-owned-runtime`; desktop source is `dtoken-local-service`; otherwise UI treats it as unavailable/preview |
| Latency breakdown | Stats and metrics payload expose compression, route adapter, provider, and cc-switch/disabled latency fields |
| Route overhead | Stats and metrics payload expose route overhead tokens and percent; caveat if derived instead of measured |
| No silent fallback | Active config and route decisions show fallback disabled unless explicitly configured |
| Event Ledger | At least route decision, compression, config patch/install step, provider forward/error classes are recorded by release tests |
| Redaction | Exports and support artifacts contain no raw prompt, raw response, API key, cookie, Authorization, token, or raw OpenCode output |
| CCR | Retrieve endpoint defaults to metadata only with `raw_available=false` |
| Memory/learn | Learn endpoint strips raw `messages`, `body`, `content`, raw prompt, and raw response fields |
| OpenCode | Report proves real OpenCode traffic entered d-token; not merely that OpenCode can call an upstream |
| Cleanup | Temporary d-token/mock/OpenCode validation processes are stopped and relevant test ports are not left listening |

## 10. Verification command order

Use this order after implementation batches:

```powershell
cd D:\D-TOKEN\d-token\code\apps\desktop
npm run build

cd D:\D-TOKEN\d-token\code
cargo test -p d-token-desktop

cd D:\D-TOKEN\d-token
support\scripts\run_release_verification.ps1 -SkipOpenCode

# Final real OpenCode gate after UI/runtime handoff is coherent:
support\scripts\run_release_verification.ps1
```

Do not run the frontend build and Tauri Rust tests in parallel, because Vite can replace hashed dist assets while Tauri reads them.
