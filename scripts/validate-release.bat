@echo off
setlocal
cd /d %~dp0\..
call scripts\release-gate.bat
exit /b %ERRORLEVEL%
