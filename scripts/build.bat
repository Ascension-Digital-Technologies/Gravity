@echo off
setlocal
cd /d %~dp0\..
echo [gravity] building workspace
cargo build --workspace
exit /b %ERRORLEVEL%
