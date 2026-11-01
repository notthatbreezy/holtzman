@echo off
setlocal
cd /d "%~dp0"
if "%~1"=="" powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0scripts\holt-pocket.ps1" start -IfSelected
if /i "%~1"=="start" powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0scripts\holt-pocket.ps1" start -IfSelected
if errorlevel 1 goto failure
".local\holt\voice-env\Scripts\python.exe" -m client.st_voice %*
if errorlevel 1 goto failure
if /i "%~1"=="stop" powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0scripts\holt-pocket.ps1" stop
if /i "%~1"=="status" powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0scripts\holt-pocket.ps1" status
if errorlevel 1 goto failure
exit /b 0
:failure
pause
exit /b 1
