# Gravity Binary Intake

Gravity v1.5 adds a dedicated binary intake path for high-throughput internal clients, market makers, SDK services, and future Stargate-native services.

## Routes

```text
POST /binary/orders
POST /binary/orders/batch
```

The public JSON API remains available for dashboards and normal clients. The binary path uses `gravity-wire` frames so hot internal callers avoid JSON parsing and reduce allocation pressure.

## Frame model

```text
GVW1
version
frame kind
flags
sequence
timestamp
payload length
payload
```

Reserved order-related frame kinds:

```text
Order      = 1
OrderBatch = 6
```

## Order payload

Order payloads include:

```text
symbol
account
side
kind
time in force
raw fixed-point price
raw fixed-point quantity
optional client id
```

All money values remain fixed-point integer values using Gravity's standard scale.
