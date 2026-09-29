@echo off
rem Point the elevated umiray task at the installed client, preserving its trigger.
setlocal
set "EXE=%LOCALAPPDATA%\umiray\umiray.exe"
set "LOG=%TEMP%\umiray-task-repair.log"

fltmc >nul 2>&1
if errorlevel 1 (
  powershell -NoProfile -Command "Start-Process -FilePath '%~f0' -Verb RunAs"
  exit /b
)

> "%LOG%" echo Repairing umiray task
if not exist "%EXE%" (
  >> "%LOG%" echo Installed client not found: %EXE%
  goto done
)

schtasks /Query /TN umiray /V /FO LIST >> "%LOG%" 2>&1
if errorlevel 1 goto done
powershell -NoProfile -Command "$ErrorActionPreference='Stop'; $exe=Join-Path $env:LOCALAPPDATA 'umiray\umiray.exe'; $action=New-ScheduledTaskAction -Execute $exe -Argument '--scheduled' -WorkingDirectory (Split-Path $exe); Set-ScheduledTask -TaskName umiray -Action $action | Out-Null; $saved=(Get-ScheduledTask -TaskName umiray).Actions[0]; if ($saved.Execute -ne $exe -or $saved.Arguments -ne '--scheduled' -or $saved.WorkingDirectory -ne (Split-Path $exe)) { throw 'Task action did not match installed client' }" >> "%LOG%" 2>&1
if errorlevel 1 goto done
schtasks /Query /TN umiray /V /FO LIST >> "%LOG%" 2>&1

:done
type "%LOG%"
echo.
echo Log: %LOG%
pause
