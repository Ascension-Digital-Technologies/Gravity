# Gravity v3.0.0 Production Streaming Layer

Gravity now has a dedicated `gravity-stream` crate for shared event publication.

## Goals

- Serialize once and broadcast to many subscribers.
- Support JSON public streams and binary internal streams.
- Track per-topic published/dropped/subscriber metrics.
- Keep recent metadata for replay and reconnect diagnostics.
- Avoid pushing stream fanout into CLOB/oracle/settlement hot paths.

## Topics

- `book`
- `trades`
- `fills`
- `oracle`
- `amm`
- `risk`
- `liquidations`
- `settlement`
- `perps`
- `index`
- `wal`
- `audit`

## API

```text
GET /streams
GET /streams/recent?limit=100
GET /streams/{topic}/recent?limit=100
GET /ws/book/{symbol}
GET /ws/oracle
```

The existing book/oracle WebSockets remain compatible. Internally, book/oracle events are now also mirrored into the shared stream hub so the following v2.9 tile runtime can consume the same stream fabric.

## Performance direction

The stream hub stores payloads as shared byte buffers and records lightweight metadata for recent replay. The following production pass should move all event-producing engines to publish directly through this hub and add binary WebSocket output for internal clients.
