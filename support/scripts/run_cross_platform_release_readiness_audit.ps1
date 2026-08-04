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

function Get-JsonFile {
  param([string]$Path)
  if (-not (Test-Path -LiteralPath $Path)) { return $null }
  return Get-Content -LiteralPath $Path -Raw | ConvertFrom-Json
}

function Get-LatestFile {
  param([string]$Pattern)
  $Files = @(Get-ChildItem -LiteralPath $OutputDir -Filter $Pattern -File -ErrorAction SilentlyContinue | Sort-Object LastWriteTime -Descending)
  if ($Files.Count -eq 0) { return $null }
  return $Files[0]
}

$TauriConfigPath = Join-Path $Root "code\apps\desktop\src-tauri\tauri.conf.json"
$TauriMacConfigPath = Join-Path $Root "code\apps\desktop\src-tauri\tauri.macos.conf.json"
$PackageJsonPath = Join-Path $Root "code\apps\desktop\package.json"
$TechStackPath = Join-Path $Root "docs\TECH_STACK.md"
$PrdPath = Join-Path $Root "docs\PRD.md"
$TauriConfig = Get-JsonFile $TauriConfigPath
$TauriMacConfig = Get-JsonFile $TauriMacConfigPath
$PackageJson = Get-JsonFile $PackageJsonPath
$TechStack = if (Test-Path -LiteralPath $TechStackPath) { Get-Content -LiteralPath $TechStackPath -Raw } else { "" }
$Prd = if (Test-Path -LiteralPath $PrdPath) { Get-Content -LiteralPath $PrdPath -Raw } else { "" }

$LatestHandoff = Get-LatestFile "release-handoff-manifest-*.json"
$LatestMacTemplate = Get-LatestFile "macos-release-template-audit-*.json"
$HandoffReport = if ($LatestHandoff) { Get-JsonFile $LatestHandoff.FullName } else { $null }
$MacTemplateReport = if ($LatestMacTemplate) { Get-JsonFile $LatestMacTemplate.FullName } else { $null }

$BundleTargets = @()
if ($TauriConfig -and $TauriConfig.bundle -and $TauriConfig.bundle.targets) {
  $BundleTargets = @($TauriConfig.bundle.targets)
}
$MacBundleTargets = @()
if ($TauriMacConfig -and $TauriMacConfig.bundle -and $TauriMacConfig.bundle.targets) {
  $MacBundleTargets = @($TauriMacConfig.bundle.targets)
}

$WindowsInstaller = Join-Path $Root "code\target\release\bundle\nsis\d-token_0.2.0_x64-setup.exe"
$MacDmgCandidates = @(
  (Join-Path $Root "code\target\release\bundle\dmg\d-token_0.2.0_aarch64.dmg"),
  (Join-Path $Root "code\target\release\bundle\dmg\d-token_0.2.0_x64.dmg"),
  (Join-Path $Root "code\target\release\bundle\macos\d-token.app")
)

$IsWindowsHost = $IsWindows -or ([System.Runtime.InteropServices.RuntimeInformation]::IsOSPlatform([System.Runtime.InteropServices.OSPlatform]::Windows))
$IsMacHost = $IsMacOS -or ([System.Runtime.InteropServices.RuntimeInformation]::IsOSPlatform([System.Runtime.InteropServices.OSPlatform]::OSX))
$WindowsArtifactExists = Test-Path -LiteralPath $WindowsInstaller
$MacArtifactExists = (@($MacDmgCandidates | Where-Object { Test-Path -LiteralPath $_ }).Count -gt 0)
$HasNsisTarget = @($BundleTargets | Where-Object { $_ -eq "nsis" }).Count -gt 0
$HasMacBundleTarget = @($MacBundleTargets | Where-Object { $_ -eq "dmg" -or $_ -eq "app" -or $_ -eq "macos" }).Count -gt 0
$PackageBuildScript = if ($PackageJson -and $PackageJson.scripts) { [string]$PackageJson.scripts.'tauri:build' } else { "" }
$PreparesSidecars = $PackageBuildScript -like "*tauri:with-sidecar*" -or $PackageBuildScript -like "*prepare_desktop_sidecars*"
$PrdMentionsMac = $Prd -match "macOS"
$TechStackMentionsMacSigning = ($TechStack -match "code signing") -and ($TechStack -match "notarization")

