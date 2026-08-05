param()

$ErrorActionPreference = "Stop"
$Root = Resolve-Path (Join-Path $PSScriptRoot "..\..")
. (Join-Path $PSScriptRoot "release_candidate_provenance.ps1")

function Invoke-TestGit {
  param(
    [string]$WorkingDirectory,
    [string[]]$Arguments
  )

  Push-Location $WorkingDirectory
  try {
    & git @Arguments | Out-Null
    if ($LASTEXITCODE -ne 0) {
      throw "test git command failed: git $($Arguments -join ' ')"
    }
  } finally {
    Pop-Location
  }
}

function New-TestCheck {
  param(
    [string]$Id,
    [bool]$Ok,
    [object]$Detail
  )

  [pscustomobject]@{ id = $Id; ok = $Ok; detail = $Detail }
}

$TempBase = [System.IO.Path]::GetFullPath([System.IO.Path]::GetTempPath()).TrimEnd('\', '/')
$TempRoot = Join-Path $TempBase "dtoken-release-candidate-test-$([Guid]::NewGuid().ToString('N'))"
$ResolvedTempRoot = [System.IO.Path]::GetFullPath($TempRoot)
if (-not $ResolvedTempRoot.StartsWith($TempBase + [System.IO.Path]::DirectorySeparatorChar, [System.StringComparison]::OrdinalIgnoreCase)) {
  throw "refusing to create release-candidate test outside the OS temp directory: $ResolvedTempRoot"
}

$Checks = @()
try {
  New-Item -ItemType Directory -Force -Path $TempRoot | Out-Null
  $EvidenceDir = Join-Path $TempRoot "evidence"
  $StampPath = Join-Path $EvidenceDir "candidate.json"
  $AppPath = Join-Path $EvidenceDir "app.bin"
  $InstallerPath = Join-Path $EvidenceDir "installer.bin"
  $SourcePath = Join-Path $TempRoot "source.txt"

  Invoke-TestGit -WorkingDirectory $TempRoot -Arguments @("init", "--quiet")
  Invoke-TestGit -WorkingDirectory $TempRoot -Arguments @("config", "user.email", "release-gate-test@d-token.local")
  Invoke-TestGit -WorkingDirectory $TempRoot -Arguments @("config", "user.name", "d-token release gate")
  [System.IO.File]::WriteAllText($SourcePath, "candidate-a", [System.Text.UTF8Encoding]::new($false))
  Invoke-TestGit -WorkingDirectory $TempRoot -Arguments @("add", "source.txt")
  Invoke-TestGit -WorkingDirectory $TempRoot -Arguments @("commit", "--quiet", "-m", "fixture")

  $CandidateA = Get-ReleaseCandidateProvenance -Root $TempRoot -OutputDir $EvidenceDir
  Write-ReleaseCandidateStamp -Root $TempRoot -Path $StampPath -OutputDir $EvidenceDir -Candidate $CandidateA | Out-Null
  Start-Sleep -Milliseconds 50
  [System.IO.File]::WriteAllText($AppPath, "app-a", [System.Text.UTF8Encoding]::new($false))
  [System.IO.File]::WriteAllText($InstallerPath, "installer-a", [System.Text.UTF8Encoding]::new($false))

  $Fresh = Get-ReleaseCandidateStampStatus -Root $TempRoot -StampPath $StampPath -ArtifactPaths @($AppPath, $InstallerPath) -OutputDir $EvidenceDir
  $Checks += New-TestCheck "fresh_stamp_matches_candidate" $Fresh.candidate_stamp_matches_worktree @{
    stamp = $Fresh.stamp.candidate_id
    current = $Fresh.current_candidate.candidate_id
  }
  $Checks += New-TestCheck "fresh_artifacts_are_newer_than_stamp" $Fresh.artifact_built_after_candidate_stamp @{
    stamp_last_write_time = $Fresh.stamp_last_write_time
  }

  [System.IO.File]::WriteAllText($SourcePath, "candidate-b", [System.Text.UTF8Encoding]::new($false))
  $StaleCandidate = Get-ReleaseCandidateStampStatus -Root $TempRoot -StampPath $StampPath -ArtifactPaths @($AppPath, $InstallerPath) -OutputDir $EvidenceDir
  $Checks += New-TestCheck "source_change_rejects_old_stamp" (-not $StaleCandidate.candidate_stamp_matches_worktree) @{
    stamp = $StaleCandidate.stamp.candidate_id
    current = $StaleCandidate.current_candidate.candidate_id
  }

  $CandidateB = Get-ReleaseCandidateProvenance -Root $TempRoot -OutputDir $EvidenceDir
  Write-ReleaseCandidateStamp -Root $TempRoot -Path $StampPath -OutputDir $EvidenceDir -Candidate $CandidateB | Out-Null
  $OldTime = (Get-Item -LiteralPath $StampPath).LastWriteTime.AddSeconds(-5)
  (Get-Item -LiteralPath $AppPath).LastWriteTime = $OldTime
  (Get-Item -LiteralPath $InstallerPath).LastWriteTime = $OldTime
  $StaleArtifacts = Get-ReleaseCandidateStampStatus -Root $TempRoot -StampPath $StampPath -ArtifactPaths @($AppPath, $InstallerPath) -OutputDir $EvidenceDir
  $Checks += New-TestCheck "old_artifacts_are_rejected" (-not $StaleArtifacts.artifact_built_after_candidate_stamp) @{
    stamp_last_write_time = $StaleArtifacts.stamp_last_write_time
  }
} finally {
  if (Test-Path -LiteralPath $TempRoot) {
    $Verified = [System.IO.Path]::GetFullPath($TempRoot)
    if (-not $Verified.StartsWith($TempBase + [System.IO.Path]::DirectorySeparatorChar, [System.StringComparison]::OrdinalIgnoreCase)) {
      throw "refusing to remove unexpected release-candidate test directory: $Verified"
    }
    Remove-Item -LiteralPath $Verified -Recurse -Force
  }
}

$Failed = @($Checks | Where-Object { -not $_.ok })
$Report = [pscustomobject]@{
  ok = ($Failed.Count -eq 0)
  mode = "release-candidate-provenance-test"
  passed = $Checks.Count - $Failed.Count
  total = $Checks.Count
  checks = $Checks
}
$Report | ConvertTo-Json -Depth 6 -Compress
if ($Failed.Count -gt 0) {
  exit 1
}
