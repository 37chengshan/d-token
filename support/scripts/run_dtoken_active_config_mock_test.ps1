param(
  [int]$DtokenPort = 38115,
  [int]$UpstreamPort = 39126
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
    def do_POST(self):
        length = int(self.headers.get("content-length", "0"))
        body = self.rfile.read(length)
        if self.path.endswith("/v1/chat/completions"):
            payload = json.loads(body or b"{}")
            raw = json.dumps({
                "id": "active-config-mock-1",
                "object": "chat.completion",
                "model": payload.get("model", "mock/model"),
                "choices": [{
                    "index": 0,
                    "message": {"role": "assistant", "content": "active-config-ok"},
                    "finish_reason": "stop"
                }],
                "usage": {"prompt_tokens": 5, "completion_tokens": 3, "total_tokens": 8}
            }).encode("utf-8")
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

$MockPath = Join-Path $env:TEMP "dtoken-active-config-mock-upstream.py"
[System.IO.File]::WriteAllText($MockPath, $MockScript, $Utf8NoBom)

$ActiveRoot = Join-Path $env:TEMP ("dtoken-active-config-root-" + [guid]::NewGuid().ToString("N"))
$env:LOCALAPPDATA = $ActiveRoot
$ActiveConfigDir = Join-Path $ActiveRoot "d-token\active-config"
$DataDir = Join-Path $env:TEMP ("dtoken-active-config-data-" + [guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Force -Path $ActiveConfigDir | Out-Null
$ActiveConfigPath = Join-Path $ActiveConfigDir "dtoken-config.json"

$Config = @{
  providers = @(@{
    id = "active-mock-upstream"
    name = "Active Mock Upstream"
    kind = "custom"
    protocol = "open_ai"
    base_url = "http://127.0.0.1:$UpstreamPort"
    api_key_ref = $null
    models = @("mock/model")
    timeout_ms = 30000
    healthcheck_path = "/health"
  })
  routes = @(@{
    id = "active-config-route"
    name = "Active Config Route"
    input_protocol = "open_ai"
    instance_id = "active-config-instance"
    target_provider_id = "active-mock-upstream"
    fallback_provider_id = $null
    chain_mode = "direct"
    compression_policy = "balanced"
    bind_host = "127.0.0.1"
    local_port = $DtokenPort
  })
  instances = @(@{
    id = "active-config-instance"
    name = "Active Config Instance"
    agent = "OpenCode"
    route_id = "active-config-route"
    local_port = $DtokenPort
    process_status = "stopped"
    log_path = "./logs/active-config.jsonl"
  })
} | ConvertTo-Json -Depth 8
[System.IO.File]::WriteAllText($ActiveConfigPath, $Config, $Utf8NoBom)

$MockProcess = Start-Process -FilePath "python" -ArgumentList @($MockPath, "$UpstreamPort") -PassThru -WindowStyle Hidden

Push-Location (Join-Path $Root "code")
try {
  $Out = Join-Path $env:TEMP "dtoken-active-config-out.log"
  $Err = Join-Path $env:TEMP "dtoken-active-config-err.log"
  $Exe = Join-Path (Get-Location) "target\debug\dtoken-proxy.exe"
  cargo build -p dtoken-proxy | Out-Null

  $DtokenProcess = Start-Process -FilePath $Exe `
    -ArgumentList @("--port", "$DtokenPort", "--data-dir", $DataDir) `
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
        content = "repeat active config`nrepeat active config`nroute through active config"
      })
    } | ConvertTo-Json -Depth 8
    $Chat = Invoke-RestMethod -Method Post `
      "http://127.0.0.1:$DtokenPort/v1/chat/completions" `
      -ContentType "application/json" `
      -Body $Body
    $Dashboard = Invoke-RestMethod "http://127.0.0.1:$DtokenPort/dashboard/payload"
    $Active = Invoke-RestMethod "http://127.0.0.1:$DtokenPort/config/active"
    $ExportJson = Invoke-RestMethod "http://127.0.0.1:$DtokenPort/export/json"
    $ExportCsv = Invoke-RestMethod "http://127.0.0.1:$DtokenPort/export/csv"

    if ($Chat.choices[0].message.content -ne "active-config-ok") { throw "upstream response mismatch" }
    if ($Dashboard.route_decisions[0].route_id -ne "active-config-route") { throw "dashboard route mismatch" }
    if ($Dashboard.route_decisions[0].provider_id -ne "active-mock-upstream") { throw "dashboard provider mismatch" }
    if ($Dashboard.active_config.config.source -ne "desktop_active_config") { throw "dashboard active config source mismatch" }
    if ($Active.config.source -ne "desktop_active_config") { throw "active config source mismatch" }
    if ($Active.config.route_count -ne 1) { throw "active route count mismatch" }
    if ($Active.config.provider_count -ne 1) { throw "active provider count mismatch" }
    if ($Active.config.fallback_disabled -ne $true) { throw "active config fallback mismatch" }
    if ($ExportJson.redacted -ne $true) { throw "export json redaction mismatch" }
    if ($ExportJson.active_config.config.source -ne "desktop_active_config") { throw "export active config source mismatch" }
    if ($ExportJson.active_config.config.fallback_disabled -ne $true) { throw "export active config fallback mismatch" }
    if ($ExportCsv -notlike "*config_status*") { throw "export csv missing config_status" }
    if ($ExportCsv -notlike "*source=desktop_active_config*") { throw "export csv missing config source" }

    [pscustomobject]@{
      health = $Health.status
      content = $Chat.choices[0].message.content
      route = $Dashboard.route_decisions[0].route_id
      provider = $Dashboard.route_decisions[0].provider_id
      saved_tokens = $Dashboard.request_logs[0].tokens_saved
      dashboard_source = $Dashboard.source
      runtime_config_source = $Active.config.source
      runtime_config_route_count = $Active.config.route_count
      runtime_config_provider_count = $Active.config.provider_count
      runtime_config_fallback_disabled = $Active.config.fallback_disabled
      export_redacted = $ExportJson.redacted
      export_active_config_source = $ExportJson.active_config.config.source
      export_csv_has_config_status = ($ExportCsv -like "*config_status*")
      active_config_path = $ActiveConfigPath
      data_dir = $DataDir
    } | ConvertTo-Json -Compress
  } finally {
    if ($DtokenProcess -and -not $DtokenProcess.HasExited) {
      Stop-Process -Id $DtokenProcess.Id -Force
    }
  }
} finally {
  Pop-Location
  Stop-StartedProcess -Process $MockProcess -CommandLineNeedle "dtoken-active-config-mock-upstream.py $UpstreamPort"
}
