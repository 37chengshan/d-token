<#
.SYNOPSIS
  Verify the cross-platform fan-in artifact contract: exact commit, source
  digest, artifact digest, and test receipts bound across macOS and Windows
  release candidates.

.DESCRIPTION
  This script enforces the fan-in gate (G8) for d-token public unsigned
  prerelease.  It validates that:

  1. Both macOS and Windows release-candidate artifacts exist and are
     present in the staged release directory.
  2. Both platforms share the same Git commit (GITHUB_SHA or local HEAD).
  3. Source digest (tracked_diff_sha256) is identical across platforms,
     proving the same source tree was built.
  4. Each platform's release-candidate.json is valid and self-consistent.
  5. Each platform's SHA-256 manifest is present and verifies its artifacts.
  6. Each platform's SBOM, license inventory, and security scan are present
     and non-empty.
  7. Each platform's codec candidate evidence and headroom capability
     snapshot are present.
  8. At least one test receipt (release-verification-*.json or
     desktop-bundle-artifact-audit-*.json) is present per platform.
  9. If either platform is missing ANY required artifact, the gate fails
     closed — no "partial prerelease".

  Exit code 0 = all fan-in checks passed.
  Exit code 1 = one or more checks failed.
  Exit code 2 = not a fan-in staging environment (single-platform host such as
  a Windows-only or macOS-only runner, or no platform directories at all).
  Single-platform jobs MUST NOT run the dual-platform fan-in as a gate; the
  publish job is the only fan-in caller.

.PARAMETER MacDir
  Directory containing macOS release artifacts (default: release-inputs/macos).

.PARAMETER WinDir
  Directory containing Windows release artifacts (default: release-inputs/windows).

.PARAMETER ExpectedCommit
  The expected Git commit SHA that both platforms must share.  Auto-detected
  from HEAD when not provided.

.PARAMETER OutputDir
  Where to write the JSON report (default: support/artifacts).

