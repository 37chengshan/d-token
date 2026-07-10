param(
  [string]$OutputDir = "",
  [string]$CertificatePath = "",
  [switch]$RequireCertificate
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

$Artifacts = @(
  Join-Path $Root "code\target\release\d-token-desktop.exe"
  Join-Path $Root "code\target\release\dtoken-proxy.exe"
  Join-Path $Root "code\target\release\bundle\nsis\d-token_0.2.0_x64-setup.exe"
)

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

function Get-SignatureInfo {
  param([string]$Path)
  if (-not (Test-Path -LiteralPath $Path)) {
    return [pscustomobject]@{
      path = $Path
      exists = $false
      bytes = 0
      sha256 = $null
      status = "Missing"
      signer_subject = $null
    }
  }
  $Item = Get-Item -LiteralPath $Path
  $Sig = Get-AuthenticodeSignature -LiteralPath $Item.FullName
  [pscustomobject]@{
    path = $Item.FullName
    exists = $true
    bytes = $Item.Length
    sha256 = (Get-FileHash -LiteralPath $Item.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
    status = [string]$Sig.Status
    signer_subject = if ($Sig.SignerCertificate) { $Sig.SignerCertificate.Subject } else { $null }
  }
}

$Signtool = Get-Command signtool.exe -ErrorAction SilentlyContinue
$ArtifactInfo = @($Artifacts | ForEach-Object { Get-SignatureInfo $_ })
$MissingArtifacts = @($ArtifactInfo | Where-Object { -not $_.exists })
$UnsignedArtifacts = @($ArtifactInfo | Where-Object { $_.exists -and $_.status -ne "Valid" })
$CertificateProvided = -not [string]::IsNullOrWhiteSpace($CertificatePath)
$CertificateExists = $CertificateProvided -and (Test-Path -LiteralPath $CertificatePath)
$TimestampUrls = @(
  "http://timestamp.digicert.com",
  "http://timestamp.sectigo.com"
)
$CandidateStampStatus = Get-ReleaseCandidateStampStatus -Root $Root -StampPath $CandidateStampPath -ArtifactPaths @($Artifacts[0], $Artifacts[2]) -OutputDir $OutputDir
$CurrentCandidate = $CandidateStampStatus.current_candidate

$Checks = @(
  New-Check "candidate_stamp_matches_worktree" $CandidateStampStatus.candidate_stamp_matches_worktree @{
    stamp_path = $CandidateStampPath
    stamp_candidate_id = if ($null -ne $CandidateStampStatus.stamp) { $CandidateStampStatus.stamp.candidate_id } else { $null }
    current_candidate_id = $CurrentCandidate.candidate_id
  }
  New-Check "release_artifacts_built_after_candidate_stamp" $CandidateStampStatus.artifact_built_after_candidate_stamp @{
    stamp_last_write_time = $CandidateStampStatus.stamp_last_write_time
  }
  New-Check "signtool_available" ($null -ne $Signtool) @{
    source = if ($null -ne $Signtool) { $Signtool.Source } else { $null }
  }
  New-Check "artifacts_exist" ($MissingArtifacts.Count -eq 0) @{
    artifacts = $ArtifactInfo
  }
  New-Check "certificate_input_ready" ((-not $RequireCertificate) -or $CertificateExists) @{
    certificate_required = [bool]$RequireCertificate
    certificate_path_provided = $CertificateProvided
    certificate_path_exists = $CertificateExists
    note = "This audit does not read certificate passwords or load private keys."
  }
  New-Check "timestamp_servers_configured" ($TimestampUrls.Count -gt 0) @{
    timestamp_urls = $TimestampUrls
  }
)

$CanSignNow = (
  $CandidateStampStatus.candidate_stamp_matches_worktree -and
  $CandidateStampStatus.artifact_built_after_candidate_stamp -and
  ($null -ne $Signtool) -and
  ($MissingArtifacts.Count -eq 0) -and
  ((-not $RequireCertificate) -or $CertificateExists)
)

$Report = [pscustomobject]@{
  ok = ($MissingArtifacts.Count -eq 0)
  mode = "windows-signing-readiness-audit"
  generated_at = (Get-Date).ToString("o")
  candidate = $CurrentCandidate
  candidate_stamp = $CandidateStampStatus.stamp
  can_sign_now = $CanSignNow
  distribution_ready_public = ($CandidateStampStatus.candidate_stamp_matches_worktree -and $CandidateStampStatus.artifact_built_after_candidate_stamp -and $UnsignedArtifacts.Count -eq 0 -and $MissingArtifacts.Count -eq 0)
  signtool = if ($null -ne $Signtool) { $Signtool.Source } else { $null }
  unsigned_artifact_count = $UnsignedArtifacts.Count
  unsigned_artifacts = $UnsignedArtifacts
  checks = $Checks
  signing_note = "Readiness audit only. It does not sign files, install certificates, read certificate passwords, or access credentials."
}

$Stamp = (Get-Date).ToString("yyyyMMdd-HHmmss")
$ReportPath = Join-Path $OutputDir "windows-signing-readiness-$Stamp.json"
$Report | Add-Member -NotePropertyName report_path -NotePropertyValue $ReportPath
[System.IO.File]::WriteAllText($ReportPath, ($Report | ConvertTo-Json -Depth 8), $Utf8NoBom)
$Report | ConvertTo-Json -Depth 8 -Compress

if (-not $Report.ok) {
  exit 1
}
