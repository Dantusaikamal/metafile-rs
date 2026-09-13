# EMF qualification — 2026-09-12

- Six project-owned Windows `EmfOnly` scenarios were generated: vectors,
  mapping/world transforms, Unicode text, cubic paths, StretchDIBits, and saved
  DC/object/clip state. All inspect and render deterministically.
- The existing release timing example reported 20,104–30,347 µs inspection
  and 12,712–15,501 µs rendering for the 528–1,852 byte EMF fixtures in the
  Docker-backed local environment. These are local measurements, not a
  comparison with another engine.
- Windows GDI+ reference and Chromium SVG rasters were manually reviewed for
  geometry, direction, text content, clipping, and bitmap orientation. Numeric
  thresholds are not applied because Windows `DrawImage` maps the complete EMF
  frame while the SVG output intentionally uses `rclBounds` as its viewBox.
- A seeded sanitizer fuzz run completed 34,966 executions in 61 seconds without
  a crash, timeout, or sanitizer finding.
