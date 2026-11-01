@echo off
setlocal
cd /d "%~dp0"
".local\holt\voice-env\Scripts\python.exe" -m client.st_voice %*
if errorlevel 1 pause
