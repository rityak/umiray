@echo off
rem Run the TUN DNS leak check from an elevated console and keep the complete output.
rem Comments are ASCII because cmd.exe reads this file in the console codepage.
setlocal
set RESULT=%TEMP%\umiray-live-dns.txt

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
"%CARGO_EXE%" test live_dns_leak_under_tun -- --ignored --nocapture --test-threads=1 > "%RESULT%" 2>&1
set CODE=%ERRORLEVEL%
start "" notepad "%RESULT%"
exit /b %CODE%
