# Contributing to metafile-rs

Thank you for helping improve `metafile-rs`. The project is feature-frozen at
the 1.x engine boundary: maintenance should be driven by reproducible real-file
compatibility defects, security findings, measured performance problems, or
documentation corrections. Please discuss broad new format or renderer work
before investing in an implementation.

## Before opening a change

- Search existing issues and the [post-1.0 backlog](docs/post-1.0-backlog.md).
- Reduce rendering defects to the smallest legal fixture you can share.
- Do not submit confidential Office documents or files with unknown
  redistribution rights.
- Keep WMF, EMF, and EMF+ parsing/playback first-party. Runtime wrappers around
  external converters or third-party metafile renderers are out of scope.
- Preserve the environment-neutral engine boundary: bytes in; SVG, metadata,
  diagnostics, or typed errors out.

Security vulnerabilities should be reported privately through GitHub's
security-advisory interface instead of a public issue.

## Development setup

The workspace requires Rust 1.88 or newer. The repository's
`rust-toolchain.toml` installs the intended components and WASM target.

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --workspace --release
cargo build -p metafile-wasm --target wasm32-unknown-unknown --release
```

WASM integration changes should additionally run:

```bash
node scripts/build-wasm.mjs
node scripts/wasm-smoke.mjs
node scripts/wasm-browser-smoke.mjs
```

Windows-reference changes should run `scripts/windows-qualification.ps1` on a
Windows host with the documented .NET and browser prerequisites.

## Tests and fixtures

- Add focused unit coverage for parser, state, geometry, and error behavior.
- Assert meaningful SVG structure or coordinates, not only that output exists.
- Add a regression test for every panic, hang, bounds error, or real-file bug.
- Record fixture origin, permission, SHA-256, expected format, and notable
  features in `fixtures/manifest.json`.
- Put non-redistributable inputs under an ignored private corpus, never in Git.
- Keep strict and permissive behavior explicit; do not silently approximate
  unsupported semantics.

See [golden testing](docs/golden-testing.md) and the format-specific record
audits under `docs/` for qualification expectations.

## Pull requests

Keep changes focused and explain:

1. the observed input or compatibility problem;
2. the specification behavior being implemented;
3. whether behavior is exact, approximate, diagnostic-only, or unsupported;
4. tests and qualification performed; and
5. any public API, WASM, performance, or security impact.

Do not commit generated `target/` output, private corpora, reference rasters, or
qualification artifacts. Run `git diff --check` before submitting.

## License

Unless explicitly stated otherwise, contributions are submitted under the
project's [Apache License 2.0](LICENSE).
