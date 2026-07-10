# M4 OpenCode Test Results — 2026-07-04

状态：Passed

## Latest Run — Real OpenCode via d-token

时间：2026-07-04 最新复测

本次按用户要求使用原本 OpenCode 配置做真实测试，不使用隔离 HOME。测试链路为：

```text
OpenCode -> d-token local service :18001 -> d-token Headroom-derived runtime -> opencode-zen upstream
```

命令：

```powershell
powershell -ExecutionPolicy Bypass -File support/scripts/run_opencode_real_routed_test.ps1
```

结果：

```json
{
  "health": "healthy",
  "model": "dtoken/deepseek-v4-flash-free",
  "route": "opencode-real-routed",
  "provider": "opencode-zen",
  "agent": "OpenCode",
  "saved_tokens": 474,
  "dashboard_source": "dtoken-owned-runtime",
  "persisted_events": 1,
  "persisted_routes": 1,
  "persisted_logs": 1,
  "persisted_ccr": 1,
  "ccr_entries": 1,
  "ccr_raw_available": false,
  "data_dir": "C:\\Users\\10777\\AppData\\Local\\Temp\\dtoken-opencode-real-data-86b297d68f2e41a693daa29ab7561dd1"
}
```

新增验证：

- `/stats`、`/stats-history`、`/metrics`、`/route-decisions/recent`、`/logs/recent`、`/events` 已读取同一个 d-token `AppState` 运行态。
- 空运行态返回 0 和空列表，不再用外部 Headroom 或默认假事件伪装成真实请求。
- d-token proxy 现在支持 `--data-dir` / `DTOKEN_DATA_DIR`，真实运行态写入 `events.jsonl`、`route-decisions.jsonl`、`request-logs.jsonl`，重启后可读回。
- d-token-owned CCR metadata 已接入：`ccr-metadata.jsonl`、`/ccr/summary`、`/ccr/retrieve/{recovery_ref}`；默认 `raw_available=false`，不保存 raw prompt/response/CCR body。
- Dashboard payload 主数据源为 `dtoken-owned-runtime`；外部 Headroom `8787` 仍只允许作为 no-touch 参考。
- Desktop App `get_dtoken_dashboard_payload` 现在优先读取 `http://127.0.0.1:18001/dashboard/payload`，并在 payload 中标记 `desktop_data_source=dtoken-local-service`；本地服务不可用时才 fallback 到 `local-empty-fallback`。
- Analytics 页面会展示 desktop data source，避免把空 fallback 误认为真实 runtime 数据。
- d-token-owned memory/learn/output shaping metadata 已接入：`memory-records.jsonl`、`/memory/learn`、`/memory/summary`、`/output-shape`；写入时会剔除 raw prompt、raw response、messages、body、content。
- `support/scripts/run_dtoken_routed_mock_test.ps1` 仍通过，证明本地 route adapter + mock upstream 路径未回归。
- `dtoken-compress` 已替换 placeholder，当前压缩引擎会执行 blank-line trim、repeat-line fold、log-prefix normalize、JSON compact measurement、aggressive long-text summarize，并记录 `elapsed_us`。
- Dashboard payload 的 `metrics.compression_performance` 现在从真实 request logs 聚合 transform 级性能：requests、tokens_saved、tokens_before、avg_latency_ms、savings_percent。
- `/v1/messages` 已支持 Anthropic-shaped input -> d-token compression -> OpenAI-compatible/cc-switch-style configured provider bridge；缺失显式 route 时 fallback disabled。
- `cargo test -p d-token-desktop` 通过 1 test；`cargo test -p dtoken-compress` 通过 3 tests；`cargo test -p dtoken-core-api` 通过 11 tests；`cargo test -p dtoken-proxy` 通过 1 test；`cargo test -p dtoken-process` 通过 4 tests；`cargo check -p d-token-desktop` 通过；`npm run build` 通过。

## Memory / Learn / Output Shaping

Mock routed test now also verifies memory/learn metadata:

```json
{
  "persisted_memory": 1,
  "memory_entries": 1
}
```

Evidence:

- `/memory/learn` accepted explicit metadata for project/agent/verbosity.
- `memory-records.jsonl` was created under the d-token data dir.
- `/memory/summary` reported the entry.
- The test intentionally submitted `raw_prompt="must not persist"` and verified the summary did not leak it.
- `/output-shape` is backed by learned metadata and can return learned verbosity rules.

