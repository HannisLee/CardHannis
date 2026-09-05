@echo off
setlocal

rem Relaunch from a hidden console when started by double-click.
if /i not "%~1"=="--hidden" (
    powershell.exe -NoProfile -WindowStyle Hidden -Command "Start-Process -FilePath '%ComSpec%' -ArgumentList '/d /c ""%~f0"" --hidden' -WorkingDirectory '%~dp0' -WindowStyle Hidden"
    exit /b 0
)
shift

cd /d "%~dp0"

rem Tools installed under D:\Code on this Windows machine.
set "CARGO_HOME=D:\Code\Cargo"
set "RUSTUP_HOME=D:\Code\Cargo\rustup"
rem vswhere lives next to the VS Installer; vcvars64 calls it and complains if it is not on PATH.
set "PATH=D:\Code\Cargo\bin;D:\Code\nodejs;C:\Program Files (x86)\Microsoft Visual Studio\Installer;%PATH%"

set "VS_BAT=C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat"
if not exist "%VS_BAT%" goto :missing_vs

call "%VS_BAT%" >nul

if not exist "node_modules\.bin\tauri.cmd" goto :missing_deps

rem Since tray mode, closing the window leaves the app running; a second dev instance
rem would fight over the same SQLite database.
tasklist /fi "IMAGENAME eq cardhannis.exe" 2>nul | find /i "cardhannis.exe" >nul && goto :already_running

echo Starting CardHannis in development mode...
echo Closing the window only hides the app to the system tray.
echo Quit from the tray icon menu, or press Ctrl+C here to stop the dev server.
call npm.cmd run tauri:dev

if errorlevel 1 goto :failed

endlocal
exit /b 0

:already_running
echo CardHannis is already running, most likely hidden in the system tray.
echo Right-click the tray icon to show or quit it, then run this script again.
pause
exit /b 1

:missing_vs
echo Could not find Visual Studio Build Tools:
echo %VS_BAT%
pause
exit /b 1

:missing_deps
echo Frontend dependencies are missing.
echo Run these commands once from this project directory:
echo   D:\Code\nodejs\npm.cmd ci
echo   D:\Code\nodejs\npm.cmd --prefix ui ci
pause
exit /b 1

:failed
echo.
echo CardHannis stopped with an error.
pause
exit /b 1
