# Architecture

`metafile-core` owns format-neutral geometry, GDI-like state, graphics objects,
resource limits, diagnostics, errors, render options, and renderer events.
`metafile-wmf` and `metafile-emf` own bounded format parsing, header validation,
stateful playback, inspection, text decoding, and DIB decoding. They depend on
core, not on an output backend. `metafile-svg` independently consumes
renderer events and serializes deterministic self-contained SVG. The small
`metafile` facade composes WMF playback with SVG and preserves the convenient
`to_svg` API. `metafile-wasm` maps byte slices and serializable Rust values to
JavaScript values through that facade.

The facade owns a format-neutral `MetafileInfo` envelope and nests
parser-specific metadata, so adding EMF does not require changing the
top-level inspection/render result shape.

Metafile records never append SVG directly. Playback mutates a complete device
context and emits mapped graphics primitives. This boundary is intended for
future EMF+ playback and other renderer backends.

Affine world transforms, generic line/cubic path figures, fill-rule-aware polygon/path clip
regions, and affine bitmap placement are format-neutral core concepts. EMF
converts boxes, ellipses, and arcs to paths before applying arbitrary affine
transforms. EMF uses 32-bit object handles internally while emitting the same
pen, brush, font, text, bitmap, clip, primitive, and path events as WMF.

Points and vectors have distinct mapping operations. Origins affect points and
rectangles, while pen widths, text advances, radii, and other extents receive
scale only. Output bounds are renderer-owned and include vector stroke width;
text bounds remain explicitly approximate.

## Pre-EMF+ core audit

Reusable without format leakage: `Transform`, `Path`/figures/cubic segments,
fill rules, alpha `Color`/`Bitmap`, affine `BitmapPlacement`, `BitmapSampling`,
text positioning, and rectangular/polygon/path/intersection clips. EMF+
playback can therefore reuse geometry, transforms, image placement, limits,
diagnostics, and the renderer boundary.

Classic `Brush`, `Pen`, `Font`, and `DeviceContext` are deliberately not being
treated as a complete GDI+ model. Paint-backed pens, gradients/textures,
source-image rectangles and attributes, richer string layout, region boolean
operations, containers, page units, and rendering-quality state need
format-neutral renderer concepts during the dedicated EMF+ pass. Their exact
ownership/resource representation is deferred until real object records drive
tests; speculative EMF+ flags are not added to shared types. See
`docs/emfplus-plan.md` for the P0/P1/P2 design boundary.
