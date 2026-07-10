param(
  [string]$OutputDir = "",
  [string]$ReleaseReportPath = "",
  [string]$ExpectedCandidateId = ""
)

$ErrorActionPreference = "Stop"
$Root = Resolve-Path (Join-Path $PSScriptRoot "..\..")
$Utf8NoBom = [System.Text.UTF8Encoding]::new($false)
if ([string]::IsNullOrWhiteSpace($OutputDir)) {
  $OutputDir = Join-Path $Root "support\artifacts"
}
New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null

. (Join-Path $PSScriptRoot "release_candidate_provenance.ps1")
$CurrentCandidate = Get-ReleaseCandidateProvenance -Root $Root -OutputDir $OutputDir
if ([string]::IsNullOrWhiteSpace($ExpectedCandidateId)) {
  $ExpectedCandidateId = $CurrentCandidate.candidate_id
}

function Get-LatestJson {
  param([string]$Pattern)
  $Files = @(Get-ChildItem -LiteralPath $OutputDir -Filter $Pattern -File -ErrorAction SilentlyContinue | Sort-Object LastWriteTime -Descending)
  if ($Files.Count -eq 0) { return $null }
  return $Files[0]
}

function Read-JsonFile {
  param([object]$File)
  if ($null -eq $File) { return $null }
  try {
    return Get-Content -LiteralPath $File.FullName -Raw | ConvertFrom-Json
  } catch {
    return $null
  }
}

function Get-ReleaseStepSummary {
  param(
    [object]$ReleaseReport,
    [string]$Id
  )
  if ($null -eq $ReleaseReport) { return $null }
  $Step = @($ReleaseReport.steps | Where-Object { $_.id -eq $Id } | Select-Object -First 1)
  if ($Step.Count -eq 0 -or $Step[0].ok -ne $true) { return $null }
  return $Step[0].summary
}

function Get-ReportFileFromSummary {
  param(
    [object]$Summary,
    [object]$Fallback
  )
  if ($null -eq $Summary -or [string]::IsNullOrWhiteSpace([string]$Summary.report_path)) {
    return $Fallback
  }
  $Path = [string]$Summary.report_path
  if (-not [System.IO.Path]::IsPathRooted($Path)) {
    $Path = Join-Path $Root $Path
  }
  if (Test-Path -LiteralPath $Path -PathType Leaf) {
    return Get-Item -LiteralPath $Path
  }
  return $Fallback
}