## Anthropic / cc-switch-style Bridge Mock

Command:

```powershell
powershell -ExecutionPolicy Bypass -File support/scripts/run_dtoken_anthropic_bridge_mock_test.ps1
```

Result:

```json
{
  "health": "healthy",
  "type": "message",
  "content": "anthropic-bridge-ok:true",
  "route": "anthropic-to-cc-switch",
  "provider": "cc-switch-openai",
  "input_protocol": "anthropic",
  "output_protocol": "openai",
  "saved_tokens": 9,
  "dashboard_source": "dtoken-owned-runtime",
  "persisted_events": 1,
  "persisted_routes": 1,
  "persisted_logs": 1,
  "persisted_ccr": 1,
  "ccr_entries": 1,
  "ccr_raw_available": false
}
```

Evidence:

- d-token accepted Anthropic-shaped `/v1/messages`.
- d-token compressed and adapted the request to an OpenAI-compatible upstream body.
- The mock upstream confirmed it received folded repeated content (`anthropic-bridge-ok:true`).
- Dashboard/Event Ledger recorded `input_protocol=anthropic` and `output_protocol=openai`.
- The bridge route persisted events, route decisions, and request logs to d-token-owned JSONL files.
- The bridge route also persisted CCR metadata and confirmed raw CCR retrieval remains disabled by default.

## Compression Benchmark

Command:

```powershell
cd code
cargo run -p dtoken-compress --example benchmark
```

Structured command:

```powershell
powershell -ExecutionPolicy Bypass -File support/scripts/run_compression_benchmark.ps1
```

Structured summary:

```json
{
  "source": "dtoken-compress-local-benchmark",
  "case_count": 8,
  "total_original_tokens": 3000,
  "total_compressed_tokens": 2218,
  "total_saved_tokens": 782,
  "total_savings_percent": 26.07,
  "avg_engine_elapsed_us": 341.12,
  "avg_wall_elapsed_us": 341.62,
  "best_case": "long_text/aggressive",
  "best_case_saved_tokens": 718,
  "best_ratio_case": "long_text/aggressive",
  "best_ratio_percent": 53.26,
  "report_path": "D:\\D-TOKEN\\d-token\\support\\artifacts\\compression-benchmark-20260705-114005.json"
}
```

Latest benchmark output:

| Case | Policy | Before tokens | After tokens | Saved | Ratio | Engine elapsed |
|------|--------|---------------|--------------|-------|-------|----------------|
| logs | balanced | 57 | 35 | 22 | 38.60% | 194 us |
| logs | aggressive | 57 | 35 | 22 | 38.60% | 97 us |
| code | balanced | 51 | 43 | 8 | 15.69% | 95 us |
| code | aggressive | 51 | 43 | 8 | 15.69% | 95 us |
| json | balanced | 44 | 42 | 2 | 4.55% | 96 us |
| json | aggressive | 44 | 42 | 2 | 4.55% | 101 us |
| long_text | balanced | 1348 | 1348 | 0 | 0.00% | 1710 us |
| long_text | aggressive | 1348 | 630 | 718 | 53.26% | 1123 us |

Interpretation:

- Log-heavy repeated output is currently the strongest balanced-policy win: 38.60% token reduction with sub-millisecond engine time.
- Code/test output with repeated blocks gets a smaller but stable reduction: 15.69%.
- Pretty JSON alone is low-yield in this synthetic sample: 4.55%, mostly from compact measurement.
- Long text only reduces under aggressive policy: 53.26%, because balanced mode intentionally avoids summarizing large prose.
- The benchmark is synthetic and local; final release testing still needs larger OpenCode-generated project workloads, streaming route tests, and per-provider latency.

## Direct OpenCode Diagnostic Scope

以下内容是早期 direct OpenCode diagnostic，用于确认 OpenCode 命令和用户原本配置中的模型可用性，不作为 d-token routed path 验收证据：

```text
opencode run -m <model> "Reply with exactly: d-token-ok"
```

诊断环境：

- OpenCode version: `1.17.12`
- `DTOKEN_TEST_SCOPE=opencode-only`
- `DTOKEN_TEST_EXCLUDE=claude-code,codex-cli,cursor,vscode,aider,cline,roo,continue,windsurf`
- `NO_PROXY=127.0.0.1,localhost`
- Removed for the test process:
  - `ANTHROPIC_BASE_URL`
  - `ANTHROPIC_API_URL`
  - `CLAUDE_CODE_PROXY`

