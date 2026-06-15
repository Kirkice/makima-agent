@echo off
title Makima Agent Chat
cd /d "%~dp0"

echo.
echo ========================================
echo   Makima Agent Chat
echo ========================================
echo.
echo Checking server...

python -c "import urllib.request; urllib.request.urlopen('http://localhost:8000/health')" 2>nul

if errorlevel 1 (
    echo.
    echo [ERROR] Server not running!
    echo.
    echo Please start the server first:
    echo   1. Double-click start_server.bat
    echo   2. Wait for startup to complete
    echo   3. Then run this script again
    echo.
    pause
    exit /b 1
)

echo [OK] Server connected
echo.

python cli.py --server http://localhost:8000

echo.
pause