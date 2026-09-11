@echo off
chcp 65001 >nul
setlocal enabledelayedexpansion

:: ===========================================================
:: Bob Agent 统一发布与打包管理中心 (Unified Release Builder)
::
:: 支持目标:
::   1. 全套打包: PC 安装包 + 便携包 + 安卓 APK 同步及 ADB 安装
::   2. 仅打包 PC 版: bob-installer.exe + bob-agent-portable.zip
::   3. 仅打包/同步 安卓 APK: 云端 CI 同步 + 本地 ADB 直装
::   4. 运行安卓真机闪退诊断: 捕获真机 Crash 与 Rust 日志
::
:: 命令行使用:
::   scripts\release.bat          (弹出交互菜单)
::   scripts\release.bat --all    (或 -a, 1: 全套打包)
::   scripts\release.bat --pc     (或 -p, 2: 仅打包 PC 版)
::   scripts\release.bat --apk    (或 -m, 3: 仅打包/同步 安卓 APK)
::   scripts\release.bat --diag   (或 -d, 4: 真机诊断)
:: ===========================================================

set "ROOT=%~dp0.."
set "DIST_DIR=%ROOT%\dist-release"

:: CLI 参数解析
set "ARG=%~1"
if "%ARG%"=="" goto MENU
if /i "%ARG%"=="--all" goto DO_ALL
if /i "%ARG%"=="-a"    goto DO_ALL
if /i "%ARG%"=="1"     goto DO_ALL
if /i "%ARG%"=="--pc"  goto DO_PC
if /i "%ARG%"=="-p"    goto DO_PC
if /i "%ARG%"=="2"     goto DO_PC
if /i "%ARG%"=="--apk" goto DO_APK
if /i "%ARG%"=="-m"    goto DO_APK
if /i "%ARG%"=="3"     goto DO_APK
if /i "%ARG%"=="--publish" goto DO_PUBLISH
if /i "%ARG%"=="-pub"      goto DO_PUBLISH
if /i "%ARG%"=="5"          goto DO_PUBLISH
if /i "%ARG%"=="--pull"    goto DO_PULL
if /i "%ARG%"=="-pl"       goto DO_PULL
if /i "%ARG%"=="6"         goto DO_PULL
if /i "%ARG%"=="--diag" goto DO_DIAG
if /i "%ARG%"=="-d"     goto DO_DIAG
if /i "%ARG%"=="4"      goto DO_DIAG
if /i "%ARG%"=="--help" goto USAGE
if /i "%ARG%"=="-h"     goto USAGE

:USAGE
echo.
echo 用法: scripts\release.bat [选项]
echo.
echo 选项:
echo   --all, -a, 1    全套打包 (PC 安装包/便携包 + 安卓 APK 同步与真机安装)
echo   --pc,  -p, 2    仅打包 PC 版 (bob-installer.exe + bob-agent-portable.zip)
echo   --apk, -m, 3    仅同步/安装安卓版 (下载最新签名 APK + ADB 直装)
echo   --diag, -d, 4   运行真机闪退与 Rust 日志诊断
echo   --publish, -pub, 5 正式全渠道发版 (PC编译 + 安卓同步 + 官网部署 + GitHub Release)
echo   --pull, -pl, 6  从云端同步全平台产物并分发官网 (下载带版本号产物 + 官网改名同步)
echo   --help, -h      显示本帮助信息
echo.
exit /b 0

:MENU
cls
echo ===========================================================
echo    Bob Agent 统一发布与打包管理中心 (Unified Release)
echo ===========================================================
echo.
echo   [1] 全套打包 (Full Suite)
echo       - 编译 PC 安装包 (bob-installer.exe)
echo       - 编译 PC 便携包 (bob-agent-portable.zip)
echo       - 同步云端已签名 Android APK (bob-mobile-latest.apk)
echo       - 若手机已连接电脑，自动通过 ADB 安装并启动
echo.
echo   [2] 仅打包 PC 桌面版 (PC Desktop Only)
echo       - 本地编译主应用与安装器，生成 Installer 与 Portable 包
echo.
echo   [3] 仅打包/同步 安卓版 APK (Android APK Only)
echo       - 检查 GitHub Actions CI 进度（支持自动等待完成）
echo       - 下载最新已签名 APK，并通过 ADB 直装到手机
echo.
echo   [4] 运行安卓真机闪退诊断 (ADB Crash Diagnostic)
echo       - 捕获真机 Crash、SIGABRT 与 Rust 运行时日志
echo.
echo   [5] 正式全渠道发版 (All-in-One Full Release)
echo       - 编译 PC 版 + 同步安卓 APK + 部署官网 (bob.bobbik.org) + 发布 GitHub Release
echo.
echo   [6] 从云端拉取全平台产物并分发官网 (Pull Cloud CI & Deploy to Website)
echo       - 从 GitHub Releases 下载最新带版本号的 PC 安装包/便携包与安卓 APK
echo       - 存入本地 dist-release 并标准化重命名推送到官网 VPS1
echo.
echo   [Q] 退出 (Exit)
echo.
echo ===========================================================
set /p "CHOICE=请输入选项 [1-6, Q] (默认 1): "
if "%CHOICE%"=="" set "CHOICE=1"
if /i "%CHOICE%"=="1" goto DO_ALL
if /i "%CHOICE%"=="2" goto DO_PC
if /i "%CHOICE%"=="3" goto DO_APK
if /i "%CHOICE%"=="4" goto DO_DIAG
if /i "%CHOICE%"=="5" goto DO_PUBLISH
if /i "%CHOICE%"=="6" goto DO_PULL
if /i "%CHOICE%"=="q" exit /b 0
echo 无效选项: %CHOICE%
pause
goto MENU

