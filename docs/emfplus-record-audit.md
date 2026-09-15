# EMF+ record audit

This audit describes playback, not merely parsing. EMF+ is an initial subset;
unsupported semantics are diagnosed in permissive mode and rejected in strict
mode. EMF+ Dual always uses the EMF+ stream, never an implicit classic-EMF
fallback.

## Rendered

- framing: Header, EndOfFile, object continuation/replacement;
- state: Save/Restore, Begin/EndContainer, page/world transform set/reset,
  multiply, translate, scale, and rotate;
- vectors: Clear, Fill/DrawRects, FillPolygon, DrawLines, DrawBeziers,
  Fill/DrawEllipse, Fill/DrawPie, DrawArc, Fill/DrawPath;
- objects: solid and common hatch/linear-gradient brushes, common pens, paths,
  simple rectangular regions, raw ARGB bitmaps, fonts, and basic StringFormat;
- clipping: reset, rectangular/path/simple-region replacement or intersection,
  and offset;
- images: DrawImage and DrawImagePoints with affine destination placement for
  supported raw bitmaps;
- text: DrawString Unicode content, family, size/style, and horizontal/vertical
  layout alignment.

## Approximate or state-only

- host SVG font selection and metrics are not pixel-identical to Windows GDI+;
- non-solid text paint falls back to a representative color with a diagnostic;
- rendering quality, compositing, smoothing, interpolation, pixel-offset, and
  text-rendering modes are retained where useful but do not all have exact SVG
  equivalents;
- supported hatch styles use deterministic SVG pattern equivalents rather than
  promising pixel-identical GDI+ sampling.

## Explicitly unsupported

- texture and path-gradient brushes; advanced linear-gradient blend behavior;
- advanced pen flags, compound lines, custom caps, and differing start/end caps;
- boolean region trees beyond simple empty/infinite/rectangle cases;
- compressed images, non-supported pixel formats, image attributes/color
  matrices, recursive metafile images, and effects;
- DrawDriverString, glyph-index/vertical/RTL/trimming/wrapping fidelity;
- closed curves and other records not listed in the rendered section.

Malformed inner headers, sizes, fragments, paths, transforms, images, and
strings return `InvalidEmfPlus` with outer EMF and inner EMF+ context. Resource
limits bound comments, records, objects, points, strings, gradients, dash data,
regions, state/container depth, decoded pixels, diagnostics, and SVG output.

## Qualification status

The committed project-owned EMF+ Only and Dual fixtures exercise a pen,
ellipse, solid fill, font, and Unicode DrawString. Synthetic unit tests cover
additional state, path, brush, clip, bitmap, malformed-input, strict/permissive,
and determinism behavior. A broad Windows GDI+ comparison corpus and an
independently sourced Office corpus remain unchecked release gates.
