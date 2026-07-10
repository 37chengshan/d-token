param(
  [string]$OutputDir = ""
)

$ErrorActionPreference = "Stop"
$Root = Resolve-Path (Join-Path $PSScriptRoot "..\..")
$Utf8NoBom = [System.Text.UTF8Encoding]::new($false)
if ([string]::IsNullOrWhiteSpace($OutputDir)) {
  $OutputDir = Join-Path $Root "support\artifacts"
}
New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null

function New-Check {
  param(
    [string]$Id,
    [bool]$Ok,
    [object]$Detail
  )
  [pscustomobject]@{
    id = $Id
    ok = $Ok
    detail = $Detail
  }
}

$WorkflowPath = Join-Path $Root ".github\workflows\macos-release.yml"
$SidecarScriptPath = Join-Path $Root "support\scripts\prepare_desktop_sidecars_macos.sh"
$TauriConfigPath = Join-Path $Root "code\apps\desktop\src-tauri\tauri.conf.json"
$PackageJsonPath = Join-Path $Root "code\apps\desktop\package.json"

$WorkflowText = if (Test-Path -LiteralPath $WorkflowPath) { Get-Content -LiteralPath $WorkflowPath -Raw } else { "" }
$SidecarScriptText = if (Test-Path -LiteralPath $SidecarScriptPath) { Get-Content -LiteralPath $SidecarScriptPath -Raw } else { "" }
$TauriConfigText = if (Test-Path -LiteralPath $TauriConfigPath) { Get-Content -LiteralPath $TauriConfigPath -Raw } else { "" }
$PackageJsonText = if (Test-Path -LiteralPath $PackageJsonPath) { Get-Content -LiteralPath $PackageJsonPath -Raw } else { "" }

$Checks = @(
  New-Check "workflow_exists" (Test-Path -LiteralPath $WorkflowPath) @{ path = $WorkflowPath }
  New-Check "workflow_manual_dispatch_only" (($WorkflowText -match "workflow_dispatch") -and ($WorkflowText -notmatch "(?m)^\s*push:") -and ($WorkflowText -notmatch "(?m)^\s*pull_request:")) @{
    note = "macOS release packaging should be manually triggered until signing/notarization credentials are intentionally configured."
  }
  New-Check "workflow_uses_macos_runner" ($WorkflowText -match "macos-") @{
    path = $WorkflowPath
  }
  New-Check "workflow_builds_frontend_and_tauri" (($WorkflowText -match "npm ci") -and ($WorkflowText -match "npm run build") -and ($WorkflowText -match "tauri build")) @{
    path = $WorkflowPath
  }
  New-Check "workflow_runs_release_quality_gates" (($WorkflowText -match "npm run test:run") -and ($WorkflowText -match "tsc --noEmit") -and ($WorkflowText -match "cargo fmt --all -- --check") -and ($WorkflowText -match "cargo clippy --workspace --all-targets -- -D warnings") -and ($WorkflowText -match "cargo test --workspace")) @{
    path = $WorkflowPath
  }
  New-Check "workflow_records_candidate_provenance" (($WorkflowText -match "Write-ReleaseCandidateStamp") -and ($WorkflowText -match "d-token-release-candidate.json")) @{
    path = $WorkflowPath
  }
  New-Check "workflow_prepares_macos_sidecar" ($WorkflowText -match "prepare_desktop_sidecars_macos.sh") @{
    path = $WorkflowPath
  }
  New-Check "workflow_uploads_artifacts" ($WorkflowText -match "actions/upload-artifact") @{
    path = $WorkflowPath
  }
  New-Check "workflow_has_signing_secret_boundary" (($WorkflowText -match "APPLE_CERTIFICATE") -and ($WorkflowText -match "APPLE_ID") -and ($WorkflowText -match "notarytool")) @{
    note = "Presence only. This audit does not read or require actual secrets."
  }
  New-Check "macos_sidecar_script_exists" (Test-Path -LiteralPath $SidecarScriptPath) @{ path = $SidecarScriptPath }
  New-Check "macos_sidecar_script_rejects_non_macos" ($SidecarScriptText -match "Darwin") @{ path = $SidecarScriptPath }
  New-Check "macos_sidecar_script_targets_apple_darwin" (($SidecarScriptText -match "aarch64-apple-darwin") -and ($SidecarScriptText -match "x86_64-apple-darwin")) @{
    path = $SidecarScriptPath
  }
  New-Check "tauri_external_bin_configured" ($TauriConfigText -match '"externalBin"\s*:\s*\[\s*"binaries/dtoken-proxy"\s*\]') @{
    path = $TauriConfigPath
  }
  New-Check "desktop_package_build_exists" ($PackageJsonText -match '"tauri:build"') @{
    path = $PackageJsonPath
  }
)

$Passed = @($Checks | Where-Object { $_.ok }).Count
$Total = @($Checks).Count
$Report = [pscustomobject]@{
  ok = ($Passed -eq $Total)
  mode = "macos-release-template-audit"
  generated_at = (Get-Date).ToString("o")
  passed = $Passed
  total = $Total
  summary = "macOS release template audit: $Passed/$Total checks passed"
  boundary = "Read-only template audit. It does not build macOS artifacts, sign, notarize, install certificates, or read GitHub secrets."
  checks = $Checks
}

$Stamp = (Get-Date).ToString("yyyyMMdd-HHmmss")
$ReportPath = Join-Path $OutputDir "macos-release-template-audit-$Stamp.json"
$Report | Add-Member -NotePropertyName report_path -NotePropertyValue $ReportPath
[System.IO.File]::WriteAllText($ReportPath, ($Report | ConvertTo-Json -Depth 8), $Utf8NoBom)
$Report | ConvertTo-Json -Depth 8 -Compress

if (-not $Report.ok) {
  exit 1
}
