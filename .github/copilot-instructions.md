# Project Guidelines

## Project and architecture

- This is a Rust 2024 PDF parsing and generation library plus the `demo` CLI. The public library entry point is `src/lib.rs`, which exposes `core`.
- `core::types` holds the shared PDF object model; `core::reader` parses files into a lazily loaded `Document`; `core::writer` serializes documents; and `core::builder` builds higher-level page and content objects.
- Stream decoding is dispatched through `core::filters::StreamFilter`. Keep PDF semantics in the filter layer and isolate third-party codec APIs behind adapters, following `InflateBackend` and `JpegBackend`.

## Build, test, and lint

Run these from the repository root:

```sh
cargo build
cargo test
cargo test --test cli gen_then_parse_roundtrip_via_cli
cargo fmt --all -- --check
cargo clippy --all-targets
```

The focused test command above runs one CLI integration test. For another test, replace its name with the desired test name; use `cargo test <filter>` to select by name across test targets. Apply Rust formatting with `cargo fmt --all`.

## Implementation conventions

- Reading is intentionally permissive: parse according to file contents and report recoverable issues as warnings rather than rejecting solely on the declared PDF version. Writing validates the target version and required features; structurally representable features may be downgraded, but unsupported content semantics must return an error.
- Add stream decoders by implementing `StreamFilter` and registering them with `builtin_filters`; preserve the existing dispatch path. Return decode errors for invalid input instead of panicking, and cover both successful and malformed inputs with tests.
- Keep `core::types` as shared data types, separate from reader/writer I/O and behavior. Follow the existing module layout: focused unit tests live alongside their module, while cross-module and CLI contracts belong under `tests/`.
- Split modules by responsibility. The roadmap calls for directory modules when a source file exceeds roughly 600 lines or 25 KiB, or combines multiple responsibilities; keep implementations, tables, and tests separate where appropriate.
- When changing supported PDF features, update the evidence and status in `docs/matrix/README.md`; follow the completion and progress conventions in `docs/roadmap/README.md`.

See `README.md` for API examples and a module overview, `docs/matrix/README.md` for implementation coverage, and `docs/roadmap/README.md` for development and module-organization conventions.
