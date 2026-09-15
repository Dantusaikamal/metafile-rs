# 0.1.0 release-readiness gates

This checklist records evidence, not intent. Local results do not mark remote
GitHub-hosted jobs or reference-renderer qualification as complete.

| Format | Engine implementation | Windows reference | Independent Office | Verdict |
| --- | --- | --- | --- | --- |
| WMF | Common document subset implemented and regression-tested | Project-owned matrix previously qualified; current hosted run pending | Missing | READY WITH DOCUMENTED LIMITATIONS |
| ordinary EMF | Common vectors/state/text/BI_RGB implemented and regression-tested | Project-owned matrix previously qualified; current hosted run pending | Missing | READY WITH DOCUMENTED LIMITATIONS |
| EMF+ | Common vectors/state/text/images/brushes/regions implemented; explicit P2 gaps remain | Seven project-owned GDI+ cases pass locally | Missing | READY WITH DOCUMENTED LIMITATIONS; hosted verification pending |

- [ ] GitHub CI green on Linux, macOS, and Windows for these qualification changes (the base commit was green)
- [x] Rust 1.88 MSRV commands pass locally
- [x] current stable (1.98.1 during this pass) commands pass locally and are in CI
- [x] `wasm32-unknown-unknown` release build passes locally
- [x] generated browser-target WASM bindings execute in Node with byte input
- [x] generated browser-target WASM bindings execute in headless Chrome locally
- [x] randomized malformed-input regression completes without a panic
- [x] bounded cargo-fuzz run completed for inspect, permissive, and strict paths
- [x] project-owned Windows-GDI/GDI+ corpus populated with redistribution provenance
- [x] eight project-owned ordinary EMF scenarios generated and deterministically rendered, including arbitrary affine and broad arc matrices
- [ ] independently sourced Office/DOCX WMF corpus populated
- [ ] independently sourced Office/DOCX EMF corpus populated
- [x] initial trusted Windows reference comparisons completed with local diff artifacts
- [x] frame-normalized ordinary-EMF Windows comparisons have enforced vector/text/bitmap profiles
- [x] common map modes have specification-derived unit coverage
- [x] broad WMF/EMF arc/pie/chord matrix qualified against Windows (full combinatorial proof is not claimed)
- [x] PolyPolygon compound paths/fill rules have unit and initial Windows-reference coverage
- [x] common BI_RGB DIB structures and malformed cases have unit coverage
- [x] WMF/EMF bitmap placement is reference-gated with interpolation-aware pixel tolerances and tight geometry bounds
- [x] structured WASM error contract exercised at runtime
- [x] initial EMF+ Only and Dual streams inspect and render through dedicated playback
- [x] EMF+ object/record framing, continuation, transform, clip, text, and bitmap regressions are bounded
- [x] seven-case EMF+ Windows GDI+ reference matrix passes locally (Only,
  Dual, vectors/texture/curves, PNG/JPEG/images, transformed text, boolean
  regions/gradient, and state/container/path)
- [ ] independently sourced Office/DOCX EMF+ corpus populated
- [x] README compatibility table reviewed against implementation
- [x] production dependency purposes documented
- [x] no external WMF renderer or converter dependency
- [x] no external EMF renderer or converter dependency
- [x] workspace forbids unsafe Rust

See `qualification-2026-09-11.md`, `qualification-2026-09-12-emf.md`,
`qualification-2026-09-14-foundation.md`, and
`qualification-2026-09-15-emfplus.md` for exact evidence and blockers. The
public facade uses a format-neutral metadata envelope. Publishing remains blocked
on GitHub-hosted CI confirmation for this change. Independent Office corpus
gates remain unverified qualification work, not fabricated release evidence.

The controlled local DOCX extraction workflow has been exercised with WMF,
ordinary EMF, and EMF+ media. It validates discovery, hashing, and
classification, but project-controlled data does not satisfy either independent
Office corpus gate. Common EMF+ playback now has project-owned Windows GDI+
differential evidence. PathGradient, advanced/custom pen behavior, exact text
shaping, and uncommon image effects remain explicit P2 compatibility work
rather than silent fallbacks. See `emfplus-record-audit.md` for the boundary.
