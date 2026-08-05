# d-token Release Readiness Report

Status: release-candidate evidence report with product-readiness, Headroom coverage, Windows/macOS release-boundary audit, packaging/distribution/signing-readiness audit, handoff manifest, artifact redaction/cleanup audit, and full OpenCode gates
Date: 2026-07-07
Truth source: `docs/PRD.md`
Plan source: `docs/RELEASE_COMPLETION_PLAN.md`

## 1. Executive summary

d-token now has a release-candidate evidence set for the current milestone:

```text
OpenCode / supported agent
鈫?
d-token local service
鈫?
Headroom-derived d-token-owned runtime
鈫?
d-token route adapter
鈫?
configured upstream
鈫?
Event Ledger / Dashboard / Doctor / Logs / Export / App
```

The current gate proves:

1. The desktop frontend builds with route-level chunks.
2. Tauri desktop Rust tests pass, including the latest 2026-07-07 rerun with product-readiness integration coverage.
3. No-touch fixture runtime passes in the latest 2026-07-07 no-OpenCode gate.
4. Mock routed runtime passes with compression, CCR, memory/learn, output-shape, persistence, and redaction checks.
5. Active config runtime passes with route/export redaction checks.
6. Product route config verification passes.
7. Product readiness is now a first-class release gate step (`product_readiness_command`) that proves the App verification path runs against d-token active config and d-token runtime metadata.
8. Headroom-derived capability coverage is now a first-class release gate step (`headroom_capability_coverage`) with 11/11 capability groups covered across runtime, frontend, and verification evidence.
9. Real OpenCode validation passes through a temporary d-token-owned service using the user-authorized local OpenCode command/config path.
10. Artifacts are redacted and contain no raw prompt/response/secrets/raw command output.
11. Relevant temporary ports/processes are cleaned after validation.

## 2. Build and test evidence

Latest full release verification:

- Command: `support/scripts/run_release_verification.ps1`
- Full OpenCode report: `support/artifacts/release-verification-20260707-172419.json`
- Latest no-touch release report: `support/artifacts/release-verification-20260707-184716.json`
- Result: passed for both recorded gates

Steps:

| Step | Result | Evidence |
|---|---:|---|
| Desktop build | Pass | Vite build with page-level chunks |
| Desktop Rust tests | Pass | `cargo test -p d-token-desktop`, latest run includes 9 desktop tests |
| Fixture runtime | Pass | d-token-owned source, fixture RTK/headroom reference, Doctor, route adapter, dashboard |
| Mock routed runtime | Pass | Compression, CCR, memory/learn, output-shape, persistence |
| Active config runtime | Pass | Active route config, fallback disabled, export redaction |
| Agent route config verification | Pass | 6/6 product route verification checks |
| Product readiness command | Pass | release step covers 9/9 readiness checks with d-token active config and d-token runtime metadata |
| Headroom capability coverage | Pass | release step covers 11/11 Headroom-derived capability groups across runtime, frontend, and verification evidence |
| Release distribution audit | Pass for local RC | app/runtime/WebView2/installer hashes recorded; installed startup dependency files present; public distribution remains blocked by unsigned app/runtime/installer |
| Windows signing readiness audit | Pass as read-only boundary | artifacts exist; `can_sign_now=false` because `signtool.exe` is unavailable and no signing certificate was provided |
| Cross-platform release readiness audit | Pass as read-only boundary | Windows local RC is ready; macOS artifact/signing/notarization is not ready on this Windows host |
| Release handoff manifest | Pass | local release-candidate handoff manifest records artifacts, hashes, latest evidence paths, cross-platform boundary, and public-distribution blockers |
| Release artifact redaction audit | Pass | latest release artifacts scanned for raw prompt/response/output fields and credential-shaped values; 0 hits |
| Release cleanup audit | Pass | validation ports and runtime/test processes checked after the gate; no leftovers |
| Installed desktop startup audit | Pass | installed app dependencies present; installed `d-token-desktop.exe` launched and exposed `d-token Control Plane` |
| Real OpenCode workload | Pass | OpenCode 鈫?d-token 鈫?upstream route evidence |

## 3. Frontend readiness

### 3.1 Implemented product surfaces

