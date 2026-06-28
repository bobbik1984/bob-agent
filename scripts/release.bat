@echo off
setlocal enabledelayedexpansion

:: ===========================================================
:: Bob Agent Release Builder
::
:: Outputs:
::   dist-release\bob-installer.exe        (Installer)
::   dist-release\bob-agent-portable.zip   (Portable)
::
:: Usage: scripts\release.bat
:: ===========================================================

set "ROOT=%~dp0.."
set "DIST_DIR=%ROOT%\dist-release"

echo.
echo  ========================================
echo    Bob Agent Release Builder
echo  ========================================
echo.

:: -- Step 1: Build main app (Release) ----------------------
echo [1/6] Building main app (npm run tauri build)...
cd /d "%ROOT%"
call npm run tauri build
if errorlevel 1 (
    echo [FAIL] Main app build failed!
    pause
    exit /b 1
)
echo [OK] Main app built.

:: -- Step 2: Generate Payload ------------------------------
echo.
echo [2/6] Generating payload (node scripts/build_payload.mjs)...
call node scripts/build_payload.mjs
if errorlevel 1 (
    echo [FAIL] Payload generation failed!
    pause
    exit /b 1
)
echo [OK] Payload generated.

:: -- Step 3: Copy payload to installer project -------------
echo.
echo [3/6] Syncing payload to installer...
copy /y "%ROOT%\payload.zip" "%ROOT%\installer\src-tauri\payload.zip" >nul
echo [OK] Payload synced.

:: -- Step 4: Build installer -------------------------------
echo.
echo [4/6] Building installer (installer\npm run tauri build)...
cd /d "%ROOT%\installer"
call npm run tauri build
if errorlevel 1 (
    echo [FAIL] Installer build failed!
    pause
    exit /b 1
)
echo [OK] Installer built.

:: -- Step 5: Collect artifacts to dist-release -------------
echo.
echo [5/6] Collecting release artifacts...
cd /d "%ROOT%"
if not exist "%DIST_DIR%" mkdir "%DIST_DIR%"

copy /y "%ROOT%\installer\src-tauri\target\release\bob-installer.exe" "%DIST_DIR%\bob-installer.exe" >nul
copy /y "%ROOT%\payload.zip" "%DIST_DIR%\bob-agent-portable.zip" >nul
echo [OK] Artifacts collected to dist-release\

:: -- Step 6: Clean up intermediates ------------------------
echo.
echo [6/6] Cleaning up intermediate files...
del /q "%ROOT%\payload.zip" 2>nul
del /q "%ROOT%\installer\src-tauri\payload.zip" 2>nul
if exist "%ROOT%\src-tauri\target\release\bundle" rd /s /q "%ROOT%\src-tauri\target\release\bundle" 2>nul
if exist "%ROOT%\installer\src-tauri\target\release\bundle" rd /s /q "%ROOT%\installer\src-tauri\target\release\bundle" 2>nul
echo [OK] Intermediates cleaned.

:: -- Done --------------------------------------------------
echo.
echo  ========================================
echo    Build complete!
echo  ========================================
echo.
echo  Installer:  dist-release\bob-installer.exe
echo  Portable:   dist-release\bob-agent-portable.zip
echo.

explorer "%DIST_DIR%"
pause
