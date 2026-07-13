param()

$ErrorActionPreference = "Stop"
$Root = Resolve-Path (Join-Path $PSScriptRoot "..\..")

function Read-RequiredText {
  param([string]$RelativePath)

  $Path = Join-Path $Root $RelativePath
  if (-not (Test-Path -LiteralPath $Path)) {
    throw "required release-gate file is missing: $RelativePath"
  }
  return Get-Content -LiteralPath $Path -Raw
}

function New-ContractCheck {
  param(
    [string]$Id,
    [bool]$Ok,
    [string]$Expected
  )

  [pscustomobject]@{
    id = $Id
    ok = $Ok
    expected = $Expected
  }
}

$Ci = Read-RequiredText ".github\workflows\ci.yml"
$MacRelease = Read-RequiredText ".github\workflows\macos-release.yml"
$WindowsRelease = Read-RequiredText ".github\workflows\windows-release.yml"
$DesktopPackage = Read-RequiredText "code\apps\desktop\package.json" | ConvertFrom-Json
$TauriConfig = Read-RequiredText "code\apps\desktop\src-tauri\tauri.conf.json" | ConvertFrom-Json
$DesktopCargo = Read-RequiredText "code\apps\desktop\src-tauri\Cargo.toml"
$DesktopCargoVersionMatch = [regex]::Match($DesktopCargo, '(?ms)^\[package\].*?^version\s*=\s*"([^"]+)"')
$Verification = Read-RequiredText "support\scripts\run_release_verification.ps1"
$Handoff = Read-RequiredText "support\scripts\run_release_handoff_manifest.ps1"
$BundleAudit = Read-RequiredText "support\scripts\run_desktop_bundle_artifact_audit.ps1"
$SidecarPreparation = Read-RequiredText "support\scripts\prepare_desktop_sidecars.ps1"
$SidecarSigning = Read-RequiredText "support\scripts\sign_windows_sidecars.ps1"
$SidecarWrapper = Read-RequiredText "code\apps\desktop\scripts\tauri-with-sidecar.mjs"
$ProvenanceTest = Read-RequiredText "support\scripts\run_release_candidate_provenance_test.ps1"
$StartupAudit = Read-RequiredText "support\scripts\run_installed_desktop_startup_audit.ps1"
$DistributionAudit = Read-RequiredText "support\scripts\run_release_distribution_audit.ps1"
$SigningAudit = Read-RequiredText "support\scripts\run_windows_signing_readiness_audit.ps1"
$ProvenancePath = Join-Path $Root "support\scripts\release_candidate_provenance.ps1"
$Provenance = if (Test-Path -LiteralPath $ProvenancePath) {
  Get-Content -LiteralPath $ProvenancePath -Raw
} else {
  ""
}
$ScriptsParse = $true
foreach ($ScriptText in @($Verification, $Handoff, $BundleAudit, $SidecarPreparation, $SidecarSigning, $Provenance, $ProvenanceTest, $StartupAudit, $DistributionAudit, $SigningAudit)) {
  try {
    [void][scriptblock]::Create($ScriptText)
  } catch {
    $ScriptsParse = $false
  }
}

