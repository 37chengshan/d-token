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
$PublicPrerelease = Read-RequiredText ".github\workflows\public-unsigned-prerelease.yml"
$DesktopPackage = Read-RequiredText "code\apps\desktop\package.json" | ConvertFrom-Json
$TauriConfig = Read-RequiredText "code\apps\desktop\src-tauri\tauri.conf.json" | ConvertFrom-Json
$Verification = Read-RequiredText "support\scripts\run_release_verification.ps1"
$Handoff = Read-RequiredText "support\scripts\run_release_handoff_manifest.ps1"
$OpenCodeBenchmark = Read-RequiredText "support\scripts\run_opencode_workload_benchmark.ps1"
$OpenCodeMac = Read-RequiredText "support\scripts\run_opencode_real_routed_test_macos.sh"
$PerformanceGate = Read-RequiredText "support\scripts\run_release_performance_gate.ps1"
$CodecEvidenceGate = Read-RequiredText "support\scripts\run_codec_candidate_evidence.ps1"
$RedactionAudit = Read-RequiredText "support\scripts\run_release_artifact_redaction_audit.ps1"
$CleanupAudit = Read-RequiredText "support\scripts\run_release_cleanup_audit.ps1"
$RuntimePerformanceGate = Read-RequiredText "support\scripts\run_runtime_performance_gate.py"
$RuntimePerformanceContract = Join-Path $Root "support\scripts\test_runtime_performance_gate.py"
$CodecEvidenceContract = Join-Path $Root "support\scripts\test_codec_candidate_evidence_contract.py"
$BundleBudget = Read-RequiredText "code\apps\desktop\scripts\check-bundle-budget.mjs"
$BundleAudit = Read-RequiredText "support\scripts\run_desktop_bundle_artifact_audit.ps1"
$SidecarPreparation = Read-RequiredText "support\scripts\prepare_desktop_sidecars.ps1"
$LinuxSidecarPreparation = Read-RequiredText "support\scripts\prepare_desktop_sidecar_for_checks.sh"
$SidecarSigning = Read-RequiredText "support\scripts\sign_windows_sidecars.ps1"
$SidecarWrapper = Read-RequiredText "code\apps\desktop\scripts\tauri-with-sidecar.mjs"
$CodebaseMemoryPreparation = Read-RequiredText "support\scripts\prepare_codebase_memory_artifact.py"
$CodebaseMemoryManifest = Read-RequiredText "code\runtimes\codebase-memory\artifact-manifest.json" | ConvertFrom-Json
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
foreach ($ScriptText in @($Verification, $Handoff, $PerformanceGate, $CodecEvidenceGate, $RedactionAudit, $CleanupAudit, $BundleAudit, $SidecarPreparation, $SidecarSigning, $Provenance, $ProvenanceTest, $StartupAudit, $DistributionAudit, $SigningAudit)) {
  try {
    [void][scriptblock]::Create($ScriptText)
  } catch {
    $ScriptsParse = $false
  }
}

$PerformanceContractOk = $false
$CodecEvidenceContractOk = $false
$Python = Get-Command "python" -ErrorAction SilentlyContinue
if ($null -eq $Python) { $Python = Get-Command "python3" -ErrorAction SilentlyContinue }
if ($null -ne $Python -and (Test-Path -LiteralPath $RuntimePerformanceContract -PathType Leaf)) {
  & $Python.Source $RuntimePerformanceContract 2>&1 | Out-Null
  $PerformanceContractOk = ($LASTEXITCODE -eq 0)
}
if ($null -ne $Python -and (Test-Path -LiteralPath $CodecEvidenceContract -PathType Leaf)) {
  & $Python.Source $CodecEvidenceContract 2>&1 | Out-Null
  $CodecEvidenceContractOk = ($LASTEXITCODE -eq 0)
}

