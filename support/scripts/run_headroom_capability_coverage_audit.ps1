param(
  [string]$CapabilitySnapshot = "",
  [string]$ExpectedCandidateId = "",
  [string]$ReleaseReport = "",
  [string]$OutputDir = ""
)

$ErrorActionPreference = "Stop"
$Root = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
$Utf8NoBom = [System.Text.UTF8Encoding]::new($false)
$ExpectedSnapshotSource = "dtoken-owned-runtime"
$MaxSnapshotBytes = 1MB
$AllowedStates = @("ready", "degraded", "unavailable", "blocked", "implemented_disabled")
$AllowedFreshness = @("live", "recent", "stale", "unknown")
$OpticalPolicyReasonCodes = @("policy_shadow_only", "policy_disabled")

if ([string]::IsNullOrWhiteSpace($OutputDir)) {
  $OutputDir = Join-Path $Root "support\artifacts"
} elseif (-not [System.IO.Path]::IsPathRooted($OutputDir)) {
  $OutputDir = Join-Path $Root $OutputDir
}
New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null

$RequiredCapabilities = @(
  [pscustomobject]@{ id = "compression"; label = "Context compression pipeline" }
  [pscustomobject]@{ id = "ccr"; label = "CCR metadata and retrieval boundary" }
  [pscustomobject]@{ id = "memory_learn"; label = "Memory / learn metadata" }
  [pscustomobject]@{ id = "output_shape"; label = "Output shaping / verbosity" }
  [pscustomobject]@{ id = "integrated_rtk"; label = "Integrated RTK status and savings bridge" }
  [pscustomobject]@{ id = "proxy_sdk_mcp"; label = "Proxy / SDK / MCP dashboard primitives" }
  [pscustomobject]@{ id = "stats_history_metrics"; label = "Stats, history, metrics, latency and route overhead" }
  [pscustomobject]@{ id = "routing_protocols"; label = "Route adapter and supported protocol bridges" }
  [pscustomobject]@{ id = "doctor_dashboard_logs_export"; label = "Doctor, dashboard, logs, Event Ledger and export" }
  [pscustomobject]@{ id = "project_policy"; label = "Project compression policy, scan, and managed config write" }
  [pscustomobject]@{ id = "backup_rollback"; label = "Config diff, backup, verify, and rollback" }
)
$OpticalCapabilityId = "optical_active_transform"
$script:ValidationErrors = New-Object System.Collections.Generic.List[string]

function Add-ValidationError {
  param([string]$Message)
  $script:ValidationErrors.Add($Message) | Out-Null
}

function Resolve-InputPath {
  param(
    [string]$Path,
    [string]$RelativeTo
  )

  if ([string]::IsNullOrWhiteSpace($Path)) { return "" }
  if ([System.IO.Path]::IsPathRooted($Path)) {
    return [System.IO.Path]::GetFullPath($Path)
  }
  return [System.IO.Path]::GetFullPath((Join-Path $RelativeTo $Path))
}

function Read-JsonDocument {
  param(
    [string]$Path,
    [string]$Label,
    [long]$MaximumBytes
  )

  if ([string]::IsNullOrWhiteSpace($Path)) {
    Add-ValidationError "$Label path is required"
    return $null
  }
  if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
    Add-ValidationError "$Label does not exist: $Path"
    return $null
  }

  $Item = Get-Item -LiteralPath $Path
  if ($MaximumBytes -gt 0 -and $Item.Length -gt $MaximumBytes) {
    Add-ValidationError "$Label exceeds the $MaximumBytes byte metadata-only limit"
    return $null
  }

  try {
    return Get-Content -LiteralPath $Path -Raw | ConvertFrom-Json
  } catch {
    Add-ValidationError "$Label is not valid JSON"
    return $null
  }
}

function Test-JsonProperty {
  param(
    [object]$Object,
    [string]$Name
  )
  return ($null -ne $Object -and $null -ne $Object.PSObject.Properties[$Name])
}

function Test-Revision {
  param([string]$Revision)
  return (-not [string]::IsNullOrWhiteSpace($Revision) -and $Revision -cmatch '^[0-9a-f]{64}$')
}

