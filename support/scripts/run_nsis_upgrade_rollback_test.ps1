<#
.SYNOPSIS
  Verify NSIS installer upgrade, clean uninstall, and rollback behaviour on a
  Windows host.

.DESCRIPTION
  This script validates the NSIS installer's old-version -> new-version ->
  rollback contract:
    1. Clean-install the PREVIOUS public installer (old version).
    2. Verify the previous version's installed files.
    3. Upgrade-install the current candidate over the previous version.
    4. Verify the installed state after the upgrade.
    5. Launch the installed desktop app and verify startup.
    6. Clean-uninstall and verify directory removal.
    7. Rollback: restore the pre-test state and verify it.

  Every step produces a structured JSON check; the rollback step is never a
  skipped pass — when no installation existed before the test, rollback means
  verifying the clean state.  If any step fails the script exits 1.

  On a non-Windows host the script exits 2 with a clear skip message.

.PARAMETER SilentInstallLatest
  Pass -SilentInstallLatest to auto-detect the latest NSIS installer (the NEW
  candidate) in the build output directory.

.PARAMETER PreviousInstaller
  Path to an OLDER public NSIS installer used as the previous version for the
  upgrade and rollback contract.  Required — a same-candidate reinstall is not
  an upgrade test, and a skipped rollback is not a pass.

.PARAMETER InstallerPath
  Explicit path to the NEW candidate NSIS installer .exe.  When provided,
  -SilentInstallLatest is ignored.

.PARAMETER InstalledDir
  The expected installation directory (default: $env:LOCALAPPDATA\d-token).

.PARAMETER StartupTimeoutSeconds
  How long to wait for the app's main window (default: 20).

.PARAMETER OutputDir
  Where to write the JSON report (default: support/artifacts).

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File support/scripts/run_nsis_upgrade_rollback_test.ps1 -SilentInstallLatest -PreviousInstaller C:\old\d-token_0.1.0_x64-setup.exe
#>
param(
  [switch]$SilentInstallLatest,
  [string]$PreviousInstaller = "",
  [string]$InstallerPath = "",
  [string]$InstalledDir = "",
  [int]$StartupTimeoutSeconds = 20,
  [string]$OutputDir = ""
)

$ErrorActionPreference = "Stop"
$Root = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
$Utf8NoBom = [System.Text.UTF8Encoding]::new($false)
if ([string]::IsNullOrWhiteSpace($OutputDir)) {
  $OutputDir = Join-Path $Root "support\artifacts"
}
if ([string]::IsNullOrWhiteSpace($InstalledDir)) {
  $InstalledDir = Join-Path $env:LOCALAPPDATA "d-token"
}
New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null

# --------------------------------------------------------------------------
# Platform gate
# --------------------------------------------------------------------------
$IsWindowsHost = $IsWindows -or (
  [System.Runtime.InteropServices.RuntimeInformation]::IsOSPlatform(
    [System.Runtime.InteropServices.OSPlatform]::Windows
  )
)
if (-not $IsWindowsHost) {
  Write-Warning "run_nsis_upgrade_rollback_test.ps1: Not a Windows host — NSIS installer cannot run.  Skipping."
  exit 2
}

# --------------------------------------------------------------------------
# Helpers
# --------------------------------------------------------------------------
function New-Check {
  param([string]$Id, [bool]$Ok, [object]$Detail)
  [pscustomobject]@{ id = $Id; ok = $Ok; detail = $Detail }
}

function Invoke-NsisStep {
  param([string]$Id, [scriptblock]$Command)
  $Started = Get-Date
  try {
    & $Command
    $Elapsed = [int]((Get-Date) - $Started).TotalMilliseconds
    return New-Check $Id $true @{ elapsed_ms = $Elapsed }
  } catch {
    return New-Check $Id $false @{ error = $_.Exception.Message }
  }
}

function Wait-ForProcessWindow {
  param([string]$ProcessName, [int]$TimeoutSeconds)
  $Deadline = (Get-Date).AddSeconds($TimeoutSeconds)
  while ((Get-Date) -lt $Deadline) {
    $procs = @(Get-Process -Name $ProcessName -ErrorAction SilentlyContinue)
    if ($procs.Count -gt 0) {
      Start-Sleep -Seconds 2
      return $procs[0]
    }
    Start-Sleep -Milliseconds 500
  }
  return $null
}

