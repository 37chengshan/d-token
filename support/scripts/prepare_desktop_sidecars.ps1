param(
  [switch]$Release
)

$ErrorActionPreference = "Stop"
$Root = Resolve-Path (Join-Path $PSScriptRoot "..\..")
$CodeRoot = Join-Path $Root "code"
$BinaryDir = Join-Path $Root "code\apps\desktop\src-tauri\binaries"
$TargetTriples = @("x86_64-pc-windows-gnu", "x86_64-pc-windows-msvc")
$SourceName = if ($IsWindows -or $env:OS -eq "Windows_NT") { "dtoken-proxy.exe" } else { "dtoken-proxy" }
$ProfileDir = if ($Release) { "release" } else { "debug" }
$CandidateStampPath = Join-Path $CodeRoot "target\$ProfileDir\d-token-release-candidate.json"

. (Join-Path $PSScriptRoot "release_candidate_provenance.ps1")

Push-Location $CodeRoot
try {
  if ($Release) {
    cargo build -p dtoken-proxy --release | Out-Null
  } else {
    cargo build -p dtoken-proxy | Out-Null
  }
} finally {
  Pop-Location
}

$Source = Join-Path $CodeRoot "target\$ProfileDir\$SourceName"
if (-not (Test-Path -LiteralPath $Source)) {
  throw "dtoken-proxy build artifact was not found at $Source"
}

New-Item -ItemType Directory -Force -Path $BinaryDir | Out-Null
$Destinations = @()
foreach ($TargetTriple in $TargetTriples) {
  $SidecarName = if ($IsWindows -or $env:OS -eq "Windows_NT") { "dtoken-proxy-$TargetTriple.exe" } else { "dtoken-proxy-$TargetTriple" }
  $Destination = Join-Path $BinaryDir $SidecarName
  $Copied = $false
  $LastError = $null
  for ($Attempt = 1; $Attempt -le 10; $Attempt++) {
    try {
      if (Test-Path -LiteralPath $Destination) {
        Remove-Item -LiteralPath $Destination -Force
      }
      Copy-Item -LiteralPath $Source -Destination $Destination -Force
      $Copied = $true
      break
    } catch {
      $LastError = $_.Exception.Message
      Start-Sleep -Milliseconds (250 * $Attempt)
    }
  }
  if (-not $Copied) {
    throw "failed to copy sidecar to $Destination after retries: $LastError"
  }
  $Destinations += [pscustomobject]@{
    path = (Resolve-Path $Destination).Path
    bytes = (Get-Item -LiteralPath $Destination).Length
    target_triple = $TargetTriple
  }
}

$WebView2Loader = $null
if ($IsWindows -or $env:OS -eq "Windows_NT") {
  $WebView2Candidates = @()
  $TargetLoader = Join-Path $CodeRoot "target\$ProfileDir\WebView2Loader.dll"
  if (Test-Path -LiteralPath $TargetLoader) {
    $WebView2Candidates += (Get-Item -LiteralPath $TargetLoader)
  }
  $RegistryRoot = Join-Path $env:USERPROFILE ".cargo\registry\src"
  if (Test-Path -LiteralPath $RegistryRoot) {
    $WebView2Candidates += @(Get-ChildItem -LiteralPath $RegistryRoot -Recurse -Filter "WebView2Loader.dll" -File -ErrorAction SilentlyContinue |
      Where-Object { $_.FullName -match "webview2-com-sys.*\\x64\\WebView2Loader\.dll$" })
  }
  $WebView2Source = @($WebView2Candidates | Sort-Object LastWriteTime -Descending | Select-Object -First 1)[0]
  if ($null -eq $WebView2Source) {
    throw "WebView2Loader.dll was not found. The Windows installer would fail to start the desktop app without it."
  }

  $WebView2Destination = Join-Path $BinaryDir "WebView2Loader.dll"
  Copy-Item -LiteralPath $WebView2Source.FullName -Destination $WebView2Destination -Force
  (Get-Item -LiteralPath $WebView2Destination).LastWriteTime = Get-Date
  $WebView2Loader = [pscustomobject]@{
    source = $WebView2Source.FullName
    path = (Resolve-Path $WebView2Destination).Path
    bytes = (Get-Item -LiteralPath $WebView2Destination).Length
  }
}

$Candidate = $null
if ($Release) {
  $Candidate = Write-ReleaseCandidateStamp -Root $Root -Path $CandidateStampPath -OutputDir (Join-Path $Root "support\artifacts")
}

[pscustomobject]@{
  ok = $true
  mode = "prepare-desktop-sidecars"
  source = (Resolve-Path $Source).Path
  destinations = $Destinations
  webview2_loader = $WebView2Loader
  profile = $ProfileDir
  candidate_id = if ($null -ne $Candidate) { $Candidate.candidate_id } else { $null }
  candidate_stamp = if ($Release) { $CandidateStampPath } else { $null }
} | ConvertTo-Json -Compress