function Test-ReasonCodes {
  param(
    [object]$Capability,
    [System.Collections.Generic.List[string]]$Errors
  )

  if (-not (Test-JsonProperty $Capability "reason_codes")) {
    $Errors.Add("reason_codes is missing") | Out-Null
    return @()
  }
  if ($Capability.reason_codes -isnot [System.Array]) {
    $Errors.Add("reason_codes must be a JSON array") | Out-Null
    return @()
  }

  $Codes = @()
  $SeenCodes = @{}
  foreach ($CodeValue in @($Capability.reason_codes)) {
    $Code = [string]$CodeValue
    if ($Code -notmatch '^[a-z0-9][a-z0-9._-]{0,63}$') {
      $Errors.Add("reason_codes contains an invalid metadata code") | Out-Null
      continue
    }
    if ($SeenCodes.ContainsKey($Code)) {
      $Errors.Add("reason_codes contains a duplicate metadata code") | Out-Null
      continue
    }
    $SeenCodes[$Code] = $true
    $Codes += $Code
  }
  return $Codes
}

function Get-CapabilityEvaluation {
  param(
    [object]$Capability,
    [string]$Id,
    [string]$Label,
    [string]$ExpectedRevision,
    [bool]$ActiveRequired
  )

  $Errors = New-Object System.Collections.Generic.List[string]
  if ($null -eq $Capability) {
    $Errors.Add("required capability is missing") | Out-Null
    return [pscustomobject]@{
      id = $Id
      label = $Label
      active_required = $ActiveRequired
      present = $false
      state = $null
      implementation_revision = $null
      source_evidence_revision = $null
      evidence_revision = $null
      freshness = $null
      reason_codes = @()
      ok = $false
      errors = @($Errors)
    }
  }

  $State = if (Test-JsonProperty $Capability "state") { [string]$Capability.state } else { "" }
  $ImplementationRevision = if (Test-JsonProperty $Capability "implementation_revision") { [string]$Capability.implementation_revision } else { "" }
  $HasSourceEvidenceRevision = Test-JsonProperty $Capability "source_evidence_revision"
  $SourceEvidenceRevision = if ($HasSourceEvidenceRevision -and $null -ne $Capability.source_evidence_revision) { [string]$Capability.source_evidence_revision } else { "" }
  $EvidenceRevision = if (Test-JsonProperty $Capability "evidence_revision") { [string]$Capability.evidence_revision } else { "" }
  $Freshness = if (Test-JsonProperty $Capability "freshness") { [string]$Capability.freshness } else { "" }
  $ReasonCodes = @(Test-ReasonCodes -Capability $Capability -Errors $Errors)

  if ($AllowedStates -cnotcontains $State) {
    $Errors.Add("state is missing or unsupported") | Out-Null
  }
  if ($ImplementationRevision -cnotmatch '^[A-Za-z0-9][A-Za-z0-9._:/+-]{0,127}$') {
    $Errors.Add("implementation_revision is missing or invalid") | Out-Null
  }
  if (-not $HasSourceEvidenceRevision) {
    $Errors.Add("source_evidence_revision is missing") | Out-Null
  } elseif (
    -not [string]::IsNullOrWhiteSpace($SourceEvidenceRevision) -and
    $SourceEvidenceRevision -cnotmatch '^[A-Za-z0-9][A-Za-z0-9._:/+-]{0,127}$'
  ) {
    $Errors.Add("source_evidence_revision is invalid") | Out-Null
  }
  if (@("ready", "degraded", "implemented_disabled") -ccontains $State -and [string]::IsNullOrWhiteSpace($SourceEvidenceRevision)) {
    $Errors.Add("verified state requires source_evidence_revision") | Out-Null
  }
  if (-not (Test-Revision $EvidenceRevision)) {
    $Errors.Add("evidence_revision must be a 64-character candidate revision") | Out-Null
  } elseif ($EvidenceRevision -cne $ExpectedRevision) {
    $Errors.Add("evidence_revision does not match the candidate revision") | Out-Null
  }
  if ($AllowedFreshness -cnotcontains $Freshness) {
    $Errors.Add("freshness is missing or unsupported") | Out-Null
  }
  if ($ReasonCodes.Count -eq 0) {
    $Errors.Add("capability requires at least one reason code") | Out-Null
  }

  if ($ActiveRequired) {
    if (@("ready", "degraded") -cnotcontains $State) {
      $Errors.Add("active-required capability must be ready or degraded") | Out-Null
    }
    if ($Freshness -ceq "unknown") {
      $Errors.Add("active-required capability freshness must not be unknown") | Out-Null
    }
  }

  return [pscustomobject]@{
    id = $Id
    label = $Label
    active_required = $ActiveRequired
    present = $true
    state = if ([string]::IsNullOrWhiteSpace($State)) { $null } else { $State }
    implementation_revision = if ([string]::IsNullOrWhiteSpace($ImplementationRevision)) { $null } else { $ImplementationRevision }
    source_evidence_revision = if ([string]::IsNullOrWhiteSpace($SourceEvidenceRevision)) { $null } else { $SourceEvidenceRevision }
    evidence_revision = if ([string]::IsNullOrWhiteSpace($EvidenceRevision)) { $null } else { $EvidenceRevision }
    freshness = if ([string]::IsNullOrWhiteSpace($Freshness)) { $null } else { $Freshness }
    reason_codes = $ReasonCodes
    ok = ($Errors.Count -eq 0)
    errors = @($Errors)
  }
}

