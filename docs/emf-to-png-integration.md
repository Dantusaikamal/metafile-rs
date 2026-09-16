# `emf-to-png` integration contract

## Boundary

```text
Buffer / Uint8Array
  -> metafile-rs WASM initialization
  -> inspectMetafile(bytes) or metafileToSvg(bytes, options)
  -> deterministic SVG + metadata + diagnostics
  -> @resvg/resvg-js
  -> PNG, or PNG-to-JPEG encoding
```

`metafile-rs` owns byte validation, format detection, WMF/EMF/EMF+ playback,
logical and physical bounds, SVG generation, typed failures, and rendering
diagnostics. `emf-to-png` continues to own Node `Buffer`, filesystem access,
requested output dimensions, fit, DPI policy, background compositing,
PNG/JPEG encoding, fallback images, logging, and CLI behavior.

The byte-to-SVG integration surface is frozen. Current
P2 fidelity gaps (notably EMF+ path gradients, custom/compound pen caps, and
exact Windows text shaping) remain structured diagnostics/strict errors and do
not require exposing parser internals to JavaScript.

## API mapping

- `inspect()` calls `inspectMetafile(Uint8Array)` and maps the stable structured
  metadata envelope without format-specific parser imports.
- `emfOrWmfToSvg()` calls `metafileToSvg`; despite its legacy name it can route
  WMF, ordinary EMF, and EMF+ while preserving its existing return contract.
- `convert()` calls `metafileToSvg`, then passes the SVG to the existing resvg
  sizing/background/raster pipeline.
- `convertFile()` remains a filesystem wrapper around `convert()`.

The compatibility exports `inspectWmf` and `wmfToSvg` remain WMF-only and are
not the integration surface for the converter.

## Initialization and concurrency

Create one cached initialization Promise per JavaScript realm. All callers
await that same Promise. Once initialized, inspection and rendering are
synchronous, CPU-bound calls over `Uint8Array`; do not instantiate a module per
conversion. Worker threads or Web Workers require one instance/cache per
worker. The engine has no shared mutable global renderer state.

## Errors and diagnostics

Preserve the WASM error object's stable `code`, `message`, and available record
or byte context. Map codes to the existing typed Node errors at the package
boundary; retain the original code/cause so new parser errors are not flattened
to generic conversion failures. Diagnostics are non-fatal structured entries
and should be returned/logged according to existing package policy. Strict mode
turns materially unsupported semantics into failures; permissive mode emits a
diagnostic and safely skips or documents an approximation.

## Published integration

`emf-to-png` 1.0.0 implements this contract with one cached WASM
initialization promise per JavaScript realm. WMF, ordinary EMF, EMF+ Only, and
EMF+ Dual all route through `metafile-rs`; the legacy `libemf2svg` runtime and
build machinery have been removed. The npm package vendors qualified generated
artifacts from `metafile-rs` 1.0.1, so installation and runtime do not require
this source repository, Rust, or network access.

## Browser future

```text
Uint8Array -> metafile-rs WASM -> SVG -> resvg-wasm -> Blob
```

No engine change is required: it has no Node, filesystem, DOM, Canvas, fetch,
network, or external-converter dependency.
