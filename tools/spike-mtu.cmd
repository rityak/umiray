@echo off
rem S-031: TUN adapter MTU spike. Elevates itself, output lands in a file and opens in notepad.
rem Comments are ASCII because cmd.exe reads this file in the console codepage.
setlocal
set RESULT=%TEMP%\umiray-spike-mtu.txt

fltmc >nul 2>&1
if not errorlevel 1 goto elevated
set SHIM=%TEMP%\umiray-elevate.vbs
> "%SHIM%" echo CreateObject("Shell.Application").ShellExecute "%~f0", "", "", "runas", 1
cscript //nologo "%SHIM%"
exit /b 0

:elevated
cd /d "%~dp0..\src-tauri"
set CARGO_EXE=%USERPROFILE%\.cargo\bin\cargo.exe
if not exist "%CARGO_EXE%" set CARGO_EXE=cargo
"%CARGO_EXE%" test live_spike_tun_mtu -- --ignored --nocapture --test-threads=1 > "%RESULT%" 2>&1
set CODE=%ERRORLEVEL%
start "" notepad "%RESULT%"
exit /b %CODE%
