# Gravity Wire

Gravity Wire is the compact binary message layer for hot internal paths. Public APIs can stay JSON, but worker-to-worker and benchmarked internal messages should use fixed-width frames where practical.

## Goals

- lower serialization overhead
- fewer allocations per hot-path message
- stable versioned frames
- explicit frame kind, sequence, timestamp, and payload length
- strict decode rules with trailing-byte rejection

## Current v1.4 scope

- new `gravity-wire` crate
- `GVW1` magic/version marker
- order frame encode/decode
- frame kind enum for order, fill, oracle, settlement, and heartbeat
- benchmark phase named `wire`

## Design rule

Do not replace public REST/WebSocket JSON with binary first. Use binary frames internally where throughput matters, then expose friendly APIs at the edge.


## v1.5 Order Batch Frames

Frame kind `OrderBatch = 6` packs a count followed by length-prefixed order payloads. This lets internal clients submit binary microbatches without JSON parsing.

The binary order payload now carries side, order kind, time-in-force, fixed-point raw price, fixed-point raw quantity, and optional client id.
