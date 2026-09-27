@echo off
rem Stop a running umiray window, including one raised by the scheduled task (D-087).
rem
rem Why a script: a client started by the task runs elevated, and an unelevated taskkill
rem gets "access denied" (GOTCHAS). schtasks /End alone does not always take it down either,
rem so both are done here, in that order.
rem
rem The script elevates itself: without rights it re-launches through ShellExecute with the
rem "runas" verb, which is the ordinary UAC window a person sees. The elevation request comes
rem from the script the person started, not from a shell.
rem
rem Comments are ASCII on purpose: a .cmd is read in the console codepage, not UTF-8.
setlocal
set APP=umiray
set CORE=mihomo
if /I "%~1"=="dev" set APP=umiray-dev
if /I "%~1"=="dev" set CORE=mihomo-dev

fltmc >nul 2>&1
if not errorlevel 1 goto elevated
set SHIM=%TEMP%\%APP%-elevate.vbs
> "%SHIM%" echo CreateObject("Shell.Application").ShellExecute "%~f0", "%~1", "", "runas", 1
cscript //nologo "%SHIM%"
exit /b 0

:elevated
schtasks /End /TN %APP% >nul 2>&1
taskkill /IM %APP%.exe /F >nul 2>&1
taskkill /IM %CORE%.exe /F >nul 2>&1
exit /b 0
