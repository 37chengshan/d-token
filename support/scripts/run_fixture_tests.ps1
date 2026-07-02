param(
  [int]$Port = 38101
)

$ErrorActionPreference = "Stop"
$Root = Resolve-Path (Join-Path $PSScriptRoot "..\..")
$env:DTOKEN_PROFILE = "fixture"
$env:DTOKEN_FIXTURE_ROOT = Join-Path $Root "support\fixtures"

python (Join-Path $Root "support\scripts\check_structure.py")

Push-Location (Join-Path $Root "code")
try {
  cargo check --workspace

  $hostLine = rustc -vV | Select-String "host:"
  $dlltool = Get-Command dlltool.exe -ErrorAction SilentlyContinue
  $canLinkRustBinary = $hostLine -notmatch "windows-gnu" -or $dlltool
  if ($canLinkRustBinary) {
    cargo test --workspace
  } else {
    Write-Host "Skipping cargo test: windows-gnu toolchain requires dlltool.exe, which is not installed."
  }

  if (-not $canLinkRustBinary) {
    [pscustomobject]@{
      health = "runtime-skipped"
      reason = "windows-gnu toolchain requires dlltool.exe to link cargo test/run"
      structure = "passed"
      cargo_check = "passed"
    } | ConvertTo-Json -Compress
    return
  }

  $out = Join-Path $env:TEMP "dtoken-fixture-$Port-out.log"
  $err = Join-Path $env:TEMP "dtoken-fixture-$Port-err.log"
  $process = Start-Process -FilePath "cargo" `
    -ArgumentList @("run", "-p", "dtoken-proxy", "--", "--port", "$Port", "--config", "configs/config.example.json") `
    -PassThru `
    -WindowStyle Hidden `
    -RedirectStandardOutput $out `
    -RedirectStandardError $err

  try {
    $health = $null
    for ($attempt = 0; $attempt -lt 30; $attempt++) {
      try {
        $health = Invoke-RestMethod "http://127.0.0.1:$Port/health"
        break
      } catch {
        Start-Sleep -Milliseconds 500
      }
    }
    if (-not $health) { throw "fixture proxy did not become healthy on port $Port" }

    $rtk = Invoke-RestMethod "http://127.0.0.1:$Port/rtk/status"
    $gain = Invoke-RestMethod "http://127.0.0.1:$Port/rtk/gain"
    $discover = Invoke-RestMethod "http://127.0.0.1:$Port/rtk/discover"
    $headroom = Invoke-RestMethod "http://127.0.0.1:$Port/headroom/status"
    $history = Invoke-RestMethod "http://127.0.0.1:$Port/stats-history"
    $doctor = Invoke-RestMethod -Method Post "http://127.0.0.1:$Port/doctor/run"

    if ($health.status -ne "healthy") { throw "health check failed" }
    if ($rtk.install_method -ne "fixture") { throw "rtk fixture status failed" }
    if ($gain.source -notlike "*fixture*") { throw "rtk gain fixture source failed" }
    if ($discover.source -ne "fixture") { throw "rtk discover fixture source failed" }
    if ($headroom.version -notlike "*fixture*") { throw "headroom fixture status failed" }
    if ($history.status -ne "fixture") { throw "headroom history fixture failed" }
    if (-not $doctor.ok) { throw "doctor failed" }

    [pscustomobject]@{
      health = $health.status
      rtk = $rtk.install_method
      gain = $gain.source
      discover = $discover.source
      headroom = $headroom.version
      history = $history.status
      doctor = $doctor.ok
    } | ConvertTo-Json -Compress
  } finally {
    if ($process -and -not $process.HasExited) {
      Stop-Process -Id $process.Id -Force
    }
  }
} finally {
  Pop-Location
}
