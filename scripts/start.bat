@echo off
setlocal
cd /d %~dp0\..
echo [gravity] starting gravityd
cargo run -p gravityd -- %*
exit /b %ERRORLEVEL%
