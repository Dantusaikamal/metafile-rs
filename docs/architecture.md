# Architecture

`metafile-core` owns format-neutral geometry, GDI-like state, graphics objects,
resource limits, diagnostics, errors, and renderer events. `metafile-wmf` owns
bounded binary parsing, typed records, WMF header validation, stateful playback,
inspection, text decoding, and DIB decoding. `metafile-svg` consumes renderer
events and serializes deterministic self-contained SVG. `metafile-wasm` only
maps byte slices and serializable Rust values to JavaScript values.

WMF records never append SVG directly. Playback mutates a complete device
context and emits mapped graphics primitives. This boundary is intended for
future EMF and EMF+ parsers.

