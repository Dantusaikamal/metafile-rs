# Golden rendering workflow

Golden assets must follow `fixtures/manifest.schema.json` and have documented
redistribution permission. Generate a reference PNG with a trusted renderer on
a controlled machine and record its renderer/version and image provenance.
Render the same WMF through metafile-rs, rasterize its SVG using the chosen test
rasterizer, then compare the two PNGs:

```bash
cargo run -p metafile-golden -- reference.png candidate.png
```

The comparator reports dimensions, non-transparent bounds, mean absolute
channel error, RMS channel error, and differing-pixel percentage. Thresholds
belong in each fixture's review record; exact compressed PNG bytes are not a
fidelity metric. The comparator is development-only and is not linked into the
metafile engine.
