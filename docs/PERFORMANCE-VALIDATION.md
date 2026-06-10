# Gravity Performance Validation

Gravity v1.1 adds a release validation path focused on repeatable throughput and latency reporting.

## Benchmark phases

The `gravitybench` binary now reports:

- insert throughput
- cancel throughput
- snapshot read throughput
- matching throughput
- p50 / p95 / p99 / max latency in microseconds

## Outputs

```text
runtime/reports/gravity-bench.json
runtime/reports/gravity-bench.csv
runtime/reports/gravity-release-report.md
```

## Run

```bat
scripts\bench.bat
scripts\validate-release.bat
```

The benchmark intentionally runs as a standalone binary instead of unstable Rust bench APIs so it works consistently on Windows and Linux.
