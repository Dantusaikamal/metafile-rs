# metafile

`metafile` is the high-level API for reading Windows Metafiles and rendering
them as SVG. It accepts WMF, ordinary EMF, EMF+ Only, and EMF+ Dual files.

```rust
use metafile::{inspect, to_svg};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let bytes = std::fs::read("drawing.wmf")?;
    let info = inspect(&bytes)?;
    println!("{info:#?}");

    let result = to_svg(&bytes, Default::default())?;
    std::fs::write("drawing.svg", result.svg)?;
    Ok(())
}
```

The SVG contains its bitmap resources and uses stable resource identifiers, so
the same input and options produce the same output. Rendering works in native
Rust and `wasm32-unknown-unknown` without GDI/GDI+, Office, an external
converter, Canvas, or network access. JavaScript bindings live in the
non-published `metafile-wasm` workspace crate.

## Results and diagnostics

`inspect` returns format-neutral metadata with WMF- or EMF-specific details.
`to_svg` returns the SVG, the same metadata envelope, and structured non-fatal
diagnostics. Permissive rendering is the default. Strict mode rejects material
approximations or skipped semantics:

```rust
use metafile::{to_svg, RenderOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let bytes = std::fs::read("drawing.wmf")?;
    let options = RenderOptions {
        strict: true,
        ..RenderOptions::default()
    };
    let result = to_svg(&bytes, options)?;
    std::fs::write("drawing.svg", result.svg)?;
    Ok(())
}
```

`ResourceLimits` controls input size, record and object counts, bitmap
dimensions, graphics-state depth, diagnostics, and SVG output. The workspace
forbids unsafe Rust.

## Compatibility

The supported subset is aimed at document-generated WMF, EMF, and EMF+ files.
Historical printer records, uncommon raster operations, and exact Windows text
metrics are not universally reproduced. Permissive mode reports those cases;
strict mode rejects material fidelity loss.

Lower-level consumers can use `metafile-wmf`, `metafile-emf`,
`metafile-emfplus`, `metafile-core`, and `metafile-svg` directly.

## License

Licensed under the Apache License 2.0.
