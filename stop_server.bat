@echo off
chcp 65001 >nul
echo ========================================
echo   Makima Agent - Stopping Server
echo ========================================
echo.

echo [INFO] Stopping Python processes...
taskkill /F /IM python.exe 2>nul
if %errorlevel% equ 0 (
    echo [OK] Server stopped successfully
) else (
    echo [INFO] No Python process found or already stopped
)

echo.
echo [INFO] Stopping uvicorn processes...
taskkill /F /IM uvicorn.exe 2>nul
if %errorlevel% equ 0 (
    echo [OK] Uvicorn stopped successfully
) else (
    echo [INFO] No uvicorn process found or already stopped
)

echo.
echo ========================================
echo   Server shutdown complete
echo ========================================
pause