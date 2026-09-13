# metafile-rs

`metafile-rs` is an early-stage, first-party Rust engine for parsing Windows
Metafiles and rendering deterministic, self-contained SVG. It targets native
Rust and `wasm32-unknown-unknown`. It does not call Windows GDI, external
converters, browser Canvas, or remote services.

This release establishes substantial WMF support and a hardened ordinary-EMF
engine. It is not a claim of complete Windows GDI compatibility. EMF+ is
detected but its playback remains a roadmap feature.

## Rust API

```rust
let bytes = std::fs::read("drawing.wmf")?;
let info = metafile::inspect(&bytes)?;
let result = metafile::to_svg(&bytes, Default::default())?;
std::fs::write("drawing.svg", result.svg)?;
```

`RenderResult` includes SVG, format-neutral `MetafileInfo` (with nested
`FormatSpecificInfo::Wmf` or `FormatSpecificInfo::Emf` details), and non-fatal diagnostics.
`RenderOptions` provides strict mode and configurable resource limits. Lower
level consumers can call `metafile_wmf::playback` with any
`metafile_core::Renderer` without depending on the SVG backend.

## WMF compatibility

| Area | Status | Notes |
| --- | --- | --- |
| Aldus Placeable WMF | Supported | Header, checksum, bounds, units-per-inch, and nested METAHEADER are validated. |
| Standard WMF | Supported with inferred bounds | Playback-derived bounds are used; empty and text-heavy files may have approximate bounds. |
| Object lifecycle / SaveDC | Supported | First-free allocation, selection, deletion checks, and full saved-state restoration are implemented. |
| MM_TEXT and metric/English/TWIPS map modes | Supported | Includes inverted Y axes; physical conversion assumes 96 SVG units per inch. |
| MM_ISOTROPIC / MM_ANISOTROPIC | Supported | Explicit window/viewport origins, extents, offsets, and scaling are applied. |
| Lines and common vector shapes | Supported | Line, polyline, polygon, rectangle, round rectangle, ellipse, and pixel. |
| Arc / pie / chord | Partial | Projection, direction, mapping inversion, closure and full ellipses are unit-tested; a limited GDI-produced Windows case passes visual/metric review, but the exhaustive matrix is incomplete. |
| PolyPolygon | Supported | One compound path preserves ALTERNATE/even-odd and WINDING/nonzero fill behavior; a GDI-produced nested-hole case is reference-compared. |
| TextOut / ExtTextOut | Partial | Charset decoding, distinct horizontal/vertical alignment, style, escapement, `dx`, UPDATECP, clipping and opaque rectangles are supported; font metrics and no-`dx` advance remain approximate and diagnostic. |
| Rectangular clipping | Partial | Intersect and saved/restored rectangular clips are supported; exclusion/region clips are diagnostic-only. |
| BI_RGB DIB | Partial | 1/4/8/24/32-bit decoding, palettes, orientation, row padding, signed source crop, destination mirroring, and STRETCHDIB are supported. COLORONCOLOR is pixelated; other stretch modes are approximate or unsupported. |
| Other bitmap transfers/compression | Diagnostic-only | Safely skipped in permissive mode and rejected in strict mode. |
| Solid/null brushes | Supported | Hatch, pattern, and DIB-pattern brush fidelity is diagnostic-only. |
| Raster operations / META_ESCAPE | Diagnostic-only | SRCCOPY bitmap transfer is supported; other operations are not emulated. |
| Ordinary EMF | Partial | Validated headers, world/mapping transforms, common vectors/paths, transformed polygon/path clips, Unicode/ANSI text, and affine BI_RGB StretchDIBits are rendered. Arbitrary-affine text and non-uniform geometric pens remain explicit approximations. See `docs/emf-record-audit.md`. |
| EMF+ | Classified, playback unsupported | Embedded EMF+ comments produce `MetafileFormat::EmfPlus`; playback fails explicitly instead of ignoring the stream. |

Unknown records are safely skipped with capped, aggregated diagnostics in
permissive mode. In strict mode, an unknown operation or a known operation
whose semantics would be skipped or materially approximated fails with a typed
`UnsupportedCriticalFeature` error. Benign informational diagnostics do not
fail strict rendering.

Standard WMFs have their SVG viewBox inferred from emitted drawing extents.
Files with no drawable operations receive a 1x1 fallback viewBox and an
explicit diagnostic that bounds are unreliable. Compatibility is being
qualified against a provenance-controlled corpus. The committed corpus
currently contains project-generated Windows GDI outputs, not yet a broad
independent Office/DOCX corpus. Passing synthetic or generated cases is not
evidence of pixel-perfect Windows GDI equivalence.

## Architecture

```text
metafile-wmf / metafile-emf -> stateful GDI playback -> metafile-core Renderer events
metafile-svg -----------------------------------------------> SVG
metafile facade composes playback + SVG rendering
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

`metafile-wasm` exposes format-neutral `inspectMetafile(Uint8Array)` and
`metafileToSvg(Uint8Array, options?)`; `inspectWmf` and `wmfToSvg` remain
WMF-specific compatibility APIs and reject EMF with a structured
`format_mismatch` error. The generated wasm-bindgen loader is
responsible for initialization; the engine itself performs no fetch, filesystem,
DOM, Canvas, or Node operations. No npm package is included yet.

```bash
node scripts/build-wasm.mjs
node scripts/wasm-smoke.mjs
```

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
permission in `fixtures/manifest.json`; private files can live in ignored
`fixtures/private/` or `METAFILE_FIXTURE_DIR`. See
[docs/release-readiness.md](docs/release-readiness.md) for the remaining 0.1.0
evidence gates.

## License

Licensed under either Apache-2.0 or MIT, at your option.
