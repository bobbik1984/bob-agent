# PowerShell 自动化 Android 崩溃诊断工具 (10秒捕获真实堆栈)
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8

Write-Host "=====================================================" -ForegroundColor Cyan
Write-Host "   Bob Mobile Android 实时闪退诊断与日志抓取工具   " -ForegroundColor Cyan
Write-Host "=====================================================" -ForegroundColor Cyan

# 1. 自动寻找 ADB 工具
$candidates = @(
    "D:\OneDrive\Software\Pixel\platform-tools\adb.exe",
    "$env:LOCALAPPDATA\Android\Sdk\platform-tools\adb.exe",
    (Get-Command adb -ErrorAction SilentlyContinue).Source
)

$adb = $null
foreach ($cand in $candidates) {
    if ($cand -and (Test-Path $cand)) {
        $adb = $cand
        break
    }
}

if (-not $adb) {
    Write-Host "[FAIL] 未找到 adb.exe，请确保手机驱动和 platform-tools 已安装。" -ForegroundColor Red
    exit 1
}

Write-Host "[1/4] 使用 ADB: $adb" -ForegroundColor Gray

# 2. 检查设备连接
$devices = & $adb devices | Select-String "device$"
if (-not $devices) {
    Write-Host "[FAIL] 未检测到任何已授权的 Android 设备，请检查 USB 连接或无线调试授权。" -ForegroundColor Red
    exit 1
}
Write-Host "[2/4] 检测到已连接设备:" -ForegroundColor Green
$devices | ForEach-Object { Write-Host "      $_" -ForegroundColor Green }

# 3. 清空旧缓冲区并触发启动
Write-Host "[3/4] 清空旧日志缓冲区，正在唤醒应用..." -ForegroundColor Yellow
& $adb logcat -c
Start-Sleep -Milliseconds 300

& $adb shell am start -n bob.agent/.MainActivity > $null
Start-Sleep -Milliseconds 1200

# 4. 抓取崩溃堆栈
Write-Host "[4/4] 抓取致命崩溃与 Panic 堆栈..." -ForegroundColor Yellow
Write-Host "-----------------------------------------------------" -ForegroundColor DarkGray

$crash = & $adb logcat -b crash -d
$rustLogs = & $adb logcat -d | Select-String "RustStdoutStderr|FATAL|SqliteFailure|SIGABRT|UnsatisfiedLinkError"

if ($crash -or $rustLogs) {
    Write-Host "[WARN] 发现崩溃记录！核心错误详情如下：`n" -ForegroundColor Red
    if ($rustLogs) {
        Write-Host "--- [Rust / JNI 日志] ---" -ForegroundColor Yellow
        $rustLogs | ForEach-Object { Write-Host $_ -ForegroundColor Yellow }
    }
    if ($crash) {
        Write-Host "`n--- [系统 Crash 缓冲区] ---" -ForegroundColor Magenta
        Write-Host $crash -ForegroundColor Magenta
    }
} else {
    Write-Host "[OK] 未捕获到崩溃信号，应用当前运行正常！" -ForegroundColor Green
}
Write-Host "-----------------------------------------------------" -ForegroundColor DarkGray
