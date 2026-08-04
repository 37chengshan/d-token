param(
  [switch]$IncludeOpenCode,
  [switch]$SkipOpenCode,
  [string]$OutputDir = ""
)

$ErrorActionPreference = "Stop"
$Root = Resolve-Path (Join-Path $PSScriptRoot "..\..")
$Utf8NoBom = [System.Text.UTF8Encoding]::new($false)
$VerificationStartedAt = Get-Date
if ([string]::IsNullOrWhiteSpace($OutputDir)) {
  $OutputDir = Join-Path $Root "support\artifacts"
} elseif (-not [System.IO.Path]::IsPathRooted($OutputDir)) {
  $OutputDir = Join-Path $Root $OutputDir
}
New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null

if ($IncludeOpenCode -and $SkipOpenCode) {
  throw "-IncludeOpenCode and -SkipOpenCode cannot be used together. The default release verification is no-touch; use only -IncludeOpenCode for the separately authorized real-agent gate."
}

. (Join-Path $PSScriptRoot "release_candidate_provenance.ps1")
$Candidate = Get-ReleaseCandidateProvenance -Root $Root -OutputDir $OutputDir
$CapabilityArtifactStamp = $VerificationStartedAt.ToString("yyyyMMdd-HHmmssfff")
$CapabilityRegistryPath = Join-Path $OutputDir ("capability-registry-" + $Candidate.candidate_id + "-" + $CapabilityArtifactStamp + ".json")
$CapabilitySnapshotPath = Join-Path $OutputDir ("headroom-capability-snapshot-" + $Candidate.candidate_id + "-" + $CapabilityArtifactStamp + ".json")
$Candidate | Add-Member -NotePropertyName capability_snapshot_path -NotePropertyValue $CapabilitySnapshotPath -Force

function Invoke-VerificationStep {
  param(
    [string]$Id,
    [string]$Description,
    [scriptblock]$Command
  )

  $Started = Get-Date
  try {
    $global:LASTEXITCODE = 0
    $PreviousErrorActionPreference = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    try {
      $Raw = @(& $Command 2>&1)
      $ExitCode = $global:LASTEXITCODE
    } finally {
      $ErrorActionPreference = $PreviousErrorActionPreference
    }
    $ElapsedMs = [int]((Get-Date) - $Started).TotalMilliseconds
    if ($ExitCode -ne 0) {
      $Tail = @($Raw | ForEach-Object { $_.ToString() } | Select-Object -Last 30)
      throw "step '$Id' exited with code $ExitCode. Output tail: $($Tail -join [Environment]::NewLine)"
    }
    $Parsed = $null
    $JsonLine = ($Raw | ForEach-Object { $_.ToString() } | Where-Object { $_.Trim().StartsWith("{") } | Select-Object -Last 1)
    if ($JsonLine) {
      try { $Parsed = $JsonLine | ConvertFrom-Json } catch { $Parsed = $null }
    }
    return [pscustomobject]@{
      id = $Id
      description = $Description
      ok = $true
      elapsed_ms = $ElapsedMs
      summary = if ($Parsed) { $Parsed } else { @{ output_line_count = @($Raw).Count } }
    }
  } catch {
    $ElapsedMs = [int]((Get-Date) - $Started).TotalMilliseconds
    return [pscustomobject]@{
      id = $Id
      description = $Description
      ok = $false
      elapsed_ms = $ElapsedMs
      error = $_.Exception.Message
    }
  }
}

