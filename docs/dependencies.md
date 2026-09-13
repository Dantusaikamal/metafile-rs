# Runtime dependencies

- `thiserror`: typed Rust error declarations without handwritten boilerplate.
- `serde`: stable structured metadata, diagnostics, options, and WASM results.
- `encoding_rs`: maintained legacy Windows code-page decoding for WMF and EMF text.
- `png`: low-level encoding of validated DIB pixels for self-contained SVG.
- `base64`: data-URI encoding for embedded PNG resources.
- `wasm-bindgen`: standard `wasm32-unknown-unknown` JavaScript ABI bindings.
- `serde-wasm-bindgen`: direct structured conversion between Serde and JS.
- `js-sys`: constructs structured JavaScript error objects even when Serde
  serialization itself fails.

The golden-image command has a development-only direct dependency on `png` to
decode candidate and reference images. It is not linked into parser or playback
consumers.

The `metafile` crate uses `serde_json` only as a development dependency for the
fixture integration test and diagnostic-writing example. Node's standard
library drives browser/corpus scripts. The Windows reference oracle is C#
compiled on demand by PowerShell and uses Windows `System.Drawing`/GDI+; it is
not a Cargo dependency or distributable runtime component.

`metafile-dib` is a first-party internal workspace crate shared by WMF and EMF;
it is not an external dependency. No dependency parses, interprets, plays, or
renders WMF/EMF records.
