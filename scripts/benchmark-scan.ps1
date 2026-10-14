param(
  [Parameter(Mandatory=$true)][string]$Path,
  [ValidateRange(1,10)][int]$Runs = 3
)

$ErrorActionPreference = "Stop"
$repoRoot = Split-Path -Parent $PSScriptRoot
Set-Location $repoRoot
$resolved = (Resolve-Path $Path).Path
cargo run --locked -q -p reposlice-cli -- benchmark $resolved --runs $Runs --format json | ConvertFrom-Json | Format-List