This is a direct OpenCode command diagnostic. Real routed evidence is in the `Latest Run — Real OpenCode via d-token` and `Real OpenCode Routed Test` sections above.

## Model Discovery

Command:

```text
opencode models opencode
```

Models returned:

- `opencode/big-pickle`
- `opencode/deepseek-v4-flash-free`
- `opencode/mimo-v2.5-free`
- `opencode/nemotron-3-ultra-free`
- `opencode/north-mini-code-free`

## Results

| Model | Result | Expected output |
|------|--------|-----------------|
| `opencode/big-pickle` | Passed | `d-token-ok` |
| `opencode/deepseek-v4-flash-free` | Passed | `d-token-ok` |
| `opencode/mimo-v2.5-free` | Passed | `d-token-ok` |
| `opencode/nemotron-3-ultra-free` | Passed | `d-token-ok` |
| `opencode/north-mini-code-free` | Passed | `d-token-ok` |

## Verification

- `cargo test -p dtoken-process`: passed, 4 tests after adding OpenCode ledger event redaction coverage.
- `cargo test -p dtoken-core-api`: passed, 3 tests after adding d-token-owned dashboard payload coverage.
- `cargo check -p d-token-desktop`: passed.
- `npm run build` in `code/apps/desktop`: passed.
- Browser preview at `http://127.0.0.1:5180/`: Install page now labels direct OpenCode model checks as diagnostic only and points real validation at `OpenCode dtoken/<model> -> d-token 127.0.0.1:18001`.
- Analytics page reads `get_dtoken_dashboard_payload` from the Tauri backend in desktop mode. The payload source is `dtoken-owned-runtime`, not external Headroom `8787`; top metrics now prefer runtime request logs, runtime token totals, runtime route transforms, and runtime payload status before fixture fallback.
- Doctor page now uses `detect_tools` plus `get_dtoken_dashboard_payload` for the real route check; the old fake `setTimeout` run-all behavior was removed.
- OpenCode direct diagnostic regression re-run after dashboard payload changes: all five discovered OpenCode models returned `d-token-ok`.

## Notes

- No provider fallback was used or configured by d-token.
- No Claude Code, Codex CLI, Cursor, Cline, OpenCode config, shell profile, or system proxy config was modified by this test.
- Direct OpenCode model checks remain diagnostic only and are not accepted as final route evidence.
- The current route evidence is the real routed test below: `OpenCode -> d-token local service :18001 -> d-token Headroom-derived runtime -> opencode-zen upstream`.
- The remaining work is not "prove route exists" anymore; it is broader capability coverage: streaming parity, more upstream profiles, cc-switch route hardening, richer UI wiring, and larger benchmark suites.

## UI Runtime Data Wiring Update

Date: 2026-07-04 continuation

Changed surfaces:

- `Analytics`: runtime payload is the primary source for requests, tokens before/after/saved, latency, agent rows, transform rows, history, CCR metadata count, memory count, and route/pipeline totals. Fixture data is now visibly fallback/preview, not blended as real evidence.
- `Analytics`: transform rows now prefer `metrics.compression_performance.transforms`, so UI can display backend-calculated per-transform request count, token savings, and savings percentage instead of recomputing everything in React.
- `Doctor`: "Run all" invokes real native checks and d-token dashboard payload reads instead of simulated timers. It reports OpenCode availability and d-token `127.0.0.1:18001` runtime payload state.
- `Install`: OpenCode panel now says "OpenCode real route test"; the existing direct model command is labeled diagnostic only and cannot be mistaken for final route evidence.
- `Install`: service install/restart action now calls the Tauri `run_install_transaction` command and renders native transaction logs; the previous front-end timer workflow was removed.
- `Install`: agent configure confirmation now calls `save_app_settings(section="app")` with a local agent-route draft and logs the native result instead of using a simulated timeout.
- `Advanced`: project policy scan now calls the Tauri `recommend_project_policy` command backed by `dtoken-project-policy::RuleBasedRecommendationEngine`; the old simulated timeout recommendation was removed.
- `Advanced`: applying the recommendation saves a local `project-policy` settings payload through `save_app_settings`; it stores aggregate policy metadata only, not raw prompts or outputs.
- Legacy route/provider fixture defaults were moved from `8788/8789/8790` examples to d-token `18001` route-adapter examples.

