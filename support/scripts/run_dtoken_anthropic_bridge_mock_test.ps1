param(
  [int]$DtokenPort = 38114,
  [int]$UpstreamPort = 39124
)

$ErrorActionPreference = "Stop"
$Root = Resolve-Path (Join-Path $PSScriptRoot "..\..")
$Utf8NoBom = [System.Text.UTF8Encoding]::new($false)

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
            content = payload.get("messages", [{}])[0].get("content", "")
            response = {
                "id": "mock-bridge-1",
                "object": "chat.completion",
                "model": payload.get("model", "mock/model"),
                "choices": [{
                    "index": 0,
                    "message": {"role": "assistant", "content": "anthropic-bridge-ok:" + str("repeated x2" in content).lower()},
                    "finish_reason": "stop"
                }],
                "usage": {"prompt_tokens": 11, "completion_tokens": 5, "total_tokens": 16}
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

$MockPath = Join-Path $env:TEMP "dtoken-anthropic-bridge-mock.py"
[System.IO.File]::WriteAllText($MockPath, $MockScript, $Utf8NoBom)

$ConfigPath = Join-Path $env:TEMP "dtoken-anthropic-bridge-config.json"
$DataDir = Join-Path $env:TEMP ("dtoken-anthropic-bridge-data-" + [guid]::NewGuid().ToString("N"))
$Config = @{
  providers = @(@{
    id = "cc-switch-openai"
    name = "cc-switch OpenAI Bridge"
    kind = "local_chain"
    protocol = "open_ai"
    base_url = "http://127.0.0.1:$UpstreamPort"
    api_key_ref = $null
    models = @("mock/model")
    timeout_ms = 30000
    healthcheck_path = "/health"
  })
  routes = @(@{
    id = "anthropic-to-cc-switch"
    name = "Anthropic to cc-switch"
    input_protocol = "anthropic"
    instance_id = "claude-code-instance"
    target_provider_id = "cc-switch-openai"
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
    route_id = "anthropic-to-cc-switch"
    local_port = $DtokenPort
    process_status = "stopped"
    log_path = "./logs/anthropic-bridge.jsonl"
  })
} | ConvertTo-Json -Depth 8
[System.IO.File]::WriteAllText($ConfigPath, $Config, $Utf8NoBom)

$MockProcess = Start-Process -FilePath "python" -ArgumentList @($MockPath, "$UpstreamPort") -PassThru -WindowStyle Hidden

Push-Location (Join-Path $Root "code")
try {
  $Out = Join-Path $env:TEMP "dtoken-anthropic-bridge-out.log"
  $Err = Join-Path $env:TEMP "dtoken-anthropic-bridge-err.log"
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
      model = "claude-3-5-sonnet-latest"
      max_tokens = 64
      messages = @(@{
        role = "user"
        content = "2026-07-04T10:00:00Z INFO bridge`n2026-07-04T10:00:00Z INFO bridge`n"
      })
    } | ConvertTo-Json -Depth 8
    $Message = Invoke-RestMethod -Method Post `
      "http://127.0.0.1:$DtokenPort/v1/messages" `
      -ContentType "application/json" `
      -Body $Body
    $Dashboard = Invoke-RestMethod "http://127.0.0.1:$DtokenPort/dashboard/payload"
    $Ccr = Invoke-RestMethod "http://127.0.0.1:$DtokenPort/ccr/summary"

    if ($Message.type -ne "message") { throw "anthropic response type mismatch" }
    if ($Message.content[0].text -ne "anthropic-bridge-ok:true") { throw "bridge response mismatch" }
    if ($Dashboard.route_decisions[0].route_id -ne "anthropic-to-cc-switch") { throw "dashboard route mismatch" }
    if ($Dashboard.request_logs[0].input_protocol -ne "anthropic") { throw "dashboard protocol mismatch" }
    foreach ($Name in @("events.jsonl", "route-decisions.jsonl", "request-logs.jsonl", "ccr-metadata.jsonl")) {
      $Path = Join-Path $DataDir $Name
      if (-not (Test-Path $Path)) { throw "missing persistent runtime file: $Name" }
      if ((Get-Content $Path).Count -lt 1) { throw "empty persistent runtime file: $Name" }
    }

    [pscustomobject]@{
      health = $Health.status
      type = $Message.type
      content = $Message.content[0].text
      route = $Dashboard.route_decisions[0].route_id
      provider = $Dashboard.route_decisions[0].provider_id
      input_protocol = $Dashboard.request_logs[0].input_protocol
      output_protocol = $Dashboard.events[0].metadata.output_protocol
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
  if ($MockProcess -and -not $MockProcess.HasExited) {
    Stop-Process -Id $MockProcess.Id -Force
  }
}
