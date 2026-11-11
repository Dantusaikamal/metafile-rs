# metafile-emf

Ordinary EMF parsing, inspection, and renderer-independent playback for
`metafile-rs`. The crate validates EMF records, maintains GDI graphics state,
handles affine transforms, and routes embedded EMF+ streams to
`metafile-emfplus`.

Use it for EMF-specific metadata or a custom renderer. The high-level
`metafile` crate provides automatic format detection and SVG output.

Approximate and unsupported behavior is explicit through diagnostics and
strict-mode errors. Resource limits bound untrusted input.

Licensed under the Apache License 2.0.