| Surface | Status | Notes |
|---|---|---|
| Shell / Sidebar / Topbar / StatusBar | Mostly covered | Sidebar/topbar square outer frame verified earlier; collapsed selected item fixed to centered 40x40. |
| Overview / Dashboard | Mostly covered | Activity calendar, token trend, tools ranking; browser preview and runtime states separated. |
| Install / Connect | Mostly covered | Connect/configure/apply/verify flow; no fake stop action; capability summary visible. |
| Advanced / Providers & Routes | Mostly covered | Route draft/apply/readback, active config polling, project policy readback. |
| Advanced / Project Policy | Mostly covered | Scan/recommend/apply/read saved policy; capability summary visible. |
| Analytics | Mostly covered | Runtime latency breakdown and route overhead connected to payload. |
| Doctor | Mostly covered | Product diagnostics, route verification, capability summary in Runtime tab. |
| Logs & Security | Mostly covered | Event Ledger surface, redacted JSON/CSV export. |
| Changes / Rollback | Partial / mostly covered for d-token config | d-token active config rollback works; external third-party config restore remains gated/staged. |
| Settings | Mostly covered | Saved appearance/glass/background/language hydration. |
| Compatibility Matrix | Mostly covered | First-class page exists and desktop data now prefers canonical `/compatibility/agents`, `/protocol-injection`, and `/chain/status` runtime payloads when available. |

### 3.2 UI verification evidence

Latest 2026-07-07 browser UI check on `http://127.0.0.1:5180/` verified all formal pages: Overview, Install, Advanced, Analytics, Doctor, Changes/Rollback, Logs/Security, Compatibility, and Settings.

Evidence from the automated browser check:

- 0 blank / unnamed buttons on every checked page.
- 0 visible hits for forbidden productized testing or fake-service lifecycle wording: `workload`, `benchmark`, `smoke`, `testing center`, `????`, `????`, `??????`, `OpenCode Real Route`, `fake-bin`, `Start service`, `Stop service`, `????`, `????`, `???????`, `One-Click Install`.
- Sidebar and topbar outer border radius measured as `0px` on the Overview page.
- Text-overflow probe found only two tiny empty icon/switch spans per page; no user-facing text overflow was detected.

Previous browser checks verified:

1. Install page shows the Headroom-derived capability panel.
2. Advanced 鈫?Headroom project scan tab shows the Headroom-derived capability panel.
3. Doctor 鈫?Runtime tab shows the Headroom-derived capability panel.
4. These panels show:
   - compression pipeline
   - CCR metadata
   - memory / learn
   - output shaping
   - integrated RTK
   - route adapter
   - dashboard primitives
   - privacy/raw data boundary
5. Checked pages did not show forbidden productized testing wording:
   - `workload`
   - `benchmark`
   - `smoke`
   - `testing center`
   - `娴嬭瘯涓績`
   - `鑳藉姏鑷`
   - `鐪熷疄閾捐矾娴嬭瘯`
   - `OpenCode Real Route`
   - `fake-bin`

Browser checks are UI evidence only. Runtime evidence is listed separately below.

## 4. Runtime and frontend/backend handoff readiness

### 4.1 Runtime payload source rule

The app treats data as real runtime data only when:

```text
source = dtoken-owned-runtime
desktop_data_source = dtoken-local-service
```

Otherwise:

1. Browser preview is labeled as preview.
2. Desktop unavailable state uses empty/waiting states instead of fake runtime facts.

### 4.2 Runtime capabilities covered by current gate

| Capability | Evidence |
|---|---|
| Health / service identity | `/health` and release verification health checks |
| Dashboard payload | `dashboard_source = dtoken-owned-runtime` |
| Route adapter | Mock and OpenCode routes recorded in Dashboard/Event Ledger |
| Compression | Before/after/saved token metrics recorded |
| Stats/history/metrics | Runtime stats/history/metrics payloads and metrics text |
| Latency breakdown | Compression, route adapter, provider, cc-switch fields present |
| Route overhead | Runtime-final vs provider-input token overhead fields present |
| CCR metadata | `/ccr/summary`, persisted `ccr-metadata.jsonl`, `raw_available=false` |
| Memory / learn | `/memory/learn`, `/memory/summary`, persisted `memory-records.jsonl`, raw prompt/response stripped |
| Output shaping | `/output-shape` reflects learned verbosity and `raw_available=false` |
| Integrated RTK | Fixture RTK status/gain/discover plus UI internal capability surface |
| Headroom-derived runtime | `/headroom/status`, `/headroom/proxy`, `/headroom/mcp`, `/headroom/stats`, `/headroom/cache-summary` are d-token-owned and not external dependencies |
| Active config | `/config/active`, route/provider counts, fallback disabled |
| Export | JSON/CSV redacted export, raw unavailable |
| Doctor | Product route/config verification and Doctor checks |

