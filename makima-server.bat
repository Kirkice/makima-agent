@echo off
chcp 65001 >nul 2>nul
title Makima Agent
cd /d "%~dp0"

:: Launch the Python launcher which manages server + chat
python launcher.py

:: Pause to show any output or error before closing
echo.
echo Press any key to exit...
pause >nul
