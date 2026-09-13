param(
    [Parameter(Mandatory = $true)][ValidateSet('generate', 'generate-affine', 'render')][string]$Action,
    [Parameter(Mandatory = $true)][string]$Path,
    [string]$Output,
    [int]$Width = 600,
    [int]$Height = 400
)

if ($Action -in @('generate', 'generate-affine')) {
    $arguments = @('run', '--project', (Join-Path $PSScriptRoot 'MetafileReference.csproj'), '--configuration', 'Release', '--no-launch-profile', '--', $Action, ([IO.Path]::GetFullPath($Path)))
} else {
    if (-not $Output) { throw 'render requires -Output' }
    $arguments = @('run', '--project', (Join-Path $PSScriptRoot 'MetafileReference.csproj'), '--configuration', 'Release', '--no-launch-profile', '--', 'render', (Resolve-Path -LiteralPath $Path).Path, [IO.Path]::GetFullPath($Output), $Width, $Height)
}

& dotnet @arguments
if ($LASTEXITCODE -ne 0) { throw "Windows reference tool failed with exit code $LASTEXITCODE" }
