param(
  [int]$DtokenPort = 38113,
  [int]$UpstreamPort = 39122
)

$ErrorActionPreference = "Stop"
$Root = Resolve-Path (Join-Path $PSScriptRoot "..\..")
$Utf8NoBom = [System.Text.UTF8Encoding]::new($false)

function Stop-StartedProcess {
  param(
    [object]$Process,
    [string]$CommandLineNeedle
  )

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

$MockScript = @'
import json, sys
from http.server import BaseHTTPRequestHandler, HTTPServer

class Handler(BaseHTTPRequestHandler):
    def do_GET(self):
        if self.path == "/health":
            self.send_response(200)
            self.end_headers()
            self.wfile.write(b'{"status":"healthy"}')
            return
        self.send_response(404)
        self.end_headers()

    def do_POST(self):
        length = int(self.headers.get("content-length", "0"))
        body = self.rfile.read(length)
        if self.path.endswith("/v1/chat/completions"):
            payload = json.loads(body or b"{}")
            response = {
                "id": "mock-upstream-1",
                "object": "chat.completion",
                "model": payload.get("model", "mock/model"),
                "choices": [{
                    "index": 0,
                    "message": {"role": "assistant", "content": "mock-upstream-ok"},
                    "finish_reason": "stop"
                }],
                "usage": {"prompt_tokens": 7, "completion_tokens": 3, "total_tokens": 10}
            }
            raw = json.dumps(response).encode("utf-8")
            self.send_response(200)
            self.send_header("content-type", "application/json")
            self.send_header("content-length", str(len(raw)))
            self.end_headers()
            self.wfile.write(raw)
            return
        self.send_response(404)
        self.end_headers()

    def log_message(self, _format, *args):
        pass

HTTPServer(("127.0.0.1", int(sys.argv[1])), Handler).serve_forever()
'@

$MockPath = Join-Path $env:TEMP "dtoken-mock-upstream.py"
[System.IO.File]::WriteAllText($MockPath, $MockScript, $Utf8NoBom)

$ConfigPath = Join-Path $env:TEMP "dtoken-routed-mock-config.json"
$DataDir = Join-Path $env:TEMP ("dtoken-routed-mock-data-" + [guid]::NewGuid().ToString("N"))
$Config = @{
  providers = @(@{
    id = "mock-upstream"
    name = "Mock Upstream"
    kind = "custom"
    protocol = "open_ai"
    base_url = "http://127.0.0.1:$UpstreamPort"
    api_key_ref = $null
    models = @("mock/model")
    timeout_ms = 30000
    healthcheck_path = "/health"
  })
  routes = @(@{
    id = "opencode-routed"
    name = "OpenCode Routed"
    input_protocol = "open_ai"
    instance_id = "opencode-instance"
    target_provider_id = "mock-upstream"
    fallback_provider_id = $null
    chain_mode = "direct"
    compression_policy = "balanced"
    bind_host = "127.0.0.1"
    local_port = $DtokenPort
  })
  instances = @(@{
    id = "opencode-instance"
    name = "OpenCode Instance"
    agent = "OpenCode"
    route_id = "opencode-routed"
    local_port = $DtokenPort
    process_status = "stopped"
    log_path = "./logs/opencode.jsonl"
  })
} | ConvertTo-Json -Depth 8
[System.IO.File]::WriteAllText($ConfigPath, $Config, $Utf8NoBom)

$MockProcess = Start-Process -FilePath "python" -ArgumentList @($MockPath, "$UpstreamPort") -PassThru -WindowStyle Hidden

Push-Location (Join-Path $Root "code")
try {
  $Out = Join-Path $env:TEMP "dtoken-routed-mock-out.log"
  $Err = Join-Path $env:TEMP "dtoken-routed-mock-err.log"
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
      model = "mock/model"
      messages = @(@{
        role = "user"
        content = "repeat log line`nrepeat log line`nroute through d-token"
      })
    } | ConvertTo-Json -Depth 8
    $Chat = Invoke-RestMethod -Method Post `
      "http://127.0.0.1:$DtokenPort/v1/chat/completions" `
      -ContentType "application/json" `
      -Body $Body
    $Dashboard = Invoke-RestMethod "http://127.0.0.1:$DtokenPort/dashboard/payload"
    $Ccr = Invoke-RestMethod "http://127.0.0.1:$DtokenPort/ccr/summary"
    $MemoryBody = @{
      project_id = "mock-project"
      agent = "OpenCode"
      kind = "verbosity"
      summary = "Prefer terse mock route output."
      output_verbosity = "terse"
      raw_prompt = "must not persist"
    } | ConvertTo-Json -Depth 8
    $MemoryLearn = Invoke-RestMethod -Method Post `
      "http://127.0.0.1:$DtokenPort/memory/learn" `
      -ContentType "application/json" `
      -Body $MemoryBody
    $Memory = Invoke-RestMethod "http://127.0.0.1:$DtokenPort/memory/summary"
    $OutputShape = Invoke-RestMethod "http://127.0.0.1:$DtokenPort/output-shape"

    if ($Chat.choices[0].message.content -ne "mock-upstream-ok") { throw "upstream response mismatch" }
    if ($Dashboard.route_decisions[0].route_id -ne "opencode-routed") { throw "dashboard route mismatch" }
    if ($Dashboard.route_decisions[0].provider_id -ne "mock-upstream") { throw "dashboard provider mismatch" }
    if (-not $MemoryLearn.ok) { throw "memory learn failed" }
    if ($Memory.entries -lt 1) { throw "memory summary did not record entry" }
    if (($Memory | ConvertTo-Json -Depth 8) -match "must not persist") { throw "memory raw prompt leaked" }
    if ($OutputShape.source -ne "dtoken-owned-runtime") { throw "output-shape source mismatch" }
    if ($OutputShape.raw_available -ne $false) { throw "output-shape raw availability mismatch" }
    if ($OutputShape.output_verbosity -ne "terse") { throw "output-shape did not reflect learned verbosity" }
    foreach ($Name in @("events.jsonl", "route-decisions.jsonl", "request-logs.jsonl", "ccr-metadata.jsonl", "memory-records.jsonl")) {
      $Path = Join-Path $DataDir $Name
      if (-not (Test-Path $Path)) { throw "missing persistent runtime file: $Name" }
      if ((Get-Content $Path).Count -lt 1) { throw "empty persistent runtime file: $Name" }
    }

    [pscustomobject]@{
      health = $Health.status
      content = $Chat.choices[0].message.content
      route = $Dashboard.route_decisions[0].route_id
      provider = $Dashboard.route_decisions[0].provider_id
      upstream_status = 200
      saved_tokens = $Dashboard.request_logs[0].tokens_saved
      dashboard_source = $Dashboard.source
      dashboard_route = $Dashboard.route_decisions[0].route_id
      dashboard_agent = $Dashboard.request_logs[0].agent
      persisted_events = (Get-Content (Join-Path $DataDir "events.jsonl")).Count
      persisted_routes = (Get-Content (Join-Path $DataDir "route-decisions.jsonl")).Count
      persisted_logs = (Get-Content (Join-Path $DataDir "request-logs.jsonl")).Count
      persisted_ccr = (Get-Content (Join-Path $DataDir "ccr-metadata.jsonl")).Count
      persisted_memory = (Get-Content (Join-Path $DataDir "memory-records.jsonl")).Count
      ccr_entries = $Ccr.entries
      ccr_raw_available = $Ccr.raw_available
      memory_entries = $Memory.entries
      output_verbosity = $OutputShape.output_verbosity
      output_shape_raw_available = $OutputShape.raw_available
      data_dir = $DataDir
    } | ConvertTo-Json -Compress
  } finally {
    Stop-StartedProcess -Process $DtokenProcess -CommandLineNeedle "--port $DtokenPort"
  }
} finally {
  Pop-Location
  Stop-StartedProcess -Process $MockProcess -CommandLineNeedle "dtoken-mock-upstream.py $UpstreamPort"
}
