# metafile-core

The common vocabulary used by the `metafile-rs` parsers and renderers:
geometry, transforms, graphics state, drawing objects, renderer events,
diagnostics, errors, options, and resource limits.

Application code will usually want the `metafile` crate. This crate is useful
when implementing another renderer or working directly with playback events
through the `Renderer` trait.

The crate forbids unsafe Rust and is compatible with native Rust and
`wasm32-unknown-unknown`.

Licensed under the Apache License 2.0.
