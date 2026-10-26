@echo off
setlocal
cd /d "%~dp0"
if /i "%~1"=="stop" (
  ".local\holt\voice-env\Scripts\python.exe" -m client.tavern stop
  exit /b
)
if /i "%~1"=="status" (
  ".local\holt\voice-env\Scripts\python.exe" -m client.tavern status
  exit /b
)
if not exist ".local\holt\tavern\installation.json" (
  echo Run scripts\setup-holt-tavern.py with the HOLT Python environment first.
  pause
  exit /b 1
)
start "" ".local\holt\voice-env\Scripts\pythonw.exe" -m client.tavern
