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
- `1101`: `image_frame.v1`

## Compatibility policy

- Never change meaning of an existing `schema_id`.
- Breaking changes require a new `schema_id`.
- Optional field additions should preserve v1 decoding behavior.
