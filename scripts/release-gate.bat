@echo off
setlocal
cd /d %~dp0\..
if not exist runtime\reports mkdir runtime\reports

echo [gravity] release gate

echo [1/8] cargo fmt check
cargo fmt --all -- --check
if errorlevel 1 exit /b %ERRORLEVEL%

echo [2/8] cargo clippy
cargo clippy --workspace --all-targets -- -D warnings
if errorlevel 1 exit /b %ERRORLEVEL%

echo [3/8] build workspace debug
cargo build --workspace
if errorlevel 1 exit /b %ERRORLEVEL%

echo [4/8] build workspace release
cargo build --workspace --release
if errorlevel 1 exit /b %ERRORLEVEL%

echo [5/8] optional JIT feature build
cargo build --features gravity-tile/cranelift-jit
if errorlevel 1 exit /b %ERRORLEVEL%

echo [6/8] tests
cargo test --workspace
if errorlevel 1 exit /b %ERRORLEVEL%

echo [7/8] benchmark
call scripts\bench.bat
if errorlevel 1 exit /b %ERRORLEVEL%

echo [8/8] report checks
if not exist runtime\reports\gravity-bench.json exit /b 1
if not exist runtime\reports\gravity-bench.csv exit /b 1
if not exist runtime\reports\gravity-release-report.md exit /b 1

echo [gravity] release gate passed
exit /b 0
