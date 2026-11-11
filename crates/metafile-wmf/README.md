# metafile-wmf

WMF parsing, inspection, and renderer-independent playback for `metafile-rs`.
The crate supports both Aldus Placeable WMF and standard WMF. It maintains
GDI-like graphics state and emits format-neutral renderer events.

Use it for WMF-specific metadata or a custom renderer. The high-level
`metafile` crate provides automatic format detection and SVG output.

Unknown or approximate operations produce structured diagnostics in permissive
mode and typed failures in strict mode. Resource limits bound untrusted input.

Licensed under the Apache License 2.0.
