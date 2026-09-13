# EMF record audit

Status reflects playback, not merely parsing.

- Rendered state: EOF, SaveDC/RestoreDC, background/text colors and modes,
  text alignment, map/fill/ROP/stretch modes, mapper flags, arc direction.
- Rendered mapping: window/viewport origin and extent, scale extent,
  SetWorldTransform and ModifyWorldTransform.
- Rendered objects: CreatePen, ExtCreatePen (solid), CreateBrushIndirect,
  ExtCreateFontIndirectW, SelectObject, DeleteObject, common stock objects.
- Rendered vectors: MoveToEx, LineTo, Polyline/Polygon/PolyPolygon and 16-bit
  forms, PolylineTo, PolyBezier/PolyBezierTo and 16-bit forms, rectangle,
  round rectangle, ellipse, arc/ArcTo, pie, chord, SetPixelV.
- Rendered paths: BeginPath, EndPath, AbortPath, CloseFigure, FillPath,
  StrokePath, StrokeAndFillPath with line and cubic segments.
- Rendered text: ExtTextOutW and ExtTextOutA, explicit advances, common
  alignment, UPDATECP, opaque background and rectangular clipping. Host font
  metrics, fallback fonts, ETO_PDY, and no-advance positioning are approximate.
- Rendered bitmap: StretchDIBits with SRCCOPY, DIB_RGB_COLORS, BI_RGB
  1/4/8/24/32-bit pixels, palettes, row padding, orientation, crop, scale, and
  signed destination mirroring.
- Diagnostic/strict failure: ExcludeClipRect, SetDIBitsToDevice, non-solid
  extended pens, non-SRCCOPY operations, compressed DIBs, custom miter limits,
  unsupported region/path clipping, and unknown records.
- Classified only: EMF+ streams inside GDI comments. EMF+ is never silently
  rendered as ordinary EMF.
