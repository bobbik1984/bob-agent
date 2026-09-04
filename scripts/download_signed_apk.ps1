# PowerShell script to download the latest signed Bob Mobile APK and auto-install via ADB
param(
    [switch]$NoWait,
    [switch]$NoInstall,
    [string]$RunId
)

[Console]::OutputEncoding = [System.Text.Encoding]::UTF8

$repo = "bobbik1984/bob-agent"
$tag = "latest-mobile"
$outputDir = Join-Path $PSScriptRoot "..\dist-release"
if (-not (Test-Path $outputDir)) {
    New-Item -ItemType Directory -Path $outputDir -Force | Out-Null
}

$targetFile = Join-Path $outputDir "bob-mobile-latest.apk"
$downloadUrl = "https://github.com/$repo/releases/download/$tag/bob-mobile-latest.apk"

Write-Host "===========================================================" -ForegroundColor Cyan
Write-Host "  Bob Mobile Android APK 同步与真机部署工具               " -ForegroundColor Cyan
Write-Host "===========================================================" -ForegroundColor Cyan

# 1. 检查 GitHub Actions 云端构建状态 (如果正在构建，则自动等待)
if (-not $NoWait) {
    try {
        $apiRunsUrl = "https://api.github.com/repos/$repo/actions/workflows/android.yml/runs?per_page=5"
        $runsResp = Invoke-RestMethod -Uri $apiRunsUrl -UseBasicParsing
        $targetRun = $null

        if ($RunId) {
            $targetRun = $runsResp.workflow_runs | Where-Object { $_.id -eq [int64]$RunId }
        } else {
            # 找到最新的一个运行记录
            $targetRun = $runsResp.workflow_runs | Select-Object -First 1
        }

        if ($targetRun -and ($targetRun.status -in @("in_progress", "queued", "waiting"))) {
            Write-Host "[CI 监控] 发现正在进行的云端构建 (Run ID: $($targetRun.id))" -ForegroundColor Cyan
            Write-Host "         分支: $($targetRun.head_branch) | 提交: $($targetRun.head_sha.Substring(0,7)) | 事件: $($targetRun.event)" -ForegroundColor Gray
            Write-Host "         构建任务进行中，正在等待编译与签名完成..." -ForegroundColor Yellow

            $startTime = Get-Date
            while ($true) {
                Start-Sleep -Seconds 12
                try {
                    $runDetail = Invoke-RestMethod -Uri "https://api.github.com/repos/$repo/actions/runs/$($targetRun.id)" -UseBasicParsing
                } catch {
                    Start-Sleep -Seconds 5
                    continue
                }
                
                $elapsed = [math]::Round(((Get-Date) - $startTime).TotalSeconds)
                
                # 获取当前正在执行的步骤名称
                $stepName = "编译中"
                try {
                    $jobsResp = Invoke-RestMethod -Uri "https://api.github.com/repos/$repo/actions/runs/$($targetRun.id)/jobs" -UseBasicParsing
                    $activeStep = $jobsResp.jobs[0].steps | Where-Object { $_.status -eq 'in_progress' } | Select-Object -Last 1
                    if ($activeStep) { $stepName = $activeStep.name }
                } catch {}

                Write-Host "         [+${elapsed}s] 状态: $($runDetail.status) | 当前阶段: $stepName" -ForegroundColor DarkGray

                if ($runDetail.status -eq 'completed') {
                    if ($runDetail.conclusion -eq 'success') {
                        Write-Host "`n[OK] 云端编译、4KB 对齐与 V2/V3 签名全部完成！" -ForegroundColor Green
                        Start-Sleep -Seconds 3
                    } else {
                        Write-Host "`n[FAIL] 云端构建未成功！结论: $($runDetail.conclusion)" -ForegroundColor Red
                        Write-Host "详情请访问: $($runDetail.html_url)" -ForegroundColor Yellow
                        exit 1
                    }
                    break
                }
            }
        }
    } catch {
        Write-Host "[WARN] 无法查询 GitHub Actions API 状态 ($_)，直接尝试拉取最新 Release..." -ForegroundColor DarkYellow
    }
}

# 2. 下载最新已签名 APK
Write-Host "`n[下载] 正在从 GitHub Releases 拉取最新 APK..." -ForegroundColor Cyan
Write-Host "URL: $downloadUrl" -ForegroundColor Gray
Write-Host "目标文件: $targetFile" -ForegroundColor Gray