Additional Logs/Security update:

- `LogsSecurity`: the Logs tab now calls `get_dtoken_dashboard_payload` and maps d-token-owned `request_logs`, `route_decisions`, and `events` into the terminal log stream.
- `LogsSecurity`: filter counts now come from runtime data when Tauri payload is available. Browser preview remains explicitly marked as fixture fallback.
- `LogsSecurity`: empty runtime state is shown as an empty d-token runtime state, not as fake activity. Running an OpenCode routed test will populate the same request/route/event chain shown in Analytics and Doctor.
- `LogsSecurity`: export now writes a redacted JSON snapshot of the currently filtered lines. It exports metadata/log summaries only, not secrets, prompts, full responses, or OpenCode credentials.
- `Advanced/Routing`: Providers, routes, and route ports now prefer d-token-owned runtime `ProviderProfile`, `RouteProfile`, and `InstanceProfile` data from `get_dtoken_dashboard_payload`.
- `Advanced/Routing`: Browser preview or empty runtime config is explicitly labeled as fixture/empty fallback; real runtime rows display provider kind/protocol/base URL, key reference status, route chain mode, target provider, local port, and fallback state.
- `Advanced/Routing`: fallback is shown as disabled unless the runtime route has an explicit `fallback_provider_id`, matching the PRD no-silent-fallback requirement.
- `Overview`: top cards and savings trend now prefer d-token runtime `stats`, `request_logs`, and `history` from `get_dtoken_dashboard_payload`.
- `Overview`: runtime mode displays saved tokens, estimated cost saved, compression rate, active agents, provider error count, and trend from the same request-log chain used by Analytics/Logs/Doctor. Browser preview and empty runtime states are explicitly labeled.
- `Overview/Heatmap`: activity calendar, token trend, and tool ranking now accept runtime-derived data. Overview aggregates `request_logs` into daily sessions/tokens/tool calls and passes that into the AiMaMi-style activity panel; fixture data remains only as explicit preview/empty fallback.
- `Changes/Rollback`: pending changes and rollback points now prefer the Tauri desktop payload (`get_desktop_data`) instead of static Claude/Codex fixture examples. Approve writes a d-token-owned local settings draft through `save_app_settings`; reject only hides the local pending row; rollback loads a d-token rollback plan through `manage_backup`. No external agent config is restored or modified by this UI path.
- `Settings`: appearance, runtime mode, redaction level, telemetry state, d-token port/bind display now save through `save_app_settings(section="app")` into d-token local app state. Browser preview remains session-only. Telemetry remains off by default and is labeled local diagnostics scope only.
- `Core API Export`: d-token-owned runtime now exposes redacted `/export/json` and `/export/csv` endpoints. Exports include stats, history, metrics, events, route decisions, request logs, CCR metadata summary, and memory summary; they explicitly exclude raw prompts, raw responses, auth headers, cookies, API keys, CCR bodies, and full command output.
- `Headroom parity endpoints`: `/headroom/proxy`, `/headroom/mcp`, `/headroom/stats`, and `/headroom/cache-summary` now derive from d-token runtime state instead of fixture/mock Headroom data. `/headroom/status` still detects external Headroom as reference-only, but marks d-token-owned Headroom-derived runtime as the product data source and `external_headroom_dependency=false`.

Verification:

```powershell
npm run build
cargo test -p d-token-desktop
cargo test -p dtoken-process
cargo test -p dtoken-core-api
cargo check -p d-token-desktop
```

Results:

- `npm run build`: passed, Vite chunk-size warning only.
- `cargo test -p d-token-desktop`: passed, 3 tests after adding project policy and install transaction coverage.
- `cargo test -p dtoken-process`: passed, 4 tests.
- `cargo test -p dtoken-core-api`: passed, 13 tests after adding Headroom parity runtime-state coverage.
- `cargo check -p d-token-desktop`: passed.
- `cargo run -p dtoken-compress --example benchmark`: passed; latest local benchmark table above.

## Real OpenCode Workload Benchmark

Command:

```powershell
powershell -ExecutionPolicy Bypass -File support/scripts/run_opencode_workload_benchmark.ps1
```

Scope:

