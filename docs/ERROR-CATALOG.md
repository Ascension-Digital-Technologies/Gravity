# Gravity Error Catalog

| Code | HTTP | Meaning |
|---|---:|---|
| `invalid_request` | 400 | Malformed input, invalid fixed-point value, unsupported state transition, or failed validation. |
| `not_found` | 404 | Requested account, market, order, pool, index product, or runtime resource does not exist. |
| `conflict` | 409 | Duplicate request, idempotency conflict, or incompatible state transition. |
| `rate_limited` | 429 | Reserved for future auth/rate-limit enforcement. |
| `internal` | 500 | Unexpected Gravity service failure. |
| `unavailable` | 503 | Storage, worker, tile, stream, WAL, or dependency is unavailable/degraded. |

All production SDK helpers should preserve the raw Gravity error object for debugging and report the standardized code to callers.
