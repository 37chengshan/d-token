param(
  [int]$Port = 38101
)

$ErrorActionPreference = "Stop"
$Root = Resolve-Path (Join-Path $PSScriptRoot "..\..")
$env:DTOKEN_PROFILE = "fixture"
$env:DTOKEN_FIXTURE_ROOT = Join-Path $Root "support\fixtures"
$Utf8NoBom = [System.Text.UTF8Encoding]::new($false)
$ConfigPath = Join-Path $env:TEMP ("dtoken-fixture-config-" + [guid]::NewGuid().ToString("N") + ".json")
$DataDir = Join-Path $env:TEMP ("dtoken-fixture-data-" + [guid]::NewGuid().ToString("N"))
[System.IO.File]::WriteAllText($ConfigPath, '{"providers":[],"routes":[],"instances":[]}', $Utf8NoBom)

python (Join-Path $Root "support\scripts\check_structure.py")

Push-Location (Join-Path $Root "code")
try {
  cargo check --workspace

  $hostLine = rustc -vV | Select-String "host:"
  $dlltool = Get-Command dlltool.exe -ErrorAction SilentlyContinue
  $canLinkRustBinary = $hostLine -notmatch "windows-gnu" -or $dlltool
  if ($canLinkRustBinary) {
    cargo test --workspace
  } else {
    Write-Host "Skipping cargo test: windows-gnu toolchain requires dlltool.exe, which is not installed."
  }

  if (-not $canLinkRustBinary) {
    [pscustomobject]@{
      health = "runtime-skipped"
      reason = "windows-gnu toolchain requires dlltool.exe to link cargo test/run"
      structure = "passed"
      cargo_check = "passed"
    } | ConvertTo-Json -Compress
    return
  }

  $out = Join-Path $env:TEMP "dtoken-fixture-$Port-out.log"
  $err = Join-Path $env:TEMP "dtoken-fixture-$Port-err.log"
  $process = Start-Process -FilePath "cargo" `
    -ArgumentList @("run", "-p", "dtoken-proxy", "--", "--port", "$Port", "--config", $ConfigPath, "--data-dir", $DataDir) `
    -PassThru `
    -WindowStyle Hidden `
    -RedirectStandardOutput $out `
    -RedirectStandardError $err

  try {
    $health = $null
    for ($attempt = 0; $attempt -lt 30; $attempt++) {
      try {
        $health = Invoke-RestMethod "http://127.0.0.1:$Port/health"
        break
      } catch {
        Start-Sleep -Milliseconds 500
      }
    }
    if (-not $health) { throw "fixture proxy did not become healthy on port $Port" }

    $rtk = Invoke-RestMethod "http://127.0.0.1:$Port/rtk/status"
    $gain = Invoke-RestMethod "http://127.0.0.1:$Port/rtk/gain"
    $discover = Invoke-RestMethod "http://127.0.0.1:$Port/rtk/discover"
    $headroom = Invoke-RestMethod "http://127.0.0.1:$Port/headroom/status"
    $history = Invoke-RestMethod "http://127.0.0.1:$Port/stats-history"
    $doctor = Invoke-RestMethod -Method Post "http://127.0.0.1:$Port/doctor/run"
    $chatBody = @{
      model = "opencode/deepseek-v4-flash-free"
      messages = @(@{
        role = "user"
        content = "repeat log line`nrepeat log line`ncompile warning warning"
      })
    } | ConvertTo-Json -Depth 8
    $chat = Invoke-RestMethod -Method Post "http://127.0.0.1:$Port/v1/chat/completions" -ContentType "application/json" -Body $chatBody
    $memoryBody = @{
      project_id = "fixture-project"
      agent = "OpenCode"
      kind = "verbosity"
      summary = "Prefer concise fixture output."
      output_verbosity = "terse"
      raw_prompt = "fixture raw prompt must not persist"
      raw_response = "fixture raw response must not persist"
    } | ConvertTo-Json -Depth 8
    $memoryLearn = Invoke-RestMethod -Method Post "http://127.0.0.1:$Port/memory/learn" -ContentType "application/json" -Body $memoryBody
    $dashboard = Invoke-RestMethod "http://127.0.0.1:$Port/dashboard/payload"
    $ccr = Invoke-RestMethod "http://127.0.0.1:$Port/ccr/summary"
    $memory = Invoke-RestMethod "http://127.0.0.1:$Port/memory/summary"
    $outputShape = Invoke-RestMethod "http://127.0.0.1:$Port/output-shape"
    $headroomProxy = Invoke-RestMethod "http://127.0.0.1:$Port/headroom/proxy"
    $headroomMcp = Invoke-RestMethod "http://127.0.0.1:$Port/headroom/mcp"
    $headroomStats = Invoke-RestMethod "http://127.0.0.1:$Port/headroom/stats"
    $headroomCache = Invoke-RestMethod "http://127.0.0.1:$Port/headroom/cache-summary"
    $headroomVersion = $null
    if ($headroom.PSObject.Properties.Name -contains "reference_headroom" -and $headroom.reference_headroom) {
      $headroomVersion = $headroom.reference_headroom.version
    } elseif ($headroom.PSObject.Properties.Name -contains "version") {
      $headroomVersion = $headroom.version
    }

    if ($health.status -ne "healthy") { throw "health check failed" }
    if ($rtk.install_method -ne "fixture") { throw "rtk fixture status failed" }
    if ($gain.source -notlike "*fixture*") { throw "rtk gain fixture source failed" }
    if ($discover.source -ne "fixture") { throw "rtk discover fixture source failed" }
    if ($headroom.source -ne "dtoken-owned-runtime") { throw "headroom runtime source failed" }
    if ($headroom.external_headroom_dependency -ne $false) { throw "headroom external dependency flag failed" }
    if ($headroomVersion -notlike "*fixture*") { throw "headroom fixture status failed" }
    if ($history.source -ne "dtoken-owned-runtime") { throw "runtime history source failed" }
    if (-not ($history.PSObject.Properties.Name -contains "history")) { throw "runtime history payload missing" }
    if (-not $doctor.ok) { throw "doctor failed" }
    if ($chat.choices[0].message.content -ne "d-token-ok") { throw "d-token route adapter response failed" }
    if ($chat.dtoken.route_decision.route_id -ne "dtoken-opencode-local") { throw "d-token route decision failed" }
    if ($chat.dtoken.compression.saved_tokens -le 0) { throw "d-token compression summary failed" }
    if ($dashboard.source -ne "dtoken-owned-runtime") { throw "dashboard source failed" }
    if ($ccr.source -ne "dtoken-owned-runtime") { throw "ccr source failed" }
    if ($ccr.raw_available -ne $false) { throw "ccr raw availability boundary failed" }
    if ($ccr.entries -lt 1) { throw "ccr metadata entry missing after routed request" }
    if (-not $memoryLearn.ok) { throw "memory learn failed" }
    if ($memory.source -ne "dtoken-owned-runtime") { throw "memory source failed" }
    if ($memory.raw_available -ne $false) { throw "memory raw availability boundary failed" }
    if ($memory.entries -lt 1) { throw "memory summary entry missing after learn" }
    if (($memory | ConvertTo-Json -Depth 10) -match "fixture raw prompt must not persist|fixture raw response must not persist") { throw "memory raw prompt/response leaked" }
    if ($outputShape.source -ne "dtoken-owned-runtime") { throw "output-shape source failed" }
    if ($outputShape.raw_available -ne $false) { throw "output-shape raw availability boundary failed" }
    if ($outputShape.output_verbosity -ne "terse") { throw "output-shape learned verbosity failed" }
    if ($headroomProxy.source -ne "dtoken-owned-runtime" -or $headroomProxy.external_headroom_dependency -ne $false) { throw "headroom proxy ownership failed" }
    if ($headroomMcp.source -ne "dtoken-owned-runtime" -or $headroomMcp.raw_available -ne $false) { throw "headroom mcp metadata boundary failed" }
    if ($headroomStats.source -ne "dtoken-owned-runtime" -or $headroomStats.external_headroom_dependency -ne $false) { throw "headroom stats ownership failed" }
    if ($headroomCache.source -ne "dtoken-owned-runtime" -or $headroomCache.raw_available -ne $false) { throw "headroom cache metadata boundary failed" }

    [pscustomobject]@{
      health = $health.status
      rtk = $rtk.install_method
      gain = $gain.source
      discover = $discover.source
      headroom = $headroomVersion
      headroom_source = $headroom.source
      external_headroom_dependency = $headroom.external_headroom_dependency
      history_source = $history.source
      history_points = @($history.history).Count
      doctor = $doctor.ok
      route = $chat.dtoken.route_decision.route_id
      saved_tokens = $chat.dtoken.compression.saved_tokens
      dashboard_source = $dashboard.source
      ccr_entries = $ccr.entries
      ccr_raw_available = $ccr.raw_available
      memory_entries = $memory.entries
      memory_raw_available = $memory.raw_available
      output_verbosity = $outputShape.output_verbosity
      output_shape_raw_available = $outputShape.raw_available
      headroom_proxy_source = $headroomProxy.source
      headroom_mcp_raw_available = $headroomMcp.raw_available
      headroom_stats_source = $headroomStats.source
      data_dir = $DataDir
    } | ConvertTo-Json -Compress
  } finally {
    if ($process -and -not $process.HasExited) {
      Stop-Process -Id $process.Id -Force
    }
  }
} finally {
  Pop-Location
}
