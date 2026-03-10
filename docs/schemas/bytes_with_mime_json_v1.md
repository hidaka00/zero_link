# Bytes With MIME JSON v1 (`schema_id=1201`)

- Envelope payload for small binary blobs when shared memory is unnecessary.
- Encoding: UTF-8 JSON object.

## Payload format

```json
{
  "mime_type": "application/octet-stream",
  "data_b64": "AAFkZW1v/w=="
}
```

- `mime_type`: MIME string for consumer-side dispatch.
- `data_b64`: raw bytes encoded as base64 (standard alphabet with padding).

## Header

- `type=2` (`Event`)
- `size=len(payload)`
- `schema_id=1201`

## Notes

- For large payloads, prefer `image_frame.v1` or raw bytes with `buffer_ref`.
- This schema prioritizes interoperability over binary efficiency.
