@echo off
setlocal
powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0scripts\holt-chatterbox.ps1" %*
if errorlevel 1 pause
