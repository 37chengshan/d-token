param(
  [string]$CertificatePath = "",
  [string]$CertificatePasswordEnv = "DTOKEN_WINDOWS_CERT_PASSWORD",
  [string]$TimestampUrl = "http://timestamp.digicert.com",
  [string]$OutputDir = "",
  [switch]$Execute
)

$ErrorActionPreference = "Stop"
$Root = Resolve-Path (Join-Path $PSScriptRoot "..\..")
$Utf8NoBom = [System.Text.UTF8Encoding]::new($false)
if ([string]::IsNullOrWhiteSpace($OutputDir)) {
  $OutputDir = Join-Path $Root "support\artifacts"
}
New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null

$Artifacts = @(
  Join-Path $Root "code\target\release\d-token-desktop.exe"
  Join-Path $Root "code\target\release\dtoken-proxy.exe"
  Join-Path $Root "code\target\release\bundle\nsis\d-token_0.2.0_x64-setup.exe"
)

function New-ArtifactPlan {
  param([string]$Path)
  if (-not (Test-Path -LiteralPath $Path)) {
    return [pscustomobject]@{
      path = $Path
      exists = $false
      bytes = 0
      sha256_before = $null
      status_before = "Missing"
      status_after = $null
      signer_after = $null
      signed = $false
      error = "artifact missing"
    }
  }
  $Item = Get-Item -LiteralPath $Path
  $Sig = Get-AuthenticodeSignature -LiteralPath $Item.FullName
  [pscustomobject]@{
    path = $Item.FullName
    exists = $true
    bytes = $Item.Length
    sha256_before = (Get-FileHash -LiteralPath $Item.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
    status_before = [string]$Sig.Status
    status_after = $null
    signer_after = $null
    signed = $false
    error = $null
  }
}

$Signtool = Get-Command signtool.exe -ErrorAction SilentlyContinue
$Password = if ([string]::IsNullOrWhiteSpace($CertificatePasswordEnv)) { $null } else { [Environment]::GetEnvironmentVariable($CertificatePasswordEnv) }
$CanExecute = (
  $Execute -and
  ($null -ne $Signtool) -and
  (-not [string]::IsNullOrWhiteSpace($CertificatePath)) -and
  (Test-Path -LiteralPath $CertificatePath) -and
  (-not [string]::IsNullOrWhiteSpace($Password))
)

$Plans = @($Artifacts | ForEach-Object { New-ArtifactPlan $_ })

if ($Execute -and -not $CanExecute) {
  $Missing = @()
  if ($null -eq $Signtool) { $Missing += "signtool.exe" }
  if ([string]::IsNullOrWhiteSpace($CertificatePath) -or -not (Test-Path -LiteralPath $CertificatePath)) { $Missing += "certificate_path" }
  if ([string]::IsNullOrWhiteSpace($Password)) { $Missing += "certificate password env '$CertificatePasswordEnv'" }
  throw "Cannot execute signing. Missing: $($Missing -join ', ')."
}

if ($CanExecute) {
  foreach ($Plan in $Plans) {
    if (-not $Plan.exists) {
      continue
    }
    try {
      $Args = @(
        "sign",
        "/fd", "SHA256",
        "/tr", $TimestampUrl,
        "/td", "SHA256",
        "/f", $CertificatePath,
        "/p", $Password,
        $Plan.path
      )
      $Raw = @(& $Signtool.Source @Args 2>&1)
      if ($LASTEXITCODE -ne 0) {
        throw "signtool exited with $LASTEXITCODE. $($Raw -join [Environment]::NewLine)"
      }
      $After = Get-AuthenticodeSignature -LiteralPath $Plan.path
      $Plan.status_after = [string]$After.Status
      $Plan.signer_after = if ($After.SignerCertificate) { $After.SignerCertificate.Subject } else { $null }
      $Plan.signed = ($After.Status -eq "Valid")
    } catch {
      $Plan.error = $_.Exception.Message
    }
  }
}

$Report = [pscustomobject]@{
  ok = if ($Execute) { (@($Plans | Where-Object { $_.exists -and -not $_.signed }).Count -eq 0) } else { $true }
  mode = "windows-release-signing"
  generated_at = (Get-Date).ToString("o")
  execute = [bool]$Execute
  dry_run = -not [bool]$Execute
  signtool = if ($null -ne $Signtool) { $Signtool.Source } else { $null }
  certificate_path_provided = -not [string]::IsNullOrWhiteSpace($CertificatePath)
  certificate_path_exists = (-not [string]::IsNullOrWhiteSpace($CertificatePath)) -and (Test-Path -LiteralPath $CertificatePath)
  certificate_password_env = $CertificatePasswordEnv
  certificate_password_present = -not [string]::IsNullOrWhiteSpace($Password)
  timestamp_url = $TimestampUrl
  artifacts = $Plans
  privacy = "The certificate password value is never written to this report. Use -Execute only when signing credentials are intentionally available."
}

$Stamp = (Get-Date).ToString("yyyyMMdd-HHmmss")
$ReportPath = Join-Path $OutputDir "windows-release-signing-$Stamp.json"
$Report | Add-Member -NotePropertyName report_path -NotePropertyValue $ReportPath
[System.IO.File]::WriteAllText($ReportPath, ($Report | ConvertTo-Json -Depth 8), $Utf8NoBom)
$Report | ConvertTo-Json -Depth 8 -Compress

if (-not $Report.ok) {
  exit 1
}
