param(
  [string]$InstalledDir = "",
  [string]$UpstreamPath = "",
  [string]$OutputDir = "",
  [int]$TimeoutSeconds = 120,
  [switch]$OpenBrowser
)

$ErrorActionPreference = "Stop"
$Root = Resolve-Path (Join-Path $PSScriptRoot "..\..")
$Utf8NoBom = [System.Text.UTF8Encoding]::new($false)
if ([string]::IsNullOrWhiteSpace($InstalledDir)) {
  $InstalledDir = Join-Path $env:LOCALAPPDATA "d-token"
}
if ([string]::IsNullOrWhiteSpace($OutputDir)) {
  $OutputDir = Join-Path $Root "support\artifacts"
}
New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null

$Launcher = Join-Path $InstalledDir "dtoken-codebase-memory-launcher.exe"
$Upstream = if ([string]::IsNullOrWhiteSpace($UpstreamPath)) {
  Join-Path $env:LOCALAPPDATA "d-token\codebase-memory\upstream\codebase-memory-mcp.exe"
} else {
  $UpstreamPath
}
foreach ($Artifact in @($Launcher)) {
  if (-not (Test-Path -LiteralPath $Artifact -PathType Leaf)) {
    throw "Installed Codebase Memory artifact is missing: $Artifact"
  }
  $Signature = Get-AuthenticodeSignature -LiteralPath $Artifact
  if ($Signature.Status -ne "Valid") {
    throw "Installed Codebase Memory artifact is not Authenticode-valid: $Artifact"
  }
}
if (-not (Test-Path -LiteralPath $Upstream -PathType Leaf)) {
  throw "User-imported pinned Codebase Memory artifact is missing: $Upstream"
}

$RunId = [guid]::NewGuid().ToString("N")
$ProjectRoot = Join-Path $env:RUNNER_TEMP "d-token-cbm-project-$RunId"
$CacheRoot = Join-Path $env:RUNNER_TEMP "d-token-cbm-cache-$RunId"
New-Item -ItemType Directory -Force -Path (Join-Path $ProjectRoot "src") | Out-Null
New-Item -ItemType Directory -Force -Path $CacheRoot | Out-Null
[System.IO.File]::WriteAllText(
  (Join-Path $ProjectRoot "src\lib.rs"),
  "pub fn dtoken_codebase_memory_smoke() -> bool { true }`n",
  $Utf8NoBom
)

function Invoke-BoundedProcess {
  param(
    [Parameter(Mandatory = $true)][string]$FilePath,
    [Parameter(Mandatory = $true)][string[]]$Arguments,
    [string]$InputText = "",
    [int]$Timeout = 120
  )

  $StartInfo = [System.Diagnostics.ProcessStartInfo]::new()
  $StartInfo.FileName = $FilePath
  $StartInfo.UseShellExecute = $false
  $StartInfo.CreateNoWindow = $true
  $StartInfo.RedirectStandardInput = $true
  $StartInfo.RedirectStandardOutput = $true
  $StartInfo.RedirectStandardError = $true
  foreach ($Argument in $Arguments) {
    $StartInfo.ArgumentList.Add($Argument)
  }

  $Process = [System.Diagnostics.Process]::new()
  $Process.StartInfo = $StartInfo
  if (-not $Process.Start()) {
    throw "Failed to start the installed Codebase Memory process."
  }
  $StdoutTask = $Process.StandardOutput.ReadToEndAsync()
  $StderrTask = $Process.StandardError.ReadToEndAsync()
  if (-not [string]::IsNullOrEmpty($InputText)) {
    $Process.StandardInput.Write($InputText)
  }
  $Process.StandardInput.Close()
  if (-not $Process.WaitForExit($Timeout * 1000)) {
    $Process.Kill($true)
    $Process.WaitForExit()
    throw "Installed Codebase Memory process exceeded the bounded timeout."
  }
  $Stdout = $StdoutTask.GetAwaiter().GetResult()
  $null = $StderrTask.GetAwaiter().GetResult()
  if ($Process.ExitCode -ne 0) {
    throw "Installed Codebase Memory process failed with exit code $($Process.ExitCode)."
  }
  return [pscustomobject]@{
    exit_code = $Process.ExitCode
    stdout = $Stdout
  }
}