- Uses the user's real OpenCode config and `dtoken/deepseek-v4-flash-free`.
- Starts only d-token-owned local service on `127.0.0.1:18001`.
- Does not start, stop, reconfigure, or depend on external Headroom `8787`, RTK, or cc-switch.
- Sends three workload prompts through `OpenCode -> d-token :18001 -> opencode-zen upstream`.

Result:

```json
{
  "health": "healthy",
  "model": "dtoken/deepseek-v4-flash-free",
  "workload_count": 3,
  "route": "opencode-workload-routed",
  "provider": "opencode-zen",
  "dashboard_source": "dtoken-owned-runtime",
  "runtime_config_source": "explicit_cli_config",
  "runtime_config_route_count": 1,
  "runtime_config_provider_count": 1,
  "runtime_config_fallback_disabled": true,
  "total_before_tokens": 49224,
  "total_after_tokens": 47791,
  "total_saved_tokens": 1433,
  "savings_percent": 2.91,
  "export_redacted": true,
  "export_raw_available": false,
  "export_active_config_source": "explicit_cli_config",
  "persisted_events": 6,
  "persisted_routes": 6,
  "persisted_logs": 6,
  "persisted_ccr": 6,
  "report_path": "D:\\D-TOKEN\\d-token\\support\\artifacts\\opencode-workload-benchmark-20260705-115410.json"
}
```

Runtime compression performance from `metrics.compression_performance`:

| Transform | Requests | Tokens before | Tokens saved | Savings | Avg latency |
|-----------|----------|---------------|--------------|---------|-------------|
| blank_line_trim | 6 | 49,224 | 1,433 | 2.91% | 8.83 ms |
| json_compact_measurement | 6 | 49,224 | 1,433 | 2.91% | 8.83 ms |

Route/provider/policy performance from `metrics.route_performance`:

| Group | Name | Requests | Failed | Tokens before | Tokens saved | Savings | Avg latency |
|-------|------|----------|--------|---------------|--------------|---------|-------------|
| route | opencode-workload-routed | 6 | 0 | 49,224 | 1,433 | 2.91% | 8.83 ms |
| provider | opencode-zen | 6 | 0 | 49,224 | 1,433 | 2.91% | 8.83 ms |
| policy | dtoken-owned-runtime | 6 | 0 | 49,224 | 1,433 | 2.91% | 8.83 ms |

OpenCode CLI note:

- All three latest workload prompts returned assistant output containing the requested `d-token-ok` marker, and all requests were recorded by d-token route/request/CCR persistence.
- The script also verifies `/config/active`, redacted `/export/json`, and `raw_available=false`, so the workload result proves route, config source, export privacy, and dashboard data consistency together.
- The script now invokes OpenCode through .NET `ProcessStartInfo` and records clean `command_exit_code=0` for all three workload prompts.

# M4 OpenCode / d-token Route Test Results — 2026-07-04

## Current Routed d-token Smoke

Scope: local d-token-owned service only. External Headroom `8787` was not started, stopped, restarted, reconfigured, or used as dashboard data source.

Command shape:

```text
POST http://127.0.0.1:38111/v1/chat/completions
model=opencode/deepseek-v4-flash-free
```

Result:

```json
{
  "health": "healthy",
  "content": "d-token-ok",
  "route": "dtoken-opencode-local",
  "provider": "opencode-official-free",
  "saved_tokens": 6,
  "dashboard_source": "dtoken-owned-runtime",
  "dashboard_route": "dtoken-opencode-local",
  "dashboard_log_agent": "OpenCode"
}
```

Evidence:

- d-token accepted an OpenAI-compatible request on its own local service.
- d-token produced a route decision with fallback disabled.
- d-token produced a compression summary without storing full prompt/response.
- Dashboard payload read the same routed request from d-token-owned state.
- This is a local routed smoke, not yet a full `opencode CLI -> d-token -> real upstream` test.

## Configured Upstream Forwarding Smoke

Scope: local d-token service plus local mock OpenAI-compatible upstream. This verifies the provider forwarding path without touching external Headroom or global OpenCode config.

Command:

```powershell
powershell -ExecutionPolicy Bypass -File support/scripts/run_dtoken_routed_mock_test.ps1
```

Result:

