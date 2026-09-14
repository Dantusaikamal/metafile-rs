# EMF record audit

Status describes playback fidelity, not merely successful parsing. The audit
covers records exercised by committed fixtures and focused unit tests.

## Fully rendered

- State consumed faithfully by supported operations: `EOF`, `SAVEDC`,
  `RESTOREDC`, `SETBKMODE`, `SETBKCOLOR`, `SETTEXTCOLOR`, `SETTEXTALIGN`,
  `SETMAPMODE`, `SETPOLYFILLMODE`, and `SETARCDIRECTION`.
- Mapping: window/viewport origin and extent, scale extent,
  `SETWORLDTRANSFORM`, and all four `MODIFYWORLDTRANSFORM` modes. Left/right
  multiplication order is independently regression-tested.
- Objects: `CREATEPEN`, solid `EXTCREATEPEN` (including standard end caps and
  joins), solid/null
  `CREATEBRUSHINDIRECT`, `EXTCREATEFONTINDIRECTW`, `SELECTOBJECT`,
  `DELETEOBJECT`, and common stock objects. Deleting a table handle preserves
  the already-realized object value in current/saved DC state, matching common
  GDI+ output and permitting deterministic handle reuse.
- Vectors: move/line, polyline/polygon/polypolygon and 16-bit variants,
  PolylineTo, PolyBezier/PolyBezierTo and 16-bit variants, rectangle,
  round-rectangle, ellipse, arc/ArcTo, pie, chord, and SetPixelV.
- Paths: Begin/End/AbortPath, CloseFigure, FillPath, StrokePath,
  StrokeAndFillPath, and SelectClipPath with `RGN_COPY`/`RGN_AND`.
- Affine geometry: rectangular and elliptical primitives become generic cubic
  or line paths before arbitrary world transforms are applied. Bitmap
  placement uses a renderer-neutral affine basis. IntersectClipRect becomes an
  exact transformed convex polygon.
- Bitmap: StretchDIBits with SRCCOPY, DIB_RGB_COLORS, BI_RGB 1/4/8/24/32-bit
  pixels, palettes, row padding, orientation, crop, scale, mirroring, rotation,
  and shear.

## Rendered with documented approximation

- ExtTextOutW/A content, selected fonts, color, explicit horizontal advances,
  common alignment, UPDATECP, ETO_OPAQUE, and ETO_CLIPPED are rendered.
  Host-dependent font selection/metrics and records without explicit advances
  are diagnostic approximations. Arbitrary affine glyph-coordinate transforms
  are positioned but diagnosed in permissive mode and rejected in strict mode.
- HALFTONE bitmap stretch maps to smooth SVG interpolation with a diagnostic.
- Geometric pens under non-uniform scale or shear use a scalar stroke width
  with a diagnostic; strict mode rejects the approximation.

## Parsed/state-only or diagnostic-only

- `SETROP2`, `SETSTRETCHBLTMODE`, `SETMAPPERFLAGS`, ICM mode, and other
  mapper/raster state are retained and consumed where supported or diagnosed;
  they do not emulate Windows color management or arbitrary raster composition.
- `EXCLUDECLIPRECT`, non-AND/COPY path-clip combine modes,
  `SETDIBITSTODEVICE`, custom miter limits, hatch/pattern brushes, non-solid
  or user-style extended pens, monochrome stretch modes, and non-SRCCOPY operations are
  diagnostic-only in permissive mode and fail when strict playback would
  materially differ.

## Unsupported or rejected

- Compressed and BITFIELDS DIBs, palette-index DIB usage, complex region
  records, ETO_PDY vertical advances, glyph-index text, RTL layout, and unknown
  material drawing records are not rendered. Malformed lengths, offsets,
  object handles, paths, transforms, comments, and DIB payloads return typed
  errors.
- EMF+ records embedded in GDI comments are classified as
  `MetafileFormat::EmfPlus`. Inspection succeeds; playback always fails
  explicitly and never silently selects an ordinary-EMF fallback.
