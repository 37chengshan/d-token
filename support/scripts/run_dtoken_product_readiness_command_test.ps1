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

$Started = Get-Date
Push-Location (Join-Path $Root "code")
try {
  $global:LASTEXITCODE = 0
  $PreviousErrorActionPreference = $ErrorActionPreference
  $ErrorActionPreference = "Continue"
  try {
    $Raw = @(cargo test -p d-token-desktop product_readiness_check_passes_with_dtoken_runtime_metadata 2>&1)
    $ExitCode = $global:LASTEXITCODE
  } finally {
    $ErrorActionPreference = $PreviousErrorActionPreference
  }
} finally {
  Pop-Location
}

$ElapsedMs = [int]((Get-Date) - $Started).TotalMilliseconds
$OutputText = ($Raw | ForEach-Object { $_.ToString() }) -join [Environment]::NewLine
$Ok = $ExitCode -eq 0
$Report = [pscustomobject]@{
  ok = $Ok
  mode = "product-readiness-command"
  elapsed_ms = $ElapsedMs
  command = "cargo test -p d-token-desktop product_readiness_check_passes_with_dtoken_runtime_metadata"
  summary = if ($Ok) {
    "Product readiness command passed with d-token active config and d-token runtime metadata"
  } else {
    "Product readiness command failed"
  }
  covered = @(
    "install_connect",
    "agent_detection",
    "route_config",
    "compression_data",
    "hook_or_config_effect",
    "auto_policy_write",
    "headroom_derived_capabilities",
    "export_redaction",
    "doctor"
  )
  expected_checks = 9
  runtime_source = "dtoken-owned-runtime"
  active_config_source = "d-token active config"
  privacy = "No raw prompts, raw responses, API keys, browser credentials, or external Headroom/RTK/cc-switch service state are read or stored."
  external_services = "Does not start, stop, reconfigure, or depend on external Headroom, RTK, or cc-switch services."
  output_line_count = @($Raw).Count
}

$Stamp = (Get-Date).ToString("yyyyMMdd-HHmmss")
$ReportPath = Join-Path $OutputDir "product-readiness-command-$Stamp.json"
$Report | Add-Member -NotePropertyName report_path -NotePropertyValue $ReportPath
[System.IO.File]::WriteAllText($ReportPath, ($Report | ConvertTo-Json -Depth 8), $Utf8NoBom)
$Report | ConvertTo-Json -Depth 8 -Compress

if (-not $Ok) {
  Write-Error $OutputText
  exit 1
}