$Checks = @(
  New-Check "prd_declares_windows_macos" ($PrdMentionsMac -and ($Prd -match "Windows")) @{
    prd_path = $PrdPath
  }
  New-Check "tech_stack_tracks_macos_signing_notarization" $TechStackMentionsMacSigning @{
    tech_stack_path = $TechStackPath
  }
  New-Check "windows_nsis_target_configured" $HasNsisTarget @{
    targets = $BundleTargets
  }
  New-Check "windows_installer_artifact_exists" $WindowsArtifactExists @{
    path = $WindowsInstaller
  }
  New-Check "desktop_sidecar_preparation_in_build" $PreparesSidecars @{
    tauri_build_script = $PackageBuildScript
  }
  New-Check "release_handoff_manifest_available" ($null -ne $HandoffReport -and $HandoffReport.ok -eq $true) @{
    path = if ($LatestHandoff) { $LatestHandoff.FullName } else { $null }
  }
  New-Check "macos_bundle_target_configured" $HasMacBundleTarget @{
    targets = $MacBundleTargets
    config_path = $TauriMacConfigPath
  }
  New-Check "macos_artifact_exists" $MacArtifactExists @{
    candidates = $MacDmgCandidates
    host_os_is_macos = $IsMacHost
    note = "macOS artifacts normally require a macOS runner with code signing/notarization credentials."
  }
)

$WindowsReadyLocal = $WindowsArtifactExists -and ($null -ne $HandoffReport) -and ($HandoffReport.local_release_candidate_ready -eq $true)
$MacReadyLocal = $MacArtifactExists -and $HasMacBundleTarget
$MacTemplateReady = ($null -ne $MacTemplateReport -and $MacTemplateReport.ok -eq $true)
$CrossPlatformLocalReady = $WindowsReadyLocal -and $MacReadyLocal

$Report = [pscustomobject]@{
  ok = $true
  mode = "cross-platform-release-readiness-audit"
  generated_at = (Get-Date).ToString("o")
  host = [pscustomobject]@{
    is_windows = $IsWindowsHost
    is_macos = $IsMacHost
  }
  product_targets = @("Windows", "macOS")
  windows = [pscustomobject]@{
    local_release_candidate_ready = $WindowsReadyLocal
    installer_exists = $WindowsArtifactExists
    installer_path = $WindowsInstaller
    bundle_target_configured = $HasNsisTarget
  }
  macos = [pscustomobject]@{
    local_release_candidate_ready = $MacReadyLocal
    release_template_ready = $MacTemplateReady
    release_template_report = if ($LatestMacTemplate) { $LatestMacTemplate.FullName } else { $null }
    bundle_target_configured = $HasMacBundleTarget
    bundle_targets = $MacBundleTargets
    artifact_exists = $MacArtifactExists
    artifact_candidates = $MacDmgCandidates
    requires_macos_runner = -not $IsMacHost
    requires_code_signing_notarization = $true
  }
  cross_platform_local_release_candidate_ready = $CrossPlatformLocalReady
  public_distribution_ready = $false
  public_distribution_blockers = @(
    if (-not $WindowsReadyLocal) { "Windows local release candidate is not ready" }
    if (-not $MacReadyLocal) { "macOS local release candidate artifact is not built/configured on this host" }
    if (-not $MacTemplateReady) { "macOS release workflow/template is not ready" }
    "Windows app/runtime/installer signing is not complete"
    "macOS code signing/notarization is not complete"
  )
  boundary = "Read-only readiness audit. It does not build macOS artifacts on Windows, sign code, notarize, install certificates, or access credentials."
  checks = $Checks
}

$Stamp = (Get-Date).ToString("yyyyMMdd-HHmmss")
$ReportPath = Join-Path $OutputDir "cross-platform-release-readiness-$Stamp.json"
$Report | Add-Member -NotePropertyName report_path -NotePropertyValue $ReportPath
[System.IO.File]::WriteAllText($ReportPath, ($Report | ConvertTo-Json -Depth 8), $Utf8NoBom)
$Report | ConvertTo-Json -Depth 8 -Compress

# This audit is informational for current Windows-host release work.
# It passes when the audit itself completed, even if macOS/public distribution is not ready.
exit 0
