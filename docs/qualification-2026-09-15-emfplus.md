# EMF+ qualification — 2026-09-15

Base: `ed6723fc032baf300721c66602341454dc53e107`.

## Windows differential

The local Windows 21-case suite completed without a timeout and all enforced
profiles passed. It contains six WMF, eight ordinary-EMF, and seven EMF+ cases.
Chrome was selected explicitly and each render/reference/raster/compare stage
ran under a hard timeout. Detailed PNG, difference, diagnostic, and metrics
artifacts were written under `artifacts/qualification-final-emfplus` and are
not committed.

New EMF+ evidence:

| Case | Profile | Bounds delta | MAE | RMS | Different pixels |
| --- | --- | ---: | ---: | ---: | ---: |
| vectors/texture/curves | vector | 0 | 5.316 | 29.175 | 10.218% |
| PNG/JPEG/images | bitmap | 1 | 5.882 | 27.830 | 13.390% |
| transformed text | text | 5 | 3.259 | 25.784 | 3.042% |
| boolean regions/gradient | gradient | 1 | 2.617 | 7.565 | 28.162% |
| state/container/path | vector | 1 | 0.239 | 3.470 | 1.303% |

The gradient profile keeps tight geometry, MAE, and RMS thresholds while
allowing one-channel interpolation rounding to mark many otherwise-correct
pixels as different. It is not used for non-gradient vector cases.

## Native and WASM

- Rust 1.88.0 GNU-host fmt, Clippy (`-D warnings`), tests, and release build:
  pass. GNU was used because this host has no Visual C++ linker/Windows SDK.
- stable 1.98.1 GNU-host fmt, Clippy (`-D warnings`), and tests: pass.
- 146 Rust tests pass, including 27 in `metafile-emfplus`.
- `wasm32-unknown-unknown` release build: pass.
- wasm-bindgen 0.2.128 generation, Node smoke, and headless Chrome smoke: pass.
- Windows .NET 8 reference project: builds with zero warnings/errors.

## Fuzzing and external evidence

The existing facade fuzz target covers inspect plus permissive/strict render
for all formats. A fresh 60-second libFuzzer campaign was run in the official
Rust nightly Linux container because this Windows host lacks the Visual C++
SDK/import libraries. It completed 13,671 executions (224/sec), reached 2,339
coverage counters and 6,452 features, added 37 corpus units, and reported no
crashes, panics, timeouts, or sanitizer findings. Peak RSS was 419 MiB while
loading and mutating the 436-file, approximately 2.3 MiB seed corpus; the
largest generated input was bounded to 50,056 bytes by the largest seed.

No independently sourced, redistributable Office corpus was available. The
untracked maintainer files in the working tree were not adopted as evidence or
committed. Project-owned GDI/GDI+ fixtures are useful differential evidence but
are not independent Office evidence.
