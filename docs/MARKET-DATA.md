# Market Data

Gravity v0.3 introduces the async adapter boundary.

## Traits

```text
MarketAdapter
FrameNormalizer
```

Adapters publish normalized `MarketEvent` values into a bounded Tokio channel. This keeps backpressure explicit and prevents unbounded memory growth.

## Included now

- `MockAdapter`
- `MockFeed`
- `JsonFrameNormalizer` placeholder
- sequence tracking
- gap/duplicate rejection

## Coming in v0.4

- Binance adapter
- Coinbase adapter
- Kraken adapter
- reconnect/backoff
- feed lag metrics
- orderbook snapshot/delta recovery hooks


## v0.4 adapter update

See `EXCHANGE-ADAPTERS.md` for the optional Binance/Coinbase/Kraken adapter layer.