# --------------------------------------------------------------------------
# Resolve installer
# --------------------------------------------------------------------------
if ([string]::IsNullOrWhiteSpace($InstallerPath) -and $SilentInstallLatest) {
  $Pattern = Join-Path $Root "code\target\x86_64-pc-windows-msvc\release\bundle\nsis\d-token_*_x64-setup.exe"
  $Candidates = @(Get-ChildItem -Path $Pattern -File -ErrorAction SilentlyContinue | Sort-Object LastWriteTime -Descending)
  if ($Candidates.Count -eq 0) {
    throw "No NSIS installer found matching: $Pattern"
  }
  $InstallerPath = $Candidates[0].FullName
}
if ([string]::IsNullOrWhiteSpace($InstallerPath) -or -not (Test-Path -LiteralPath $InstallerPath -PathType Leaf)) {
  throw "A valid NEW candidate NSIS installer path is required. Use -SilentInstallLatest or -InstallerPath."
}
if ([string]::IsNullOrWhiteSpace($PreviousInstaller) -or -not (Test-Path -LiteralPath $PreviousInstaller -PathType Leaf)) {
  throw "A valid PREVIOUS NSIS installer path is required via -PreviousInstaller. The old-version -> new-version -> rollback gate cannot be verified without an older installer."
}
if ([System.IO.Path]::GetFullPath($PreviousInstaller) -eq [System.IO.Path]::GetFullPath($InstallerPath)) {
  throw "The previous installer and the new candidate installer are the same file — that is a reinstall, not an upgrade/rollback test."
}
$InstallerItem = Get-Item -LiteralPath $InstallerPath
$InstallerHash = (Get-FileHash -LiteralPath $InstallerPath -Algorithm SHA256).Hash.ToLowerInvariant()
$PreviousInstallerItem = Get-Item -LiteralPath $PreviousInstaller
$PreviousInstallerHash = (Get-FileHash -LiteralPath $PreviousInstaller -Algorithm SHA256).Hash.ToLowerInvariant()

# --------------------------------------------------------------------------
# Backup existing installation data before modifying
# --------------------------------------------------------------------------
$BackupDir = Join-Path $env:TEMP "d-token-nsis-backup-$([Guid]::NewGuid().ToString('N'))"
$InstalledPreviously = Test-Path -LiteralPath $InstalledDir
if ($InstalledPreviously) {
  Write-Host "Backing up existing installation to: $BackupDir"
  Copy-Item -LiteralPath $InstalledDir -Destination $BackupDir -Recurse -Force
}

$Checks = @()

