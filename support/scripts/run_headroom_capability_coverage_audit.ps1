param(
  [string]$ReleaseReport = "",
  [string]$OutputDir = ""
)

$ErrorActionPreference = "Stop"
$Root = Resolve-Path (Join-Path $PSScriptRoot "..\..")
$Utf8NoBom = [System.Text.UTF8Encoding]::new($false)
if ([string]::IsNullOrWhiteSpace($OutputDir)) {
  $OutputDir = Join-Path $Root "support\artifacts"
}
New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null

if ([string]::IsNullOrWhiteSpace($ReleaseReport)) {
  $reports = Get-ChildItem -Path $OutputDir -Filter "release-verification-*.json" -ErrorAction SilentlyContinue |
    Sort-Object LastWriteTime -Descending
  $ReleaseReport = if ($reports.Count -gt 0) { $reports[0].FullName } else { "" }
} elseif (-not [System.IO.Path]::IsPathRooted($ReleaseReport)) {
  $ReleaseReport = Join-Path $Root $ReleaseReport
}

$release = $null
if (-not [string]::IsNullOrWhiteSpace($ReleaseReport) -and (Test-Path $ReleaseReport)) {
  $release = Get-Content -LiteralPath $ReleaseReport -Raw | ConvertFrom-Json
}

function Read-Text($relative) {
  Get-Content -LiteralPath (Join-Path $Root $relative) -Raw
}

$runtime = Read-Text "code\crates\dtoken-core-api\src\lib.rs"
$frontend = @(
  Read-Text "code\apps\desktop\src\app\components\common\CapabilitySummary.tsx"
  Read-Text "code\apps\desktop\src\app\components\pages\Install.tsx"
  Read-Text "code\apps\desktop\src\app\components\pages\Advanced.tsx"
  Read-Text "code\apps\desktop\src\app\components\pages\Doctor.tsx"
  Read-Text "code\apps\desktop\src\app\components\pages\Analytics.tsx"
  Read-Text "code\apps\desktop\src\app\components\pages\LogsSecurity.tsx"
  Read-Text "code\apps\desktop\src\app\components\pages\Compatibility.tsx"
) -join "`n"
$desktop = Read-Text "code\apps\desktop\src-tauri\src\main.rs"
$support = @(
  Read-Text "support\scripts\run_fixture_tests.ps1"
  Read-Text "support\scripts\run_dtoken_routed_mock_test.ps1"
  Read-Text "support\scripts\run_dtoken_agent_route_verification_mock.ps1"
  Read-Text "support\scripts\run_dtoken_product_readiness_command_test.ps1"
  Read-Text "support\scripts\run_dtoken_responses_mock_test.ps1"
  Read-Text "support\scripts\run_dtoken_runtime_surface_mock_test.ps1"
  Read-Text "support\scripts\run_dtoken_anthropic_bridge_mock_test.ps1"
) -join "`n"
$releaseText = if ($release) { $release | ConvertTo-Json -Depth 32 } else { "" }

function HasAny($text, [string[]]$needles) {
  foreach ($needle in $needles) {
    if ($text -like "*$needle*") { return $true }
  }
  return $false
}

function StepOk([string]$id) {
  if (-not $release) { return $false }
  foreach ($step in @($release.steps)) {
    if ($step.id -eq $id -and $step.ok -eq $true) { return $true }
  }
  return $false
}

function New-Capability {
  param(
    [string]$Id,
    [string]$Label,
    [bool]$RuntimeOk,
    [bool]$FrontendOk,
    [bool]$VerificationOk,
    [object]$Evidence
  )
  [pscustomobject]@{
    id = $Id
    label = $Label
    ok = ($RuntimeOk -and $FrontendOk -and $VerificationOk)
    runtime_ok = $RuntimeOk
    frontend_ok = $FrontendOk
    verification_ok = $VerificationOk
    evidence = $Evidence
  }
}