```json
{
  "health": "healthy",
  "content": "mock-upstream-ok",
  "route": "opencode-routed",
  "provider": "mock-upstream",
  "upstream_status": 200,
  "saved_tokens": 6,
  "dashboard_source": "dtoken-owned-runtime",
  "dashboard_route": "opencode-routed",
  "dashboard_agent": "OpenCode"
}
```

Evidence:

- d-token loaded a ProviderProfile / RouteProfile from config.
- d-token accepted an OpenAI-compatible request.
- d-token computed compression statistics before forwarding.
- d-token forwarded to the configured upstream and preserved fallback=disabled metadata.
- Dashboard payload reflected the same route and agent after the request.
- This is still a mock upstream test; the remaining gap is real `opencode CLI -> d-token -> OpenCode official/free upstream`.

## Real OpenCode Routed Test

Scope: user-requested real OpenCode configuration. `C:\Users\10777\.config\opencode\opencode.json` was backed up, then a `dtoken` provider was added with `baseURL=http://127.0.0.1:18001/v1`. d-token used OpenCode Zen as the configured upstream: `https://opencode.ai/zen/v1`.

Backup created:

```text
C:\Users\10777\.config\opencode\opencode.json.dtoken-backup-20260704-162022
```

Command:

```powershell
powershell -ExecutionPolicy Bypass -File support/scripts/run_opencode_real_routed_test.ps1
```

Result:

```json
{
  "health": "healthy",
  "model": "dtoken/deepseek-v4-flash-free",
  "opencode_output": "> build 路 deepseek-v4-flash-free",
  "route": "opencode-real-routed",
  "provider": "opencode-zen",
  "agent": "OpenCode",
  "saved_tokens": 474,
  "dashboard_source": "dtoken-owned-runtime",
  "persisted_events": 1,
  "persisted_routes": 1,
  "persisted_logs": 1
}
```

Interpretation:

- OpenCode used the real user config and selected provider/model `dtoken/deepseek-v4-flash-free`.
- The request reached d-token local service on `127.0.0.1:18001`.
- d-token routed to configured upstream provider `opencode-zen`.
- d-token dashboard recorded the same request with `agent=OpenCode`.
- Compression summary recorded `saved_tokens=474` for the latest OpenCode request using the unified `dtoken-compress` engine.
- The latest routed test also verified d-token-owned JSONL persistence for events, route decisions, and request logs.
- The latest routed test also verified CCR metadata persistence with `raw_available=false`.
- The remaining work is broader capability coverage: more models, streaming parity, Anthropic-compatible route, cc-switch route, full Headroom feature migration, UI wiring, and larger performance benchmark suites.

## Current Direct OpenCode Diagnostic Model Checks

## Desktop Runtime Export Wiring

Scope: desktop Logs/Security export now prefers d-token-owned runtime export endpoints instead of only exporting the currently visible front-end log rows.

Implementation evidence:

- Tauri command `export_dtoken_runtime(format)` reads `http://127.0.0.1:18001/export/json` or `/export/csv`.
- Logs/Security export button calls the Tauri command in desktop runtime and downloads the redacted runtime export.
- Browser preview fallback still exports only the current visible redacted lines and labels itself as `frontend-visible-lines-fallback`.
- No external Headroom / RTK / cc-switch service is started, stopped, reconfigured, or used as the product data source.

Verification:

```powershell
cd code/apps/desktop
rtk npm run build

cd ../../
rtk cargo test -p d-token-desktop
rtk cargo check -p d-token-desktop
rtk cargo test -p dtoken-core-api
```

Result:

```text
npm run build: passed, Vite chunk-size warning only
cargo test -p d-token-desktop: 3 passed
cargo check -p d-token-desktop: passed
cargo test -p dtoken-core-api: 13 passed
```

## OpenAI-compatible Streaming Boundary and SSE Passthrough

Scope: OpenCode and other OpenAI-compatible agents may send `stream=true`. The runtime now distinguishes real configured routes from the built-in no-upstream test path.

Change:

- Configured OpenAI route/provider path now supports SSE passthrough for `stream=true`.
- d-token compresses and accounts for the upstream request body before forwarding.
- d-token proxies the upstream `text/event-stream` response body to the client without storing raw SSE content.
- Runtime records route decision, request log, CCR metadata, and Event Ledger metadata with `stream=true`, `stream_passthrough=true`, and `fallback=disabled`.
- The no-upstream built-in test path still returns explicit `501 Not Implemented` with `streaming_not_supported_yet` instead of faking a non-streaming success.
- No upstream fallback is attempted and no external Headroom service is used.

