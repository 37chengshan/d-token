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

function Test-PortFree {
  param([int]$Port)
  $Connections = @(Get-NetTCPConnection -LocalAddress 127.0.0.1 -LocalPort $Port -State Listen -ErrorAction SilentlyContinue)
  return $Connections.Count -eq 0
}

function Invoke-CargoLifecycleTests {
  Push-Location (Join-Path $Root "code")
  try {
    $env:DTOKEN_LIFECYCLE_TEST = "1"
    $global:LASTEXITCODE = 0
    $PreviousErrorActionPreference = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    try {
      $Raw = @(cargo test -p d-token-desktop service_lifecycle -- --nocapture 2>&1)
      $ExitCode = $global:LASTEXITCODE
    } finally {
      $ErrorActionPreference = $PreviousErrorActionPreference
      Remove-Item Env:\DTOKEN_LIFECYCLE_TEST -ErrorAction SilentlyContinue
    }
    [pscustomobject]@{
      exit_code = $ExitCode
      output_tail = @($Raw | ForEach-Object { $_.ToString() } | Select-Object -Last 80)
    }
  } finally {
    Pop-Location
  }
}

$Checks = @()
$PortFreeBefore = Test-PortFree 18001
$Checks += [pscustomobject]@{
  id = "port_free_before"
  ok = $PortFreeBefore
  detail = "127.0.0.1:18001 must be free before lifecycle test so no unmanaged service is touched"
}

$Cargo = $null
if ($PortFreeBefore) {
  $Cargo = Invoke-CargoLifecycleTests
  $Checks += [pscustomobject]@{
    id = "cargo_lifecycle_tests"
    ok = ($Cargo.exit_code -eq 0)
    detail = $Cargo
  }
} else {
  $Checks += [pscustomobject]@{
    id = "cargo_lifecycle_tests"
    ok = $false
    detail = "Skipped because product port was already occupied"
  }
}

Start-Sleep -Milliseconds 300
$PortFreeAfter = Test-PortFree 18001
$Checks += [pscustomobject]@{
  id = "port_free_after"
  ok = $PortFreeAfter
  detail = "127.0.0.1:18001 should be free after managed start/stop verification"
}

$Passed = @($Checks | Where-Object { $_.ok }).Count
$Total = @($Checks).Count
$Report = [pscustomobject]@{
  ok = ($Passed -eq $Total)
  mode = "dtoken-service-lifecycle"
  passed = $Passed
  total = $Total
  summary = "d-token service lifecycle audit: $Passed/$Total checks passed"
  privacy = "Uses only d-token-owned desktop command tests, a managed PID manifest, and 127.0.0.1:18001 health. External Headroom, RTK, cc-switch, shell profiles, and agent auth files are not touched."
  checks = $Checks
}

$Stamp = (Get-Date).ToString("yyyyMMdd-HHmmss")
$ReportPath = Join-Path $OutputDir "dtoken-service-lifecycle-$Stamp.json"
$Report | Add-Member -NotePropertyName report_path -NotePropertyValue $ReportPath
[System.IO.File]::WriteAllText($ReportPath, ($Report | ConvertTo-Json -Depth 10), $Utf8NoBom)
$Report | ConvertTo-Json -Depth 8 -Compress

if (-not $Report.ok) {
  exit 1
}
