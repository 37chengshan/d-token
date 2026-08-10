@echo off
setlocal
set MODE=%~1
if "%MODE%"=="" set MODE=0

if "%MODE%"=="large" (
  for /L %%I in (1,1,512) do (
    echo stdout-%%I-abcdefghijklmnopqrstuvwxyz
    echo stderr-%%I-abcdefghijklmnopqrstuvwxyz 1>&2
  )
  exit /b 0
)

if "%MODE%"=="sleep" (
  timeout /t %~2 /nobreak >nul
  exit /b 0
)

if "%MODE%"=="git" (
  echo Receiving objects:  10%%
  echo Receiving objects:  10%%
  echo Receiving objects:  10%%
  echo Receiving objects:  10%%
  echo up to date
  exit /b 0
)

if "%MODE%"=="json" (
  echo {
  echo   "name": "fixture",
  echo   "ok": true,
  echo   "count": 3
  echo }
  exit /b 0
)

echo [31merror[0m
echo.
echo error
echo error
echo SECRET=must-not-persist 1>&2
exit /b %MODE%
