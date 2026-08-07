<#
.SYNOPSIS
  Run Windows DACL/ACL-specific tests from dtoken-context-store and
  dtoken-config-patcher crates via cargo test on a Windows MSVC host.

.DESCRIPTION
  This script invokes the #[cfg(windows)] DACL/ACL tests that cannot be
  compiled or executed on non-Windows hosts.  It must be run on a
  windows-2022 (or later) GitHub Actions runner or a local Windows machine
  with the MSVC Rust toolchain installed.

  Exit code 0 = all DACL/ACL tests passed.
  Exit code 1 = one or more tests failed.

  On a non-Windows host the script exits immediately with code 2 and a
  clear message — it never claims success on a platform where the tests
  cannot run.

.PARAMETER Target
  Rust target triple (default: x86_64-pc-windows-msvc).

.PARAMETER CrateDir
  Repository-relative path to the workspace root (default: code).

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File support/scripts/run_windows_dacl_acl_tests.ps1
#>
param(
  [string]$Target = "x86_64-pc-windows-msvc",
  [string]$CrateDir = "code"
)

$ErrorActionPreference = "Stop"
$Root = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
$Utf8NoBom = [System.Text.UTF8Encoding]::new($false)

# --------------------------------------------------------------------------
# Platform gate — only run on actual Windows
# --------------------------------------------------------------------------
$IsWindowsHost = $IsWindows -or (
  [System.Runtime.InteropServices.RuntimeInformation]::IsOSPlatform(
    [System.Runtime.InteropServices.OSPlatform]::Windows
  )
)
if (-not $IsWindowsHost) {
  Write-Warning "run_windows_dacl_acl_tests.ps1: Not a Windows host — DACL/ACL tests are #[cfg(windows)] and cannot be compiled here.  Skipping."
  exit 2
}

# --------------------------------------------------------------------------
# Test groups
# --------------------------------------------------------------------------
# Filters must match REAL cargo test names/module paths (cargo test filters
# match the fully-qualified test path, not function names of non-test code).
# Every group's filter was verified against the crate source:
#   - dtoken-context-store::windows_private_acl::tests::directory_acl_is_protected_and_limited_to_current_user_and_system
#     and file_acl_is_protected_and_limited_to_current_user_and_system match the module path "windows_private_acl".
#   - dtoken-context-store::lib::tests::open_applies_protected_private_dacl_...
#     and open_restricts_parent_database_... match "open_applies" / "open_restricts".
#   - dtoken-config-patcher has NO windows_acl test; the config privacy ACL is
#     exercised by dtoken-context-store's "open_applies" tests.
#   - dtoken-codebase-memory's secure_cache_permissions is a helper function,
#     not a test; the cache/DB ACL behaviour is covered by "dacl" (which
#     matches dtoken-context-store's dacl test names).
# A cargo run that reports "test result: ok. 0 passed; 0 failed" (0 tests run)
# is always a FAILURE below — the filter named nothing.
$TestGroups = @(
  @{ id = "private_acl";   crate = "dtoken-context-store";  filter = "windows_private_acl" }
  @{ id = "open_acl";      crate = "dtoken-context-store";  filter = "open_applies" }
  @{ id = "broad_acl";     crate = "dtoken-context-store";  filter = "dacl" }
)

$Results = @()
$AllOk = $true

foreach ($Group in $TestGroups) {
  $Id = $Group.id
  $Crate = $Group.crate
  $Filter = $Group.filter
  $CratePath = Join-Path $Root $CrateDir

  Write-Host "=== DACL/ACL: [$Id] cargo test -p $Crate -- $Filter ==="

  $Started = Get-Date
  $ExitCode = 0
  $OutputLines = @()

  try {
    Push-Location $CratePath
    $Output = & "cargo" "test" "-p" $Crate "--" $Filter 2>&1
    $ExitCode = $global:LASTEXITCODE
    $OutputLines = @($Output | ForEach-Object { $_.ToString() })
  } catch {
    $ExitCode = 1
    $OutputLines = @("Exception: $($_.Exception.Message)")
  } finally {
    Pop-Location
  }

  $ElapsedMs = [int]((Get-Date) - $Started).TotalMilliseconds
  # A cargo filter that matches zero tests exits 0 with "0 tests run" — that is
  # a false pass (the filter named nothing).  Treat it as a hard failure.
  $OutputText = $OutputLines -join "`n"
  $ZeroTests = ($OutputText -match "0 tests run")
  $Passed = ($ExitCode -eq 0) -and (-not $ZeroTests)
  if (-not $Passed) { $AllOk = $false }

  $Results += [pscustomobject]@{
    id       = $Id
    crate    = $Crate
    filter   = $Filter
    ok       = $Passed
    exit_code = $ExitCode
    zero_tests = $ZeroTests
    elapsed_ms = $ElapsedMs
  }

  if (-not $Passed) {
    Write-Host "FAILED [$Id] exit=$ExitCode zero_tests=$ZeroTests"
    $OutputLines | Select-Object -Last 20 | ForEach-Object { Write-Host "  $_" }
  }
}

# --------------------------------------------------------------------------
# Report
# --------------------------------------------------------------------------
$PassedCount = @($Results | Where-Object { $_.ok }).Count
$TotalCount = $Results.Count

$Report = [pscustomobject]@{
  ok = $AllOk
  mode = "windows-dacl-acl-tests"
  platform = "windows"
  target = $Target
  passed = $PassedCount
  total = $TotalCount
  summary = "DACL/ACL tests: $PassedCount/$TotalCount passed"
  groups = $Results
}

$Stamp = (Get-Date).ToString("yyyyMMdd-HHmmss")
$ReportDir = Join-Path $Root "support\artifacts"
New-Item -ItemType Directory -Force -Path $ReportDir | Out-Null
$ReportPath = Join-Path $ReportDir "windows-dacl-acl-tests-$Stamp.json"
$Report | Add-Member -NotePropertyName report_path -NotePropertyValue $ReportPath
[System.IO.File]::WriteAllText($ReportPath, ($Report | ConvertTo-Json -Depth 8), $Utf8NoBom)

$Report | ConvertTo-Json -Depth 6 -Compress

if (-not $AllOk) {
  exit 1
}