@echo off
setlocal

echo.
echo  sshgit installer
echo  ================
echo.

:: ── Locate binaries ───────────────────────────────────────────────────────────
:: Pre-compiled release: sshgit.exe and bash-stub.exe sit next to install.bat
:: Source install: build them with cargo

if exist "%~dp0sshgit.exe" (
    echo Using pre-compiled binaries.
    set "SSHGIT_BIN=%~dp0sshgit.exe"
    set "STUB_BIN=%~dp0bash-stub.exe"
    if not exist "%STUB_BIN%" (
        echo ERROR: bash-stub.exe not found next to install.bat
        exit /b 1
    )
) else (
    echo Pre-compiled binaries not found -- building from source...
    echo.
    where cargo >nul 2>&1
    if errorlevel 1 (
        echo ERROR: cargo not found. Install Rust from https://www.rust-lang.org
        exit /b 1
    )
    cargo build --release
    if errorlevel 1 ( echo ERROR: build failed. & exit /b 1 )
    cargo build --release --manifest-path "%~dp0bash-stub\Cargo.toml"
    if errorlevel 1 ( echo ERROR: stub build failed. & exit /b 1 )
    set "SSHGIT_BIN=%~dp0target\release\sshgit.exe"
    set "STUB_BIN=%~dp0bash-stub\target\release\bash-stub.exe"
)

:: ── Install directory ─────────────────────────────────────────────────────────
set "INSTALL_DIR=%LOCALAPPDATA%\sshgit"
for %%d in (cmd bin usr\bin) do md "%INSTALL_DIR%\%%d" 2>nul

:: ── Deploy binaries ───────────────────────────────────────────────────────────
copy /y "%SSHGIT_BIN%" "%INSTALL_DIR%\cmd\git.exe" >nul
for %%d in (cmd bin usr\bin) do (
    for %%n in (bash.exe sh.exe ssh.exe) do (
        copy /y "%STUB_BIN%" "%INSTALL_DIR%\%%d\%%n" >nul
    )
)

:: ── Config ────────────────────────────────────────────────────────────────────
echo.
set /p "HOST=SSH host (e.g. dev or 192.168.1.10): "
set /p "ROOT=Root path on remote where repos live (e.g. /home/jake/Development): "

set "CONFIG_DIR=%USERPROFILE%\.config\sshgit"
md "%CONFIG_DIR%" 2>nul

(
    echo host = "%HOST%"
    echo root = "%ROOT%"
) > "%CONFIG_DIR%\config.toml"

:: ── Done ──────────────────────────────────────────────────────────────────────
echo.
echo  Done!
echo.
echo  Installed to: %INSTALL_DIR%\cmd\git.exe
echo  Config:       %CONFIG_DIR%\config.toml
echo.
echo  Next step:
echo    Point your git GUI at: %INSTALL_DIR%\cmd\git.exe
echo.
echo  In Fork: Preferences ^> Git ^> Git executable
echo.

endlocal
