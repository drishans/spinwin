@echo off
REM API integration tests — starts a fresh server, tests full flows, tears down
setlocal enabledelayedexpansion

set "SCRIPT_DIR=%~dp0"
set "PROJECT_DIR=%SCRIPT_DIR%..\.."
set "SERVER_DIR=%PROJECT_DIR%\server"
set "BINARY=%PROJECT_DIR%\target\release\spinwin-server.exe"
set "DB_FILE=%SERVER_DIR%\test_integration.db"
set "PORT=3099"
set "BASE=http://localhost:%PORT%"
set "PASSED=0"
set "FAILED=0"

echo ============================================
echo   API INTEGRATION TESTS
echo ============================================
echo.

REM Build
echo   Building release binary...
cargo build --release --manifest-path "%PROJECT_DIR%\Cargo.toml" 2>&1 | findstr /v "Compiling"

REM Clean old DB
if exist "%DB_FILE%" del /f "%DB_FILE%"

REM Start server
set "DATABASE_URL=sqlite:%DB_FILE%?mode=rwc"
set "BIND_ADDR=127.0.0.1:%PORT%"
start /b "" "%BINARY%" > nul 2>&1
timeout /t 3 /nobreak > nul

REM Check server is running
curl -s "%BASE%/api/prizes" > nul 2>&1
if !errorlevel! neq 0 (
    echo   FAIL: Server failed to start
    exit /b 1
)
echo   Server running on port %PORT%
echo.

REM Run the test logic via Python
python "%SCRIPT_DIR%\api_integration_test.py" "%BASE%"
set "TEST_RESULT=!errorlevel!"

REM Cleanup: kill the server
for /f "tokens=5" %%a in ('netstat -aon ^| findstr ":%PORT% " ^| findstr "LISTENING"') do (
    taskkill /f /pid %%a > nul 2>&1
)
if exist "%DB_FILE%" del /f "%DB_FILE%"

exit /b !TEST_RESULT!
