# Gravity Binary Protocol Contract

Gravity keeps JSON APIs for public clients and binary APIs for high-speed internal clients.

## Current binary endpoints

- `POST /binary/orders`
- `POST /binary/orders/batch`

## Frame rules

Gravity Wire frames use the `GVW1` magic/version boundary and strict decode validation:

- invalid magic fails closed
- unsupported version fails closed
- unsupported frame kind fails closed
- payload-length mismatch fails closed
- trailing bytes fail closed

## Compatibility

Frame kind additions are backward-compatible. Field reorderings or changed numeric encodings require a wire version bump.
