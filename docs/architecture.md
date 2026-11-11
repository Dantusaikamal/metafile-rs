# Architecture

`metafile-core` contains the geometry, graphics state, objects, limits,
diagnostics, errors, options, and renderer events shared by all formats.
`metafile-wmf`, `metafile-emf`, and `metafile-emfplus` parse their respective
record streams and play them into a `metafile-core::Renderer`. DIB decoding is
shared by WMF and EMF through `metafile-dib`.

The format crates do not depend on an output backend. `metafile-svg` consumes
renderer events and writes self-contained SVG, while the `metafile` facade
handles format detection and joins parsing, playback, and SVG output behind
`inspect` and `to_svg`. `metafile-wasm` exposes that facade to JavaScript.

The facade owns a format-neutral `MetafileInfo` envelope and nests
parser-specific metadata. WMF, ordinary EMF, and EMF+ therefore share the
same top-level inspection and render result shape.

EMF+ records are reassembled across `EMR_GDICOMMENT` boundaries before bounded
inner-record parsing. EMF+ owns its 64-slot object table, graphics state,
Save/Restore/container stacks, page/world transforms, and Dual policy. The EMF
container routes a detected EMF+ stream to this player. Classic records are
played only for an explicit state-only `GetDC` delegation or the narrow
diagnostic empty-Only compatibility form; unsupported drawing records never
cause an implicit fallback.

Metafile records never append SVG directly. Playback mutates a complete device
context and emits mapped graphics primitives. WMF, EMF, and EMF+ all use this
boundary, which also permits future renderer backends.

Affine world transforms, generic line/cubic path figures, fill-rule-aware
polygon/path clip regions, and affine bitmap placement are format-neutral core
concepts. EMF converts boxes, ellipses, and arcs to paths before applying
arbitrary affine transforms. EMF uses 32-bit object handles internally while
emitting the same pen, brush, font, text, bitmap, clip, primitive, and path
events as WMF.

Points and vectors have distinct mapping operations. Origins affect points and
rectangles, while pen widths, text advances, radii, and other extents receive
scale only. Output bounds are renderer-owned and include vector stroke width;
text bounds remain explicitly approximate.

## Shared renderer vocabulary

Reusable without format leakage: `Transform`, `Path`/figures/cubic segments,
fill rules, alpha `Color`/`Bitmap`, affine `BitmapPlacement`, `BitmapSampling`,
text positioning, and rectangular/polygon/path/intersection clips. EMF+
playback reuses geometry, transforms, image placement, limits,
diagnostics, and the renderer boundary.

Classic `Brush`, `Pen`, `Font`, and `DeviceContext` remain GDI types. EMF+ owns
its richer objects and resolves them at draw time to format-neutral `Paint`,
`Stroke`, `Path`, `TextRun`, `ClipRegion`, and affine `BitmapPlacement` values.
Core currently represents solid, hatch, linear-gradient, and texture paint;
rich stroke style; affine images; and boolean clip regions. Path gradients,
broader image attributes, and some advanced string-layout concepts are not yet
part of the shared renderer vocabulary.
