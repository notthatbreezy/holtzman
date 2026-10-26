@echo off
setlocal
cd /d "%~dp0"
if not exist ".local\holt\voice-env\Scripts\pythonw.exe" (
  echo HOLT needs its client environment. See client\README.md for setup.
  pause
  exit /b 1
)
if /i "%~1"=="status" (
  ".local\holt\voice-env\Scripts\python.exe" -m client.app status
  exit /b
)
if /i "%~1"=="stop" (
  ".local\holt\voice-env\Scripts\python.exe" -m client.app stop
  exit /b
)
start "" ".local\holt\voice-env\Scripts\pythonw.exe" -m client.app %*
