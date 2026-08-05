param(
  [int]$DtokenPort = 38117,
  [int]$UpstreamPort = 39128
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

function New-Check {
  param(
    [string]$Id,
    [string]$Label,
    [bool]$Ok,
    [string]$Summary,
    [object]$Detail
  )

  [pscustomobject]@{
    id = $Id
    label = $Label
    ok = $Ok
    summary = $Summary
    detail = $Detail
  }
}

$MockScript = @'
import json, sys
from http.server import BaseHTTPRequestHandler, HTTPServer

class Handler(BaseHTTPRequestHandler):
    def do_GET(self):
        if self.path == "/health":
            raw = b'{"status":"healthy"}'
            self.send_response(200)
            self.send_header("content-type", "application/json")
            self.send_header("content-length", str(len(raw)))
            self.end_headers()
            self.wfile.write(raw)
            return
        self.send_response(404)
        self.end_headers()

    def do_POST(self):
        length = int(self.headers.get("content-length", "0"))
        body = self.rfile.read(length)
        if self.path.endswith("/v1/chat/completions"):
            payload = json.loads(body or b"{}")
            raw = json.dumps({
                "id": "agent-route-verification-1",
                "object": "chat.completion",
                "model": payload.get("model", "mock/model"),
                "choices": [{
                    "index": 0,
                    "message": {"role": "assistant", "content": "agent-route-verification-ok"},
                    "finish_reason": "stop"
                }],
                "usage": {"prompt_tokens": 6, "completion_tokens": 4, "total_tokens": 10}
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

$MockPath = Join-Path $env:TEMP "dtoken-agent-route-verification-upstream.py"
[System.IO.File]::WriteAllText($MockPath, $MockScript, $Utf8NoBom)

$ActiveRoot = Join-Path $env:TEMP ("dtoken-agent-route-root-" + [guid]::NewGuid().ToString("N"))
$env:LOCALAPPDATA = $ActiveRoot
$ActiveConfigDir = Join-Path $ActiveRoot "d-token\active-config"
$DataDir = Join-Path $env:TEMP ("dtoken-agent-route-data-" + [guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Force -Path $ActiveConfigDir | Out-Null
$ActiveConfigPath = Join-Path $ActiveConfigDir "dtoken-config.json"

$Config = @{
  providers = @(@{
    id = "agent-route-mock-upstream"
    name = "Agent Route Mock Upstream"
    kind = "custom"
    protocol = "open_ai"
    base_url = "http://127.0.0.1:$UpstreamPort"
    api_key_ref = $null
    models = @("mock/model")
    timeout_ms = 30000
    healthcheck_path = "/health"
  })
  routes = @(@{
    id = "agent-route-verification"
    name = "Agent Route Verification"
    input_protocol = "open_ai"
    instance_id = "agent-route-instance"
    target_provider_id = "agent-route-mock-upstream"
    fallback_provider_id = $null
    chain_mode = "direct"
    compression_policy = "balanced"
    bind_host = "127.0.0.1"
    local_port = $DtokenPort
  })
  instances = @(@{
    id = "agent-route-instance"
    name = "Agent Route Instance"
    agent = "OpenCode"
    route_id = "agent-route-verification"
    local_port = $DtokenPort
    process_status = "stopped"
    log_path = "./logs/agent-route-verification.jsonl"
  })
} | ConvertTo-Json -Depth 8
[System.IO.File]::WriteAllText($ActiveConfigPath, $Config, $Utf8NoBom)

$MockProcess = Start-Process -FilePath "python" -ArgumentList @($MockPath, "$UpstreamPort") -PassThru -WindowStyle Hidden

Push-Location (Join-Path $Root "code")
try {
  $Out = Join-Path $env:TEMP "dtoken-agent-route-verification-out.log"
  $Err = Join-Path $env:TEMP "dtoken-agent-route-verification-err.log"
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
        content = "verify agent route`nverify agent route`nobserve dashboard source"
      })
    } | ConvertTo-Json -Depth 8
    $Chat = Invoke-RestMethod -Method Post `
      "http://127.0.0.1:$DtokenPort/v1/chat/completions" `
      -ContentType "application/json" `
      -Body $Body
    $RuntimeActive = Invoke-RestMethod "http://127.0.0.1:$DtokenPort/config/active"
    $Dashboard = Invoke-RestMethod "http://127.0.0.1:$DtokenPort/dashboard/payload"
    $ExportJson = Invoke-RestMethod "http://127.0.0.1:$DtokenPort/export/json"

    $ActiveFile = Get-Content $ActiveConfigPath -Raw | ConvertFrom-Json
    $ActiveLoaded = (Test-Path $ActiveConfigPath) -and @($ActiveFile.routes).Count -gt 0 -and @($ActiveFile.providers).Count -gt 0
    $FallbackDisabled = ($null -eq $ActiveFile.routes[0].fallback_provider_id)
    $HealthOk = $Health.status -in @("healthy", "ready")
    $RuntimeLoaded = $RuntimeActive.config.loaded -eq $true
    $RuntimeRouteCount = [int]$RuntimeActive.config.route_count
    $DashboardOk = $Dashboard.source -eq "dtoken-owned-runtime"
    $RequestCount = [int]$Dashboard.stats.requests.total
    $RouteDecisions = @($Dashboard.route_decisions).Count
    $ExportOk = ($ExportJson.redacted -eq $true) -and ($ExportJson.raw_available -eq $false)

    if ($Chat.choices[0].message.content -ne "agent-route-verification-ok") { throw "upstream response mismatch" }

    $Checks = @(
      New-Check "active_config" "Active d-token config" $ActiveLoaded "d-token active route config is loaded" @{
        path = $ActiveConfigPath
        route = $ActiveFile.routes[0].id
        provider = $ActiveFile.providers[0].id
      }
      New-Check "fallback" "Fallback boundary" $FallbackDisabled "Silent fallback is disabled" @{
        route = $ActiveFile.routes[0].id
        provider = $ActiveFile.providers[0].id
      }
      New-Check "runtime_health" "d-token local service" $HealthOk "127.0.0.1:$DtokenPort responded healthy" $Health
      New-Check "runtime_active_config" "Runtime route loader" ($RuntimeLoaded -and $RuntimeRouteCount -gt 0) "Runtime loaded the active route config" $RuntimeActive.config
      New-Check "dashboard" "Dashboard data source" $DashboardOk "$($Dashboard.source) / $RequestCount requests / $RouteDecisions route decisions" @{
        source = $Dashboard.source
        requests = $RequestCount
        route_decisions = $RouteDecisions
      }
      New-Check "export_redaction" "Redacted export" $ExportOk "Runtime export is redacted" @{
        redacted = $ExportJson.redacted
        raw_available = $ExportJson.raw_available
      }
    )

    $Passed = @($Checks | Where-Object { $_.ok }).Count
    $Total = @($Checks).Count
    if ($Passed -ne $Total) {
      throw "agent route verification checks failed: $Passed/$Total"
    }

    [pscustomobject]@{
      ok = $true
      mode = "agent-route-verification"
      passed = $Passed
      total = $Total
      summary = "Agent route configuration is active and observable through d-token runtime"
      route = $ActiveFile.routes[0].id
      provider = $ActiveFile.providers[0].id
      service_url = "http://127.0.0.1:$DtokenPort"
      dashboard_source = $Dashboard.source
      request_count = $RequestCount
      route_decisions = $RouteDecisions
      export_redacted = $ExportJson.redacted
      export_raw_available = $ExportJson.raw_available
      active_config_path = $ActiveConfigPath
      data_dir = $DataDir
      privacy = "Read-only d-token local verification; no external Headroom/RTK/cc-switch service is touched and no secrets are read."
      checks = $Checks
    } | ConvertTo-Json -Depth 12 -Compress
  } finally {
    Stop-StartedProcess -Process $DtokenProcess -CommandLineNeedle "--port $DtokenPort"
  }
} finally {
  Pop-Location
  Stop-StartedProcess -Process $MockProcess -CommandLineNeedle "dtoken-agent-route-verification-upstream.py $UpstreamPort"
}
