param(
  [int]$DtokenPort = 38118,
  [int]$UpstreamPort = 39128
)

$ErrorActionPreference = "Stop"
$Root = Resolve-Path (Join-Path $PSScriptRoot "..\..")
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
            content = str(payload.get("messages", [{}])[0].get("content", ""))
            response = {
                "id": "responses-bridge-1",
                "object": "chat.completion",
                "model": payload.get("model", "mock/model"),
                "choices": [{
                    "index": 0,
                    "message": {"role": "assistant", "content": "responses-bridge-ok:" + str("repeat" in content).lower()},
                    "finish_reason": "stop"
                }],
                "usage": {"prompt_tokens": 9, "completion_tokens": 4, "total_tokens": 13}
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

$MockPath = Join-Path $env:TEMP "dtoken-responses-mock-upstream.py"
[System.IO.File]::WriteAllText($MockPath, $MockScript, $Utf8NoBom)

$ConfigPath = Join-Path $env:TEMP "dtoken-responses-mock-config.json"
$DataDir = Join-Path $env:TEMP ("dtoken-responses-data-" + [guid]::NewGuid().ToString("N"))
$Config = @{
  providers = @(@{
    id = "responses-mock-upstream"
    name = "Responses Mock Upstream"
    kind = "custom"
    protocol = "open_ai"
    base_url = "http://127.0.0.1:$UpstreamPort"
    api_key_ref = $null
    models = @("mock/model")
    timeout_ms = 30000
    healthcheck_path = "/health"
  })
  routes = @(@{
    id = "responses-route"
    name = "Responses Route"
    input_protocol = "open_ai"
    instance_id = "codex-instance"
    target_provider_id = "responses-mock-upstream"
    fallback_provider_id = $null
    chain_mode = "direct"
    compression_policy = "balanced"
    bind_host = "127.0.0.1"
    local_port = $DtokenPort
  })
  instances = @(@{
    id = "codex-instance"
    name = "Codex Responses Instance"
    agent = "Codex CLI"
    route_id = "responses-route"
    local_port = $DtokenPort
    process_status = "stopped"
    log_path = "./logs/responses.jsonl"
  })
} | ConvertTo-Json -Depth 8
[System.IO.File]::WriteAllText($ConfigPath, $Config, $Utf8NoBom)

$MockProcess = Start-Process -FilePath "python" -ArgumentList @($MockPath, "$UpstreamPort") -PassThru -WindowStyle Hidden

Push-Location (Join-Path $Root "code")
try {
  $Out = Join-Path $env:TEMP "dtoken-responses-out.log"
  $Err = Join-Path $env:TEMP "dtoken-responses-err.log"
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

    $Body = @{
      model = "mock/model"
      input = "repeat response route`nrepeat response route`n"
    } | ConvertTo-Json -Depth 8
    $Response = Invoke-RestMethod -Method Post `
      "http://127.0.0.1:$DtokenPort/v1/responses" `
      -ContentType "application/json" `
      -Body $Body
    $Dashboard = Invoke-RestMethod "http://127.0.0.1:$DtokenPort/dashboard/payload"
    $Events = Invoke-RestMethod "http://127.0.0.1:$DtokenPort/events"
    $ExportJson = Invoke-RestMethod "http://127.0.0.1:$DtokenPort/export/json"

    if ($Response.object -ne "response") { throw "responses object mismatch" }
    if ($Response.output[0].content[0].text -ne "responses-bridge-ok:true") { throw "responses text mismatch" }
    if ($Dashboard.route_decisions[0].route_id -ne "responses-route") { throw "dashboard route mismatch" }
    $ProviderForward = @($Events | Where-Object { $_.stage -eq "provider_forward" })
    if ($ProviderForward.Count -lt 1) { throw "missing provider_forward event" }
    if (($Events | ConvertTo-Json -Depth 12) -match "repeat response route") { throw "raw responses input leaked into events" }
    if ($ExportJson.redacted -ne $true) { throw "export json redaction mismatch" }

    [pscustomobject]@{
      health = $Health.status
      object = $Response.object
      route = $Dashboard.route_decisions[0].route_id
      provider = $Dashboard.route_decisions[0].provider_id
      dashboard_source = $Dashboard.source
      persisted_events = (Get-Content (Join-Path $DataDir "events.jsonl")).Count
      provider_forward_events = $ProviderForward.Count
      export_redacted = $ExportJson.redacted
      export_raw_available = $ExportJson.raw_available
      data_dir = $DataDir
    } | ConvertTo-Json -Compress
  } finally {
    Stop-StartedProcess -Process $DtokenProcess -CommandLineNeedle "--port $DtokenPort"
  }
} finally {
  Pop-Location
  Stop-StartedProcess -Process $MockProcess -CommandLineNeedle "dtoken-responses-mock-upstream.py $UpstreamPort"
}
