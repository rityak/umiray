@echo off
rem B-015: живая проверка «задача указывает на исчезнувший бинарь».
rem Заводит задачу планировщика, ломает ей путь, смотрит, что клиент этого не слушается,
rem и возвращает всё как было. Правá просит сам — обычным окном UAC.
net session >nul 2>&1
if errorlevel 1 (
  powershell -NoProfile -Command "Start-Process -Verb RunAs -FilePath '%~f0'"
  exit /b
)
set OUT=%TEMP%\umiray-stale-task.log
cd /d "%~dp0..\src-tauri"
echo Проверка B-015, вывод в %OUT%
cargo test live_a_stale_task -- --ignored --nocapture --test-threads=1 > "%OUT%" 2>&1
type "%OUT%"
echo.
echo Вывод сохранён в %OUT%
pause
