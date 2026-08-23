$ErrorActionPreference = "Stop"

function Get-ReleaseSha256Text {
  param([AllowEmptyString()][string]$Text)

  $Bytes = [System.Text.Encoding]::UTF8.GetBytes($Text)
  $Sha = [System.Security.Cryptography.SHA256]::Create()
  try {
    return ([System.BitConverter]::ToString($Sha.ComputeHash($Bytes))).Replace("-", "").ToLowerInvariant()
  } finally {
    $Sha.Dispose()
  }
}

function Invoke-ReleaseMetadataCommand {
  param(
    [string]$WorkingDirectory,
    [string]$FilePath,
    [string[]]$ArgumentList,
    [switch]$AllowFailure
  )

  Push-Location $WorkingDirectory
  try {
    $global:LASTEXITCODE = 0
    $PreviousErrorActionPreference = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    try {
      # Git may emit line-ending notices on stderr while still succeeding. Keep
      # candidate hashes based on stdout only and use the exit code for failure.
      $Output = @(& $FilePath @ArgumentList 2>$null | ForEach-Object { $_.ToString() })
      $ExitCode = $global:LASTEXITCODE
    } finally {
      $ErrorActionPreference = $PreviousErrorActionPreference
    }
  } finally {
    Pop-Location
  }

  if ($ExitCode -ne 0 -and -not $AllowFailure) {
    throw "metadata command failed ($FilePath $($ArgumentList -join ' ')): $($Output -join [Environment]::NewLine)"
  }

  [pscustomobject]@{
    ok = ($ExitCode -eq 0)
    exit_code = $ExitCode
    lines = $Output
    text = ($Output -join "`n").Trim()
  }
}

