@echo off
rem One-liner wrapper so `Build\version.bat [--full|--json]` works from cmd.exe.
rem Prefers PowerShell 7 (pwsh), falls back to Windows PowerShell.
setlocal
set "ARGS="
if /I "%~1"=="--full" set "ARGS=-Full"
if /I "%~1"=="--json" set "ARGS=-Json"
where pwsh >nul 2>&1 && (
  pwsh -NoProfile -ExecutionPolicy Bypass -File "%~dp0version.ps1" %ARGS%
) || (
  powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0version.ps1" %ARGS%
)
