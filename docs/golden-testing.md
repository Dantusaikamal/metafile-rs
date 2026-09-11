# Golden rendering workflow

Golden assets must follow `fixtures/manifest.schema.json` and have documented
redistribution permission. Generate a reference PNG with a trusted renderer on
a controlled machine and record its renderer/version and image provenance.
Render the same WMF through metafile-rs, rasterize its SVG using the chosen test
rasterizer, then compare the two PNGs:

```bash
cargo run -p metafile-golden -- --profile vector --metrics metrics.json --difference difference.png reference.png candidate.png
```

The comparator reports dimensions, non-transparent bounds, mean absolute
channel error, RMS channel error, and differing-pixel percentage. Optional
profiles provide conservative first-pass gates:

| Profile | MAE | RMS | differing pixels | maximum bounds delta |
| --- | ---: | ---: | ---: | ---: |
| vector | 8 | 30 | 15% | 3 px |
| bitmap | 10 | 35 | 25% | 3 px |
| text | 15 | 50 | 35% | 8 px |

Profiles differ because font and bitmap interpolation vary between GDI+, SVG
rasterizers, and downstream consumers. A passing metric never replaces visual
review. The optional difference image stores absolute RGB error in red.
Generated assets belong under ignored `artifacts/qualification/`; exact PNG
bytes are not a fidelity metric. This tool is not linked into the engine.