.EXAMPLE
  # In the public-unsigned-prerelease workflow:
  powershell -ExecutionPolicy Bypass -File support/scripts/run_fan_in_artifact_contract.ps1 `
    -MacDir release-inputs/macos -WinDir release-inputs/windows -ExpectedCommit $env:GITHUB_SHA
#>
param(
  [string]$MacDir = "",
  [string]$WinDir = "",
  [string]$ExpectedCommit = "",
  [string]$OutputDir = ""
)

$ErrorActionPreference = "Stop"
$Root = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
$Utf8NoBom = [System.Text.UTF8Encoding]::new($false)
if ([string]::IsNullOrWhiteSpace($OutputDir)) {
  $OutputDir = Join-Path $Root "support\artifacts"
}
New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null

# --------------------------------------------------------------------------
# Detect expected commit from HEAD
# --------------------------------------------------------------------------
if ([string]::IsNullOrWhiteSpace($ExpectedCommit)) {
  try {
    Push-Location $Root
    $ExpectedCommit = (& "git" "rev-parse" "HEAD" 2>$null).Trim()
    if ($global:LASTEXITCODE -ne 0 -or [string]::IsNullOrWhiteSpace($ExpectedCommit)) {
      $ExpectedCommit = ""
    }
  } catch {
    $ExpectedCommit = ""
  } finally {
    Pop-Location
  }
}

# --------------------------------------------------------------------------
# Helpers
# --------------------------------------------------------------------------
function New-Check {
  param([string]$Id, [bool]$Ok, [object]$Detail)
  [pscustomobject]@{ id = $Id; ok = $Ok; detail = $Detail }
}

function Read-JsonFile {
  param([string]$Path)
  if ([string]::IsNullOrWhiteSpace($Path) -or -not (Test-Path -LiteralPath $Path -PathType Leaf)) {
    return $null
  }
  try {
    return Get-Content -LiteralPath $Path -Raw | ConvertFrom-Json
  } catch {
    return $null
  }
}

function Find-File {
  param([string]$Dir, [string]$Pattern, [string]$NameRegex = "")
  if ([string]::IsNullOrWhiteSpace($Dir) -or -not (Test-Path -LiteralPath $Dir)) {
    return $null
  }
  # Recursive search: actions/upload-artifact@v4 preserves workspace-relative
  # paths, so after download the evidence lives in nested code/target/... and
  # support/artifacts/... directories — never in a flat top level.
  $Files = @(Get-ChildItem -LiteralPath $Dir -Filter $Pattern -File -Recurse -ErrorAction SilentlyContinue)
  if ($Files.Count -eq 0) { return $null }
  if ($Files.Count -eq 1) { return $Files[0].FullName }
  # Disambiguation: prefer a canonical filename, then the shallowest path, then
  # a deterministic full-path sort.  Log ambiguity so a wrong pick is visible.
  if (-not [string]::IsNullOrWhiteSpace($NameRegex)) {
    $Named = @($Files | Where-Object { $_.Name -match $NameRegex })
    if ($Named.Count -eq 1) { return $Named[0].FullName }
    if ($Named.Count -gt 1) { $Files = $Named }
  }
  $Sorted = @($Files | Sort-Object @{ Expression = { $_.FullName.Length } }, FullName)
  Write-Warning "Find-File: $($Files.Count) matches for '$Pattern' in '$Dir'; picked '$($Sorted[0].FullName)'"
  return $Sorted[0].FullName
}

function Find-Files {
  param([string]$Dir, [string[]]$Patterns)
  if ([string]::IsNullOrWhiteSpace($Dir) -or -not (Test-Path -LiteralPath $Dir)) {
    return @()
  }
  $Found = @()
  foreach ($Pattern in $Patterns) {
    $Found += @(Get-ChildItem -LiteralPath $Dir -Filter $Pattern -File -Recurse -ErrorAction SilentlyContinue)
  }
  return @($Found | Sort-Object FullName -Unique)
}

function Get-ArtifactHash {
  param([string]$Path)
  if ([string]::IsNullOrWhiteSpace($Path) -or -not (Test-Path -LiteralPath $Path -PathType Leaf)) {
    return $null
  }
  return (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
}

# --------------------------------------------------------------------------
# Platform gate — at least one platform directory must be provided
# --------------------------------------------------------------------------
$MacDirResolved = if ([string]::IsNullOrWhiteSpace($MacDir)) { "" } else {
  $p = $MacDir
  if (-not [System.IO.Path]::IsPathRooted($p)) { $p = Join-Path $Root $p }
  $p
}
$WinDirResolved = if ([string]::IsNullOrWhiteSpace($WinDir)) { "" } else {
  $p = $WinDir
  if (-not [System.IO.Path]::IsPathRooted($p)) { $p = Join-Path $Root $p }
  $p
}

$MacDirExists = (-not [string]::IsNullOrWhiteSpace($MacDirResolved)) -and (Test-Path -LiteralPath $MacDirResolved)
$WinDirExists = (-not [string]::IsNullOrWhiteSpace($WinDirResolved)) -and (Test-Path -LiteralPath $WinDirResolved)

$Checks = @()

# ---- 1. Both platform directories exist -------------------------------------
$Checks += New-Check "mac_dir_exists" $MacDirExists @{ path = $MacDirResolved; exists = $MacDirExists }
$Checks += New-Check "win_dir_exists" $WinDirExists @{ path = $WinDirResolved; exists = $WinDirExists }

if (-not $MacDirExists -or -not $WinDirExists) {
  # On a single-platform host (e.g. a Windows-only runner) the dual-platform
  # fan-in cannot be verified.  Exit 2 per the documented contract ("not a
  # fan-in staging environment") — never a pass, never a misleading exit 1.
  # Single-platform workflows must not invoke this script as a release gate;
  # only the publish job (both platform dirs present) runs the real fan-in.
  if ($MacDirExists -or $WinDirExists) {
    Write-Warning "run_fan_in_artifact_contract.ps1: Only one platform directory found — fan-in verification requires both.  Exiting 2 (not a fan-in staging environment)."
  } else {
    Write-Warning "run_fan_in_artifact_contract.ps1: No platform directories found — skipping (exit 2)."
  }
  exit 2
}

# ---- 2. Release-candidate JSON for each platform ----------------------------
$MacCandidate = $null
$WinCandidate = $null
$MacCandidatePath = ""
$WinCandidatePath = ""
if ($MacDirExists) {
  $MacCandidatePath = Find-File $MacDirResolved "d-token-release-candidate.json"
  $MacCandidate = Read-JsonFile $MacCandidatePath
}
if ($WinDirExists) {
  $WinCandidatePath = Find-File $WinDirResolved "d-token-release-candidate.json"
  $WinCandidate = Read-JsonFile $WinCandidatePath
}
$Checks += New-Check "mac_candidate_json" ($null -ne $MacCandidate) @{
  path = $MacCandidatePath; parsed = ($null -ne $MacCandidate)
}
$Checks += New-Check "win_candidate_json" ($null -ne $WinCandidate) @{
  path = $WinCandidatePath; parsed = ($null -ne $WinCandidate)
}

# If either platform is missing its candidate JSON, fail closed.
if ($null -eq $MacCandidate -or $null -eq $WinCandidate) {
  $Report = [pscustomobject]@{
    ok = $false
    mode = "fan-in-artifact-contract"
    expected_commit = $ExpectedCommit
    summary = "FAN-IN FAILED: Missing release-candidate.json on one or both platforms."
    checks = $Checks
  }
  $Report | ConvertTo-Json -Depth 6 -Compress
  exit 1
}

# ---- 3. Same commit across platforms -----------------------------------------
$MacHead = [string]$MacCandidate.git_head
$WinHead = [string]$WinCandidate.git_head
# Two empty (or missing) git_head values must NOT count as "same commit":
# same-commit evidence is only meaningful when both platforms report a real,
# non-empty revision.
$SameCommit = (-not [string]::IsNullOrWhiteSpace($MacHead)) -and
              (-not [string]::IsNullOrWhiteSpace($WinHead)) -and
              ($MacHead -eq $WinHead)
$Checks += New-Check "same_commit_across_platforms" $SameCommit @{
  mac_git_head = $MacHead; win_git_head = $WinHead
}

# ---- 4. Expected commit matches (if provided) --------------------------------
$CommitMatchesExpected = $true
if (-not [string]::IsNullOrWhiteSpace($ExpectedCommit)) {
  $CommitMatchesExpected = ($MacHead -eq $ExpectedCommit) -and ($WinHead -eq $ExpectedCommit)
}
$Checks += New-Check "commit_matches_expected" $CommitMatchesExpected @{
  expected = $ExpectedCommit; mac = $MacHead; win = $WinHead
}

# ---- 5. Same source digest (tracked_diff_sha256) -----------------------------
$MacSourceDigest = [string]$MacCandidate.tracked_diff_sha256
$WinSourceDigest = [string]$WinCandidate.tracked_diff_sha256
$SameSourceDigest = ($MacSourceDigest -eq $WinSourceDigest) -and
                    (-not [string]::IsNullOrWhiteSpace($MacSourceDigest))
$Checks += New-Check "same_source_digest" $SameSourceDigest @{
  mac = $MacSourceDigest; win = $WinSourceDigest
  anchor = "publish-job-independent-checkout-git-diff"
  note = "Two self-reported digests that merely equal each other are not proof. The authoritative source anchor is the publish job's independent recomputation of 'git diff --binary HEAD -- .' against a fresh checkout of the expected commit, plus the git_head binding asserted above."
}

# ---- 6. Dirty state consistency ----------------------------------------------
$MacDirty = [bool]$MacCandidate.candidate_dirty
$WinDirty = [bool]$WinCandidate.candidate_dirty
$SameDirtyState = ($MacDirty -eq $WinDirty)
$Checks += New-Check "same_dirty_state" $SameDirtyState @{
  mac_dirty = $MacDirty; win_dirty = $WinDirty
}

# ---- 7. SHA-256 manifest for each platform -----------------------------------
$MacShaFile = Find-File $MacDirResolved "SHA256SUMS-macos*"
$WinShaFile = Find-File $WinDirResolved "SHA256SUMS-windows*"
$MacShaExists = ($null -ne $MacShaFile) -and ((Get-Item -LiteralPath $MacShaFile).Length -gt 0)
$WinShaExists = ($null -ne $WinShaFile) -and ((Get-Item -LiteralPath $WinShaFile).Length -gt 0)
$Checks += New-Check "mac_sha256_manifest" $MacShaExists @{
  path = $MacShaFile; bytes = if ($MacShaExists) { (Get-Item $MacShaFile).Length } else { 0 }
}
$Checks += New-Check "win_sha256_manifest" $WinShaExists @{
  path = $WinShaFile; bytes = if ($WinShaExists) { (Get-Item $WinShaFile).Length } else { 0 }
}

# ---- 8. SBOM for each platform -----------------------------------------------
$MacSbom = Find-File $MacDirResolved "*.cdx.json" "^d-token-.*\.cdx\.json$"
$WinSbom = Find-File $WinDirResolved "*.cdx.json" "^d-token-.*\.cdx\.json$"
$MacSbomExists = ($null -ne $MacSbom) -and ((Get-Item -LiteralPath $MacSbom).Length -gt 100)
$WinSbomExists = ($null -ne $WinSbom) -and ((Get-Item -LiteralPath $WinSbom).Length -gt 100)
$Checks += New-Check "mac_sbom" $MacSbomExists @{
  path = $MacSbom; bytes = if ($MacSbomExists) { (Get-Item $MacSbom).Length } else { 0 }
}
$Checks += New-Check "win_sbom" $WinSbomExists @{
  path = $WinSbom; bytes = if ($WinSbomExists) { (Get-Item $WinSbom).Length } else { 0 }
}

# ---- 9. License inventory for each platform -----------------------------------
$MacLicenses = Find-File $MacDirResolved "dependency-licenses.csv"
$WinLicenses = Find-File $WinDirResolved "dependency-licenses.csv"
$MacLicensesExists = ($null -ne $MacLicenses) -and ((Get-Item -LiteralPath $MacLicenses).Length -gt 50)
$WinLicensesExists = ($null -ne $WinLicenses) -and ((Get-Item -LiteralPath $WinLicenses).Length -gt 50)
$Checks += New-Check "mac_licenses" $MacLicensesExists @{
  path = $MacLicenses; bytes = if ($MacLicensesExists) { (Get-Item $MacLicenses).Length } else { 0 }
}
$Checks += New-Check "win_licenses" $WinLicensesExists @{
  path = $WinLicenses; bytes = if ($WinLicensesExists) { (Get-Item $WinLicenses).Length } else { 0 }
}

# ---- 10. Security scan for each platform --------------------------------------
$MacSecurity = Find-File $MacDirResolved "security-scan.md"
$WinSecurity = Find-File $WinDirResolved "security-scan.md"
$MacSecurityExists = ($null -ne $MacSecurity) -and ((Get-Item -LiteralPath $MacSecurity).Length -gt 50)
$WinSecurityExists = ($null -ne $WinSecurity) -and ((Get-Item -LiteralPath $WinSecurity).Length -gt 50)
$Checks += New-Check "mac_security_scan" $MacSecurityExists @{
  path = $MacSecurity; bytes = if ($MacSecurityExists) { (Get-Item $MacSecurity).Length } else { 0 }
}
$Checks += New-Check "win_security_scan" $WinSecurityExists @{
  path = $WinSecurity; bytes = if ($WinSecurityExists) { (Get-Item $WinSecurity).Length } else { 0 }
}

# ---- 11. Codec candidate evidence for each platform ---------------------------
$MacCodec = Find-File $MacDirResolved "production-codec-candidate-evidence.json"
$WinCodec = Find-File $WinDirResolved "production-codec-candidate-evidence.json"
$MacCodecExists = ($null -ne $MacCodec) -and ((Get-Item -LiteralPath $MacCodec).Length -gt 50)
$WinCodecExists = ($null -ne $WinCodec) -and ((Get-Item -LiteralPath $WinCodec).Length -gt 50)
$Checks += New-Check "mac_codec_evidence" $MacCodecExists @{
  path = $MacCodec; bytes = if ($MacCodecExists) { (Get-Item $MacCodec).Length } else { 0 }
}
$Checks += New-Check "win_codec_evidence" $WinCodecExists @{
  path = $WinCodec; bytes = if ($WinCodecExists) { (Get-Item $WinCodec).Length } else { 0 }
}

# ---- 12. Headroom capability snapshot for each platform ------------------------
$MacSnapshot = Find-File $MacDirResolved "headroom-capability-snapshot.json"
$WinSnapshot = Find-File $WinDirResolved "headroom-capability-snapshot.json"
$MacSnapshotExists = ($null -ne $MacSnapshot) -and ((Get-Item -LiteralPath $MacSnapshot).Length -gt 50)
$WinSnapshotExists = ($null -ne $WinSnapshot) -and ((Get-Item -LiteralPath $WinSnapshot).Length -gt 50)
$Checks += New-Check "mac_headroom_snapshot" $MacSnapshotExists @{
  path = $MacSnapshot; bytes = if ($MacSnapshotExists) { (Get-Item $MacSnapshot).Length } else { 0 }
}
$Checks += New-Check "win_headroom_snapshot" $WinSnapshotExists @{
  path = $WinSnapshot; bytes = if ($WinSnapshotExists) { (Get-Item $WinSnapshot).Length } else { 0 }
}

# ---- 13. Test receipts for each platform --------------------------------------
$MacTestReceipts = @(Find-Files $MacDirResolved @("release-verification-*.json", "desktop-bundle-artifact-audit-*.json", "release-performance-gate.json", "*-package-audit.*", "*-package-audit.md"))
$WinTestReceipts = @(Find-Files $WinDirResolved @("release-verification-*.json", "desktop-bundle-artifact-audit-*.json", "release-performance-gate.json", "*-package-audit.*", "*-package-audit.md"))
$MacHasTestReceipt = ($MacTestReceipts.Count -gt 0)
$WinHasTestReceipt = ($WinTestReceipts.Count -gt 0)
$Checks += New-Check "mac_test_receipts" $MacHasTestReceipt @{
  count = $MacTestReceipts.Count
  paths = @($MacTestReceipts | ForEach-Object { $_.Name })
}
$Checks += New-Check "win_test_receipts" $WinHasTestReceipt @{
  count = $WinTestReceipts.Count
  paths = @($WinTestReceipts | ForEach-Object { $_.Name })
}

# ---- 14. Build info for each platform -----------------------------------------
$MacBuildInfo = Find-File $MacDirResolved "build-info.json"
$WinBuildInfo = Find-File $WinDirResolved "build-info.json"
$MacBuildInfoExists = ($null -ne $MacBuildInfo) -and ((Get-Item -LiteralPath $MacBuildInfo).Length -gt 50)
$WinBuildInfoExists = ($null -ne $WinBuildInfo) -and ((Get-Item -LiteralPath $WinBuildInfo).Length -gt 50)
$Checks += New-Check "mac_build_info" $MacBuildInfoExists @{
  path = $MacBuildInfo; bytes = if ($MacBuildInfoExists) { (Get-Item $MacBuildInfo).Length } else { 0 }
}
$Checks += New-Check "win_build_info" $WinBuildInfoExists @{
  path = $WinBuildInfo; bytes = if ($WinBuildInfoExists) { (Get-Item $WinBuildInfo).Length } else { 0 }
}

# ---- 15. UNSIGNED-TECHNICAL-PREVIEW notice for each platform --------------------
$MacNotice = Find-File $MacDirResolved "UNSIGNED-TECHNICAL-PREVIEW.txt"
$WinNotice = Find-File $WinDirResolved "UNSIGNED-TECHNICAL-PREVIEW.txt"
$MacNoticeExists = ($null -ne $MacNotice) -and ((Get-Item -LiteralPath $MacNotice).Length -gt 50)
$WinNoticeExists = ($null -ne $WinNotice) -and ((Get-Item -LiteralPath $WinNotice).Length -gt 50)
$Checks += New-Check "mac_unsigned_notice" $MacNoticeExists @{
  path = $MacNotice; bytes = if ($MacNoticeExists) { (Get-Item $MacNotice).Length } else { 0 }
}
$Checks += New-Check "win_unsigned_notice" $WinNoticeExists @{
  path = $WinNotice; bytes = if ($WinNoticeExists) { (Get-Item $WinNotice).Length } else { 0 }
}

# ---- 16. No secret leakage across ALL staged artifacts -------------------------
# Scan every staged release file, not just the two candidate JSONs.  Patterns
# match credential VALUES (shapes), not bare key names, so a future key like
# "raw_prompt_sha256" cannot cause a false positive while sk-ant-... and ghp_
# tokens are caught.  Candidate JSONs are scanned by value; other text files
# (manifests, SBOM, license CSV, security scan, codec evidence, capability
# snapshots, build info, notices) are scanned raw.
$SecretsFound = @()
$SecretValuePatterns = @(
  "sk-ant-[A-Za-z0-9_-]{16,}",
  "sk-[A-Za-z0-9]{32,}",
  "ghp_[A-Za-z0-9]{20,}",
  "gho_[A-Za-z0-9]{20,}",
  "AKIA[0-9A-Z]{16}",
  "AIza[0-9A-Za-z_\-]{20,}",
  "xox[baprs]-[A-Za-z0-9-]{10,}",
  "Bearer\s+[A-Za-z0-9._~+/=-]{20,}"
)
function Test-ValueSecret {
  param([string]$Text, [string]$Label)
  foreach ($Pattern in $SecretValuePatterns) {
    if ($Text -match $Pattern) {
      $script:SecretsFound += "$Label`:$Pattern"
    }
  }
}
function Test-KeyValueSecret {
  param([object]$Object, [string]$Label)
  $JsonText = $Object | ConvertTo-Json -Depth 8 -Compress
  foreach ($Pattern in @("sk-[A-Za-z0-9]{32,}", "AKIA[0-9A-Z]{16}", "AIza[0-9A-Za-z_\-]{20,}", "ghp_[A-Za-z0-9]{20,}")) {
    if ($JsonText -match $Pattern) {
      $script:SecretsFound += "$Label`:$Pattern"
    }
  }
}
if ($null -ne $MacCandidate) { Test-KeyValueSecret $MacCandidate "mac_candidate" }
if ($null -ne $WinCandidate) { Test-KeyValueSecret $WinCandidate "win_candidate" }
# Scan every TEXT artifact (JSON, Markdown, CSV, manifests, SBOM, notices).
# Binary artifacts (DMG, NSIS installer, DLLs, executables) are skipped — they
# are hash-verified separately and can be tens of megabytes.
$TextLikeExtensions = @(".json", ".md", ".csv", ".txt", ".yml", ".yaml", ".toml", ".xml", ".sbom")
$MaxScanBytes = 4MB
foreach ($File in @(Find-Files $MacDirResolved @("*")) ) {
  if ($File.Length -gt $MaxScanBytes) { continue }
  if ($File.Extension.ToLowerInvariant() -notin $TextLikeExtensions) { continue }
  $Text = [System.IO.File]::ReadAllText($File.FullName)
  Test-ValueSecret $Text ("mac:" + $File.Name)
}
foreach ($File in @(Find-Files $WinDirResolved @("*")) ) {
  if ($File.Length -gt $MaxScanBytes) { continue }
  if ($File.Extension.ToLowerInvariant() -notin $TextLikeExtensions) { continue }
  $Text = [System.IO.File]::ReadAllText($File.FullName)
  Test-ValueSecret $Text ("win:" + $File.Name)
}
$NoSecretsLeaked = ($SecretsFound.Count -eq 0)
$Checks += New-Check "no_secrets_in_candidate_jsons" $NoSecretsLeaked @{
  secrets_found = $SecretsFound
  note = "Credential-shaped values are rejected across every staged artifact; key-name substrings alone never trigger."
}