Verification:

```powershell
cd code
rtk cargo test -p dtoken-core-api
rtk cargo check -p dtoken-core-api
rtk cargo test -p d-token-desktop
rtk cargo check -p d-token-desktop

cd apps/desktop
rtk npm run build
```

Result:

```text
cargo test -p dtoken-core-api: 14 passed
cargo check -p dtoken-core-api: passed
cargo test -p d-token-desktop: 3 passed
cargo check -p d-token-desktop: passed
npm run build: passed, Vite chunk-size warning only
```

Follow-up verification:

```text
cargo test -p dtoken-core-api: 15 passed
cargo check -p dtoken-core-api: passed
cargo test -p d-token-desktop: 3 passed
cargo check -p d-token-desktop: passed
npm run build: passed, Vite chunk-size warning only
```

Remaining gap:

- Real OpenCode streaming test through the user's configured provider still needs to be run and added to this report. Current streaming evidence uses a local mock SSE upstream inside the Rust test suite.

## Real OpenCode Zen Streaming Route Test

Scope: d-token local service routed a real OpenAI-compatible streaming request to the user's configured OpenCode Zen upstream (`https://opencode.ai/zen/v1`). OpenCode CLI does not expose a `--stream` flag in `opencode run --help`, so this test exercises the same configured OpenCode Zen upstream through d-token with an explicit `stream=true` request.

Command:

```powershell
powershell -ExecutionPolicy Bypass -File support/scripts/run_opencode_real_streaming_route_test.ps1
```

Result:

```json
{
  "health": "healthy",
  "model": "deepseek-v4-flash-free",
  "route": "opencode-real-streaming",
  "provider": "opencode-zen",
  "status_code": 200,
  "content_type": "text/event-stream; charset=utf-8",
  "response_contains_sse_data": true,
  "response_contains_done": true,
  "response_contains_expected": false,
  "elapsed_ms": 25709,
  "saved_tokens": 0,
  "input_tokens_before": 54,
  "input_tokens_after": 54,
  "transforms": ["json_compact_measurement"],
  "dashboard_source": "dtoken-owned-runtime",
  "persisted_events": 1,
  "persisted_routes": 1,
  "persisted_logs": 1,
  "persisted_ccr": 1,
  "export_redacted": true,
  "export_raw_available": false
}
```

Interpretation:

- d-token accepted an explicit OpenAI-compatible `stream=true` request.
- d-token forwarded the request to real OpenCode Zen upstream and returned SSE (`text/event-stream`) to the caller.
- The SSE body included data frames and `[DONE]`.
- d-token Dashboard recorded route `opencode-real-streaming`, provider `opencode-zen`, and source `dtoken-owned-runtime`.
- d-token persistence wrote events, route decisions, request logs, and CCR metadata.
- Runtime export stayed redacted and reported `raw_available=false`.
- The model did not echo the requested marker exactly, so `response_contains_expected=false`; this does not contradict streaming route success, but it should be improved with a more deterministic streaming assertion in a later test.

Regression verification after the real streaming test:

```text
cargo test -p dtoken-core-api: 15 passed
cargo check -p d-token-desktop: passed
npm run build: passed, Vite chunk-size warning only
```

## Desktop Route Draft Persistence

Scope: Advanced / Routing page now has a first real configuration read/write/apply path for d-token-owned route/provider drafts. This is not a managed apply to OpenCode or external services; it only reads and writes d-token local app state.

Implementation:

- Added Tauri command `save_route_profile_draft`.
- Added Tauri command `list_route_profile_drafts`.
- Added Tauri command `apply_route_profile_draft`.
- Writes local draft JSON under `%LOCALAPPDATA%\d-token\routing\<route>.json`.
- Reads those drafts back into the Advanced / Routing page and displays the latest saved drafts.
- Applies a selected draft to `%LOCALAPPDATA%\d-token\active-config\dtoken-config.json`.
- Writes a d-token rollback manifest under `%LOCALAPPDATA%\d-token\rollback-points\`.
- Generates ProviderProfile, RouteProfile, and InstanceProfile-shaped data.
- Enforces `bind_host=127.0.0.1` and `fallback_provider_id=null`.
- Rejects invalid provider URLs and invalid local ports.
- Sanitizes `api_key_ref`; plaintext-looking `sk-...` values are not persisted.
- Advanced / Routing page now has an OpenCode route draft form for provider name, base URL, model, and local port.

Safety evidence:

- Does not modify OpenCode config.
- Does not modify external Headroom / RTK / cc-switch config.
- Does not start/stop services.
- Does not set system proxy or shell profile.
- Stores only d-token local routing draft state.

Verification:

```text
cargo test -p d-token-desktop: 4 passed
cargo check -p d-token-desktop: passed
npm run build: passed, Vite chunk-size warning only
cargo test -p dtoken-core-api: 15 passed
python support/scripts/check_structure.py: passed
```

## Runtime Active Config Loading

Scope: `dtoken-proxy` now loads the App-applied active config by default, so Advanced / Routing `Apply` can drive real runtime routing without passing `--config`.

Implementation:

- Explicit `--config` remains highest priority.
- `DTOKEN_CONFIG` is the next override.
- Default Windows path is `%LOCALAPPDATA%\d-token\active-config\dtoken-config.json`.
- Default user fallback is `~/.dtoken/active-config/dtoken-config.json`.
- If no active config exists, runtime starts with an empty config as before.

Command:

```powershell
powershell -ExecutionPolicy Bypass -File support/scripts/run_dtoken_active_config_mock_test.ps1
```

Result:

```json
{
  "health": "healthy",
  "content": "active-config-ok",
  "route": "active-config-route",
  "provider": "active-mock-upstream",
  "saved_tokens": 2,
  "dashboard_source": "dtoken-owned-runtime",
  "runtime_config_source": "desktop_active_config",
  "runtime_config_route_count": 1,
  "runtime_config_provider_count": 1,
  "runtime_config_fallback_disabled": true,
  "export_redacted": true,
  "export_active_config_source": "desktop_active_config",
  "export_csv_has_config_status": true
}
```

Interpretation:

- The script wrote a d-token active config under a temporary `LOCALAPPDATA`.
- `dtoken-proxy` was started without `--config`.
- Runtime loaded the active config automatically.
- The request routed to the configured mock upstream and Dashboard recorded the configured route/provider.
- Runtime `/config/active` reported `source=desktop_active_config`, one route, one provider, and `fallback_disabled=true`.
- Dashboard payload now also includes `active_config`, so App pages can compare desktop-applied config with the running runtime config source.
- Redacted `/export/json` now includes active config metadata; `/export/csv` includes a `config_status` row with source, loaded state, fallback state, and path.
- No external Headroom / RTK / cc-switch / OpenCode config was modified.

Verification:

```text
cargo test -p dtoken-proxy: 2 passed
cargo check -p dtoken-proxy: passed
cargo test -p dtoken-core-api: 17 passed
npm run build: passed, Vite chunk-size warning only
cargo test -p d-token-desktop: 4 passed
python support/scripts/check_structure.py: passed
```

## Desktop Active Config Visibility

Scope: the desktop App now exposes the currently applied d-token active route config as first-class UI/runtime state. This is d-token-owned local state only; it does not read or modify OpenCode, external Headroom, RTK, cc-switch, shell profile, or system proxy config.

Implementation:

- Added Tauri command `get_active_route_config_status`.
- `get_dtoken_dashboard_payload` now includes `desktop_active_config` in both local-service and local-empty-fallback modes.
- Advanced / Routing shows an "Active d-token config" card with config path, route, provider, base URL, provider/route counts, and fallback-disabled status.
- Doctor / Runtime reads the same active config status during Run All and reports route/provider/fallback state beside the runtime payload check.
- Active config status reports only metadata and rejects plaintext-looking `sk-...` refs through the existing route draft sanitizer.

Verification:

```text
cargo test -p d-token-desktop: 4 passed
npm run build: passed, Vite chunk-size warning only
cargo test -p dtoken-proxy: 2 passed
cargo test -p dtoken-core-api: 15 passed
python support/scripts/check_structure.py: passed
```

Safety evidence:

- The desktop unit test applies a route draft containing a plaintext-looking `sk-...` value and verifies the active status does not contain it.
- The active config status reports `state=loaded`, route id, provider id, and `fallback_disabled=true`.
- No external service was started, stopped, restarted, reconfigured, or used as a product data source for this change.