$Release = $null
$ReleaseReportPath = ""
$ExpectedRevision = ""
$CandidateBindingSource = ""

if (-not [string]::IsNullOrWhiteSpace($ExpectedCandidateId)) {
  if (-not (Test-Revision $ExpectedCandidateId)) {
    Add-ValidationError "-ExpectedCandidateId must be exactly 64 lowercase hexadecimal characters"
  } else {
    $ExpectedRevision = $ExpectedCandidateId
    $CandidateBindingSource = "explicit-candidate-id"
  }
}

if (-not [string]::IsNullOrWhiteSpace($ReleaseReport)) {
  $ReleaseReportPath = Resolve-InputPath -Path $ReleaseReport -RelativeTo $Root
  $Release = Read-JsonDocument -Path $ReleaseReportPath -Label "release report" -MaximumBytes (4MB)
  if ($null -ne $Release) {
    if (-not (Test-JsonProperty $Release "candidate") -or $null -eq $Release.candidate) {
      Add-ValidationError "release report candidate is missing"
    } elseif (-not (Test-JsonProperty $Release.candidate "candidate_id") -or -not (Test-Revision ([string]$Release.candidate.candidate_id))) {
      Add-ValidationError "release report candidate.candidate_id is missing or invalid"
    } else {
      $ReleaseRevision = [string]$Release.candidate.candidate_id
      if (-not [string]::IsNullOrWhiteSpace($ExpectedRevision) -and $ExpectedRevision -cne $ReleaseRevision) {
        Add-ValidationError "-ExpectedCandidateId does not match release report candidate.candidate_id"
      } else {
        $ExpectedRevision = $ReleaseRevision
        $CandidateBindingSource = "release-report"
      }
    }
  }
}

$CapabilitySnapshotPath = ""
if (-not [string]::IsNullOrWhiteSpace($CapabilitySnapshot)) {
  $CapabilitySnapshotPath = Resolve-InputPath -Path $CapabilitySnapshot -RelativeTo $Root
} elseif ($null -ne $Release -and (Test-JsonProperty $Release "candidate") -and $null -ne $Release.candidate -and (Test-JsonProperty $Release.candidate "capability_snapshot_path")) {
  $ReleaseDirectory = Split-Path -Parent $ReleaseReportPath
  $CapabilitySnapshotPath = Resolve-InputPath -Path ([string]$Release.candidate.capability_snapshot_path) -RelativeTo $ReleaseDirectory
} else {
  Add-ValidationError "provide -CapabilitySnapshot or a release report with candidate.capability_snapshot_path"
}

if ([string]::IsNullOrWhiteSpace($ExpectedRevision)) {
  Add-ValidationError "provide -ExpectedCandidateId with an explicit snapshot, or a release report with candidate.candidate_id"
}

$Snapshot = Read-JsonDocument -Path $CapabilitySnapshotPath -Label "capability snapshot" -MaximumBytes $MaxSnapshotBytes
$SnapshotSource = ""
$SnapshotRevision = ""
$SnapshotCollectedAt = ""
$SourceRegistryRevision = ""
$SnapshotCollectedAtParsed = [DateTimeOffset]::MinValue

