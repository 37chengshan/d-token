param(
  [Parameter(Mandatory = $true)]
  [string]$CertificateThumbprint,
  [string]$TimestampUrl = "http://timestamp.digicert.com"
)

$ErrorActionPreference = "Stop"
$Root = Resolve-Path (Join-Path $PSScriptRoot "..\..")
$PreparedSidecar = Join-Path $Root "code\apps\desktop\src-tauri\binaries\dtoken-proxy-x86_64-pc-windows-msvc.exe"
$ReleaseSidecar = Join-Path $Root "code\target\release\dtoken-proxy.exe"

if ([string]::IsNullOrWhiteSpace($CertificateThumbprint)) {
  throw "A Windows signing certificate thumbprint is required."
}

$WindowsSdkRoot = Join-Path ${env:ProgramFiles(x86)} "Windows Kits\10\bin"
$SignTool = @(Get-ChildItem -LiteralPath $WindowsSdkRoot -Recurse -Filter "signtool.exe" -File -ErrorAction SilentlyContinue |
  Where-Object { $_.FullName -match '\\x64\\signtool\.exe$' } |
  Sort-Object FullName -Descending |
  Select-Object -First 1)[0]
if ($null -eq $SignTool) {
  throw "signtool.exe was not found in the Windows 10 SDK."
}

$Artifacts = @($PreparedSidecar, $ReleaseSidecar)
if (-not (Test-Path -LiteralPath $PreparedSidecar -PathType Leaf)) {
  throw "The sidecar to sign is missing: $PreparedSidecar"
}

& $SignTool.FullName sign /sha1 $CertificateThumbprint /fd SHA256 /tr $TimestampUrl /td SHA256 $PreparedSidecar
if ($LASTEXITCODE -ne 0) {
  throw "signtool failed for $PreparedSidecar with exit code $LASTEXITCODE."
}

Copy-Item -LiteralPath $PreparedSidecar -Destination $ReleaseSidecar -Force
foreach ($Artifact in $Artifacts) {
  $Signature = Get-AuthenticodeSignature -LiteralPath $Artifact
  if ($Signature.Status -ne "Valid") {
    throw "Authenticode verification failed for $Artifact: $($Signature.Status)"
  }
}

$PreparedHash = (Get-FileHash -LiteralPath $PreparedSidecar -Algorithm SHA256).Hash.ToLowerInvariant()
$ReleaseHash = (Get-FileHash -LiteralPath $ReleaseSidecar -Algorithm SHA256).Hash.ToLowerInvariant()
if ($PreparedHash -ne $ReleaseHash) {
  throw "Signed sidecar copies do not have identical bytes."
}

[pscustomobject]@{
  ok = $true
  mode = "sign-windows-sidecars"
  signtool = $SignTool.FullName
  sha256 = $PreparedHash
  artifacts = $Artifacts
} | ConvertTo-Json -Compress
