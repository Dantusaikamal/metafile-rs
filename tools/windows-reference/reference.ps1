param(
    [Parameter(Mandatory = $true)][ValidateSet('generate', 'render')][string]$Action,
    [Parameter(Mandatory = $true)][string]$Path,
    [string]$Output,
    [int]$Width = 600,
    [int]$Height = 400
)

$source = Join-Path $PSScriptRoot 'MetafileReference.cs'
Add-Type -Path $source -ReferencedAssemblies System.Drawing

if ($Action -eq 'generate') {
    [MetafileReference]::Generate((Resolve-Path -LiteralPath $Path).Path)
} else {
    if (-not $Output) { throw 'render requires -Output' }
    [MetafileReference]::Render(
        (Resolve-Path -LiteralPath $Path).Path,
        [IO.Path]::GetFullPath($Output),
        $Width,
        $Height
    )
}
