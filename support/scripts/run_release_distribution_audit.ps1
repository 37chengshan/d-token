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
$CandidateStampPath = Join-Path $Root "code\target\release\d-token-release-candidate.json"

$AppExe = Join-Path $Root "code\target\release\d-token-desktop.exe"
$RuntimeSidecar = Join-Path $Root "code\target\release\dtoken-proxy.exe"
$WebView2Loader = Join-Path $Root "code\target\release\WebView2Loader.dll"
$InstallerPattern = Join-Path $Root "code\target\release\bundle\nsis\d-token_*_x64-setup.exe"
$Installers = @(Get-ChildItem -Path $InstallerPattern -File -ErrorAction SilentlyContinue | Sort-Object LastWriteTime -Descending)
$Installer = if ($Installers.Count -gt 0) { $Installers[0].FullName } else { $null }
$InstalledDir = Join-Path $env:LOCALAPPDATA "d-token"

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
      last_write_time = $null
      authenticode = [pscustomobject]@{
        status = "Missing"
        status_message = "File is missing"
        signer_subject = $null
        signer_thumbprint = $null
      }
    }
  }

  $Item = Get-Item -LiteralPath $Path
  $Hash = (Get-FileHash -LiteralPath $Item.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
  $Signature = Get-AuthenticodeSignature -LiteralPath $Item.FullName
  $Signer = $Signature.SignerCertificate
  return [pscustomobject]@{
    id = $Id
    required = $Required
    exists = $true
    path = $Item.FullName
    bytes = $Item.Length
    sha256 = $Hash
    last_write_time = $Item.LastWriteTime.ToString("o")
    authenticode = [pscustomobject]@{
      status = [string]$Signature.Status
      status_message = [string]$Signature.StatusMessage
      signer_subject = if ($null -ne $Signer) { $Signer.Subject } else { $null }
      signer_thumbprint = if ($null -ne $Signer) { $Signer.Thumbprint } else { $null }
    }
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

$Artifacts = @(
  Get-Artifact "app_exe" $AppExe
  Get-Artifact "runtime_sidecar" $RuntimeSidecar
  Get-Artifact "webview2_loader" $WebView2Loader
  Get-Artifact "nsis_installer" $Installer
)

$InstalledFiles = [pscustomobject]@{
  directory = $InstalledDir
  directory_exists = Test-Path -LiteralPath $InstalledDir
  app_exe_exists = Test-Path -LiteralPath (Join-Path $InstalledDir "d-token-desktop.exe")
  runtime_sidecar_exists = Test-Path -LiteralPath (Join-Path $InstalledDir "dtoken-proxy.exe")
  webview2_loader_exists = Test-Path -LiteralPath (Join-Path $InstalledDir "WebView2Loader.dll")
  uninstall_exists = Test-Path -LiteralPath (Join-Path $InstalledDir "uninstall.exe")
}

$RequiredArtifactsExist = (@($Artifacts | Where-Object { $_.required -and -not $_.exists }).Count -eq 0)
$HashesRecorded = (@($Artifacts | Where-Object { $_.required -and [string]::IsNullOrWhiteSpace($_.sha256) }).Count -eq 0)
$InstallerArtifact = @($Artifacts | Where-Object { $_.id -eq "nsis_installer" } | Select-Object -First 1)[0]
$InstallerLooksReal = ($InstallerArtifact.exists -and $InstallerArtifact.bytes -gt 1000000)
$InstalledStartupFilesPresent = (
  $InstalledFiles.directory_exists -and
  $InstalledFiles.app_exe_exists -and
  $InstalledFiles.runtime_sidecar_exists -and
  $InstalledFiles.webview2_loader_exists
)
$UnsignedArtifacts = @($Artifacts | Where-Object { $_.required -and $_.authenticode.status -ne "Valid" })
$CandidateStampStatus = Get-ReleaseCandidateStampStatus -Root $Root -StampPath $CandidateStampPath -ArtifactPaths @(
  $AppExe
  $Installer
) -OutputDir $OutputDir
$CurrentCandidate = $CandidateStampStatus.current_candidate

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
  New-Check "required_artifacts_exist" $RequiredArtifactsExist $Artifacts
  New-Check "hashes_recorded" $HashesRecorded ($Artifacts | Select-Object id, sha256)
  New-Check "installer_size_nontrivial" $InstallerLooksReal @{ installer_bytes = $InstallerArtifact.bytes; min_bytes = 1000000 }
  New-Check "installed_startup_files_present_if_installed_dir_exists" ((-not $InstalledFiles.directory_exists) -or $InstalledStartupFilesPresent) $InstalledFiles
  New-Check "all_required_artifacts_signed_for_public_distribution" ($UnsignedArtifacts.Count -eq 0) ($UnsignedArtifacts | Select-Object id, path, @{Name="status";Expression={$_.authenticode.status}}, @{Name="signer_subject";Expression={$_.authenticode.signer_subject}})
)

$ReleaseCandidateReady = (
  $CandidateStampStatus.candidate_stamp_matches_worktree -and
  $CandidateStampStatus.artifact_built_after_candidate_stamp -and
  $RequiredArtifactsExist -and
  $HashesRecorded -and
  $InstallerLooksReal -and
  ((-not $InstalledFiles.directory_exists) -or $InstalledStartupFilesPresent)
)
$DistributionReadyPublic = $ReleaseCandidateReady -and ($UnsignedArtifacts.Count -eq 0)

$Report = [pscustomobject]@{
  ok = $ReleaseCandidateReady
  mode = "release-distribution-audit"
  generated_at = (Get-Date).ToString("o")
  candidate = $CurrentCandidate
  candidate_stamp = $CandidateStampStatus.stamp
  release_candidate_ready = $ReleaseCandidateReady
  distribution_ready_public = $DistributionReadyPublic
  signing_status = if ($UnsignedArtifacts.Count -eq 0) { "signed-valid" } else { "unsigned-or-not-valid" }
  signing_note = if ($DistributionReadyPublic) {
    "All required artifacts have valid Authenticode signatures."
  } elseif (-not $ReleaseCandidateReady) {
    "The discovered artifacts are not the current source candidate. Rebuild and rerun startup verification before any release handoff."
  } else {
    "Artifacts are suitable for local release-candidate verification only. Public distribution still needs signing/reputation hardening for unsigned or non-valid artifacts."
  }
  artifacts = $Artifacts
  installed_files = $InstalledFiles
  checks = $Checks
}

$Stamp = (Get-Date).ToString("yyyyMMdd-HHmmss")
$ReportPath = Join-Path $OutputDir "release-distribution-manifest-$Stamp.json"
$Report | Add-Member -NotePropertyName report_path -NotePropertyValue $ReportPath
[System.IO.File]::WriteAllText($ReportPath, ($Report | ConvertTo-Json -Depth 10), $Utf8NoBom)
$Report | ConvertTo-Json -Depth 8 -Compress

if (-not $Report.ok) {
  exit 1
}
