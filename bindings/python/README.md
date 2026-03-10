# ZeroLink Python Binding (MVP)

Minimal Python wrapper over the ZeroLink C ABI.

## Features (current)

- open/close client
- publish event payload
- subscribe/unsubscribe with Python callback

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

3. Run smoke test:

```bash
PYTHONPATH=bindings/python/src python3 -m unittest -v bindings/python/tests/test_smoke.py
```
