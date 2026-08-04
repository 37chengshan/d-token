@echo off
setlocal

if "%~1"=="identity" (
  echo {"component":"dtoken-agent-bridge","protocol_version":1,"version":"0.2.0-beta.1","arch":"fixture","os":"fixture"}
  exit /b 0
)

echo fake dtoken-agent-bridge: unsupported command: %* 1>&2
exit /b 2