function Assert-OpenCodeSummary {
  param(
    [object]$Summary,
    [object]$Candidate
  )

  if ($null -eq $Summary) {
    throw "opencode real workload summary is missing"
  }

  foreach ($Field in @("candidate_id", "runtime_identity", "service_url", "route", "provider", "report_path")) {
    if ([string]::IsNullOrWhiteSpace([string]$Summary.$Field)) {
      throw "opencode real workload summary is missing required field: $Field"
    }
  }

  if ([string]$Summary.candidate_id -ne [string]$Candidate.candidate_id) {
    throw "opencode real workload candidate mismatch"
  }
  if ([string]$Summary.runtime_identity -ne "dtoken-runtime-local") {
    throw "opencode real workload runtime identity mismatch"
  }
  if ([string]$Summary.service_url -notmatch '^http://127\.0\.0\.1:\d+$') {
    throw "opencode real workload service URL must stay on loopback"
  }
  if ($Summary.route_decision_count -lt 1 -or $Summary.request_log_count -lt 1) {
    throw "opencode real workload must record route and request logs"
  }
  if ($Summary.ledger.events -lt 1 -or $Summary.ledger.route_decisions -lt 1 -or $Summary.ledger.request_logs -lt 1 -or $Summary.ledger.ccr_metadata -lt 1) {
    throw "opencode real workload ledger evidence is incomplete"
  }
  if ($null -eq $Summary.metrics -or $null -eq $Summary.metrics.compression -or $null -eq $Summary.metrics.route) {
    throw "opencode real workload metrics are incomplete"
  }
  if ($null -eq $Summary.ccr -or $Summary.ccr.entries -lt 1 -or $Summary.ccr.raw_available -ne $false) {
    throw "opencode real workload CCR evidence is incomplete"
  }
  if ($Summary.export_redacted -ne $true -or $Summary.export_raw_available -ne $false) {
    throw "opencode real workload export redaction mismatch"
  }
  if (-not (Test-Path -LiteralPath ([string]$Summary.report_path) -PathType Leaf)) {
    throw "opencode real workload report path is missing: $($Summary.report_path)"
  }
}

function Invoke-RepositoryPython {
  param([string[]]$ArgumentList)

  $Python = Get-Command "python3" -ErrorAction SilentlyContinue
  if ($null -eq $Python) {
    $Python = Get-Command "python" -ErrorAction SilentlyContinue
  }
  if ($null -eq $Python) {
    throw "Python 3 is required for candidate capability snapshot validation"
  }

  $PythonMajor = (& $Python.Source -c "import sys; print(sys.version_info[0])").Trim()
  if ($LASTEXITCODE -ne 0 -or $PythonMajor -ne "3") {
    throw "Python 3 is required for candidate capability snapshot validation"
  }

  & $Python.Source @ArgumentList
  if ($LASTEXITCODE -ne 0) {
    throw "Python command failed with exit code $LASTEXITCODE"
  }
}

$Steps = @()

$Steps += Invoke-VerificationStep "release_gate_contract" "Static regression contract for CI, no-touch verification, provenance, and artifact freshness" {
  powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $Root "support\scripts\run_release_gate_contract_test.ps1")
}

$Steps += Invoke-VerificationStep "release_candidate_provenance" "Dynamic rejection tests for stale candidate stamps and pre-candidate artifacts" {
  powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $Root "support\scripts\run_release_candidate_provenance_test.ps1")
}

$Steps += Invoke-VerificationStep "headroom_capability_contract" "Static and fixture contracts for candidate-bound native capability evidence" {
  Invoke-RepositoryPython -ArgumentList @((Join-Path $Root "support\scripts\test_generate_headroom_capability_snapshot.py"))
  Invoke-RepositoryPython -ArgumentList @((Join-Path $Root "support\scripts\test_headroom_capability_coverage_audit_contract.py"))
}

$Steps += Invoke-VerificationStep "frontend_dependencies" "Install the exact desktop frontend dependency lock" {
  Push-Location (Join-Path $Root "code\apps\desktop")
  try { npm ci } finally { Pop-Location }
}

$Steps += Invoke-VerificationStep "frontend_vitest" "Desktop frontend Vitest regression suite" {
  Push-Location (Join-Path $Root "code\apps\desktop")
  try { npm run test:run } finally { Pop-Location }
}

