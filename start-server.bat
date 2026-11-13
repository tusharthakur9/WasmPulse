@echo off
setlocal enabledelayedexpansion

title WasmPulse Serverless Engine
echo ==========================================================
echo Starting WasmPulse Serverless Engine on http://localhost:8080...
echo ==========================================================
echo.
cd /d "%~dp0"

where wsl >nul 2>nul
if %errorlevel%==0 (
    if exist "target\release\engine-gateway" (
        echo [INFO] WSL environment detected with precompiled release engine.
        echo [INFO] Starting engine via WSL (high-density pooling allocator active)...
        start http://localhost:8080
        wsl bash -c "cd /mnt/c/Users/HP/OneDrive/Desktop/WasmPulse-Serverless && ./target/release/engine-gateway"
        exit /b 0
    )
)

where cargo >nul 2>nul
if errorlevel 1 (
    echo [ERROR] Rust/Cargo was not found in Windows PATH.
    echo Tip: You can run 'run-in-wsl.bat' if you have WSL installed.
    echo Or install Rust from https://rustup.rs and reopen this terminal.
    echo.
    pause
    exit /b 1
)

where rustup >nul 2>nul
if errorlevel 1 (
    echo rustup was not found in PATH.
    echo Install Rust from https://rustup.rs and reopen this terminal.
    echo.
    pause
    exit /b 1
)

where link.exe >nul 2>nul
if errorlevel 1 (
    echo Missing MSVC linker: link.exe was not found.
    echo Tip: You can double-click 'run-in-wsl.bat' to run instantly via WSL!
    echo.
    pause
    exit /b 1
)

rustup target list --installed | findstr /C:"wasm32-wasip1" >nul
if errorlevel 1 (
    echo Installing required Rust target: wasm32-wasip1
    rustup target add wasm32-wasip1
    if errorlevel 1 (
        echo Failed to install wasm32-wasip1.
        pause
        exit /b 1
    )
)

if not exist "target\wasm32-wasip1\release" (
    mkdir "target\wasm32-wasip1\release"
)
if exist "sample-functions\rust_echo.wasm" (
    copy /y "sample-functions\rust_echo.wasm" "target\wasm32-wasip1\release\rust_echo.wasm" >nul
)

if not exist "target\release\engine-gateway.exe" (
    echo Building the WasmPulse gateway...
    cargo build --release -p engine-gateway
    if errorlevel 1 (
        echo.
        echo Build failed on Windows.
        echo NOTE: Windows 11 Smart App Control may block unsigned build scripts.
        echo Enable 'Developer Mode' in Windows Settings or run 'run-in-wsl.bat'.
        echo.
        pause
        exit /b 1
    )
)

if not exist "bin" mkdir "bin"
copy /y "target\release\engine-gateway.exe" "bin\engine-gateway.exe" >nul

start http://localhost:8080
"bin\engine-gateway.exe"

exit /b %errorlevel%