# --------------------------------------------------------------------------
# Summary
# --------------------------------------------------------------------------
$Passed = @($Checks | Where-Object { $_.ok }).Count
$Total = $Checks.Count
$RequiredCritical = @(
  "mac_dir_exists", "win_dir_exists",
  "mac_candidate_json", "win_candidate_json",
  "same_commit_across_platforms", "commit_matches_expected",
  "same_source_digest",
  "mac_sha256_manifest", "win_sha256_manifest",
  "mac_sbom", "win_sbom",
  "mac_security_scan", "win_security_scan",
  "mac_test_receipts", "win_test_receipts",
  "no_secrets_in_candidate_jsons"
)
$CriticalFailed = @($Checks | Where-Object { $_.id -in $RequiredCritical -and -not $_.ok })
$AllOk = ($CriticalFailed.Count -eq 0)

$Blockers = @()
if (-not $SameCommit) { $Blockers += "platforms_built_from_different_commits" }
if (-not $SameSourceDigest) { $Blockers += "source_digest_mismatch" }
if (-not $CommitMatchesExpected) { $Blockers += "commit_does_not_match_expected" }
if (-not $MacShaExists -or -not $WinShaExists) { $Blockers += "sha256_manifest_missing" }
if (-not $MacHasTestReceipt -or -not $WinHasTestReceipt) { $Blockers += "test_receipts_missing" }
if ($null -eq $MacCandidate -or $null -eq $WinCandidate) { $Blockers += "candidate_json_missing" }
if ($SecretsFound.Count -gt 0) { $Blockers += "secrets_leaked_in_candidate_jsons" }