$Steps += Invoke-VerificationStep "frontend_typecheck" "Desktop frontend TypeScript no-emit type check" {
  Push-Location (Join-Path $Root "code\apps\desktop")
  try { npx tsc --noEmit } finally { Pop-Location }
}

$Steps += Invoke-VerificationStep "desktop_build" "Vite desktop build with page-level chunks" {
  Push-Location (Join-Path $Root "code\apps\desktop")
  try { npm run build } finally { Pop-Location }
}

$Steps += Invoke-VerificationStep "release_performance_gate" "Candidate-bound bundle budget plus release-profile routed concurrency, short soak, failure, disconnect, latency, and RSS gate" {
  powershell -NoProfile -ExecutionPolicy Bypass `
    -File (Join-Path $Root "support\scripts\run_release_performance_gate.ps1") `
    -OutputPath (Join-Path $OutputDir "release-performance-gate.json")
}

$Steps += Invoke-VerificationStep "rust_fmt" "Rust formatting check across the workspace" {
  Push-Location (Join-Path $Root "code")
  try { cargo fmt --all -- --check } finally { Pop-Location }
}

$Steps += Invoke-VerificationStep "rust_clippy" "Rust Clippy across the workspace and every target with warnings denied" {
  Push-Location (Join-Path $Root "code")
  try { cargo clippy --workspace --all-targets -- -D warnings } finally { Pop-Location }
}

$Steps += Invoke-VerificationStep "rust_workspace_tests" "All Rust workspace tests including desktop commands and runtime crates" {
  Push-Location (Join-Path $Root "code")
  try { cargo test --workspace } finally { Pop-Location }
}

$Steps += Invoke-VerificationStep "codec_candidate_evidence" "Candidate-bound active codec corpus, property, semantic, negative, cancellation, rollback, and latency evidence" {
  powershell -NoProfile -ExecutionPolicy Bypass `
    -File (Join-Path $Root "support\scripts\run_codec_candidate_evidence.ps1") `
    -OutputPath (Join-Path $OutputDir "production-codec-candidate-evidence.json")
}

$Steps += Invoke-VerificationStep "fixture_runtime" "No-touch fixture runtime, Doctor, read-only Headroom/RTK diagnostics, dashboard payload" {
  powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $Root "support\scripts\run_fixture_tests.ps1") -Port 38101
}

$Steps += Invoke-VerificationStep "mock_routed_runtime" "d-token routed mock upstream with compression, CCR, memory, persistence" {
  powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $Root "support\scripts\run_dtoken_routed_mock_test.ps1")
}

$Steps += Invoke-VerificationStep "active_config_runtime" "d-token active config load, route, export redaction" {
  powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $Root "support\scripts\run_dtoken_active_config_mock_test.ps1")
}

$Steps += Invoke-VerificationStep "candidate_capability_registry" "Native /capabilities evidence from the already-built candidate runtime on loopback" {
  powershell -NoProfile -ExecutionPolicy Bypass `
    -File (Join-Path $Root "support\scripts\run_candidate_capability_registry_snapshot.ps1") `
    -CandidateId $Candidate.candidate_id `
    -OutputDir $OutputDir `
    -RegistryPath $CapabilityRegistryPath
}

$Steps += Invoke-VerificationStep "candidate_capability_snapshot" "Strict candidate wrapper generated from the native capability registry artifact" {
  Invoke-RepositoryPython -ArgumentList @(
    (Join-Path $Root "support\scripts\generate_headroom_capability_snapshot.py"),
    "--registry", $CapabilityRegistryPath,
    "--candidate-id", $Candidate.candidate_id,
    "--output", $CapabilitySnapshotPath
  )
}

$Steps += Invoke-VerificationStep "agent_route_config_verification" "Product route config verification through d-token-owned runtime" {
  powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $Root "support\scripts\run_dtoken_agent_route_verification_mock.ps1")
}

