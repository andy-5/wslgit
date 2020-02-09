@echo off
net session >nul 2>&1
if %ERRORLEVEL% neq 0 (
   echo.
   echo This script must be run as administrator to work properly!
   echo Right click on the script and select "Run as administrator".
   echo.
   goto :error
)

set CWD=%~dp0
set BINDIR=%CWD%bin
set CMDDIR=%CWD%cmd
cd %CWD%

echo.
echo Make sure Fork.RI is executable in WSL...
wsl -- chmod +x cmd/Fork.RI
if %ERRORLEVEL% neq 0 (
    echo ERROR! Failed to make Fork.RI executable in WSL.
    goto :error
)
echo OK.

echo.
if exist "%CMDDIR%\git.exe" (
    echo 'git.exe' already exist.
) else (
    echo Create 'git.exe' symlink...
    mklink %CMDDIR%\git.exe %CMDDIR%\wslgit.exe
    if %ERRORLEVEL% neq 0 (
        echo ERROR! Failed to create symlink '%CMDDIR%\git.exe'.
        goto :error
    ) else (
        echo OK.
    )
)

echo.
if not exist "%BINDIR%" (
    echo Create 'bin' directory...
    mkdir "%BINDIR%"
    if %ERRORLEVEL% neq 0 (
        echo ERROR! Failed to create directory '%BINDIR%'.
        goto :error
    ) else (
        echo OK.
    )
)

echo.
if exist "%BINDIR%\git.exe" (
    echo 'bin\git.exe' already exist.
) else (
    echo Create 'bin\git.exe' symlink...
    mklink %BINDIR%\git.exe %CMDDIR%\wslgit.exe
    if %ERRORLEVEL% neq 0 (
        echo ERROR! Failed to create symlink '%BINDIR%\git.exe'.
        goto :error
    ) else (
        echo OK.
    )
)

echo.
if exist "%BINDIR%\Fork.RI" (
    echo 'bin\Fork.RI' already exist.
) else (
    echo Create 'bin\Fork.RI' symlink...
    mklink %BINDIR%\Fork.RI %CMDDIR%\Fork.RI
    if %ERRORLEVEL% neq 0 (
        echo ERROR! Failed to create symlink '%BINDIR%\Fork.RI'.
        goto :error
    ) else (
        echo OK.
    )
)

echo.
if exist "%BINDIR%\sh.exe" (
    echo 'bin\sh.exe' already exist.
) else (
    echo Create 'bin\sh.exe' symlink...
    mklink %BINDIR%\sh.exe C:\Windows\System32\wsl.exe
    if %ERRORLEVEL% neq 0 (
        echo ERROR! Failed to create symlink '%BINDIR%\sh.exe'.
        goto :error
    ) else (
        echo OK.
    )
)

echo.
if exist "%BINDIR%\bash.exe" (
    echo 'bin\bash.exe' already exist.
) else (
    echo Create 'bin\bash.exe' symlink...
    mklink %BINDIR%\bash.exe C:\Windows\System32\wsl.exe
    if %ERRORLEVEL% neq 0 (
        echo ERROR! Failed to create symlink '%BINDIR%\bash.exe'.
        goto :error
    ) else (
        echo OK.
    )
)

echo.
echo Installation successful!
echo.
echo (Optional) Add to the Windows Path environment variable:
echo  %CMDDIR%
echo.
pause
exit /B 0

:error
pause
exit /B 1