function Get-Artifact {
  param(
    [string]$Id,
    [string]$Path,
    [bool]$Required = $true
  )
  if ([string]::IsNullOrWhiteSpace($Path) -or -not (Test-Path -LiteralPath $Path)) {
    return [pscustomobject]@{
      id = $Id
      required = $Required
      exists = $false
      path = $Path
      bytes = 0
      sha256 = $null
      authenticode_status = "Missing"
      signer_subject = $null
      last_write_time = $null
    }
  }
  $Item = Get-Item -LiteralPath $Path
  $Sig = Get-AuthenticodeSignature -LiteralPath $Item.FullName
  [pscustomobject]@{
    id = $Id
    required = $Required
    exists = $true
    path = $Item.FullName
    bytes = $Item.Length
    sha256 = (Get-FileHash -LiteralPath $Item.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
    authenticode_status = [string]$Sig.Status
    signer_subject = if ($Sig.SignerCertificate) { $Sig.SignerCertificate.Subject } else { $null }
    last_write_time = $Item.LastWriteTime.ToString("o")
  }
}

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

$ReleaseDir = Join-Path $Root "code\target\release"
$InstallerFiles = @(Get-ChildItem -Path (Join-Path $ReleaseDir "bundle\nsis\d-token_*_x64-setup.exe") -File -ErrorAction SilentlyContinue | Sort-Object LastWriteTime -Descending)
$InstallerPath = if ($InstallerFiles.Count -gt 0) { $InstallerFiles[0].FullName } else { $null }

$Artifacts = @(
  Get-Artifact "windows_app_exe" (Join-Path $ReleaseDir "d-token-desktop.exe")
  Get-Artifact "windows_runtime_sidecar" (Join-Path $ReleaseDir "dtoken-proxy.exe")
  Get-Artifact "windows_webview2_loader" (Join-Path $ReleaseDir "WebView2Loader.dll")
  Get-Artifact "windows_nsis_installer" $InstallerPath
)

$LatestRelease = if (-not [string]::IsNullOrWhiteSpace($ReleaseReportPath)) {
  if (-not (Test-Path -LiteralPath $ReleaseReportPath -PathType Leaf)) {
    throw "explicit release verification report was not found: $ReleaseReportPath"
  }
  Get-Item -LiteralPath $ReleaseReportPath
} else {
  Get-LatestJson "release-verification-*.json"
}
$LatestOpenCode = Get-LatestJson "opencode-workload-benchmark-*.json"
$LatestDistribution = Get-LatestJson "release-distribution-manifest-*.json"
$LatestSigning = Get-LatestJson "windows-signing-readiness-*.json"
$LatestCrossPlatform = Get-LatestJson "cross-platform-release-readiness-*.json"
$LatestMacTemplate = Get-LatestJson "macos-release-template-audit-*.json"
$LatestRedaction = Get-LatestJson "release-artifact-redaction-audit-*.json"
$LatestCleanup = Get-LatestJson "release-cleanup-audit-*.json"
$LatestStartup = Get-LatestJson "installed-desktop-startup-audit-*.json"

$ReleaseReport = Read-JsonFile $LatestRelease
$OpenCodeReport = Read-JsonFile $LatestOpenCode
$DistributionReport = Read-JsonFile $LatestDistribution
$SigningReport = Read-JsonFile $LatestSigning
$CrossPlatformReport = Read-JsonFile $LatestCrossPlatform
$MacTemplateReport = Read-JsonFile $LatestMacTemplate
$RedactionReport = Read-JsonFile $LatestRedaction
$CleanupReport = Read-JsonFile $LatestCleanup
$StartupReport = Read-JsonFile $LatestStartup

# Bind every release-readiness subreport to the explicit verification report
# when available. Falling back to "latest" is retained only for standalone
# handoff diagnostics and cannot satisfy the candidate-bound release check.
$DistributionStepReport = Get-ReleaseStepSummary -ReleaseReport $ReleaseReport -Id "release_distribution_audit"
$SigningStepReport = Get-ReleaseStepSummary -ReleaseReport $ReleaseReport -Id "windows_signing_readiness"
$CrossPlatformStepReport = Get-ReleaseStepSummary -ReleaseReport $ReleaseReport -Id "cross_platform_release_readiness"
$MacTemplateStepReport = Get-ReleaseStepSummary -ReleaseReport $ReleaseReport -Id "macos_release_template_audit"
$RedactionStepReport = Get-ReleaseStepSummary -ReleaseReport $ReleaseReport -Id "release_artifact_redaction_audit"
$CleanupStepReport = Get-ReleaseStepSummary -ReleaseReport $ReleaseReport -Id "release_cleanup_audit"
if ($null -ne $DistributionStepReport) { $DistributionReport = $DistributionStepReport; $LatestDistribution = Get-ReportFileFromSummary $DistributionStepReport $LatestDistribution }
if ($null -ne $SigningStepReport) { $SigningReport = $SigningStepReport; $LatestSigning = Get-ReportFileFromSummary $SigningStepReport $LatestSigning }
if ($null -ne $CrossPlatformStepReport) { $CrossPlatformReport = $CrossPlatformStepReport; $LatestCrossPlatform = Get-ReportFileFromSummary $CrossPlatformStepReport $LatestCrossPlatform }
if ($null -ne $MacTemplateStepReport) { $MacTemplateReport = $MacTemplateStepReport; $LatestMacTemplate = Get-ReportFileFromSummary $MacTemplateStepReport $LatestMacTemplate }
if ($null -ne $RedactionStepReport) { $RedactionReport = $RedactionStepReport; $LatestRedaction = Get-ReportFileFromSummary $RedactionStepReport $LatestRedaction }
if ($null -ne $CleanupStepReport) { $CleanupReport = $CleanupStepReport; $LatestCleanup = Get-ReportFileFromSummary $CleanupStepReport $LatestCleanup }

$RequiredArtifactsExist = (@($Artifacts | Where-Object { $_.required -and -not $_.exists }).Count -eq 0)
$HashesRecorded = (@($Artifacts | Where-Object { $_.required -and [string]::IsNullOrWhiteSpace($_.sha256) }).Count -eq 0)
$Installer = @($Artifacts | Where-Object { $_.id -eq "windows_nsis_installer" } | Select-Object -First 1)[0]
$InstallerUsable = ($Installer.exists -and $Installer.bytes -gt 1000000)
$ReleaseReportCandidateId = if ($null -ne $ReleaseReport -and $null -ne $ReleaseReport.candidate) { [string]$ReleaseReport.candidate.candidate_id } else { "" }
$CandidateIdMatches = (
  -not [string]::IsNullOrWhiteSpace($ReleaseReportCandidateId) -and
  $ReleaseReportCandidateId -eq $ExpectedCandidateId -and
  $ReleaseReportCandidateId -eq $CurrentCandidate.candidate_id
)
$RequiredReleaseGateSteps = @(
  "release_gate_contract",
  "release_candidate_provenance",
  "frontend_dependencies",
  "frontend_vitest",
  "frontend_typecheck",
  "desktop_build",
  "rust_fmt",
  "rust_clippy",
  "rust_workspace_tests",
  "desktop_bundle_artifact_audit",
  "release_distribution_audit",
  "release_artifact_redaction_audit",
  "release_cleanup_audit"
)
$MissingOrFailedReleaseGateSteps = @()
foreach ($StepId in $RequiredReleaseGateSteps) {
  $Step = @($ReleaseReport.steps | Where-Object { $_.id -eq $StepId } | Select-Object -First 1)
  if ($Step.Count -eq 0 -or $Step[0].ok -ne $true) {
    $MissingOrFailedReleaseGateSteps += $StepId
  }
}
$ReleaseGateCoverageOk = ($MissingOrFailedReleaseGateSteps.Count -eq 0)
$ReleaseGateOk = ($null -ne $ReleaseReport -and $ReleaseReport.ok -eq $true -and $CandidateIdMatches -and $ReleaseGateCoverageOk)
$DistributionReadyLocal = ($CandidateIdMatches -and $null -ne $DistributionReport -and $DistributionReport.release_candidate_ready -eq $true)
$DistributionReadyPublic = ($CandidateIdMatches -and $null -ne $DistributionReport -and $DistributionReport.distribution_ready_public -eq $true)
$SigningCanRun = ($CandidateIdMatches -and $null -ne $SigningReport -and $SigningReport.can_sign_now -eq $true)
$RedactionOk = ($CandidateIdMatches -and $null -ne $RedactionReport -and $RedactionReport.ok -eq $true)
$CleanupOk = ($CandidateIdMatches -and $null -ne $CleanupReport -and $CleanupReport.ok -eq $true)
$StartupCandidateId = if ($null -ne $StartupReport -and $null -ne $StartupReport.candidate) { [string]$StartupReport.candidate.candidate_id } else { "" }
$StartupCandidateMatches = (-not [string]::IsNullOrWhiteSpace($StartupCandidateId) -and $StartupCandidateId -eq $CurrentCandidate.candidate_id)
$StartupOk = ($null -ne $StartupReport -and $StartupReport.ok -eq $true -and $StartupCandidateMatches)
$CrossPlatformRecorded = ($CandidateIdMatches -and $null -ne $CrossPlatformReport -and $CrossPlatformReport.ok -eq $true)
$CrossPlatformLocalReady = ($CandidateIdMatches -and $null -ne $CrossPlatformReport -and $CrossPlatformReport.cross_platform_local_release_candidate_ready -eq $true)
$MacTemplateReady = ($CandidateIdMatches -and $null -ne $MacTemplateReport -and $MacTemplateReport.ok -eq $true)

$Checks = @(
  New-Check "required_artifacts_exist" $RequiredArtifactsExist $Artifacts
  New-Check "artifact_hashes_recorded" $HashesRecorded ($Artifacts | Select-Object id, sha256)
  New-Check "installer_usable_size" $InstallerUsable @{ bytes = $Installer.bytes; min_bytes = 1000000 }
  New-Check "candidate_id_matches" $CandidateIdMatches @{
    expected_candidate_id = $ExpectedCandidateId
    current_candidate_id = $CurrentCandidate.candidate_id
    release_report_candidate_id = $ReleaseReportCandidateId
    path = if ($LatestRelease) { $LatestRelease.FullName } else { $null }
  }
  New-Check "release_gate_required_steps_passed" $ReleaseGateCoverageOk @{
    required = $RequiredReleaseGateSteps
    missing_or_failed = $MissingOrFailedReleaseGateSteps
  }
  New-Check "latest_release_gate_passed" $ReleaseGateOk @{ path = if ($LatestRelease) { $LatestRelease.FullName } else { $null } }
  New-Check "release_distribution_local_candidate" $DistributionReadyLocal @{ path = if ($LatestDistribution) { $LatestDistribution.FullName } else { $null }; public = $DistributionReadyPublic }
  New-Check "windows_signing_readiness_recorded" ($null -ne $SigningReport) @{ path = if ($LatestSigning) { $LatestSigning.FullName } else { $null }; can_sign_now = $SigningCanRun }
  New-Check "cross_platform_release_readiness_recorded" $CrossPlatformRecorded @{ path = if ($LatestCrossPlatform) { $LatestCrossPlatform.FullName } else { $null }; cross_platform_local_release_candidate_ready = $CrossPlatformLocalReady }
  New-Check "macos_release_template_ready" $MacTemplateReady @{ path = if ($LatestMacTemplate) { $LatestMacTemplate.FullName } else { $null } }
  New-Check "artifact_redaction_passed" $RedactionOk @{ path = if ($LatestRedaction) { $LatestRedaction.FullName } else { $null } }
  New-Check "cleanup_passed" $CleanupOk @{ path = if ($LatestCleanup) { $LatestCleanup.FullName } else { $null } }
  New-Check "installed_startup_audit_passed_for_candidate" $StartupOk @{
    path = if ($LatestStartup) { $LatestStartup.FullName } else { $null }
    report_candidate_id = $StartupCandidateId
    current_candidate_id = $CurrentCandidate.candidate_id
  }
)

$LocalReleaseCandidateReady = (
  $RequiredArtifactsExist -and
  $HashesRecorded -and
  $InstallerUsable -and
  $ReleaseGateOk -and
  $DistributionReadyLocal -and
  $RedactionOk -and
  $CleanupOk -and
  $StartupOk
)

$Report = [pscustomobject]@{
  ok = $LocalReleaseCandidateReady
  mode = "release-handoff-manifest"
  generated_at = (Get-Date).ToString("o")
  product = "d-token"
  version = "0.2.0"
  candidate = $CurrentCandidate
  local_release_candidate_ready = $LocalReleaseCandidateReady
  public_distribution_ready = ($LocalReleaseCandidateReady -and $DistributionReadyPublic -and $SigningCanRun)
  cross_platform_local_release_candidate_ready = $CrossPlatformLocalReady
  public_distribution_blockers = @(
    if (-not $CrossPlatformLocalReady) { "macOS release artifact/signing readiness is not complete on this host" }
    if (-not $DistributionReadyPublic) { "app/runtime/installer Authenticode signatures are not valid for public distribution" }
    if (-not $SigningCanRun) { "Windows signing toolchain/certificate is not ready on this machine" }
  )
  no_touch_boundary = "Does not start, stop, or reconfigure external Headroom, RTK, cc-switch, OpenCode, shell profiles, or agent auth files."
  privacy = "This manifest stores artifact hashes, report paths, and release status only; no raw prompts, raw responses, API keys, cookies, bearer tokens, or certificate passwords."
  artifacts = $Artifacts
  evidence = [pscustomobject]@{
    latest_no_touch_release = if ($LatestRelease) { $LatestRelease.FullName } else { $null }
    latest_full_opencode = if ($LatestOpenCode) { $LatestOpenCode.FullName } else { $null }
    latest_distribution_manifest = if ($LatestDistribution) { $LatestDistribution.FullName } else { $null }
    latest_windows_signing_readiness = if ($LatestSigning) { $LatestSigning.FullName } else { $null }
    latest_cross_platform_release_readiness = if ($LatestCrossPlatform) { $LatestCrossPlatform.FullName } else { $null }
    latest_macos_release_template_audit = if ($LatestMacTemplate) { $LatestMacTemplate.FullName } else { $null }
    latest_artifact_redaction = if ($LatestRedaction) { $LatestRedaction.FullName } else { $null }
    latest_cleanup = if ($LatestCleanup) { $LatestCleanup.FullName } else { $null }
    latest_installed_startup = if ($LatestStartup) { $LatestStartup.FullName } else { $null }
  }
  opencode_summary = if ($null -ne $OpenCodeReport) {
    [pscustomobject]@{
      route = $OpenCodeReport.route
      provider = $OpenCodeReport.provider
      workload_count = $OpenCodeReport.workload_count
      total_saved_tokens = $OpenCodeReport.total_saved_tokens
      savings_percent = $OpenCodeReport.savings_percent
      fallback_disabled = $OpenCodeReport.runtime_config_fallback_disabled
      export_redacted = $OpenCodeReport.export_redacted
      export_raw_available = $OpenCodeReport.export_raw_available
      report_path = $OpenCodeReport.report_path
    }
  } else { $null }
  checks = $Checks
}

$Stamp = (Get-Date).ToString("yyyyMMdd-HHmmss")
$ReportPath = Join-Path $OutputDir "release-handoff-manifest-$Stamp.json"
$Report | Add-Member -NotePropertyName report_path -NotePropertyValue $ReportPath
[System.IO.File]::WriteAllText($ReportPath, ($Report | ConvertTo-Json -Depth 10), $Utf8NoBom)
$Report | ConvertTo-Json -Depth 8 -Compress

if (-not $Report.ok) {
  exit 1
}