$Steps += Invoke-VerificationStep "product_readiness_command" "App product readiness command with d-token runtime metadata" {
  powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $Root "support\scripts\run_dtoken_product_readiness_command_test.ps1") -OutputDir $OutputDir
}

$Steps += Invoke-VerificationStep "service_lifecycle" "Identity-checked d-token managed service lifecycle" {
  powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $Root "support\scripts\run_dtoken_service_lifecycle_test.ps1") -OutputDir $OutputDir
}

$Steps += Invoke-VerificationStep "responses_runtime" "OpenAI Responses-compatible route, provider-forward ledger, export redaction" {
  powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $Root "support\scripts\run_dtoken_responses_mock_test.ps1")
}

$Steps += Invoke-VerificationStep "runtime_surface" "Compatibility, protocol injection, chain status, and config-patch ledger surface" {
  powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $Root "support\scripts\run_dtoken_runtime_surface_mock_test.ps1")
}

$Steps += Invoke-VerificationStep "anthropic_bridge_runtime" "Anthropic-shaped route bridge through d-token-owned runtime" {
  powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $Root "support\scripts\run_dtoken_anthropic_bridge_mock_test.ps1")
}

if ($IncludeOpenCode) {
  $Steps += Invoke-VerificationStep "opencode_real_workload" "Real OpenCode through temporary d-token-owned service" {
    powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $Root "support\scripts\run_opencode_workload_benchmark.ps1") -OutputDir $OutputDir
  }
}

$Steps += Invoke-VerificationStep "headroom_capability_coverage" "Candidate-bound Headroom active coverage from native runtime capability evidence" {
  powershell -NoProfile -ExecutionPolicy Bypass `
    -File (Join-Path $Root "support\scripts\run_headroom_capability_coverage_audit.ps1") `
    -CapabilitySnapshot $CapabilitySnapshotPath `
    -ExpectedCandidateId $Candidate.candidate_id `
    -OutputDir $OutputDir
}

$Steps += Invoke-VerificationStep "desktop_packaging_audit" "Desktop packaging configuration audit" {
  powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $Root "support\scripts\run_desktop_packaging_audit.ps1") -OutputDir $OutputDir
}

$Steps += Invoke-VerificationStep "desktop_bundle_artifact_audit" "Desktop installer artifact path, size, and hash audit" {
  powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $Root "support\scripts\run_desktop_bundle_artifact_audit.ps1") -OutputDir $OutputDir
}

$Steps += Invoke-VerificationStep "release_distribution_audit" "Release candidate manifest, installed-file dependency, and signing-status audit" {
  powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $Root "support\scripts\run_release_distribution_audit.ps1") -OutputDir $OutputDir
}

$Steps += Invoke-VerificationStep "windows_signing_readiness" "Read-only Windows signing readiness audit for public distribution" {
  powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $Root "support\scripts\run_windows_signing_readiness_audit.ps1") -OutputDir $OutputDir
}

$Steps += Invoke-VerificationStep "cross_platform_release_readiness" "Read-only Windows/macOS release readiness boundary audit" {
  powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $Root "support\scripts\run_cross_platform_release_readiness_audit.ps1") -OutputDir $OutputDir
}

$Steps += Invoke-VerificationStep "macos_release_template_audit" "Read-only macOS release workflow and sidecar template audit" {
  powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $Root "support\scripts\run_macos_release_template_audit.ps1") -OutputDir $OutputDir
}

$Steps += Invoke-VerificationStep "release_artifact_redaction_audit" "Release artifact redaction audit for raw output, prompts, responses, and credential-shaped values" {
  powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $Root "support\scripts\run_release_artifact_redaction_audit.ps1") -OutputDir $OutputDir
}

$Steps += Invoke-VerificationStep "release_cleanup_audit" "Release cleanup audit for residual validation processes and listening test ports" {
  powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $Root "support\scripts\run_release_cleanup_audit.ps1") -OutputDir $OutputDir
}

