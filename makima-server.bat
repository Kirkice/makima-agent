@echo off
chcp 65001 >nul 2>nul
title Makima Agent
cd /d "%~dp0"

:: Launch the Python launcher which manages server + chat
python launcher.py

:: If launcher fails, pause to show any error
if errorlevel 1 pause