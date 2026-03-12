# ZeroLink C# Binding (MVP)

Minimal .NET wrapper over the stable ZeroLink C ABI (`zl_ffi`).

## Layout

- `src/Zerolink`: library (`ZerolinkClient`, schema helpers, native interop).
- `samples/ConsolePubSub`: simple local pub/sub sample.
- `smoke/ZerolinkSmoke`: CI-oriented smoke entrypoint.
- `tests/ZerolinkSchemaSmoke`: schema codec smoke (no daemon/native call needed).

## Prerequisites

- .NET SDK 8.0+
- Native library built beforehand:
  - Linux/macOS: `libzl_ffi.so` / `libzl_ffi.dylib`
  - Windows: `zl_ffi.dll`

Build native library first:

```bash
./scripts/bindings/build_native.sh
```

Set native library path when needed:

- `ZEROLINK_NATIVE_LIB=/abs/path/to/libzl_ffi.so`
- or place library in a standard loader path.

## Build

```bash
dotnet build bindings/csharp/src/Zerolink/Zerolink.csproj
dotnet build bindings/csharp/samples/ConsolePubSub/ConsolePubSub.csproj
dotnet build bindings/csharp/smoke/ZerolinkSmoke/ZerolinkSmoke.csproj
dotnet build bindings/csharp/tests/ZerolinkSchemaSmoke/ZerolinkSchemaSmoke.csproj
```

## Run sample

```bash
dotnet run --project bindings/csharp/samples/ConsolePubSub/ConsolePubSub.csproj
```

## Smoke

```bash
dotnet run --project bindings/csharp/smoke/ZerolinkSmoke/ZerolinkSmoke.csproj -- local
```

Schema-only smoke:

```bash
dotnet run --project bindings/csharp/tests/ZerolinkSchemaSmoke/ZerolinkSchemaSmoke.csproj
```

Daemon mode:

```bash
dotnet run --project bindings/csharp/smoke/ZerolinkSmoke/ZerolinkSmoke.csproj -- daemon://local
```
