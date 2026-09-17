param(
  [string]$Corpus = "quality/performance-corpus.tsv",
  [string]$Thresholds = "quality/performance-thresholds.json",
  [string]$WorkingDirectory = ".reposlice-performance",
  [string]$ResultsDirectory = "quality/results/performance",
  [switch]$KeepRepositories
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest
$repoRoot = Split-Path -Parent $PSScriptRoot
Set-Location $repoRoot
$work = Join-Path $repoRoot $WorkingDirectory
$results = Join-Path $repoRoot $ResultsDirectory
New-Item -ItemType Directory -Force $work, $results | Out-Null
$limits = Get-Content (Join-Path $repoRoot $Thresholds) -Raw | ConvertFrom-Json
$rows = Get-Content (Join-Path $repoRoot $Corpus) | Where-Object { $_ -and -not $_.StartsWith("#") }
$summary = @()

foreach ($row in $rows) {
  $parts = $row -split "`t"
  $caseName, $repositoryName, $url, $relativePath, $runs = $parts
  $repositoryPath = Join-Path $work $repositoryName
  if (-not (Test-Path $repositoryPath)) {
    git -c credential.interactive=never clone --depth 1 $url $repositoryPath
    if ($LASTEXITCODE -ne 0) { throw "Could not clone benchmark repository $repositoryName" }
  }
  $scanPath = if ($relativePath -eq ".") { $repositoryPath } else { Join-Path $repositoryPath $relativePath }
  if (-not (Test-Path $scanPath)) { throw "Benchmark scan path does not exist: $scanPath" }
  $raw = cargo run --locked -q -p reposlice-cli -- benchmark $scanPath --runs $runs --format json
  if ($LASTEXITCODE -ne 0) { throw "Benchmark failed: $caseName" }
  $metric = $raw | ConvertFrom-Json
  if ([int]$metric.files -lt [int]$limits.minimumFiles) { throw "$caseName returned no useful files" }
  if ([int]$metric.model_json_bytes -lt [int]$limits.minimumModelBytes) { throw "$caseName returned an unexpectedly empty model" }
  if ([double]$metric.cold_files_per_second -le 0 -or [double]$metric.warm_files_per_second -le 0) { throw "$caseName returned invalid throughput" }
  if ([double]$metric.warm_average_ms -gt ([double]$metric.cold_ms * [double]$limits.warmRegressionBudget)) {
    throw "$caseName warm scan exceeded regression budget: cold=$($metric.cold_ms)ms warm=$($metric.warm_average_ms)ms"
  }
  $diagnosticsPerFile = if ([int]$metric.files -gt 0) { [double]$metric.diagnostics / [double]$metric.files } else { 0 }
  if ($diagnosticsPerFile -gt [double]$limits.maximumDiagnosticsPerFile) { throw "$caseName diagnostic density is unexpectedly high" }
  $metric | ConvertTo-Json -Depth 20 | Set-Content -Encoding utf8 (Join-Path $results "$caseName.json")
  $summary += [pscustomobject]@{
    case = $caseName
    files = [int]$metric.files
    components = [int]$metric.components
    entrypoints = [int]$metric.entrypoints
    dependencies = [int]$metric.dependencies
    coldMs = [double]$metric.cold_ms
    warmMs = [double]$metric.warm_average_ms
    p95Ms = [double]$metric.p95_ms
    modelBytes = [int]$metric.model_json_bytes
  }
}

$summary | ConvertTo-Json -Depth 10 | Set-Content -Encoding utf8 (Join-Path $results "summary.json")
if (-not $KeepRepositories) { Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue }
Write-Host "RepoSlice performance corpus passed ($($summary.Count) cases)."
