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
    @{ Name = 'windows-gdi-bitmap'; Profile = $null }
)

foreach ($case in $cases) {
    $name = $case.Name
    $input = "fixtures/real-world/$name.wmf"
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
