param(
  [string]$OutputDir = "",
  [string[]]$AdditionalPath = @(),
  [switch]$All
)

$ErrorActionPreference = "Stop"
$Root = Resolve-Path (Join-Path $PSScriptRoot "..\..")
$Utf8NoBom = [System.Text.UTF8Encoding]::new($false)
if ([string]::IsNullOrWhiteSpace($OutputDir)) {
  $OutputDir = Join-Path $Root "support\artifacts"
}
New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null

$BlockedPropertyNames = @(
  "raw_prompt",
  "raw_response",
  "api_key",
  "authorization",
  "cookie",
  "bearer",
  "output",
  "prompt",
  "response"
)

$SensitiveValuePatterns = @(
  @{ id = "openai_style_secret"; pattern = "sk-[A-Za-z0-9_\-]{16,}" },
  @{ id = "bearer_token_value"; pattern = "(?i)\bBearer\s+[A-Za-z0-9_\-\.]{10,}" },
  @{ id = "aws_access_key"; pattern = "AKIA[0-9A-Z]{16}" },
  @{ id = "google_api_key"; pattern = "AIza[0-9A-Za-z_\-]{20,}" }
)

function Get-JsonArtifactsToScan {
  $Files = @()
  if ($All) {
    $Files += @(Get-ChildItem -LiteralPath $OutputDir -Filter "*.json" -File -ErrorAction SilentlyContinue)
  } else {
    $Prefixes = @(
      "release-verification-",
      "release-performance-gate",
      "opencode-workload-benchmark-",
      "product-readiness-command-",
      "dtoken-service-lifecycle-",
      "headroom-capability-coverage-",
      "desktop-packaging-audit-",
      "desktop-bundle-artifact-audit-",
      "release-distribution-manifest-"
    )

    foreach ($Prefix in $Prefixes) {
      $Latest = Get-ChildItem -LiteralPath $OutputDir -Filter "$Prefix*.json" -File -ErrorAction SilentlyContinue |
        Sort-Object LastWriteTime -Descending |
        Select-Object -First 1
      if ($null -ne $Latest) {
        $Files += $Latest
      }
    }
  }

  foreach ($Candidate in $AdditionalPath) {
    if ([string]::IsNullOrWhiteSpace($Candidate)) {
      continue
    }
    $Resolved = Resolve-Path -LiteralPath $Candidate -ErrorAction SilentlyContinue
    if ($null -eq $Resolved) {
      throw "Additional redaction audit path is missing: $Candidate"
    }
    $Item = Get-Item -LiteralPath $Resolved.Path
    if ($Item.PSIsContainer) {
      $Files += @(Get-ChildItem -LiteralPath $Item.FullName -Filter "*.json" -File -Recurse -ErrorAction SilentlyContinue)
    } elseif ($Item.Extension -eq ".json") {
      $Files += $Item
    }
  }

  return @($Files | Sort-Object FullName -Unique)
}

function Add-Hit {
  param(
    [System.Collections.ArrayList]$Hits,
    [string]$File,
    [string]$Kind,
    [string]$Path,
    [string]$Pattern
  )
  [void]$Hits.Add([pscustomobject]@{
    file = $File
    kind = $Kind
    json_path = $Path
    pattern = $Pattern
  })
}

function Scan-Node {
  param(
    [object]$Node,
    [string]$JsonPath,
    [string]$File,
    [System.Collections.ArrayList]$Hits
  )

  if ($null -eq $Node) {
    return
  }

  if ($Node -is [string]) {
    foreach ($PatternSpec in $SensitiveValuePatterns) {
      if ($Node -match $PatternSpec.pattern) {
        Add-Hit $Hits $File "sensitive_value_pattern" $JsonPath $PatternSpec.id
      }
    }
    return
  }

  if ($Node -is [System.Collections.IEnumerable] -and -not ($Node -is [string]) -and -not ($Node -is [System.Management.Automation.PSCustomObject])) {
    $Index = 0
    foreach ($Item in $Node) {
      Scan-Node -Node $Item -JsonPath "$JsonPath[$Index]" -File $File -Hits $Hits
      $Index += 1
    }
    return
  }

  if ($Node -is [System.Management.Automation.PSCustomObject]) {
    foreach ($Property in $Node.PSObject.Properties) {
      $Name = [string]$Property.Name
      $Lower = $Name.ToLowerInvariant()
      $ChildPath = if ([string]::IsNullOrWhiteSpace($JsonPath)) { $Name } else { "$JsonPath.$Name" }
      if ($BlockedPropertyNames -contains $Lower) {
        Add-Hit $Hits $File "blocked_property_name" $ChildPath $Lower
      }
      Scan-Node -Node $Property.Value -JsonPath $ChildPath -File $File -Hits $Hits
    }
    return
  }
}

$Artifacts = Get-JsonArtifactsToScan
$Hits = [System.Collections.ArrayList]::new()
$ParseErrors = [System.Collections.ArrayList]::new()

foreach ($Artifact in $Artifacts) {
  try {
    $Raw = Get-Content -LiteralPath $Artifact.FullName -Raw
    $Json = $Raw | ConvertFrom-Json
    Scan-Node -Node $Json -JsonPath "" -File $Artifact.FullName -Hits $Hits
  } catch {
    [void]$ParseErrors.Add([pscustomobject]@{
      file = $Artifact.FullName
      error = $_.Exception.Message
    })
  }
}

$Ok = (($Hits.Count -eq 0) -and ($ParseErrors.Count -eq 0) -and ($Artifacts.Count -gt 0))
$Report = [pscustomobject]@{
  ok = $Ok
  mode = "release-artifact-redaction-audit"
  generated_at = (Get-Date).ToString("o")
  scanned_files = @($Artifacts | ForEach-Object { $_.FullName })
  scanned_file_count = $Artifacts.Count
  hit_count = $Hits.Count
  parse_error_count = $ParseErrors.Count
  policy = "Release artifacts must not contain raw prompt/response/output fields or credential-shaped values. The audit records only paths and pattern names, never matched secret values."
  hits = $Hits
  parse_errors = $ParseErrors
}

$Stamp = (Get-Date).ToString("yyyyMMdd-HHmmss")
$ReportPath = Join-Path $OutputDir "release-artifact-redaction-audit-$Stamp.json"
$Report | Add-Member -NotePropertyName report_path -NotePropertyValue $ReportPath
[System.IO.File]::WriteAllText($ReportPath, ($Report | ConvertTo-Json -Depth 10), $Utf8NoBom)
$Report | ConvertTo-Json -Depth 8 -Compress

if (-not $Report.ok) {
  exit 1
}