if ($null -ne $Snapshot) {
  $SnapshotSource = if (Test-JsonProperty $Snapshot "source") { [string]$Snapshot.source } else { "" }
  $SnapshotRevision = if (Test-JsonProperty $Snapshot "revision") { [string]$Snapshot.revision } else { "" }
  $SnapshotCollectedAt = if (Test-JsonProperty $Snapshot "collected_at") { [string]$Snapshot.collected_at } else { "" }
  $SourceRegistryRevision = if (Test-JsonProperty $Snapshot "source_registry_revision") { [string]$Snapshot.source_registry_revision } else { "" }

  if (-not (Test-JsonProperty $Snapshot "schema_version") -or $Snapshot.schema_version -ne 1) {
    Add-ValidationError "snapshot schema_version must be 1"
  }
  if ($SnapshotSource -cne $ExpectedSnapshotSource) {
    Add-ValidationError "snapshot source must be '$ExpectedSnapshotSource'"
  }
  if ($SourceRegistryRevision -cnotmatch '^[A-Za-z0-9][A-Za-z0-9._:/+-]{0,127}$') {
    Add-ValidationError "snapshot source_registry_revision is missing or invalid"
  }
  if (-not (Test-Revision $SnapshotRevision)) {
    Add-ValidationError "snapshot revision must be a 64-character candidate revision"
  } elseif ([string]::IsNullOrWhiteSpace($ExpectedRevision) -or $SnapshotRevision -cne $ExpectedRevision) {
    Add-ValidationError "snapshot revision does not match the candidate revision"
  }

  $HasOffset = $SnapshotCollectedAt -match '(Z|[+-][0-9]{2}:[0-9]{2})$'
  $TimestampParsed = $HasOffset -and [DateTimeOffset]::TryParse(
    $SnapshotCollectedAt,
    [Globalization.CultureInfo]::InvariantCulture,
    [Globalization.DateTimeStyles]::RoundtripKind,
    [ref]$SnapshotCollectedAtParsed
  )
  if (-not $TimestampParsed) {
    Add-ValidationError "snapshot collected_at must be an ISO-8601 timestamp with a timezone"
  } elseif ($SnapshotCollectedAtParsed -gt [DateTimeOffset]::UtcNow.AddMinutes(5)) {
    Add-ValidationError "snapshot collected_at must not be in the future"
  }

  if (-not (Test-JsonProperty $Snapshot "capabilities") -or $Snapshot.capabilities -isnot [System.Array]) {
    Add-ValidationError "snapshot capabilities must be a JSON array"
  }
}

$CapabilitiesById = @{}
if ($null -ne $Snapshot -and (Test-JsonProperty $Snapshot "capabilities") -and $Snapshot.capabilities -is [System.Array]) {
  foreach ($Capability in @($Snapshot.capabilities)) {
    if ($null -eq $Capability -or -not (Test-JsonProperty $Capability "id") -or [string]::IsNullOrWhiteSpace([string]$Capability.id)) {
      Add-ValidationError "snapshot contains a capability without an id"
      continue
    }
    $CapabilityId = [string]$Capability.id
    if ($CapabilityId -cnotmatch '^[a-z0-9][a-z0-9_]{0,63}$') {
      Add-ValidationError "snapshot contains an invalid capability id"
      continue
    }
    if ($CapabilitiesById.ContainsKey($CapabilityId)) {
      Add-ValidationError "snapshot contains duplicate capability id: $CapabilityId"
      continue
    }
    $CapabilitiesById[$CapabilityId] = $Capability
  }
}

