$ErrorActionPreference = "Stop"

$repo = "bblanchon/pdfium-binaries"
$fileName = "pdfium-win-x64.tgz"
$downloadUrl = "https://github.com/$repo/releases/latest/download/$fileName"

$targetDir = Join-Path $PSScriptRoot "..\src-tauri"
$tgzPath = Join-Path $targetDir $fileName
$dllDest = Join-Path $targetDir "pdfium.dll"

if (Test-Path $dllDest) {
    Write-Host "pdfium.dll already exists at $dllDest. Skipping download."
    exit 0
}

Write-Host "Downloading PDFium from $downloadUrl..."
$downloaded = $false
for ($i = 1; $i -le 5; $i++) {
    try {
        if (Get-Command curl.exe -ErrorAction SilentlyContinue) {
            curl.exe -L --retry 5 --retry-delay 3 -sSf -o $tgzPath $downloadUrl
            if ((Test-Path $tgzPath) -and ((Get-Item $tgzPath).Length -gt 1048576)) {
                $downloaded = $true
                break
            }
        }
        Invoke-WebRequest -Uri $downloadUrl -OutFile $tgzPath -TimeoutSec 60
        if ((Test-Path $tgzPath) -and ((Get-Item $tgzPath).Length -gt 1048576)) {
            $downloaded = $true
            break
        }
    } catch {
        Write-Host "Attempt $i failed: $_. Retrying in 3 seconds..."
        Start-Sleep -Seconds 3
    }
}

if (-not $downloaded) {
    Write-Error "Failed to download PDFium after 5 attempts."
    exit 1
}

Write-Host "Extracting PDFium using tar..."
Set-Location -Path $targetDir
tar -xzf $fileName

Write-Host "Cleaning up archive..."
if (Test-Path $tgzPath) { Remove-Item $tgzPath -Force }

# Move the dll to the root of src-tauri so it's easily bundled
$dllSource = Join-Path $targetDir "bin\pdfium.dll"

if (Test-Path $dllSource) {
    Move-Item -Path $dllSource -Destination $dllDest -Force
    Write-Host "Successfully installed pdfium.dll into $targetDir"
} else {
    Write-Host "Warning: pdfium.dll not found in extracted files."
}

# Clean up extracted folders we don't need
$foldersToClean = @("bin", "lib", "include", "args.gn")
foreach ($f in $foldersToClean) {
    $p = Join-Path $targetDir $f
    if (Test-Path $p) { Remove-Item -Path $p -Recurse -Force }
}
