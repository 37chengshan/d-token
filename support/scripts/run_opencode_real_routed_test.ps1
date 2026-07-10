param(
  [int]$DtokenPort = 18001,
  [string]$Model = "dtoken/deepseek-v4-flash-free"
)

$ErrorActionPreference = "Stop"
$Root = Resolve-Path (Join-Path $PSScriptRoot "..\..")
$Utf8NoBom = [System.Text.UTF8Encoding]::new($false)

$ConfigPath = Join-Path $env:TEMP "dtoken-opencode-real-upstream.json"
$DataDir = Join-Path $env:TEMP ("dtoken-opencode-real-data-" + [guid]::NewGuid().ToString("N"))
$Config = @{
  providers = @(@{
    id = "opencode-zen"
    name = "OpenCode Zen"
    kind = "custom"
    protocol = "open_ai"
    base_url = "https://opencode.ai/zen/v1"
    api_key_ref = $null
    models = @("deepseek-v4-flash-free", "mimo-v2.5-free")
    timeout_ms = 120000
    healthcheck_path = "/health"
  })
  routes = @(@{
    id = "opencode-real-routed"
    name = "OpenCode Real Routed"
    input_protocol = "open_ai"
    instance_id = "opencode-real-instance"
    target_provider_id = "opencode-zen"
    fallback_provider_id = $null
    chain_mode = "direct"
    compression_policy = "balanced"
    bind_host = "127.0.0.1"
    local_port = $DtokenPort
  })
  instances = @(@{
    id = "opencode-real-instance"
    name = "OpenCode Real Instance"
    agent = "OpenCode"
    route_id = "opencode-real-routed"
    local_port = $DtokenPort
    process_status = "stopped"
    log_path = "./logs/opencode-real.jsonl"
  })
} | ConvertTo-Json -Depth 8
[System.IO.File]::WriteAllText($ConfigPath, $Config, $Utf8NoBom)

Push-Location (Join-Path $Root "code")
try {
  $Out = Join-Path $env:TEMP "dtoken-opencode-real-out.log"
  $Err = Join-Path $env:TEMP "dtoken-opencode-real-err.log"
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
      try {
        $Health = Invoke-RestMethod "http://127.0.0.1:$DtokenPort/health"
        break
      } catch {
        Start-Sleep -Milliseconds 500
      }
    }
    if (-not $Health) { throw "dtoken-proxy did not become healthy on $DtokenPort" }

    Pop-Location
    $OpOut = Join-Path $env:TEMP "opencode-dtoken-run-out.txt"
    $OpErr = Join-Path $env:TEMP "opencode-dtoken-run-err.txt"
    $OpenCodeProcess = Start-Process -FilePath "opencode.cmd" `
      -ArgumentList @("run", "-m", $Model, "Reply with exactly: d-token-ok") `
      -PassThru `
      -WindowStyle Hidden `
      -RedirectStandardOutput $OpOut `
      -RedirectStandardError $OpErr `
      -WorkingDirectory $Root
    if (-not $OpenCodeProcess.WaitForExit(120000)) {
      Stop-Process -Id $OpenCodeProcess.Id -Force
      throw "OpenCode timed out"
    }
    Push-Location (Join-Path $Root "code")

    $Dashboard = Invoke-RestMethod "http://127.0.0.1:$DtokenPort/dashboard/payload"
    $Ccr = Invoke-RestMethod "http://127.0.0.1:$DtokenPort/ccr/summary"
    $Stdout = Get-Content $OpOut -Raw -ErrorAction SilentlyContinue
    $Stderr = Get-Content $OpErr -Raw -ErrorAction SilentlyContinue
    $Text = (($Stdout + "`n" + $Stderr) -replace "`e\[[0-9;]*m", "").Trim()

    if ($Dashboard.route_decisions[0].route_id -ne "opencode-real-routed") {
      throw "dashboard did not record opencode-real-routed"
    }
    if ($Dashboard.request_logs[0].agent -ne "OpenCode") {
      throw "dashboard did not record OpenCode agent"
    }
    foreach ($Name in @("events.jsonl", "route-decisions.jsonl", "request-logs.jsonl", "ccr-metadata.jsonl")) {
      $Path = Join-Path $DataDir $Name
      if (-not (Test-Path $Path)) { throw "missing persistent runtime file: $Name" }
      if ((Get-Content $Path).Count -lt 1) { throw "empty persistent runtime file: $Name" }
    }

    [pscustomobject]@{
      health = $Health.status
      model = $Model
      opencode_output = $Text
      route = $Dashboard.route_decisions[0].route_id
      provider = $Dashboard.route_decisions[0].provider_id
      agent = $Dashboard.request_logs[0].agent
      saved_tokens = $Dashboard.request_logs[0].tokens_saved
      dashboard_source = $Dashboard.source
      persisted_events = (Get-Content (Join-Path $DataDir "events.jsonl")).Count
      persisted_routes = (Get-Content (Join-Path $DataDir "route-decisions.jsonl")).Count
      persisted_logs = (Get-Content (Join-Path $DataDir "request-logs.jsonl")).Count
      persisted_ccr = (Get-Content (Join-Path $DataDir "ccr-metadata.jsonl")).Count
      ccr_entries = $Ccr.entries
      ccr_raw_available = $Ccr.raw_available
      data_dir = $DataDir
    } | ConvertTo-Json -Compress
  } finally {
    if ($DtokenProcess -and -not $DtokenProcess.HasExited) {
      Stop-Process -Id $DtokenProcess.Id -Force
    }
  }
} finally {
  Pop-Location
}