$CapabilityEvaluations = @()
foreach ($Required in $RequiredCapabilities) {
  $Capability = if ($CapabilitiesById.ContainsKey($Required.id)) { $CapabilitiesById[$Required.id] } else { $null }
  $Evaluation = Get-CapabilityEvaluation `
    -Capability $Capability `
    -Id $Required.id `
    -Label $Required.label `
    -ExpectedRevision $ExpectedRevision `
    -ActiveRequired $true
  $CapabilityEvaluations += $Evaluation
}

$OpticalCapability = if ($CapabilitiesById.ContainsKey($OpticalCapabilityId)) { $CapabilitiesById[$OpticalCapabilityId] } else { $null }
$OpticalEvaluation = Get-CapabilityEvaluation `
  -Capability $OpticalCapability `
  -Id $OpticalCapabilityId `
  -Label "Optical production active transform" `
  -ExpectedRevision $ExpectedRevision `
  -ActiveRequired $false

$OpticalPolicyErrors = New-Object System.Collections.Generic.List[string]
foreach ($ExistingError in @($OpticalEvaluation.errors)) {
  $OpticalPolicyErrors.Add([string]$ExistingError) | Out-Null
}
if ($OpticalEvaluation.present) {
  if ($OpticalEvaluation.state -cne "implemented_disabled") {
    $OpticalPolicyErrors.Add("Optical active must remain implemented_disabled under the current shadow/disabled release policy") | Out-Null
  }
  if ($OpticalEvaluation.freshness -ceq "unknown") {
    $OpticalPolicyErrors.Add("Optical policy evidence freshness must not be unknown") | Out-Null
  }
  if (@($OpticalEvaluation.reason_codes | Where-Object { $OpticalPolicyReasonCodes -ccontains $_ }).Count -eq 0) {
    $OpticalPolicyErrors.Add("Optical active requires policy_shadow_only or policy_disabled reason evidence") | Out-Null
  }
}
$OpticalPolicyCompliant = ($OpticalPolicyErrors.Count -eq 0)
$OpticalReport = [pscustomobject]@{
  id = $OpticalEvaluation.id
  label = $OpticalEvaluation.label
  active_required = $false
  present = $OpticalEvaluation.present
  state = $OpticalEvaluation.state
  implementation_revision = $OpticalEvaluation.implementation_revision
  source_evidence_revision = $OpticalEvaluation.source_evidence_revision
  evidence_revision = $OpticalEvaluation.evidence_revision
  freshness = $OpticalEvaluation.freshness
  reason_codes = $OpticalEvaluation.reason_codes
  policy = "shadow-or-disabled"
  policy_compliant = $OpticalPolicyCompliant
  counts_toward_headroom_active_complete = $false
  errors = @($OpticalPolicyErrors)
}

$ActiveComplete = @($CapabilityEvaluations | Where-Object { $_.ok }).Count
$ActiveRequiredTotal = @($RequiredCapabilities).Count
$HeadroomActiveComplete = ($ActiveComplete -eq $ActiveRequiredTotal)
$SnapshotContractValid = ($script:ValidationErrors.Count -eq 0)
$Ok = ($SnapshotContractValid -and $HeadroomActiveComplete -and $OpticalPolicyCompliant)

$Report = [pscustomobject]@{
  ok = $Ok
  mode = "candidate-bound-headroom-capability-coverage"
  candidate_revision = if ([string]::IsNullOrWhiteSpace($ExpectedRevision)) { $null } else { $ExpectedRevision }
  candidate_binding_source = if ([string]::IsNullOrWhiteSpace($CandidateBindingSource)) { $null } else { $CandidateBindingSource }
  release_report = if ([string]::IsNullOrWhiteSpace($ReleaseReportPath)) { $null } else { $ReleaseReportPath }
  capability_snapshot = if ([string]::IsNullOrWhiteSpace($CapabilitySnapshotPath)) { $null } else { $CapabilitySnapshotPath }
  snapshot_source = if ([string]::IsNullOrWhiteSpace($SnapshotSource)) { $null } else { $SnapshotSource }
  snapshot_revision = if ([string]::IsNullOrWhiteSpace($SnapshotRevision)) { $null } else { $SnapshotRevision }
  source_registry_revision = if ([string]::IsNullOrWhiteSpace($SourceRegistryRevision)) { $null } else { $SourceRegistryRevision }
  snapshot_collected_at = if ([string]::IsNullOrWhiteSpace($SnapshotCollectedAt)) { $null } else { $SnapshotCollectedAt }
  snapshot_contract_valid = $SnapshotContractValid
  validation_errors = @($script:ValidationErrors)
  headroom_active_complete = $HeadroomActiveComplete
  active_complete = $ActiveComplete
  active_required_total = $ActiveRequiredTotal
  optical_policy_compliant = $OpticalPolicyCompliant
  summary = "Headroom active capability evidence: $ActiveComplete/$ActiveRequiredTotal candidate-bound requirements complete; Optical active is reported separately and never counts toward this total."
  privacy = "Reads only the explicitly selected, size-limited metadata JSON documents. Does not read secrets, auth files, raw prompts, raw responses, source files, or fixture results."
  external_services = "Does not access the network or start, stop, inspect, or reconfigure any runtime, Agent, Headroom, RTK, or cc-switch service."
  capabilities = $CapabilityEvaluations
  optical_active = $OpticalReport
}

$Stamp = (Get-Date).ToString("yyyyMMdd-HHmmss")
$ReportPath = Join-Path $OutputDir "headroom-capability-coverage-$Stamp.json"
$Report | Add-Member -NotePropertyName report_path -NotePropertyValue $ReportPath
[System.IO.File]::WriteAllText($ReportPath, ($Report | ConvertTo-Json -Depth 12), $Utf8NoBom)
$Report | ConvertTo-Json -Depth 12 -Compress

if (-not $Ok) {
  exit 1
}
