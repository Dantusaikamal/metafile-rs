# metafile-rs

`metafile-rs` is a first-party Rust engine for parsing Windows Metafiles and
rendering deterministic, self-contained SVG. It targets native
Rust and `wasm32-unknown-unknown`. It does not call Windows GDI, external
converters, browser Canvas, or remote services.

Version 1.0.1 is production-ready for common WMF, EMF, and EMF+ document
workloads, with structured diagnostics for unsupported or approximate edge
cases. It is not a claim of universal or pixel-perfect Windows GDI/GDI+
compatibility.

The engine supports WMF, ordinary EMF, and EMF+ through the same first-party
native/WASM byte-to-SVG API. It powers `emf-to-png` 1.0.0 for server-side
WMF/EMF/EMF+ to SVG, PNG, and JPEG conversion without Office, LibreOffice,
Inkscape, Canvas, or an external conversion process.

## Rust API

```rust
let bytes = std::fs::read("drawing.wmf")?;
let info = metafile::inspect(&bytes)?;
let result = metafile::to_svg(&bytes, Default::default())?;
std::fs::write("drawing.svg", result.svg)?;
```

`RenderResult` includes SVG, format-neutral `MetafileInfo` (with nested
format-specific details), and non-fatal diagnostics. `RenderOptions` provides
strict mode and configurable resource limits. Lower-level consumers can use
the format crates with any `metafile_core::Renderer` implementation without
depending on the SVG backend.

## WMF compatibility

| Area | Status | Notes |
| --- | --- | --- |
| Aldus Placeable WMF | Supported | Header, checksum, bounds, units-per-inch, and nested METAHEADER are validated. |
| Standard WMF | Supported with inferred bounds | Playback-derived bounds are used; empty and text-heavy files may have approximate bounds. |
| Object lifecycle / SaveDC | Supported | First-free allocation, selection, deletion checks, and full saved-state restoration are implemented. |
| MM_TEXT and metric/English/TWIPS map modes | Supported | Includes inverted Y axes; physical conversion assumes 96 SVG units per inch. |
| MM_ISOTROPIC / MM_ANISOTROPIC | Supported | Explicit window/viewport origins, extents, offsets, and scaling are applied. |
| Lines and common vector shapes | Supported | Line, polyline, polygon, rectangle, round rectangle, ellipse, and pixel. |
| Arc / pie / chord | Partial | Projection, direction, mapping inversion, closure, and full ellipses are unit-tested and covered by broad project-owned Windows reference matrices; exhaustive historical combinations are not claimed. |
| PolyPolygon | Supported | One compound path preserves ALTERNATE/even-odd and WINDING/nonzero fill behavior; a GDI-produced nested-hole case is reference-compared. |
| TextOut / ExtTextOut | Partial | Charset decoding, distinct horizontal/vertical alignment, style, escapement, `dx`, UPDATECP, clipping and opaque rectangles are supported; font metrics and no-`dx` advance remain approximate and diagnostic. |
| Rectangular clipping | Partial | Intersect and saved/restored rectangular clips are supported; exclusion/region clips are diagnostic-only. |
| BI_RGB DIB | Partial | 1/4/8/24/32-bit decoding, palettes, orientation, row padding, signed source crop, destination mirroring, and STRETCHDIB are supported. COLORONCOLOR is pixelated; other stretch modes are approximate or unsupported. |
| Other bitmap transfers/compression | Diagnostic-only | Safely skipped in permissive mode and rejected in strict mode. |
| Solid/null brushes | Supported | Hatch, pattern, and DIB-pattern brush fidelity is diagnostic-only. |
| Raster operations / META_ESCAPE | Diagnostic-only | SRCCOPY bitmap transfer is supported; other operations are not emulated. |
| Ordinary EMF | Partial | Validated headers, world/mapping transforms, common vectors/paths, transformed polygon/path clips, Unicode/ANSI text, and affine BI_RGB StretchDIBits are rendered. Arbitrary-affine text and non-uniform geometric pens remain explicit approximations. See `docs/emf-record-audit.md`. |
| EMF+ | Common document subset | EMF+ Only and Dual streams use dedicated playback for vectors/paths, state and affine transforms, boolean regions, Unicode text, PNG/JPEG/raw images, textures, and linear gradients. Text metrics and selected layout behavior are approximate. Path gradients and advanced pens remain explicit P2 gaps. Classic records are used only for explicit, diagnostic `GetDC`/legacy-empty compatibility forms—not as a fallback for unsupported drawing. |

Unknown records are safely skipped with capped, aggregated diagnostics in
permissive mode. In strict mode, an unknown operation or a known operation
whose semantics would be skipped or materially approximated fails with a typed
`UnsupportedCriticalFeature` error. Benign informational diagnostics do not
fail strict rendering.

Standard WMFs have their SVG viewBox inferred from emitted drawing extents.
Files with no drawable operations receive a 1x1 fallback viewBox and an
explicit diagnostic that bounds are unreliable. Compatibility is qualified
against a provenance-controlled corpus. The committed corpus
contains project-generated Windows GDI outputs plus an ignored, maintainer-owned
private Office corpus. Public redistributable Office evidence remains limited.
Passing generated or private cases is not evidence of pixel-perfect Windows
GDI equivalence.

## Architecture

```text
metafile-wmf / metafile-emf / metafile-emfplus -> stateful playback
                                      -> metafile-core Renderer events
metafile-svg                         -> deterministic SVG
metafile facade composes playback + SVG rendering
bindings/wasm: Uint8Array/bytes -> structured metadata/render result
```

See [docs/architecture.md](docs/architecture.md),
[docs/dependencies.md](docs/dependencies.md), and the evidence-based
[WMF](docs/wmf-record-audit.md), [EMF](docs/emf-record-audit.md), and
[EMF+](docs/emfplus-record-audit.md) record audits.

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
responsible for initialization; the engine itself performs no fetch,
filesystem, DOM, Canvas, or Node operations. This repository does not publish
an npm package; `emf-to-png` bundles qualified generated artifacts from the
tagged engine release.

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
[docs/release-readiness.md](docs/release-readiness.md) for the evidence gates
used for 1.0.1 and the remaining public-corpus limitation.

Contributions are welcome when they are driven by reproducible compatibility,
security, performance, or documentation issues. Read
[CONTRIBUTING.md](CONTRIBUTING.md) before submitting code or fixtures.

## License

Licensed under the [Apache License 2.0](LICENSE).
