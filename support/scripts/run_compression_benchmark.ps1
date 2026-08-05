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

Push-Location (Join-Path $Root "code")
try {
  $Raw = cargo run --quiet -p dtoken-compress --example benchmark
  $Payload = $Raw | ConvertFrom-Json
  $Cases = @($Payload.cases)
  if ($Cases.Count -eq 0) {
    throw "compression benchmark returned no cases"
  }

  $TotalOriginal = 0
  $TotalCompressed = 0
  $TotalSaved = 0
  $TotalEngineElapsed = 0
  $TotalWallElapsed = 0
  foreach ($Case in $Cases) {
    $TotalOriginal += [int]$Case.original_tokens
    $TotalCompressed += [int]$Case.compressed_tokens
    $TotalSaved += [int]$Case.tokens_saved
    $TotalEngineElapsed += [int64]$Case.engine_elapsed_us
    $TotalWallElapsed += [int64]$Case.wall_elapsed_us
  }

  $Summary = [pscustomobject]@{
    case_count = $Cases.Count
    total_original_tokens = $TotalOriginal
    total_compressed_tokens = $TotalCompressed
    total_saved_tokens = $TotalSaved
    total_savings_percent = if ($TotalOriginal -gt 0) { [math]::Round(($TotalSaved / $TotalOriginal) * 100, 2) } else { 0 }
    avg_engine_elapsed_us = [math]::Round($TotalEngineElapsed / [double]$Cases.Count, 2)
    avg_wall_elapsed_us = [math]::Round($TotalWallElapsed / [double]$Cases.Count, 2)
    best_case = ($Cases | Sort-Object -Property tokens_saved -Descending | Select-Object -First 1)
    best_ratio_case = ($Cases | Sort-Object -Property compression_ratio -Descending | Select-Object -First 1)
    worst_ratio_case = ($Cases | Sort-Object -Property compression_ratio | Select-Object -First 1)
  }

  $Report = [pscustomobject]@{
    source = "dtoken-compress-local-benchmark"
    generated_at = (Get-Date).ToString("o")
    privacy = "Synthetic local benchmark only; no prompt, provider, OpenCode, Headroom, RTK, or cc-switch config is read."
    summary = $Summary
    cases = $Cases
  }

  $Stamp = (Get-Date).ToString("yyyyMMdd-HHmmss")
  $JsonPath = Join-Path $OutputDir "compression-benchmark-$Stamp.json"
  [System.IO.File]::WriteAllText($JsonPath, ($Report | ConvertTo-Json -Depth 12), $Utf8NoBom)

  [pscustomobject]@{
    source = $Report.source
    case_count = $Summary.case_count
    total_original_tokens = $Summary.total_original_tokens
    total_compressed_tokens = $Summary.total_compressed_tokens
    total_saved_tokens = $Summary.total_saved_tokens
    total_savings_percent = $Summary.total_savings_percent
    avg_engine_elapsed_us = $Summary.avg_engine_elapsed_us
    avg_wall_elapsed_us = $Summary.avg_wall_elapsed_us
    best_case = "$($Summary.best_case.case)/$($Summary.best_case.policy)"
    best_case_saved_tokens = $Summary.best_case.tokens_saved
    best_ratio_case = "$($Summary.best_ratio_case.case)/$($Summary.best_ratio_case.policy)"
    best_ratio_percent = [math]::Round([double]$Summary.best_ratio_case.compression_ratio * 100, 2)
    report_path = $JsonPath
  } | ConvertTo-Json -Compress
} finally {
  Pop-Location
}
