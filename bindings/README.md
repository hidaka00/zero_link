# Language Bindings

This directory contains language-specific SDK wrappers built on top of the stable C ABI.

## Layout

- `c/include/zerolink_connector.h`: public C header copied from `docs/ffi/zerolink_connector.h`.
- `python/`: Python SDK (`ctypes`) and smoke examples.
- `csharp/`: .NET SDK (P/Invoke) with sample and smoke.
- `node/`: reserved for future wrapper.

## Policy

- Rust core exposes only C ABI (`crates/zl-ffi`).
- Language wrappers stay thin and idiomatic.
- ABI compatibility is validated by smoke tests per language.

## Build

Use helper scripts in `scripts/bindings/` to build native libraries and place them where wrappers can load them.
