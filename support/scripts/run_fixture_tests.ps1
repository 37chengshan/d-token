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
    cargo test -p dtoken-config-patcher --test agent_bridge_fixture
    cargo test -p dtoken-agent-bridge --test subprocess_contract
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
    $projects = Invoke-RestMethod "http://127.0.0.1:$Port/projects"
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
    if ($projects.source -ne "dtoken-owned-runtime") { throw "project registry source failed" }
    if ($projects.state -ne "unavailable") { throw "unverified project registry must fail closed" }
    if ($dashboard.source -ne "dtoken-owned-runtime") { throw "dashboard source failed" }
    if ($ccr.source -ne "dtoken-owned-runtime") { throw "ccr source failed" }
    if ($ccr.raw_available -ne $false) { throw "ccr raw availability boundary failed" }
    if ($memory.source -ne "dtoken-owned-runtime") { throw "memory source failed" }
    if ($memory.raw_available -ne $false) { throw "memory raw availability boundary failed" }
    if ($outputShape.source -ne "dtoken-owned-runtime") { throw "output-shape source failed" }
    if ($outputShape.raw_available -ne $false) { throw "output-shape raw availability boundary failed" }
    if ($outputShape.reason_code -ne "memory_output_shape_authorization_unavailable") { throw "output-shape authorization boundary failed" }
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
      projects_state = $projects.state
      dashboard_source = $dashboard.source
      ccr_entries = $ccr.entries
      ccr_raw_available = $ccr.raw_available
      memory_entries = $memory.entries
      memory_raw_available = $memory.raw_available
      output_shape_state = $outputShape.state
      output_shape_reason = $outputShape.reason_code
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
