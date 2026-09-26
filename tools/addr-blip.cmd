@echo off
rem Blink the address table: add a temporary link-local address to the active adapter
rem and remove it. That is exactly the change NotifyAddrChange reports (D-112) — the same
rem one a wake-up or an interface switch produces.
rem
rem netsh needs rights, so the script elevates itself: without them it re-launches through
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
rem Adapter is chosen by value, not by localized text: Status is an enum, the same on any
rem Windows language (same reason system/net.rs uses cmdlets instead of netsh output).
powershell -NoProfile -NonInteractive -Command ^
  "$a = Get-NetAdapter | Where-Object { $_.Status -eq 'Up' } | Select-Object -First 1;" ^
  "if (-not $a) { Set-Content -Encoding utf8 '%MARK%' 'no-adapter'; exit 1 };" ^
  "New-NetIPAddress -InterfaceIndex $a.ifIndex -IPAddress 169.254.77.77 -PrefixLength 16 -SkipAsSource $true -ErrorAction SilentlyContinue | Out-Null;" ^
  "Start-Sleep -Seconds 2;" ^
  "Remove-NetIPAddress -IPAddress 169.254.77.77 -Confirm:$false -ErrorAction SilentlyContinue;" ^
  "Set-Content -Encoding utf8 '%MARK%' (\"blinked on \" + $a.Name)"
exit /b 0
