param(
  [Parameter(Mandatory = $true)]
  [string]$CertificateThumbprint,
  [string]$TimestampUrl = "http://timestamp.digicert.com"
)

$ErrorActionPreference = "Stop"
$Root = Resolve-Path (Join-Path $PSScriptRoot "..\..")
$SidecarPairs = @(
  [pscustomobject]@{
    name = "dtoken-proxy"
    prepared = Join-Path $Root "code\apps\desktop\src-tauri\binaries\dtoken-proxy-x86_64-pc-windows-msvc.exe"
    release = Join-Path $Root "code\target\release\dtoken-proxy.exe"
  },
  [pscustomobject]@{
    name = "dtoken-agent-bridge"
    prepared = Join-Path $Root "code\apps\desktop\src-tauri\binaries\dtoken-agent-bridge-x86_64-pc-windows-msvc.exe"
    release = Join-Path $Root "code\target\release\dtoken-agent-bridge.exe"
  },
  [pscustomobject]@{
    name = "dtoken-codebase-memory-launcher"
    prepared = Join-Path $Root "code\apps\desktop\src-tauri\binaries\dtoken-codebase-memory-launcher-x86_64-pc-windows-msvc.exe"
    release = Join-Path $Root "code\target\release\dtoken-codebase-memory-launcher.exe"
  }
)

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

$Artifacts = @()
$Hashes = @{}
foreach ($Pair in $SidecarPairs) {
  if (-not (Test-Path -LiteralPath $Pair.prepared -PathType Leaf)) {
    throw "The sidecar to sign is missing: $($Pair.prepared)"
  }
  & $SignTool.FullName sign /sha1 $CertificateThumbprint /fd SHA256 /tr $TimestampUrl /td SHA256 $Pair.prepared
  if ($LASTEXITCODE -ne 0) {
    throw "signtool failed for $($Pair.prepared) with exit code $LASTEXITCODE."
  }
  Copy-Item -LiteralPath $Pair.prepared -Destination $Pair.release -Force
  foreach ($Artifact in @($Pair.prepared, $Pair.release)) {
    $Signature = Get-AuthenticodeSignature -LiteralPath $Artifact
    if ($Signature.Status -ne "Valid") {
      throw "Authenticode verification failed for ${Artifact}: $($Signature.Status)"
    }
    $Artifacts += $Artifact
  }
  $PreparedHash = (Get-FileHash -LiteralPath $Pair.prepared -Algorithm SHA256).Hash.ToLowerInvariant()
  $ReleaseHash = (Get-FileHash -LiteralPath $Pair.release -Algorithm SHA256).Hash.ToLowerInvariant()
  if ($PreparedHash -ne $ReleaseHash) {
    throw "Signed sidecar copies do not have identical bytes for $($Pair.name)."
  }
  $Hashes[$Pair.name] = $PreparedHash
}

[pscustomobject]@{
  ok = $true
  mode = "sign-windows-sidecars"
  signtool = $SignTool.FullName
  sha256 = $Hashes
  artifacts = $Artifacts
} | ConvertTo-Json -Compress
