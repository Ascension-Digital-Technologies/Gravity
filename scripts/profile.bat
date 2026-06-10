@echo off
setlocal
cd /d %~dp0\..
if not exist runtime\reports mkdir runtime\reports
echo [gravity] running profile workload
cargo run -p gravitybench --release -- --orders=250000 --compressions=2500 --wire-ops=250000 --wire-batches=2500 --wire-batch-size=1024 --parallel-markets=8 --parallel-orders=250000 --tiles=8 --tile-jobs=500000 --oracle-events=250000 --amm-quotes=250000 --risk-checks=250000 --liquidations=250000 --perps=250000 --index=250000 --hardware-plans=250000 --jit-kernels=250000 --perf-pool=500000 --json-out=runtime\reports\gravity-profile.json --csv-out=runtime\reports\gravity-profile.csv --md-out=runtime\reports\gravity-profile.md
exit /b %ERRORLEVEL%
