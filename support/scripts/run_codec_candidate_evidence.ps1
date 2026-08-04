param(
  [string]$OutputPath = "",
  [long]$MaxP95Us = 50000
)

$ErrorActionPreference = "Stop"
$Root = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
if ([string]::IsNullOrWhiteSpace($OutputPath)) {
  $OutputPath = Join-Path $Root "code\target\release\metadata\production-codec-candidate-evidence.json"
} elseif (-not [System.IO.Path]::IsPathRooted($OutputPath)) {
  $OutputPath = Join-Path $Root $OutputPath
}
if ($MaxP95Us -lt 1) {
  throw "MaxP95Us must be a positive integer"
}

. (Join-Path $PSScriptRoot "release_candidate_provenance.ps1")
$Candidate = Get-ReleaseCandidateProvenance -Root $Root -OutputDir (Split-Path -Parent $OutputPath)
$OutputParent = Split-Path -Parent $OutputPath
New-Item -ItemType Directory -Force -Path $OutputParent | Out-Null

Push-Location (Join-Path $Root "code")
try {
  cargo run --release -p dtoken-compress --example codec_candidate_probe -- `
    --candidate-id $Candidate.candidate_id `
    --candidate-revision $Candidate.git_head `
    --output $OutputPath `
    --max-p95-us $MaxP95Us
  if ($LASTEXITCODE -ne 0) {
    throw "codec candidate probe failed with exit code $LASTEXITCODE"
  }
} finally {
  Pop-Location
}

if (-not (Test-Path -LiteralPath $OutputPath -PathType Leaf)) {
  throw "codec candidate probe did not write its evidence report"
}
$Report = Get-Content -LiteralPath $OutputPath -Raw | ConvertFrom-Json
if ($Report.scope -cne "production-codec-candidate-evidence") {
  throw "codec candidate evidence scope mismatch"
}
if ([string]$Report.candidate.candidate_id -cne [string]$Candidate.candidate_id) {
  throw "codec candidate evidence candidate id mismatch"
}
if ([string]$Report.candidate.git_head -cne [string]$Candidate.git_head) {
  throw "codec candidate evidence revision mismatch"
}
if ($Report.gate.ok -ne $true) {
  throw "codec candidate evidence gate failed"
}
if (
  $Report.privacy.metadata_only -ne $true -or
  $Report.privacy.raw_corpus_stored -ne $false -or
  $Report.privacy.raw_request_stored -ne $false -or
  $Report.privacy.raw_response_stored -ne $false -or
  $Report.privacy.secret_values_stored -ne $false
) {
  throw "codec candidate evidence privacy contract mismatch"
}

# Mirrors the Rust-owned compressor registry declaration order
# (code/crates/dtoken-compress/src/compressor_registry.rs).  The probe
# report's codec ids come from the same registry, so a drift here fails
# the release gate instead of silently shipping mismatched evidence.
$ExpectedCodecIds = @(
  "json-codec",
  "log-codec",
  "search-codec",
  "build-codec",
  "diff-codec",
  "text-codec",
  "html-codec",
  "config-codec",
  "code-compressor"
)
$Codecs = @($Report.codecs)
if ($Codecs.Count -ne $ExpectedCodecIds.Count) {
  throw "codec candidate evidence must contain every production codec"
}
for ($Index = 0; $Index -lt $ExpectedCodecIds.Count; $Index += 1) {
  $Codec = $Codecs[$Index]
  $ExpectedId = $ExpectedCodecIds[$Index]
  if ([string]$Codec.id -cne $ExpectedId) {
    throw "codec candidate evidence order/id mismatch at index $Index"
  }
  if (
    [string]$Codec.state -cne "active" -or
    [int64]$Codec.property_cases -lt 128 -or
    [int64]$Codec.semantic_cases -lt 128 -or
    [int64]$Codec.negative_cases -lt 4 -or
    [int64]$Codec.budget_or_cancel_cases -lt 1 -or
    [int64]$Codec.rollback_cases -lt 128 -or
    [int64]$Codec.failures -ne 0 -or
    [int64]$Codec.latency_p95_us -gt $MaxP95Us
  ) {
    throw "codec candidate evidence failed for $ExpectedId"
  }
}

[pscustomobject]@{
  ok = $true
  candidate_id = $Candidate.candidate_id
  candidate_revision = $Candidate.git_head
  evidence_revision = [string]$Report.evidence_revision
  codec_count = $Codecs.Count
  report_path = $OutputPath
  privacy = [pscustomobject]@{
    metadata_only = $true
    raw_corpus_stored = $false
    raw_request_stored = $false
    raw_response_stored = $false
    secret_values_stored = $false
  }
} | ConvertTo-Json -Depth 5 -Compress