$capabilities = @(
  New-Capability "compression" "Context compression pipeline" `
    (HasAny $runtime @("compress_for_route", "tokens_saved", "transforms_applied")) `
    (HasAny $frontend @("Compression", "compression", "压缩")) `
    ((StepOk "mock_routed_runtime") -or (StepOk "opencode_real_workload")) `
    @{ runtime = "compress_for_route + request logs"; frontend = "CapabilitySummary / Analytics / Overview"; verification = "mock_routed_runtime or opencode_real_workload" }

  New-Capability "ccr" "CCR metadata and retrieval boundary" `
    (HasAny $runtime @("/ccr/summary", "ccr_summary", "ccr_retrieve", "ccr-metadata")) `
    (HasAny $frontend @("CCR metadata", "CCR")) `
    ((StepOk "fixture_runtime") -and (HasAny $releaseText @("ccr_raw_available"))) `
    @{ runtime = "/ccr/summary + /ccr/retrieve"; frontend = "CapabilitySummary / Analytics"; verification = "fixture/runtime reports raw_available=false" }

  New-Capability "memory_learn" "Memory / learn metadata" `
    (HasAny $runtime @("/memory/summary", "/memory/learn", "memory_learn", "memory_summary")) `
    (HasAny $frontend @("Memory / Learn", "memory", "learn")) `
    ((StepOk "fixture_runtime") -and (HasAny $releaseText @("memory_raw_available"))) `
    @{ runtime = "/memory/learn + /memory/summary"; frontend = "CapabilitySummary / Advanced / Analytics"; verification = "fixture/runtime reports raw_available=false" }

  New-Capability "output_shape" "Output shaping / verbosity" `
    (HasAny $runtime @("/output-shape", "output_shape", "verbosity")) `
    (HasAny $frontend @("Output shaping", "输出 shaping", "verbosity")) `
    ((StepOk "fixture_runtime") -and (HasAny $releaseText @("output_shape_raw_available"))) `
    @{ runtime = "/output-shape"; frontend = "CapabilitySummary / Advanced"; verification = "fixture/runtime output-shape raw boundary" }

  New-Capability "integrated_rtk" "Integrated RTK status and savings bridge" `
    (HasAny $runtime @("/rtk/status", "/rtk/gain", "/rtk/discover", "/rtk/session")) `
    (HasAny $frontend @("Integrated RTK", "RTK")) `
    (StepOk "fixture_runtime") `
    @{ runtime = "/rtk/* endpoints"; frontend = "CapabilitySummary / Analytics / Compatibility"; verification = "fixture_runtime" }

  New-Capability "proxy_sdk_mcp" "Proxy / SDK / MCP dashboard primitives" `
    (HasAny $runtime @("/headroom/proxy", "/headroom/mcp", "/headroom/stats", "/headroom/cache-summary")) `
    (HasAny $frontend @("route adapter", "dashboard primitives", "Headroom-derived")) `
    (StepOk "fixture_runtime") `
    @{ runtime = "/headroom/proxy + /headroom/mcp + /headroom/stats"; frontend = "CapabilitySummary / Analytics"; verification = "fixture_runtime" }

  New-Capability "stats_history_metrics" "Stats, history, metrics, latency and route overhead" `
    (HasAny $runtime @("/stats-history", "/metrics", "latency_breakdown", "route_overhead")) `
    (HasAny $frontend @("latency", "route overhead", "Route Overhead", "Analytics")) `
    ((StepOk "fixture_runtime") -and ((StepOk "opencode_real_workload") -or (StepOk "mock_routed_runtime"))) `
    @{ runtime = "/stats + /stats-history + /metrics"; frontend = "Analytics / Overview"; verification = "fixture_runtime + opencode_real_workload" }

  New-Capability "routing_protocols" "Route adapter, OpenAI Responses, Anthropic bridge, cc-switch chain" `
    (HasAny $runtime @("/v1/responses", "/v1/messages", "/chain/status", "/protocol-injection")) `
    (HasAny $frontend @("Compatibility", "protocol", "cc-switch", "route")) `
    ((StepOk "responses_runtime") -and (StepOk "anthropic_bridge_runtime") -and (StepOk "runtime_surface")) `
    @{ runtime = "/v1/responses + /v1/messages + /chain/status"; frontend = "Compatibility / Advanced / Logs"; verification = "responses_runtime + anthropic_bridge_runtime + runtime_surface" }

  New-Capability "doctor_dashboard_logs_export" "Doctor, dashboard, logs, Event Ledger and export" `
    (HasAny $runtime @("/doctor/run", "/dashboard/payload", "/events", "/export/json", "/export/csv")) `
    (HasAny $frontend @("Doctor", "Logs", "Export", "Dashboard")) `
    ((StepOk "product_readiness_command") -and (StepOk "agent_route_config_verification") -and ((StepOk "opencode_real_workload") -or (StepOk "mock_routed_runtime"))) `
    @{ runtime = "/doctor/run + /dashboard/payload + /events + export"; frontend = "Doctor / LogsSecurity / Overview"; verification = "product_readiness_command + route verification + OpenCode" }

  New-Capability "project_policy" "Project compression policy, auto scan, config write" `
    (HasAny $runtime @("/projects/recommend-compression", "project_default_policy", "recommend_compression")) `
    (HasAny $frontend @("recommend_project_policy", "project-policy", "HeadroomScan")) `
    (StepOk "product_readiness_command") `
    @{ runtime = "/projects/recommend-compression"; frontend = "Advanced HeadroomScan"; verification = "product_readiness_command" }

  New-Capability "backup_rollback" "Config diff, backup and rollback" `
    (HasAny $runtime @("/config/patches/plan", "/config/rollback-points")) `
    (HasAny $frontend @("Rollback", "rollback", "backup", "diff")) `
    ((HasAny $desktop @("manage_backup", "rollback_restore_reinstates_previous_dtoken_active_config")) -and (StepOk "desktop_rust_tests")) `
    @{ runtime = "/config/patches/plan + /config/rollback-points"; frontend = "Changes / Install"; verification = "desktop_rust_tests" }
)

$passed = @($capabilities | Where-Object { $_.ok }).Count
$total = @($capabilities).Count
$report = [pscustomobject]@{
  ok = ($passed -eq $total)
  mode = "headroom-capability-coverage"
  passed = $passed
  total = $total
  release_report = $ReleaseReport
  summary = "Headroom-derived capability coverage: $passed/$total covered by runtime, frontend, and verification evidence"
  privacy = "Static/source/report coverage audit only. No raw prompts, raw responses, API keys, cookies, auth files, external Headroom, RTK, or cc-switch services are read."
  external_services = "Does not start, stop, reconfigure, or depend on external Headroom, RTK, or cc-switch services."
  capabilities = $capabilities
}

$stamp = (Get-Date).ToString("yyyyMMdd-HHmmss")
$reportPath = Join-Path $OutputDir "headroom-capability-coverage-$stamp.json"
$report | Add-Member -NotePropertyName report_path -NotePropertyValue $reportPath
[System.IO.File]::WriteAllText($reportPath, ($report | ConvertTo-Json -Depth 12), $Utf8NoBom)
$report | ConvertTo-Json -Depth 10 -Compress

if (-not $report.ok) {
  exit 1
}