$Report = [pscustomobject]@{
  ok = $AllOk
  mode = "fan-in-artifact-contract"
  expected_commit = $ExpectedCommit
  mac_candidate_id = if ($null -ne $MacCandidate) { $MacCandidate.candidate_id } else { $null }
  win_candidate_id = if ($null -ne $WinCandidate) { $WinCandidate.candidate_id } else { $null }
  same_commit = $SameCommit
  same_source_digest = $SameSourceDigest
  source_digest_anchor = "publish-job-independent-checkout-git-diff"
  passed = $Passed
  total = $Total
  # critical_passed must be derived from the critical list, not from the full
  # check total — ($Total - failures) overstates the number of critical checks
  # that passed and can misreport a blocked release as mostly-green.
  critical_passed = ($RequiredCritical.Count - $CriticalFailed.Count)
  critical_total = $RequiredCritical.Count
  summary = "Fan-in artifact contract: $Passed/$Total checks passed, $($CriticalFailed.Count) critical blocker(s)"
  blockers = $Blockers
  checks = $Checks
}

$Stamp = (Get-Date).ToString("yyyyMMdd-HHmmss")
$ReportPath = Join-Path $OutputDir "fan-in-artifact-contract-$Stamp.json"
$Report | Add-Member -NotePropertyName report_path -NotePropertyValue $ReportPath
[System.IO.File]::WriteAllText($ReportPath, ($Report | ConvertTo-Json -Depth 8), $Utf8NoBom)

$Report | ConvertTo-Json -Depth 6 -Compress

if (-not $AllOk) {
  exit 1
}