param(
  [string]$OutputDir = "",
  [string]$InstalledDir = "",
  [switch]$SilentInstallLatest,
  [int]$StartupTimeoutSeconds = 20
)

$ErrorActionPreference = "Stop"
$Root = Resolve-Path (Join-Path $PSScriptRoot "..\..")
$Utf8NoBom = [System.Text.UTF8Encoding]::new($false)
if ([string]::IsNullOrWhiteSpace($OutputDir)) {
  $OutputDir = Join-Path $Root "support\artifacts"
}
if ([string]::IsNullOrWhiteSpace($InstalledDir)) {
  $InstalledDir = Join-Path $env:LOCALAPPDATA "d-token"
}
New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null

. (Join-Path $PSScriptRoot "release_candidate_provenance.ps1")
$CandidateStampPath = Join-Path $Root "code\target\release\d-token-release-candidate.json"
$ReleaseAppExe = Join-Path $Root "code\target\release\d-token-desktop.exe"
$ReleaseRuntimeSidecar = Join-Path $Root "code\target\release\dtoken-proxy.exe"
$ReleaseWebView2Loader = Join-Path $Root "code\target\release\WebView2Loader.dll"

$InstallerPattern = Join-Path $Root "code\target\release\bundle\nsis\d-token_*_x64-setup.exe"
$LatestInstaller = @(Get-ChildItem -Path $InstallerPattern -File -ErrorAction SilentlyContinue | Sort-Object LastWriteTime -Descending | Select-Object -First 1)[0]

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

