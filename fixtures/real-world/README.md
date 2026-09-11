# Real-world fixtures

Only fixtures with documented redistribution permission may be placed here.
For every binary, record its source, author, license, acquisition date, and a
brief description of the features it exercises. The initial test suite uses
small WMFs constructed in Rust so their provenance and semantics are explicit.

Every added file must also have an entry in `../manifest.json` conforming to
`../manifest.schema.json`. Reference images require their own provenance and
the exact trusted renderer/version used to produce them. An empty manifest is
intentional until redistributable, known-origin samples are available.