function Get-ReleaseRelativePath {
  param(
    [string]$Root,
    [string]$Path
  )

  $RootFull = [System.IO.Path]::GetFullPath($Root).TrimEnd('\', '/')
  $PathFull = [System.IO.Path]::GetFullPath($Path)
  $Prefix = $RootFull + [System.IO.Path]::DirectorySeparatorChar
  if (-not $PathFull.StartsWith($Prefix, [System.StringComparison]::OrdinalIgnoreCase)) {
    return $null
  }
  return $PathFull.Substring($Prefix.Length).Replace('\', '/')
}

function Test-ReleaseCandidateExcludedPath {
  param(
    [string]$RelativePath,
    [string[]]$AdditionalExcludedPrefixes = @()
  )

  $Normalized = $RelativePath.Replace('\', '/').TrimStart('./')
  $Prefixes = @(
    ".ai-bridge/",
    ".codebase-memory/",
    ".commandcode/",
    ".planning/",
    "code/target/",
    "code/apps/desktop/dist/",
    "code/apps/desktop/node_modules/",
    "support/artifacts/"
  ) + @($AdditionalExcludedPrefixes)

  foreach ($Prefix in $Prefixes) {
    if ([string]::IsNullOrWhiteSpace($Prefix)) { continue }
    $NormalizedPrefix = $Prefix.Replace('\', '/').TrimStart('./').TrimEnd('/') + "/"
    if ($Normalized.StartsWith($NormalizedPrefix, [System.StringComparison]::OrdinalIgnoreCase)) {
      return $true
    }
  }
  return $false
}

function Get-ReleaseCandidateProvenance {
  [CmdletBinding()]
  param(
    [Parameter(Mandatory = $true)][string]$Root,
    [string]$OutputDir = ""
  )

  $RootPath = (Resolve-Path -LiteralPath $Root).Path
  $GitHeadResult = Invoke-ReleaseMetadataCommand -WorkingDirectory $RootPath -FilePath "git" -ArgumentList @("rev-parse", "HEAD")
  $GitStatusResult = Invoke-ReleaseMetadataCommand -WorkingDirectory $RootPath -FilePath "git" -ArgumentList @("-c", "core.quotepath=false", "status", "--porcelain=v1", "--untracked-files=all")
  $TrackedDiffResult = Invoke-ReleaseMetadataCommand -WorkingDirectory $RootPath -FilePath "git" -ArgumentList @("diff", "--binary", "HEAD", "--", ".")
  # N0 canonical source digest: SHA-256 of canonical JSON for
  # {tracked path: blob SHA-256} at HEAD. The tracked Python helper is shared
  # by N0 and every upstream producer; failures remain unresolved/fail-closed.
  $SourceDigestResult = Invoke-ReleaseMetadataCommand -WorkingDirectory $RootPath -FilePath "python3" -ArgumentList @(
    (Join-Path $PSScriptRoot "canonical_source_digest.py"),
    "--repo-root", $RootPath,
    "--revision", "HEAD",
    "--digest-only"
  ) -AllowFailure
  $UntrackedResult = Invoke-ReleaseMetadataCommand -WorkingDirectory $RootPath -FilePath "git" -ArgumentList @("-c", "core.quotepath=false", "ls-files", "--others", "--exclude-standard")

  $AdditionalExcludedPrefixes = @()
  if (-not [string]::IsNullOrWhiteSpace($OutputDir)) {
    $OutputRelative = Get-ReleaseRelativePath -Root $RootPath -Path $OutputDir
    if (-not [string]::IsNullOrWhiteSpace($OutputRelative)) {
      $AdditionalExcludedPrefixes += $OutputRelative
    }
  }

  $UntrackedManifest = @()
  foreach ($RelativePath in @($UntrackedResult.lines | Where-Object { -not [string]::IsNullOrWhiteSpace($_) } | Sort-Object)) {
    $NormalizedPath = $RelativePath.Replace('\', '/')
    if (Test-ReleaseCandidateExcludedPath -RelativePath $NormalizedPath -AdditionalExcludedPrefixes $AdditionalExcludedPrefixes) {
      continue
    }

    $FullPath = Join-Path $RootPath ($NormalizedPath.Replace('/', [System.IO.Path]::DirectorySeparatorChar))
    if (-not (Test-Path -LiteralPath $FullPath -PathType Leaf)) {
      continue
    }
    $UntrackedManifest += "$NormalizedPath`t$((Get-FileHash -LiteralPath $FullPath -Algorithm SHA256).Hash.ToLowerInvariant())"
  }

  $TrackedDiffSha256 = Get-ReleaseSha256Text $TrackedDiffResult.text
  $SourceDigest = if ($SourceDigestResult.ok -and $SourceDigestResult.text -match "^[0-9a-f]{64}$") {
    $SourceDigestResult.text
  } else {
    ""
  }
  $UntrackedManifestText = $UntrackedManifest -join "`n"
  $UntrackedManifestSha256 = Get-ReleaseSha256Text $UntrackedManifestText
  $NodeResult = Invoke-ReleaseMetadataCommand -WorkingDirectory $RootPath -FilePath "node" -ArgumentList @("--version") -AllowFailure
  $RustcResult = Invoke-ReleaseMetadataCommand -WorkingDirectory $RootPath -FilePath "rustc" -ArgumentList @("--version") -AllowFailure
  $CargoResult = Invoke-ReleaseMetadataCommand -WorkingDirectory $RootPath -FilePath "cargo" -ArgumentList @("--version") -AllowFailure
  $Platform = [pscustomobject]@{
    os_description = [System.Runtime.InteropServices.RuntimeInformation]::OSDescription.Trim()
    os_architecture = ([string][System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture).Trim()
    process_architecture = ([string][System.Runtime.InteropServices.RuntimeInformation]::ProcessArchitecture).Trim()
    powershell = $PSVersionTable.PSVersion.ToString().Trim()
  }
  $CandidateMaterial = @(
    "git_head=$($GitHeadResult.text)",
    "tracked_diff_sha256=$TrackedDiffSha256",
    "untracked_manifest_sha256=$UntrackedManifestSha256",
    "os_description=$($Platform.os_description)",
    "os_architecture=$($Platform.os_architecture)",
    "process_architecture=$($Platform.process_architecture)",
    "node=$($NodeResult.text)",
    "rustc=$($RustcResult.text)",
    "cargo=$($CargoResult.text)"
  ) -join "`n"
  $CandidateId = Get-ReleaseSha256Text $CandidateMaterial

  [pscustomobject]@{
    candidate_id = $CandidateId
    captured_at = (Get-Date).ToString("o")
    repository_root = $RootPath
    git_head = $GitHeadResult.text
    git_dirty = (@($GitStatusResult.lines | Where-Object { -not [string]::IsNullOrWhiteSpace($_) }).Count -gt 0)
    git_status_entry_count = @($GitStatusResult.lines | Where-Object { -not [string]::IsNullOrWhiteSpace($_) }).Count
    git_status_sha256 = Get-ReleaseSha256Text $GitStatusResult.text
    candidate_dirty = (-not [string]::IsNullOrWhiteSpace($TrackedDiffResult.text)) -or ($UntrackedManifest.Count -gt 0)
    tracked_diff_sha256 = $TrackedDiffSha256
    source_digest = $SourceDigest
    untracked_file_count = $UntrackedManifest.Count
    untracked_manifest_sha256 = $UntrackedManifestSha256
    platform = $Platform
    node = if ($NodeResult.ok) { $NodeResult.text } else { $null }
    rustc = if ($RustcResult.ok) { $RustcResult.text } else { $null }
    cargo = if ($CargoResult.ok) { $CargoResult.text } else { $null }
  }
}

function Write-ReleaseCandidateStamp {
  [CmdletBinding()]
  param(
    [Parameter(Mandatory = $true)][string]$Root,
    [Parameter(Mandatory = $true)][string]$Path,
    [string]$OutputDir = "",
    [object]$Candidate = $null
  )

  if ($null -eq $Candidate) {
    $Candidate = Get-ReleaseCandidateProvenance -Root $Root -OutputDir $OutputDir
  }
  $Parent = Split-Path -Parent $Path
  New-Item -ItemType Directory -Force -Path $Parent | Out-Null
  $TempPath = "$Path.tmp-$([Guid]::NewGuid().ToString('N'))"
  $Utf8NoBom = [System.Text.UTF8Encoding]::new($false)
  try {
    [System.IO.File]::WriteAllText($TempPath, ($Candidate | ConvertTo-Json -Depth 8), $Utf8NoBom)
    Move-Item -LiteralPath $TempPath -Destination $Path -Force
  } finally {
    if (Test-Path -LiteralPath $TempPath) {
      Remove-Item -LiteralPath $TempPath -Force
    }
  }
  return $Candidate
}

function Get-ReleaseCandidateStampStatus {
  [CmdletBinding()]
  param(
    [Parameter(Mandatory = $true)][string]$Root,
    [Parameter(Mandatory = $true)][string]$StampPath,
    [string[]]$ArtifactPaths = @(),
    [string]$OutputDir = ""
  )

  $CurrentCandidate = Get-ReleaseCandidateProvenance -Root $Root -OutputDir $OutputDir
  $StampItem = if (Test-Path -LiteralPath $StampPath -PathType Leaf) { Get-Item -LiteralPath $StampPath } else { $null }
  $Stamp = if ($null -ne $StampItem) {
    try { Get-Content -LiteralPath $StampItem.FullName -Raw | ConvertFrom-Json } catch { $null }
  } else { $null }
  $MissingArtifacts = @($ArtifactPaths | Where-Object { [string]::IsNullOrWhiteSpace($_) -or -not (Test-Path -LiteralPath $_ -PathType Leaf) })
  $ArtifactsBuiltAfterStamp = $false
  if ($null -ne $StampItem -and $MissingArtifacts.Count -eq 0 -and $ArtifactPaths.Count -gt 0) {
    $ArtifactsBuiltAfterStamp = (@($ArtifactPaths | Where-Object { (Get-Item -LiteralPath $_).LastWriteTime -lt $StampItem.LastWriteTime }).Count -eq 0)
  }

  [pscustomobject]@{
    stamp_exists = ($null -ne $StampItem)
    stamp_parsed = ($null -ne $Stamp)
    stamp_path = $StampPath
    stamp = $Stamp
    current_candidate = $CurrentCandidate
    candidate_stamp_matches_worktree = (
      $null -ne $Stamp -and
      -not [string]::IsNullOrWhiteSpace([string]$Stamp.candidate_id) -and
      [string]$Stamp.candidate_id -eq $CurrentCandidate.candidate_id
    )
    artifact_built_after_candidate_stamp = $ArtifactsBuiltAfterStamp
    artifact_paths = $ArtifactPaths
    missing_artifacts = $MissingArtifacts
    stamp_last_write_time = if ($null -ne $StampItem) { $StampItem.LastWriteTime.ToString("o") } else { $null }
  }
}
