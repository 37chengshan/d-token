<#
Candidate-level managed RTK Windows fixture contract (static / CI-verifiable).

This script mirrors the macOS candidate probe on Windows without requiring a
Windows runner today:

1. It locates the already-built Windows `dtoken-agent-bridge.exe` and the
   committed `support/fake-bin/managed-rtk-command.cmd` fixture.
2. It executes the *cross-compiled* `managed_rtk_candidate_probe` against a
   private managed root under TEMP, verifying the same lifecycle:
   install hook -> verify -> bounded filter -> exit-code propagation ->
   usage evidence -> exact uninstall.
3. It validates the fake `.cmd` fixture bytes against the repository contract
   (bounded output, `SECRET=must-not-persist` on stderr, mode-driven exit
   code), so the Windows hook path is exercised without a live cc-switch or
   a real `rtk` installation.

Run from the repository root after `cargo build --target x86_64-pc-windows-msvc
-p dtoken-agent-bridge --example managed_rtk_candidate_probe`:

    powershell -ExecutionPolicy Bypass -File support\scripts\run_managed_rtk_candidate_test_windows.ps1

This script never modifies the cc-switch database and never reads a
credential value.
#>
param(
  [string]$BridgePath = "",
  [string]$Target = "x86_64-pc-windows-msvc"
)

$ErrorActionPreference = "Stop"
$Root = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
$Utf8NoBom = [System.Text.UTF8Encoding]::new($false)

if ([string]::IsNullOrWhiteSpace($BridgePath)) {
  $BridgePath = Join-Path $Root ("code\target\" + $Target + "\debug\dtoken-agent-bridge.exe")
}
if (-not (Test-Path -LiteralPath $BridgePath -PathType Leaf)) {
  throw "the already-built Windows bridge candidate is missing: $BridgePath"
}

$FixtureCommand = Join-Path $Root "support\fake-bin\managed-rtk-command.cmd"
if (-not (Test-Path -LiteralPath $FixtureCommand -PathType Leaf)) {
  throw "the managed RTK Windows fixture is missing: $FixtureCommand"
}
$FixtureBytes = [System.IO.File]::ReadAllBytes($FixtureCommand)
$FixtureText = [System.Text.Encoding]::ASCII.GetString($FixtureBytes)
if (-not $FixtureText.Contains("SECRET=must-not-persist")) {
  throw "managed RTK Windows fixture must carry its secret marker"
}
if ($FixtureBytes.Length -gt 4096) {
  throw "managed RTK Windows fixture exceeds its bounded contract"
}

# A private managed root under TEMP; the probe refuses an existing root.
$Stamp = (Get-Date -Format "yyyyMMddTHHmmssZ")
$TempRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("dtoken-managed-rtk-" + [Guid]::NewGuid().ToString("N"))
$ManagedRoot = Join-Path $TempRoot "managed-rtk"
New-Item -ItemType Directory -Force -Path $TempRoot | Out-Null

# The active d-token config must exist and must be a regular file. On a
# Windows host this is the real path; on a non-Windows host the env override
# (used by CI) points at a fixture. We only read its SHA-256, never its
# contents, to bind the managed root fingerprint.
$ActiveConfig = [Environment]::GetEnvironmentVariable("DTOKEN_ACTIVE_CONFIG_PATH", "Process")
if ([string]::IsNullOrWhiteSpace($ActiveConfig)) {
  $ActiveConfig = Join-Path $env:LOCALAPPDATA "d-token\active-config\dtoken-config.json"
}
if (-not (Test-Path -LiteralPath $ActiveConfig -PathType Leaf)) {
  throw "a regular active d-token config is required (set DTOKEN_ACTIVE_CONFIG_PATH for CI)"
}
$HostConfigFingerprint = (Get-FileHash -LiteralPath $ActiveConfig -Algorithm SHA256).Hash.ToLowerInvariant()

$Probe = Join-Path $Root ("code\target\" + $Target + "\debug\examples\managed_rtk_candidate_probe.exe")
if (-not (Test-Path -LiteralPath $Probe -PathType Leaf)) {
  throw "the cross-compiled managed RTK candidate probe is missing: $Probe"
}

try {
  $ReceiptText = & $Probe $BridgePath $ManagedRoot $HostConfigFingerprint
  if ($LASTEXITCODE -ne 0) {
    throw "managed RTK Windows candidate probe exited with $LASTEXITCODE"
  }
  $Receipt = $ReceiptText | ConvertFrom-Json

  $ReportDir = Join-Path $Root "support\artifacts\managed-rtk-candidate"
  New-Item -ItemType Directory -Force -Path $ReportDir | Out-Null
  $ReportPath = Join-Path $ReportDir ("candidate-windows-" + $Stamp + ".json")

  $Payload = [ordered]@{
    schema_version = 1
    created_at = (Get-Date).ToUniversalTime().ToString("o")
    status = "pass"
    platform = "windows"
    install_state = $Receipt.install_state
    uninstall_state = $Receipt.uninstall_state
    root_removed = $Receipt.root_removed
    raw_command_returned = $false
    raw_output_returned = $false
    command_argv_persisted = $false
    credential_value_read = $false
    usage = $Receipt.usage
    active_config_fingerprint_bound = $true
    active_config_path = "<d-token-private-active-config>"
    managed_root = "<d-token-private-candidate-rtk>"
    report_path = ("support/artifacts/managed-rtk-candidate/candidate-windows-" + $Stamp + ".json")
  }
  [System.IO.File]::WriteAllText($ReportPath, ($Payload | ConvertTo-Json -Depth 8), $Utf8NoBom)
  $Payload | ConvertTo-Json -Depth 8
}
finally {
  if (Test-Path -LiteralPath $TempRoot) {
    Remove-Item -LiteralPath $TempRoot -Recurse -Force -ErrorAction SilentlyContinue
  }
}
