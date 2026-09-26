@echo off
setlocal DisableDelayedExpansion
chcp 65001 >nul
title Easy_Irodori_TTS
cd /d "%~dp0"
if not exist "%~dp0Easy_irodori_tts.ps1" (
  echo Missing Easy_irodori_tts.ps1. Keep all launcher files together.
  pause
  exit /b 1
)
powershell.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -File "%~dp0Easy_irodori_tts.ps1" %*
set "EASY_EXIT=%ERRORLEVEL%"
if not "%EASY_EXIT%"=="0" echo Setup or launch failed. See the logs folder.
pause
exit /b %EASY_EXIT%
