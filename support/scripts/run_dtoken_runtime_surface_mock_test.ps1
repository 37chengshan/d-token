param(
  [int]$DtokenPort = 38119
)

$ErrorActionPreference = "Stop"
$Root = Resolve-Path (Join-Path $PSScriptRoot "..\..")
$DataDir = Join-Path $env:TEMP ("dtoken-surface-data-" + [guid]::NewGuid().ToString("N"))
$ConfigPath = Join-Path $env:TEMP "dtoken-surface-config.json"
$Utf8NoBom = [System.Text.UTF8Encoding]::new($false)

function Stop-StartedProcess {
  param([object]$Process, [string]$CommandLineNeedle)
  if ($Process -and -not $Process.HasExited) {
    Stop-Process -Id $Process.Id -Force -ErrorAction SilentlyContinue
    Start-Sleep -Milliseconds 200
  }
  if (-not [string]::IsNullOrWhiteSpace($CommandLineNeedle)) {
    Get-CimInstance Win32_Process |
      Where-Object { $_.CommandLine -like "*$CommandLineNeedle*" } |
      ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }
  }
}

$Config = @{
  providers = @(@{
    id = "cc-switch-interface"
    name = "cc-switch Interface"
    kind = "local_chain"
    protocol = "open_ai"
    base_url = "http://127.0.0.1:39999"
    api_key_ref = $null
    models = @("mock/model")
    timeout_ms = 30000
    healthcheck_path = "/health"
  })
  routes = @(@{
    id = "chain-bridge-route"
    name = "Chain Bridge Route"
    input_protocol = "anthropic"
    instance_id = "claude-code-instance"
    target_provider_id = "cc-switch-interface"
    fallback_provider_id = $null
    chain_mode = "chain_bridge"
    compression_policy = "balanced"
    bind_host = "127.0.0.1"
    local_port = $DtokenPort
  })
  instances = @(@{
    id = "claude-code-instance"
    name = "Claude Code Instance"
    agent = "Claude Code"
    route_id = "chain-bridge-route"
    local_port = $DtokenPort
    process_status = "stopped"
    log_path = "./logs/chain.jsonl"
  })
} | ConvertTo-Json -Depth 8
[System.IO.File]::WriteAllText($ConfigPath, $Config, $Utf8NoBom)

Push-Location (Join-Path $Root "code")
try {
  $Out = Join-Path $env:TEMP "dtoken-surface-out.log"
  $Err = Join-Path $env:TEMP "dtoken-surface-err.log"
  $Exe = Join-Path (Get-Location) "target\debug\dtoken-proxy.exe"
  cargo build -p dtoken-proxy | Out-Null

  $DtokenProcess = Start-Process -FilePath $Exe `
    -ArgumentList @("--port", "$DtokenPort", "--config", $ConfigPath, "--data-dir", $DataDir) `
    -PassThru `
    -WindowStyle Hidden `
    -RedirectStandardOutput $Out `
    -RedirectStandardError $Err

  try {
    $Health = $null
    for ($Attempt = 0; $Attempt -lt 40; $Attempt++) {
      try { $Health = Invoke-RestMethod "http://127.0.0.1:$DtokenPort/health"; break } catch { Start-Sleep -Milliseconds 500 }
    }
    if (-not $Health) { throw "dtoken-proxy did not become healthy on $DtokenPort" }

    $Compat = Invoke-RestMethod "http://127.0.0.1:$DtokenPort/compatibility/agents"
    $Protocols = Invoke-RestMethod "http://127.0.0.1:$DtokenPort/protocol-injection"
    $Chain = Invoke-RestMethod "http://127.0.0.1:$DtokenPort/chain/status"
    $PatchBody = @{ agent = "OpenCode"; scope = "route-profile" } | ConvertTo-Json -Depth 4
    $PatchPlan = Invoke-RestMethod -Method Post `
      "http://127.0.0.1:$DtokenPort/config/patches/plan" `
      -ContentType "application/json" `
      -Body $PatchBody
    $Events = Invoke-RestMethod "http://127.0.0.1:$DtokenPort/events"

    if (@($Compat).Count -lt 3) { throw "compatibility matrix too small" }
    if (-not (@($Protocols) | Where-Object { $_.protocol -eq "open_ai" })) { throw "missing OpenAI protocol profile" }
    if (-not (@($Protocols) | Where-Object { $_.protocol -eq "anthropic" })) { throw "missing Anthropic protocol profile" }
    if ($Chain.status -ne "configured") { throw "chain status not configured" }
    if (@($Chain.routes).Count -lt 1) { throw "chain route missing" }
    if (($PatchPlan | ConvertTo-Json -Depth 12) -match "api_key|Authorization|Cookie|Bearer|sk-") { throw "patch plan leaked credential-shaped text" }
    if (-not (@($Events) | Where-Object { $_.stage -eq "config_patch" })) { throw "config_patch event missing" }

    [pscustomobject]@{
      health = $Health.status
      compatibility_agents = @($Compat).Count
      protocol_profiles = @($Protocols).Count
      chain_status = $Chain.status
      chain_routes = @($Chain.routes).Count
      config_patch_events = @($Events | Where-Object { $_.stage -eq "config_patch" }).Count
      patch_plan_redacted = $true
      data_dir = $DataDir
    } | ConvertTo-Json -Compress
  } finally {
    Stop-StartedProcess -Process $DtokenProcess -CommandLineNeedle "--port $DtokenPort"
  }
} finally {
  Pop-Location
}