### 4.3 Known caveat

Latency stage values are currently deterministic/derived from request logs:

```text
model = derived_from_compression_elapsed_and_route_forward_latency
```

They are suitable for the current dashboard/accounting model, but not yet measured by persisted per-stage timers. A later runtime hardening pass should persist per-stage timings directly.

## 5. Real OpenCode validation

OpenCode validation report:

- `support/artifacts/opencode-workload-benchmark-20260707-172418.json`

Evidence:

| Field | Value |
|---|---:|
| Route | `opencode-workload-routed` |
| Provider | `opencode-zen` |
| Model | `dtoken/deepseek-v4-flash-free` |
| Workloads | 3 |
| Runtime requests | 6 |
| Before tokens | 49,711 |
| After tokens | 48,278 |
| Saved tokens | 1,433 |
| Savings percent | 2.88% |
| Dashboard source | `dtoken-owned-runtime` |
| Runtime config source | `explicit_cli_config` |
| Fallback disabled | true |
| Export redacted | true |
| Export raw available | false |
| Persisted events | 6 |
| Persisted routes | 6 |
| Persisted logs | 6 |
| Persisted CCR rows | 6 |

Per-run OpenCode artifacts store only:

1. command status
2. elapsed time
3. expected marker
4. output length
5. SHA-256 output hash
6. pass/fail booleans

They do not store raw OpenCode output text.

## 6. Redaction audit

Scanned:

- `support/artifacts/release-verification-20260707-172419.json`
- `support/artifacts/opencode-workload-benchmark-20260707-172418.json`

No matches were found for:

```text
raw_prompt
raw_response
api_key
Authorization
Cookie
Bearer
sk-*
"output":
"prompt":
"response":
```

The release report and OpenCode report both state that no raw prompts, raw responses, API keys, browser session credentials, or auth files are stored.

### 6.1 Latest no-touch release gate

Latest no-touch release verification:

- Command: `support/scripts/run_release_verification.ps1 -SkipOpenCode`
- Report: `support/artifacts/release-verification-20260707-184716.json`
- Result: passed

Covered steps: desktop build, desktop Rust tests, fixture runtime, mock routed runtime, active config runtime, agent route config verification, product readiness command, OpenAI Responses route, compatibility/protocol/chain runtime surface, Anthropic bridge runtime, Headroom-derived capability coverage audit, desktop packaging audit, desktop installer artifact audit, release distribution audit, Windows signing readiness audit, cross-platform release readiness audit, release artifact redaction audit, release cleanup audit, and release handoff manifest.

The full OpenCode real gate was rerun after adding installer artifact verification. The latest full report is `support/artifacts/release-verification-20260707-172419.json`, and the OpenCode sub-report is `support/artifacts/opencode-workload-benchmark-20260707-172418.json`.


### 6.2 Desktop packaging audit

Latest desktop packaging audit:

- Focused report: `support/artifacts/desktop-packaging-audit-20260707-172419.json`
- Included in latest no-touch release gate: `support/artifacts/release-verification-20260707-142417.json`
- Result: 9/9 checks passed

The audit verifies package/Tauri version alignment, bundle activation, Windows NSIS target, app icon, CSP, frontend build script, Tauri build script, existing frontend dist, and product metadata. It explicitly does **not** perform installer signing/notarization; signing status is `unsigned-development-build`.

### 6.3 Desktop installer artifact audit

Actual Tauri/NSIS packaging was run successfully after the packaging config audit. Artifact verification is now included in the release gate.

