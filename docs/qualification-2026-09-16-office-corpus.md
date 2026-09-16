# Private Office corpus qualification — 2026-09-16

This document contains aggregate, non-sensitive evidence only. Source
documents, extracted media, hashes, references, candidates, differences, and
per-file manifests remain under the ignored `private-office-corpus/` tree.

## Corpus

| Evidence | Count |
| --- | ---: |
| Office source documents | 3 |
| legacy DOC | 2 |
| DOCX | 1 |
| PPT | 0 |
| PPTX | 0 |
| extracted WMF | 7 |
| extracted ordinary EMF | 7 |
| extracted EMF+ Only | 6 |
| extracted EMF+ Dual | 2 |
| extracted metafiles total | 22 |

Two legacy documents are maintainer-owned private qualification inputs; one is
independently sourced and the other has no independently attested provenance.
The DOCX is a project-generated extraction control and is not independent
evidence. OOXML media and legacy OfficeArt BLIPs were copied/inflated without
Office/LibreOffice re-save or image re-encoding, and SHA-256 hashes are stored
in the private manifests.

## Results

- 21 of 22 extracted metafiles inspect and render permissively.
- One project-generated ordinary EMF is structurally malformed: its
  `EMR_CREATEBRUSHINDIRECT` record declares only 12 payload bytes and omits the
  required hatch value. Both modes retain the bounded `TruncatedInput` error.
- The private EMF+ Dual chart passes Windows comparison using the text profile
  (bounds delta 1, MAE 4.98, RMS 29.40).
- Six ordinary EMFs and six compatibility-form EMF+ Only files from a legacy
  Office document pass Windows vector comparison after non-square device-pixel
  mapping was preserved in SVG.
- Six standard WMFs from that document pass content-frame-normalized Windows
  comparison. Standard WMF carries no intrinsic frame; raw Windows output adds
  host margins while the engine intentionally infers drawing bounds. Normalized
  MAE is 0.06–0.23 and RMS is 1.10–2.88.
- The project-generated WMF control is visually equivalent but retains a small
  inclusive-edge/rasterization approximation; no threshold was loosened.

Strict rendering rejects the real noncanonical EMF+ compatibility forms where
material approximations are present. Those expected strict failures are
recorded separately from malformed input in the private manifest.

## Real-file fixes

1. A compact 12-byte `SetPageTransform` with `DataSize == 0` is accepted only
   in permissive mode, only when its flags otherwise form a valid page-unit
   record. A structured `emfplus_noncanonical_set_page_transform` diagnostic
   is emitted and strict mode rejects it. Windows rendered canonical scale 1.0
   and compact controlled variants byte-identically, confirming the effective
   scale rather than assuming it.
2. EMF high-bit stock pseudo-handles now cover defined brushes, pens, fonts,
   and the default palette without consuming or corrupting object-table slots.
3. State-only EMF+ Dual `GetDC` delegation and the observed empty-only legacy
   producer form select classic records through explicit, diagnostic policies;
   unsupported EMF+ drawing never triggers fallback.
4. SVG declares `preserveAspectRatio="none"`, preserving metafile device
   mapping when logical and physical frame aspect ratios differ.

## Regression and safety evidence

- 153 Rust tests pass: 56 WMF, 54 ordinary EMF, 29 EMF+, and 14 shared/facade
  tests.
- The unchanged project-owned Windows differential matrix passes 21/21: six
  WMF, eight ordinary EMF, and seven EMF+ cases.
- Rust 1.88 MSRV and current-stable format, Clippy `-D warnings`, tests, and
  required release builds pass locally.
- Fresh wasm-bindgen 0.2.128 output passes Node and headless-browser smokes.
- The format-neutral inspect/permissive/strict fuzz target completed 23,380
  executions in 91 seconds with coverage 2,344, feature count 6,546, no crash,
  panic, timeout, or sanitizer finding.

## Remaining evidence boundary

Private real-Office validation is complete for this corpus. No public,
redistributable, independently sourced Office corpus was added, and no PPT or
PPTX input was available. Hosted CI must still run against the final release
commit after maintainer review and push.
