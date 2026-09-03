# PowerShell script to download the latest signed Bob Mobile APK
$repo = "bobbik1984/bob-agent"
$tag = "latest-mobile"
$outputDir = Join-Path $PSScriptRoot "..\dist-release"
if (-not (Test-Path $outputDir)) {
    New-Item -ItemType Directory -Path $outputDir -Force | Out-Null
}

$targetFile = Join-Path $outputDir "bob-mobile-latest.apk"
$downloadUrl = "https://github.com/$repo/releases/download/$tag/bob-mobile-latest.apk"

Write-Host "=========================================" -ForegroundColor Cyan
Write-Host " Fetching Latest Signed Bob Mobile APK   " -ForegroundColor Cyan
Write-Host "=========================================" -ForegroundColor Cyan
Write-Host "Target URL: $downloadUrl" -ForegroundColor Gray
Write-Host "Downloading to: $targetFile..." -ForegroundColor Yellow

try {
    Invoke-WebRequest -Uri $downloadUrl -OutFile $targetFile -UseBasicParsing
    $sizeMb = [math]::Round((Get-Item $targetFile).Length / 1MB, 2)
    Write-Host "`n[OK] 成功下载最新已签名 APK ($sizeMb MB)!" -ForegroundColor Green
    Write-Host "文件位置: $targetFile" -ForegroundColor White
    
    $adb = Get-Command adb -ErrorAction SilentlyContinue
    if ($adb) {
        $devices = & adb devices | Select-String "device$"
        if ($devices) {
            Write-Host "`n检测到已连接的 Android 设备，正在自动无线/有线安装..." -ForegroundColor Cyan
            & adb install -r $targetFile
            Write-Host "[OK] 安装完成！" -ForegroundColor Green
        }
    }
} catch {
    Write-Host "`n[FAIL] 下载失败: $_" -ForegroundColor Red
    Write-Host "请确保 GitHub Actions 构建已完成并生成了 latest-mobile Release。" -ForegroundColor Yellow
}
