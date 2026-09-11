# 0.1.0 release-readiness gates

This checklist records evidence, not intent. Local results do not mark remote
GitHub-hosted jobs or reference-renderer qualification as complete.

- [ ] GitHub CI green on Linux, macOS, and Windows
- [x] Rust 1.88 MSRV commands pass locally
- [x] current stable (1.98.1 during this pass) commands pass locally and are in CI
- [x] `wasm32-unknown-unknown` release build passes locally
- [x] generated browser-target WASM bindings execute in Node with byte input
- [x] randomized malformed-input regression completes without a panic
- [x] bounded cargo-fuzz run completed for inspect, permissive, and strict paths
- [ ] real-world fixture corpus populated with redistribution provenance
- [ ] trusted reference-render comparisons completed
- [x] common map modes have specification-derived unit coverage
- [ ] arc/pie/chord qualified against a trusted Windows reference
- [x] PolyPolygon compound paths and fill rules have unit coverage
- [x] common BI_RGB DIB structures and malformed cases have unit coverage
- [x] structured WASM error contract exercised at runtime
- [x] README compatibility table reviewed against implementation
- [x] production dependency purposes documented
- [x] no external WMF renderer or converter dependency
- [x] workspace forbids unsafe Rust

Before release, populate the corpus and record the reference comparison results.
Re-review stable error codes and public facade types after that evidence exposes
any remaining fidelity gaps.