try {
  # ---- Step 1: Clean-install the PREVIOUS version -----------------------------
  $Checks += Invoke-NsisStep "clean_install" {
    Write-Host "=== Step 1: Clean install of PREVIOUS version ==="
    if (Test-Path -LiteralPath $InstalledDir) {
      Remove-Item -LiteralPath $InstalledDir -Recurse -Force
    }
    $proc = Start-Process -FilePath $PreviousInstaller -ArgumentList "/S" -PassThru -Wait -NoNewWindow
    if ($proc.ExitCode -ne 0) {
      throw "Previous-version install failed with exit code $($proc.ExitCode)"
    }
    if (-not (Test-Path -LiteralPath $InstalledDir)) {
      throw "Installed directory not found after previous-version install: $InstalledDir"
    }
  }

  # ---- Step 2: Verify installed files (previous version) ----------------------
  $Checks += Invoke-NsisStep "verify_installed_files" {
    Write-Host "=== Step 2: Verify installed files (previous version) ==="
    $Required = @(
      "d-token-desktop.exe",
      "dtoken-proxy.exe",
      "dtoken-agent-bridge.exe",
      "WebView2Loader.dll",
      "uninstall.exe"
    )
    $Missing = @()
    foreach ($File in $Required) {
      $Path = Join-Path $InstalledDir $File
      if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        $Missing += $File
      }
    }
    if ($Missing.Count -gt 0) {
      throw "Missing installed files: $($Missing -join ', ')"
    }
  }

  # ---- Step 3: Upgrade-install the NEW candidate over the previous version ----
  $Checks += Invoke-NsisStep "upgrade_reinstall" {
    Write-Host "=== Step 3: Upgrade install of NEW candidate over previous version ==="
    Get-Process -Name "d-token-desktop" -ErrorAction SilentlyContinue | Stop-Process -Force
    $proc = Start-Process -FilePath $InstallerPath -ArgumentList "/S" -PassThru -Wait -NoNewWindow
    if ($proc.ExitCode -ne 0) {
      throw "Upgrade install of new candidate failed with exit code $($proc.ExitCode)"
    }
    if (-not (Test-Path -LiteralPath $InstalledDir)) {
      throw "Installed directory missing after upgrade install"
    }
  }

  # ---- Step 4: Verify installed files after upgrade ---------------------------
  $Checks += Invoke-NsisStep "verify_after_reinstall" {
    Write-Host "=== Step 4: Verify after upgrade ==="
    $Required = @(
      "d-token-desktop.exe",
      "dtoken-proxy.exe",
      "dtoken-agent-bridge.exe",
      "WebView2Loader.dll",
      "uninstall.exe"
    )
    $Missing = @()
    foreach ($File in $Required) {
      $Path = Join-Path $InstalledDir $File
      if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        $Missing += $File
      }
    }
    if ($Missing.Count -gt 0) {
      throw "Missing files after upgrade: $($Missing -join ', ')"
    }
  }

  # ---- Step 5: Launch the upgraded installed app ------------------------------
  $Checks += Invoke-NsisStep "launch_installed_app" {
    Write-Host "=== Step 5: Launch upgraded installed app ==="
    $AppExe = Join-Path $InstalledDir "d-token-desktop.exe"
    if (-not (Test-Path -LiteralPath $AppExe)) {
      throw "Installed desktop app not found: $AppExe"
    }
    $AppProcess = Start-Process -FilePath $AppExe -PassThru
    $Found = Wait-ForProcessWindow -ProcessName "d-token-desktop" -TimeoutSeconds $StartupTimeoutSeconds
    if ($null -eq $Found) {
      if (-not $AppProcess.HasExited) { $AppProcess.Kill() }
      throw "d-token-desktop did not start within $StartupTimeoutSeconds seconds"
    }
    # Allow the window to settle, then kill
    Start-Sleep -Seconds 3
    if (-not $Found.HasExited) { $Found.Kill() }
  }

  # ---- Step 6: Clean uninstall ------------------------------------------------
  $Checks += Invoke-NsisStep "clean_uninstall" {
    Write-Host "=== Step 6: Clean uninstall ==="
    $Uninstaller = Join-Path $InstalledDir "uninstall.exe"
    if (-not (Test-Path -LiteralPath $Uninstaller)) {
      throw "Uninstaller not found: $Uninstaller"
    }
    $proc = Start-Process -FilePath $Uninstaller -ArgumentList "/S" -PassThru -Wait -NoNewWindow
    if ($proc.ExitCode -ne 0) {
      throw "Silent uninstall failed with exit code $($proc.ExitCode)"
    }
    # Allow a moment for the uninstaller to clean up
    Start-Sleep -Seconds 2
    if (Test-Path -LiteralPath $InstalledDir) {
      # Some NSIS installers leave the directory but remove files; check for
      # the main binary as the authoritative indicator
      $RemainingExe = Join-Path $InstalledDir "d-token-desktop.exe"
      if (Test-Path -LiteralPath $RemainingExe) {
        throw "d-token-desktop.exe still exists after uninstall"
      }
    }
  }

  # ---- Step 7: Rollback — restore the pre-test state and VERIFY it ------------
  $Checks += Invoke-NsisStep "rollback_restore" {
    Write-Host "=== Step 7: Rollback restore ==="
    if ($InstalledPreviously) {
      # Remove the uninstalled directory and restore backup
      if (Test-Path -LiteralPath $InstalledDir) {
        Remove-Item -LiteralPath $InstalledDir -Recurse -Force
      }
      Copy-Item -LiteralPath $BackupDir -Destination $InstalledDir -Recurse -Force
      if (-not (Test-Path -LiteralPath (Join-Path $InstalledDir "d-token-desktop.exe"))) {
        throw "Rollback restore did not produce d-token-desktop.exe"
      }
    } else {
      # Nothing was installed before the test, so the correct rollback is a
      # clean state.  This is a VERIFIED assertion, never a skipped pass.
      $RemainingExe = Join-Path $InstalledDir "d-token-desktop.exe"
      if (Test-Path -LiteralPath $RemainingExe) {
        throw "Rollback to clean state failed: d-token-desktop.exe is still present"
      }
    }
  }
}
finally {
  # Cleanup: remove backup directory
  if (Test-Path -LiteralPath $BackupDir) {
    Remove-Item -LiteralPath $BackupDir -Recurse -Force -ErrorAction SilentlyContinue
  }
}

# --------------------------------------------------------------------------
# Report
# --------------------------------------------------------------------------
$Passed = @($Checks | Where-Object { $_.ok }).Count
$Total = $Checks.Count
$AllOk = ($Passed -eq $Total)

$Report = [pscustomobject]@{
  ok = $AllOk
  mode = "nsis-upgrade-rollback-test"
  platform = "windows"
  installer_path = $InstallerPath
  installer_sha256 = $InstallerHash
  installer_bytes = $InstallerItem.Length
  previous_installer_path = $PreviousInstaller
  previous_installer_sha256 = $PreviousInstallerHash
  passed = $Passed
  total = $Total
  summary = "NSIS upgrade/rollback test: $Passed/$Total checks passed"
  checks = $Checks
}

$Stamp = (Get-Date).ToString("yyyyMMdd-HHmmss")
$ReportPath = Join-Path $OutputDir "nsis-upgrade-rollback-test-$Stamp.json"
$Report | Add-Member -NotePropertyName report_path -NotePropertyValue $ReportPath
[System.IO.File]::WriteAllText($ReportPath, ($Report | ConvertTo-Json -Depth 8), $Utf8NoBom)

$Report | ConvertTo-Json -Depth 6 -Compress

if (-not $AllOk) {
  exit 1
}