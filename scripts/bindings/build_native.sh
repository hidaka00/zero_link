#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$repo_root"

cargo build -p zl-ffi

mkdir -p bindings/python/src/zerolink/native

if [[ -f target/debug/libzl_ffi.so ]]; then
  cp target/debug/libzl_ffi.so bindings/python/src/zerolink/native/
fi
if [[ -f target/debug/libzl_ffi.dylib ]]; then
  cp target/debug/libzl_ffi.dylib bindings/python/src/zerolink/native/
fi
if [[ -f target/debug/zl_ffi.dll ]]; then
  cp target/debug/zl_ffi.dll bindings/python/src/zerolink/native/
fi

echo "native library copied to bindings/python/src/zerolink/native"
