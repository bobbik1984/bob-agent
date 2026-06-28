$ErrorActionPreference = "Stop"

$repo = "bblanchon/pdfium-binaries"
Write-Host "Fetching latest release information..."
$releaseUrl = "https://api.github.com/repos/$repo/releases/latest"
$releaseInfo = Invoke-RestMethod -Uri $releaseUrl
$tag = $releaseInfo.tag_name

Write-Host "Latest tag: $tag"

$fileName = "pdfium-win-x64.tgz"
$downloadUrl = "https://github.com/$repo/releases/download/$tag/$fileName"

$targetDir = Join-Path $PSScriptRoot "..\src-tauri"
$tgzPath = Join-Path $targetDir $fileName

Write-Host "Downloading PDFium from $downloadUrl..."
Invoke-WebRequest -Uri $downloadUrl -OutFile $tgzPath

Write-Host "Extracting PDFium using tar..."
Set-Location -Path $targetDir
tar -xzf $fileName

Write-Host "Cleaning up archive..."
Remove-Item $tgzPath

# Move the dll to the root of src-tauri so it's easily bundled
$dllSource = Join-Path $targetDir "bin\pdfium.dll"
$dllDest = Join-Path $targetDir "pdfium.dll"

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
