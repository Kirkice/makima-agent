@echo off
chcp 65001 >nul
echo ========================================
echo   Makima Agent - Starting Server
echo ========================================
echo.

cd apps\backend
echo [INFO] Starting FastAPI server on http://localhost:8000
echo [INFO] API Documentation: http://localhost:8000/docs
echo [INFO] Press Ctrl+C to stop the server
echo.

python -m uvicorn makima.app:app --host 0.0.0.0 --port 8000 --reload

pause