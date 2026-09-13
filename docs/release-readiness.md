# 0.1.0 release-readiness gates

This checklist records evidence, not intent. Local results do not mark remote
GitHub-hosted jobs or reference-renderer qualification as complete.

- [ ] GitHub CI green on Linux, macOS, and Windows for these qualification changes (the base commit was green)
- [x] Rust 1.88 MSRV commands pass locally
- [x] current stable (1.98.1 during this pass) commands pass locally and are in CI
- [x] `wasm32-unknown-unknown` release build passes locally
- [x] generated browser-target WASM bindings execute in Node with byte input
- [x] generated browser-target WASM bindings execute in headless Chrome locally
- [x] randomized malformed-input regression completes without a panic
- [x] bounded cargo-fuzz run completed for inspect, permissive, and strict paths
- [x] project-owned Windows-GDI/GDI+ corpus populated with redistribution provenance
- [x] six project-owned ordinary EMF scenarios generated and deterministically rendered
- [ ] independently sourced Office/DOCX WMF corpus populated
- [x] initial trusted Windows reference comparisons completed with local diff artifacts
- [x] common map modes have specification-derived unit coverage
- [ ] exhaustive arc/pie/chord matrix qualified against Windows (initial case complete)
- [x] PolyPolygon compound paths/fill rules have unit and initial Windows-reference coverage
- [x] common BI_RGB DIB structures and malformed cases have unit coverage
- [x] structured WASM error contract exercised at runtime
- [x] README compatibility table reviewed against implementation
- [x] production dependency purposes documented
- [x] no external WMF renderer or converter dependency
- [x] no external EMF renderer or converter dependency
- [x] workspace forbids unsafe Rust

See `qualification-2026-09-11.md` for exact evidence and blockers. The public
facade now uses a format-neutral metadata envelope. Release remains blocked on
an independently sourced Word/DOCX corpus, exhaustive arc/inversion references,
and trustworthy bitmap reference scaling.
