param(
  [int]$DtokenPort = 18001,
  [string]$Model = "deepseek-v4-flash-free"
)

$ErrorActionPreference = "Stop"
$Root = Resolve-Path (Join-Path $PSScriptRoot "..\..")
$Utf8NoBom = [System.Text.UTF8Encoding]::new($false)

$ConfigPath = Join-Path $env:TEMP "dtoken-opencode-real-stream-upstream.json"
$DataDir = Join-Path $env:TEMP ("dtoken-opencode-real-stream-data-" + [guid]::NewGuid().ToString("N"))
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
    id = "opencode-real-streaming"
    name = "OpenCode Real Streaming"
    input_protocol = "open_ai"
    instance_id = "opencode-real-streaming-instance"
    target_provider_id = "opencode-zen"
    fallback_provider_id = $null
    chain_mode = "direct"
    compression_policy = "balanced"
    bind_host = "127.0.0.1"
    local_port = $DtokenPort
  })
  instances = @(@{
    id = "opencode-real-streaming-instance"
    name = "OpenCode Real Streaming Instance"
    agent = "OpenCode"
    route_id = "opencode-real-streaming"
    local_port = $DtokenPort
    process_status = "stopped"
    log_path = "./logs/opencode-real-streaming.jsonl"
  })
} | ConvertTo-Json -Depth 8
[System.IO.File]::WriteAllText($ConfigPath, $Config, $Utf8NoBom)

Push-Location (Join-Path $Root "code")
try {
  $Out = Join-Path $env:TEMP "dtoken-opencode-real-stream-out.log"
  $Err = Join-Path $env:TEMP "dtoken-opencode-real-stream-err.log"
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

    $Body = @{
      model = $Model
      stream = $true
      messages = @(@{
        role = "user"
        content = "Reply with a very short sentence containing exactly this marker: d-token-stream-ok. repeated stream log`nrepeated stream log"
      })
    } | ConvertTo-Json -Depth 8

    $RequestBodyPath = Join-Path $env:TEMP "dtoken-opencode-real-stream-request.json"
    $ResponseHeaderPath = Join-Path $env:TEMP "dtoken-opencode-real-stream-headers.txt"
    $ResponseBodyPath = Join-Path $env:TEMP "dtoken-opencode-real-stream-body.txt"
    [System.IO.File]::WriteAllText($RequestBodyPath, $Body, $Utf8NoBom)

    $Started = Get-Date
    & curl.exe -sS -N --max-time 120 `
      -D $ResponseHeaderPath `
      -o $ResponseBodyPath `
      -X POST "http://127.0.0.1:$DtokenPort/v1/chat/completions" `
      -H "Content-Type: application/json" `
      --data-binary "@$RequestBodyPath"
    if ($LASTEXITCODE -ne 0) { throw "curl streaming request failed with exit code $LASTEXITCODE" }
    $ElapsedMs = [int]((Get-Date) - $Started).TotalMilliseconds
    $HeaderText = Get-Content $ResponseHeaderPath -Raw
    $StatusLine = @($HeaderText -split "`r?`n" | Where-Object { $_ -match "^HTTP/" })[-1]
    $StatusCode = [int]([regex]::Match($StatusLine, "HTTP/\S+\s+(\d+)").Groups[1].Value)
    $ContentType = [string](@($HeaderText -split "`r?`n" | Where-Object { $_ -match "^content-type:" })[-1] -replace "^content-type:\s*", "")
    $StreamText = Get-Content $ResponseBodyPath -Raw
    $Dashboard = Invoke-RestMethod "http://127.0.0.1:$DtokenPort/dashboard/payload"
    $Export = Invoke-RestMethod "http://127.0.0.1:$DtokenPort/export/json"

    if ($StatusCode -ne 200) { throw "streaming response status was $StatusCode" }
    if ($ContentType -notlike "*text/event-stream*") { throw "streaming content type was not text/event-stream: $ContentType" }
    if ($StreamText -notmatch "data:") { throw "streaming body did not contain SSE data frames" }
    if ($Dashboard.route_decisions[0].route_id -ne "opencode-real-streaming") {
      throw "dashboard did not record opencode-real-streaming"
    }
    if ($Dashboard.route_decisions[0].provider_id -ne "opencode-zen") {
      throw "dashboard did not record opencode-zen"
    }
    if ($Dashboard.request_logs[0].error) {
      throw "request log recorded error: $($Dashboard.request_logs[0].error)"
    }
    if ($Export.raw_available -ne $false) {
      throw "runtime export unexpectedly exposes raw payloads"
    }
    foreach ($Name in @("events.jsonl", "route-decisions.jsonl", "request-logs.jsonl", "ccr-metadata.jsonl")) {
      $Path = Join-Path $DataDir $Name
      if (-not (Test-Path $Path)) { throw "missing persistent runtime file: $Name" }
      if ((Get-Content $Path).Count -lt 1) { throw "empty persistent runtime file: $Name" }
    }

    [pscustomobject]@{
      health = $Health.status
      model = $Model
      route = $Dashboard.route_decisions[0].route_id
      provider = $Dashboard.route_decisions[0].provider_id
      status_code = $StatusCode
      content_type = $ContentType
      response_contains_sse_data = ($StreamText -match "data:")
      response_contains_done = ($StreamText -match "\[DONE\]")
      response_contains_expected = ($StreamText -match "d-token-stream-ok")
      elapsed_ms = $ElapsedMs
      saved_tokens = $Dashboard.request_logs[0].tokens_saved
      input_tokens_before = $Dashboard.request_logs[0].input_tokens_before
      input_tokens_after = $Dashboard.request_logs[0].input_tokens_after
      transforms = $Dashboard.request_logs[0].transforms_applied
      dashboard_source = $Dashboard.source
      persisted_events = (Get-Content (Join-Path $DataDir "events.jsonl")).Count
      persisted_routes = (Get-Content (Join-Path $DataDir "route-decisions.jsonl")).Count
      persisted_logs = (Get-Content (Join-Path $DataDir "request-logs.jsonl")).Count
      persisted_ccr = (Get-Content (Join-Path $DataDir "ccr-metadata.jsonl")).Count
      export_redacted = $Export.redacted
      export_raw_available = $Export.raw_available
      data_dir = $DataDir
    } | ConvertTo-Json -Depth 8 -Compress
  } finally {
    if ($DtokenProcess -and -not $DtokenProcess.HasExited) {
      Stop-Process -Id $DtokenProcess.Id -Force
    }
  }
} finally {
  Pop-Location
}
