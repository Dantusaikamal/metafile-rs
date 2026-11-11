# metafile-svg

SVG renderer backend for `metafile-rs`. It implements
`metafile-core::Renderer` and emits paths, text, clips, masks, gradients,
patterns, and embedded raster data without external resources.

Pair it with a format player when building a custom pipeline. For the standard
byte-to-SVG path, use `metafile::to_svg`.

Licensed under the Apache License 2.0.
