# WMF record qualification audit

This inventory was reviewed against the MS-WMF record definitions during the
0.1.0 qualification pass. “Rendered” means a first-party renderer event is
emitted. “Approximate” is never intended to imply Windows pixel identity.

| Record | Parsed | Played/rendered status | Qualification and limits |
| --- | --- | --- | --- |
| `META_EOF` | yes | state/control | Unit-tested; missing EOF is diagnostic. |
| `META_SAVEDC`, `META_RESTOREDC` | yes | full DC snapshot/restore | Unit-tested for relative/absolute restore, object selections, mapping, position and clipping. |
| `META_SETBKCOLOR`, `META_SETTEXTCOLOR` | yes | state | Unit-tested through text/opaque output. |
| `META_SETBKMODE` | yes | state | TRANSPARENT/OPAQUE used by text; invalid values currently behave as transparent. |
| `META_SETTEXTALIGN` | yes | rendered | LEFT/CENTER/RIGHT and TOP/BOTTOM/BASELINE are unit-tested; UPDATECP is deterministic but approximate without `dx`; RTL reading is diagnostic/strict-error. |
| `META_SETMAPMODE` | yes | state | MM_TEXT, metric, English, TWIPS, isotropic and anisotropic mapping are unit-tested. Fixed physical modes use 96 SVG units/inch. |
| window/viewport origin, extent, offset and scale records | yes | state | Signed values, negative extents, vector-vs-point mapping, zero denominators and isotropic adjustment are unit-tested. |
| `META_SETROP2` | yes | state/diagnostic | R2_COPYPEN is accepted; other vector raster operations are unsupported. |
| `META_SETPOLYFILLMODE` | yes | rendered | ALTERNATE/even-odd and WINDING/nonzero compound paths are tested. |
| `META_SETSTRETCHBLTMODE` | yes | state | No diagnostic until a scaled bitmap consumes it. COLORONCOLOR maps to pixelated interpolation; HALFTONE is approximate; monochrome modes are unsupported. |
| `META_SETMAPPERFLAGS` | yes | state | No diagnostic until text consumes font matching; then diagnostic/strict-error because host-specific font matching is unavailable. |
| `META_CREATEPENINDIRECT` | yes | object | Width-zero cosmetic and mapped geometric widths are distinct and tested. Solid/null are rendered; dash arrays and INSIDEFRAME are explicit approximations; unknown styles are unsupported. |
| `META_CREATEBRUSHINDIRECT` | yes | object | Solid/null rendered. Hatch, pattern and unknown brushes are diagnostic/strict-error and treated as transparent in permissive mode. |
| `META_CREATEFONTINDIRECT` | yes | object | Common logical-font fields and code-page selection are preserved; Windows font selection/metrics are approximate. |
| `META_SELECTOBJECT`, `META_DELETEOBJECT` | yes | object | First-free allocation, stock objects, invalid handles, selected/saved selection lifetime and reuse are tested. |
| `META_MOVETO`, `META_LINETO` | yes | rendered | Current position and mapped endpoints are tested. Non-copy ROP2 is not emulated. |
| `META_POLYLINE`, `META_POLYGON`, `META_POLYPOLYGON` | yes | rendered | Signed points and fill rules are tested; a real GDI PolyPolygon reference case is included. |
| `META_RECTANGLE`, `META_ROUNDRECT`, `META_ELLIPSE` | yes | rendered | Mapped bounds/radii and null paint objects are supported. Exact GDI bottom/right edge raster inclusion is rasterizer-dependent. |
| `META_ARC`, `META_PIE`, `META_CHORD` | yes | rendered | Endpoints are projected, large-arc/closure are implemented, full ellipse uses two SVG arcs, and compatible-mode direction is not inverted by mirrored mapping. A limited Windows case is compared; the exhaustive inversion/quadrant matrix remains incomplete. |
| `META_SETPIXEL` | yes | rendered | Emits a one-unit SVG rectangle; device-pixel coverage is backend-dependent. |
| `META_TEXTOUT`, `META_EXTTEXTOUT` | yes | rendered/approximate | Encoding, escaping, alignment, escapement, `dx`, ETO_OPAQUE and ETO_CLIPPED are tested. Font metrics and no-`dx` advancement are diagnostic approximations. |
| `META_INTERSECTCLIPRECT` | yes | rendered | Rectangular intersection and SaveDC restoration are tested. |
| `META_EXCLUDECLIPRECT` | yes | diagnostic-only | Rectangular-only clip model cannot express exclusion. |
| `META_STRETCHDIB` | yes | rendered subset | SRCCOPY/DIB_RGB_COLORS and BI_RGB 1/4/8/24/32 are supported. Palette, padding, orientation, signed crop and destination mirroring are tested. Other ROPs/compression are diagnostic/strict-error. |
| `META_DIBBITBLT`, `META_DIBSTRETCHBLT`, `META_SETDIBTODEV` | yes | diagnostic-only | Safely skipped in permissive mode and rejected in strict mode. |
| `META_ESCAPE` | yes | diagnostic-only | Escape payload is bounded and skipped; printer-driver behavior is intentionally unsupported. |
| all other functions | bounded raw payload | diagnostic-only | Unknown records are skipped with capped aggregation or rejected in strict mode. |

The parser uses each record's specified parameter order, including reverse Y/X
ordering for point-like state records and normal X/Y order for polygon arrays.
This audit corrected mirrored arc direction, zero-width pen scaling, text
baseline mapping, `TA_UPDATECP`, and signed bitmap crop/destination behavior.