function New-FileInfo {
  param([string]$Path)
  if (-not (Test-Path -LiteralPath $Path)) {
    return [pscustomobject]@{
      path = $Path
      exists = $false
      bytes = 0
      sha256 = $null
    }
  }
  $Item = Get-Item -LiteralPath $Path
  return [pscustomobject]@{
    path = $Item.FullName
    exists = $true
    bytes = $Item.Length
    sha256 = (Get-FileHash -LiteralPath $Item.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
  }
}

$CandidateStampStatus = Get-ReleaseCandidateStampStatus -Root $Root -StampPath $CandidateStampPath -ArtifactPaths @(
  $ReleaseAppExe
  if ($null -ne $LatestInstaller) { $LatestInstaller.FullName } else { $null }
) -OutputDir $OutputDir
$CurrentCandidate = $CandidateStampStatus.current_candidate
$CurrentReleaseBundle = ($CandidateStampStatus.candidate_stamp_matches_worktree -and $CandidateStampStatus.artifact_built_after_candidate_stamp)

$InstallResult = $null
if ($SilentInstallLatest) {
  if (-not $CurrentReleaseBundle) {
    throw "Refusing to install an artifact that is not stamped for the current source candidate. Rebuild the desktop bundle first."
  }
  if ($null -eq $LatestInstaller) {
    throw "No NSIS installer was found under code\target\release\bundle\nsis."
  }
  $InstallProcess = Start-Process -FilePath $LatestInstaller.FullName -ArgumentList @("/S") -PassThru -WindowStyle Hidden
  if (-not $InstallProcess.WaitForExit(120000)) {
    try { Stop-Process -Id $InstallProcess.Id -Force -ErrorAction SilentlyContinue } catch {}
    throw "Silent installer did not exit within 120 seconds."
  }
  $InstallResult = [pscustomobject]@{
    installer = $LatestInstaller.FullName
    exit_code = $InstallProcess.ExitCode
  }
}

$AppExe = Join-Path $InstalledDir "d-token-desktop.exe"
$RuntimeSidecar = Join-Path $InstalledDir "dtoken-proxy.exe"
$WebView2Loader = Join-Path $InstalledDir "WebView2Loader.dll"
$UninstallExe = Join-Path $InstalledDir "uninstall.exe"

$InstalledFiles = [pscustomobject]@{
  directory = $InstalledDir
  app_exe = New-FileInfo $AppExe
  runtime_sidecar = New-FileInfo $RuntimeSidecar
  webview2_loader = New-FileInfo $WebView2Loader
  uninstall = New-FileInfo $UninstallExe
}

$ReleaseFiles = [pscustomobject]@{
  app_exe = New-FileInfo $ReleaseAppExe
  runtime_sidecar = New-FileInfo $ReleaseRuntimeSidecar
  webview2_loader = New-FileInfo $ReleaseWebView2Loader
}

$DependencyOk = (
  $InstalledFiles.app_exe.exists -and
  $InstalledFiles.runtime_sidecar.exists -and
  $InstalledFiles.webview2_loader.exists
)
$InstalledMatchesCurrentBundle = (
  $DependencyOk -and
  $ReleaseFiles.app_exe.exists -and
  $ReleaseFiles.runtime_sidecar.exists -and
  $ReleaseFiles.webview2_loader.exists -and
  $InstalledFiles.app_exe.sha256 -eq $ReleaseFiles.app_exe.sha256 -and
  $InstalledFiles.runtime_sidecar.sha256 -eq $ReleaseFiles.runtime_sidecar.sha256 -and
  $InstalledFiles.webview2_loader.sha256 -eq $ReleaseFiles.webview2_loader.sha256
)

$LaunchedProcess = $null
$LaunchError = $null
$WindowTitle = $null
$ProcessAlive = $false
try {
  if (-not $CurrentReleaseBundle) {
    throw "Current release bundle provenance is missing or stale; startup was not attempted."
  }
  if (-not $DependencyOk) {
    throw "Installed app dependency files are incomplete."
  }
  if (-not $InstalledMatchesCurrentBundle) {
    throw "Installed files do not match the current release bundle; startup was not attempted."
  }
  $LaunchedProcess = Start-Process -FilePath $AppExe -PassThru -WindowStyle Hidden
  $Deadline = (Get-Date).AddSeconds($StartupTimeoutSeconds)
  while ((Get-Date) -lt $Deadline) {
    Start-Sleep -Milliseconds 500
    $Current = Get-Process -Id $LaunchedProcess.Id -ErrorAction SilentlyContinue
    if ($null -eq $Current) {
      break
    }
    if (-not [string]::IsNullOrWhiteSpace($Current.MainWindowTitle)) {
      $WindowTitle = $Current.MainWindowTitle
      break
    }
  }
  $FinalProcess = Get-Process -Id $LaunchedProcess.Id -ErrorAction SilentlyContinue
  if ($null -ne $FinalProcess) {
    $ProcessAlive = $true
    if ([string]::IsNullOrWhiteSpace($WindowTitle)) {
      $WindowTitle = $FinalProcess.MainWindowTitle
    }
  }
} catch {
  $LaunchError = $_.Exception.Message
} finally {
  if ($null -ne $LaunchedProcess) {
    try {
      $StillRunning = Get-Process -Id $LaunchedProcess.Id -ErrorAction SilentlyContinue
      if ($null -ne $StillRunning) {
        Stop-Process -Id $StillRunning.Id -Force -ErrorAction SilentlyContinue
      }
    } catch {}
  }
}

$Checks = @(
  New-Check "candidate_stamp_matches_worktree" $CandidateStampStatus.candidate_stamp_matches_worktree @{
    stamp_path = $CandidateStampPath
    stamp_candidate_id = if ($null -ne $CandidateStampStatus.stamp) { $CandidateStampStatus.stamp.candidate_id } else { $null }
    current_candidate_id = $CurrentCandidate.candidate_id
  }
  New-Check "release_artifacts_built_after_candidate_stamp" $CandidateStampStatus.artifact_built_after_candidate_stamp @{
    stamp_last_write_time = $CandidateStampStatus.stamp_last_write_time
    missing_artifacts = $CandidateStampStatus.missing_artifacts
  }
  New-Check "installed_dependencies_present" $DependencyOk $InstalledFiles
  New-Check "installed_files_match_current_bundle" $InstalledMatchesCurrentBundle @{
    installed = $InstalledFiles
    release = $ReleaseFiles
  }
  New-Check "installed_app_launches_and_stays_alive" ($ProcessAlive -and [string]::IsNullOrWhiteSpace($LaunchError)) @{
    process_id = if ($null -ne $LaunchedProcess) { $LaunchedProcess.Id } else { $null }
    alive = $ProcessAlive
    window_title = $WindowTitle
    error = $LaunchError
    timeout_seconds = $StartupTimeoutSeconds
  }
)

if ($SilentInstallLatest) {
  $Checks = @(
    New-Check "silent_install_latest_installer" ($InstallResult.exit_code -eq 0) $InstallResult
  ) + $Checks
}

$Passed = @($Checks | Where-Object { $_.ok }).Count
$Total = @($Checks).Count
$Report = [pscustomobject]@{
  ok = ($Passed -eq $Total)
  mode = "installed-desktop-startup-audit"
  generated_at = (Get-Date).ToString("o")
  passed = $Passed
  total = $Total
  candidate = $CurrentCandidate
  latest_installer = if ($null -ne $LatestInstaller) { $LatestInstaller.FullName } else { $null }
  installed_dir = $InstalledDir
  privacy = "Launches the installed d-token desktop app and checks dependency presence/window process state. Does not read secrets or agent auth files."
  external_services = "Does not start, stop, or reconfigure external Headroom, RTK, cc-switch, or OpenCode services."
  checks = $Checks
}

$Stamp = (Get-Date).ToString("yyyyMMdd-HHmmss")
$ReportPath = Join-Path $OutputDir "installed-desktop-startup-audit-$Stamp.json"
$Report | Add-Member -NotePropertyName report_path -NotePropertyValue $ReportPath
[System.IO.File]::WriteAllText($ReportPath, ($Report | ConvertTo-Json -Depth 8), $Utf8NoBom)
$Report | ConvertTo-Json -Depth 8 -Compress

if (-not $Report.ok) {
  exit 1
}
