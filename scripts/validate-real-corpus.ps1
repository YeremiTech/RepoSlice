param(
  [string]$Corpus = "quality/real-project-corpus.tsv",
  [string]$WorkingDirectory = ".reposlice-quality",
  [string]$ResultsDirectory = "quality/results/real-corpus",
  [switch]$KeepRepositories
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest
$repoRoot = Split-Path -Parent $PSScriptRoot
Set-Location $repoRoot
$work = Join-Path $repoRoot $WorkingDirectory
$results = Join-Path $repoRoot $ResultsDirectory
New-Item -ItemType Directory -Force -Path $work, $results | Out-Null

$compatibilityRank = @{ L0 = 0; L1 = 1; L2 = 2; L3 = 3; L4 = 4; L5 = 5 }
$rows = Get-Content $Corpus | Where-Object { $_ -and -not $_.StartsWith("#") }
$failures = @()
$summary = @()

foreach ($row in $rows) {
  $parts = $row -split "`t"
  $caseName, $repositoryName, $url, $relativePath, $expectedRaw, $minimumCompatibility, $minimumComponents, $minimumEntrypoints = $parts
  $repositoryPath = Join-Path $work $repositoryName
  if (-not (Test-Path $repositoryPath)) {
    Write-Host "Cloning $repositoryName..."
    git -c credential.interactive=never clone --depth 1 $url $repositoryPath
    if ($LASTEXITCODE -ne 0) { throw "Could not clone corpus repository $repositoryName" }
  }

  $scanPath = if ($relativePath -eq ".") { $repositoryPath } else { Join-Path $repositoryPath $relativePath }
  if (-not (Test-Path $scanPath)) {
    $message = "$caseName: scan path does not exist"
    $failures += $message
    $summary += [pscustomobject]@{
      case = $caseName
      status = "FAIL"
      missingTechnologies = @()
      components = 0
      entrypoints = 0
      compatibility = $null
      message = $message
    }
    continue
  }

  Write-Host "Analyzing $caseName..."
  $raw = cargo run --locked -q -p reposlice-cli -- scan $scanPath --format json
  if ($LASTEXITCODE -ne 0) { throw "Analysis failed: $caseName" }
  $json = $raw | ConvertFrom-Json
  $detected = @($json.technologies | ForEach-Object { $_.name })
  $expected = $expectedRaw -split ","
  $missing = @($expected | Where-Object { $_ -notin $detected })
  $compatibilityOk = $compatibilityRank[$json.compatibility] -ge $compatibilityRank[$minimumCompatibility]
  $ok = $missing.Count -eq 0 -and $compatibilityOk -and [int]$json.components -ge [int]$minimumComponents -and [int]$json.entrypoints -ge [int]$minimumEntrypoints
  $message = if ($ok) {
    ""
  } else {
    "$caseName missing=[$($missing -join ',')] components=$($json.components)/$minimumComponents entrypoints=$($json.entrypoints)/$minimumEntrypoints compatibility=$($json.compatibility)/$minimumCompatibility"
  }
  if (-not $ok) { $failures += $message }

  $summary += [pscustomobject]@{
    case = $caseName
    status = $(if ($ok) { "PASS" } else { "FAIL" })
    expectedTechnologies = @($expected)
    detectedTechnologies = @($detected)
    missingTechnologies = @($missing)
    components = [int]$json.components
    minimumComponents = [int]$minimumComponents
    entrypoints = [int]$json.entrypoints
    minimumEntrypoints = [int]$minimumEntrypoints
    compatibility = [string]$json.compatibility
    minimumCompatibility = [string]$minimumCompatibility
    message = $message
  }

  Write-Host ("{0}: {1} | {2} components | {3} entrypoints | {4}" -f $caseName, $(if ($ok) {"PASS"} else {"FAIL"}), $json.components, $json.entrypoints, $json.compatibility)
}

[pscustomobject]@{
  generatedAtUtc = [DateTime]::UtcNow.ToString("o")
  cases = $summary
  passed = @($summary | Where-Object { $_.status -eq "PASS" }).Count
  failed = @($summary | Where-Object { $_.status -eq "FAIL" }).Count
} | ConvertTo-Json -Depth 20 | Set-Content -Encoding utf8 (Join-Path $results "summary.json")

if (-not $KeepRepositories) {
  Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
}
if ($failures.Count -gt 0) {
  Write-Error ("Real corpus validation failed:`n" + ($failures -join "`n"))
}
Write-Host "Real corpus validation passed ($($summary.Count) cases)."
