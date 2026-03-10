# Repository Guidelines

## Project Structure & Module Organization
This repository is a Rust workspace for the ZeroLink connector MVP.

- `Cargo.toml`: workspace manifest and shared package metadata.
- `crates/`: core modules and binaries:
- `zl-proto`, `zl-ipc`, `zl-shm`, `zl-router`, `zl-metrics`, `zl-ffi` (library crates)
- `connectord`, `zl-cli` (binary crates)
- `docs/`: architecture, requirements, and C-ABI references (for example `docs/ffi/zerolink_connector.h`).

Keep new code inside the most specific crate boundary; avoid cross-cutting utilities unless they are truly shared.

## Build, Test, and Development Commands
Use Cargo from the repository root.

- `cargo check --workspace`: fast type and borrow validation.
- `cargo build --workspace`: compile all crates.
- `cargo test --workspace`: run unit tests across the workspace.
- `cargo run -p zl-cli -- smoke-pubsub audio/asr/text hello`: smoke-test publish/subscribe flow.
- `cargo run -p zl-cli -- smoke-control _sys/control reload '{}'`: smoke-test control envelope path.
- `cargo fmt --all` and `cargo clippy --workspace --all-targets -- -D warnings`: formatting and lint gates before PR.

## Coding Style & Naming Conventions
- Rust edition: 2021.
- Follow `rustfmt` defaults (4-space indentation; no tabs).
- Use `snake_case` for functions/modules/files, `PascalCase` for types, and `SCREAMING_SNAKE_CASE` for constants.
- Keep FFI surface stable and explicit: prefer additive changes to structs/functions, never reorder ABI fields.
- Topic naming should follow documented constraints (e.g., `audio/asr/text`, reserved prefixes like `_sys/`).

## Testing Guidelines
- Place unit tests in `mod tests` within each crate (`src/lib.rs`/`src/main.rs`) unless integration tests are required.
- Name tests by behavior (`publish_invalid_topic_returns_error`).
- Run `cargo test --workspace` locally before opening a PR.
- For protocol/FFI changes, include at least one smoke path validation using `zl-cli` commands above.

## Commit & Pull Request Guidelines
Git history uses Conventional Commit prefixes: `feat:`, `fix:`, `docs:`, `chore:`.

- Keep subject lines imperative and scoped (example: `feat: add CBOR control envelope decoding`).
- One logical change per commit.
- PRs should include: purpose, impacted crates, test evidence (`cargo test`/smoke output), and linked issue/task.
- Include CLI output snippets when behavior changes in `zl-cli` or FFI-visible APIs.
