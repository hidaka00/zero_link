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

## `bool.v1` (`schema_id=1004`)

- Payload: 1 byte (`0x00=false`, non-zero=true).
- Header:
  - `type=2` (`Event`)
  - `size=1`

## `int32_le.v1` (`schema_id=1005`)

- Payload: 4 bytes, little-endian signed int32.
- Header:
  - `type=2` (`Event`)
  - `size=4`

## `uint64_le.v1` (`schema_id=1006`)

- Payload: 8 bytes, little-endian unsigned uint64.
- Header:
  - `type=2` (`Event`)
  - `size=8`