- Artifact report: `support/artifacts/desktop-bundle-artifact-audit-20260707-172419.json`
- Included in latest no-touch release gate: `support/artifacts/release-verification-20260707-171929.json`
- App executable: `D:\D-TOKEN\d-token\code\target\release\d-token-desktop.exe`
- App executable size: 27,319,024 bytes
- App executable SHA-256: `e656013dc94b81e153599b431eca2eebb1cc5a5d40d40b942a09346c6a275677`
- NSIS installer: `D:\D-TOKEN\d-token\code\target\release\bundle\nsis\d-token_0.2.0_x64-setup.exe`
- NSIS installer size: 9,890,345 bytes
- NSIS installer SHA-256: `8a3ddecee780cfb2db2f1823305a62b878a43010d29da4891d2913e8a41cac22`
- Signing status: `unsigned-development-build`

This proves a real Windows installer artifact exists. It still does not prove public distribution readiness until signing/notarization/reputation hardening is completed.

### 6.4 Release distribution manifest audit

Latest release distribution audit:

- Latest report: `support/artifacts/release-distribution-manifest-20260707-184711.json`
- Included in latest no-touch release gate: `support/artifacts/release-verification-20260707-184716.json`
- Result: passed as a local release-candidate audit.

The audit records hashes and signing status for:

- `D:\D-TOKEN\d-token\code\target\release\d-token-desktop.exe`
- `D:\D-TOKEN\d-token\code\target\release\dtoken-proxy.exe`
- `D:\D-TOKEN\d-token\code\target\release\WebView2Loader.dll`
- `D:\D-TOKEN\d-token\code\target\release\bundle\nsis\d-token_0.2.0_x64-setup.exe`

Current distribution status:

| Field | Value |
|---|---|
| `release_candidate_ready` | true |
| `distribution_ready_public` | false |
| Signing status | `unsigned-or-not-valid` |
| App exe | `NotSigned` |
| Runtime sidecar | `NotSigned` |
| NSIS installer | `NotSigned` |
| WebView2Loader.dll | Valid Microsoft signature |
| Installed dependency check | `d-token-desktop.exe`, `dtoken-proxy.exe`, and `WebView2Loader.dll` present under `C:\Users\10777\AppData\Local\d-token` |

This audit is read-only. It does not sign artifacts, install certificates, access signing credentials, or mutate external services.

### 6.5 Windows signing readiness audit

Latest Windows signing readiness audit:

- Focused report: `support/artifacts/windows-signing-readiness-20260707-182612.json`
- Latest release-gate report: `support/artifacts/windows-signing-readiness-20260707-184712.json`
- Dry-run signing plan: `support/artifacts/windows-release-signing-20260707-182612.json`
- Included in latest no-touch release gate: `support/artifacts/release-verification-20260707-184716.json`
- Result: passed as a read-only readiness boundary.

Current signing status:

| Field | Value |
|---|---|
| `can_sign_now` | false |
| `distribution_ready_public` | false |
| `signtool.exe` | not found |
| Certificate path | not provided |
| Certificate password env | not present / not read |
| Unsigned artifacts | 3: app exe, runtime sidecar, NSIS installer |

Boundary:

- The readiness audit does not sign files, install certificates, read certificate passwords, or access credentials.
- The dry-run signing plan ran without `-Execute`; it recorded artifact hashes and signing prerequisites only.
- Real signing must be a separate explicit action after a Windows signing toolchain and signing certificate are intentionally provided.

### 6.6 Cross-platform release readiness audit

Latest cross-platform release readiness audit:

- Focused report: `support/artifacts/cross-platform-release-readiness-20260707-184255.json`
- Latest release-gate report: `support/artifacts/cross-platform-release-readiness-20260707-184712.json`
- Included in latest no-touch release gate: `support/artifacts/release-verification-20260707-184716.json`
- Result: passed as a read-only release-boundary audit.

Current platform status:

| Platform | Local RC status | Evidence |
|---|---|---|
| Windows | ready | NSIS installer exists and latest handoff manifest is ready |
| macOS | not ready | current Windows host has no macOS bundle target/artifact; macOS runner + code signing/notarization still required |

Current cross-platform status:

| Field | Value |
|---|---|
| `cross_platform_local_release_candidate_ready` | false |
| `public_distribution_ready` | false |
| Windows installer | ready as local unsigned candidate |
| macOS artifact | not built/configured on this host |
| macOS signing/notarization | not complete |

Boundary:

- This audit does not build macOS artifacts on Windows.
- It does not sign, notarize, install certificates, or access credentials.
- It exists so the project does not accidentally claim Windows/macOS public distribution readiness from a Windows-only package.

### 6.7 Release artifact redaction audit

Latest release artifact redaction audit:

