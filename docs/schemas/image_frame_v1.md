# Image Frame v1 (`schema_id=1101`)

Image data uses frame (`type=1`) with binary pixels in `buffer_ref`.

## Transport

- Header:
  - `type=1` (`Frame`)
  - `schema_id=1101`
  - `size=byte_length_of_pixel_buffer`
- Pixel bytes: raw contiguous frame bytes in `buffer_ref`.

## Metadata (side-channel)

Use a paired control/event payload (JSON/CBOR) carrying:

- `width` (u32)
- `height` (u32)
- `channels` (u32)
- `stride_bytes` (u32)
- `pixel_format` (string, e.g. `rgb8`, `rgba8`, `gray8`, `bgr8`)

## Notes

- Receiver reconstructs shape/format from metadata and reads bytes from `buffer_ref`.
- Endianness only matters for packed pixel formats wider than 8-bit; define per format.
