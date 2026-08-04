param(
  [string]$OutputPath = "",
  [string]$ProxyPath = "",
  [switch]$SkipBuild
)

$ErrorActionPreference = "Stop"
$Root = Resolve-Path (Join-Path $PSScriptRoot "..\..")
$Utf8NoBom = [System.Text.UTF8Encoding]::new($false)
if ([string]::IsNullOrWhiteSpace($OutputPath)) {
  $OutputPath = Join-Path $Root "code\target\release\metadata\release-performance-gate.json"
} elseif (-not [System.IO.Path]::IsPathRooted($OutputPath)) {
  $OutputPath = Join-Path $Root $OutputPath
}

. (Join-Path $PSScriptRoot "release_candidate_provenance.ps1")
$Candidate = Get-ReleaseCandidateProvenance -Root $Root -OutputDir (Split-Path -Parent $OutputPath)

$Python = Get-Command "python" -ErrorAction SilentlyContinue
if ($null -eq $Python) {
  $Python = Get-Command "python3" -ErrorAction SilentlyContinue
}
if ($null -eq $Python) {
  throw "Python 3 is required for the release performance gate"
}

if ([string]::IsNullOrWhiteSpace($ProxyPath)) {
  $RunningOnWindows = [System.Runtime.InteropServices.RuntimeInformation]::IsOSPlatform(
    [System.Runtime.InteropServices.OSPlatform]::Windows
  )
  $ExecutableName = if ($RunningOnWindows) { "dtoken-proxy.exe" } else { "dtoken-proxy" }
  $ProxyPath = Join-Path $Root "code\target\release\$ExecutableName"
} elseif (-not [System.IO.Path]::IsPathRooted($ProxyPath)) {
  $ProxyPath = Join-Path $Root $ProxyPath
}
if (-not $SkipBuild) {
  Push-Location (Join-Path $Root "code")
  try {
    cargo build --release -p dtoken-proxy
    if ($LASTEXITCODE -ne 0) { throw "release dtoken-proxy build failed with exit code $LASTEXITCODE" }
  } finally {
    Pop-Location
  }
}
if (-not (Test-Path -LiteralPath $ProxyPath -PathType Leaf)) {
  throw "release dtoken-proxy is missing: $ProxyPath"
}

$DesktopRoot = Join-Path $Root "code\apps\desktop"
$BundleManifest = Join-Path $DesktopRoot "dist\.vite\manifest.json"
if (-not (Test-Path -LiteralPath $BundleManifest -PathType Leaf)) {
  throw "frontend dist manifest is missing; build the exact candidate before running the release performance gate"
}

$TemporaryRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("dtoken-release-performance-" + [Guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Force -Path $TemporaryRoot | Out-Null
$BundleReportPath = Join-Path $TemporaryRoot "bundle-budget.json"
$RuntimeReportPath = Join-Path $TemporaryRoot "runtime-performance.json"
$RuntimeExitCode = 1
try {
  Push-Location $DesktopRoot
  try {
    node scripts/check-bundle-budget.mjs --json-output $BundleReportPath
    $BundleExitCode = $LASTEXITCODE
  } finally {
    Pop-Location
  }
  if (-not (Test-Path -LiteralPath $BundleReportPath -PathType Leaf)) {
    throw "desktop bundle budget did not write its metadata report"
  }

  & $Python.Source (Join-Path $PSScriptRoot "run_runtime_performance_gate.py") `
    --proxy $ProxyPath `
    --candidate-id $Candidate.candidate_id `
    --candidate-revision $Candidate.git_head `
    --output $RuntimeReportPath
  $RuntimeExitCode = $LASTEXITCODE

  if (-not (Test-Path -LiteralPath $RuntimeReportPath -PathType Leaf)) {
    throw "runtime performance gate did not write its metadata report"
  }
  $Bundle = Get-Content -LiteralPath $BundleReportPath -Raw | ConvertFrom-Json
  $Runtime = Get-Content -LiteralPath $RuntimeReportPath -Raw | ConvertFrom-Json
  $FailedChecks = @(
    if ($Bundle.gate.ok -ne $true -or $BundleExitCode -ne 0) { "desktop_bundle_budget" }
    if ($Runtime.gate.ok -ne $true -or $RuntimeExitCode -ne 0) { "runtime_performance" }
    if ([string]$Runtime.candidate.candidate_id -ne [string]$Candidate.candidate_id) { "runtime_candidate_binding" }
    if ([string]$Runtime.candidate.git_head -ne [string]$Candidate.git_head) { "runtime_revision_binding" }
  )
  $Report = [pscustomobject]@{
    schema_version = 1
    scope = "release-performance-gate"
    generated_at = (Get-Date).ToUniversalTime().ToString("o")
    candidate = $Candidate
    bundle = $Bundle
    runtime = $Runtime
    gate = [pscustomobject]@{
      ok = ($FailedChecks.Count -eq 0)
      failed_checks = $FailedChecks
    }
    privacy = [pscustomobject]@{
      metadata_only = $true
      raw_request_stored = $false
      raw_response_stored = $false
      process_output_stored = $false
    }
    platform_boundary = "The same gate runs on macOS and Windows. RSS is dtoken-proxy RSS/working-set only; desktop WebView process-group endurance remains a separate platform acceptance gate."
  }

  $OutputParent = Split-Path -Parent $OutputPath
  New-Item -ItemType Directory -Force -Path $OutputParent | Out-Null
  [System.IO.File]::WriteAllText($OutputPath, ($Report | ConvertTo-Json -Depth 16), $Utf8NoBom)
  $Report | Add-Member -NotePropertyName report_path -NotePropertyValue $OutputPath
  $Report | ConvertTo-Json -Depth 16 -Compress
  if ($FailedChecks.Count -gt 0) { exit 1 }
} finally {
  Remove-Item -LiteralPath $TemporaryRoot -Recurse -Force -ErrorAction SilentlyContinue
}
