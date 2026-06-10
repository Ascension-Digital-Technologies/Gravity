@echo off
setlocal
cd /d %~dp0\..
if not exist runtime\reports mkdir runtime\reports
echo [gravity] running benchmark
cargo run -p gravitybench --release -- --orders=100000 --compressions=1000 --wire-ops=100000 --wire-batches=1000 --wire-batch-size=1024 --parallel-markets=4 --parallel-orders=100000 --tiles=4 --tile-jobs=250000 --oracle-events=100000 --amm-quotes=100000 --risk-checks=100000 --liquidations=100000 --perps=100000 --index=100000 --hardware-plans=100000 --jit-kernels=100000 --perf-pool=250000 --json-out=runtime\reports\gravity-bench.json --csv-out=runtime\reports\gravity-bench.csv --md-out=runtime\reports\gravity-release-report.md
exit /b %ERRORLEVEL%