:: ===========================================================
:: 全套打包 (Full Suite)
:: ===========================================================
:DO_ALL
echo.
echo ===========================================================
echo   [模式 1] 开始全套打包 (PC 版 + 安卓 APK 同步)
echo ===========================================================
call :BUILD_PC_ROUTINE
if errorlevel 1 goto FAIL

call :SYNC_APK_ROUTINE
if errorlevel 1 goto FAIL

goto FINISH_ALL

:: ===========================================================
:: ===========================================================
:: 正式全渠道发版 (All-in-One Full Release)
:: ===========================================================
:DO_PUBLISH
echo.
echo ===========================================================
echo   [模式 5] 正式全渠道发版 (PC编译 + 安卓同步 + 官网同步 + GitHub Release)
echo ===========================================================
call :BUILD_PC_ROUTINE
if errorlevel 1 goto FAIL

call :SYNC_APK_ROUTINE
if errorlevel 1 goto FAIL

echo.
echo ==> [PUBLISH 1/2] 同步构建产物至官网与下载中心 (bob.bobbik.org)...
python "%ROOT%\website\sync_deploy.py"
if errorlevel 1 (
    echo [FAIL] 官网同步失败！
    goto FAIL
)

echo.
echo ==> [PUBLISH 2/2] 上传全平台产物至 GitHub Release...
python "%ROOT%\scripts\upload_github_release.py"
if errorlevel 1 (
    echo [FAIL] GitHub Release 上传失败！
    goto FAIL
)

goto FINISH_PUBLISH

:: ===========================================================
:: 从云端同步全平台产物并分发官网 (Pull Cloud CI)
:: ===========================================================
:DO_PULL
echo.
echo ===========================================================
echo   [模式 6] 从云端拉取最新带版本号产物并同步官网
echo ===========================================================
python "%ROOT%\scripts\pull_cloud_artifacts.py"
if errorlevel 1 goto FAIL
goto FINISH_PULL

:: 仅打包 PC 版 (PC Only)
:: ===========================================================
:DO_PC
echo.
echo ===========================================================
echo   [模式 2] 仅打包 PC 桌面版
echo ===========================================================
call :BUILD_PC_ROUTINE
if errorlevel 1 goto FAIL

goto FINISH_PC

:: ===========================================================
:: 仅同步/安装安卓 APK (APK Only)
:: ===========================================================
:DO_APK
echo.
echo ===========================================================
echo   [模式 3] 仅同步/安装安卓 APK
echo ===========================================================
call :SYNC_APK_ROUTINE
if errorlevel 1 goto FAIL

goto FINISH_APK

:: ===========================================================
:: 真机诊断 (Diagnostic)
:: ===========================================================
:DO_DIAG
echo.
echo ===========================================================
echo   [模式 4] 运行安卓真机闪退诊断
echo ===========================================================
powershell -NoProfile -ExecutionPolicy Bypass -File "%ROOT%\scripts\diagnose_crash.ps1"
if /I not "%BOB_RELEASE_NONINTERACTIVE%"=="1" pause
exit /b 0

:: ===========================================================
:: 子程序: 编译 PC 版本
:: ===========================================================
:BUILD_PC_ROUTINE
echo.
echo ==> [PC 1/6] 编译主应用 (pnpm run tauri build)...
cd /d "%ROOT%"
call pnpm run tauri build
if errorlevel 1 (
    echo [FAIL] PC 主应用编译失败！
    exit /b 1
)
echo [OK] PC 主应用编译完成。

echo.
echo ==> [PC 2/6] 生成安装 Payload (node scripts/build_payload.mjs)...
call node scripts/build_payload.mjs
if errorlevel 1 (
    echo [FAIL] Payload 生成失败！
    exit /b 1
)
echo [OK] Payload 生成成功。