$curl = Get-Command curl.exe -ErrorAction SilentlyContinue
$downloadSuccess = $false

if ($curl) {
    Write-Host "使用 curl.exe 快速流式下载..." -ForegroundColor DarkGray
    & curl.exe -L --progress-bar -f -o "$targetFile" "$downloadUrl"
    if ($LASTEXITCODE -eq 0 -and (Test-Path $targetFile)) {
        $downloadSuccess = $true
    }
}

if (-not $downloadSuccess) {
    try {
        Write-Host "使用 .NET WebClient 下载..." -ForegroundColor DarkGray
        $webClient = New-Object System.Net.WebClient
        $webClient.DownloadFile($downloadUrl, $targetFile)
        $downloadSuccess = $true
    } catch {
        Write-Host "[WARN] WebClient 异常: $_. 尝试 Invoke-WebRequest..." -ForegroundColor DarkYellow
        $ProgressPreference = 'SilentlyContinue'
        Invoke-WebRequest -Uri $downloadUrl -OutFile $targetFile -UseBasicParsing
        $downloadSuccess = (Test-Path $targetFile)
    }
}

if ($downloadSuccess -and (Test-Path $targetFile)) {
    $sizeMb = [math]::Round((Get-Item $targetFile).Length / 1MB, 2)
    Write-Host "`n[OK] 成功下载最新已签名 APK ($sizeMb MB)!" -ForegroundColor Green
    Write-Host "文件位置: $targetFile" -ForegroundColor White
} else {
    Write-Host "`n[FAIL] 下载失败！" -ForegroundColor Red
    Write-Host "请确保 GitHub Actions 构建已完成并生成了 latest-mobile Release。" -ForegroundColor Yellow
    exit 1
}

# 3. 自动检测 ADB 并推送到真机
if (-not $NoInstall) {
    $candidates = @(
        "D:\OneDrive\Software\Pixel\platform-tools\adb.exe",
        "$env:LOCALAPPDATA\Android\Sdk\platform-tools\adb.exe",
        "$env:ANDROID_HOME\platform-tools\adb.exe",
        (Get-Command adb -ErrorAction SilentlyContinue).Source
    )

    $adb = $null
    foreach ($cand in $candidates) {
        if ($cand -and (Test-Path $cand)) {
            $adb = $cand
            break
        }
    }

    if ($adb) {
        Write-Host "`n[ADB] 发现 ADB: $adb" -ForegroundColor Gray
        $devices = & $adb devices | Select-String "device$"
        if ($devices) {
            Write-Host "[OK] 检测到已连接的 Android 设备:" -ForegroundColor Green
            $devices | ForEach-Object { Write-Host "     $_" -ForegroundColor Green }
            
            Write-Host "正在推送到手机并覆盖安装 (-r)..." -ForegroundColor Yellow
            $installOut = & $adb install -r $targetFile 2>&1
            Write-Host ($installOut -join "`n") -ForegroundColor Gray

            if ($installOut -match "Success") {
                Write-Host "[OK] 手机端 APK 安装成功！" -ForegroundColor Green
                Write-Host "正在唤醒启动应用 (MainActivity)..." -ForegroundColor Cyan
                & $adb shell am start -n bob.agent/.MainActivity > $null
                Write-Host "[OK] Bob Mobile 已在前台启动！" -ForegroundColor Green
                Write-Host "`n[提示] 如需观察真机实时运行或闪退诊断，可运行: powershell -File scripts\diagnose_crash.ps1" -ForegroundColor Cyan
            } else {
                Write-Host "[WARN] ADB 安装返回异常，请确认手机已解锁并允许安装更新。" -ForegroundColor Yellow
            }
        } else {
            Write-Host "`n[INFO] 未检测到 USB/无线连接的 Android 设备。" -ForegroundColor Yellow
            Write-Host "       APK 已保存在: $targetFile" -ForegroundColor Gray
            Write-Host "       插上手机后可直接运行: & '$adb' install -r '$targetFile'" -ForegroundColor Gray
        }
    } else {
        Write-Host "`n[INFO] 未检测到 adb.exe。APK 已就绪: $targetFile" -ForegroundColor Gray
    }
}

Write-Host "===========================================================" -ForegroundColor Cyan
