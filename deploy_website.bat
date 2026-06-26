@echo off
title Deploy bob-agent Website
echo ========================================
echo   Deploying bob-agent Website (Local Sync)
echo ========================================
echo.

cd /d "%~dp0"

echo [1/2] Checking target directory...
set "TARGET_DIR=..\Assistant\common\bob-website"
if not exist "%TARGET_DIR%" (
    echo [FAIL] Target directory %TARGET_DIR% not found!
    pause
    exit /b 1
)

echo [2/2] Copying files to Assistant/common/bob-website...
xcopy "website\*" "%TARGET_DIR%\" /E /Y /C /H
if %ERRORLEVEL% neq 0 (
    echo [FAIL] Copy failed!
    pause
    exit /b 1
)

echo.
echo ========================================
echo   DONE! Files copied to common\bob-website.
echo   Syncthing will automatically push them to VPS1.
echo   Visit https://bob.bobbik.org shortly.
echo ========================================
echo.
pause
