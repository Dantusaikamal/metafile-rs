# 0.1.0 qualification evidence — 2026-09-11

Base: `1f82bbf8d1863ed90f9a62a94ce223812d7f2939`.

## Corpus

- Six project-owned WMFs were emitted by an actual Windows GDI metafile DC:
  vector primitives, arc/pie/chord, PolyPolygon, text, StretchDIBits, and a
  5,000-record stress stream.
- One EMF and one EMF+ Dual sample were emitted by GDI+ and are classified only;
  the Rust facade returns `UnsupportedFormat` for both as intended.
- Two sibling `emf2png` EMFs were classified locally. Their redistribution
  provenance was not established, so they were not copied or committed.
- Word automation was available but did not complete reliably in this
  environment. No Word/DOCX-extracted WMF is claimed.

## Windows comparisons

Windows `System.Drawing`/GDI+ replayed placeable WMFs into a white 600x400 PNG;
Chrome rasterized deterministic SVG at the same size. Metrics are MAE / RMS /
differing-pixel percentage. Visual diffs were generated under ignored
`artifacts/qualification/`.

| Fixture | Metrics | Assessment |
| --- | --- | --- |
| arcs | 0.618014 / 9.586706 / 0.860833% | close; raster edge differences only in the covered case |
| PolyPolygon | 1.191973 / 12.418120 / 1.706667% | close; compound fill visually agrees |
| text | 0.654733 / 11.395009 / 0.697083% | visually aligned for covered defaults; font metrics remain approximate |
| vector | 0.912168 / 9.881850 / 1.567500% | geometry and colors visually agree |
| bitmap | 14.409085 / 36.828574 / 25.695417% | not a fidelity pass: GDI+ interpolates the whole metafile while candidate COLORONCOLOR remains pixelated |
| 5,000-line stress | 29.178441 / 67.709570 / 65.318750% | geometry visually agrees; GDI+ is aliased while browser strokes are antialiased, so pixel metrics are not a pass |

The bitmap discrepancy is an oracle-method limitation, not silently waived by
a threshold. A direct pixel-scale GDI playback oracle is still needed. The arc
case does not cover the full requested quadrant/inverted-axis matrix.

## Fuzz, performance, and WASM

- cargo-fuzz ran inspect plus permissive and strict render paths for 121 seconds
  with the committed real corpus as seeds: 21,895 executions, 1,998 coverage
  edges, 5,182 features, no crash/hang, and 468 MiB peak fuzzer RSS.
- Release-mode repeated timings (Docker Linux on this Windows host): vector
  22.327 µs inspect / 20.597 µs render-to-SVG; text 21.235 / 23.778 µs;
  bitmap 18.150 / 23.164 µs; 5,002-record vector stream 365.998 µs inspect /
  24.869 ms render-to-711,108-byte SVG. Separate playback-vs-serialization
  timing and process peak memory were not instrumented.
- Cargo release WASM was 1,008,534 bytes; wasm-bindgen's browser artifact was
  541,737 bytes and 281,485 bytes with gzip -9. Both Node and real headless
  Chrome runtime tests passed.

## API audit

- `metafile-wmf::WmfInfo` remains parser-specific.
- `metafile::{inspect,to_svg}` expose format-neutral `MetafileInfo` with a
  tagged `FormatSpecificInfo` payload.
- `Pen::cosmetic`, typed text alignments, and `BitmapSampling` make material
  renderer semantics explicit.
- `Renderer` and `DeviceContext` remain public in core as the intentional
  parser/backend extension boundary; private record types remain internal.

## Release decision

The implementation is materially stronger, but the evidence does **not** yet
support releasing 0.1.0 as trustworthy for arbitrary WMFs extracted from Word.
The blockers are an owned/redistributable Word corpus, broader independent WMFs,
the exhaustive arc reference matrix, and a direct GDI bitmap oracle that avoids
whole-metafile interpolation.