- Latest report: `support/artifacts/release-artifact-redaction-audit-20260707-184713.json`
- Included in latest no-touch release gate: `support/artifacts/release-verification-20260707-184716.json`
- Result: passed.

The audit scanned the latest release JSON artifacts, including the latest no-touch release report, latest OpenCode workload report, product-readiness report, service lifecycle report, Headroom capability coverage report, packaging report, bundle artifact report, and distribution manifest.

Policy:

- Release artifacts must not contain raw prompt fields.
- Release artifacts must not contain raw response fields.
- Release artifacts must not contain raw command output fields.
- Release artifacts must not contain credential-shaped values such as OpenAI-style `sk-*`, bearer tokens, AWS access keys, or Google API key shapes.
- The audit records only file paths, JSON paths, and pattern names. It does not store matched secret values.

Latest result:

| Field | Value |
|---|---:|
| Scanned files | 8 |
| Hits | 0 |
| Parse errors | 0 |

This makes artifact redaction a machine-enforced release gate, not just a manual review note.

### 6.8 Release cleanup audit

Latest release cleanup audit:

- Latest report: `support/artifacts/release-cleanup-audit-20260707-184715.json`
- Included in latest no-touch release gate: `support/artifacts/release-verification-20260707-184716.json`
- Result: passed.

The cleanup audit is read-only. It does not kill processes, stop services, mutate shell configuration, or touch external Headroom/RTK/cc-switch/OpenCode configuration.

Current checked cleanup conditions:

| Check | Result |
|---|---|
| No listeners on `18001`, `38101`, `38113`, `38117`, `39122` | Pass |
| No residual `dtoken-proxy.exe`, `opencode.exe`, `opencode.cmd`, `cargo.exe`, `rustc.exe`, or `makensis.exe` | Pass |
| No generated mock-upstream Python scripts still running | Pass |

The audit intentionally does not flag a normal installed or preview `d-token-desktop.exe` app window, because that is a user-facing application process rather than a validation leftover.

### 6.9 Installed desktop startup audit

Latest installed desktop startup audit:

- Latest focused report: `support/artifacts/installed-desktop-startup-audit-20260707-182353.json`
- Result: passed.

This audit exists because a previous installed startup attempt failed before UI load when `WebView2Loader.dll` was missing. The new script makes that class of issue repeatable to verify.

Current checked startup conditions:

| Check | Result |
|---|---|
| Installed `d-token-desktop.exe` exists | Pass |
| Installed `dtoken-proxy.exe` exists | Pass |
| Installed `WebView2Loader.dll` exists | Pass |
| Installed `uninstall.exe` exists | Pass |
| Installed app launches and stays alive | Pass |
| Window title | `d-token Control Plane` |

The audit launches the installed desktop app briefly and then closes the process it started. It is intentionally kept as a focused release/installer-startup gate rather than part of every no-touch release run, because it opens a desktop window. It does not start, stop, or reconfigure external Headroom/RTK/cc-switch/OpenCode services and does not read secrets.


### 6.10 Release handoff manifest

Latest release handoff manifest:

- Focused report: `support/artifacts/release-handoff-manifest-20260707-183504.json`
- Latest release-gate report: `support/artifacts/release-handoff-manifest-20260707-184716.json`
- Included in latest no-touch release gate: `support/artifacts/release-verification-20260707-184716.json`
- Result: passed for local release-candidate handoff.

The handoff manifest is the compact release-delivery summary. It records:

- Windows app executable path, bytes, SHA-256, and signing status.
- d-token runtime sidecar path, bytes, SHA-256, and signing status.
- WebView2Loader path, bytes, SHA-256, and Microsoft signature status.
- NSIS installer path, bytes, SHA-256, and signing status.
- Latest no-touch release report path.
- Latest real OpenCode evidence path.
- Latest distribution, signing-readiness, redaction, cleanup, and installed-startup evidence paths.

Current handoff status:

| Field | Value |
|---|---|
| `local_release_candidate_ready` | true |
| `public_distribution_ready` | false |
| `cross_platform_local_release_candidate_ready` | false |
| Public blockers | macOS artifact/signing readiness incomplete; unsigned app/runtime/installer; Windows signing toolchain/certificate not ready |
| Installer | `D:\D-TOKEN\d-token\code\target\release\bundle\nsis\d-token_0.2.0_x64-setup.exe` |
| Installer SHA-256 | `bec7724cbc3c99a0794a1e4f4e4423c1794a7e9d27adb5872683953171391d41` |

