@echo off
REM Mystery Prize test — verifies fallback behavior when prizes are exhausted
setlocal enabledelayedexpansion

set "SCRIPT_DIR=%~dp0"
set "PROJECT_DIR=%SCRIPT_DIR%..\.."
set "SERVER_DIR=%PROJECT_DIR%\server"
set "BINARY=%PROJECT_DIR%\target\release\spinwin-server.exe"
set "DB_FILE=%SERVER_DIR%\test_mystery.db"
set "PORT=3096"
set "BASE=http://localhost:%PORT%"

echo ============================================
echo   MYSTERY PRIZE TEST
echo ============================================
echo.
echo   Verifies Mystery Prize fallback behavior
echo   when all other prizes are exhausted.
echo.

REM Build
echo   Building release binary...
cargo build --release --manifest-path "%PROJECT_DIR%\Cargo.toml" 2>&1 | findstr /v "Compiling"

REM Clean old DB and start server
if exist "%DB_FILE%" del /f "%DB_FILE%"
set "DATABASE_URL=sqlite:%DB_FILE%?mode=rwc"
set "BIND_ADDR=127.0.0.1:%PORT%"
set "GOOGLE_SHEET_ID=none"
set "SMTP_EMAIL="
set "SMTP_PASSWORD="
set "SPINWIN_SMALL_STOCK=1"
start /b "" "%BINARY%" > nul 2>&1
timeout /t 3 /nobreak > nul

curl -s "%BASE%/api/prizes" > nul 2>&1
if !errorlevel! neq 0 (
    echo   FAIL: Server failed to start
    exit /b 1
)
echo   Server running on port %PORT%
echo.

REM Run test via Python
python "%SCRIPT_DIR%\mystery_prize_test.py" "%BASE%"
set "TEST_RESULT=!errorlevel!"

REM Cleanup
for /f "tokens=5" %%a in ('netstat -aon ^| findstr ":%PORT% " ^| findstr "LISTENING"') do (
    taskkill /f /pid %%a > nul 2>&1
)
if exist "%DB_FILE%" del /f "%DB_FILE%"

exit /b !TEST_RESULT!
