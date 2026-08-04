param(
  [Parameter(Mandatory = $true)][string]$CandidateId,
  [string]$OutputDir = "",
  [string]$RegistryPath = "",
  [int]$Port = 38129
)

$ErrorActionPreference = "Stop"
$Root = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
$Utf8NoBom = [System.Text.UTF8Encoding]::new($false)

if ($CandidateId -cnotmatch '^[0-9a-f]{64}$') {
  throw "-CandidateId must be exactly 64 lowercase hexadecimal characters"
}
if ($Port -lt 1024 -or $Port -gt 65535) {
  throw "-Port must be between 1024 and 65535"
}
if ([string]::IsNullOrWhiteSpace($OutputDir)) {
  $OutputDir = Join-Path $Root "support\artifacts"
} elseif (-not [System.IO.Path]::IsPathRooted($OutputDir)) {
  $OutputDir = Join-Path $Root $OutputDir
}
New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null
if ([string]::IsNullOrWhiteSpace($RegistryPath)) {
  $RegistryPath = Join-Path $OutputDir ("capability-registry-" + $CandidateId + ".json")
} elseif (-not [System.IO.Path]::IsPathRooted($RegistryPath)) {
  $RegistryPath = Join-Path $OutputDir $RegistryPath
}
$RegistryDirectory = Split-Path -Parent $RegistryPath
if (-not [string]::IsNullOrWhiteSpace($RegistryDirectory)) {
  New-Item -ItemType Directory -Force -Path $RegistryDirectory | Out-Null
}

$IsWindowsHost = [System.Runtime.InteropServices.RuntimeInformation]::IsOSPlatform(
  [System.Runtime.InteropServices.OSPlatform]::Windows
)
$ExecutableName = if ($IsWindowsHost) { "dtoken-proxy.exe" } else { "dtoken-proxy" }
$ExecutablePath = Join-Path $Root ("code\target\debug\" + $ExecutableName)
if (-not (Test-Path -LiteralPath $ExecutablePath -PathType Leaf)) {
  throw "the already-built dtoken-proxy candidate is missing: $ExecutablePath"
}

$TempRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("dtoken-capability-registry-" + [Guid]::NewGuid().ToString("N"))
$ConfigPath = Join-Path $TempRoot "explicit-config.json"
$DataDir = Join-Path $TempRoot "runtime-data"
$StdoutPath = Join-Path $TempRoot "runtime.stdout.log"
$StderrPath = Join-Path $TempRoot "runtime.stderr.log"
$RuntimeNonce = [Guid]::NewGuid().ToString("N")
$PreviousNonce = [Environment]::GetEnvironmentVariable("DTOKEN_RUNTIME_INSTANCE_NONCE", "Process")
$RuntimeProcess = $null

New-Item -ItemType Directory -Force -Path $TempRoot | Out-Null
New-Item -ItemType Directory -Force -Path $DataDir | Out-Null
$Config = [pscustomobject]@{
  providers = @()
  routes = @()
  instances = @()
}
[System.IO.File]::WriteAllText($ConfigPath, ($Config | ConvertTo-Json -Depth 4), $Utf8NoBom)

try {
  [Environment]::SetEnvironmentVariable("DTOKEN_RUNTIME_INSTANCE_NONCE", $RuntimeNonce, "Process")
  $RuntimeProcess = Start-Process `
    -FilePath $ExecutablePath `
    -ArgumentList @("--host", "127.0.0.1", "--port", "$Port", "--config", $ConfigPath, "--data-dir", $DataDir) `
    -PassThru `
    -RedirectStandardOutput $StdoutPath `
    -RedirectStandardError $StderrPath

  $Health = $null
  for ($Attempt = 0; $Attempt -lt 40; $Attempt++) {
    if ($RuntimeProcess.HasExited) {
      throw "candidate dtoken-proxy exited before its loopback health endpoint became ready"
    }
    try {
      $Health = Invoke-RestMethod -Uri "http://127.0.0.1:$Port/health" -Method Get -TimeoutSec 2
      if (
        [string]$Health.runtime_identity -eq "dtoken-runtime-local" -and
        [string]$Health.component -eq "dtoken-core-api" -and
        [string]$Health.runtime_instance_nonce -eq $RuntimeNonce
      ) {
        break
      }
      $Health = $null
    } catch {
      $Health = $null
    }
    Start-Sleep -Milliseconds 250
  }

  if ($null -eq $Health) {
    throw "candidate dtoken-proxy did not present the expected identity and nonce on 127.0.0.1:$Port"
  }

  $ActiveConfig = Invoke-RestMethod -Uri "http://127.0.0.1:$Port/config/active" -Method Get -TimeoutSec 2
  if ([string]$ActiveConfig.config.source -ne "explicit_cli_config") {
    throw "candidate dtoken-proxy did not load the explicit metadata-only config"
  }

  $Registry = Invoke-RestMethod -Uri "http://127.0.0.1:$Port/capabilities" -Method Get -TimeoutSec 2
  if ($null -eq $Registry) {
    throw "candidate dtoken-proxy returned no capability registry"
  }

  $RegistryJson = $Registry | ConvertTo-Json -Depth 16
  if ([System.Text.Encoding]::UTF8.GetByteCount($RegistryJson) -gt 1MB) {
    throw "candidate capability registry exceeds the metadata-only size limit"
  }
  $TemporaryRegistryPath = "$RegistryPath.tmp-$([Guid]::NewGuid().ToString('N'))"
  try {
    [System.IO.File]::WriteAllText($TemporaryRegistryPath, $RegistryJson, $Utf8NoBom)
    Move-Item -LiteralPath $TemporaryRegistryPath -Destination $RegistryPath -Force
  } finally {
    if (Test-Path -LiteralPath $TemporaryRegistryPath) {
      Remove-Item -LiteralPath $TemporaryRegistryPath -Force
    }
  }

  [pscustomobject]@{
    ok = $true
    candidate_id = $CandidateId
    runtime_identity = $Health.runtime_identity
    runtime_component = $Health.component
    service_url = "http://127.0.0.1:$Port"
    config_source = $ActiveConfig.config.source
    registry_revision = $Registry.revision
    registry_schema_version = $Registry.schema_version
    registry_collected_at_ms = $Registry.collected_at_ms
    capability_count = @($Registry.capabilities).Count
    registry_path = $RegistryPath
    executable_sha256 = (Get-FileHash -LiteralPath $ExecutablePath -Algorithm SHA256).Hash.ToLowerInvariant()
    privacy = "Metadata-only /capabilities snapshot; no requests, prompts, responses, credentials, or auth files were read."
  } | ConvertTo-Json -Compress
} finally {
  if ($RuntimeProcess -and -not $RuntimeProcess.HasExited) {
    Stop-Process -Id $RuntimeProcess.Id -Force -ErrorAction SilentlyContinue
    Start-Sleep -Milliseconds 200
  }
  if ($null -eq $PreviousNonce) {
    [Environment]::SetEnvironmentVariable("DTOKEN_RUNTIME_INSTANCE_NONCE", $null, "Process")
  } else {
    [Environment]::SetEnvironmentVariable("DTOKEN_RUNTIME_INSTANCE_NONCE", $PreviousNonce, "Process")
  }
  if (Test-Path -LiteralPath $TempRoot) {
    Remove-Item -LiteralPath $TempRoot -Recurse -Force -ErrorAction SilentlyContinue
  }
}
