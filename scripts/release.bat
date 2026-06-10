@echo off
setlocal
cd /d %~dp0\..
echo [gravity] building release binaries
cargo build --workspace --release
exit /b %ERRORLEVEL%
