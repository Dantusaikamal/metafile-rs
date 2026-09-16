# Post-1.0 compatibility backlog

Items here are explicit compatibility work, not silently claimed support.
Promote an item to a release blocker if independent Office corpus evidence
shows it is common.

## P2 rare compatibility

- faithful EMF+ PathGradientBrush rendering;
- pen transforms, custom/compound caps, differing endpoint caps, and non-flat
  dash caps;
- recursive metafile image objects, less-common encoded image formats, and
  indexed/16-bit raw EMF+ pixels;
- DrawDriverString, glyph-index/vertical shaping, and closer Windows line
  breaking;
- two-dimensional linear-gradient blends and uncommon GDI+ effects;
- additional historical WMF/EMF printer, region, raster-operation, and
  compressed-bitmap records already reported by strict/permissive playback.

## Post-1.0 bugfix qualification

- add independently redistributable Word/DOCX and PowerPoint/PPTX WMF, EMF,
  and EMF+ samples when maintainers can provide them with provenance;
- retain the ignored private Office corpus as a regression gate; the initial
  three-document run did not promote any remaining P2 feature to a release
  blocker;
- add a regression fixture for every real-file rendering or security defect;
- tune performance only from measured representative workloads.

## Ongoing qualification

Every maintenance release must pass the configured GitHub-hosted Linux,
macOS, Windows, WASM, browser, and Windows-reference jobs. Remaining items in
this document are not release blockers unless reproducible corpus evidence
shows that they affect common document workloads.
