@echo off
setlocal
powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0scripts\holt-pocket.ps1" %*
if errorlevel 1 (
  pause
  exit /b 1
)
if "%~1"=="" pause
