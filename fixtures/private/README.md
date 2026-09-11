# Private qualification corpus

Place non-redistributable customer or document-derived metafiles here, or set
`METAFILE_FIXTURE_DIR` to an external corpus directory. Everything in this
directory except this README is ignored by Git. Run
`node scripts/classify-corpus.mjs` to inventory committed and private inputs.
Never move a private binary into `real-world/` without recording permission and
provenance in `fixtures/manifest.json`.
