param(
    [string]$Artifacts = 'artifacts/qualification',
    [int]$StageTimeoutSeconds = 120,
    [int]$BuildTimeoutSeconds = 600
)

$ErrorActionPreference = 'Stop'
$artifactsPath = [IO.Path]::GetFullPath($Artifacts)
New-Item -ItemType Directory -Path $artifactsPath -Force | Out-Null

function Invoke-QualificationStage {
    param(
        [Parameter(Mandatory = $true)][string]$Label,
        [Parameter(Mandatory = $true)][string]$FilePath,
        [Parameter(Mandatory = $true)][string[]]$Arguments,
        [Parameter(Mandatory = $true)][int]$TimeoutSeconds
    )

    $timer = [Diagnostics.Stopwatch]::StartNew()
    $startInfo = [Diagnostics.ProcessStartInfo]::new()
    $startInfo.FileName = $FilePath
    $startInfo.UseShellExecute = $false
    $startInfo.RedirectStandardOutput = $true
    $startInfo.RedirectStandardError = $true
    $startInfo.WorkingDirectory = (Get-Location).Path
    if ($null -ne $startInfo.ArgumentList) {
        foreach ($argument in $Arguments) { [void]$startInfo.ArgumentList.Add($argument) }
    }
    else {
        # Windows PowerShell 5 uses .NET Framework and lacks ArgumentList.
        # Qualification arguments never contain embedded quotes or end in a
        # backslash, so quoting every argument is deterministic here.
        $startInfo.Arguments = ($Arguments | ForEach-Object {
            if ($_ -match '"' -or $_.EndsWith('\')) { throw "unsupported native argument: $_" }
            '"' + $_ + '"'
        }) -join ' '
    }
    $process = [Diagnostics.Process]::new()
    $process.StartInfo = $startInfo
    try {
        if (-not $process.Start()) { throw "could not start $FilePath" }
        $stdout = $process.StandardOutput.ReadToEndAsync()
        $stderr = $process.StandardError.ReadToEndAsync()
        if (-not $process.WaitForExit($TimeoutSeconds * 1000)) {
            try { $process.Kill($true) }
            catch {
                try { $process.Kill() }
                catch { Write-Warning "Could not terminate process: $_" }
            }
            $process.WaitForExit()
            $message = "$Label TIMEOUT after $([Math]::Round($timer.Elapsed.TotalSeconds, 1))s (limit ${TimeoutSeconds}s)"
            Add-Content -LiteralPath (Join-Path $artifactsPath 'qualification-failure.txt') -Value $message
            throw $message
        }
        $out = $stdout.GetAwaiter().GetResult()
        $err = $stderr.GetAwaiter().GetResult()
        if ($out) { $out.TrimEnd() -split "`r?`n" | ForEach-Object { Write-Host "    $_" } }
        if ($err) { $err.TrimEnd() -split "`r?`n" | ForEach-Object { Write-Host "    $_" } }
        if ($process.ExitCode -ne 0) {
            $message = "$Label FAIL $([Math]::Round($timer.Elapsed.TotalSeconds, 1))s (exit $($process.ExitCode))"
            Add-Content -LiteralPath (Join-Path $artifactsPath 'qualification-failure.txt') -Value $message
            throw $message
        }
        Write-Host ("  {0,-22} PASS {1:N1}s" -f $Label, $timer.Elapsed.TotalSeconds)
    }
    finally { $process.Dispose() }
}

Invoke-QualificationStage -Label 'metafile build' -FilePath 'cargo' -Arguments @('build', '-p', 'metafile', '--example', 'render', '--release') -TimeoutSeconds $BuildTimeoutSeconds
Invoke-QualificationStage -Label 'comparator build' -FilePath 'cargo' -Arguments @('build', '-p', 'metafile-golden', '--release') -TimeoutSeconds $BuildTimeoutSeconds

$referenceDll = [IO.Path]::GetFullPath('tools/windows-reference/bin/Release/net8.0-windows/MetafileReference.dll')
if (-not (Test-Path -LiteralPath $referenceDll)) {
    Invoke-QualificationStage -Label 'reference build' -FilePath 'dotnet' -Arguments @('build', 'tools/windows-reference/MetafileReference.csproj', '--configuration', 'Release', '--nologo') -TimeoutSeconds $BuildTimeoutSeconds
}

$cases = @(
    @{ Name = 'windows-gdi-vector'; Profile = 'vector' },
    @{ Name = 'windows-gdi-arcs'; Profile = 'vector' },
    @{ Name = 'windows-gdi-arc-matrix'; Profile = 'vector' },
    @{ Name = 'windows-gdi-polypolygon'; Profile = 'vector' },
    @{ Name = 'windows-gdi-text'; Profile = 'text' },
    @{ Name = 'windows-gdi-bitmap'; Extension = 'wmf'; Profile = 'bitmap' },
    @{ Name = 'windows-gdiplus-emf'; Extension = 'emf'; Profile = 'text' },
    @{ Name = 'windows-emf-mapping'; Extension = 'emf'; Profile = 'vector' },
    @{ Name = 'windows-emf-text'; Extension = 'emf'; Profile = 'text' },
    @{ Name = 'windows-emf-paths'; Extension = 'emf'; Profile = 'vector' },
    @{ Name = 'windows-emf-bitmap'; Extension = 'emf'; Profile = 'bitmap' },
    @{ Name = 'windows-emf-state'; Extension = 'emf'; Profile = 'vector' },
    @{ Name = 'windows-emf-affine'; Extension = 'emf'; Profile = 'bitmap' },
    @{ Name = 'windows-emf-arc-matrix'; Extension = 'emf'; Profile = 'vector' },
    @{ Name = 'windows-gdiplus-emfplus-only'; Extension = 'emf'; Profile = 'text' },
    @{ Name = 'windows-gdiplus-emfplus'; Extension = 'emf'; Profile = 'text' }
)

for ($index = 0; $index -lt $cases.Count; $index++) {
    $case = $cases[$index]
    $name = $case.Name
    $extension = if ($case.Extension) { $case.Extension } else { 'wmf' }
    $input = [IO.Path]::GetFullPath("fixtures/real-world/$name.$extension")
    Write-Host "[$($index + 1)/$($cases.Count)] $name"
    Invoke-QualificationStage -Label 'metafile render' -FilePath ([IO.Path]::GetFullPath('target/release/examples/render.exe')) -Arguments @($input, (Join-Path $artifactsPath "$name.svg"), (Join-Path $artifactsPath "$name-diagnostics.json")) -TimeoutSeconds $StageTimeoutSeconds
    Invoke-QualificationStage -Label 'reference render' -FilePath 'dotnet' -Arguments @($referenceDll, 'render', $input, (Join-Path $artifactsPath "$name-reference.png"), '600', '400') -TimeoutSeconds $StageTimeoutSeconds
    Invoke-QualificationStage -Label 'SVG rasterization' -FilePath 'node' -Arguments @('scripts/rasterize-svg.mjs', (Join-Path $artifactsPath "$name.svg"), (Join-Path $artifactsPath "$name-candidate.png"), '600', '400') -TimeoutSeconds $StageTimeoutSeconds
    Invoke-QualificationStage -Label 'comparison' -FilePath ([IO.Path]::GetFullPath('target/release/metafile-golden.exe')) -Arguments @('--metrics', (Join-Path $artifactsPath "$name-metrics.json"), '--difference', (Join-Path $artifactsPath "$name-difference.png"), '--profile', $case.Profile, (Join-Path $artifactsPath "$name-reference.png"), (Join-Path $artifactsPath "$name-candidate.png")) -TimeoutSeconds $StageTimeoutSeconds
}
