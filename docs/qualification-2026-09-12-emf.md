# EMF qualification — 2026-09-12

- Seven project-owned Windows `EmfOnly` scenarios were generated: vectors,
  mapping/world transforms, Unicode text, cubic paths, StretchDIBits, and saved
  DC/object/clip state, plus a release-hardening affine case with rotated and
  sheared geometry, path clipping, left/right transform composition, and a raw
  transformed SRCCOPY DIB. All inspect and render deterministically.
- Release-mode, 100-iteration diagnostic timings on the Docker-backed local
  host measured: small WMF 256.608/14.229 µs, 5,002-record WMF
  398.725/21,825.476 µs, small EMF 118.774/151.987 µs, affine EMF
  151.067/272.881 µs, and bitmap EMF 160.761/216.033 µs for mean
  inspect/render respectively. These are local diagnostics, not comparative
  benchmarks.
- Windows GDI+ reference and Chromium SVG rasters were manually reviewed for
  geometry, direction, text content, clipping, and bitmap orientation. SVG
  playback now derives its canvas from `rclFrame` and the header device/mm
  relationship, matching Windows frame placement while retaining `rclBounds`
  as inspection-level ink bounds. Fresh comparisons for all seven ordinary EMF
  fixtures had a maximum content-bounds delta of two pixels. The vector, text,
  bitmap, and mixed affine cases now use enforced CI profiles.
- A seeded libFuzzer/sanitizer run of the combined WMF/EMF inspect, permissive,
  and strict render entry points completed 18,411 executions in 61 seconds. It
  reached 2,173 coverage counters and 5,697 features at 436 MiB peak RSS, with
  no crash, panic, timeout, or sanitizer finding.
- Current local comparison ranges were 0.282291-5.084443 mean absolute error,
  4.967005-16.698566 RMS error, and 0.683750%-18.817500% differing pixels.
  Bitmap interpolation and host-font differences still require their broader
  purpose-specific profiles.
