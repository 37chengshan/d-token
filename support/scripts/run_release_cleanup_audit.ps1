param(
  [string]$OutputDir = ""
)

$ErrorActionPreference = "Stop"

#Requires -Version 7.0

if ($PSVersionTable.PSVersion -lt [version]"7.0") {
  throw "run_release_cleanup_audit.ps1 requires PowerShell 7 or newer (found $($PSVersionTable.PSVersion))."
}

$Root = Resolve-Path (Join-Path $PSScriptRoot "..\..")
$Utf8NoBom = [System.Text.UTF8Encoding]::new($false)
if ([string]::IsNullOrWhiteSpace($OutputDir)) {
  $OutputDir = Join-Path $Root "support\artifacts"
}
New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null

$PortsToCheck = @(18001, 38101, 38113, 38117, 39122)
# Disallowed process names are platform-aware.  On macOS/Linux the OS
# reports bare executable names without a .exe suffix, so the Windows
# list would never match.  Keep both lists in sync when a process name
# is added or removed.
$DisallowedProcessNames = if ($IsWindows) {
  @(
    "dtoken-proxy.exe",
    "opencode.exe",
    "opencode.cmd",
    "cargo.exe",
    "rustc.exe",
    "makensis.exe"
  )
} else {
  @(
    "dtoken-proxy",
    "opencode",
    "cargo",
    "rustc",
    "makensis"
  )
}
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
$Processes = @()
if ($IsWindows) {
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
    ForEach-Object {
      $Name = ([string]$_.Name).ToLowerInvariant()
      $CommandLine = [string]$_.CommandLine
      $MatchedMarker = @($GeneratedPythonMarkers | Where-Object { $CommandLine -like "*$_*" } | Select-Object -First 1)
      if (($DisallowedProcessNames -contains $Name) -or
          (($Name -eq "python.exe" -or $Name -eq "python3.exe" -or $Name -eq "py.exe") -and $MatchedMarker.Count -gt 0)) {
        [pscustomobject]@{
          process_id = $_.ProcessId
          name = $_.Name
          matched_marker = if ($MatchedMarker.Count -gt 0) { $MatchedMarker[0] } else { "disallowed_process_name" }
        }
      }
    })
} else {
  $Lsof = Get-Command "lsof" -ErrorAction SilentlyContinue
  if ($null -ne $Lsof) {
    $Lines = @(& $Lsof.Source -nP -iTCP -sTCP:LISTEN 2>$null)
    foreach ($Line in $Lines | Select-Object -Skip 1) {
      $Fields = @($Line -split "\s+" | Where-Object { -not [string]::IsNullOrWhiteSpace($_) })
      if ($Fields.Count -lt 9) {
        continue
      }
      if ($Line -match ":(\d+)\s+\(LISTEN\)\s*$") {
        $Port = [int]$Matches[1]
        if ($PortsToCheck -contains $Port) {
          $Listeners += [pscustomobject]@{
            process = $Fields[0]
            process_id = [int]$Fields[1]
            local_port = $Port
            state = "LISTEN"
          }
        }
      }
    }
  }

  $Ps = Get-Command "ps" -ErrorAction SilentlyContinue
  if ($null -ne $Ps) {
    foreach ($Line in @(& $Ps.Source -axo "pid=,comm=,args=" 2>$null)) {
      if ($Line -notmatch "^\s*(\d+)\s+(\S+)\s*(.*)$") {
        continue
      }
      $ProcessId = [int]$Matches[1]
      $Name = [System.IO.Path]::GetFileName([string]$Matches[2]).ToLowerInvariant()
      $CommandLine = [string]$Matches[3]
      $MatchedMarker = @($GeneratedPythonMarkers | Where-Object { $CommandLine -like "*$_*" } | Select-Object -First 1)
      if (($DisallowedProcessNames -contains $Name) -or
          (($Name -eq "python" -or $Name -eq "python3" -or $Name -eq "py") -and $MatchedMarker.Count -gt 0)) {
        $Processes += [pscustomobject]@{
          process_id = $ProcessId
          name = $Name
          matched_marker = if ($MatchedMarker.Count -gt 0) { $MatchedMarker[0] } else { "disallowed_process_name" }
        }
      }
    }
  }
}

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
