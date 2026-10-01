@echo off
rem Run a node check elevated, with the webview debug port in reach:
rem   tools\as-admin.cmd tools\qd-live.mjs
rem The port travels in WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS, and a variable set before
rem elevation never reaches the elevated process (GOTCHAS). So the script elevates itself
rem first, and the check launches the window from there - elevated and with the port.
rem Output lands in %TEMP%\umiray-as-admin.log: an elevated console is not ours to read.
rem Comments are ASCII on purpose: a .cmd is read in the console codepage, not UTF-8.
setlocal
net session >nul 2>&1
if errorlevel 1 (
  powershell -NoProfile -Command "Start-Process -Verb RunAs -FilePath '%~f0' -ArgumentList '%*'"
  exit /b
)
set OUT=%TEMP%\umiray-as-admin.log
cd /d "%~dp0.."
echo running %* - output in %OUT%
echo %* > "%OUT%"
node %* >> "%OUT%" 2>&1
echo exit %errorlevel% >> "%OUT%"