$Checks = @(
  New-ContractCheck "release_scripts_parse" $ScriptsParse "Every release-gate PowerShell script parses before any gate is executed"
  New-ContractCheck "ci_frontend_vitest" ($Ci -match "npm run test:run") "CI runs the current Vitest suite"
  New-ContractCheck "ci_frontend_typecheck" ($Ci -match "(?:npx|npm exec) tsc(?: --)? --noEmit") "CI runs TypeScript without emitting files"
  New-ContractCheck "ci_frontend_build" ($Ci -match "npm run build") "CI builds the desktop frontend"
  New-ContractCheck "ci_linux_desktop_dependencies" ($Ci -match "libwebkit2gtk-4.1-dev") "Linux CI installs the native Tauri/WebView dependency needed by workspace checks"
  New-ContractCheck "ci_rust_fmt" ($Ci -match "cargo fmt --all -- --check") "CI rejects Rust formatting drift"
  New-ContractCheck "ci_rust_clippy" ($Ci -match "cargo clippy --workspace --all-targets -- -D warnings") "CI rejects Clippy warnings across every target"
  New-ContractCheck "ci_rust_workspace_tests" ($Ci -match "cargo test --workspace") "CI runs all Rust workspace tests"
  New-ContractCheck "ci_candidate_provenance_test" ($Ci -match "run_release_candidate_provenance_test.ps1") "CI proves that stale candidate stamps and artifacts are rejected"
  New-ContractCheck "desktop_release_versions_match" ($DesktopCargoVersionMatch.Success -and $DesktopPackage.version -eq $TauriConfig.version -and $DesktopPackage.version -eq $DesktopCargoVersionMatch.Groups[1].Value) "package.json, Tauri config, and the desktop Rust crate use one release version"
  New-ContractCheck "macos_public_release_is_fail_closed" (($MacRelease -match "APPLE_CERTIFICATE") -and ($MacRelease -match "notarytool submit") -and ($MacRelease -match "spctl --assess") -and ($MacRelease -notmatch "inputs\.notarize")) "The macOS public release workflow cannot upload an unsigned or unnotarized candidate"
  New-ContractCheck "macos_release_uploads_only_stapled_distribution" (($MacRelease -match "bundle/dmg/\*\.dmg") -and ($MacRelease -notmatch "bundle/macos/\*\.app") -and ($MacRelease -match "stapler validate")) "Only the notarized and stapled DMG is uploaded as the public macOS distribution artifact"
  New-ContractCheck "macos_release_writes_sha256_manifest" (($MacRelease -match "SHA256SUMS-macos\.txt") -and ($MacRelease -match "shasum -a 256")) "The macOS release artifact includes a SHA-256 manifest"
  New-ContractCheck "windows_release_uses_windows_runner" (($WindowsRelease -match "windows-2022") -and ($WindowsRelease -match "workflow_dispatch")) "The Windows release workflow builds on a real Windows runner and is manually dispatched"
  New-ContractCheck "windows_release_is_fail_closed" (($WindowsRelease -match "WINDOWS_CERTIFICATE is required") -and ($WindowsRelease -match "certificateThumbprint") -and ($WindowsRelease -match "Get-AuthenticodeSignature") -and ($WindowsRelease -match 'Status -ne "Valid"')) "The Windows public release workflow requires signing credentials and rejects invalid signatures"
  New-ContractCheck "windows_release_uses_sidecar_wrapper" ($WindowsRelease -match "npm run tauri:with-sidecar -- build") "Windows packaging uses the project sidecar-aware Tauri wrapper"
  New-ContractCheck "windows_release_signs_exact_sidecar_inputs" (($SidecarWrapper -match "DTOKEN_WINDOWS_SIGNING_THUMBPRINT") -and ($SidecarWrapper -match "sign_windows_sidecars\.ps1") -and ($SidecarSigning -match "dtoken-proxy-x86_64-pc-windows-msvc\.exe") -and ($SidecarSigning -match "Get-AuthenticodeSignature") -and ($WindowsRelease -match "DTOKEN_WINDOWS_SIGNING_THUMBPRINT")) "The exact prepared and release sidecar bytes are signed and verified before Tauri bundles them"
  New-ContractCheck "windows_release_installs_current_candidate" (($WindowsRelease -match "run_installed_desktop_startup_audit\.ps1 -SilentInstallLatest") -and ($WindowsRelease -match "uninstall\.exe")) "Windows release validation installs, launches, verifies, and uninstalls the current NSIS candidate"
  New-ContractCheck "windows_release_verifies_installed_signatures" (($WindowsRelease -match "Verify installed Authenticode signatures") -and ($WindowsRelease -match 'd-token/dtoken-proxy\.exe') -and ($WindowsRelease -match 'Installed Authenticode verification failed')) "The installed app and exact installed runtime sidecar retain valid Authenticode signatures"
  New-ContractCheck "windows_release_writes_sha256_manifest" (($WindowsRelease -match "SHA256SUMS-windows\.txt") -and ($WindowsRelease -match "Get-FileHash")) "The Windows release artifact includes a SHA-256 manifest"
  New-ContractCheck "verification_frontend_vitest" (($Verification -match '"frontend_vitest"') -and ($Verification -match "npm run test:run")) "No-touch verification records Vitest as its own step"
  New-ContractCheck "verification_frontend_dependencies" (($Verification -match '"frontend_dependencies"') -and ($Verification -match "npm ci")) "No-touch verification installs the exact frontend dependency lock"
  New-ContractCheck "verification_frontend_typecheck" (($Verification -match '"frontend_typecheck"') -and ($Verification -match "(?:npx|npm exec) tsc(?: --)? --noEmit")) "No-touch verification records TypeScript as its own step"
  New-ContractCheck "verification_rust_fmt" (($Verification -match '"rust_fmt"') -and ($Verification -match "cargo fmt --all -- --check")) "No-touch verification records Rust fmt as its own step"
  New-ContractCheck "verification_rust_clippy" (($Verification -match '"rust_clippy"') -and ($Verification -match "cargo clippy --workspace --all-targets -- -D warnings")) "No-touch verification records workspace Clippy as its own step"
  New-ContractCheck "verification_rust_workspace_tests" (($Verification -match '"rust_workspace_tests"') -and ($Verification -match "cargo test --workspace")) "No-touch verification records all workspace tests"
  New-ContractCheck "verification_candidate_provenance_test" (($Verification -match '"release_candidate_provenance"') -and ($Verification -match "run_release_candidate_provenance_test.ps1")) "No-touch verification dynamically checks candidate freshness behavior"
  New-ContractCheck "verification_opencode_opt_in" (($Verification -match '\[switch\]\$IncludeOpenCode') -and ($Verification -match 'if \(\$IncludeOpenCode\)') -and ($Verification -notmatch 'if \(-not \$SkipOpenCode\)')) "Real OpenCode is opt-in; the default gate is no-touch"
  New-ContractCheck "provenance_helper_exists" (Test-Path -LiteralPath $ProvenancePath) "A shared release-candidate provenance helper exists"
  New-ContractCheck "provenance_records_required_fields" (($Provenance -match "candidate_id") -and ($Provenance -match "git_head") -and ($Provenance -match "git_dirty") -and ($Provenance -match "platform") -and ($Provenance -match "node") -and ($Provenance -match "rustc")) "Candidate identity includes Git, dirty-state, platform, Node, and Rust evidence"
  New-ContractCheck "verification_embeds_candidate" (($Verification -match "Get-ReleaseCandidateProvenance") -and ($Verification -match 'candidate\s*=\s*\$Candidate')) "Release report embeds the current candidate provenance"
  New-ContractCheck "handoff_uses_explicit_release_report" (($Handoff -match '\[string\]\$ReleaseReportPath') -and ($Handoff -match "candidate_id_matches")) "Handoff validates an explicit release report against the current candidate"
  New-ContractCheck "handoff_binds_step_reports" (($Handoff -match "Get-ReleaseStepSummary") -and ($Handoff -match 'Id "release_distribution_audit"') -and ($Handoff -match 'Id "release_artifact_redaction_audit"')) "Handoff reads readiness evidence from the explicit current verification report"
  New-ContractCheck "handoff_rejects_old_startup_evidence" (($Handoff -match "StartupCandidateMatches") -and ($Handoff -match "installed_startup_audit_passed_for_candidate")) "Handoff does not reuse installed-startup evidence from an older candidate"
  New-ContractCheck "packaging_writes_candidate_stamp" (($SidecarPreparation -match "Write-ReleaseCandidateStamp") -and ($SidecarPreparation -match "d-token-release-candidate.json")) "Release sidecar preparation stamps the source candidate before packaging"
  New-ContractCheck "bundle_rejects_stale_candidate" (($BundleAudit -match "candidate_stamp_matches_worktree") -and ($BundleAudit -match "artifact_built_after_candidate_stamp")) "Bundle audit rejects stale artifacts from another worktree state"
  New-ContractCheck "distribution_rejects_stale_candidate" (($DistributionAudit -match "candidate_stamp_matches_worktree") -and ($DistributionAudit -match "artifact_built_after_candidate_stamp")) "Standalone distribution audit cannot mark an old bundle as the current release candidate"
  New-ContractCheck "signing_readiness_binds_candidate" (($SigningAudit -match "candidate_stamp_matches_worktree") -and ($SigningAudit -match "artifact_built_after_candidate_stamp")) "Signing readiness cannot promote artifacts from a different candidate"
  New-ContractCheck "startup_audit_binds_current_bundle" (($StartupAudit -match "candidate_stamp_matches_worktree") -and ($StartupAudit -match "installed_files_match_current_bundle")) "Installed startup evidence is bound to the current candidate and release bundle"
)

$Failed = @($Checks | Where-Object { -not $_.ok })
$Report = [pscustomobject]@{
  ok = ($Failed.Count -eq 0)
  mode = "release-gate-contract-test"
  passed = $Checks.Count - $Failed.Count
  total = $Checks.Count
  failed = $Failed
  checks = $Checks
}

$Report | ConvertTo-Json -Depth 6 -Compress
if ($Failed.Count -gt 0) {
  exit 1
}
