@echo off
REM Run all Spin & Win tests on Windows
setlocal enabledelayedexpansion

set "SCRIPT_DIR=%~dp0"
set "TOTAL_PASSED=0"
set "TOTAL_FAILED=0"

echo.
echo +============================================+
echo ^|       SPIN ^& WIN — FULL TEST SUITE         ^|
echo +============================================+
echo.

call :run_test "Core Crypto Tests" "%SCRIPT_DIR%core_crypto_test.bat"
call :run_test "API Integration Tests" "%SCRIPT_DIR%api_integration_test.bat"
call :run_test "Prize Alignment Test" "%SCRIPT_DIR%prize_alignment_test.bat"
call :run_test "Mystery Prize Test" "%SCRIPT_DIR%mystery_prize_test.bat"
call :run_test "Concurrent Stress Test" "%SCRIPT_DIR%stress_test.bat"

echo +============================================+
echo   Suites passed: !TOTAL_PASSED!
echo   Suites failed: !TOTAL_FAILED!
if !TOTAL_FAILED! equ 0 (
    echo   ALL TESTS PASSED
) else (
    echo   SOME TESTS FAILED
)
echo +============================================+

if !TOTAL_FAILED! neq 0 exit /b 1
exit /b 0

:run_test
set "TEST_NAME=%~1"
set "TEST_SCRIPT=%~2"
echo --- Running: %TEST_NAME% ---
echo.
call "%TEST_SCRIPT%"
if !errorlevel! equ 0 (
    set /a TOTAL_PASSED+=1
) else (
    set /a TOTAL_FAILED+=1
    echo.
    echo   FAILED: %TEST_NAME%
)
echo.
goto :eof
