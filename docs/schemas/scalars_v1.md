# Scalars v1

## `int64_le.v1` (`schema_id=1001`)

- Payload: 8 bytes, little-endian signed int64.
- Header:
  - `type=2` (`Event`)
  - `size=8`

## `float64_le.v1` (`schema_id=1002`)

- Payload: 8 bytes, IEEE754 float64 little-endian.
- Header:
  - `type=2` (`Event`)
  - `size=8`

## `utf8_string.v1` (`schema_id=1003`)

- Payload: UTF-8 bytes (no trailing NUL).
- Header:
  - `type=2` (`Event`)
  - `size=len(payload)`