echo.
echo ==> [PC 3/6] 同步 Payload 至安装器工程...
copy /y "%ROOT%\payload.zip" "%ROOT%\installer\src-tauri\payload.zip" >nul
echo [OK] Payload 已同步。

echo.
echo ==> [PC 4/6] 编译轻量级安装器 (installer\pnpm run tauri build)...
cd /d "%ROOT%\installer"
call pnpm run tauri build
if errorlevel 1 (
    echo [FAIL] 安装器编译失败！
    exit /b 1
)
echo [OK] 安装器编译完成。

echo.
echo ==> [PC 5/6] 归档产物至 dist-release\...
cd /d "%ROOT%"
if not exist "%DIST_DIR%" mkdir "%DIST_DIR%"
copy /y "%ROOT%\installer\src-tauri\target\release\bob-installer.exe" "%DIST_DIR%\bob-installer.exe" >nul
copy /y "%ROOT%\payload.zip" "%DIST_DIR%\bob-agent-portable.zip" >nul
echo [OK] PC 产物已归档:
echo      - %DIST_DIR%\bob-installer.exe
echo      - %DIST_DIR%\bob-agent-portable.zip

echo.
echo ==> [PC 6/6] 清理中间临时文件...
del /q "%ROOT%\payload.zip" 2>nul
del /q "%ROOT%\installer\src-tauri\payload.zip" 2>nul
if exist "%ROOT%\src-tauri\target\release\bundle" rd /s /q "%ROOT%\src-tauri\target\release\bundle" 2>nul
if exist "%ROOT%\installer\src-tauri\target\release\bundle" rd /s /q "%ROOT%\installer\src-tauri\target\release\bundle" 2>nul
echo [OK] 临时文件清理完毕。
exit /b 0

:: ===========================================================
:: 子程序: 同步安卓 APK
:: ===========================================================
:SYNC_APK_ROUTINE
echo.
echo ==> [APK] 检查云端 CI 状态并拉取最新已签名 APK...
powershell -NoProfile -ExecutionPolicy Bypass -File "%ROOT%\scripts\download_signed_apk.ps1"
if errorlevel 1 (
    echo [FAIL] 安卓 APK 同步或安装失败！
    exit /b 1
)
exit /b 0

:: ===========================================================
:FINISH_PUBLISH
echo.
echo ===========================================================
echo    全渠道发版完成 (Full Release Completed)!
echo ===========================================================
echo.
echo  PC 安装包:    dist-release\bob-installer.exe
echo  PC 便携版:    dist-release\bob-agent-portable.zip
echo  安卓 APK:     dist-release\bob-mobile-latest.apk
echo  官方站点:     https://bob.bobbik.org
echo  Release页面:  https://github.com/bobbik1984/bob-agent/releases/tag/v0.9.5
echo.
goto SHOW_EXPLORER

:FINISH_ALL
echo.
echo ===========================================================
echo    全套打包完成 (Full Suite Build Completed)!
echo ===========================================================
echo.
echo  PC 安装包:    dist-release\bob-installer.exe
echo  PC 便携版:    dist-release\bob-agent-portable.zip
echo  安卓 APK:     dist-release\bob-mobile-latest.apk
echo.
goto SHOW_EXPLORER

:FINISH_PC
echo.
echo ===========================================================
echo    PC 版打包完成!
echo ===========================================================
echo.
echo  PC 安装包:    dist-release\bob-installer.exe
echo  PC 便携版:    dist-release\bob-agent-portable.zip
echo.
goto SHOW_EXPLORER

:FINISH_APK
echo.
echo ===========================================================
echo    安卓 APK 同步完成!
echo ===========================================================
echo.
echo  安卓 APK:     dist-release\bob-mobile-latest.apk
echo.
goto SHOW_EXPLORER

:FINISH_PULL
echo.
echo ===========================================================
echo    云端产物同步与官网分发完成!
echo ===========================================================
echo.
echo  本地目录:   dist-release\ (保留带版本号的安装包与便携包)
echo  官网地址:   https://bob.bobbik.org (标准化统一命名直链)
echo.
goto SHOW_EXPLORER

:SHOW_EXPLORER
if /I not "%BOB_RELEASE_NONINTERACTIVE%"=="1" (
    explorer "%DIST_DIR%"
    pause
)
exit /b 0

:FAIL
echo.
echo ===========================================================
echo   [错误] 打包流程异常终止，请检查上方日志输出。
echo ===========================================================
if /I not "%BOB_RELEASE_NONINTERACTIVE%"=="1" pause
exit /b 1
