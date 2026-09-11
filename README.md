# metafile-rs

`metafile-rs` is an early-stage, first-party Rust engine for parsing Windows
Metafiles and rendering deterministic, self-contained SVG. It targets native
Rust and `wasm32-unknown-unknown`. It does not call Windows GDI, external
converters, a browser canvas, or remote services.

This release establishes substantial WMF support. It is not a claim of complete
WMF compatibility. EMF and EMF+ are roadmap formats and are not implemented.

## Rust API

```rust
let bytes = std::fs::read("drawing.wmf")?;
let info = metafile_wmf::inspect(&bytes)?;
let result = metafile_wmf::to_svg(&bytes, Default::default())?;
std::fs::write("drawing.svg", result.svg)?;
```

`RenderResult` includes SVG, inspection metadata, and non-fatal diagnostics.
`RenderOptions` provides strict mode and configurable resource limits.

## Current WMF coverage

- Aldus Placeable and ordinary Standard WMF headers, including checksum and
  declared-size validation.
- Window/viewport origin and extent mapping, scaling and offsets, signed and
  inverted axes.
- Device-context save/restore; selected pens, brushes, and fonts; colors,
  text/background modes, fill mode, raster state, mapping flags, and clipping.
- Pen, brush, and font object creation, selection, and deletion.
- Lines, polylines, polygons, polypolygons, rectangles, rounded rectangles,
  ellipses, arcs, pies, chords, pixels, TextOut, and ExtTextOut.
- Rectangular intersection clipping.
- BI_RGB DIB decoding for 1-, 4-, 8-, 24-, and 32-bit pixels in
  META_STRETCHDIB, embedded as PNG data URIs.

Parsed but currently diagnostic-only areas include other DIB transfer records,
ExcludeClipRect, hatch/pattern brush fidelity, raster operations, and escape
functions. SVG font metrics are approximate and depend on the consuming SVG
renderer. Unknown records are safely skipped with capped, aggregated
diagnostics unless strict mode is enabled.

Standard WMFs have their SVG viewBox inferred from emitted drawing extents.
Files with no drawable operations receive a 1×1 fallback viewBox and should be
treated as lacking reliable bounds.

## Architecture

```text
metafile-wmf parser -> stateful GDI playback -> metafile-core Renderer events
                                            -> metafile-svg -> SVG
bindings/wasm: Uint8Array/bytes -> structured metadata/render result
```

See [docs/architecture.md](docs/architecture.md) and
[docs/dependencies.md](docs/dependencies.md).

## Security

All binary reads are bounded. Record lengths and allocations use checked
arithmetic. Default limits bound input size, record and object counts, points,
bitmap dimensions/pixels, SaveDC depth, diagnostics, and SVG output. Malformed
input returns typed errors and the workspace forbids unsafe Rust.

## WebAssembly

`metafile-wasm` exposes `inspectWmf(Uint8Array)` and
`wmfToSvg(Uint8Array, options?)`. The generated wasm-bindgen loader is
responsible for initialization; the engine itself performs no fetch, filesystem,
DOM, Canvas, or Node operations. No npm package is included yet.

## Development

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --workspace --release
cargo build -p metafile-wasm --target wasm32-unknown-unknown --release
```

A cargo-fuzz target is provided under `fuzz/`. Real-world fixtures belong in
`fixtures/real-world/` and require recorded provenance and redistribution
permission.

## License

Licensed under either Apache-2.0 or MIT, at your option.
