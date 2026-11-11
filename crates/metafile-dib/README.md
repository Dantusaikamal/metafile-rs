# metafile-dib

Device Independent Bitmap (DIB) decoding shared by the WMF and EMF players.
The decoder checks headers, dimensions, palettes, row layout, orientation, and
allocation sizes before producing renderer-neutral bitmap data.

This is an implementation-support crate published for use by the other
`metafile-rs` crates. Applications should normally depend on `metafile`.

Licensed under the Apache License 2.0.
