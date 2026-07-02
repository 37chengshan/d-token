@echo off
setlocal
set ROOT=%~dp0..

if "%~1"=="--version" (
  type "%ROOT%\fixtures\headroom\version.txt"
  exit /b 0
)

if "%~1"=="doctor" (
  type "%ROOT%\fixtures\headroom\doctor.txt"
  exit /b 0
)

echo fake headroom: unsupported command: %* 1>&2
exit /b 2
