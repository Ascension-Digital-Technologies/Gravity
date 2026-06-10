# Gravity Profiling

Gravity v1.5 adds profile helper scripts that run a heavier release benchmark workload and write reports into `runtime/reports`.

## Windows

```bat
scripts\profile.bat
```

## Linux/macOS

```bash
scripts/profile.sh
```

The current profile workload measures:

```text
insert
batch-insert
snapshot
cancel
match
compress
wire
wire-batch
parallel-markets
```

Future profiler additions should include platform-specific flamegraph support where available. Keep profiling off the production hot path.
