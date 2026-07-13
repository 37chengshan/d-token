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

. (Join-Path $PSScriptRoot "release_candidate_provenance.ps1")

$BundleRoot = Join-Path $Root "code\target\release\bundle"
$AppExe = Join-Path $Root "code\target\release\d-token-desktop.exe"
$ReleaseSidecar = Join-Path $Root "code\target\release\dtoken-proxy.exe"
$ReleaseWebView2Loader = Join-Path $Root "code\target\release\WebView2Loader.dll"
$PreparedSidecarMsvc = Join-Path $Root "code\apps\desktop\src-tauri\binaries\dtoken-proxy-x86_64-pc-windows-msvc.exe"
$PreparedWebView2Loader = Join-Path $Root "code\apps\desktop\src-tauri\binaries\WebView2Loader.dll"
$CandidateStampPath = Join-Path $Root "code\target\release\d-token-release-candidate.json"
$InstallerPattern = Join-Path $BundleRoot "nsis\d-token_*_x64-setup.exe"
$Installers = @(Get-ChildItem -Path $InstallerPattern -File -ErrorAction SilentlyContinue | Sort-Object LastWriteTime -Descending)
$Installer = if ($Installers.Count -gt 0) { $Installers[0] } else { $null }

function Get-HashValue {
  param([string]$Path)
  if (-not (Test-Path -LiteralPath $Path)) { return $null }
  return (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
}

function New-Artifact {
  param([string]$Path)
  if (-not (Test-Path -LiteralPath $Path)) { return $null }
  $Item = Get-Item -LiteralPath $Path
  [pscustomobject]@{
    path = $Item.FullName
    bytes = $Item.Length
    sha256 = Get-HashValue $Item.FullName
    last_write_time = $Item.LastWriteTime.ToString("o")
  }
}

$AppArtifact = New-Artifact $AppExe
$ReleaseSidecarArtifact = New-Artifact $ReleaseSidecar
$ReleaseWebView2LoaderArtifact = New-Artifact $ReleaseWebView2Loader
$PreparedSidecarMsvcArtifact = New-Artifact $PreparedSidecarMsvc
$PreparedWebView2LoaderArtifact = New-Artifact $PreparedWebView2Loader
$InstallerArtifact = if ($null -ne $Installer) { New-Artifact $Installer.FullName } else { $null }
$CandidateStampStatus = Get-ReleaseCandidateStampStatus -Root $Root -StampPath $CandidateStampPath -ArtifactPaths @(
  if ($null -ne $AppArtifact) { $AppArtifact.path } else { $null }
  if ($null -ne $InstallerArtifact) { $InstallerArtifact.path } else { $null }
) -OutputDir $OutputDir
$CurrentCandidate = $CandidateStampStatus.current_candidate
$CandidateStamp = $CandidateStampStatus.stamp
$CandidateStampMatchesWorktree = $CandidateStampStatus.candidate_stamp_matches_worktree
$ArtifactBuiltAfterCandidateStamp = $CandidateStampStatus.artifact_built_after_candidate_stamp
$Checks = @(
  [pscustomobject]@{ id = "candidate_stamp_matches_worktree"; ok = $CandidateStampMatchesWorktree; detail = @{ stamp_path = $CandidateStampPath; stamp_candidate_id = if ($null -ne $CandidateStamp) { $CandidateStamp.candidate_id } else { $null }; current_candidate_id = $CurrentCandidate.candidate_id } }
  [pscustomobject]@{ id = "artifact_built_after_candidate_stamp"; ok = $ArtifactBuiltAfterCandidateStamp; detail = @{ stamp_last_write_time = $CandidateStampStatus.stamp_last_write_time; app_last_write_time = if ($null -ne $AppArtifact) { $AppArtifact.last_write_time } else { $null }; installer_last_write_time = if ($null -ne $InstallerArtifact) { $InstallerArtifact.last_write_time } else { $null }; missing_artifacts = $CandidateStampStatus.missing_artifacts } }
  [pscustomobject]@{ id = "release_exe_exists"; ok = ($null -ne $AppArtifact); detail = $AppArtifact }
  [pscustomobject]@{ id = "runtime_sidecar_release_exists"; ok = ($null -ne $ReleaseSidecarArtifact); detail = $ReleaseSidecarArtifact }
  [pscustomobject]@{ id = "runtime_sidecar_prepared_exists"; ok = ($null -ne $PreparedSidecarMsvcArtifact); detail = @{ target = "x86_64-pc-windows-msvc"; msvc = $PreparedSidecarMsvcArtifact } }
  [pscustomobject]@{ id = "webview2_loader_release_exists"; ok = ($null -ne $ReleaseWebView2LoaderArtifact); detail = $ReleaseWebView2LoaderArtifact }
  [pscustomobject]@{ id = "webview2_loader_prepared_exists"; ok = ($null -ne $PreparedWebView2LoaderArtifact); detail = $PreparedWebView2LoaderArtifact }
  [pscustomobject]@{ id = "nsis_installer_exists"; ok = ($null -ne $InstallerArtifact); detail = $InstallerArtifact }
  [pscustomobject]@{ id = "installer_size_nontrivial"; ok = (($null -ne $InstallerArtifact) -and ($InstallerArtifact.bytes -gt 1000000)); detail = @{ min_bytes = 1000000; bytes = if ($null -ne $InstallerArtifact) { $InstallerArtifact.bytes } else { 0 } } }
  [pscustomobject]@{ id = "hashes_recorded"; ok = (($null -ne $AppArtifact.sha256) -and ($null -ne $ReleaseSidecarArtifact.sha256) -and ($null -ne $PreparedSidecarMsvcArtifact.sha256) -and ($null -ne $ReleaseWebView2LoaderArtifact.sha256) -and ($null -ne $PreparedWebView2LoaderArtifact.sha256) -and ($null -ne $InstallerArtifact.sha256)); detail = @{ app_sha256 = $AppArtifact.sha256; sidecar_release_sha256 = $ReleaseSidecarArtifact.sha256; sidecar_prepared_msvc_sha256 = $PreparedSidecarMsvcArtifact.sha256; webview2_release_sha256 = $ReleaseWebView2LoaderArtifact.sha256; webview2_prepared_sha256 = $PreparedWebView2LoaderArtifact.sha256; installer_sha256 = $InstallerArtifact.sha256 } }
  [pscustomobject]@{ id = "sidecar_hash_matches_prepared_bundle_input"; ok = (($null -ne $ReleaseSidecarArtifact.sha256) -and ($ReleaseSidecarArtifact.sha256 -eq $PreparedSidecarMsvcArtifact.sha256)); detail = @{ release = $ReleaseSidecarArtifact.sha256; prepared_msvc = $PreparedSidecarMsvcArtifact.sha256 } }
  [pscustomobject]@{ id = "webview2_loader_hash_matches_prepared_bundle_input"; ok = (($null -ne $ReleaseWebView2LoaderArtifact.sha256) -and ($ReleaseWebView2LoaderArtifact.sha256 -eq $PreparedWebView2LoaderArtifact.sha256)); detail = @{ release = $ReleaseWebView2LoaderArtifact.sha256; prepared = $PreparedWebView2LoaderArtifact.sha256 } }
  [pscustomobject]@{ id = "installer_built_after_sidecar_prepared"; ok = (($null -ne $InstallerArtifact) -and ($null -ne $PreparedSidecarMsvcArtifact) -and ((Get-Item -LiteralPath $InstallerArtifact.path).LastWriteTime -ge (Get-Item -LiteralPath $PreparedSidecarMsvcArtifact.path).LastWriteTime)); detail = @{ installer_last_write_time = if ($null -ne $InstallerArtifact) { $InstallerArtifact.last_write_time } else { $null }; sidecar_last_write_time = if ($null -ne $PreparedSidecarMsvcArtifact) { $PreparedSidecarMsvcArtifact.last_write_time } else { $null } } }
  [pscustomobject]@{ id = "installer_built_after_webview2_loader_prepared"; ok = (($null -ne $InstallerArtifact) -and ($null -ne $PreparedWebView2LoaderArtifact) -and ((Get-Item -LiteralPath $InstallerArtifact.path).LastWriteTime -ge (Get-Item -LiteralPath $PreparedWebView2LoaderArtifact.path).LastWriteTime)); detail = @{ installer_last_write_time = if ($null -ne $InstallerArtifact) { $InstallerArtifact.last_write_time } else { $null }; webview2_loader_last_write_time = if ($null -ne $PreparedWebView2LoaderArtifact) { $PreparedWebView2LoaderArtifact.last_write_time } else { $null } } }
)
$Passed = @($Checks | Where-Object { $_.ok }).Count
$Total = @($Checks).Count
$Report = [pscustomobject]@{
  ok = ($Passed -eq $Total)
  mode = "desktop-bundle-artifact-audit"
  passed = $Passed
  total = $Total
  summary = "Desktop bundle artifact audit: $Passed/$Total checks passed"
  signing_status = "unsigned-development-build"
  signing_note = "The installer artifact exists but has not been signed or notarized in this audit. Do not treat it as public distribution ready until signing is completed."
  candidate = $CurrentCandidate
  candidate_stamp = $CandidateStamp
  artifacts = [pscustomobject]@{
    app_exe = $AppArtifact
    dtoken_proxy_release = $ReleaseSidecarArtifact
    webview2_loader_release = $ReleaseWebView2LoaderArtifact
    dtoken_proxy_prepared_msvc_sidecar = $PreparedSidecarMsvcArtifact
    webview2_loader_prepared_resource = $PreparedWebView2LoaderArtifact
    nsis_installer = $InstallerArtifact
  }
  checks = $Checks
}

$Stamp = (Get-Date).ToString("yyyyMMdd-HHmmss")
$ReportPath = Join-Path $OutputDir "desktop-bundle-artifact-audit-$Stamp.json"
$Report | Add-Member -NotePropertyName report_path -NotePropertyValue $ReportPath
[System.IO.File]::WriteAllText($ReportPath, ($Report | ConvertTo-Json -Depth 8), $Utf8NoBom)
$Report | ConvertTo-Json -Depth 8 -Compress

if (-not $Report.ok) {
  exit 1
}
