# ZeroLink Python Binding (MVP)

Minimal Python wrapper over the ZeroLink C ABI.

## Features (current)

- open/close client
- publish event payload
- subscribe/unsubscribe with Python callback
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
