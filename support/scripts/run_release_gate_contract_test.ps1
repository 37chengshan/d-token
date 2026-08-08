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
$GateStatusContract = Join-Path $Root "support\tests\test_release_gate_status_aggregation.py"
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
$DaclAclTest = Read-RequiredText "support\scripts\run_windows_dacl_acl_tests.ps1"
$NsisUpgradeRollbackTest = Read-RequiredText "support\scripts\run_nsis_upgrade_rollback_test.ps1"
$WslRtkIntegrityTest = Read-RequiredText "support\scripts\run_wsl_rtk_integrity_test.ps1"
$FanInArtifactContract = Read-RequiredText "support\scripts\run_fan_in_artifact_contract.ps1"
$GateStatusAggregator = Read-RequiredText "support\scripts\generate_release_gate_status.py"
$ScriptsParse = $true
foreach ($ScriptText in @($Verification, $Handoff, $PerformanceGate, $CodecEvidenceGate, $RedactionAudit, $CleanupAudit, $BundleAudit, $SidecarPreparation, $SidecarSigning, $Provenance, $ProvenanceTest, $StartupAudit, $DistributionAudit, $SigningAudit, $DaclAclTest, $NsisUpgradeRollbackTest, $WslRtkIntegrityTest, $FanInArtifactContract)) {
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
$GateStatusContractOk = $false
if ($null -ne $Python -and (Test-Path -LiteralPath $GateStatusContract -PathType Leaf)) {
  & $Python.Source $GateStatusContract 2>&1 | Out-Null
  $GateStatusContractOk = ($LASTEXITCODE -eq 0)
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
  New-ContractCheck "public_prerelease_uses_registry_owned_codec_evidence" (($PublicPrerelease -match "def codec_ids") -and ($PublicPrerelease -match "duplicate codec ids") -and ($PublicPrerelease -match "codec registry ids differ across release platforms") -and ($PublicPrerelease -notmatch "EXPECTED_CODEC_COUNT")) "Public release trusts candidate-bound Rust registry evidence and rejects empty, duplicate, or cross-platform codec-id drift without a second magic count"
  New-ContractCheck "platform_release_workflows_run_redaction_audit" (($MacRelease -match "run_release_artifact_redaction_audit.ps1") -and ($WindowsRelease -match "run_release_artifact_redaction_audit.ps1") -and ($MacRelease -match "support/artifacts") -and ($WindowsRelease -match "support/artifacts")) "Both release candidates scan metadata, candidate provenance, and support evidence for blocked fields and credential-shaped values"
  New-ContractCheck "platform_release_workflows_run_cleanup_audit" (($MacRelease -match "run_release_cleanup_audit.ps1") -and ($WindowsRelease -match "run_release_cleanup_audit.ps1")) "Both release runners fail closed when validation ports or test processes remain"
  New-ContractCheck "redaction_audit_accepts_explicit_candidate_paths" (($RedactionAudit -match "AdditionalPath") -and ($RedactionAudit -match "Resolve-Path") -and ($RedactionAudit -match "Get-ChildItem")) "The redaction gate can scan candidate-bound evidence outside its report directory"
  New-ContractCheck "cleanup_audit_is_cross_platform_and_does_not_persist_command_lines" (($CleanupAudit -match '\$IsWindows') -and ($CleanupAudit -match 'Get-NetTCPConnection') -and ($CleanupAudit -match 'Get-Command \"lsof\"') -and ($CleanupAudit -notmatch 'Select-Object ProcessId, Name, CommandLine')) "Cleanup evidence supports Windows and Unix runners without persisting raw process command lines"
  New-ContractCheck "cleanup_audit_requires_powershell_7_before_execution" ($CleanupAudit.TrimStart().StartsWith("#Requires -Version 7.0")) "The cleanup audit fails before parsing unsupported PowerShell runtimes"
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
  New-ContractCheck "ci_version_tag_only_on_real_tag_refs" (($Ci -match "GITHUB_REF_TYPE") -and ($Ci -match "check_release_version\.py --tag") -and ($Ci -match "unset GITHUB_REF_NAME")) "CI passes GITHUB_REF_NAME to check_release_version.py as a release tag only when GITHUB_REF_TYPE is a real tag; branch/PR ref names are never treated as release tags"
  New-ContractCheck "public_prerelease_workflow_exists" (($PublicPrerelease -match "push:") -and ($PublicPrerelease -match 'tags:\s*\n\s*-\s*"v\*"') -and ($PublicPrerelease -match "workflow_dispatch")) "A dedicated public unsigned prerelease workflow handles both tag-driven publication and manual validation runs"
  New-ContractCheck "public_prerelease_uses_reusable_platform_jobs" (($PublicPrerelease -match "uses:\s+\./\.github/workflows/macos-release\.yml") -and ($PublicPrerelease -match "uses:\s+\./\.github/workflows/windows-release\.yml")) "The public unsigned prerelease workflow fans into reusable macOS and Windows packaging jobs for the same ref"
  New-ContractCheck "public_prerelease_publishes_only_after_both_platform_jobs" (($PublicPrerelease -match "needs:\s*\n\s*-\s*build-macos\s*\n\s*-\s*build-windows") -and ($PublicPrerelease -match "gh release create") -and ($PublicPrerelease -match "--prerelease")) "GitHub prerelease publication is gated on both platform jobs succeeding in the same workflow run"
  New-ContractCheck "public_prerelease_is_immutable" (($PublicPrerelease -match "gh release view") -and ($PublicPrerelease -match "already exists; immutable prereleases must use a new beta tag") -and ($PublicPrerelease -match "--verify-tag")) "An existing prerelease is never overwritten; a fix must use a new immutable beta tag"
  New-ContractCheck "public_prerelease_binds_platform_evidence_to_same_commit" (($PublicPrerelease -match 'value\.get\(\"git_head\"\) == expected_sha') -and ($PublicPrerelease -match 'value\.get\(\"source_revision\"\) == expected_sha') -and ($PublicPrerelease -match 'value\.get\(\"tag\"\) == expected_tag') -and ($PublicPrerelease -match 'candidate_dirty\"\) is False')) "Both platform evidence bundles must prove the tagged commit and a clean candidate checkout before publication"
  New-ContractCheck "public_prerelease_verifies_installer_hashes" (($PublicPrerelease -match "verify_manifest_artifact\(mac_manifest_path, mac_artifact_path\)") -and ($PublicPrerelease -match "verify_manifest_artifact\(win_manifest_path, win_artifact_path\)") -and ($PublicPrerelease -match "hashlib\.sha256")) "The public DMG and NSIS installer are rehashed against their platform manifests before release creation"
  New-ContractCheck "public_prerelease_includes_both_platform_security_metadata" (($PublicPrerelease -match "macos-arm64-d-token-\$\{version\}\.cdx\.json") -and ($PublicPrerelease -match "windows-x64-d-token-\$\{version\}\.cdx\.json") -and ($PublicPrerelease -match "macos-arm64-dependency-licenses\.csv") -and ($PublicPrerelease -match "windows-x64-dependency-licenses\.csv") -and ($PublicPrerelease -match "verify_manifest_artifact\(win_manifest_path, win_security_path\)")) "The public prerelease publishes and hash-verifies SBOM, dependency-license, and security evidence from both platform jobs"
  New-ContractCheck "macos_release_is_reusable_and_manual" (($MacRelease -match "workflow_call") -and ($MacRelease -match "workflow_dispatch")) "The macOS unsigned packaging workflow is both reusable from the aggregate release and manually invokable for candidate validation"
  New-ContractCheck "macos_release_verifies_unsigned_preview_truth" (($MacRelease -match "UNSIGNED-TECHNICAL-PREVIEW\.txt") -and ($MacRelease -match "SHA256SUMS-macos-arm64-unsigned-technical-preview\.txt") -and ($MacRelease -match "run_macos_release_package_audit\.sh")) "The macOS workflow publishes unsigned-preview truth text, SHA-256 evidence, and package audit output"
  New-ContractCheck "macos_release_hashes_only_metadata_files" (($MacRelease -match "find code/target/release/metadata -type f -print0") -and ($MacRelease -match 'METADATA_ARTIFACTS\[@\]') -and ($MacRelease -match 'test "\$\{#METADATA_ARTIFACTS\[@\]\}" -gt 0')) "The macOS SHA-256 manifest enumerates regular metadata files and rejects an empty evidence directory"
  New-ContractCheck "windows_release_uses_windows_runner" (($WindowsRelease -match "windows-2022") -and ($WindowsRelease -match "workflow_call") -and ($WindowsRelease -match "workflow_dispatch")) "The Windows unsigned packaging workflow builds on a real Windows runner and supports both reusable and manual execution"
  New-ContractCheck "windows_release_verifies_unsigned_preview_truth" (($WindowsRelease -match "UNSIGNED-TECHNICAL-PREVIEW\.txt") -and ($WindowsRelease -match "SHA256SUMS-windows-x64-unsigned-technical-preview\.txt") -and ($WindowsRelease -match "run_windows_signing_readiness_audit\.ps1 -UnsignedTechnicalPreview")) "The Windows workflow states unsigned-preview truth explicitly and binds it to SHA-256 plus readiness audit output"
  New-ContractCheck "windows_release_uses_sidecar_wrapper" ($WindowsRelease -match "npm run tauri:with-sidecar -- build") "Windows packaging uses the project sidecar-aware Tauri wrapper"
  New-ContractCheck "windows_release_prepares_sidecar_before_rust_gate" (($WindowsRelease -match "Prepare Windows d-token runtime sidecar for workspace checks") -and ($WindowsRelease -match "prepare_desktop_sidecars\.ps1")) "A clean Windows checkout prepares the real MSVC dtoken-proxy before Clippy and workspace tests"
  New-ContractCheck "windows_sidecar_preparation_fetches_webview2" (($SidecarPreparation -match 'cargo fetch --locked --target \$TargetTriple') -and ($SidecarPreparation -match 'webview2-com-sys')) "A clean Windows checkout fetches the locked desktop dependency source before locating WebView2Loader.dll"
  New-ContractCheck "windows_signing_support_is_isolated" (($SidecarWrapper -match "DTOKEN_WINDOWS_SIGNING_THUMBPRINT") -and ($SidecarWrapper -match "sign_windows_sidecars\.ps1") -and ($SidecarSigning -match "dtoken-proxy-x86_64-pc-windows-msvc\.exe") -and ($SidecarSigning -match "Get-AuthenticodeSignature")) "Windows signing support remains isolated in helper scripts and is not implied by the unsigned prerelease workflow"
  New-ContractCheck "codebase_memory_lightweight_distribution" ((@($TauriConfig.bundle.externalBin) -contains "binaries/dtoken-codebase-memory-launcher") -and -not (@($TauriConfig.bundle.externalBin) -contains "binaries/codebase-memory-mcp")) "Tauri bundles only the constrained launcher; the pinned upstream is user-imported"
  New-ContractCheck "codebase_memory_release_is_pinned" (($CodebaseMemoryManifest.upstream.tag -eq "v0.9.1-rc.1") -and ($CodebaseMemoryManifest.upstream.release_immutable -eq $true) -and (@($CodebaseMemoryManifest.artifacts).Count -eq 4)) "Codebase Memory desktop artifacts are pinned to immutable v0.9.1-rc.1 entries"
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
     New-ContractCheck "dacl_acl_test_script_exists" (Test-Path -LiteralPath (Join-Path $PSScriptRoot "run_windows_dacl_acl_tests.ps1")) "A Windows DACL/ACL test runner script exists"
  New-ContractCheck "dacl_acl_test_script_has_platform_gate" ($DaclAclTest -match "IsWindowsHost") -and ($DaclAclTest -match "exit 2") "The DACL/ACL test runner skips on non-Windows"
  New-ContractCheck "dacl_acl_test_script_has_all_test_groups" (($DaclAclTest -match "directory_acl_is_protected_and_limited_to_current_user_and_system") -and ($DaclAclTest -match "open_applies_protected_private_dacl") -and ($DaclAclTest -match "dacl")) "The DACL/ACL test runner targets real crate test names (module paths or test names), not function names that match zero tests"
  New-ContractCheck "dacl_acl_test_script_fails_on_zero_tests" ($DaclAclTest -match "test result:") -and ($DaclAclTest -match "0 tests run") "The DACL/ACL test runner treats a zero-test cargo result as a failure instead of a false pass"
  New-ContractCheck "dacl_acl_test_ci_windows_workflow" ($Ci -match "run_windows_dacl_acl_tests.ps1") "The CI windows workflow invokes the DACL/ACL test runner"
  New-ContractCheck "nsis_upgrade_rollback_script_exists" (Test-Path -LiteralPath (Join-Path $PSScriptRoot "run_nsis_upgrade_rollback_test.ps1")) "An NSIS upgrade/rollback test script exists"
  New-ContractCheck "nsis_upgrade_rollback_script_has_platform_gate" ($NsisUpgradeRollbackTest -match "IsWindowsHost") -and ($NsisUpgradeRollbackTest -match "exit 2") "The NSIS upgrade/rollback test skips on non-Windows"
  New-ContractCheck "nsis_upgrade_rollback_has_all_steps" ($NsisUpgradeRollbackTest -match "clean_install" -and $NsisUpgradeRollbackTest -match "verify_installed_files" -and $NsisUpgradeRollbackTest -match "upgrade_reinstall" -and $NsisUpgradeRollbackTest -match "verify_after_reinstall" -and $NsisUpgradeRollbackTest -match "launch_installed_app" -and $NsisUpgradeRollbackTest -match "clean_uninstall" -and $NsisUpgradeRollbackTest -match "rollback_restore") "The NSIS upgrade/rollback test covers clean install, file verification, upgrade, post-upgrade verification, app launch, uninstall, and rollback — all 7 steps"
  New-ContractCheck "nsis_upgrade_rollback_has_previous_installer" (($NsisUpgradeRollbackTest -match "PreviousInstaller") -and ($WindowsRelease -match "resolve_previous_installer") -and ($WindowsRelease -match "previous_release_tag")) "The NSIS test installs an older public installer first, then the new candidate, then restores — a real old-version -> new-version -> rollback contract, not a same-candidate reinstall"
  New-ContractCheck "nsis_upgrade_rollback_ci_windows_release" ($WindowsRelease -match "run_nsis_upgrade_rollback_test.ps1") "The Windows release workflow invokes the NSIS upgrade/rollback test"
  New-ContractCheck "windows_release_gates_are_fail_closed" ((-not ($WindowsRelease -match "continue-on-error")) -and ($WindowsRelease -match "run_wsl_rtk_integrity_test.ps1") -and ($WindowsRelease -match "run_nsis_upgrade_rollback_test.ps1") -and ($WindowsRelease -match "run_managed_rtk_candidate_test_windows.ps1")) "NSIS, WSL RTK, and managed RTK candidate gates are not neutralized by continue-on-error; a failed gate blocks the Windows release"
  New-ContractCheck "wsl_rtk_integrity_script_exists" (Test-Path -LiteralPath (Join-Path $PSScriptRoot "run_wsl_rtk_integrity_test.ps1")) "A WSL RTK integrity test script exists"
  New-ContractCheck "wsl_rtk_integrity_script_has_platform_gate" ($WslRtkIntegrityTest -match "IsWindowsHost") -and ($WslRtkIntegrityTest -match "exit 2") "The WSL RTK integrity test skips on non-Windows"
  New-ContractCheck "wsl_rtk_integrity_checks_fixture_contract" ($WslRtkIntegrityTest -match "SECRET=must-not-persist" -and $WslRtkIntegrityTest -match "4096") "The WSL RTK integrity test validates the fixture byte contract"
  New-ContractCheck "wsl_rtk_integrity_fails_closed_when_wsl_missing" (($WslRtkIntegrityTest -match "wsl_available") -and ($WslRtkIntegrityTest -match "exit 1")) "The WSL RTK integrity test fails when wsl.exe is absent on a Windows host instead of reporting a false pass"
  New-ContractCheck "wsl_rtk_integrity_ci_windows_release" ($WindowsRelease -match "run_wsl_rtk_integrity_test.ps1") "The Windows release workflow invokes the WSL RTK integrity test"
  New-ContractCheck "fan_in_artifact_contract_script_exists" (Test-Path -LiteralPath (Join-Path $PSScriptRoot "run_fan_in_artifact_contract.ps1")) "A fan-in artifact contract verification script exists"
  New-ContractCheck "fan_in_artifact_contract_checks_commit_and_digest" ($FanInArtifactContract -match "same_commit_across_platforms" -and $FanInArtifactContract -match "same_source_digest" -and $FanInArtifactContract -match "commit_matches_expected") "The fan-in contract verifies same commit and source digest across platforms"
  New-ContractCheck "fan_in_artifact_contract_checks_shasum_sbom_security_receipts" ($FanInArtifactContract -match "mac_sha256_manifest" -and $FanInArtifactContract -match "win_sbom" -and $FanInArtifactContract -match "mac_security_scan" -and $FanInArtifactContract -match "win_test_receipts") "The fan-in contract verifies SHA-256, SBOM, security scan, and test receipts per platform"
  New-ContractCheck "fan_in_artifact_contract_secret_scanning" ($FanInArtifactContract -match "no_secrets_in_candidate_jsons" -and $FanInArtifactContract -match "sk-[A-Za-z0-9]") "The fan-in contract scans for secrets in candidate JSONs"
  New-ContractCheck "fan_in_artifact_contract_fail_closed" ($FanInArtifactContract -match "exit 1" -and $FanInArtifactContract -match "exit 2") "The fan-in contract is fail-closed on missing platform or critical mismatch"
  New-ContractCheck "fan_in_artifact_contract_single_platform_exits_2" ($FanInArtifactContract -match "Only one platform directory found") -and ($FanInArtifactContract -match "exit 2") "A single-platform host exits 2 (not a fan-in staging environment) instead of failing the platform build job"
  New-ContractCheck "fan_in_artifact_contract_public_prerelease" ($PublicPrerelease -match "run_fan_in_artifact_contract.ps1") "The public prerelease workflow invokes the fan-in contract verification"
  New-ContractCheck "fan_in_artifact_contract_ci_windows_not_invoked" (-not ($Ci -match "run_fan_in_artifact_contract.ps1")) "CI does not run the dual-platform fan-in on a single Windows runner (a step that cannot pass)"
  New-ContractCheck "fan_in_artifact_contract_independent_digest_anchor" (($FanInArtifactContract -match '\^\{tree\}') -and ($FanInArtifactContract -match "commit-tree-object-sha256") -and ($FanInArtifactContract -match "Get-TextSha256")) "The source digest is anchored by an independent computation of the expected commit's tree digest (sha256 of the ASCII hex bytes of 'git rev-parse <ExpectedCommit>^{tree}'), not merely by two self-reported values being equal"
  New-ContractCheck "public_prerelease_checkout_preserves_downloaded_artifacts" (($PublicPrerelease -match "actions/checkout@v4") -and ($PublicPrerelease -match "clean: false") -and ($PublicPrerelease -match "release-inputs/macos") -and ($PublicPrerelease.IndexOf("actions/checkout@v4") -gt $PublicPrerelease.IndexOf("release-inputs/macos"))) "The publish job's source checkout happens after artifact downloads and must set clean: false — the checkout default clean: true would delete release-inputs/ and break the stage step"
  New-ContractCheck "release_gate_status_aggregator_exists" (Test-Path -LiteralPath (Join-Path $PSScriptRoot "generate_release_gate_status.py")) "A cross-platform G0-G8 release gate status aggregator exists"
  New-ContractCheck "release_gate_status_aggregator_contract" $GateStatusContractOk "Dynamic contract tests prove the aggregator is fail-closed and never counts parse/unexecuted evidence as pass"
  New-ContractCheck "release_gate_status_aggregator_fail_closed_default" (($GateStatusAggregator -match "--report-only") -and ($GateStatusAggregator -match "return 1") -and ($GateStatusAggregator -match "blocked") -and ($GateStatusAggregator -match "unavailable")) "The gate status aggregator is fail-closed by default (non-zero exit on blocked/unavailable) and only --report-only relaxes the exit code for diagnostics"
  New-ContractCheck "release_gate_status_aggregator_never_counts_parse_as_pass" ((-not ($GateStatusAggregator -match "release_scripts_parse")) -and ($GateStatusAggregator -match "release-verification-") -and ($GateStatusAggregator -match "fan-in-artifact-contract-")) "G6/G8 evidence is aggregated from executed report artifacts, never from 'scripts parse', metadata-only presence, or unsigned notices"
  New-ContractCheck "release_gate_status_g6_requires_powershell_report" (($GateStatusAggregator -match "G6") -and ($GateStatusAggregator -match "release-verification") -and ($GateStatusAggregator -match "NOT executed")) "G6 PowerShell Ops gates cannot pass without an executed release-verification report; unexecuted PowerShell is recorded as unavailable/blocked"
  New-ContractCheck "release_gate_status_g8_requires_fan_in" (($GateStatusAggregator -match "G8") -and ($GateStatusAggregator -match "fan-in-artifact-contract") -and ($GateStatusAggregator -match "双平台同 commit")) "G8 cannot pass without fan-in evidence binding both platforms to the same non-empty commit and source digest"
  New-ContractCheck "fan_in_report_critical_counts_bound_to_critical_list" (($FanInArtifactContract -match "critical_total") -and ($FanInArtifactContract -match 'RequiredCritical\.Count - \$CriticalFailed\.Count')) "The fan-in report computes critical passed counts from the critical list, not from the full check total"
  New-ContractCheck "fan_in_same_commit_requires_nonempty_heads" ($FanInArtifactContract -match 'IsNullOrWhiteSpace\(\$MacHead\)') "Fan-in same-commit evidence rejects empty git_head values instead of treating two missing values as equal"
  New-ContractCheck "dacl_acl_test_windows_release_workflow" ($WindowsRelease -match "run_windows_dacl_acl_tests.ps1") "The real Windows release runner (windows-2022) executes the DACL/ACL tests; an ubuntu skip cannot be Windows MSVC evidence"
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
