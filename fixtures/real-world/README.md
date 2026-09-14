# Real-world fixtures

Only fixtures with documented redistribution permission may be placed here.
For every binary, record its source, author, license, acquisition date, and a
brief description of the features it exercises. Synthetic unit fixtures remain
programmatically constructed in Rust and are reported separately.

Every added file must also have an entry in `../manifest.json` conforming to
`../manifest.schema.json`. Reference images require their own provenance and
the exact trusted renderer/version used to produce them.

The `windows-gdi-*.wmf` and `windows-emf-*.emf` files are project-owned qualification assets generated
through an actual Windows GDI metafile device context by
`tools/windows-reference`. They are small, redistributable under the repository
license, and are “real GDI-produced” rather than hand-assembled test bytes.
They are not a substitute for a broad, independently sourced Office corpus.

For maintainer-owned DOCX/PPTX files, use
`scripts/extract-office-metafiles.ps1`. It extracts only `word/media` and
`ppt/media` WMF/EMF entries and creates a reviewable manifest fragment with
source-document and extracted-file SHA-256 hashes. EMF+ comments inside `.emf`
members are classified as `emfplus` and marked unsupported. Do not commit the
results until provenance and redistribution permission have been verified.