function ConvertFrom-JsonLines {
  param([string]$Text)
  $Values = @()
  foreach ($Line in ($Text -split "`r?`n")) {
    if (-not [string]::IsNullOrWhiteSpace($Line)) {
      $Values += ($Line | ConvertFrom-Json -Depth 100)
    }
  }
  return $Values
}

$UiProcess = $null
$BrowserDispatched = $false
try {
  $Initialize = @{ jsonrpc = "2.0"; id = 1; method = "initialize"; params = @{ protocolVersion = "2024-11-05"; capabilities = @{}; clientInfo = @{ name = "d-token-windows-candidate-audit"; version = "1" } } } | ConvertTo-Json -Compress -Depth 8
  $ToolsList = @{ jsonrpc = "2.0"; id = 2; method = "tools/list"; params = @{} } | ConvertTo-Json -Compress -Depth 8
  $ToolCall = @{ jsonrpc = "2.0"; id = 3; method = "tools/call"; params = @{ name = "list_projects"; arguments = @{} } } | ConvertTo-Json -Compress -Depth 8
  $McpInput = "$Initialize`n$ToolsList`n$ToolCall`n"
  $Mcp = Invoke-BoundedProcess -FilePath $Launcher -Arguments @("mcp", "stdio", $Upstream, $ProjectRoot, (Join-Path $CacheRoot "agent")) -InputText $McpInput -Timeout $TimeoutSeconds
  $McpResponses = @(ConvertFrom-JsonLines $Mcp.stdout)
  $InitializeResponse = @($McpResponses | Where-Object { $_.id -eq 1 -and $_.result.serverInfo.name -eq "dtoken-codebase-memory-launcher" })
  $ToolsResponse = @($McpResponses | Where-Object { $_.id -eq 2 -and @($_.result.tools).Count -eq 11 })
  $CallResponse = @($McpResponses | Where-Object { $_.id -eq 3 -and $null -ne $_.result })
  if ($InitializeResponse.Count -ne 1 -or $ToolsResponse.Count -ne 1 -or $CallResponse.Count -ne 1) {
    throw "Installed launcher did not complete the bounded MCP initialize/tools/list/tool-call sequence."
  }

  $Index = Invoke-BoundedProcess -FilePath $Launcher -Arguments @("index", $Upstream, $ProjectRoot, (Join-Path $CacheRoot "ui"), "fast") -Timeout $TimeoutSeconds
  $IndexReceipt = $Index.stdout | ConvertFrom-Json -Depth 20
  if ($IndexReceipt.outcome -ne "ready" -or $IndexReceipt.reason_code -ne "index_completed") {
    throw "Installed launcher did not complete the controlled index."
  }

  $TokenBytes = [byte[]]::new(32)
  [System.Security.Cryptography.RandomNumberGenerator]::Fill($TokenBytes)
  $Token = [Convert]::ToHexString($TokenBytes).ToLowerInvariant()
  $StartInfo = [System.Diagnostics.ProcessStartInfo]::new()
  $StartInfo.FileName = $Launcher
  $StartInfo.UseShellExecute = $false
  $StartInfo.CreateNoWindow = $true
  $StartInfo.RedirectStandardInput = $true
  $StartInfo.RedirectStandardOutput = $true
  $StartInfo.RedirectStandardError = $true
  foreach ($Argument in @("ui-host", $Upstream, $ProjectRoot, (Join-Path $CacheRoot "ui"), "$PID", "120")) {
    $StartInfo.ArgumentList.Add($Argument)
  }
  $UiProcess = [System.Diagnostics.Process]::new()
  $UiProcess.StartInfo = $StartInfo
  if (-not $UiProcess.Start()) {
    throw "Failed to start the installed Codebase Memory UI host."
  }
  $UiProcess.StandardInput.WriteLine((@{ token = $Token } | ConvertTo-Json -Compress))
  $UiProcess.StandardInput.Flush()
  $ReadyTask = $UiProcess.StandardOutput.ReadLineAsync()
  if (-not $ReadyTask.Wait($TimeoutSeconds * 1000)) {
    throw "Installed Codebase Memory UI host did not become ready in time."
  }
  $Ready = $ReadyTask.GetAwaiter().GetResult() | ConvertFrom-Json -Depth 20
  if ($Ready.component -ne "dtoken-codebase-memory-ui-host" -or $Ready.owner -ne "d-token" -or $Ready.host -ne "127.0.0.1" -or $Ready.parent_pid -ne $PID) {
    throw "Installed Codebase Memory UI host returned invalid ownership evidence."
  }

  $BaseUrl = "http://127.0.0.1:$($Ready.port)/s/$Token/"
  $Page = Invoke-WebRequest -Uri $BaseUrl -Method Get -TimeoutSec 30
  $Health = Invoke-WebRequest -Uri "${BaseUrl}api/health" -Method Get -TimeoutSec 30
  $Architecture = Invoke-WebRequest -Uri "${BaseUrl}api/architecture" -Method Get -TimeoutSec 30
  if ($Page.StatusCode -ne 200 -or $Health.StatusCode -ne 200 -or $Architecture.StatusCode -ne 200) {
    throw "Installed Codebase Memory UI host did not serve its read-only surface."
  }
  if ($Page.Headers["Content-Security-Policy"] -notmatch "default-src 'none'" -or $Page.Headers["Cache-Control"] -notmatch "no-store") {
    throw "Installed Codebase Memory UI host omitted required browser security headers."
  }
  $DeniedToken = Invoke-WebRequest -Uri "http://127.0.0.1:$($Ready.port)/s/invalid/api/health" -Method Get -SkipHttpErrorCheck -TimeoutSec 30
  $DeniedAdmin = Invoke-WebRequest -Uri "${BaseUrl}api/browse" -Method Get -SkipHttpErrorCheck -TimeoutSec 30
  if ($DeniedToken.StatusCode -ne 404 -or $DeniedAdmin.StatusCode -ne 404) {
    throw "Installed Codebase Memory UI host exposed an unauthorized or unapproved route."
  }
  if ($OpenBrowser) {
    $null = Start-Process -FilePath $BaseUrl -PassThru
    $BrowserDispatched = $true
  }

  foreach ($ProtectedPath in @(
    (Join-Path $ProjectRoot ".codebase-memory"),
    (Join-Path $ProjectRoot "artifact.json"),
    (Join-Path $ProjectRoot ".gitattributes")
  )) {
    if (Test-Path -LiteralPath $ProtectedPath) {
      throw "Codebase Memory candidate wrote a protected repository artifact."
    }
  }

  $Report = [pscustomobject]@{
    ok = $true
    mode = "installed-codebase-memory-smoke"
    generated_at = (Get-Date).ToString("o")
    candidate_surface = "installed-launcher-with-user-imported-upstream"
    mcp = @{ initialize = 1; tools_list = 11; allowed_tool_calls = 1; denied = 0 }
    index = @{ outcome = $IndexReceipt.outcome; reason_code = $IndexReceipt.reason_code }
    ui_host = @{ owner = $Ready.owner; host = $Ready.host; health = 200; architecture = 200; unauthorized = 404; admin_route = 404 }
    browser_dispatched = $BrowserDispatched
    repository_zero_write = $true
    privacy = "Metadata-only candidate evidence; no token, graph, source, prompt, response, or raw MCP payload is persisted."
  }
  $Stamp = (Get-Date).ToString("yyyyMMdd-HHmmss")
  $ReportPath = Join-Path $OutputDir "installed-codebase-memory-smoke-$Stamp.json"
  [System.IO.File]::WriteAllText($ReportPath, ($Report | ConvertTo-Json -Depth 8), $Utf8NoBom)
  $Report | ConvertTo-Json -Depth 8 -Compress
} finally {
  if ($null -ne $UiProcess) {
    try { $UiProcess.StandardInput.Close() } catch {}
    try {
      if (-not $UiProcess.WaitForExit(10000)) {
        $UiProcess.Kill($true)
        $UiProcess.WaitForExit()
      }
    } catch {}
    $UiProcess.Dispose()
  }
  foreach ($Path in @($ProjectRoot, $CacheRoot)) {
    if (Test-Path -LiteralPath $Path) {
      Remove-Item -LiteralPath $Path -Recurse -Force -ErrorAction SilentlyContinue
    }
  }
}
