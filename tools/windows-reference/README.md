# Windows reference oracle

This development-only tool generates WMFs through the Windows GDI metafile DC
and renders them through the Windows `System.Drawing`/GDI+ metafile playback
path into a white, 600x400 PNG. It is not a workspace crate or an engine
runtime dependency. It is intended only as a qualification oracle on Windows.

```powershell
pwsh tools/windows-reference/reference.ps1 -Action generate -Path fixtures/real-world
pwsh tools/windows-reference/reference.ps1 -Action render -Path fixtures/real-world/windows-gdi-vector.wmf -Output artifacts/reference.png
```

The generator adds a project-owned Aldus placeable header around the standard
WMF bytes emitted by GDI. The generator also creates project-owned EMF and EMF+
Dual samples through GDI+ for future-format corpus/classification coverage.
The harness is a Windows-only .NET 8 project using the Windows Desktop
`System.Drawing.Common` framework. It is qualification tooling only and is
not a Rust runtime dependency. The engine renders ordinary EMF; EMF+ remains
classification/inspection-only and is explicitly rejected for playback.