$Ok = ($Steps | Where-Object { -not $_.ok }).Count -eq 0
$LatestOpenCode = $Steps | Where-Object { $_.id -eq "opencode_real_workload" } | Select-Object -First 1
$OpenCodeSummary = $null
if ($LatestOpenCode -and $LatestOpenCode.ok -and $LatestOpenCode.summary) {
  Assert-OpenCodeSummary -Summary $LatestOpenCode.summary -Candidate $Candidate
  $OpenCodeSummary = @{
    candidate_id = $LatestOpenCode.summary.candidate_id
    runtime_identity = $LatestOpenCode.summary.runtime_identity
    service_url = $LatestOpenCode.summary.service_url
    route = $LatestOpenCode.summary.route
    provider = $LatestOpenCode.summary.provider
    route_decision_count = $LatestOpenCode.summary.route_decision_count
    request_log_count = $LatestOpenCode.summary.request_log_count
    workload_count = $LatestOpenCode.summary.workload_count
    total_saved_tokens = $LatestOpenCode.summary.total_saved_tokens
    savings_percent = $LatestOpenCode.summary.savings_percent
    fallback_disabled = $LatestOpenCode.summary.runtime_config_fallback_disabled
    export_redacted = $LatestOpenCode.summary.export_redacted
    export_raw_available = $LatestOpenCode.summary.export_raw_available
    ledger = $LatestOpenCode.summary.ledger
    metrics = $LatestOpenCode.summary.metrics
    ccr = $LatestOpenCode.summary.ccr
    report_path = $LatestOpenCode.summary.report_path
  }
}

$Report = [pscustomobject]@{
  ok = $Ok
  started_at = $VerificationStartedAt.ToString("o")
  generated_at = (Get-Date).ToString("o")
  scope = "release-verification"
  gate_mode = if ($IncludeOpenCode) { "no-touch-plus-authorized-opencode" } else { "no-touch" }
  candidate = $Candidate
  privacy = "No raw prompts, raw responses, API keys, browser session credentials, or auth files are stored in this report."
  external_services = if ($IncludeOpenCode) {
    "Does not start/stop/reconfigure external Headroom, RTK, or cc-switch. This explicitly opted-in run includes only the user-authorized local OpenCode workload."
  } else {
    "No-touch default: does not invoke OpenCode and does not start/stop/reconfigure external Headroom, RTK, cc-switch, agent configuration, or agent auth state."
  }
  steps = $Steps
  opencode_summary = $OpenCodeSummary
  handoff_manifest = $null
}

$Stamp = (Get-Date).ToString("yyyyMMdd-HHmmss")
$ReportPath = Join-Path $OutputDir "release-verification-$Stamp.json"
$Report | Add-Member -NotePropertyName report_path -NotePropertyValue $ReportPath
[System.IO.File]::WriteAllText($ReportPath, ($Report | ConvertTo-Json -Depth 16), $Utf8NoBom)

if ($Ok) {
  $HandoffStep = Invoke-VerificationStep "release_handoff_manifest" "Release handoff manifest bound to this exact Git/worktree candidate" {
    powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $Root "support\scripts\run_release_handoff_manifest.ps1") -OutputDir $OutputDir -ReleaseReportPath $ReportPath -ExpectedCandidateId $Candidate.candidate_id
  }
  $Steps += $HandoffStep
  $Ok = ($Steps | Where-Object { -not $_.ok }).Count -eq 0
  $Report.ok = $Ok
  $Report.generated_at = (Get-Date).ToString("o")
  $Report.steps = $Steps
  if ($HandoffStep.ok) {
    $Report.handoff_manifest = $HandoffStep.summary
  }
  [System.IO.File]::WriteAllText($ReportPath, ($Report | ConvertTo-Json -Depth 16), $Utf8NoBom)
}

$Report | ConvertTo-Json -Depth 12 -Compress

if (-not $Ok) {
  exit 1
}
