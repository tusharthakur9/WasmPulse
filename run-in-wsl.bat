@echo off
setlocal
title WasmPulse Serverless Engine (WSL Mode)
echo ======================================================================
echo   WasmPulse Serverless Engine - High-Performance Execution (WSL)
echo ======================================================================
echo.
cd /d "%~dp0"

where wsl >nul 2>nul
if errorlevel 1 (
    echo [ERROR] WSL (Windows Subsystem for Linux) was not found.
    echo Please run via standard Windows PowerShell or install WSL with: wsl --install
    pause
    exit /b 1
)

echo [1/2] Checking precompiled Linux binaries and sample microservices...
wsl bash -c "cd /mnt/c/Users/HP/OneDrive/Desktop/WasmPulse-Serverless && if [ ! -f target/release/engine-gateway ]; then cargo build --release -p engine-gateway; fi"

echo [2/2] Launching Web Console and Gateway on http://localhost:8080...
start http://localhost:8080

echo.
echo Engine running at http://localhost:8080
echo Press Ctrl+C in this window to stop the server.
echo.

wsl bash -c "cd /mnt/c/Users/HP/OneDrive/Desktop/WasmPulse-Serverless && ./target/release/engine-gateway"

pause
