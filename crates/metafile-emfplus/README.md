# metafile-emfplus

EMF+ stream inspection and renderer-independent playback for `metafile-rs`.
It handles EMF+ Only and Dual streams, object/state playback, paths,
transforms, clipping, text, images, textures, and common brushes. Known
fidelity limits are returned as diagnostics or strict-mode errors.

`metafile-emf` normally extracts and frames the EMF+ stream. Applications that
only need format detection and SVG output should use `metafile`.

Parsing and playback are bounded by configurable resource limits and use no
platform graphics API.

Licensed under the Apache License 2.0.
