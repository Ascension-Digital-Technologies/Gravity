# Hot/Cold Storage Runtime

Gravity v1.3 adds a bounded hot/cold persistence queue.

## Hot Path

The market worker mutates the owned CLOB and immediately updates:

- cached depth snapshots
- book-event ring
- audit ring
- persistence queue
- WebSocket broadcast channel

## Cold Path

Cold durable writes are represented by persistence records that can be drained by future database writer workers.

The queue is bounded. When full, the oldest record is dropped and the drop counter is incremented. This protects the market worker from unbounded memory growth.

## API

```text
GET /persistence
GET /persistence/recent?limit=100
```

These endpoints expose queue depth, capacity, drops, and recent pending persistence records.
