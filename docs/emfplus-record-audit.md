# EMF+ record audit

This audit describes playback, not merely parsing. Unsupported semantics are
diagnosed in permissive mode and rejected in strict mode when used. Unsupported
object definitions retain their object-table slots and do not fail unless a
visible record selects them. EMF+ Dual uses its EMF+ stream; the classic EMF
fallback is never silently substituted.

## Implemented and rendered

- framing: Header, EndOfFile, object continuation/replacement, and correct
  EMF+ Only versus Dual classification from the Header record D flag;
- state: Save/Restore, Begin/EndContainer (including source units), page/world
  transform set/reset/multiply/translate/scale/rotate, and non-commutative
  transform ordering;
- vectors: Clear, Fill/DrawRects, FillPolygon, DrawLines, DrawBeziers,
  Fill/DrawEllipse, Fill/DrawPie, DrawArc, Fill/DrawPath, DrawCurve,
  DrawClosedCurve, and FillClosedCurve;
- objects: solid/hatch/linear-gradient/texture brushes, common pens, paths,
  recursive RegionNode trees, raw and encoded bitmaps, fonts, StringFormat,
  and the serialized ImageAttributes wrap/clamp object;
- clipping: reset, offset, path/rectangle/region clipping, and Replace,
  Intersect, Union, Exclude, Complement, and XOR boolean combinations;
- images: raw 24/32-bit pixels plus PNG and JPEG, integral source crop,
  affine DrawImagePoints placement, mirroring, common interpolation modes,
  and bounded wrap/clamp sampling;
- text: Unicode DrawString, family/size/style and unit conversion, horizontal
  and vertical alignment, NoWrap, deterministic wrapping/trimming, RTL flags,
  layout clipping, multiline block centering, and full affine transforms;
- brushes/strokes: deterministic texture patterns and Tile/TileFlip modes,
  transformed linear gradients, preset colors, one-dimensional blend factors,
  gamma-correction selection, brush-backed pens, standard equal caps, joins,
  miter, dash style/pattern/offset, and pen width units.

The five project-owned P1 fixtures exercise these paths against Windows GDI+.
They supplement the original EMF+ Only and Dual fixtures.

## Approximate, always diagnostic where material

- SVG host fonts and metrics cannot be pixel-identical to Windows GDI+;
- StringFormat shaping, word-boundary trimming, and several uncommon direction
  flags use deterministic host-independent approximations;
- fractional image source rectangles expand to whole pixels;
- non-solid DrawString paint uses a representative color because the shared
  text vocabulary does not support painting glyphs with a brush;
- quality/compositing/smoothing/pixel-offset/text-rendering modes without an
  exact SVG equivalent are retained and diagnosed;
- supported hatch styles are deterministic SVG equivalents, not identical GDI+
  device-pixel patterns.

## Explicitly unsupported (P2 unless corpus evidence promotes them)

- PathGradientBrush (parsed as a bounded unsupported object; never replaced by
  a misleading radial gradient);
- simultaneous two-dimensional linear-gradient blend factors;
- texture Clamp edge behavior beyond the supported image-attribute sampling;
- pen transforms, non-flat dash caps, differing start/end caps, compound lines,
  custom caps, and adjustable arrows;
- recursive metafile image objects and encoded image formats other than PNG or
  JPEG;
- raw indexed/16-bit EMF+ bitmap layouts;
- DrawDriverString, glyph-index shaping, vertical scripts, and exact Windows
  line breaking;
- effects and unknown visible records.

The serialized MS-EMFPLUS ImageAttributes object contains Version, Reserved,
WrapMode, ClampColor, and ObjectClamp fields. GDI+ color adjustments such as a
ColorMatrix are commonly applied by the producer while serializing image data;
they are not invented as undocumented trailing ImageAttributes fields. Any
nonzero extension bytes are rejected explicitly.

## Safety and qualification

Malformed inner headers, sizes, fragments, paths, transforms, images, regions,
and strings return typed errors with outer EMF and inner EMF+ context. Limits
bound comment/record/object bytes, points, strings, gradient stops, dash data,
region nodes/depth, state/container depth, decoded pixels, diagnostics, and SVG
output. PNG/JPEG dimensions are checked before accepting decoded allocations.

Local Windows qualification covers 21 total WMF/EMF/EMF+ cases, including
seven EMF+ cases. Independent Office-derived EMF+ evidence and hosted CI remain
separate, explicitly unchecked gates.
