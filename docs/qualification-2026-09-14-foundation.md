# Pre-EMF+ foundation qualification — 2026-09-14

## Qualification runner

The cancelled hosted run exposed only the enclosing PowerShell command, because
the old runner printed neither case nor stage and imposed no child-process
deadline. The exact child that stalled therefore cannot be recovered from the
available log. The unbounded synchronous Chromium screenshot invocation was
the principal hang risk: it had no timeout, reused the default browser profile,
and its parent PowerShell invocation also had no way to terminate the browser
tree. Repeated `dotnet run` calls rebuilt/started the reference tool for every
case and added another unbounded stage.

The runner now starts every external process through one timed wrapper, logs
case/stage duration, captures stdout/stderr, kills the entire process tree on
timeout, writes `qualification-failure.txt`, and exits nonzero. Rust builds use
600-second limits; each render/reference/raster/compare stage uses 120 seconds.
The job has a 30-minute outer limit and still uploads artifacts under
`if: always()`. The compiled .NET DLL is reused. Chromium has an independent
60-second timeout and unique temporary profile.

The former hosted stall's exact stage remains **BLOCKED** until a new hosted run
produces instrumented output. Local reference/raster/compare stages complete in
seconds; no timeout was reproduced.

## Broad arc matrix

Project-owned Windows generators added paired WMF and ordinary-EMF cases for
90°, 180°, greater-than-180°, near-full, quadrant, non-square, pie/chord,
clockwise/counterclockwise, coincident-endpoint, and representative degenerate
behavior. EMF additionally reference-qualifies inverted X, inverted Y, and both
axes. WMF inversion remains unit-tested; the GDI+ WMF oracle did not preserve
the constructed viewport-inversion setup reliably, so that claim is not made
from the WMF reference image.

Results at 600×400 using the vector profile:

| Fixture | Bounds delta | MAE | RMS | Different pixels | Result |
| --- | ---: | ---: | ---: | ---: | --- |
| `windows-gdi-arc-matrix.wmf` | 1 px | 1.172320 | 12.614951 | 1.783333% | PASS |
| `windows-emf-arc-matrix.emf` | 1 px | 0.831944 | 9.089094 | 2.037083% | PASS |

The complete 14-case suite (six WMF and eight ordinary EMF cases) was rerun
through candidate rendering, Windows reference rendering, Chromium
rasterization, and profiled comparison. All 14 passed. Across the suite the
maximum bounds delta was 2 px; the largest pixel errors remain the established
WMF bitmap interpolation case (MAE 14.409085, RMS 36.828574, 25.695417%
different), within its bitmap profile.

## Corpus status

No redistribution-cleared independent Office-derived WMF or EMF was found.
Counts remain zero and both gates are blocked. The local controlled DOCX
contains one WMF, one ordinary EMF, and one EMF+ member under `word/media`.
Extraction discovered all three, preserved bytes, produced SHA-256 entries,
and classified them as `wmf/render`, `emf/render`, and
`emfplus/unsupported`. Because its media is project-generated, this validates
tooling only and is not real-Office fidelity evidence.

## Safety and performance sanity

A 63-second combined fuzz run completed 4,057 executions, 2,174 coverage
counters and 5,738 features at 403 MiB peak RSS with no crash, panic, timeout,
or sanitizer finding.

Contended Docker release timings over 100 iterations were: small WMF
469.093/17.289 µs, 5,002-record WMF 723.991/25,413.059 µs, small EMF
241.615/241.382 µs, affine EMF 200.531/353.478 µs, and bitmap EMF
257.743/274.501 µs for inspect/render respectively. The large stream remained
linear and no hang or pathological memory growth was observed. These are sanity
measurements taken while other validation containers were compiling, not
marketing benchmarks or a direct regression comparison.
