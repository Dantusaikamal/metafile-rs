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

The byte-to-SVG integration surface is frozen for the converter pass. Current
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

## Migration

1. Add a private renderer adapter initialized once and cover all existing Node
   API defaults with contract tests.
2. Route WMF to metafile-rs first while retaining the legacy ordinary-EMF
   renderer behind an explicit internal switch.
3. Compare SVG/raster results for the qualification corpus, then route ordinary
   EMF and supported EMF+.
4. If a temporary fallback remains, make its selection explicit in diagnostics;
   never present an EMF+ Dual ordinary-EMF fallback as full EMF+ fidelity.
5. Remove `libemf2svg` only after the published package matrix and real Office
   corpus gates pass.

## Browser future

```text
Uint8Array -> metafile-rs WASM -> SVG -> resvg-wasm -> Blob
```

No engine change is required: it has no Node, filesystem, DOM, Canvas, fetch,
network, or external-converter dependency.
