import ctypes
import os
from pathlib import Path


def _candidate_paths() -> list[Path]:
    env = os.getenv("ZEROLINK_NATIVE_LIB")
    if env:
        return [Path(env)]

    root = Path(__file__).resolve().parents[4]
    return [
        root / "target" / "debug" / "libzl_ffi.so",
        root / "target" / "debug" / "libzl_ffi.dylib",
        root / "target" / "debug" / "zl_ffi.dll",
        root / "bindings" / "python" / "src" / "zerolink" / "native" / "libzl_ffi.so",
        root / "bindings" / "python" / "src" / "zerolink" / "native" / "libzl_ffi.dylib",
        root / "bindings" / "python" / "src" / "zerolink" / "native" / "zl_ffi.dll",
    ]


def load_library() -> ctypes.CDLL:
    tried: list[str] = []
    for path in _candidate_paths():
        tried.append(str(path))
        if path.exists():
            return ctypes.CDLL(str(path))
    joined = "\n  - ".join(tried)
    raise FileNotFoundError(
        "ZeroLink native library was not found.\n"
        f"Tried:\n  - {joined}\n"
        "Set ZEROLINK_NATIVE_LIB or run scripts/bindings/build_native.sh"
    )
