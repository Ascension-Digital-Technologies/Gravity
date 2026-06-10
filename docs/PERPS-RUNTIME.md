# Gravity Perps Runtime

Gravity v2.6.0 adds the perpetual futures foundation. The perps engine is intentionally kept outside Stargate's hot path: Gravity computes mark/index pricing, funding state, margin/PnL views, liquidation price hints, and settlement hints, while Stargate/L3 remains final settlement truth.

## Features

- Perp market definitions
- Index and mark price tracking
- Funding-rate update path
- Open-interest accounting
- Long/short position records
- Fixed-point notional, PnL, equity, and margin calculations
- Liquidation price hinting
- Perp events and stats
- API routes and benchmark phase

## API

- `GET /perps/markets`
- `POST /perps/markets`
- `GET /perps/markets/{symbol}`
- `POST /perps/positions/open`
- `GET /perps/positions?limit=100`
- `GET /perps/accounts/{account}/positions`
- `POST /perps/funding`
- `GET /perps/events?limit=100`
- `GET /perps/stats`

## Performance

`gravitybench` includes a `perps` phase for funding updates and position open calculations. The current target is `250k ops/sec` as a foundation target before deeper pooling and JIT kernels.
