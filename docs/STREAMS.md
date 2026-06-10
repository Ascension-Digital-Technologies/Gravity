# Gravity Streams

Gravity v0.8.0 includes snapshot WebSocket stream endpoints:

- `/ws/book/{symbol}` sends refreshed orderbook snapshots about every 250 ms.
- `/ws/oracle` sends refreshed oracle report snapshots about every 500 ms.

These are intentionally simple and safe. A later release should replace snapshot polling with broadcast channels fed directly by book/oracle events.


## v0.8 stream payloads

`/ws/book/{symbol}` now sends:

- `book_snapshot` as the first message
- `book_event` for accepted, rejected, canceled, and fill events
- `heartbeat` every 15 seconds
- `lagged` if the receiver falls behind the broadcast ring

`/ws/oracle` now sends:

- `oracle_snapshot` as the first message
- `oracle_report` for live oracle updates
- `heartbeat` every 15 seconds
- `lagged` if the receiver falls behind the broadcast ring
