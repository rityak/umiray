@echo off
rem Blink the address table: add a temporary link-local address to the active adapter
rem and remove it. That is exactly the change NotifyAddrChange reports (D-112) — the same
rem one a wake-up or an interface switch produces.
rem
rem Changing addresses needs rights, so the script elevates itself: without them it re-launches through
rem ShellExecute with the "runas" verb, which is the ordinary UAC window a person sees.
rem The elevation request comes from the script the person started, not from a shell.
rem
rem Comments are ASCII on purpose: a .cmd is read in the console codepage, not UTF-8.
setlocal
set MARK=%TEMP%\umiray-wake-check.txt
if exist "%MARK%" del "%MARK%"

fltmc >nul 2>&1
if not errorlevel 1 goto elevated
set SHIM=%TEMP%\umiray-elevate.vbs
> "%SHIM%" echo CreateObject("Shell.Application").ShellExecute "%~f0", "", "", "runas", 1
cscript //nologo "%SHIM%"
exit /b 0

:elevated
rem The work lives in addr-blip.ps1: it remembers DHCP and the DNS source of the adapter and
rem puts them back. New-NetIPAddress on a DHCP interface turns DHCP off (GOTCHAS).
powershell -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "%~dp0addr-blip.ps1" -Mark "%MARK%"
exit /b %errorlevel%
