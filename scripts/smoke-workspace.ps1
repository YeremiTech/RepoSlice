$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

function Invoke-RepoSliceJson {
    param([Parameter(Mandatory = $true)][string[]]$CommandArgs)
    $cargoArgs = @("run", "--locked", "-q", "-p", "reposlice-cli", "--") + $CommandArgs + @("--format", "json")
    $output = & cargo @cargoArgs
    if ($LASTEXITCODE -ne 0) {
        throw "RepoSlice command failed: $($CommandArgs -join ' ')"
    }
    $text = ($output | Out-String).Trim()
    if ([string]::IsNullOrWhiteSpace($text)) {
        throw "RepoSlice command returned no JSON: $($CommandArgs -join ' ')"
    }
    return $text | ConvertFrom-Json
}

$temp = Join-Path ([IO.Path]::GetTempPath()) ("reposlice-smoke-{0}-{1}" -f $PID, [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds())
$previousHome = $env:REPOSLICE_HOME
try {
    New-Item -ItemType Directory -Force $temp | Out-Null
    $env:REPOSLICE_HOME = Join-Path $temp "home"
    $repoA = Join-Path $temp "repo-a"
    $repoB = Join-Path $temp "repo-b"
    New-Item -ItemType Directory -Force $repoA, $repoB | Out-Null

    @'
{
  "name": "smoke-api",
  "dependencies": { "express": "1.0.0" }
}
'@ | Set-Content -Encoding utf8 (Join-Path $repoA "package.json")
    @'
const express = require("express");
const app = express();
app.get("/health", (_req, res) => res.send("ok"));
'@ | Set-Content -Encoding utf8 (Join-Path $repoA "index.js")

    @'
{
  "name": "smoke-web",
  "dependencies": { "react": "1.0.0" }
}
'@ | Set-Content -Encoding utf8 (Join-Path $repoB "package.json")
    @'
export async function loadHealth() {
  return fetch("/health");
}
'@ | Set-Content -Encoding utf8 (Join-Path $repoB "client.ts")

    $workspace = Invoke-RepoSliceJson -CommandArgs @("create-workspace", "Smoke Workspace")
    $first = Invoke-RepoSliceJson -CommandArgs @("add-repository", $workspace.id, $repoA)
    $second = Invoke-RepoSliceJson -CommandArgs @("add-repository", $workspace.id, $repoB)

    $scan = Invoke-RepoSliceJson -CommandArgs @("scan-workspace", $workspace.id)
    if ($scan.repositories -ne 2) { throw "Expected two repositories after workspace scan" }

    $cached = Invoke-RepoSliceJson -CommandArgs @("cached-workspace", $workspace.id)
    if ($cached.repositories -ne 2) { throw "Persisted workspace cache lost repositories" }

    $integrity = Invoke-RepoSliceJson -CommandArgs @("validate-workspace", $workspace.id)
    if (-not $integrity.passed) { throw "Workspace integrity validation failed" }

    $fullModel = Invoke-RepoSliceJson -CommandArgs @("export-workspace", $workspace.id)
    if ($fullModel.repositories.Count -ne 2) { throw "JSON workspace export lost repositories" }

    $mermaidPath = Join-Path $temp "workspace.mmd"
    & cargo run --locked -q -p reposlice-cli -- export-workspace $workspace.id --format mermaid --output $mermaidPath
    if ($LASTEXITCODE -ne 0 -or -not (Test-Path $mermaidPath -PathType Leaf)) { throw "Mermaid workspace export failed" }

    $graphmlPath = Join-Path $temp "workspace.graphml"
    & cargo run --locked -q -p reposlice-cli -- export-workspace $workspace.id --format graphml --output $graphmlPath
    if ($LASTEXITCODE -ne 0 -or -not (Test-Path $graphmlPath -PathType Leaf)) { throw "GraphML workspace export failed" }

    Add-Content -Encoding utf8 (Join-Path $repoA "index.js") "`napp.post('/items', (_req, res) => res.sendStatus(204));"
    $refresh = Invoke-RepoSliceJson -CommandArgs @("scan-repository", $workspace.id, $first.id)
    if ($refresh.repositories -ne 2) { throw "Single-repository refresh did not preserve the full workspace" }

    $integrityAfterRefresh = Invoke-RepoSliceJson -CommandArgs @("validate-workspace", $workspace.id)
    if (-not $integrityAfterRefresh.passed) { throw "Workspace integrity failed after incremental refresh" }

    $audit = Invoke-RepoSliceJson -CommandArgs @("audit", $workspace.id)
    if ($audit.analyses.Count -ne 2) { throw "Expected persisted audit records for both repositories" }

    Invoke-RepoSliceJson -CommandArgs @("remove-repository", $workspace.id, $second.id) | Out-Null
    $repositories = Invoke-RepoSliceJson -CommandArgs @("repositories", $workspace.id)
    if ($repositories.Count -ne 1 -or $repositories[0].id -ne $first.id) {
        throw "Repository removal did not preserve the expected workspace registry"
    }
    if (-not (Test-Path $repoB -PathType Container)) {
        throw "Removing an external repository deleted the original source directory"
    }
    $invalidated = Invoke-RepoSliceJson -CommandArgs @("cached-workspace", $workspace.id)
    if ($invalidated.cached -ne $false) {
        throw "Repository removal did not invalidate the persisted workspace cache"
    }

    Write-Host "RepoSlice workspace lifecycle smoke test passed."
}
finally {
    if ($null -eq $previousHome) {
        Remove-Item Env:REPOSLICE_HOME -ErrorAction SilentlyContinue
    } else {
        $env:REPOSLICE_HOME = $previousHome
    }
    Remove-Item -Recurse -Force $temp -ErrorAction SilentlyContinue
}
