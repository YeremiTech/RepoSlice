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

function Resolve-RepositoryPath {
  param(
    [Parameter(Mandatory = $true)][string]$Base,
    [Parameter(Mandatory = $true)][string]$Relative
  )
  $baseFull = [IO.Path]::GetFullPath($Base).TrimEnd([IO.Path]::DirectorySeparatorChar, [IO.Path]::AltDirectorySeparatorChar)
  $candidate = [IO.Path]::GetFullPath((Join-Path $baseFull $Relative))
  $prefix = $baseFull + [IO.Path]::DirectorySeparatorChar
  if ($candidate -ne $baseFull -and -not $candidate.StartsWith($prefix, [StringComparison]::OrdinalIgnoreCase)) {
    throw "Path escapes the repository root: $Relative"
  }
  return $candidate
}

function Normalize-Remote {
  param([Parameter(Mandatory = $true)][string]$Url)
  $trimmed = $Url.Trim()
  if ($trimmed -match '^https://github\.com/([^/]+)/([^/?#]+?)(?:\.git)?/?$') {
    return "https://github.com/$($Matches[1].ToLowerInvariant())/$($Matches[2].ToLowerInvariant())"
  }
  throw "Corpus URL must be an HTTPS GitHub repository URL: $Url"
}

function Test-RepositoryRemote {
  param(
    [Parameter(Mandatory = $true)][string]$RepositoryPath,
    [Parameter(Mandatory = $true)][string]$Url
  )
  if (-not (Test-Path -LiteralPath (Join-Path $RepositoryPath ".git") -PathType Container)) {
    return $false
  }
  $remote = (& git -C $RepositoryPath remote get-url origin 2>$null | Out-String).Trim()
  if ($LASTEXITCODE -ne 0 -or [string]::IsNullOrWhiteSpace($remote)) {
    return $false
  }
  try { return (Normalize-Remote $remote) -eq (Normalize-Remote $Url) } catch { return $false }
}

$corpusPath = Resolve-RepositoryPath $repoRoot $Corpus
$work = Resolve-RepositoryPath $repoRoot $WorkingDirectory
$results = Resolve-RepositoryPath $repoRoot $ResultsDirectory
if (-not (Test-Path -LiteralPath $corpusPath -PathType Leaf)) {
  throw "Corpus file does not exist: $corpusPath"
}
New-Item -ItemType Directory -Force -Path $work, $results | Out-Null

$compatibilityRank = @{ L0 = 0; L1 = 1; L2 = 2; L3 = 3; L4 = 4; L5 = 5 }
$rows = Get-Content -LiteralPath $corpusPath | Where-Object { $_ -and -not $_.StartsWith("#") }
$failures = @()
$summary = @()

try {
  foreach ($row in $rows) {
    $parts = $row -split "`t", 9
    if ($parts.Count -ne 8 -or $parts | Where-Object { [string]::IsNullOrWhiteSpace($_) }) {
      throw "Corpus row must contain eight non-empty tab-separated fields: $row"
    }
    $caseName, $repositoryName, $url, $relativePath, $expectedRaw, $minimumCompatibility, $minimumComponents, $minimumEntrypoints = $parts
    if ($repositoryName -notmatch '^[A-Za-z0-9][A-Za-z0-9._-]*$') {
      throw "Invalid repository name in corpus: $repositoryName"
    }
    if (-not $compatibilityRank.ContainsKey($minimumCompatibility)) {
      throw "Invalid compatibility level for ${caseName}: $minimumCompatibility"
    }
    [int]$minimumComponents = $minimumComponents
    [int]$minimumEntrypoints = $minimumEntrypoints
    $repositoryPath = Resolve-RepositoryPath $work $repositoryName
    $normalizedUrl = Normalize-Remote $url

    if (Test-Path -LiteralPath $repositoryPath) {
      if (-not (Test-RepositoryRemote $repositoryPath $normalizedUrl)) {
        throw "Existing corpus directory has a different or incomplete Git remote: $repositoryPath"
      }
    } else {
      Write-Host "Cloning $repositoryName..."
      & git -c credential.interactive=never clone --depth 1 -- $normalizedUrl $repositoryPath
      if ($LASTEXITCODE -ne 0 -or -not (Test-RepositoryRemote $repositoryPath $normalizedUrl)) {
        throw "Could not clone corpus repository $repositoryName"
      }
    }

    $scanPath = if ($relativePath -eq ".") { $repositoryPath } else { Resolve-RepositoryPath $repositoryPath $relativePath }
    if (-not (Test-Path -LiteralPath $scanPath)) {
      $message = "${caseName}: scan path does not exist"
      $failures += $message
      $summary += [pscustomobject]@{
        case = $caseName; status = "FAIL"; missingTechnologies = @(); components = 0; entrypoints = 0; compatibility = $null; message = $message
      }
      continue
    }

    Write-Host "Analyzing $caseName..."
    $raw = & cargo run --locked -q -p reposlice-cli -- scan $scanPath --format json | Out-String
    if ($LASTEXITCODE -ne 0) { throw "Analysis failed: $caseName" }
    try { $json = $raw | ConvertFrom-Json } catch { throw "CLI returned invalid JSON for ${caseName}: $($_.Exception.Message)" }
    $detected = @($json.technologies | ForEach-Object { $_.name })
    $expected = @($expectedRaw -split "," | ForEach-Object { $_.Trim() } | Where-Object { $_ })
    $missing = @($expected | Where-Object { $_ -notin $detected })
    $compatibilityOk = $compatibilityRank[[string]$json.compatibility] -ge $compatibilityRank[$minimumCompatibility]
    $ok = $missing.Count -eq 0 -and $compatibilityOk -and [int]$json.components -ge $minimumComponents -and [int]$json.entrypoints -ge $minimumEntrypoints
    $message = if ($ok) { "" } else { "${caseName} missing=[$($missing -join ',')] components=$($json.components)/$minimumComponents entrypoints=$($json.entrypoints)/$minimumEntrypoints compatibility=$($json.compatibility)/$minimumCompatibility" }
    if (-not $ok) { $failures += $message }

    $summary += [pscustomobject]@{
      case = $caseName; status = $(if ($ok) { "PASS" } else { "FAIL" }); expectedTechnologies = @($expected); detectedTechnologies = @($detected); missingTechnologies = @($missing)
      components = [int]$json.components; minimumComponents = $minimumComponents; entrypoints = [int]$json.entrypoints; minimumEntrypoints = $minimumEntrypoints
      compatibility = [string]$json.compatibility; minimumCompatibility = [string]$minimumCompatibility; message = $message
    }
    Write-Host ("{0}: {1} | {2} components | {3} entrypoints | {4}" -f $caseName, $(if ($ok) { "PASS" } else { "FAIL" }), $json.components, $json.entrypoints, $json.compatibility)
  }
}
finally {
  [pscustomobject]@{
    generatedAtUtc = [DateTime]::UtcNow.ToString("o")
    cases = $summary
    passed = @($summary | Where-Object { $_.status -eq "PASS" }).Count
    failed = @($summary | Where-Object { $_.status -eq "FAIL" }).Count
  } | ConvertTo-Json -Depth 20 | Set-Content -Encoding utf8 (Join-Path $results "summary.json")
  if (-not $KeepRepositories) { Remove-Item -LiteralPath $work -Recurse -Force -ErrorAction SilentlyContinue }
}
if ($failures.Count -gt 0) { throw ("Real corpus validation failed:`n" + ($failures -join "`n")) }
Write-Host "Real corpus validation passed ($($summary.Count) cases)."
