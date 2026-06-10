@echo off
setlocal
cd /d %~dp0\..
echo [gravity] running tests
cargo test --workspace
exit /b %ERRORLEVEL%