This manifest stores only hashes, paths, and status. It does not include raw prompts, raw responses, secrets, auth files, or certificate passwords.

## 7. Process and port cleanup

After the full release gate:

1. No relevant validation ports remained in LISTEN state:
   - 38101
   - 38113
   - 38117
   - 39122
   - 18001
2. No d-token proxy, mock upstream, active-config mock, agent-route mock, or OpenCode validation process remained.
3. The only process match during cleanup inspection was the PowerShell command performing the inspection itself.

## 8. Performance and bundle summary

Latest observed build characteristics:

| Artifact | Approx size |
|---|---:|
| Main app chunk | ~356 kB |
| AreaChart vendor chunk | ~386 kB lazy chunk |
| Install page chunk | ~69.5 kB |
| Analytics page chunk | ~25.1 kB |
| Advanced page chunk | ~28.5 kB |
| CapabilitySummary chunk | ~6.8 kB |

The app uses lazy page chunks. The main app chunk did not materially regress after adding capability summaries.

## 9. No-touch boundary status

The release verification states and current evidence show:

1. External Headroom, RTK, and cc-switch services were not started, stopped, reconfigured, or used as product data sources.
2. Fixture and mock gates use temporary d-token-owned runtime processes and temporary data directories.
3. OpenCode real validation uses the user-authorized local OpenCode command/config path.
4. Reports are redacted and do not store secrets or raw prompt/response content.

## 10. Current limitations / release deferrals

These are known and should not be hidden:

1. Full OS-level persistent service lifecycle start/stop from the normal UI is not claimed yet. The UI uses install/connect/configure/verify language until identity-checked lifecycle commands are implemented.
2. Per-stage latency is derived, not persisted as separate measured timers yet.
3. Full OS-level persistent service lifecycle remains deferred until identity-checked native lifecycle commands are implemented and verified.
4. External third-party agent config rollback remains staged/gated. d-token-owned active config rollback works.
5. Packaging configuration audit and actual unsigned Windows NSIS installer artifact are covered. Signing/notarization/reputation hardening for public distribution remains incomplete.

## 11. PRD acceptance status for current milestone

| PRD requirement area | Status |
|---|---|
| d-token-owned runtime data source | Covered |
| External Headroom no-touch boundary | Covered |
| OpenCode as first real validation entry | Covered |
| Formal UI avoids development testing concepts | Covered in latest all-page browser check |
| Multi-agent product model | Mostly covered |
| Frontend/backend real-time handoff | Mostly covered; latest no-touch release gate passed |
| Compression / CCR / memory / output shaping / integrated RTK surfaces | Mostly covered |
| Route/provider/fallback visibility | Covered for current gates |
| Event Ledger / logs / export redaction | Covered for current gates |
| Backup / rollback | Covered for d-token active config; external config restore deferred |
| Compatibility matrix | UI covered; canonical runtime/protocol/chain data handoff mostly covered |
| Release packaging/signing | Local release-candidate installer covered; distribution and signing-readiness audits say public distribution is false until signtool/certificate signing is completed |

## 12. Recommendation

The project is a release candidate for the current PRD milestone if the target is:

```text
buildable desktop app
+ d-token-owned runtime proof
+ UI coverage for PRD capabilities
+ real OpenCode validation
+ redacted artifacts
+ no-touch safety for external Headroom/RTK/cc-switch
```

Before public distribution, complete signing/notarization/reputation hardening work. Current Windows signing readiness says the artifacts exist, but `signtool.exe` and signing credentials are not available, so `can_sign_now=false`. The release handoff manifest now makes the local-RC/public-distribution split explicit for every handoff, including the Windows-ready/macOS-not-yet-ready boundary. Identity-checked app-managed d-token service lifecycle is now verified by the release gate; OS-level service registration can remain a later hardening track if required.

## 11. 2026-07-07 service lifecycle and bundled runtime update

Latest full release verification after app-managed service lifecycle and runtime sidecar packaging changes:

