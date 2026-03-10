## Summary
- add contributor guide (`AGENTS.md`)
- add minimal CI for Rust workspace (`fmt`, `clippy`, `test`)
- add smoke gates (`zl-cli smoke-pubsub`, `zl-cli smoke-control`)
- clarify FFI unsafe boundaries and safety docs in `zl-ffi`

## Changes
- docs:
  - add `AGENTS.md` contributor guide
- ci:
  - add `.github/workflows/ci.yml`
  - run `cargo fmt --all -- --check`
  - run `cargo clippy --workspace --all-targets -- -D warnings`
  - run `cargo test --workspace`
  - run smoke checks with `zl-cli`
- code:
  - mark FFI exports as `unsafe extern "C" fn`
  - add `# Safety` sections for unsafe exports
  - update `zl-cli`/tests call sites with explicit `unsafe`

## Validation
- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
- `cargo run -p zl-cli -- smoke-pubsub audio/asr/text ci-smoke`
- `cargo run -p zl-cli -- smoke-control _sys/control reload '{}'`

## Notes
- current implementation remains in-memory; daemonized inter-process transport is future work.
