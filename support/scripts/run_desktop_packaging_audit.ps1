param(
  [string]$OutputDir = ""
)

$ErrorActionPreference = "Stop"
$Root = Resolve-Path (Join-Path $PSScriptRoot "..\..")
$Utf8NoBom = [System.Text.UTF8Encoding]::new($false)
if ([string]::IsNullOrWhiteSpace($OutputDir)) {
  $OutputDir = Join-Path $Root "support\artifacts"
}
New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null

$PackagePath = Join-Path $Root "code\apps\desktop\package.json"
$TauriPath = Join-Path $Root "code\apps\desktop\src-tauri\tauri.conf.json"
$IconPath = Join-Path $Root "code\apps\desktop\src-tauri\icons\icon.ico"
$DistPath = Join-Path $Root "code\apps\desktop\dist\index.html"
$SidecarPathMsvc = Join-Path $Root "code\apps\desktop\src-tauri\binaries\dtoken-proxy-x86_64-pc-windows-msvc.exe"
$PreparedWebView2Loader = Join-Path $Root "code\apps\desktop\src-tauri\binaries\WebView2Loader.dll"

$Package = Get-Content -LiteralPath $PackagePath -Raw | ConvertFrom-Json
$Tauri = Get-Content -LiteralPath $TauriPath -Raw | ConvertFrom-Json

function New-Check {
  param(
    [string]$Id,
    [string]$Label,
    [bool]$Ok,
    [object]$Detail
  )
  [pscustomobject]@{
    id = $Id
    label = $Label
    ok = $Ok
    detail = $Detail
  }
}

$Targets = @($Tauri.bundle.targets)
$ExternalBins = @($Tauri.bundle.externalBin)
$Resources = $Tauri.bundle.resources
$WebView2ResourceConfigured = $false
if ($null -ne $Resources) {
  $WebView2ResourceConfigured = $null -ne $Resources.'binaries/WebView2Loader.dll'
}
$Scripts = $Package.scripts
$Checks = @(
  New-Check "version_match" "package and Tauri versions match" ($Package.version -eq $Tauri.version) @{
    package = $Package.version
    tauri = $Tauri.version
  }
  New-Check "bundle_active" "Tauri bundle is enabled" ($Tauri.bundle.active -eq $true) @{
    active = $Tauri.bundle.active
  }
  New-Check "bundle_target" "Windows installer target is configured" ($Targets -contains "nsis") @{
    targets = $Targets
  }
  New-Check "icon_exists" "Application icon exists" (Test-Path $IconPath) @{
    path = $IconPath
    bytes = if (Test-Path $IconPath) { (Get-Item $IconPath).Length } else { 0 }
  }
  New-Check "csp_set" "Content security policy is set" (-not [string]::IsNullOrWhiteSpace([string]$Tauri.app.security.csp)) @{
    csp = $Tauri.app.security.csp
  }
  New-Check "build_script" "Frontend build script exists" (-not [string]::IsNullOrWhiteSpace([string]$Scripts.build)) @{
    build = $Scripts.build
  }
  New-Check "tauri_build_script" "Tauri build script exists" (-not [string]::IsNullOrWhiteSpace([string]$Scripts.'tauri:build')) @{
    tauri_build = $Scripts.'tauri:build'
  }
  New-Check "runtime_sidecar_configured" "dtoken-proxy sidecar is configured for bundling" ($ExternalBins -contains "binaries/dtoken-proxy") @{
    external_bin = $ExternalBins
  }
  New-Check "runtime_sidecar_prepared" "canonical MSVC dtoken-proxy sidecar exists before packaging" (Test-Path $SidecarPathMsvc) @{
    msvc_path = $SidecarPathMsvc
    msvc_bytes = if (Test-Path $SidecarPathMsvc) { (Get-Item $SidecarPathMsvc).Length } else { 0 }
  }
  New-Check "webview2_loader_resource_configured" "WebView2Loader.dll is configured as root install resource" $WebView2ResourceConfigured @{
    resources = $Resources
  }
  New-Check "webview2_loader_prepared" "WebView2Loader.dll exists before packaging" (Test-Path $PreparedWebView2Loader) @{
    path = $PreparedWebView2Loader
    bytes = if (Test-Path $PreparedWebView2Loader) { (Get-Item $PreparedWebView2Loader).Length } else { 0 }
  }
  New-Check "frontend_dist" "Frontend dist exists from latest build" (Test-Path $DistPath) @{
    path = $DistPath
  }
  New-Check "metadata" "Product metadata is configured" (
    -not [string]::IsNullOrWhiteSpace([string]$Tauri.productName) -and
    -not [string]::IsNullOrWhiteSpace([string]$Tauri.identifier) -and
    -not [string]::IsNullOrWhiteSpace([string]$Tauri.bundle.shortDescription) -and
    -not [string]::IsNullOrWhiteSpace([string]$Tauri.bundle.longDescription)
  ) @{
    product_name = $Tauri.productName
    identifier = $Tauri.identifier
    category = $Tauri.bundle.category
  }
)

$Passed = @($Checks | Where-Object { $_.ok }).Count
$Total = @($Checks).Count
$Report = [pscustomobject]@{
  ok = ($Passed -eq $Total)
  mode = "desktop-packaging-audit"
  passed = $Passed
  total = $Total
  summary = "Desktop packaging audit: $Passed/$Total checks passed"
  signing_status = "unsigned-development-build"
  signing_note = "Installer signing/notarization is not performed by this audit. Public distribution still requires signing credentials and a notarization/signing pass."
  checks = $Checks
}

$Stamp = (Get-Date).ToString("yyyyMMdd-HHmmss")
$ReportPath = Join-Path $OutputDir "desktop-packaging-audit-$Stamp.json"
$Report | Add-Member -NotePropertyName report_path -NotePropertyValue $ReportPath
[System.IO.File]::WriteAllText($ReportPath, ($Report | ConvertTo-Json -Depth 8), $Utf8NoBom)
$Report | ConvertTo-Json -Depth 8 -Compress

if (-not $Report.ok) {
  exit 1
}
