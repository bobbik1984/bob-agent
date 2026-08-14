param(
    [string]$OutputDirectory = (Join-Path ([System.IO.Path]::GetTempPath()) 'bob-phase55-baseline')
)

$ErrorActionPreference = 'Stop'
$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$capturedAt = (Get-Date).ToUniversalTime().ToString('o')

function Get-FileMeasurement {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [string]$Provenance = 'current_worktree_file'
    )

    $item = Get-Item -LiteralPath $Path
    $rootUri = [System.Uri]::new(($repoRoot.TrimEnd('\') + '\'))
    $itemUri = [System.Uri]::new($item.FullName)
    $relativePath = [System.Uri]::UnescapeDataString($rootUri.MakeRelativeUri($itemUri).ToString())
    [ordered]@{
        path = $relativePath
        bytes = $item.Length
        sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $item.FullName).Hash
        provenance = $Provenance
    }
}

function Get-GitCommit {
    try {
        $value = & git -c "safe.directory=$($repoRoot.Replace('\', '/'))" -C $repoRoot rev-parse HEAD 2>$null
        if ($LASTEXITCODE -eq 0) { return $value.Trim() }
    } catch {
    }
    return 'not_measured'
}

function Get-CargoVersion {
    $cargoPath = Join-Path $repoRoot 'src-tauri\Cargo.toml'
    $match = Select-String -LiteralPath $cargoPath -Pattern '^version\s*=\s*"([^"]+)"' | Select-Object -First 1
    if ($match -and $match.Matches.Count -gt 0) {
        return $match.Matches[0].Groups[1].Value
    }
    return 'not_measured'
}

$artifactRoots = @(
    (Join-Path $repoRoot 'dist-release'),
    (Join-Path $repoRoot 'src-tauri\gen\android\app\build\outputs')
)
$artifactExtensions = @('.exe', '.msi', '.zip', '.apk', '.aab')
$artifacts = @()
foreach ($artifactRoot in $artifactRoots) {
    if (-not (Test-Path -LiteralPath $artifactRoot)) { continue }
    Get-ChildItem -LiteralPath $artifactRoot -File -Recurse | Where-Object {
        $artifactExtensions -contains $_.Extension.ToLowerInvariant()
    } | ForEach-Object {
        $artifacts += Get-FileMeasurement -Path $_.FullName -Provenance 'existing_artifact_unverified'
    }
}

$manifestPaths = @(
    'package.json',
    'package-lock.json',
    'src-tauri\Cargo.toml',
    'src-tauri\Cargo.lock'
)
$manifests = @()
foreach ($relativePath in $manifestPaths) {
    $fullPath = Join-Path $repoRoot $relativePath
    if (Test-Path -LiteralPath $fullPath) {
        $manifests += Get-FileMeasurement -Path $fullPath
    }
}

$databaseFiles = @()
$dataDirectory = Join-Path $repoRoot 'data'
if (Test-Path -LiteralPath $dataDirectory) {
    Get-ChildItem -LiteralPath $dataDirectory -File -Filter '*.db' | ForEach-Object {
        $rootUri = [System.Uri]::new(($repoRoot.TrimEnd('\') + '\'))
        $itemUri = [System.Uri]::new($_.FullName)
        $databaseFiles += [ordered]@{
            path = [System.Uri]::UnescapeDataString($rootUri.MakeRelativeUri($itemUri).ToString())
            bytes = $_.Length
        }
    }
}

$measurement = [ordered]@{
    schemaVersion = 1
    capturedAtUtc = $capturedAt
    repoRoot = $repoRoot
    commit = Get-GitCommit
    version = Get-CargoVersion
    deviceRole = 'pc'
    artifacts = $artifacts
    manifests = $manifests
    databaseFiles = $databaseFiles
    performance = [ordered]@{
        coldStartMs = 'not_measured'
        idleWorkingSetBytes = 'not_measured'
        idleCpuPercent = 'not_measured'
        reason = 'Requires a controlled installed-build run; no estimate is recorded.'
    }
}

New-Item -ItemType Directory -Path $OutputDirectory -Force | Out-Null
$timestamp = Get-Date -Format 'yyyyMMdd-HHmmss'
$outputPath = Join-Path $OutputDirectory "phase55-baseline-$timestamp.json"
$measurement | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $outputPath -Encoding UTF8
Write-Output $outputPath
