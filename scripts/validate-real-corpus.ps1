param(
  [string]$Corpus = "quality/real-project-corpus.tsv",
  [string]$WorkingDirectory = ".reposlice-quality",
  [switch]$KeepRepositories
)

$ErrorActionPreference = "Stop"
$repoRoot = Split-Path -Parent $PSScriptRoot
Set-Location $repoRoot
$work = Join-Path $repoRoot $WorkingDirectory
New-Item -ItemType Directory -Force -Path $work | Out-Null

$compatibilityRank = @{ L0 = 0; L1 = 1; L2 = 2; L3 = 3; L4 = 4; L5 = 5 }
$rows = Get-Content $Corpus | Where-Object { $_ -and -not $_.StartsWith("#") }
$failures = @()
foreach ($row in $rows) {
  $parts = $row -split "`t"
  $caseName, $repositoryName, $url, $relativePath, $expectedRaw, $minimumCompatibility, $minimumComponents, $minimumEntrypoints = $parts
  $repositoryPath = Join-Path $work $repositoryName
  if (-not (Test-Path $repositoryPath)) {
    Write-Host "Cloning $repositoryName..."
    git clone --depth 1 $url $repositoryPath
  }
  $scanPath = if ($relativePath -eq ".") { $repositoryPath } else { Join-Path $repositoryPath $relativePath }
  if (-not (Test-Path $scanPath)) {
    $failures += "$caseName: scan path does not exist: $scanPath"
    continue
  }

  Write-Host "Analyzing $caseName..."
  $json = cargo run --locked -q -p reposlice-cli -- scan $scanPath --format json | ConvertFrom-Json
  $detected = @($json.technologies | ForEach-Object { $_.name })
  $expected = $expectedRaw -split ","
  $missing = @($expected | Where-Object { $_ -notin $detected })
  $compatibilityOk = $compatibilityRank[$json.compatibility] -ge $compatibilityRank[$minimumCompatibility]
  $ok = $missing.Count -eq 0 -and $compatibilityOk -and [int]$json.components -ge [int]$minimumComponents -and [int]$json.entrypoints -ge [int]$minimumEntrypoints
  if (-not $ok) {
    $failures += "$caseName missing=[$($missing -join ',')] components=$($json.components)/$minimumComponents entrypoints=$($json.entrypoints)/$minimumEntrypoints compatibility=$($json.compatibility)/$minimumCompatibility"
  }
  Write-Host ("{0}: {1} | {2} components | {3} entrypoints | {4}" -f $caseName, $(if ($ok) {"PASS"} else {"FAIL"}), $json.components, $json.entrypoints, $json.compatibility)
}

if (-not $KeepRepositories) {
  Remove-Item -Recurse -Force $work
}
if ($failures.Count -gt 0) {
  Write-Error ("Real corpus validation failed:`n" + ($failures -join "`n"))
}
Write-Host "Real corpus validation passed."
