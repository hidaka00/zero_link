# Standard Schemas (MVP)

This directory defines standard cross-process message schema IDs for ZeroLink.

## Goals

- Make payload interpretation consistent across language bindings.
- Keep heavy binary data in shared memory (`buffer_ref`) when possible.
- Version schemas by additive IDs (do not repurpose existing IDs).

## ID table (v1)

- `1`: `raw_bytes.v1`
- `1001`: `int64_le.v1`
- `1002`: `float64_le.v1`
- `1003`: `utf8_string.v1`
- `1004`: `bool.v1`
- `1005`: `int32_le.v1`
- `1006`: `uint64_le.v1`
- `1007`: `float32_le.v1`
- `1008`: `timestamp_ns_i64.v1`
- `1101`: `image_frame.v1`
- `1201`: `bytes_with_mime_json.v1`

## Compatibility policy

- Never change meaning of an existing `schema_id`.
- Breaking changes require a new `schema_id`.
- Optional field additions should preserve v1 decoding behavior.
