param(
  [int]$DtokenPort = 18001,
  [string]$Model = "dtoken/deepseek-v4-flash-free",
  [string]$OutputDir = ""
)

$ErrorActionPreference = "Stop"
$Root = Resolve-Path (Join-Path $PSScriptRoot "..\..")
$Utf8NoBom = [System.Text.UTF8Encoding]::new($false)
if ([string]::IsNullOrWhiteSpace($OutputDir)) {
  $OutputDir = Join-Path $Root "support\artifacts"
}
New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null

$ConfigPath = Join-Path $env:TEMP "dtoken-opencode-workload-upstream.json"
$DataDir = Join-Path $env:TEMP ("dtoken-opencode-workload-data-" + [guid]::NewGuid().ToString("N"))
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
    id = "opencode-workload-routed"
    name = "OpenCode Workload Routed"
    input_protocol = "open_ai"
    instance_id = "opencode-workload-instance"
    target_provider_id = "opencode-zen"
    fallback_provider_id = $null
    chain_mode = "direct"
    compression_policy = "balanced"
    bind_host = "127.0.0.1"
    local_port = $DtokenPort
  })
  instances = @(@{
    id = "opencode-workload-instance"
    name = "OpenCode Workload Instance"
    agent = "OpenCode"
    route_id = "opencode-workload-routed"
    local_port = $DtokenPort
    process_status = "stopped"
    log_path = "./logs/opencode-workload.jsonl"
  })
} | ConvertTo-Json -Depth 8
[System.IO.File]::WriteAllText($ConfigPath, $Config, $Utf8NoBom)

$Workloads = @(
  @{
    id = "repeated_logs"
    prompt = "Reply with exactly: d-token-ok repeated_logs. Analyze this log briefly: 2026-07-04T10:00:00Z INFO build started`n2026-07-04T10:00:00Z INFO build started`n2026-07-04T10:00:01Z WARN retry route`n2026-07-04T10:00:01Z WARN retry route"
  },
  @{
    id = "code_duplicate"
    prompt = "Reply with exactly: d-token-ok code_duplicate. Review duplicated code: fn main(){println!(\""hello\"");}`nfn main(){println!(\""hello\"");}`n// route adapter`n// route adapter"
  },
  @{
    id = "json_config"
    prompt = "Reply with exactly: d-token-ok json_config. Inspect config: {`"route`":`"opencode-workload-routed`",`"provider`":`"opencode-zen`",`"events`":[1,1,1],`"events`":[1,1,1]}"
  }
)

function Quote-CmdArg {
  param([string]$Value)
  return '"' + ($Value -replace '\\', '\\' -replace '"', '\"') + '"'
}

function Invoke-OpenCodeWorkload {
  param(
    [string]$Model,
    [string]$Prompt,
    [string]$WorkingDirectory,
    [string]$StdoutPath,
    [string]$StderrPath
  )

  $StartInfo = [System.Diagnostics.ProcessStartInfo]::new()
  $StartInfo.FileName = "opencode.cmd"
  $StartInfo.UseShellExecute = $false
  $StartInfo.RedirectStandardOutput = $true
  $StartInfo.RedirectStandardError = $true
  $StartInfo.CreateNoWindow = $true
  $StartInfo.WorkingDirectory = $WorkingDirectory
  $StartInfo.Arguments = "run -m $(Quote-CmdArg $Model) $(Quote-CmdArg $Prompt)"
  $StartInfo.Environment["NO_PROXY"] = "127.0.0.1,localhost"
  $StartInfo.Environment.Remove("ANTHROPIC_BASE_URL") | Out-Null
  $StartInfo.Environment.Remove("ANTHROPIC_API_URL") | Out-Null
  $StartInfo.Environment.Remove("CLAUDE_CODE_PROXY") | Out-Null

  $Process = [System.Diagnostics.Process]::new()
  $Process.StartInfo = $StartInfo
  [void]$Process.Start()
  if (-not $Process.WaitForExit(120000)) {
    try { $Process.Kill() } catch {}
    throw "OpenCode timed out"
  }
  $Stdout = $Process.StandardOutput.ReadToEnd()
  $Stderr = $Process.StandardError.ReadToEnd()
  [System.IO.File]::WriteAllText($StdoutPath, $Stdout, $Utf8NoBom)
  [System.IO.File]::WriteAllText($StderrPath, $Stderr, $Utf8NoBom)
  return [int]$Process.ExitCode
}

