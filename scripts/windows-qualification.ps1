param([string]$Artifacts = 'artifacts/qualification')

$ErrorActionPreference = 'Stop'
New-Item -ItemType Directory -Path $Artifacts -Force | Out-Null
cargo build -p metafile --example render --release
cargo build -p metafile-golden --release

$cases = @(
    @{ Name = 'windows-gdi-vector'; Profile = 'vector' },
    @{ Name = 'windows-gdi-arcs'; Profile = 'vector' },
    @{ Name = 'windows-gdi-polypolygon'; Profile = 'vector' },
    @{ Name = 'windows-gdi-text'; Profile = 'text' },
    @{ Name = 'windows-gdi-bitmap'; Extension = 'wmf'; Profile = $null },
    # Windows DrawImage maps the full EMF frame while SVG intentionally uses rclBounds;
    # collect metrics and images, but do not apply framing-sensitive WMF thresholds.
    @{ Name = 'windows-gdiplus-emf'; Extension = 'emf'; Profile = $null },
    @{ Name = 'windows-emf-mapping'; Extension = 'emf'; Profile = $null },
    @{ Name = 'windows-emf-text'; Extension = 'emf'; Profile = $null },
    @{ Name = 'windows-emf-paths'; Extension = 'emf'; Profile = $null },
    @{ Name = 'windows-emf-bitmap'; Extension = 'emf'; Profile = $null },
    @{ Name = 'windows-emf-state'; Extension = 'emf'; Profile = $null }
)

foreach ($case in $cases) {
    $name = $case.Name
    $extension = if ($case.Extension) { $case.Extension } else { 'wmf' }
    $input = "fixtures/real-world/$name.$extension"
    & target/release/examples/render.exe $input "$Artifacts/$name.svg" "$Artifacts/$name-diagnostics.json"
    & tools/windows-reference/reference.ps1 -Action render -Path $input -Output "$Artifacts/$name-reference.png"
    node scripts/rasterize-svg.mjs "$Artifacts/$name.svg" "$Artifacts/$name-candidate.png" 600 400
    $arguments = @(
        '--metrics', "$Artifacts/$name-metrics.json",
        '--difference', "$Artifacts/$name-difference.png"
    )
    if ($case.Profile) { $arguments += @('--profile', $case.Profile) }
    $arguments += @("$Artifacts/$name-reference.png", "$Artifacts/$name-candidate.png")
    & target/release/metafile-golden.exe @arguments
}