- Full release report: `support/artifacts/release-verification-20260707-172419.json`
- OpenCode report: `support/artifacts/opencode-workload-benchmark-20260707-172418.json`
- No-touch report before full OpenCode rerun: `support/artifacts/release-verification-20260707-171929.json`
- Service lifecycle report: `support/artifacts/dtoken-service-lifecycle-20260707-172341.json`
- Bundle artifact report: `support/artifacts/desktop-bundle-artifact-audit-20260707-172419.json`

Evidence added in this checkpoint:

1. App-managed d-token service lifecycle is identity-checked and verified: managed manifest, stale manifest cleanup, mismatched identity refusal, real `dtoken-proxy` start/health/stop on `127.0.0.1:18001`, and port cleanup after stop.
2. The desktop package now prepares and bundles the d-token runtime sidecar through Tauri `externalBin`. Both Windows GNU and MSVC sidecar filenames are prepared.
3. Artifact audit now requires the installer to be built after sidecar preparation, preventing stale installer artifacts from being counted as release evidence.
4. The regenerated NSIS installer is `9,890,345` bytes with SHA-256 `8a3ddecee780cfb2db2f1823305a62b878a43010d29da4891d2913e8a41cac22`.
5. Full OpenCode real validation still passes after lifecycle and packaging changes: 3 workloads, 6 runtime requests, 49,709 before tokens, 48,276 after tokens, 1,433 saved tokens, 2.88% savings, fallback disabled, export redacted, raw export unavailable, and persisted Event Ledger rows.
6. Field-level redaction audit found no `raw_prompt`, `raw_response`, `api_key`, `Authorization`, `Cookie`, `Bearer`, `output`, `prompt`, or `response` keys in the latest full release and OpenCode reports.

Remaining public-release caveat: the artifact is still an unsigned development build. Signing/notarization/reputation hardening is required before public distribution.

## 12. 2026-07-07 installed Windows startup fix

User-provided installed-app startup evidence showed a release blocker:

```text
d-token-desktop.exe failed because WebView2Loader.dll was missing
```

Root cause:

- The previous artifact audit verified the release build directory and runtime sidecar, but did not verify that the NSIS installer placed `WebView2Loader.dll` next to the installed `d-token-desktop.exe`.

Fix:

- `WebView2Loader.dll` is now copied into `code/apps/desktop/src-tauri/binaries/WebView2Loader.dll` during sidecar preparation.
- Tauri `bundle.resources` installs `binaries/WebView2Loader.dll` into the app root.
- Packaging audit now includes WebView2Loader resource checks.
- Bundle artifact audit now includes WebView2Loader hash and installer freshness checks.

Latest evidence:

- Packaging audit: `support/artifacts/desktop-packaging-audit-20260707-174051.json`, 13/13 checks passed.
- Bundle artifact audit: `support/artifacts/desktop-bundle-artifact-audit-20260707-174052.json`, 12/12 checks passed.
- No-touch release gate: `support/artifacts/release-verification-20260707-174527.json`, passed.
- Rebuilt NSIS installer: `D:\D-TOKEN\d-token\code\target\release\bundle\nsis\d-token_0.2.0_x64-setup.exe`
- Installer size: 9,956,611 bytes
- Installer SHA-256: `bec7724cbc3c99a0794a1e4f4e4423c1794a7e9d27adb5872683953171391d41`
- App executable SHA-256: `cb3d8d0ba03ecd0465a4aef50a4515f74a304851d4f942703daf1049e86c7c63`
- WebView2Loader SHA-256: `8427b1fc58ec707813e5c0a51eb5d69397bb333250a7b891be4d3b123f1e0f1c`

Installed-app verification:

- Silent install completed with exit code 0.
- Installed directory `C:\Users\10777\AppData\Local\d-token` contains:
  - `d-token-desktop.exe`
  - `dtoken-proxy.exe`
  - `WebView2Loader.dll`
  - `uninstall.exe`
- Launching `C:\Users\10777\AppData\Local\d-token\d-token-desktop.exe` kept a live process with window title `d-token Control Plane`. This is now repeatable through `support/scripts/run_installed_desktop_startup_audit.ps1`, latest report `support/artifacts/installed-desktop-startup-audit-20260707-181725.json`.

OpenCode note:

- The OpenCode real gate was not rerun for this specific checkpoint because the change only fixes installer startup packaging and does not change d-token routing/runtime behavior. The latest full OpenCode evidence remains `support/artifacts/release-verification-20260707-172419.json`.