Push-Location (Join-Path $Root "code")
try {
  $Out = Join-Path $env:TEMP "dtoken-opencode-workload-out.log"
  $Err = Join-Path $env:TEMP "dtoken-opencode-workload-err.log"
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
    $Runs = @()
    foreach ($Workload in $Workloads) {
      $OpOut = Join-Path $env:TEMP ("opencode-workload-" + $Workload.id + "-out.txt")
      $OpErr = Join-Path $env:TEMP ("opencode-workload-" + $Workload.id + "-err.txt")
      $Started = Get-Date
      $ExitCode = Invoke-OpenCodeWorkload -Model $Model -Prompt $Workload.prompt -WorkingDirectory $Root -StdoutPath $OpOut -StderrPath $OpErr
      $ElapsedMs = [int]((Get-Date) - $Started).TotalMilliseconds
      $Stdout = Get-Content $OpOut -Raw -ErrorAction SilentlyContinue
      $Stderr = Get-Content $OpErr -Raw -ErrorAction SilentlyContinue
      $Text = (($Stdout + "`n" + $Stderr) -replace "`e\[[0-9;]*m", "").Trim()
      $ContainsExpected = $Text -like "*d-token-ok*"
      $Sha256 = [System.Security.Cryptography.SHA256]::Create()
      try {
        $OutputHash = [System.BitConverter]::ToString(
          $Sha256.ComputeHash([System.Text.Encoding]::UTF8.GetBytes($Text))
        ).Replace("-", "").ToLowerInvariant()
      } finally {
        $Sha256.Dispose()
      }
      $ExpectedMarker = if ($ContainsExpected -and $Text -match "d-token-ok\s+[a-zA-Z0-9_-]+") { $Matches[0] } else { "" }
      $Runs += [pscustomobject]@{
        id = $Workload.id
        elapsed_ms = $ElapsedMs
        command_exit_code = $ExitCode
        command_ok = $ExitCode -eq 0
        response_contains_expected = $ContainsExpected
        output_chars = $Text.Length
        output_sha256 = $OutputHash
        expected_marker = $ExpectedMarker
      }
    }
    Push-Location (Join-Path $Root "code")

    $Dashboard = Invoke-RestMethod "http://127.0.0.1:$DtokenPort/dashboard/payload"
    $Active = Invoke-RestMethod "http://127.0.0.1:$DtokenPort/config/active"
    $Export = Invoke-RestMethod "http://127.0.0.1:$DtokenPort/export/json"
    $RequestLogs = @($Dashboard.request_logs)
    $RouteDecisions = @($Dashboard.route_decisions)
    if ($RequestLogs.Count -lt $Workloads.Count) {
      throw "expected at least $($Workloads.Count) request logs, got $($RequestLogs.Count)"
    }
    if ($RouteDecisions[0].route_id -ne "opencode-workload-routed") {
      throw "dashboard did not record opencode-workload-routed"
    }
    if ($Active.config.source -ne "explicit_cli_config") {
      throw "runtime active config source mismatch"
    }
    if ($Active.config.fallback_disabled -ne $true) {
      throw "runtime active config fallback mismatch"
    }
    if ($Export.redacted -ne $true) {
      throw "runtime export is not marked redacted"
    }
    if ($Export.raw_available -ne $false) {
      throw "runtime export raw_available must be false"
    }
    if ($Export.active_config.config.source -ne "explicit_cli_config") {
      throw "runtime export active config source mismatch"
    }

    $TotalBefore = 0
    $TotalAfter = 0
    $TotalSaved = 0
    foreach ($Log in $RequestLogs) {
      $TotalBefore += [int]$Log.input_tokens_before
      $TotalAfter += [int]$Log.input_tokens_after
      $TotalSaved += [int]$Log.tokens_saved
    }
    $SavingsPercent = if ($TotalBefore -gt 0) { [math]::Round(($TotalSaved / $TotalBefore) * 100, 2) } else { 0 }

    $Result = [pscustomobject]@{
      health = $Health.status
      model = $Model
      workload_count = $Workloads.Count
      route = $RouteDecisions[0].route_id
      provider = $RouteDecisions[0].provider_id
      dashboard_source = $Dashboard.source
      runtime_config_source = $Active.config.source
      runtime_config_route_count = $Active.config.route_count
      runtime_config_provider_count = $Active.config.provider_count
      runtime_config_fallback_disabled = $Active.config.fallback_disabled
      total_before_tokens = $TotalBefore
      total_after_tokens = $TotalAfter
      total_saved_tokens = $TotalSaved
      savings_percent = $SavingsPercent
      export_redacted = $Export.redacted
      export_raw_available = $Export.raw_available
      export_active_config_source = $Export.active_config.config.source
      compression_performance = $Dashboard.metrics.compression_performance
      route_performance = $Dashboard.metrics.route_performance
      runs = $Runs
      persisted_events = (Get-Content (Join-Path $DataDir "events.jsonl")).Count
      persisted_routes = (Get-Content (Join-Path $DataDir "route-decisions.jsonl")).Count
      persisted_logs = (Get-Content (Join-Path $DataDir "request-logs.jsonl")).Count
      persisted_ccr = (Get-Content (Join-Path $DataDir "ccr-metadata.jsonl")).Count
      data_dir = $DataDir
    }
    $Stamp = (Get-Date).ToString("yyyyMMdd-HHmmss")
    $ReportPath = Join-Path $OutputDir "opencode-workload-benchmark-$Stamp.json"
    [System.IO.File]::WriteAllText($ReportPath, ($Result | ConvertTo-Json -Depth 12), $Utf8NoBom)
    $Result | Add-Member -NotePropertyName report_path -NotePropertyValue $ReportPath
    $Result | ConvertTo-Json -Depth 8 -Compress
  } finally {
    if ($DtokenProcess -and -not $DtokenProcess.HasExited) {
      Stop-Process -Id $DtokenProcess.Id -Force
    }
  }
} finally {
  Pop-Location
}
