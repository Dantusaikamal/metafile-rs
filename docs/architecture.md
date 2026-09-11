# Architecture

`metafile-core` owns format-neutral geometry, GDI-like state, graphics objects,
resource limits, diagnostics, errors, render options, and renderer events.
`metafile-wmf` owns bounded binary parsing, private typed records, WMF header
validation, stateful playback, inspection, text decoding, and DIB decoding. It
depends on core, not on an output backend. `metafile-svg` independently consumes
renderer events and serializes deterministic self-contained SVG. The small
`metafile` facade composes WMF playback with SVG and preserves the convenient
`to_svg` API. `metafile-wasm` maps byte slices and serializable Rust values to
JavaScript values through that facade.

The facade owns a format-neutral `MetafileInfo` envelope and nests
parser-specific metadata, so adding EMF does not require changing the
top-level inspection/render result shape.

WMF records never append SVG directly. Playback mutates a complete device
context and emits mapped graphics primitives. This boundary is intended for
future EMF and EMF+ parsers.

Points and vectors have distinct mapping operations. Origins affect points and
rectangles, while pen widths, text advances, radii, and other extents receive
scale only. Output bounds are renderer-owned and include vector stroke width;
text bounds remain explicitly approximate.
