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

$PortsToCheck = @(18001, 38101, 38113, 38117, 39122)
$DisallowedProcessNames = @(
  "dtoken-proxy.exe",
  "opencode.exe",
  "opencode.cmd",
  "cargo.exe",
  "rustc.exe",
  "makensis.exe"
)
$GeneratedPythonMarkers = @(
  "dtoken-mock-upstream.py",
  "dtoken-active-config-mock-upstream.py",
  "dtoken-agent-route-verification-upstream.py",
  "dtoken-responses-mock-upstream.py",
  "dtoken-anthropic-bridge-upstream.py"
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

$Listeners = @()
try {
  $Listeners = @(Get-NetTCPConnection -State Listen -ErrorAction SilentlyContinue |
    Where-Object { $PortsToCheck -contains $_.LocalPort } |
    Select-Object LocalAddress, LocalPort, OwningProcess, State)
} catch {
  $Listeners = @([pscustomobject]@{
    error = $_.Exception.Message
  })
}

$Processes = @(Get-CimInstance Win32_Process -ErrorAction SilentlyContinue |
  Where-Object {
    $Name = ([string]$_.Name).ToLowerInvariant()
    $CommandLine = [string]$_.CommandLine
    ($DisallowedProcessNames -contains $Name) -or
    (($Name -eq "python.exe" -or $Name -eq "python3.exe" -or $Name -eq "py.exe") -and
      (@($GeneratedPythonMarkers | Where-Object { $CommandLine -like "*$_*" }).Count -gt 0))
  } |
  Select-Object ProcessId, Name, CommandLine)

$Checks = @(
  New-Check "no_release_test_ports_listening" (@($Listeners).Count -eq 0) @{
    ports = $PortsToCheck
    listeners = $Listeners
  }
  New-Check "no_release_test_processes_left" (@($Processes).Count -eq 0) @{
    disallowed_process_names = $DisallowedProcessNames
    generated_python_markers = $GeneratedPythonMarkers
    processes = $Processes
    note = "The installed/preview d-token desktop window is intentionally not flagged. This audit checks validation/runtime leftovers only."
  }
)

$Passed = @($Checks | Where-Object { $_.ok }).Count
$Total = @($Checks).Count
$Report = [pscustomobject]@{
  ok = ($Passed -eq $Total)
  mode = "release-cleanup-audit"
  generated_at = (Get-Date).ToString("o")
  passed = $Passed
  total = $Total
  privacy = "Read-only process and port cleanup audit. Does not kill processes, mutate services, or inspect secrets."
  external_services = "Does not start, stop, or reconfigure external Headroom, RTK, cc-switch, OpenCode, or user shell configuration."
  checks = $Checks
}

$Stamp = (Get-Date).ToString("yyyyMMdd-HHmmss")
$ReportPath = Join-Path $OutputDir "release-cleanup-audit-$Stamp.json"
$Report | Add-Member -NotePropertyName report_path -NotePropertyValue $ReportPath
[System.IO.File]::WriteAllText($ReportPath, ($Report | ConvertTo-Json -Depth 8), $Utf8NoBom)
$Report | ConvertTo-Json -Depth 8 -Compress

if (-not $Report.ok) {
  exit 1
}
