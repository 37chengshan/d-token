@echo off
setlocal
set ROOT=%~dp0..

if "%~1"=="--version" (
  type "%ROOT%\fixtures\rtk\version.txt"
  exit /b 0
)

if "%~1 %~2 %~3 %~4"=="gain --all --format json" (
  type "%ROOT%\fixtures\rtk\gain-all.json"
  exit /b 0
)

if "%~1 %~2"=="discover --all" (
  type "%ROOT%\fixtures\rtk\discover.txt"
  exit /b 0
)

if "%~1"=="session" (
  type "%ROOT%\fixtures\rtk\session.txt"
  exit /b 0
)

echo fake rtk: unsupported command: %* 1>&2
exit /b 2
