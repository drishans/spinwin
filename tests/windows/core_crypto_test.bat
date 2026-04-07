@echo off
REM Core cryptographic tests — runs Rust unit tests in the core crate
setlocal enabledelayedexpansion

set "SCRIPT_DIR=%~dp0"
set "PROJECT_DIR=%SCRIPT_DIR%..\.."

echo ============================================
echo   CORE CRYPTO TESTS
echo ============================================
echo.
echo   Testing: Ed25519 signing, verification,
echo   tamper detection, wrong-key rejection,
echo   key serialization round-trip
echo.

cd /d "%PROJECT_DIR%"
cargo test -p spinwin-core -- --nocapture 2>&1
if !errorlevel! neq 0 (
    echo   CORE CRYPTO TESTS FAILED
    exit /b 1
)

echo.
echo   All core crypto tests passed.
exit /b 0