$Checks = @(
  New-ContractCheck "release_scripts_parse" $ScriptsParse "Every release-gate PowerShell script parses before any gate is executed"
  New-ContractCheck "runtime_performance_contract" $PerformanceContractOk "Dynamic contract tests prove percentile calculation and fail-closed threshold/path behavior"
  New-ContractCheck "codec_candidate_evidence_contract" $CodecEvidenceContractOk "Dynamic contract tests prove candidate binding, cross-platform evidence parity, and public release wiring"
  New-ContractCheck "performance_gate_is_candidate_bound_and_metadata_only" (($PerformanceGate -match "Get-ReleaseCandidateProvenance") -and ($RuntimePerformanceGate -match '"candidate_id"') -and ($RuntimePerformanceGate -match '"metadata_only": True') -and ($RuntimePerformanceGate -match '"raw_request_stored": False') -and ($RuntimePerformanceGate -match '"raw_response_stored": False')) "The performance report binds the current candidate and stores metrics rather than request or response bodies"
  New-ContractCheck "performance_gate_has_strict_adjustable_thresholds" (($RuntimePerformanceGate -match "DTOKEN_PERF_MAX_P95_MS") -and ($RuntimePerformanceGate -match "DTOKEN_PERF_MAX_P99_MS") -and ($RuntimePerformanceGate -match "DTOKEN_PERF_MAX_FAILURE_RATE_PERCENT") -and ($RuntimePerformanceGate -match "DTOKEN_PERF_MAX_PEAK_RSS_MIB") -and ($RuntimePerformanceGate -match "DTOKEN_PERF_MIN_REQUEST_COUNT")) "Release defaults are strict while environment variables allow explicit runner calibration"
  New-ContractCheck "performance_gate_covers_concurrency_soak_errors_disconnect_rss" (($RuntimePerformanceGate -match "ThreadPoolExecutor") -and ($RuntimePerformanceGate -match "short_soak_seconds") -and ($RuntimePerformanceGate -match "error_path_observed") -and ($RuntimePerformanceGate -match "client_disconnect_observed") -and ($RuntimePerformanceGate -match "peak_rss_mib") -and ($RuntimePerformanceGate -match "ledger_request_count") -and ($RuntimePerformanceGate -match "route_decision_count")) "The real routed local workload measures concurrency, short soak, expected errors, client disconnect survival, RSS, ledger requests, and route decisions"
  New-ContractCheck "bundle_budget_writes_release_evidence" (($BundleBudget -match "--json-output") -and ($BundleBudget -match "initial_recursive_js_budget_bytes") -and ($PerformanceGate -match "check-bundle-budget.mjs")) "The existing frontend bundle budget is re-evaluated and embedded in candidate-bound release evidence"
  New-ContractCheck "platform_release_workflows_run_performance_gate" (($MacRelease -match "run_release_performance_gate.ps1") -and ($WindowsRelease -match "run_release_performance_gate.ps1")) "Both macOS and Windows release runners execute the same fail-closed performance gate"
  New-ContractCheck "release_verification_requires_performance_gate" (($Verification -match '"release_performance_gate"') -and ($Handoff -match '"release_performance_gate"')) "Release verification records the performance gate and handoff treats it as required evidence"
  New-ContractCheck "codec_evidence_is_candidate_bound_and_metadata_only" (($CodecEvidenceGate -match "Get-ReleaseCandidateProvenance") -and ($CodecEvidenceGate -match "codec_candidate_probe") -and ($CodecEvidenceGate -match "raw_corpus_stored") -and ($CodecEvidenceGate -match "secret_values_stored")) "The active codec matrix is measured in release mode, candidate-bound, and metadata-only"
  New-ContractCheck "platform_release_workflows_run_codec_evidence" (($MacRelease -match "run_codec_candidate_evidence.ps1") -and ($WindowsRelease -match "run_codec_candidate_evidence.ps1")) "Both macOS and Windows candidates execute the same active codec evidence gate"
  New-ContractCheck "release_verification_requires_codec_evidence" (($Verification -match '"codec_candidate_evidence"') -and ($Handoff -match '"codec_candidate_evidence"')) "Release verification and handoff require codec evidence"
  New-ContractCheck "platform_release_workflows_run_redaction_audit" (($MacRelease -match "run_release_artifact_redaction_audit.ps1") -and ($WindowsRelease -match "run_release_artifact_redaction_audit.ps1") -and ($MacRelease -match "support/artifacts") -and ($WindowsRelease -match "support/artifacts")) "Both release candidates scan metadata, candidate provenance, and support evidence for blocked fields and credential-shaped values"
  New-ContractCheck "platform_release_workflows_run_cleanup_audit" (($MacRelease -match "run_release_cleanup_audit.ps1") -and ($WindowsRelease -match "run_release_cleanup_audit.ps1")) "Both release runners fail closed when validation ports or test processes remain"
  New-ContractCheck "redaction_audit_accepts_explicit_candidate_paths" (($RedactionAudit -match "AdditionalPath") -and ($RedactionAudit -match "Resolve-Path") -and ($RedactionAudit -match "Get-ChildItem")) "The redaction gate can scan candidate-bound evidence outside its report directory"
  New-ContractCheck "cleanup_audit_is_cross_platform_and_does_not_persist_command_lines" (($CleanupAudit -match '\$IsWindows') -and ($CleanupAudit -match 'Get-NetTCPConnection') -and ($CleanupAudit -match 'Get-Command \"lsof\"') -and ($CleanupAudit -notmatch 'Select-Object ProcessId, Name, CommandLine')) "Cleanup evidence supports Windows and Unix runners without persisting raw process command lines"
  New-ContractCheck "ci_frontend_vitest" ($Ci -match "npm run test:run") "CI runs the current Vitest suite"
  New-ContractCheck "ci_frontend_typecheck" ($Ci -match "(?:npx|npm exec) tsc(?: --)? --noEmit") "CI runs TypeScript without emitting files"
  New-ContractCheck "ci_frontend_build" ($Ci -match "npm run build") "CI builds the desktop frontend"
  New-ContractCheck "ci_linux_desktop_dependencies" ($Ci -match "libwebkit2gtk-4.1-dev") "Linux CI installs the native Tauri/WebView dependency needed by workspace checks"
  New-ContractCheck "ci_prepares_linux_sidecar" (($Ci -match "prepare_desktop_sidecar_for_checks\.sh") -and ($LinuxSidecarPreparation -match "cargo build -p dtoken-proxy") -and ($LinuxSidecarPreparation -match "HOST_TRIPLE")) "A clean Linux checkout prepares the real host dtoken-proxy before Tauri workspace checks"
  New-ContractCheck "ci_runs_structure_scope_and_version_gates" (($Ci -match "check_structure\.py") -and ($Ci -match "check_release_scope\.py") -and ($Ci -match "check_release_version\.py")) "CI runs structure, release scope, and release version gates before build/test packaging checks"
  New-ContractCheck "ci_rust_fmt" ($Ci -match "cargo fmt --all -- --check") "CI rejects Rust formatting drift"
  New-ContractCheck "ci_rust_clippy" ($Ci -match "cargo clippy --workspace --all-targets -- -D warnings") "CI rejects Clippy warnings across every target"
  New-ContractCheck "ci_rust_workspace_tests" ($Ci -match "cargo test --workspace") "CI runs all Rust workspace tests"
  New-ContractCheck "ci_candidate_provenance_test" ($Ci -match "run_release_candidate_provenance_test.ps1") "CI proves that stale candidate stamps and artifacts are rejected"
  New-ContractCheck "desktop_release_versions_guard_exists" (($DesktopPackage.version -eq $TauriConfig.version) -and ($Ci -match "check_release_version\.py")) "Release version consistency is enforced through check_release_version.py and matching package/Tauri versions"
  New-ContractCheck "public_prerelease_workflow_exists" (($PublicPrerelease -match "push:") -and ($PublicPrerelease -match 'tags:\s*\n\s*-\s*"v\*"') -and ($PublicPrerelease -match "workflow_dispatch")) "A dedicated public unsigned prerelease workflow handles both tag-driven publication and manual validation runs"
  New-ContractCheck "public_prerelease_uses_reusable_platform_jobs" (($PublicPrerelease -match "uses:\s+\./\.github/workflows/macos-release\.yml") -and ($PublicPrerelease -match "uses:\s+\./\.github/workflows/windows-release\.yml")) "The public unsigned prerelease workflow fans into reusable macOS and Windows packaging jobs for the same ref"
  New-ContractCheck "public_prerelease_publishes_only_after_both_platform_jobs" (($PublicPrerelease -match "needs:\s*\n\s*-\s*build-macos\s*\n\s*-\s*build-windows") -and ($PublicPrerelease -match "gh release create") -and ($PublicPrerelease -match "--prerelease")) "GitHub prerelease publication is gated on both platform jobs succeeding in the same workflow run"
  New-ContractCheck "public_prerelease_is_immutable" (($PublicPrerelease -match "gh release view") -and ($PublicPrerelease -match "already exists; immutable prereleases must use a new beta tag") -and ($PublicPrerelease -match "--verify-tag")) "An existing prerelease is never overwritten; a fix must use a new immutable beta tag"
  New-ContractCheck "public_prerelease_binds_platform_evidence_to_same_commit" (($PublicPrerelease -match 'value\.get\(\"git_head\"\) == expected_sha') -and ($PublicPrerelease -match 'value\.get\(\"source_revision\"\) == expected_sha') -and ($PublicPrerelease -match 'value\.get\(\"tag\"\) == expected_tag') -and ($PublicPrerelease -match 'candidate_dirty\"\) is False')) "Both platform evidence bundles must prove the tagged commit and a clean candidate checkout before publication"
  New-ContractCheck "public_prerelease_verifies_installer_hashes" (($PublicPrerelease -match "verify_manifest_artifact\(mac_manifest_path, mac_artifact_path\)") -and ($PublicPrerelease -match "verify_manifest_artifact\(win_manifest_path, win_artifact_path\)") -and ($PublicPrerelease -match "hashlib\.sha256")) "The public DMG and NSIS installer are rehashed against their platform manifests before release creation"
  New-ContractCheck "public_prerelease_includes_both_platform_security_metadata" (($PublicPrerelease -match "macos-arm64-d-token-\$\{version\}\.cdx\.json") -and ($PublicPrerelease -match "windows-x64-d-token-\$\{version\}\.cdx\.json") -and ($PublicPrerelease -match "macos-arm64-dependency-licenses\.csv") -and ($PublicPrerelease -match "windows-x64-dependency-licenses\.csv") -and ($PublicPrerelease -match "verify_manifest_artifact\(win_manifest_path, win_security_path\)")) "The public prerelease publishes and hash-verifies SBOM, dependency-license, and security evidence from both platform jobs"
  New-ContractCheck "macos_release_is_reusable_and_manual" (($MacRelease -match "workflow_call") -and ($MacRelease -match "workflow_dispatch")) "The macOS unsigned packaging workflow is both reusable from the aggregate release and manually invokable for candidate validation"
  New-ContractCheck "macos_release_verifies_unsigned_preview_truth" (($MacRelease -match "UNSIGNED-TECHNICAL-PREVIEW\.txt") -and ($MacRelease -match "SHA256SUMS-macos-arm64-unsigned-technical-preview\.txt") -and ($MacRelease -match "run_macos_release_package_audit\.sh")) "The macOS workflow publishes unsigned-preview truth text, SHA-256 evidence, and package audit output"
  New-ContractCheck "windows_release_uses_windows_runner" (($WindowsRelease -match "windows-2022") -and ($WindowsRelease -match "workflow_call") -and ($WindowsRelease -match "workflow_dispatch")) "The Windows unsigned packaging workflow builds on a real Windows runner and supports both reusable and manual execution"
  New-ContractCheck "windows_release_verifies_unsigned_preview_truth" (($WindowsRelease -match "UNSIGNED-TECHNICAL-PREVIEW\.txt") -and ($WindowsRelease -match "SHA256SUMS-windows-x64-unsigned-technical-preview\.txt") -and ($WindowsRelease -match "run_windows_signing_readiness_audit\.ps1 -UnsignedTechnicalPreview")) "The Windows workflow states unsigned-preview truth explicitly and binds it to SHA-256 plus readiness audit output"
  New-ContractCheck "windows_release_uses_sidecar_wrapper" ($WindowsRelease -match "npm run tauri:with-sidecar -- build") "Windows packaging uses the project sidecar-aware Tauri wrapper"
  New-ContractCheck "windows_release_prepares_sidecar_before_rust_gate" (($WindowsRelease -match "Prepare Windows d-token runtime sidecar for workspace checks") -and ($WindowsRelease -match "prepare_desktop_sidecars\.ps1")) "A clean Windows checkout prepares the real MSVC dtoken-proxy before Clippy and workspace tests"
  New-ContractCheck "windows_sidecar_preparation_fetches_webview2" (($SidecarPreparation -match 'cargo fetch --locked --target \$TargetTriple') -and ($SidecarPreparation -match 'webview2-com-sys')) "A clean Windows checkout fetches the locked desktop dependency source before locating WebView2Loader.dll"
  New-ContractCheck "windows_signing_support_is_isolated" (($SidecarWrapper -match "DTOKEN_WINDOWS_SIGNING_THUMBPRINT") -and ($SidecarWrapper -match "sign_windows_sidecars\.ps1") -and ($SidecarSigning -match "dtoken-proxy-x86_64-pc-windows-msvc\.exe") -and ($SidecarSigning -match "Get-AuthenticodeSignature")) "Windows signing support remains isolated in helper scripts and is not implied by the unsigned prerelease workflow"
  New-ContractCheck "codebase_memory_lightweight_distribution" ((@($TauriConfig.bundle.externalBin) -contains "binaries/dtoken-codebase-memory-launcher") -and -not (@($TauriConfig.bundle.externalBin) -contains "binaries/codebase-memory-mcp")) "Tauri bundles only the constrained launcher; the pinned upstream is user-imported"
  New-ContractCheck "codebase_memory_release_is_pinned" (($CodebaseMemoryManifest.upstream.tag -eq "v0.9.0") -and ($CodebaseMemoryManifest.upstream.release_immutable -eq $true) -and (@($CodebaseMemoryManifest.artifacts).Count -eq 4)) "Codebase Memory desktop artifacts are pinned to immutable v0.9.0 entries"
  New-ContractCheck "codebase_memory_user_import_verification" (($CodebaseMemoryPreparation -match "archive_sha256") -and ($CodebaseMemoryPreparation -match "binary_sha256") -and ($WindowsRelease -match "Prepare pinned user-imported Code Graph component") -and ($WindowsRelease -match "--require-attestation")) "Release smoke verifies a separately imported pinned upstream without bundling it"
  New-ContractCheck "codebase_memory_launcher_is_packaged_for_windows_release" (($SidecarSigning -match "dtoken-codebase-memory-launcher-x86_64-pc-windows-msvc\.exe") -and ($WindowsRelease -match "dtoken-codebase-memory-launcher\.exe")) "The d-token-owned Windows Codebase Memory launcher remains part of the packaged Windows unsigned prerelease set"
  New-ContractCheck "linux_codebase_memory_uses_lightweight_launcher" (($LinuxSidecarPreparation -match "dtoken-codebase-memory-launcher") -and ($LinuxSidecarPreparation -match "user-imported-pinned-upstream") -and -not ($LinuxSidecarPreparation -match 'codebase-memory-mcp-\$HOST_TRIPLE')) "Linux CI stages only the lightweight launcher"
  New-ContractCheck "windows_release_installs_current_candidate" (($WindowsRelease -match "run_installed_desktop_startup_audit\.ps1 -SilentInstallLatest") -and ($WindowsRelease -match "uninstall\.exe")) "Windows release validation installs, launches, verifies, and uninstalls the current NSIS candidate"
  New-ContractCheck "windows_release_writes_sha256_manifest" (($WindowsRelease -match "SHA256SUMS-windows-x64-unsigned-technical-preview\.txt") -and ($WindowsRelease -match "Get-FileHash") -and ($WindowsRelease -match 'dtoken-agent-bridge\.exe')) "The Windows unsigned prerelease artifact includes the Agent bridge in its SHA-256 manifest"
  New-ContractCheck "verification_frontend_vitest" (($Verification -match '"frontend_vitest"') -and ($Verification -match "npm run test:run")) "No-touch verification records Vitest as its own step"
  New-ContractCheck "verification_frontend_dependencies" (($Verification -match '"frontend_dependencies"') -and ($Verification -match "npm ci")) "No-touch verification installs the exact frontend dependency lock"
  New-ContractCheck "verification_frontend_typecheck" (($Verification -match '"frontend_typecheck"') -and ($Verification -match "(?:npx|npm exec) tsc(?: --)? --noEmit")) "No-touch verification records TypeScript as its own step"
  New-ContractCheck "verification_rust_fmt" (($Verification -match '"rust_fmt"') -and ($Verification -match "cargo fmt --all -- --check")) "No-touch verification records Rust fmt as its own step"
  New-ContractCheck "verification_rust_clippy" (($Verification -match '"rust_clippy"') -and ($Verification -match "cargo clippy --workspace --all-targets -- -D warnings")) "No-touch verification records workspace Clippy as its own step"
  New-ContractCheck "verification_rust_workspace_tests" (($Verification -match '"rust_workspace_tests"') -and ($Verification -match "cargo test --workspace")) "No-touch verification records all workspace tests"
  New-ContractCheck "verification_candidate_provenance_test" (($Verification -match '"release_candidate_provenance"') -and ($Verification -match "run_release_candidate_provenance_test.ps1")) "No-touch verification dynamically checks candidate freshness behavior"
  New-ContractCheck "verification_opencode_opt_in" (($Verification -match '\[switch\]\$IncludeOpenCode') -and ($Verification -match 'if \(\$IncludeOpenCode\)') -and ($Verification -notmatch 'if \(-not \$SkipOpenCode\)')) "Real OpenCode is opt-in; the default gate is no-touch"
  New-ContractCheck "verification_opencode_summary_requires_candidate_and_runtime_identity" (($Verification -match 'candidate_id') -and ($Verification -match 'runtime_identity') -and ($Verification -match 'route_decision_count') -and ($Verification -match 'request_log_count') -and ($Verification -match 'ledger') -and ($Verification -match 'metrics') -and ($Verification -match 'ccr')) "Release verification requires a machine-readable OpenCode summary with candidate, runtime identity, route, ledger, metrics, and CCR evidence"
  New-ContractCheck "opencode_benchmark_writes_machine_readable_summary" (($OpenCodeBenchmark -match 'candidate_id = \$Candidate.candidate_id') -and ($OpenCodeBenchmark -match 'runtime_identity = \$Health.runtime_identity') -and ($OpenCodeBenchmark -match 'service_url = "http://127.0.0.1:') -and ($OpenCodeBenchmark -match 'ledger = \$Ledger') -and ($OpenCodeBenchmark -match 'metrics = \$Metrics') -and ($OpenCodeBenchmark -match 'ccr =') -and ($OpenCodeBenchmark -match 'report_path')) "The OpenCode workload benchmark writes candidate-bound, runtime-bound, ledger-bound, metrics-bound, and CCR-bound machine-readable evidence"
  New-ContractCheck "opencode_macos_report_writes_candidate_runtime_restore_and_metrics" (($OpenCodeMac -match 'candidate_id') -and ($OpenCodeMac -match 'runtime_identity') -and ($OpenCodeMac -match 'service_url') -and ($OpenCodeMac -match 'ledger') -and ($OpenCodeMac -match 'metrics') -and ($OpenCodeMac -match 'ccr') -and ($OpenCodeMac -match 'config_transaction') -and ($OpenCodeMac -match 'restore_status')) "The macOS real OpenCode routed acceptance report carries candidate, runtime identity, ledger, metrics, CCR, and restore evidence"
  New-ContractCheck "handoff_binds_opencode_report_to_candidate" (($Handoff -match 'opencode_real_workload') -and ($Handoff -match 'opencode_report_bound_to_current_candidate') -and ($Handoff -match 'candidate_id') -and ($Handoff -match 'runtime_identity') -and ($Handoff -match 'ledger') -and ($Handoff -match 'metrics') -and ($Handoff -match 'ccr')) "The release handoff manifest binds the OpenCode report to the current candidate and validates its runtime, ledger, metrics, and CCR evidence"
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
