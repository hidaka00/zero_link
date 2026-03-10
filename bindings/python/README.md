# ZeroLink Python Binding (MVP)

Minimal Python wrapper over the ZeroLink C ABI.

## Features (current)

- open/close client
- publish event payload
- publish frame payload via shared buffer reference (`publish_buffer`)
- subscribe/unsubscribe with Python callback
- send control envelope (`send_control`)
- daemon health query (`health`, daemon endpoint only)
- status-specific exceptions (`ZlInvalidArgError`, `ZlIpcDisconnectedError`, etc.)

## Quick start

1. Build native library:

```bash
./scripts/bindings/build_native.sh
```

2. Run example:

```bash
PYTHONPATH=bindings/python/src python3 bindings/python/examples/pubsub.py
```

Daemon example (requires `connectord` running on `daemon://local`):

```bash
PYTHONPATH=bindings/python/src python3 bindings/python/examples/daemon_demo.py
```

You can override native library path with `ZEROLINK_NATIVE_LIB`.

3. Optional: install as editable package for local development (no public publish):

```bash
./scripts/bindings/python_dev_setup.sh
source .venv-zerolink/bin/activate
```

4. Run smoke test:

```bash
python3 -m unittest -v bindings/python/tests/test_smoke.py
```

5. Run daemon smoke test (when `connectord` is running):

```bash
ZEROLINK_PY_SMOKE_DAEMON=1 \
ZEROLINK_PY_ENDPOINT=daemon://local \
python3 -m unittest -v bindings/python/tests/test_smoke.py
```

Daemon smoke includes both standard publish and buffer-ref publish paths.

## Standard schema helpers

The package exports common schema IDs and codec helpers for cross-process consistency:

- `SCHEMA_INT64_LE_V1`, `SCHEMA_FLOAT64_LE_V1`, `SCHEMA_UTF8_STRING_V1`, `SCHEMA_IMAGE_FRAME_V1`
- `encode_int64/decode_int64`
- `encode_float64/decode_float64`
- `encode_string/decode_string`
- `ImageMeta` (JSON metadata helper for image frame side-channel)

Schema definitions are documented under `docs/schemas/`.
