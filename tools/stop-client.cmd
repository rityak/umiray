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

fltmc >nul 2>&1
if not errorlevel 1 goto elevated
set SHIM=%TEMP%\umiray-elevate.vbs
> "%SHIM%" echo CreateObject("Shell.Application").ShellExecute "%~f0", "", "", "runas", 1
cscript //nologo "%SHIM%"
exit /b 0

:elevated
schtasks /End /TN umiray >nul 2>&1
taskkill /IM umiray.exe /F >nul 2>&1
taskkill /IM mihomo.exe /F >nul 2>&1
exit /b 0
