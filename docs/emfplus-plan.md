# EMF+ implementation plan (historical)

This document records the design plan used to implement EMF+ before the 1.0.1
release. It is retained for architectural context, not as a current roadmap.
See `emfplus-record-audit.md` and `post-1.0-backlog.md` for current status.

This document records the staged design. P0 now has an initial implementation;
the exact implemented/approximate/unsupported status is maintained in
`emfplus-record-audit.md`. P1 and P2 remain roadmap work.

## Current-core fit

The renderer already provides format-neutral points, vectors, affine
transforms, multi-figure line/cubic paths, fill rules, alpha-bearing colors and
images, affine image placement, text runs, and rectangular/polygon/path/
intersection clips. These remain shared.

`Brush`, `Pen`, `Font`, and `DeviceContext` currently model classic GDI. The
EMF+ player should own its object table/state and resolve those objects to
renderer concepts at draw time instead of adding raw EMF+ flags to core.
Before richer objects are emitted, the renderer vocabulary must grow to cover:

- paint: solid, hatch, texture, linear gradient, then path gradient;
- stroke: paint source, width/unit, caps, joins, dash sequence/offset, miter
  limit, compound lines, and custom caps;
- image draw: alpha image, source rectangle, affine destination, wrap mode,
  interpolation, and image attributes;
- text layout: layout rectangle, alignment, trimming, wrapping, direction,
  typographic mode, and transformed baseline;
- clip composition: replace, intersect, union, exclude, complement, and xor;
- quality state: compositing mode/quality, smoothing, interpolation, pixel
  offset, and text rendering hint.

Paint/images should use deterministic renderer resources, never SVG syntax in
playback. Exact ownership types are deferred until object records are
implemented and testable; adding speculative variants now would harden guesses
into the public core API.

## Parsing and framing

EMF+ data is carried by `EMR_GDICOMMENT` payloads beginning with `EMF+`.
Validate comment length, every 12-byte record header, record/data sizes,
alignment, object continuations, and aggregate allocation with checked
arithmetic. Multiple comment blocks form one ordered stream. Unknown records
are bounded and diagnostic; malformed framing is an error. Inspection retains
header version/flags and record summaries.

EMF+ Only and Dual require distinct stream policy. Dual playback must not be
presented as full fidelity just because ordinary-EMF fallback records exist.
Any future fallback option must be explicit in options, diagnostics and result
metadata.

## P0 — common document fidelity

- Header, EndOfFile, GetDC, Clear, and format classification.
- Bounded object table and continuation handling for solid/hatch/linear
  gradient brushes, pens, paths, images, fonts, string formats, and regions.
- Save/Restore and BeginContainer/BeginContainerNoParams/EndContainer with
  correctly scoped state and bounded nesting.
- world transform operations, page transform/unit, and container-local
  transforms with tested multiplication order.
- reset/set/intersect clipping for rectangles, paths, and regions.
- FillPath/DrawPath plus lines, polygons, rectangles, ellipses, arcs, pies,
  Beziers, and closed curves through renderer-neutral paths.
- DrawImage/DrawImagePoints with source rectangles, alpha, affine placement,
  wrap mode, and interpolation diagnostics.
- DrawString with font/string-format objects, layout rectangle, alignment,
  Unicode, and explicit metric limitations.
- compositing, smoothing, interpolation, pixel-offset, and text-rendering state
  rendered when possible and otherwise diagnosed.
- deterministic output, strict/permissive behavior, and explicit Dual policy.

## P1 — important compatibility

- texture brushes, gradient wrap/blend/preset colors, brush-backed pens, dash
  patterns/offsets, miter limits, compound lines, and standard caps;
- region boolean trees and union/exclude/xor/complement clipping;
- image attributes, color matrices, gamma, color keys, remap tables, and common
  bitmap pixel formats;
- DrawDriverString, trimming/wrapping, vertical/RTL layout;
- path-gradient representation and common focus/wrap behavior;
- inspection summaries for versions, objects, Dual content, record categories,
  and unsupported-feature counts.

## P2 — uncommon or advanced

- custom line caps, adjustable arrows, metafile/image recursion, effects,
  advanced path-gradient focus scales, and rare driver-specific records;
- exact Windows font fallback/hinting and color-management equivalence, which
  may remain explicitly approximate.

## Limits, diagnostics, fuzzing, and reference strategy

Extend limits for EMF+ records/comment bytes, object fragments, region nodes,
gradient stops, dash entries, strings, decoded image bytes, container depth,
and recursive image/metafile depth. Diagnostics retain outer EMF and inner
EMF+ record indices/types/offsets. Repeated unsupported features are capped and
aggregated.

Fuzz comment reassembly, object fragments/table replacement, transforms,
paths, regions, images, strings, Save/Restore, and containers. Invariants stay:
no panic, undefined behavior, infinite loop, or uncontrolled allocation.

Windows qualification starts with project-owned GDI+ EMF+ Only and Dual files,
then redistribution-cleared Office fixtures. Separate vector, text, gradient/
texture, image, clipping, transform, and Dual-policy cases prevent broad pixel
tolerances from hiding geometric failures.
